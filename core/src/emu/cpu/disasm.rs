use super::opcodes::{self, Lookup};
use crate::emu::memory::MemoryMapper;

pub struct DisasmLine {
    pub addr: u16,
    pub bytes: [u8; 3],
    pub byte_count: u8,
    pub text: String,
    pub operand_detail: Option<String>,
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

fn is_store(opcode: u8) -> bool {
    matches!(
        opcode,
        0x85 | 0x95 | 0x8D | 0x9D | 0x99 | 0x81 | 0x91 // STA
        | 0x86 | 0x96 | 0x8E // STX
        | 0x84 | 0x94 | 0x8C // STY
    )
}

pub struct CpuRegs {
    pub x: u8,
    pub y: u8,
}

fn resolve_operand(
    mode: usize,
    lo: u8,
    hi: u8,
    opcode: u8,
    regs: Option<&CpuRegs>,
    mem: &dyn MemoryMapper,
) -> Option<String> {
    let regs = regs?;
    let (eff_addr, show_addr) = match mode {
        opcodes::ADDR_MODE_ZP => (lo as u16, true),
        opcodes::ADDR_MODE_ZPX => (lo.wrapping_add(regs.x) as u16, true),
        opcodes::ADDR_MODE_ZPY => (lo.wrapping_add(regs.y) as u16, true),
        opcodes::ADDR_MODE_ABS => {
            let a = (hi as u16) << 8 | lo as u16;
            (a, false)
        }
        opcodes::ADDR_MODE_ABX => {
            let a = ((hi as u16) << 8 | lo as u16).wrapping_add(regs.x as u16);
            (a, true)
        }
        opcodes::ADDR_MODE_ABY => {
            let a = ((hi as u16) << 8 | lo as u16).wrapping_add(regs.y as u16);
            (a, true)
        }
        opcodes::ADDR_MODE_INX => {
            let ptr = lo.wrapping_add(regs.x);
            let lo_byte = mem.cpu_peek(ptr as u16);
            let hi_byte = mem.cpu_peek(ptr.wrapping_add(1) as u16);
            let a = (hi_byte as u16) << 8 | lo_byte as u16;
            (a, true)
        }
        opcodes::ADDR_MODE_INY => {
            let lo_byte = mem.cpu_peek(lo as u16);
            let hi_byte = mem.cpu_peek(lo.wrapping_add(1) as u16);
            let a = ((hi_byte as u16) << 8 | lo_byte as u16).wrapping_add(regs.y as u16);
            (a, true)
        }
        _ => return None,
    };

    if is_store(opcode) {
        if show_addr {
            Some(format!("@ {:04X}", eff_addr))
        } else {
            None
        }
    } else {
        let val = mem.cpu_peek(eff_addr);
        if show_addr {
            Some(format!("@ {:04X} = {:02X}", eff_addr, val))
        } else {
            Some(format!("= {:02X}", val))
        }
    }
}

pub fn disassemble_one(
    addr: u16,
    mem: &dyn MemoryMapper,
    lookup: &Lookup,
    regs: Option<&CpuRegs>,
) -> DisasmLine {
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

    let operand_detail = resolve_operand(mode, lo, hi, opcode, regs, mem);

    DisasmLine {
        addr,
        bytes: [opcode, lo, hi],
        byte_count: size,
        text,
        operand_detail,
    }
}

pub fn disassemble_around(
    pc: u16,
    lines_before: usize,
    lines_after: usize,
    mem: &dyn MemoryMapper,
    lookup: &Lookup,
    regs: Option<&CpuRegs>,
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
                before.push(disassemble_one(addr, mem, lookup, None));
            }
        } else {
            let mut addr = pc.wrapping_sub(lines_before as u16);
            for _ in 0..lines_before {
                if addr >= pc {
                    break;
                }
                before.push(disassemble_one(addr, mem, lookup, None));
                let size = lookup.size(mem.cpu_peek(addr));
                addr = addr.wrapping_add(size.max(1));
            }
        }
    }

    let pc_idx = before.len();
    let mut result = before;
    result.push(disassemble_one(pc, mem, lookup, regs));

    let mut addr = pc.wrapping_add(lookup.size(mem.cpu_peek(pc)).max(1));
    for _ in 0..lines_after {
        result.push(disassemble_one(addr, mem, lookup, None));
        let size = lookup.size(mem.cpu_peek(addr)).max(1);
        addr = addr.wrapping_add(size);
    }

    (result, pc_idx)
}
