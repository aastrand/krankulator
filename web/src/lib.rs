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
use krankulator_core::emu::io::{controller, IOHandler, PollResult};
use krankulator_core::emu::memory::MemoryMapper;

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

struct WebAudioBackend {
    port: Rc<RefCell<Option<web_sys::MessagePort>>>,
    buffer: Vec<f32>,
}

impl WebAudioBackend {
    fn new(port: Rc<RefCell<Option<web_sys::MessagePort>>>) -> Self {
        Self {
            port,
            buffer: Vec::with_capacity(1024),
        }
    }

    fn send_to_worklet(&mut self) {
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
    ctx: Option<CanvasRenderingContext2d>,
    keys: Rc<RefCell<HashSet<String>>>,
    rgba_buf: Vec<u8>,
}

impl WebIOHandler {
    fn new(keys: Rc<RefCell<HashSet<String>>>) -> Self {
        Self {
            ctx: None,
            keys,
            rgba_buf: vec![0u8; 256 * 240 * 4],
        }
    }
}

impl IOHandler for WebIOHandler {
    fn init(&mut self) -> Result<(), String> {
        let canvas = document()
            .get_element_by_id("nes-canvas")
            .ok_or("no canvas")?
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|_| "not a canvas")?;

        let ctx = canvas
            .get_context("2d")
            .map_err(|_| "get_context failed")?
            .ok_or("no 2d context")?
            .dyn_into::<CanvasRenderingContext2d>()
            .map_err(|_| "not CanvasRenderingContext2d")?;

        self.ctx = Some(ctx);
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
        let Some(ctx) = &self.ctx else { return };

        let rgb = &buf.data;
        for i in 0..(256 * 240) {
            self.rgba_buf[i * 4] = rgb[i * 3];
            self.rgba_buf[i * 4 + 1] = rgb[i * 3 + 1];
            self.rgba_buf[i * 4 + 2] = rgb[i * 3 + 2];
            self.rgba_buf[i * 4 + 3] = 255;
        }

        let clamped = wasm_bindgen::Clamped(&self.rgba_buf[..]);
        if let Ok(img_data) = ImageData::new_with_u8_clamped_array_and_sh(clamped, 256, 240) {
            let _ = ctx.put_image_data(&img_data, 0.0, 0.0);
        }
    }

    fn exit(&self, s: String) {
        web_sys::console::log_1(&s.into());
    }
}

// --- Main entry ---

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    set_status("Load a .nes ROM to start");
    setup_file_input();
    setup_lucky_button();
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

thread_local! {
    static GENERATION: Cell<u32> = Cell::new(0);
}

fn start_emulator(rom_data: Vec<u8>) {
    use krankulator_core::emu::io::loader;

    GENERATION.with(|g| g.set(g.get().wrapping_add(1)));

    let mapper = match loader::load_nes_from_bytes(&rom_data) {
        Ok(m) => m,
        Err(e) => {
            set_status(&format!("Failed to load ROM: {}", e));
            return;
        }
    };

    set_status("Starting...");

    let keys: Rc<RefCell<HashSet<String>>> = Rc::new(RefCell::new(HashSet::new()));
    setup_keyboard(keys.clone());

    let audio_port: Rc<RefCell<Option<web_sys::MessagePort>>> = Rc::new(RefCell::new(None));
    let io_handler = Box::new(WebIOHandler::new(keys));
    let audio = Box::new(WebAudioBackend::new(audio_port.clone()));

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

    set_status("Running");

    let gen = GENERATION.with(|g| g.get());
    wasm_bindgen_futures::spawn_local(async move {
        connect_audio_worklet(audio_ctx, audio_port).await;
        run_loop(emu, gen);
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
    let _ = node.connect_with_audio_node(&ctx.destination());

    if let Ok(port) = node.port() {
        *audio_port.borrow_mut() = Some(port);
        web_sys::console::log_1(&"Audio worklet connected".into());
    }

    if let Ok(promise) = ctx.resume() {
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }
}

const FRAME_DURATION_MS: f64 = 1000.0 / 60.0988;

fn run_loop(mut emu: emu::Emulator, gen: u32) {
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    let perf = window().performance().unwrap();
    let mut last_frame_time = perf.now();
    let mut time_accumulator = 0.0;

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let current_gen = GENERATION.with(|g| g.get());
        if current_gen != gen {
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

        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    window()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .unwrap();
}

fn setup_keyboard(keys: Rc<RefCell<HashSet<String>>>) {
    let keys_down = keys.clone();
    let keydown = Closure::wrap(Box::new(move |e: KeyboardEvent| {
        e.prevent_default();
        keys_down.borrow_mut().insert(e.code());
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
