use super::Emulator;

use shrust::{Shell, ShellIO};
use std::io::prelude::*;

fn strip_hex_input(s: &str) -> &str {
    if s.len() > 1 {
        match &s[..2] {
            "0x" => &s[2..],
            _ => s,
        }
    } else {
        s
    }
}

pub fn debug(emu: &Emulator) {
    let mut shell = Shell::new(emu);
    shell.new_command("m", "mem.ram[addr]", 1, |io, emu, w| {
        let input = strip_hex_input(w[0]);
        match u16::from_str_radix(input, 16) {
            Ok(addr) => {
                writeln!(
                    io,
                    "self.mem.ram[0x{:x}] = 0x{:x}",
                    addr, emu.mem.ram[addr as usize]
                )?;
            }
            _ => {
                writeln!(io, "invalid address: {}", w[0])?;
            }
        }
        Ok(())
    });

    shell.new_command("o", "opcode", 1, |io, emu, w| {
        let input = strip_hex_input(w[0]);
        match u8::from_str_radix(input, 16) {
            Ok(o) => {
                writeln!(io, "opcode 0x{:x} => {}", o, emu.lookup.name(o))?;
            }
            _ => {
                writeln!(io, "invalid opcode: {}", w[0])?;
            }
        };
        Ok(())
    });

    shell.run_loop(&mut ShellIO::default());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_hex_input() {
        assert_eq!(strip_hex_input("1"), "1");
        assert_eq!(strip_hex_input("12"), "12");
        assert_eq!(strip_hex_input("0x"), "");
        assert_eq!(strip_hex_input("0x43"), "43");
    }
}
