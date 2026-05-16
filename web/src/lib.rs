use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    AudioContext, AudioContextOptions, AudioWorkletNode, CanvasRenderingContext2d,
    HtmlCanvasElement, ImageData, KeyboardEvent,
};

use krankulator_core::emu;
use krankulator_core::emu::audio::AudioBackend;
use krankulator_core::emu::gfx;
use krankulator_core::emu::io::{controller, loader, IOHandler, PollResult};
use krankulator_core::emu::memory::MemoryMapper;

// --- localStorage persistence ---

fn local_storage() -> Option<web_sys::Storage> {
    window().local_storage().ok()?
}

fn rom_key(rom_hash: &str, suffix: &str) -> String {
    format!("krankulator:{}:{}", rom_hash, suffix)
}

fn hash_rom(data: &[u8]) -> String {
    let mut h: u32 = 0x811c9dc5;
    for &b in data {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    format!("{:08x}", h)
}

fn storage_get(key: &str) -> Option<String> {
    local_storage()?.get_item(key).ok()?
}

fn storage_set(key: &str, value: &str) {
    if let Some(storage) = local_storage() {
        let _ = storage.set_item(key, value);
    }
}

fn save_sram_to_storage(rom_hash: &str, sram: &[u8]) {
    let encoded = base64_encode(sram);
    storage_set(&rom_key(rom_hash, "sram"), &encoded);
}

fn load_sram_from_storage(rom_hash: &str) -> Option<Vec<u8>> {
    let encoded = storage_get(&rom_key(rom_hash, "sram"))?;
    base64_decode(&encoded)
}

fn save_state_to_storage(rom_hash: &str, slot: u8, data: &[u8]) {
    let encoded = base64_encode(data);
    storage_set(&rom_key(rom_hash, &format!("ss{}", slot)), &encoded);
}

fn load_state_from_storage(rom_hash: &str, slot: u8) -> Option<Vec<u8>> {
    let encoded = storage_get(&rom_key(rom_hash, &format!("ss{}", slot)))?;
    base64_decode(&encoded)
}

const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_CHARS[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for c in s.bytes() {
        let val = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => continue,
        };
        buf = (buf << 6) | val as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

fn window() -> web_sys::Window {
    web_sys::window().unwrap()
}

fn document() -> web_sys::Document {
    window().document().unwrap()
}

fn set_status(msg: &str) {
    if let Some(el) = document().get_element_by_id("status") {
        el.set_text_content(Some(msg));
    }
}

// --- Web Audio Backend ---

const AUDIO_TARGET_LEVEL: u32 = 2048;
const AUDIO_HIGH_WATER: u32 = 4096;

struct WebAudioBackend {
    port: Rc<RefCell<Option<web_sys::MessagePort>>>,
    buffer: Vec<f32>,
    worklet_level: Rc<Cell<u32>>,
}

impl WebAudioBackend {
    fn new(
        port: Rc<RefCell<Option<web_sys::MessagePort>>>,
        worklet_level: Rc<Cell<u32>>,
    ) -> Self {
        Self {
            port,
            buffer: Vec::with_capacity(1024),
            worklet_level,
        }
    }

    fn send_to_worklet(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        let level = self.worklet_level.get();
        if level > AUDIO_HIGH_WATER {
            let excess = (level - AUDIO_TARGET_LEVEL) as usize;
            let drop = excess.min(self.buffer.len());
            self.buffer.drain(..drop);
        }
        if self.buffer.is_empty() {
            return;
        }
        if let Some(port) = self.port.borrow().as_ref() {
            let arr = js_sys::Float32Array::new_with_length(self.buffer.len() as u32);
            arr.copy_from(&self.buffer);
            let _ = port.post_message(&arr);
        }
        self.buffer.clear();
    }
}

impl AudioBackend for WebAudioBackend {
    fn push_samples(&mut self, samples: &[f32]) {
        self.buffer.extend_from_slice(samples);
    }

    fn flush(&mut self) {
        self.send_to_worklet();
    }

    fn clear(&mut self) {
        self.buffer.clear();
    }
}

// --- Web IO Handler ---

struct WebIOHandler {
    contexts: Vec<CanvasRenderingContext2d>,
    keys: Rc<RefCell<HashSet<String>>>,
    rgba_buf: Vec<u8>,
}

impl WebIOHandler {
    fn new(keys: Rc<RefCell<HashSet<String>>>) -> Self {
        Self {
            contexts: Vec::new(),
            keys,
            rgba_buf: vec![0u8; 256 * 240 * 4],
        }
    }
}

fn get_canvas_ctx(id: &str) -> Option<CanvasRenderingContext2d> {
    let canvas = document()
        .get_element_by_id(id)?
        .dyn_into::<HtmlCanvasElement>()
        .ok()?;
    canvas
        .get_context("2d")
        .ok()??
        .dyn_into::<CanvasRenderingContext2d>()
        .ok()
}

impl IOHandler for WebIOHandler {
    fn init(&mut self) -> Result<(), String> {
        for id in &["nes-canvas", "nes-canvas-touch"] {
            if let Some(ctx) = get_canvas_ctx(id) {
                self.contexts.push(ctx);
            }
        }
        if self.contexts.is_empty() {
            return Err("no canvas found".to_string());
        }
        Ok(())
    }

    fn log(&self, logline: String) {
        web_sys::console::log_1(&logline.into());
    }

    fn poll(&mut self, mem: &mut dyn MemoryMapper, _apu: &mut emu::apu::APU) -> PollResult {
        let keys = self.keys.borrow();
        let ctrl = &mut mem.controllers()[0];

        let mapping: &[(&str, u8)] = &[
            ("ArrowUp", controller::UP),
            ("ArrowDown", controller::DOWN),
            ("ArrowLeft", controller::LEFT),
            ("ArrowRight", controller::RIGHT),
            ("KeyZ", controller::A),
            ("KeyX", controller::B),
            ("KeyC", controller::START),
            ("KeyV", controller::SELECT),
        ];

        for &(code, button) in mapping {
            if keys.contains(code) {
                ctrl.set_pressed(button);
            } else {
                ctrl.set_not_pressed(button);
            }
        }

        PollResult::default()
    }

    fn render(&mut self, buf: &gfx::buf::Buffer) {
        if self.contexts.is_empty() {
            return;
        }

        let rgb = &buf.data;
        for i in 0..(256 * 240) {
            self.rgba_buf[i * 4] = rgb[i * 3];
            self.rgba_buf[i * 4 + 1] = rgb[i * 3 + 1];
            self.rgba_buf[i * 4 + 2] = rgb[i * 3 + 2];
            self.rgba_buf[i * 4 + 3] = 255;
        }

        let clamped = wasm_bindgen::Clamped(&self.rgba_buf[..]);
        if let Ok(img_data) = ImageData::new_with_u8_clamped_array_and_sh(clamped, 256, 240) {
            for ctx in &self.contexts {
                let _ = ctx.put_image_data(&img_data, 0.0, 0.0);
            }
        }
    }

    fn exit(&self, s: String) {
        web_sys::console::log_1(&s.into());
    }
}

// --- Main entry ---

thread_local! {
    static KEYS: Rc<RefCell<HashSet<String>>> = Rc::new(RefCell::new(HashSet::new()));
    static GENERATION: Cell<u32> = Cell::new(0);
    static AUDIO_CTX: RefCell<Option<AudioContext>> = RefCell::new(None);
}

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    set_status("Load a .nes ROM to start");
    KEYS.with(|keys| {
        setup_keyboard(keys.clone());
        setup_touch_controls(keys.clone());
    });
    setup_file_input();
    setup_lucky_button();
    setup_touch_load_button();
    setup_touch_lucky_button();
    setup_audio_resume_on_interaction();
}

fn setup_file_input() {
    let input = document()
        .get_element_by_id("rom-input")
        .unwrap()
        .dyn_into::<web_sys::HtmlInputElement>()
        .unwrap();

    let closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
        let input = event
            .target()
            .unwrap()
            .dyn_into::<web_sys::HtmlInputElement>()
            .unwrap();
        let files = input.files().unwrap();
        if files.length() == 0 {
            return;
        }
        let file = files.get(0).unwrap();
        let reader = web_sys::FileReader::new().unwrap();
        let reader_clone = reader.clone();

        let onload = Closure::wrap(Box::new(move |_: web_sys::Event| {
            let array_buffer = reader_clone.result().unwrap();
            let uint8_array = js_sys::Uint8Array::new(&array_buffer);
            let mut rom_data = vec![0u8; uint8_array.length() as usize];
            uint8_array.copy_to(&mut rom_data);
            start_emulator(rom_data);
        }) as Box<dyn FnMut(_)>);

        reader.set_onload(Some(onload.as_ref().unchecked_ref()));
        onload.forget();
        let _ = reader.read_as_array_buffer(&file);
    }) as Box<dyn FnMut(_)>);

    input.add_event_listener_with_callback("change", closure.as_ref().unchecked_ref()).unwrap();
    closure.forget();
}

