mod asm;
mod cpu;

use asm::util;
use cpu::opcodes;
use cpu::Cpu;

use std::env;

// TODO: move
fn get_16b_addr(mem: &[u8], offset: u16) -> u16 {
    ((mem[offset as usize+2] as u16) << 8) + mem[offset as usize+1] as u16
}

fn help() {
    println!("Usage: krankulator <path-to-code>");
}

fn run(rom: Vec<u8>) {
    let mut cpu: Cpu =  Cpu::new();
    let mut mem: [u8; 65536] = [0; 65536];

    let mut i = 0;
    for code in rom.iter() {
        mem[0x400 + i] = *code;
        i += 1;
    }

    loop {
        let opcode = mem[cpu.pc as usize];
        match opcode {
            opcodes::BRK => {
                println!("BRK");
                break;
            },
            opcodes::LDA => {
                // TODO: addressing modes
                cpu.a = mem[cpu.pc as usize+1];
                println!("LDA 0x{:x}, a={:x}", cpu.pc+1, cpu.a);
                cpu.pc += 2
            },
            opcodes::STA => {
                let addr: u16 = get_16b_addr(&mem, cpu.pc);
                mem[addr as usize] = cpu.a;
                println!("STA 0x{:x}, a={:x}", addr, cpu.a);
                cpu.pc += 3
            },
            _ => panic!("Unkown opcode: 0x{:x}", opcode)
        }
    }

}

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.len() {
        2 => {
            run(util::read_code(&args[1]))
        },
        _ => help()
    }
}
