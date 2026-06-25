mod audio;
pub(crate) mod crt_renderer;
mod input;
mod io;
mod loading_screen;
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
    static GENERATION: Cell<u32> = const { Cell::new(0) };
    static AUDIO_CTX: RefCell<Option<AudioContext>> = const { RefCell::new(None) };
}

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    set_status("Load a .nes ROM to start");
    loading_screen::preload_sprite();
    KEYS.with(|keys| {
        input::setup_keyboard(keys.clone());
        input::setup_touch_controls(keys.clone());
        input::setup_canvas_double_tap(keys.clone());
    });
    setup_file_input();
    setup_lucky_button();
    setup_touch_load_button();
    setup_touch_lucky_button();
    setup_fullscreen_toggle();
    setup_game_drawer();
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

        let file_name = file.name();
        let onload = Closure::wrap(Box::new(move |_: web_sys::Event| {
            let array_buffer = reader_clone.result().unwrap();
            let uint8_array = js_sys::Uint8Array::new(&array_buffer);
            let mut rom_data = vec![0u8; uint8_array.length() as usize];
            uint8_array.copy_to(&mut rom_data);
            start_emulator(rom_data, Some(file_name.clone()));
        }) as Box<dyn FnMut(_)>);

        reader.set_onload(Some(onload.as_ref().unchecked_ref()));
        onload.forget();
        let _ = reader.read_as_array_buffer(&file);
    }) as Box<dyn FnMut(_)>);

    input
        .add_event_listener_with_callback("change", closure.as_ref().unchecked_ref())
        .unwrap();
    closure.forget();
}

fn setup_lucky_button() {
    let btn = document().get_element_by_id("lucky-btn").unwrap();

    let closure = Closure::wrap(Box::new(move |_: web_sys::Event| {
        set_status("Picking a random game...");
        wasm_bindgen_futures::spawn_local(fetch_random_rom());
    }) as Box<dyn FnMut(_)>);

    btn.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
        .unwrap();
    closure.forget();
}

fn setup_fullscreen_toggle() {
    let canvas = document().get_element_by_id("nes-canvas").unwrap();

    let canvas_clone = canvas.clone();
    let closure = Closure::wrap(Box::new(move |_: web_sys::Event| {
        let doc = document();
        if doc.fullscreen_element().is_some() {
            doc.exit_fullscreen();
        } else {
            canvas_clone.request_fullscreen().ok();
        }
    }) as Box<dyn FnMut(_)>);

    canvas
        .add_event_listener_with_callback("dblclick", closure.as_ref().unchecked_ref())
        .unwrap();
    closure.forget();
}

fn setup_touch_load_button() {
    let Some(btn) = document().get_element_by_id("touch-load") else {
        return;
    };

    let closure = Closure::wrap(Box::new(move |_: web_sys::Event| {
        if let Some(input) = document().get_element_by_id("rom-input") {
            if let Ok(input) = input.dyn_into::<web_sys::HtmlInputElement>() {
                input.click();
            }
        }
    }) as Box<dyn FnMut(_)>);
    btn.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
        .unwrap();
    closure.forget();
}

fn setup_touch_lucky_button() {
    let Some(btn) = document().get_element_by_id("touch-lucky") else {
        return;
    };

    let closure = Closure::wrap(Box::new(move |_: web_sys::Event| {
        wasm_bindgen_futures::spawn_local(fetch_random_rom());
    }) as Box<dyn FnMut(_)>);
    btn.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
        .unwrap();
    closure.forget();
}