fn setup_lucky_button() {
    let btn = document()
        .get_element_by_id("lucky-btn")
        .unwrap();

    let closure = Closure::wrap(Box::new(move |_: web_sys::Event| {
        set_status("Fetching ROM...");
        wasm_bindgen_futures::spawn_local(async {
            match fetch_rom("https://file.classicjoy.games/games/meta-man/mega-man-2.nes").await {
                Ok(data) => start_emulator(data),
                Err(e) => set_status(&format!("Failed to fetch: {}", e)),
            }
        });
    }) as Box<dyn FnMut(_)>);

    btn.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref()).unwrap();
    closure.forget();
}

fn setup_touch_load_button() {
    let Some(btn) = document().get_element_by_id("touch-load") else { return };

    let closure = Closure::wrap(Box::new(move |_: web_sys::Event| {
        if let Some(input) = document().get_element_by_id("rom-input") {
            if let Ok(input) = input.dyn_into::<web_sys::HtmlInputElement>() {
                input.click();
            }
        }
    }) as Box<dyn FnMut(_)>);
    btn.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref()).unwrap();
    closure.forget();
}

fn setup_touch_lucky_button() {
    let Some(btn) = document().get_element_by_id("touch-lucky") else { return };

    let closure = Closure::wrap(Box::new(move |_: web_sys::Event| {
        wasm_bindgen_futures::spawn_local(async {
            match fetch_rom("https://file.classicjoy.games/games/meta-man/mega-man-2.nes").await {
                Ok(data) => start_emulator(data),
                Err(e) => set_status(&format!("Failed to fetch: {}", e)),
            }
        });
    }) as Box<dyn FnMut(_)>);
    btn.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref()).unwrap();
    closure.forget();
}

