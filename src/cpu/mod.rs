pub mod opcodes;

#[derive(Debug)]
pub struct Cpu {
    pub pc: u16,

    pub a: u8,
    pub x: u8,
    pub y: u8,

    pub stack: u8,
    status: u8
}

impl Cpu {
    pub fn new() -> Cpu {
        Cpu{pc: 0x400, a: 0, x: 0, y: 0, stack: 0, status: 0}
    }
}

// TODO: move to mem.rs?
fn get_16b_addr(mem: &[u8], offset: u16) -> u16 {
    ((mem[offset as usize+2] as u16) << 8) + mem[offset as usize+1] as u16
}

pub fn run(rom: Vec<u8>) {
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
            opcodes::ADC => {
                // Add Memory to Accumulator with Carry
                let operand: u8 = mem[cpu.pc as usize +1];
                let val: u32 = cpu.a as u32 + operand as u32;
                if val > u8::max_value() as u32 - 1 {
                    cpu.status = cpu.status | 0b01000000;
                    cpu.a = (val - u8::max_value() as u32 - 1) as u8;
                } else {
                    cpu.a = val as u8;
                }
                println!("0x{:x}: ADC 0x{:x}\t a=0x{:x}\t status=0x{:x}", cpu.pc, operand, cpu.a, cpu.status);
                cpu.pc += 2;
            },
            opcodes::BRK => {
                println!("BRK");
                break;
            },
            opcodes::INX => {
                // Increment Index X by One
                cpu.x += 1;
                println!("0x{:x}: INX\t x={:x}", cpu.pc, cpu.x);

                cpu.pc += 1;
            },
            opcodes::LDA => {
                // TODO: addressing modes
                cpu.a = mem[cpu.pc as usize+1];
                println!("0x{:x}: LDA 0x{:x}\t a={:x}", cpu.pc, cpu.pc+1, cpu.a);
                cpu.pc += 2;
            },
            opcodes::STA => {
                let addr: u16 = get_16b_addr(&mem, cpu.pc);
                mem[addr as usize] = cpu.a;
                println!("0x{:x}: STA 0x{:x}\t a={:x}",  cpu.pc, addr, cpu.a);
                cpu.pc += 3
            },
            opcodes::TAX => {
                // Transfer Accumulator to Index X
                cpu.x = cpu.a;
                println!("0x{:x}: TAX\t a={:x}", cpu.pc, cpu.a);
                cpu.pc += 1
            },
            _ => panic!("Unkown opcode: 0x{:x}", opcode)
        }
    }

}
