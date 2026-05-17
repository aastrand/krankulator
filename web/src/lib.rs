mod audio;
mod input;
mod io;
mod persistence;

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::AudioContext;

use krankulator_core::emu;
use krankulator_core::emu::io::loader;

use audio::WebAudioBackend;
use io::WebIOHandler;
use persistence::{
    hash_rom, load_sram_from_storage, load_state_from_storage, save_sram_to_storage,
    save_state_to_storage, setup_beforeunload_sram, SRAM_SNAPSHOT,
};

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
        input::setup_keyboard(keys.clone());
        input::setup_touch_controls(keys.clone());
        input::setup_canvas_double_tap(keys.clone());
    });
    setup_file_input();
    setup_lucky_button();
    setup_touch_load_button();
    setup_touch_lucky_button();
    audio::setup_audio_resume_on_interaction();
    audio::setup_visibility_pause();
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
    let audio_backend = Box::new(WebAudioBackend::new(audio_port.clone(), worklet_level.clone()));

    let mut emu = emu::Emulator::new_with(io_handler, mapper, audio_backend);
    emu.cpu.status = 0x34;
    emu.cpu.sp = 0xfd;
    emu.toggle_should_trigger_nmi(true);
    emu.toggle_should_exit_on_infinite_loop(false);
    emu.toggle_quiet_mode(true);

    if let Err(msg) = emu.init() {
        set_status(&format!("Init failed: {}", msg));
        return;
    }

    let audio_ctx = audio::create_audio_context();
    if let Some(ref ctx) = audio_ctx {
        let _ = ctx.resume();
    }
    AUDIO_CTX.with(|ac| *ac.borrow_mut() = audio_ctx.clone());

    set_status("Running");

    let gen = GENERATION.with(|g| g.get());
    let rom_hash_clone = rom_hash.clone();
    setup_beforeunload_sram(rom_hash, has_battery, gen);
    wasm_bindgen_futures::spawn_local(async move {
        audio::connect_audio_worklet(audio_ctx, audio_port, worklet_level).await;
        run_loop(emu, gen, keys, rom_hash_clone, has_battery);
    });
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
    let mut prev_tab = false;
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
            let emu_start = perf.now();
            if !emu.run_one_frame() {
                set_status("Emulator stopped");
                return;
            }
            let emu_ms = perf.now() - emu_start;
            emu.overlay.set_frame_time(emu_ms);
            time_accumulator -= FRAME_DURATION_MS;
            frames_run += 1;
        }

        if time_accumulator > FRAME_DURATION_MS * 3.0 {
            time_accumulator = 0.0;
        }

        let k = keys.borrow();
        let mut save_held = k.contains("KeyS");
        let mut load_held = k.contains("KeyA");
        let mut cycle_held = k.contains("KeyQ");
        let tab_held = k.contains("Tab");
        drop(k);

        if let Some(gp) = input::poll_gamepad() {
            save_held |= gp.save_state;
            load_held |= gp.load_state;
            cycle_held |= gp.cycle_slot;
        }

        if save_held && !prev_save {
            let data = emu.save_state_to_bytes();
            save_state_to_storage(&rom_hash, savestate_slot, &data);
            emu.overlay.toast("STATE SAVED".into());
        }
        if load_held && !prev_load {
            if let Some(data) = load_state_from_storage(&rom_hash, savestate_slot) {
                match emu.load_state_from_bytes(&data) {
                    Ok(()) => emu.overlay.toast("STATE LOADED".into()),
                    Err(e) => emu.overlay.toast(format!("LOAD FAILED: {}", e)),
                }
            } else {
                emu.overlay.toast(format!("NO SAVE IN SLOT {}", savestate_slot));
            }
        }
        if cycle_held && !prev_cycle {
            savestate_slot = (savestate_slot + 1) % 4;
            emu.overlay.toast(format!("SLOT {}", savestate_slot));
        }
        if tab_held && !prev_tab {
            emu.overlay.toggle();
        }
        prev_save = save_held;
        prev_load = load_held;
        prev_cycle = cycle_held;
        prev_tab = tab_held;

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
