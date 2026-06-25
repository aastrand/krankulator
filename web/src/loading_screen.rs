use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlImageElement};

use super::{document, window};

#[wasm_bindgen(inline_js = "
let progressValue = 0;
export function get_fetch_progress() { return progressValue; }
export async function fetch_with_progress(url) {
    progressValue = 0;
    const resp = await fetch(url);
    if (!resp.ok) throw new Error('HTTP ' + resp.status);
    const total = parseInt(resp.headers.get('content-length') || '0', 10);
    if (!total || !resp.body) {
        const buf = await resp.arrayBuffer();
        progressValue = 1.0;
        return new Uint8Array(buf);
    }
    const reader = resp.body.getReader();
    const chunks = [];
    let loaded = 0;
    while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        chunks.push(value);
        loaded += value.length;
        progressValue = Math.min(loaded / total, 1.0);
    }
    const result = new Uint8Array(loaded);
    let offset = 0;
    for (const chunk of chunks) {
        result.set(chunk, offset);
        offset += chunk.length;
    }
    progressValue = 1.0;
    return result;
}
")]
extern "C" {
    pub fn get_fetch_progress() -> f64;

    #[wasm_bindgen(catch)]
    pub async fn fetch_with_progress(url: &str) -> Result<JsValue, JsValue>;
}

thread_local! {
    static MEGAMAN_SPRITE: RefCell<Option<HtmlImageElement>> = RefCell::new(None);
    static LOADING_OVERLAYS: RefCell<Vec<(HtmlCanvasElement, CanvasRenderingContext2d)>> = RefCell::new(Vec::new());
    static LOADING_LABEL: RefCell<String> = RefCell::new(String::new());
    static LOADING_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

pub fn preload_sprite() {
    let img = HtmlImageElement::new().unwrap();
    img.set_src("megaman-walking.png");
    MEGAMAN_SPRITE.with(|s| *s.borrow_mut() = Some(img));
}

pub fn start_loading(label: &str) {
    stop_loading();

    let mut overlays = Vec::new();
    for id in &["nes-canvas", "nes-canvas-touch"] {
        if let Some(o) = create_overlay(id) {
            overlays.push(o);
        }
    }
    LOADING_OVERLAYS.with(|lo| *lo.borrow_mut() = overlays);
    LOADING_LABEL.with(|l| *l.borrow_mut() = label.to_string());
    LOADING_ACTIVE.with(|a| a.set(true));

    start_animation_loop();
}

pub fn stop_loading() {
    LOADING_ACTIVE.with(|a| a.set(false));
    LOADING_OVERLAYS.with(|lo| {
        for (canvas, _) in lo.borrow().iter() {
            if let Some(parent) = canvas.parent_node() {
                let _ = parent.remove_child(canvas);
            }
        }
        lo.borrow_mut().clear();
    });
}

fn create_overlay(target_id: &str) -> Option<(HtmlCanvasElement, CanvasRenderingContext2d)> {
    let target = document().get_element_by_id(target_id)?;
    let rect = target.get_bounding_client_rect();

    if rect.width() == 0.0 || rect.height() == 0.0 {
        return None;
    }

    let canvas: HtmlCanvasElement = document()
        .create_element("canvas")
        .ok()?
        .dyn_into()
        .ok()?;
    canvas.set_width(256);
    canvas.set_height(240);

    let style = canvas.style();
    let _ = style.set_property("position", "fixed");
    let _ = style.set_property("left", &format!("{}px", rect.left()));
    let _ = style.set_property("top", &format!("{}px", rect.top()));
    let _ = style.set_property("width", &format!("{}px", rect.width()));
    let _ = style.set_property("height", &format!("{}px", rect.height()));
    let _ = style.set_property("image-rendering", "pixelated");
    let _ = style.set_property("z-index", "1000");
    let _ = style.set_property("pointer-events", "none");

    document().body()?.append_child(&canvas).ok()?;

    let ctx = canvas
        .get_context("2d")
        .ok()??
        .dyn_into::<CanvasRenderingContext2d>()
        .ok()?;
    ctx.set_image_smoothing_enabled(false);

    Some((canvas, ctx))
}

fn start_animation_loop() {
    #[allow(clippy::type_complexity)]
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    let perf = window().performance().unwrap();
    let mut last_frame_time = perf.now();
    let mut anim_frame: u32 = 0;

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let active = LOADING_ACTIVE.with(|a| a.get());
        if !active {
            return;
        }

        let now = perf.now();
        if now - last_frame_time > 120.0 {
            anim_frame = (anim_frame + 1) % 4;
            last_frame_time = now;
        }

        let progress = get_fetch_progress();

        LOADING_OVERLAYS.with(|lo| {
            MEGAMAN_SPRITE.with(|sprite| {
                LOADING_LABEL.with(|label| {
                    let label = label.borrow();
                    let sprite = sprite.borrow();
                    for (_, ctx) in lo.borrow().iter() {
                        draw_frame(ctx, sprite.as_ref(), anim_frame, progress, &label);
                    }
                });
            });
        });

        window()
            .request_animation_frame(f.borrow().as_ref().unwrap().as_ref().unchecked_ref())
            .unwrap();
    }) as Box<dyn FnMut()>));

    window()
        .request_animation_frame(g.borrow().as_ref().unwrap().as_ref().unchecked_ref())
        .unwrap();
}