async fn fetch_rom(url: &str) -> Result<Vec<u8>, String> {
    let resp_value = wasm_bindgen_futures::JsFuture::from(window().fetch_with_str(url))
        .await
        .map_err(|e| format!("{:?}", e))?;

    let resp: web_sys::Response = resp_value.dyn_into().map_err(|_| "not a Response")?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let buf = wasm_bindgen_futures::JsFuture::from(
        resp.array_buffer().map_err(|_| "no array_buffer")?,
    )
    .await
    .map_err(|e| format!("{:?}", e))?;

    let uint8 = js_sys::Uint8Array::new(&buf);
    let mut data = vec![0u8; uint8.length() as usize];
    uint8.copy_to(&mut data);
    Ok(data)
}

fn start_emulator(rom_data: Vec<u8>) {
    GENERATION.with(|g| g.set(g.get().wrapping_add(1)));

    let rom_hash = hash_rom(&rom_data);
    let has_battery = loader::rom_has_battery(&rom_data);
    let sram = if has_battery {
        load_sram_from_storage(&rom_hash)
    } else {
        None
    };

    let mapper = match loader::load_nes_from_bytes_with_sram(&rom_data, sram) {
        Ok(m) => m,
        Err(e) => {
            set_status(&format!("Failed to load ROM: {}", e));
            return;
        }
    };

    set_status("Starting...");
    if let Some(el) = document().get_element_by_id("lucky-nudge") {
        let _ = el.dyn_into::<web_sys::HtmlElement>().map(|e| e.style().set_property("display", "none"));
    }

    let keys = KEYS.with(|k| k.clone());
    let audio_port: Rc<RefCell<Option<web_sys::MessagePort>>> = Rc::new(RefCell::new(None));
    let worklet_level: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let io_handler = Box::new(WebIOHandler::new(keys.clone()));
    let audio = Box::new(WebAudioBackend::new(audio_port.clone(), worklet_level.clone()));

    let mut emu = emu::Emulator::new_with(io_handler, mapper, audio);
    emu.cpu.status = 0x34;
    emu.cpu.sp = 0xfd;
    emu.toggle_should_trigger_nmi(true);
    emu.toggle_should_exit_on_infinite_loop(false);
    emu.toggle_quiet_mode(true);

    if let Err(msg) = emu.init() {
        set_status(&format!("Init failed: {}", msg));
        return;
    }

    let audio_ctx = create_audio_context();
    if let Some(ref ctx) = audio_ctx {
        let _ = ctx.resume();
    }
    AUDIO_CTX.with(|ac| *ac.borrow_mut() = audio_ctx.clone());

    set_status("Running");

    let gen = GENERATION.with(|g| g.get());
    let rom_hash_clone = rom_hash.clone();
    setup_beforeunload_sram(rom_hash, has_battery, gen);
    wasm_bindgen_futures::spawn_local(async move {
        connect_audio_worklet(audio_ctx, audio_port, worklet_level).await;
        run_loop(emu, gen, keys, rom_hash_clone, has_battery);
    });
}

