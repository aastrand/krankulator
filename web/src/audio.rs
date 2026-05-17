use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{AudioContext, AudioContextOptions, AudioWorkletNode};

use krankulator_core::emu::audio::AudioBackend;

use super::AUDIO_CTX;

const AUDIO_TARGET_LEVEL: u32 = 2048;
const AUDIO_HIGH_WATER: u32 = 4096;

pub struct WebAudioBackend {
    port: Rc<RefCell<Option<web_sys::MessagePort>>>,
    buffer: Vec<f32>,
    worklet_level: Rc<Cell<u32>>,
}

impl WebAudioBackend {
    pub fn new(
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

pub fn create_audio_context() -> Option<AudioContext> {
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

pub async fn connect_audio_worklet(
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

pub fn setup_audio_resume_on_interaction() {
    let closure = Closure::wrap(Box::new(move |_: web_sys::Event| {
        AUDIO_CTX.with(|ac| {
            if let Some(ctx) = ac.borrow().as_ref() {
                if ctx.state() == web_sys::AudioContextState::Suspended {
                    let _ = ctx.resume();
                }
            }
        });
    }) as Box<dyn FnMut(_)>);

    let doc = super::document();
    let _ = doc.add_event_listener_with_callback("touchstart", closure.as_ref().unchecked_ref());
    let _ = doc.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref());
    closure.forget();
}

pub fn setup_visibility_pause() {
    let closure = Closure::wrap(Box::new(move |_: web_sys::Event| {
        AUDIO_CTX.with(|ac| {
            if let Some(ctx) = ac.borrow().as_ref() {
                if super::document().hidden() {
                    let _ = ctx.suspend();
                } else {
                    let _ = ctx.resume();
                }
            }
        });
    }) as Box<dyn FnMut(_)>);

    let doc = super::document();
    let _ = doc.add_event_listener_with_callback("visibilitychange", closure.as_ref().unchecked_ref());
    closure.forget();
}