fn setup_game_drawer() {
    let Some(drawer) = document().get_element_by_id("game-drawer") else {
        return;
    };
    let edge_el = document().get_element_by_id("game-drawer-edge");

    let drawer_el: web_sys::HtmlElement = drawer.dyn_into().unwrap();

    // Desktop: show drawer when ?list is in the URL
    let href = document().url().unwrap_or_default();
    if href.contains("?list") || href.contains("&list") {
        let _ = drawer_el.class_list().add_1("desktop-visible");
        let _ = drawer_el.class_list().add_1("open");
    }

    let is_open = Rc::new(Cell::new(false));
    let touch_start_x: Rc<Cell<Option<f64>>> = Rc::new(Cell::new(None));
    let touch_start_y: Rc<Cell<Option<f64>>> = Rc::new(Cell::new(None));
    let drag_committed: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    let drag_offset: Rc<Cell<Option<f64>>> = Rc::new(Cell::new(None));
    // Populate the drawer from list.json (only games with ROMs)
    {
        let drawer_el = drawer_el.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let list_url =
                web_sys::Url::new_with_base("list.json", &document().base_uri().unwrap().unwrap())
                    .unwrap()
                    .href();
            let Ok(list) = fetch_json(&list_url).await else {
                return;
            };
            let arr: js_sys::Array = list.into();
            let container = document().get_element_by_id("game-drawer-list").unwrap();
            for i in 0..arr.length() {
                let entry = arr.get(i);
                let rom_url = match js_sys::Reflect::get(&entry, &"rom".into())
                    .ok()
                    .and_then(|v| v.as_string())
                {
                    Some(u) => u,
                    None => continue,
                };
                let name = js_sys::Reflect::get(&entry, &"name".into())
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_default();
                let box_art = js_sys::Reflect::get(&entry, &"box_art".into())
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_default();
                let cartridge = js_sys::Reflect::get(&entry, &"cartridge".into())
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_default();

                let card = document().create_element("div").unwrap();
                let _ = card.set_attribute("class", "game-card");

                let bg_img = document().create_element("img").unwrap();
                let _ = bg_img.set_attribute("class", "game-card-bg");
                let _ = bg_img.set_attribute("src", &box_art);
                let _ = bg_img.set_attribute("loading", "lazy");
                let _ = bg_img.set_attribute("alt", "");
                card.append_child(&bg_img).unwrap();

                let img = document().create_element("img").unwrap();
                let _ = img.set_attribute("class", "game-card-cart");
                let _ = img.set_attribute("src", &cartridge);
                let _ = img.set_attribute("loading", "lazy");
                let _ = img.set_attribute("alt", "");
                card.append_child(&img).unwrap();

                let label = document().create_element("div").unwrap();
                let _ = label.set_attribute("class", "game-card-name");
                label.set_text_content(Some(&name));
                card.append_child(&label).unwrap();

                let name = name.clone();
                let drawer_el = drawer_el.clone();
                let onclick = Closure::wrap(Box::new(move |_: web_sys::Event| {
                    let _ = drawer_el.class_list().remove_1("open");
                    let _ = drawer_el.style().remove_property("transform");
                    let name = name.clone();
                    let rom_url = rom_url.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        load_rom_with_loading_screen(&rom_url, &name).await;
                    });
                }) as Box<dyn FnMut(_)>);
                card.add_event_listener_with_callback("click", onclick.as_ref().unchecked_ref())
                    .unwrap();
                onclick.forget();

                container.append_child(&card).unwrap();
            }
        });
    }

    // Touchstart on edge zone (high z-index, above game controls)
    if let Some(edge) = edge_el {
        {
            let touch_start_x = touch_start_x.clone();
            let touch_start_y = touch_start_y.clone();
            let drag_committed = drag_committed.clone();
            let drag_offset = drag_offset.clone();
            let start = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
                e.prevent_default();
                e.stop_propagation();
                let Some(touch) = e.touches().get(0) else {
                    return;
                };
                touch_start_x.set(Some(touch.client_x() as f64));
                touch_start_y.set(Some(touch.client_y() as f64));
                drag_committed.set(false);
                drag_offset.set(Some(0.0));
            }) as Box<dyn FnMut(_)>);
            edge.add_event_listener_with_callback("touchstart", start.as_ref().unchecked_ref())
                .unwrap();
            start.forget();
        }

        // Drawer itself: close gesture
        {
            let touch_start_x = touch_start_x.clone();
            let touch_start_y = touch_start_y.clone();
            let drag_committed = drag_committed.clone();
            let drag_offset = drag_offset.clone();
            let is_open = is_open.clone();
            let start = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
                if !is_open.get() {
                    return;
                }
                let Some(touch) = e.touches().get(0) else {
                    return;
                };
                touch_start_x.set(Some(touch.client_x() as f64));
                touch_start_y.set(Some(touch.client_y() as f64));
                drag_committed.set(false);
                drag_offset.set(Some(0.0));
            }) as Box<dyn FnMut(_)>);
            drawer_el
                .add_event_listener_with_callback("touchstart", start.as_ref().unchecked_ref())
                .unwrap();
            start.forget();
        }
    }

    // Touchmove: track horizontal drag
    {
        let drawer_el = drawer_el.clone();
        let touch_start_x = touch_start_x.clone();
        let touch_start_y = touch_start_y.clone();
        let drag_offset = drag_offset.clone();
        let drag_committed = drag_committed.clone();
        let is_open = is_open.clone();
        let mov = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
            let Some(start_x) = touch_start_x.get() else {
                return;
            };
            let Some(touch) = e.touches().get(0) else {
                return;
            };
            let dx = touch.client_x() as f64 - start_x;
            let dy = touch.client_y() as f64 - touch_start_y.get().unwrap_or(0.0);

            if !drag_committed.get() {
                let adx = dx.abs();
                let ady = dy.abs();
                if adx < 10.0 && ady < 10.0 {
                    return;
                }
                if ady > adx {
                    touch_start_x.set(None);
                    return;
                }
                drag_committed.set(true);
            }

            let drawer_width = drawer_el.offset_width() as f64;

            if is_open.get() {
                let offset = dx.max(0.0).min(drawer_width);
                drag_offset.set(Some(offset));
                let _ = drawer_el
                    .style()
                    .set_property("transform", &format!("translateX({offset}px)"));
                let _ = drawer_el.style().set_property("transition", "none");
            } else {
                let offset = (drawer_width + dx).max(0.0).min(drawer_width);
                drag_offset.set(Some(offset));
                let _ = drawer_el
                    .style()
                    .set_property("transform", &format!("translateX({offset}px)"));
                let _ = drawer_el.style().set_property("transition", "none");
            }
        }) as Box<dyn FnMut(_)>);
        document()
            .add_event_listener_with_callback("touchmove", mov.as_ref().unchecked_ref())
            .unwrap();
        mov.forget();
    }

    // Touchend: snap open or closed
    {
        let drawer_el = drawer_el.clone();
        let touch_start_x = touch_start_x.clone();
        let drag_offset = drag_offset.clone();
        let is_open = is_open.clone();
        let end = Closure::wrap(Box::new(move |_: web_sys::TouchEvent| {
            if touch_start_x.get().is_none() {
                return;
            }
            touch_start_x.set(None);
            let _ = drawer_el.style().remove_property("transition");

            let drawer_width = drawer_el.offset_width() as f64;
            let offset = drag_offset.get().unwrap_or(drawer_width);
            drag_offset.set(None);

            let threshold = drawer_width * 0.3;
            if is_open.get() {
                if offset > threshold {
                    let _ = drawer_el.class_list().remove_1("open");
                    let _ = drawer_el.style().remove_property("transform");
                    is_open.set(false);
                } else {
                    let _ = drawer_el.class_list().add_1("open");
                    let _ = drawer_el.style().remove_property("transform");
                }
            } else {
                if offset < drawer_width - threshold {
                    let _ = drawer_el.class_list().add_1("open");
                    let _ = drawer_el.style().remove_property("transform");
                    is_open.set(true);
                } else {
                    let _ = drawer_el.class_list().remove_1("open");
                    let _ = drawer_el.style().remove_property("transform");
                }
            }
        }) as Box<dyn FnMut(_)>);
        document()
            .add_event_listener_with_callback("touchend", end.as_ref().unchecked_ref())
            .unwrap();
        document()
            .add_event_listener_with_callback("touchcancel", end.as_ref().unchecked_ref())
            .unwrap();
        end.forget();
    }
}