fn create_audio_context() -> Option<AudioContext> {
    let opts = AudioContextOptions::new();
    opts.set_sample_rate(44100.0);
    match AudioContext::new_with_context_options(&opts) {
        Ok(c) => Some(c),
        Err(e) => {
            web_sys::console::error_1(&format!("AudioContext creation failed: {:?}", e).into());
            None
        }
    }
}

async fn connect_audio_worklet(
    ctx: Option<AudioContext>,
    audio_port: Rc<RefCell<Option<web_sys::MessagePort>>>,
    worklet_level: Rc<Cell<u32>>,
) {
    let Some(ctx) = ctx else { return };

    let worklet = match ctx.audio_worklet() {
        Ok(w) => w,
        Err(e) => {
            web_sys::console::error_1(&format!("audio_worklet() failed: {:?}", e).into());
            return;
        }
    };

    let promise = match worklet.add_module("audio_processor.js") {
        Ok(p) => p,
        Err(e) => {
            web_sys::console::error_1(&format!("addModule failed: {:?}", e).into());
            return;
        }
    };
    if let Err(e) = wasm_bindgen_futures::JsFuture::from(promise).await {
        web_sys::console::error_1(&format!("addModule promise rejected: {:?}", e).into());
        return;
    }

    let node = match AudioWorkletNode::new(&ctx, "nes-audio-processor") {
        Ok(n) => n,
        Err(e) => {
            web_sys::console::error_1(&format!("AudioWorkletNode creation failed: {:?}", e).into());
            return;
        }
    };
    if let Ok(stream_dest) = ctx.create_media_stream_destination() {
        let _ = node.connect_with_audio_node(&stream_dest);
        if let Ok(audio_el) = web_sys::HtmlAudioElement::new() {
            audio_el.set_src_object(Some(&stream_dest.stream()));
            let _ = audio_el.play();
            std::mem::forget(audio_el);
            std::mem::forget(stream_dest);
        }
    } else {
        let _ = node.connect_with_audio_node(&ctx.destination());
    }

    if let Ok(port) = node.port() {
        let level = worklet_level;
        let onmessage = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
            if let Some(val) = e.data().as_f64() {
                level.set(val as u32);
            }
        }) as Box<dyn FnMut(_)>);
        port.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();
        *audio_port.borrow_mut() = Some(port);
        web_sys::console::log_1(&"Audio worklet connected".into());
    }

    if let Ok(promise) = ctx.resume() {
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }
}