fn draw_frame(
    ctx: &CanvasRenderingContext2d,
    sprite: Option<&HtmlImageElement>,
    frame: u32,
    progress: f64,
    label: &str,
) {
    let w = 256.0_f64;
    let h = 240.0_f64;

    // Black background
    ctx.set_fill_style_str("#000000");
    ctx.fill_rect(0.0, 0.0, w, h);

    // Progress bar dimensions
    let bar_x = 24.0;
    let bar_w = 208.0;
    let bar_y = 160.0;
    let bar_h = 8.0;

    // Progress bar border (NES white)
    ctx.set_fill_style_str("#FCFCFC");
    ctx.fill_rect(bar_x - 2.0, bar_y - 2.0, bar_w + 4.0, bar_h + 4.0);

    // Progress bar inner background
    ctx.set_fill_style_str("#000000");
    ctx.fill_rect(bar_x, bar_y, bar_w, bar_h);

    // Progress bar fill (NES blue)
    let fill_w = progress * bar_w;
    ctx.set_fill_style_str("#5C94FC");
    ctx.fill_rect(bar_x, bar_y, fill_w, bar_h);

    // Mega Man sprite
    if let Some(sprite) = sprite {
        if sprite.complete() && sprite.natural_width() > 0 {
            let src_w = 25.0;
            let src_h = 24.0;
            let scale = 1.0;
            let dst_w = src_w * scale;
            let dst_h = src_h * scale;

            let mm_x = bar_x + progress * (bar_w - dst_w);
            let mm_y = bar_y - dst_h + 4.0;

            let frame_sx = (frame % 4) as f64 * src_w;
            ctx.save();
            ctx.translate(mm_x + dst_w / 2.0, mm_y + dst_h / 2.0).unwrap();
            ctx.scale(-1.0, 1.0).unwrap();
            let _ = ctx
                .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                    sprite, frame_sx, 0.0, src_w, src_h, -dst_w / 2.0, -dst_h / 2.0, dst_w, dst_h,
                );
            ctx.restore();
        }
    }

    // "LOADING" text
    ctx.set_fill_style_str("#FCFCFC");
    ctx.set_font("8px 'Press Start 2P', monospace");
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    let _ = ctx.fill_text("LOADING", w / 2.0, 70.0);

    // Game name
    let display_name = if label.len() > 28 {
        &label[..28]
    } else {
        label
    };
    let _ = ctx.fill_text(display_name, w / 2.0, 90.0);

    // Percentage text
    let pct = (progress * 100.0) as u32;
    let _ = ctx.fill_text(&format!("{}%", pct), w / 2.0, 190.0);
}