async fn fetch_random_rom() {
    let list_url =
        web_sys::Url::new_with_base("list.json", &document().base_uri().unwrap().unwrap())
            .unwrap()
            .href();
    let list = match fetch_json(&list_url).await {
        Ok(v) => v,
        Err(e) => {
            set_status(&format!("Failed to load game list: {e}"));
            return;
        }
    };
    let arr: js_sys::Array = list.into();
    let playable: Vec<JsValue> = (0..arr.length())
        .map(|i| arr.get(i))
        .filter(|e| {
            js_sys::Reflect::get(e, &"rom".into())
                .ok()
                .is_some_and(|v| v.is_string())
        })
        .collect();
    if playable.is_empty() {
        set_status("No playable games in list");
        return;
    }
    let idx = (js_sys::Math::random() * playable.len() as f64) as usize;
    let entry = &playable[idx];
    let name = js_sys::Reflect::get(entry, &"name".into())
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    let rom_url = match js_sys::Reflect::get(entry, &"rom".into())
        .ok()
        .and_then(|v| v.as_string())
    {
        Some(u) => u,
        None => {
            set_status(&format!("No ROM URL for {name}"));
            return;
        }
    };
    load_rom_with_loading_screen(&rom_url, &name).await;
}

async fn load_rom_with_loading_screen(url: &str, name: &str) {
    loading_screen::start_loading(name);

    let result = loading_screen::fetch_with_progress(url).await;

    loading_screen::stop_loading();

    match result {
        Ok(js_val) => {
            let uint8 = js_sys::Uint8Array::new(&js_val);
            let mut data = vec![0u8; uint8.length() as usize];
            uint8.copy_to(&mut data);

            if url.ends_with(".zip") || (data.len() > 4 && &data[0..4] == b"PK\x03\x04") {
                match extract_nes_from_zip(&data) {
                    Ok(rom) => start_emulator(rom, Some(format!("{name}.nes"))),
                    Err(e) => set_status(&format!("Failed: {e}")),
                }
            } else {
                start_emulator(data, Some(format!("{name}.nes")));
            }
        }
        Err(e) => {
            set_status(&format!("Failed to fetch {name}: {e:?}"));
        }
    }
}

