use super::opcodes::{self, Lookup};
use crate::emu::memory::MemoryMapper;

pub struct DisasmLine {
    pub addr: u16,
    pub bytes: [u8; 3],
    pub byte_count: u8,
    pub text: String,
}

fn format_operand(mode: usize, lo: u8, hi: u8) -> String {
    match mode {
        opcodes::ADDR_MODE_IMM => format!("#${lo:02X}"),
        opcodes::ADDR_MODE_ZP => format!("${lo:02X}"),
        opcodes::ADDR_MODE_ZPX => format!("${lo:02X},X"),
        opcodes::ADDR_MODE_ZPY => format!("${lo:02X},Y"),
        opcodes::ADDR_MODE_ABS => {
            let addr = (hi as u16) << 8 | lo as u16;
            format!("${addr:04X}")
        }
        opcodes::ADDR_MODE_ABX => {
            let addr = (hi as u16) << 8 | lo as u16;
            format!("${addr:04X},X")
        }
        opcodes::ADDR_MODE_ABY => {
            let addr = (hi as u16) << 8 | lo as u16;
            format!("${addr:04X},Y")
        }
        opcodes::ADDR_MODE_INX => format!("(${lo:02X},X)"),
        opcodes::ADDR_MODE_INY => format!("(${lo:02X}),Y"),
        _ => String::new(),
    }
}

fn mnemonic_base(name: &str) -> &str {
    if let Some(idx) = name.find('_') {
        &name[..idx]
    } else {
        name
    }
}

fn is_branch(opcode: u8) -> bool {
    matches!(
        opcode,
        0x10 | 0x30 | 0x50 | 0x70 | 0x90 | 0xB0 | 0xD0 | 0xF0
    )
}

fn is_jmp_indirect(opcode: u8) -> bool {
    opcode == 0x6C
}

fn is_relative(opcode: u8) -> bool {
    is_branch(opcode)
}

pub fn disassemble_one(addr: u16, mem: &dyn MemoryMapper, lookup: &Lookup) -> DisasmLine {
    let opcode = mem.cpu_peek(addr);
    let size = lookup.size(opcode) as u8;
    let mode = lookup.mode(opcode);
    let name = lookup.name(opcode);
    let base = mnemonic_base(name);

    let lo = if size >= 2 {
        mem.cpu_peek(addr.wrapping_add(1))
    } else {
        0
    };
    let hi = if size >= 3 {
        mem.cpu_peek(addr.wrapping_add(2))
    } else {
        0
    };

    let text = if size == 1 {
        base.to_string()
    } else if is_relative(opcode) {
        let offset = lo as i8;
        let target = addr.wrapping_add(2).wrapping_add(offset as u16);
        format!("{base} ${target:04X}")
    } else if is_jmp_indirect(opcode) {
        let indirect_addr = (hi as u16) << 8 | lo as u16;
        format!("{base} (${indirect_addr:04X})")
    } else {
        let operand = format_operand(mode, lo, hi);
        format!("{base} {operand}")
    };

    DisasmLine {
        addr,
        bytes: [opcode, lo, hi],
        byte_count: size,
        text,
    }
}

pub fn disassemble_around(
    pc: u16,
    lines_before: usize,
    lines_after: usize,
    mem: &dyn MemoryMapper,
    lookup: &Lookup,
) -> (Vec<DisasmLine>, usize) {
    let mut before = Vec::new();
    if lines_before > 0 {
        let scan_start = pc.saturating_sub((lines_before * 3 + 6) as u16);
        let mut scan_addr = scan_start;
        let mut candidates = Vec::new();
        while scan_addr < pc {
            candidates.push(scan_addr);
            let opcode = mem.cpu_peek(scan_addr);
            let size = lookup.size(opcode);
            scan_addr = scan_addr.wrapping_add(size.max(1));
        }
        if scan_addr == pc {
            let skip = candidates.len().saturating_sub(lines_before);
            for &addr in &candidates[skip..] {
                before.push(disassemble_one(addr, mem, lookup));
            }
        } else {
            let mut addr = pc.wrapping_sub(lines_before as u16);
            for _ in 0..lines_before {
                if addr >= pc {
                    break;
                }
                before.push(disassemble_one(addr, mem, lookup));
                let size = lookup.size(mem.cpu_peek(addr));
                addr = addr.wrapping_add(size.max(1));
            }
        }
    }

    let pc_idx = before.len();
    let mut result = before;
    result.push(disassemble_one(pc, mem, lookup));

    let mut addr = pc.wrapping_add(lookup.size(mem.cpu_peek(pc)).max(1));
    for _ in 0..lines_after {
        result.push(disassemble_one(addr, mem, lookup));
        let size = lookup.size(mem.cpu_peek(addr)).max(1);
        addr = addr.wrapping_add(size);
    }

    (result, pc_idx)
}
