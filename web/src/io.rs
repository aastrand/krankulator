use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

use krankulator_core::emu;
use krankulator_core::emu::gfx;
use krankulator_core::emu::io::{controller, IOHandler, PollResult};
use krankulator_core::emu::memory::MemoryMapper;

use super::crt_renderer::CrtRenderer;
use super::document;

pub struct WebIOHandler {
    contexts: Vec<CanvasRenderingContext2d>,
    crt: Option<CrtRenderer>,
    keys: Rc<RefCell<HashSet<String>>>,
    rgba_buf: Vec<u8>,
    prev_f9: bool,
}

impl WebIOHandler {
    pub fn new(keys: Rc<RefCell<HashSet<String>>>) -> Self {
        Self {
            contexts: Vec::new(),
            crt: None,
            keys,
            rgba_buf: vec![0u8; 256 * 240 * 4],
            prev_f9: false,
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
        match CrtRenderer::new(&["nes-canvas", "nes-canvas-touch"]) {
            Ok(crt) => {
                self.crt = Some(crt);
            }
            Err(e) => {
                web_sys::console::warn_1(&format!("WebGL2 CRT init failed: {e}").into());
                for id in &["nes-canvas", "nes-canvas-touch"] {
                    if let Some(ctx) = get_canvas_ctx(id) {
                        self.contexts.push(ctx);
                    }
                }
                if self.contexts.is_empty() {
                    return Err("no canvas found".to_string());
                }
            }
        }
        Ok(())
    }

    fn log(&self, logline: String) {
        web_sys::console::log_1(&logline.into());
    }

    fn poll(&mut self, mem: &mut dyn MemoryMapper, _apu: &mut emu::apu::APU) -> PollResult {
        let keys = self.keys.borrow();

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

        let mut state: u8 = 0;
        for &(code, button) in mapping {
            if keys.contains(code) {
                state |= button;
            }
        }

        let mut rewind = keys.contains("KeyW");
        if let Some(gp) = super::input::poll_gamepad() {
            state |= gp.buttons;
            rewind |= gp.rewind;
        }

        let mut toasts = Vec::new();
        let f9_held = keys.contains("F9");
        if f9_held && !self.prev_f9 {
            if let Some(crt) = &mut self.crt {
                crt.enabled = !crt.enabled;
                if crt.enabled {
                    toasts.push("CRT scanlines ON".into());
                } else {
                    toasts.push("CRT scanlines OFF".into());
                }
            }
        }
        self.prev_f9 = f9_held;

        mem.controllers()[0].load_status(state);
        PollResult {
            rewind,
            toasts,
            ..PollResult::default()
        }
    }

    fn render(&mut self, buf: &gfx::buf::Buffer) {
        let rgb = &buf.data;
        for i in 0..(256 * 240) {
            self.rgba_buf[i * 4] = rgb[i * 3];
            self.rgba_buf[i * 4 + 1] = rgb[i * 3 + 1];
            self.rgba_buf[i * 4 + 2] = rgb[i * 3 + 2];
            self.rgba_buf[i * 4 + 3] = 255;
        }

        if let Some(crt) = &mut self.crt {
            crt.render(&self.rgba_buf);
            return;
        }

        if self.contexts.is_empty() {
            return;
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
