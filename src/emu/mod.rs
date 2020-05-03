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
                opcodes::ADC_IMM => {
                    // Add Memory to Accumulator with Carry
                    let operand: u8 = self.mem.ram[self.cpu.pc as usize +1];
                    self.cpu.add_to_a_with_carry(operand, false);
                    // TODO: build a common logging system
                    println!("0x{:x}: ADC 0x{:x}\t a=0x{:x}\t N={}\t V={}\t Z={}\t C={}", self.cpu.pc, operand, self.cpu.a, self.cpu.negative_flag(), self.cpu.overflow_flag(), self.cpu.zero_flag(), self.cpu.carry_flag());
                    self.cpu.pc += 2;
                },
                opcodes::ADC_ZP => {
                    // Add Memory to Accumulator with Carry
                    let operand: u8 = self.mem.ram[self.mem.ram[self.cpu.pc as usize +1] as usize];
                    self.cpu.add_to_a_with_carry(operand, true);
                    println!("0x{:x}: ADC 0x{:x}\t a=0x{:x}\t N={}\t V={}\t Z={}\t C={}", self.cpu.pc, operand, self.cpu.a, self.cpu.negative_flag(), self.cpu.overflow_flag(), self.cpu.zero_flag(), self.cpu.carry_flag());
                    self.cpu.pc += 2;
                },
                opcodes::BRK => {
                    println!("BRK");
                    break;
                },
                opcodes::DEX => {
                    // Decrement Index X by One
                    self.cpu.x -= 1;
                    println!("0x{:x}: DEX\t x={:x}", self.cpu.pc, self.cpu.x);
                    self.cpu.check_negative(self.cpu.x);
                    self.cpu.check_zero(self.cpu.x);

                    self.cpu.pc += 1;
                },
                opcodes::DEY => {
                    // Decrement Index Y by One
                    self.cpu.y -= 1;
                    println!("0x{:x}: DEY\t y={:x}", self.cpu.pc, self.cpu.y);
                    self.cpu.check_negative(self.cpu.y);
                    self.cpu.check_zero(self.cpu.y);

                    self.cpu.pc += 1;
                },
                opcodes::INX => {
                    // Increment Index X by One
                    self.cpu.x += 1;
                    println!("0x{:x}: INX\t x={:x}", self.cpu.pc, self.cpu.x);
                    self.cpu.check_negative(self.cpu.x);
                    self.cpu.check_zero(self.cpu.x);

                    self.cpu.pc += 1;
                },
                opcodes::INY => {
                    // Increment Index Y by One
                    self.cpu.y += 1;
                    println!("0x{:x}: INY\t y={:x}", self.cpu.pc, self.cpu.y);
                    self.cpu.check_negative(self.cpu.y);
                    self.cpu.check_zero(self.cpu.y);

                    self.cpu.pc += 1;
                },
                opcodes::LDA_ABS => {
                    self.cpu.a = self.mem.ram[self.cpu.pc as usize+1];
                    println!("0x{:x}: LDA 0x{:x}\t a={:x}", self.cpu.pc, self.cpu.pc+1, self.cpu.a);
                    self.cpu.pc += 2;
                },
                opcodes::SBC_IMM => {
                    // Subtract Memory to Accumulator with Carry
                    let operand: u8 = self.mem.ram[self.cpu.pc as usize +1];
                    self.cpu.sub_from_a_with_carry(operand, false);
                    println!("0x{:x}: SBC 0x{:x}\t a=0x{:x}\t N={}\t V={}\t Z={}\t C={}", self.cpu.pc, operand, self.cpu.a, self.cpu.negative_flag(), self.cpu.overflow_flag(), self.cpu.zero_flag(), self.cpu.carry_flag());
                    self.cpu.pc += 2;
                },
                opcodes::SBC_ZP => {
                    // Subtract Memory to Accumulator with Carry
                    let operand: u8 = self.mem.ram[self.mem.ram[self.cpu.pc as usize +1] as usize];
                    self.cpu.sub_from_a_with_carry(operand, true);
                    println!("0x{:x}: SBC 0x{:x}\t a=0x{:x}\t N={}\t V={}\t Z={}\t C={}", self.cpu.pc, operand, self.cpu.a, self.cpu.negative_flag(), self.cpu.overflow_flag(), self.cpu.zero_flag(), self.cpu.carry_flag());
                    self.cpu.pc += 2;
                },                opcodes::STA_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc);
                    self.mem.ram[addr as usize] = self.cpu.a;
                    println!("0x{:x}: STA 0x{:x}\t a={:x}",  self.cpu.pc, addr, self.cpu.a);
                    self.cpu.pc += 3
                },
                opcodes::STA_ZP => {
                    let addr: u16 = self.mem.ram[self.cpu.pc as usize + 1].into();
                    self.mem.ram[addr as usize] = self.cpu.a;
                    println!("0x{:x}: STA 0x{:x}\t a={:x}",  self.cpu.pc, addr, self.cpu.a);
                    self.cpu.pc += 2
                },
                opcodes::TAX => {
                    // Transfer Accumulator to Index X
                    self.cpu.x = self.cpu.a;
                    println!("0x{:x}: TAX\t x={:x}", self.cpu.pc, self.cpu.x);
                    self.cpu.check_negative(self.cpu.x);
                    self.cpu.check_zero(self.cpu.x);

                    self.cpu.pc += 1
                },
                opcodes::TXA => {
                    // Transfer Index X to Accumulator
                    self.cpu.a = self.cpu.x;
                    println!("0x{:x}: TXA\t a={:x}", self.cpu.pc, self.cpu.a);
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);

                    self.cpu.pc += 1
                },
                opcodes::TAY => {
                    // Transfer Accumulator to Index Y
                    self.cpu.y = self.cpu.a;
                    println!("0x{:x}: TAY\t y={:x}", self.cpu.pc, self.cpu.y);
                    self.cpu.check_negative(self.cpu.y);
                    self.cpu.check_zero(self.cpu.y);

                    self.cpu.pc += 1
                },
                opcodes::TYA => {
                    // Transfer Index Y to Accumulator
                    self.cpu.a = self.cpu.y;
                    println!("0x{:x}: TAY\t a={:x}", self.cpu.pc, self.cpu.a);
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);


                    self.cpu.pc += 1
                },
                opcodes::TSX => {
                    // Transfer Stack Pointer to Index X
                    self.cpu.x = self.cpu.sp;
                    println!("0x{:x}: TSX\t x={:x}", self.cpu.pc, self.cpu.x);
                    self.cpu.check_negative(self.cpu.x);
                    self.cpu.check_zero(self.cpu.x);

                    self.cpu.pc += 1
                },
                opcodes::TXS => {
                    // Transfer Index X to Stack Pointer
                    self.cpu.sp = self.cpu.x;
                    println!("0x{:x}: TXS\t sp={:x}", self.cpu.pc, self.cpu.sp);
                    self.cpu.check_negative(self.cpu.sp);
                    self.cpu.check_zero(self.cpu.sp);

                    self.cpu.pc += 1
                },
                _ => panic!("Unkown opcode: 0x{:x}", opcode)
            }
        }
    }
}
