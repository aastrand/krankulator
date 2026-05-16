use super::super::util;
use std::collections::HashSet;

pub fn toggle_breakpoint(s: &str, breakpoints: &mut Box<HashSet<u16>>) -> String {
    match util::hex_str_to_u16(s) {
        Ok(o) => {
            if breakpoints.contains(&o) {
                breakpoints.remove(&o);
                format!("removed breakpoint 0x{:x}", o)
            } else {
                breakpoints.insert(o);
                format!("added breakpoint 0x{:x}", o)
            }
        }
        _ => format!("invalid address: {}", s),
    }
}
