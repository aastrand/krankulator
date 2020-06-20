use super::Emulator;

use shrust::{ExecError, Shell, ShellIO};
use std::collections::HashSet;
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

#[allow(unused_must_use)]
pub fn debug(emu: &mut Emulator) {
    let mut shell = Shell::new(emu);
    shell.new_command("m", "mem.ram[addr]", 1, |io, emu, w| {
        let input = strip_hex_input(w[0]);
        match u16::from_str_radix(input, 16) {
            Ok(addr) => {
                writeln!(
                    io,
                    "was self.mem.ram[0x{:x}] == 0x{:x}",
                    addr, emu.mem.ram[addr as usize]
                )?;

                if w.len() > 1 {
                    let value = strip_hex_input(w[1]);
                    match u8::from_str_radix(value, 16) {
                        Ok(v) => {
                            emu.mem.ram[addr as usize] = v;
                            writeln!(io, "wrote self.mem.ram[0x{:x}] = 0x{:x}", addr, v)?;
                        }
                        _ => {
                            writeln!(io, "invalid value: {}", w[1])?;
                        }
                    }
                }
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

    shell.new_command(
        "cpu",
        "edit cpu.<member> (a, x, y, sp, status, pc), e.g 'cpu a 0xff'",
        2,
        |io, emu, w| {
            let value = strip_hex_input(w[1]);
            match u16::from_str_radix(value, 16) {
                Ok(v) => match w[0] {
                    "a" => {
                        let value: u8 = (v & 0xff) as u8;
                        emu.cpu.a = value;
                        writeln!(io, "wrote cpu.{} = 0x{:x}", w[0], value)?;
                    }
                    "x" => {
                        let value: u8 = (v & 0xff) as u8;
                        emu.cpu.x = value;
                        writeln!(io, "wrote cpu.{} = 0x{:x}", w[0], value)?;
                    }
                    "y" => {
                        let value: u8 = (v & 0xff) as u8;
                        emu.cpu.y = value;
                        writeln!(io, "wrote cpu.{} = 0x{:x}", w[0], value)?;
                    }
                    "sp" => {
                        let value: u8 = (v & 0xff) as u8;
                        emu.cpu.sp = value;
                        writeln!(io, "wrote cpu.{} = 0x{:x}", w[0], value)?;
                    }
                    "status" => {
                        let value: u8 = (v & 0xff) as u8;
                        emu.cpu.status = value;
                        writeln!(io, "wrote cpu.{} = 0x{:x}", w[0], value)?;
                    }
                    "pc" => {
                        emu.cpu.pc = v;
                        writeln!(io, "wrote cpu.{} = 0x{:x}", w[0], v)?;
                    }
                    _ => {
                        writeln!(io, "invalid cpu member: {}", w[0])?;
                    }
                },
                _ => {
                    writeln!(io, "invalid value: {}", w[1])?;
                }
            };
            Ok(())
        },
    );

    shell.new_command("b", "add/remove breakpoint", 0, |io, emu, w| {
        if w.len() > 0 {
            writeln!(io, "{}", toggle_breakpoint(w[0], &mut emu.breakpoints));
        }

        writeln!(io, "breakpoints:")?;
        for b in emu.breakpoints.iter() {
            writeln!(
                io,
                "0x{:x}: {}",
                b,
                emu.lookup.name(emu.mem.ram[*b as usize]) // TODO: add arguments,
            )?;
        }
        Ok(())
    });

    shell.new_command_noargs("s", "toggle stepping", |io, emu| {
        emu.stepping = !emu.stepping;
        writeln!(io, "debug stepping now: {}", emu.stepping)?;

        Ok(())
    });

    shell.new_command_noargs("l", "toggle instruction logging", |io, emu| {
        emu.should_log = !emu.should_log;
        writeln!(io, "instruction logging enabled: {}", emu.should_log)?;

        Ok(())
    });

    shell.new_command_noargs("c", "continue", |_, _| Err(ExecError::Quit));
    shell.new_command_noargs("q", "quit", |_, _| {
        std::process::exit(0);
    });

    shell.run_loop(&mut ShellIO::default());
}

pub fn toggle_breakpoint(s: &str, breakpoints: &mut Box<HashSet<u16>>) -> String {
    let input = strip_hex_input(s);
    match u16::from_str_radix(input, 16) {
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