thread_local! {
    static BEFOREUNLOAD_CLOSURE: RefCell<Option<Closure<dyn FnMut(web_sys::Event)>>> = RefCell::new(None);
    static SRAM_SNAPSHOT: RefCell<Option<Vec<u8>>> = RefCell::new(None);
}

fn setup_beforeunload_sram(rom_hash: String, has_battery: bool, gen: u32) {
    BEFOREUNLOAD_CLOSURE.with(|prev| {
        if let Some(old) = prev.borrow_mut().take() {
            let _ = window().remove_event_listener_with_callback(
                "beforeunload",
                old.as_ref().unchecked_ref(),
            );
        }
    });

    if !has_battery {
        return;
    }

    let closure = Closure::wrap(Box::new(move |_: web_sys::Event| {
        let current_gen = GENERATION.with(|g| g.get());
        if current_gen != gen {
            return;
        }
        SRAM_SNAPSHOT.with(|snap| {
            if let Some(data) = snap.borrow().as_ref() {
                save_sram_to_storage(&rom_hash, data);
            }
        });
    }) as Box<dyn FnMut(_)>);

    let _ = window().add_event_listener_with_callback("beforeunload", closure.as_ref().unchecked_ref());
    BEFOREUNLOAD_CLOSURE.with(|prev| *prev.borrow_mut() = Some(closure));
}

const FRAME_DURATION_MS: f64 = 1000.0 / 60.0988;

fn run_loop(
    mut emu: emu::Emulator,
    gen: u32,
    keys: Rc<RefCell<HashSet<String>>>,
    rom_hash: String,
    has_battery: bool,
) {
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    let perf = window().performance().unwrap();
    let mut last_frame_time = perf.now();
    let mut time_accumulator = 0.0;
    let mut savestate_slot: u8 = 0;
    let mut prev_save = false;
    let mut prev_load = false;
    let mut prev_cycle = false;
    let mut sram_save_counter: u32 = 0;

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let current_gen = GENERATION.with(|g| g.get());
        if current_gen != gen {
            if has_battery {
                if let Some(sram) = emu.mem.sram_data() {
                    save_sram_to_storage(&rom_hash, sram);
                    SRAM_SNAPSHOT.with(|s| *s.borrow_mut() = Some(sram.to_vec()));
                }
            }
            return;
        }

        let now = perf.now();
        time_accumulator += now - last_frame_time;
        last_frame_time = now;

        let mut frames_run = 0;
        while time_accumulator >= FRAME_DURATION_MS && frames_run < 2 {
            if !emu.run_one_frame() {
                set_status("Emulator stopped");
                return;
            }
            time_accumulator -= FRAME_DURATION_MS;
            frames_run += 1;
        }

        if time_accumulator > FRAME_DURATION_MS * 3.0 {
            time_accumulator = 0.0;
        }

        let k = keys.borrow();
        let save_held = k.contains("KeyS");
        let load_held = k.contains("KeyA");
        let cycle_held = k.contains("KeyQ");
        drop(k);

        if save_held && !prev_save {
            let data = emu.save_state_to_bytes();
            save_state_to_storage(&rom_hash, savestate_slot, &data);
            set_status(&format!("State saved (slot {})", savestate_slot));
        }
        if load_held && !prev_load {
            if let Some(data) = load_state_from_storage(&rom_hash, savestate_slot) {
                match emu.load_state_from_bytes(&data) {
                    Ok(()) => set_status(&format!("State loaded (slot {})", savestate_slot)),
                    Err(e) => set_status(&format!("Load failed: {}", e)),
                }
            } else {
                set_status(&format!("No save in slot {}", savestate_slot));
            }
        }
        if cycle_held && !prev_cycle {
            savestate_slot = (savestate_slot + 1) % 4;
            set_status(&format!("Slot {}", savestate_slot));
        }
        prev_save = save_held;
        prev_load = load_held;
        prev_cycle = cycle_held;

        if has_battery {
            sram_save_counter += 1;
            if sram_save_counter >= 300 {
                sram_save_counter = 0;
                if let Some(sram) = emu.mem.sram_data() {
                    save_sram_to_storage(&rom_hash, sram);
                    SRAM_SNAPSHOT.with(|s| *s.borrow_mut() = Some(sram.to_vec()));
                }
            }
        }

        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    window()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .unwrap();
}

