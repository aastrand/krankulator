use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::KeyboardEvent;

use krankulator_core::emu::io::controller;

use super::{document, window};

pub const MAPPED_KEYS: &[&str] = &[
    "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight",
    "KeyZ", "KeyX", "KeyC", "KeyV",
    "KeyS", "KeyA", "KeyQ", "KeyF", "Tab",
];

pub fn setup_keyboard(keys: Rc<RefCell<HashSet<String>>>) {
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

pub fn setup_touch_controls(keys: Rc<RefCell<HashSet<String>>>) {
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

pub fn setup_canvas_double_tap(keys: Rc<RefCell<HashSet<String>>>) {
    let last_tap: Rc<Cell<f64>> = Rc::new(Cell::new(0.0));
    let perf = window().performance().unwrap();

    for id in &["nes-canvas", "nes-canvas-touch"] {
        let Some(el) = document().get_element_by_id(id) else { continue };
        let last = last_tap.clone();
        let perf = perf.clone();
        let keys = keys.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
            if e.touches().length() != 1 {
                return;
            }
            let now = perf.now();
            if now - last.get() < 300.0 {
                keys.borrow_mut().insert("Tab".into());
                let keys2 = keys.clone();
                let remove = Closure::once_into_js(move || {
                    keys2.borrow_mut().remove("Tab");
                });
                let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(
                    remove.unchecked_ref(),
                    50,
                );
                last.set(0.0);
            } else {
                last.set(now);
            }
        }) as Box<dyn FnMut(_)>);
        el.add_event_listener_with_callback("touchstart", closure.as_ref().unchecked_ref()).unwrap();
        closure.forget();
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

pub struct GamepadPollResult {
    pub buttons: u8,
    pub save_state: bool,
    pub load_state: bool,
    pub cycle_slot: bool,
}

pub fn poll_gamepad() -> Option<GamepadPollResult> {
    let gamepads = window().navigator().get_gamepads().ok()?;
    for i in 0..gamepads.length() {
        let gp: web_sys::Gamepad = gamepads.get(i).dyn_into().ok()?;
        if !gp.connected() || gp.mapping() != web_sys::GamepadMappingType::Standard {
            continue;
        }

        let btns = gp.buttons();
        let btn = |idx: u32| -> bool {
            btns.get(idx)
                .dyn_into::<web_sys::GamepadButton>()
                .ok()
                .map_or(false, |b| b.pressed())
        };

        let axes = gp.axes();
        let axis = |idx: u32| -> f64 {
            axes.get(idx).as_f64().unwrap_or(0.0)
        };

        let lx = axis(0);
        let ly = axis(1);

        let mut buttons: u8 = 0;
        if btn(1) { buttons |= controller::A; }
        if btn(0) { buttons |= controller::B; }
        if btn(9) { buttons |= controller::START; }
        if btn(8) { buttons |= controller::SELECT; }
        if btn(12) || ly < -0.5 { buttons |= controller::UP; }
        if btn(13) || ly > 0.5 { buttons |= controller::DOWN; }
        if btn(14) || lx < -0.5 { buttons |= controller::LEFT; }
        if btn(15) || lx > 0.5 { buttons |= controller::RIGHT; }

        return Some(GamepadPollResult {
            buttons,
            save_state: btn(5),
            load_state: btn(4),
            cycle_slot: btn(6),
        });
    }
    None
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
