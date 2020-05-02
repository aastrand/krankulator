pub mod cpu;
pub mod memory;
pub mod opcodes;

pub struct Emulator {
    pub cpu: cpu::Cpu,
    pub mem: memory::Memory
}

impl Emulator {
    pub fn new() -> Emulator {
        Emulator{
            cpu: cpu::Cpu::new(),
            mem: memory::Memory::new()
        }
    }

    pub fn install_rom(&mut self, rom: Vec<u8>) {
        let mut i = 0;
        for code in rom.iter() {
            self.mem.ram[0x400 + i] = *code;
            i += 1;
        }
    }

    pub fn run(&mut self) {
        loop {
            let opcode = self.mem.ram[self.cpu.pc as usize];

            match opcode {
                opcodes::ADC => {
                    // Add Memory to Accumulator with Carry
                    let operand: u8 = self.mem.ram[self.cpu.pc as usize +1];
                    self.cpu.add_to_a_with_carry(operand);
                    println!("0x{:x}: ADC 0x{:x}\t a=0x{:x}\t overflow={}", self.cpu.pc, operand, self.cpu.a, self.cpu.overflow_flag());
                    self.cpu.pc += 2;
                },
                opcodes::BRK => {
                    println!("BRK");
                    break;
                },
                opcodes::INX => {
                    // Increment Index X by One
                    self.cpu.x += 1;
                    println!("0x{:x}: INX\t x={:x}", self.cpu.pc, self.cpu.x);

                    self.cpu.pc += 1;
                },
                opcodes::LDA => {
                    // TODO: addressing modes
                    self.cpu.a = self.mem.ram[self.cpu.pc as usize+1];
                    println!("0x{:x}: LDA 0x{:x}\t a={:x}", self.cpu.pc, self.cpu.pc+1, self.cpu.a);
                    self.cpu.pc += 2;
                },
                opcodes::STA => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc);
                    self.mem.ram[addr as usize] = self.cpu.a;
                    println!("0x{:x}: STA 0x{:x}\t a={:x}",  self.cpu.pc, addr, self.cpu.a);
                    self.cpu.pc += 3
                },
                opcodes::TAX => {
                    // Transfer Accumulator to Index X
                    self.cpu.x = self.cpu.a;
                    println!("0x{:x}: TAX\t a={:x}", self.cpu.pc, self.cpu.a);
                    self.cpu.pc += 1
                },
                _ => panic!("Unkown opcode: 0x{:x}", opcode)
            }
        }
    }
}