const MAPPED_KEYS: &[&str] = &[
    "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight",
    "KeyZ", "KeyX", "KeyC", "KeyV",
    "KeyS", "KeyA", "KeyQ",
];

fn setup_audio_resume_on_interaction() {
    let closure = Closure::wrap(Box::new(move |_: web_sys::Event| {
        AUDIO_CTX.with(|ac| {
            if let Some(ctx) = ac.borrow().as_ref() {
                if ctx.state() == web_sys::AudioContextState::Suspended {
                    let _ = ctx.resume();
                }
            }
        });
    }) as Box<dyn FnMut(_)>);

    let doc = document();
    let _ = doc.add_event_listener_with_callback("touchstart", closure.as_ref().unchecked_ref());
    let _ = doc.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref());
    closure.forget();
}

// --- Touch Controls ---

fn setup_touch_controls(keys: Rc<RefCell<HashSet<String>>>) {
    let get_el = |id: &str| -> Option<web_sys::HtmlElement> {
        document()
            .get_element_by_id(id)?
            .dyn_into::<web_sys::HtmlElement>()
            .ok()
    };

    if let (Some(zone), Some(stick)) = (get_el("dpad-zone"), get_el("dpad-stick")) {
        setup_dpad(zone, stick, keys.clone());
    }

    let buttons: &[(&str, &str)] = &[
        ("touch-a", "KeyZ"),
        ("touch-b", "KeyX"),
        ("touch-start", "KeyC"),
        ("touch-select", "KeyV"),
    ];
    for &(id, key_code) in buttons {
        if let Some(el) = get_el(id) {
            setup_action_button(el, key_code, keys.clone());
        }
    }
}