async fn fetch_json(url: &str) -> Result<JsValue, String> {
    let resp_value = wasm_bindgen_futures::JsFuture::from(window().fetch_with_str(url))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: web_sys::Response = resp_value.dyn_into().map_err(|_| "not a Response")?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    wasm_bindgen_futures::JsFuture::from(resp.json().map_err(|_| "no json")?)
        .await
        .map_err(|e| format!("{e:?}"))
}

fn extract_nes_from_zip(zip: &[u8]) -> Result<Vec<u8>, String> {
    let mut offset = 0;
    while offset + 30 <= zip.len() {
        if &zip[offset..offset + 4] != b"PK\x03\x04" {
            break;
        }
        let compression = u16::from_le_bytes([zip[offset + 8], zip[offset + 9]]);
        let compressed_size = u32::from_le_bytes([
            zip[offset + 18],
            zip[offset + 19],
            zip[offset + 20],
            zip[offset + 21],
        ]) as usize;
        let uncompressed_size = u32::from_le_bytes([
            zip[offset + 22],
            zip[offset + 23],
            zip[offset + 24],
            zip[offset + 25],
        ]) as usize;
        let name_len = u16::from_le_bytes([zip[offset + 26], zip[offset + 27]]) as usize;
        let extra_len = u16::from_le_bytes([zip[offset + 28], zip[offset + 29]]) as usize;
        let name_start = offset + 30;
        let name_end = name_start + name_len;
        let data_start = name_end + extra_len;

        if name_end > zip.len() || data_start > zip.len() {
            return Err("Corrupt zip".into());
        }

        let name = String::from_utf8_lossy(&zip[name_start..name_end]);
        if name.to_lowercase().ends_with(".nes") {
            let data_end = data_start + compressed_size;
            if data_end > zip.len() {
                return Err("Corrupt zip".into());
            }
            return match compression {
                0 => Ok(zip[data_start..data_end].to_vec()),
                8 => inflate_raw(&zip[data_start..data_end], uncompressed_size),
                _ => Err(format!("Unsupported zip compression method {compression}")),
            };
        }

        offset = data_start + compressed_size;
    }
    Err("No .nes file found in zip".into())
}

fn inflate_raw(compressed: &[u8], _expected_size: usize) -> Result<Vec<u8>, String> {
    miniz_oxide::inflate::decompress_to_vec(compressed)
        .map_err(|e| format!("Inflate error: {:?}", e))
}

