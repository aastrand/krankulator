pub mod cpu;
pub mod memory;
pub mod opcodes;

pub fn run(rom: Vec<u8>) {
    let mut cpu: cpu::Cpu = cpu::Cpu::new();
    let mut mem: memory::Memory = memory::Memory::new();

    let mut i = 0;
    for code in rom.iter() {
        mem.ram[0x400 + i] = *code;
        i += 1;
    }

    loop {
        let opcode = mem.ram[cpu.pc as usize];

        match opcode {
            opcodes::ADC => {
                // Add Memory to Accumulator with Carry
                let operand: u8 = mem.ram[cpu.pc as usize +1];
                cpu.add_to_a_with_carry(operand);
                println!("0x{:x}: ADC 0x{:x}\t a=0x{:x}\t overflow={}", cpu.pc, operand, cpu.a, cpu.overflow_flag());
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
                cpu.a = mem.ram[cpu.pc as usize+1];
                println!("0x{:x}: LDA 0x{:x}\t a={:x}", cpu.pc, cpu.pc+1, cpu.a);
                cpu.pc += 2;
            },
            opcodes::STA => {
                let addr: u16 = mem.get_16b_addr(cpu.pc);
                mem.ram[addr as usize] = cpu.a;
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