fn setup_dpad(
    zone: web_sys::HtmlElement,
    stick: web_sys::HtmlElement,
    keys: Rc<RefCell<HashSet<String>>>,
) {
    let active_touch: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));

    let update_directions =
        |zone: &web_sys::HtmlElement,
         stick: &web_sys::HtmlElement,
         keys: &Rc<RefCell<HashSet<String>>>,
         touch: &web_sys::Touch| {
            let rect = zone.get_bounding_client_rect();
            let radius = rect.width() / 2.0;
            let cx = rect.left() + radius;
            let cy = rect.top() + rect.height() / 2.0;
            let dx = touch.client_x() as f64 - cx;
            let dy = touch.client_y() as f64 - cy;

            let dist = (dx * dx + dy * dy).sqrt();
            let dead_zone = radius * 0.15;

            let (up, down, left, right) = if dist < dead_zone {
                (false, false, false, false)
            } else {
                let angle = dy.atan2(dx);
                let threshold = std::f64::consts::PI / 2.8;
                (
                    (angle + std::f64::consts::FRAC_PI_2).abs() < threshold,
                    (angle - std::f64::consts::FRAC_PI_2).abs() < threshold,
                    (angle.abs() - std::f64::consts::PI).abs() < threshold,
                    angle.abs() < threshold,
                )
            };

            let clamp_dist = dist.min(radius);
            let (sx, sy) = if dist > 0.0 {
                (dx / dist * clamp_dist, dy / dist * clamp_dist)
            } else {
                (0.0, 0.0)
            };
            let _ = stick
                .style()
                .set_property("transform", &format!("translate(calc(-50% + {sx:.0}px), calc(-50% + {sy:.0}px))"));

            let mut k = keys.borrow_mut();
            let dirs: &[(&str, bool)] = &[
                ("ArrowUp", up),
                ("ArrowDown", down),
                ("ArrowLeft", left),
                ("ArrowRight", right),
            ];
            for &(code, active) in dirs {
                if active {
                    k.insert(code.to_string());
                } else {
                    k.remove(code);
                }
            }
        };

    let clear_directions = |stick: &web_sys::HtmlElement, keys: &Rc<RefCell<HashSet<String>>>| {
        let _ = stick
            .style()
            .set_property("transform", "translate(-50%, -50%)");
        let mut k = keys.borrow_mut();
        for code in &["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"] {
            k.remove(*code);
        }
    };

    {
        let zone2 = zone.clone();
        let stick2 = stick.clone();
        let keys2 = keys.clone();
        let at = active_touch.clone();
        let start = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            if at.get().is_some() {
                return;
            }
            let touches = e.changed_touches();
            if let Some(touch) = touches.get(0) {
                at.set(Some(touch.identifier()));
                update_directions(&zone2, &stick2, &keys2, &touch);
            }
        }) as Box<dyn FnMut(_)>);
        zone.add_event_listener_with_callback("touchstart", start.as_ref().unchecked_ref())
            .unwrap();
        start.forget();
    }

    {
        let zone2 = zone.clone();
        let stick2 = stick.clone();
        let keys2 = keys.clone();
        let at = active_touch.clone();
        let mov = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            let Some(tid) = at.get() else { return };
            let touches = e.changed_touches();
            for i in 0..touches.length() {
                if let Some(touch) = touches.get(i) {
                    if touch.identifier() == tid {
                        update_directions(&zone2, &stick2, &keys2, &touch);
                        return;
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);
        zone.add_event_listener_with_callback("touchmove", mov.as_ref().unchecked_ref())
            .unwrap();
        mov.forget();
    }

    {
        let stick2 = stick;
        let keys2 = keys;
        let at = active_touch;
        let end = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            let Some(tid) = at.get() else { return };
            let touches = e.changed_touches();
            for i in 0..touches.length() {
                if let Some(touch) = touches.get(i) {
                    if touch.identifier() == tid {
                        at.set(None);
                        clear_directions(&stick2, &keys2);
                        return;
                    }
                }
            }
        }) as Box<dyn FnMut(_)>);
        zone.add_event_listener_with_callback("touchend", end.as_ref().unchecked_ref())
            .unwrap();
        zone.add_event_listener_with_callback("touchcancel", end.as_ref().unchecked_ref())
            .unwrap();
        end.forget();
    }
}

fn setup_action_button(
    el: web_sys::HtmlElement,
    key_code: &'static str,
    keys: Rc<RefCell<HashSet<String>>>,
) {
    {
        let keys2 = keys.clone();
        let el2 = el.clone();
        let start = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            keys2.borrow_mut().insert(key_code.to_string());
            let _ = el2.class_list().add_1("pressed");
        }) as Box<dyn FnMut(_)>);
        el.add_event_listener_with_callback("touchstart", start.as_ref().unchecked_ref())
            .unwrap();
        start.forget();
    }

    {
        let keys2 = keys;
        let el2 = el.clone();
        let end = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            keys2.borrow_mut().remove(key_code);
            let _ = el2.class_list().remove_1("pressed");
        }) as Box<dyn FnMut(_)>);
        el.add_event_listener_with_callback("touchend", end.as_ref().unchecked_ref())
            .unwrap();
        el.add_event_listener_with_callback("touchcancel", end.as_ref().unchecked_ref())
            .unwrap();
        end.forget();
    }
}

fn setup_keyboard(keys: Rc<RefCell<HashSet<String>>>) {
    let keys_down = keys.clone();
    let keydown = Closure::wrap(Box::new(move |e: KeyboardEvent| {
        let code = e.code();
        if MAPPED_KEYS.contains(&code.as_str()) {
            e.prevent_default();
        }
        keys_down.borrow_mut().insert(code);
    }) as Box<dyn FnMut(_)>);

    let keys_up = keys;
    let keyup = Closure::wrap(Box::new(move |e: KeyboardEvent| {
        keys_up.borrow_mut().remove(&e.code());
    }) as Box<dyn FnMut(_)>);

    let doc = document();
    doc.add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref()).unwrap();
    doc.add_event_listener_with_callback("keyup", keyup.as_ref().unchecked_ref()).unwrap();
    keydown.forget();
    keyup.forget();
}