fn start_emulator(rom_data: Vec<u8>, filename: Option<String>) {
    GENERATION.with(|g| g.set(g.get().wrapping_add(1)));

    let region = loader::detect_region_with_filename(&rom_data, filename.as_deref());

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
            set_status(&format!("Failed to load ROM: {e}"));
            return;
        }
    };

    set_status("Starting...");
    if let Some(el) = document().get_element_by_id("lucky-nudge") {
        let _ = el
            .dyn_into::<web_sys::HtmlElement>()
            .map(|e| e.style().set_property("display", "none"));
    }

    let keys = KEYS.with(|k| k.clone());
    let audio_port: Rc<RefCell<Option<web_sys::MessagePort>>> = Rc::new(RefCell::new(None));
    let worklet_level: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let io_handler = Box::new(WebIOHandler::new(keys.clone()));
    let audio_backend = Box::new(WebAudioBackend::new(
        audio_port.clone(),
        worklet_level.clone(),
    ));

    let is_pal = region == emu::Region::Pal;
    let mut emu = emu::Emulator::new_with_region(io_handler, mapper, audio_backend, region);
    emu.cpu.status = 0x34;
    emu.cpu.sp = 0xfd;
    emu.toggle_should_trigger_nmi(true);
    emu.toggle_should_exit_on_infinite_loop(false);
    emu.toggle_quiet_mode(true);
    emu.set_overscan(!is_pal);

    if let Err(msg) = emu.init() {
        set_status(&format!("Init failed: {msg}"));
        return;
    }

    let audio_ctx = audio::create_audio_context();
    if let Some(ref ctx) = audio_ctx {
        let _ = ctx.resume();
    }
    AUDIO_CTX.with(|ac| *ac.borrow_mut() = audio_ctx.clone());

    if let Some(ref name) = filename {
        let label = name.trim_end_matches(".nes");
        set_status(&format!("Playing {label}"));
    } else {
        set_status("Running");
    }

    let gen = GENERATION.with(|g| g.get());
    let rom_hash_clone = rom_hash.clone();
    setup_beforeunload_sram(rom_hash, has_battery, gen);
    wasm_bindgen_futures::spawn_local(async move {
        audio::connect_audio_worklet(audio_ctx, audio_port, worklet_level).await;
        let frame_duration_ms = region.config().frame_duration_nanos as f64 / 1_000_000.0;
        run_loop(
            emu,
            gen,
            keys,
            rom_hash_clone,
            has_battery,
            frame_duration_ms,
        );
    });
}

fn run_loop(
    mut emu: emu::Emulator,
    gen: u32,
    keys: Rc<RefCell<HashSet<String>>>,
    rom_hash: String,
    has_battery: bool,
    frame_duration_ms: f64,
) {
    #[allow(clippy::type_complexity)]
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
    let mut prev_fullscreen = false;
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

        let fast_forward = keys.borrow().contains("Space");
        let max_frames = if fast_forward { 20 } else { 2 };

        let mut frames_run = 0;
        while time_accumulator >= frame_duration_ms && frames_run < max_frames {
            let emu_start = perf.now();
            if !emu.run_one_frame() {
                set_status("Emulator stopped");
                return;
            }
            let emu_ms = perf.now() - emu_start;
            emu.overlay.set_frame_time(emu_ms);
            time_accumulator -= frame_duration_ms;
            frames_run += 1;
        }

        if time_accumulator > frame_duration_ms * 3.0 {
            time_accumulator = 0.0;
        }

        let k = keys.borrow();
        let mut save_held = k.contains("KeyS");
        let mut load_held = k.contains("KeyA");
        let mut cycle_held = k.contains("KeyQ");
        let tab_held = k.contains("Tab");
        let fullscreen_held = k.contains("KeyF");
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
                    Err(e) => emu.overlay.toast(format!("LOAD FAILED: {e}")),
                }
            } else {
                emu.overlay
                    .toast(format!("NO SAVE IN SLOT {savestate_slot}"));
            }
        }
        if cycle_held && !prev_cycle {
            savestate_slot = (savestate_slot + 1) % 4;
            emu.overlay.toast(format!("SLOT {savestate_slot}"));
        }
        if tab_held && !prev_tab {
            emu.overlay.toggle();
        }
        if fullscreen_held && !prev_fullscreen {
            let doc = document();
            if doc.fullscreen_element().is_some() {
                doc.exit_fullscreen();
            } else if let Some(canvas) = doc.get_element_by_id("nes-canvas") {
                let _ = canvas.request_fullscreen();
            }
        }
        prev_fullscreen = fullscreen_held;
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
