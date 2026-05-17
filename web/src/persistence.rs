use std::cell::RefCell;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use super::{window, GENERATION};

fn local_storage() -> Option<web_sys::Storage> {
    window().local_storage().ok()?
}

fn rom_key(rom_hash: &str, suffix: &str) -> String {
    format!("krankulator:{}:{}", rom_hash, suffix)
}

pub fn hash_rom(data: &[u8]) -> String {
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

pub fn save_sram_to_storage(rom_hash: &str, sram: &[u8]) {
    let encoded = base64_encode(sram);
    storage_set(&rom_key(rom_hash, "sram"), &encoded);
}

pub fn load_sram_from_storage(rom_hash: &str) -> Option<Vec<u8>> {
    let encoded = storage_get(&rom_key(rom_hash, "sram"))?;
    base64_decode(&encoded)
}

pub fn save_state_to_storage(rom_hash: &str, slot: u8, data: &[u8]) {
    let encoded = base64_encode(data);
    storage_set(&rom_key(rom_hash, &format!("ss{}", slot)), &encoded);
}

pub fn load_state_from_storage(rom_hash: &str, slot: u8) -> Option<Vec<u8>> {
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

thread_local! {
    static BEFOREUNLOAD_CLOSURE: RefCell<Option<Closure<dyn FnMut(web_sys::Event)>>> = RefCell::new(None);
    pub static SRAM_SNAPSHOT: RefCell<Option<Vec<u8>>> = RefCell::new(None);
}

pub fn setup_beforeunload_sram(rom_hash: String, has_battery: bool, gen: u32) {
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
