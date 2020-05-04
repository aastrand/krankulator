pub mod cpu;
pub mod memory;
pub mod opcodes;

pub struct Emulator {
    pub cpu: cpu::Cpu,
    pub mem: memory::Memory,
    pub lookup: opcodes::Lookup
}

impl Emulator {
    pub fn new() -> Emulator {
        Emulator{
            cpu: cpu::Cpu::new(),
            mem: memory::Memory::new(),
            lookup: opcodes::Lookup::new()
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
                    let operand: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    self.cpu.add_to_a_with_carry(operand);
                },
                opcodes::ADC_ZP => {
                    // Add Memory to Accumulator with Carry
                    let operand: u8 = self.mem.indirect_value_at_addr(self.cpu.pc + 1);
                    self.cpu.add_to_a_with_carry(operand);
                },
                opcodes::BRK => {
                    println!("BRK");
                    break;
                },
                opcodes::CLC => {
                    self.cpu.clear_status_flag(cpu::CARRY_BIT);
                },
                opcodes::DEX => {
                    // Decrement Index X by One
                    self.cpu.x -= 1;
                    println!("0x{:x}: DEX\t x={:x}", self.cpu.pc, self.cpu.x);
                    // Increment and decrement instructions do not affect the carry flag.
                    self.cpu.check_negative(self.cpu.x);
                    self.cpu.check_zero(self.cpu.x);
                },
                opcodes::DEY => {
                    // Decrement Index Y by One
                    self.cpu.y -= 1;
                    // Increment and decrement instructions do not affect the carry flag.
                    self.cpu.check_negative(self.cpu.y);
                    self.cpu.check_zero(self.cpu.y);
                },
                opcodes::INX => {
                    // Increment Index X by One
                    self.cpu.x += 1;
                    // Increment and decrement instructions do not affect the carry flag.
                    self.cpu.check_negative(self.cpu.x);
                    self.cpu.check_zero(self.cpu.x);
                },
                opcodes::INY => {
                    // Increment Index Y by One
                    self.cpu.y += 1;
                    // Increment and decrement instructions do not affect the carry flag.
                    self.cpu.check_negative(self.cpu.y);
                    self.cpu.check_zero(self.cpu.y);
                },
                opcodes::LDA_IMM => {
                    self.cpu.a = self.mem.value_at_addr(self.cpu.pc + 1);
                },
                opcodes::LDX_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc);
                    self.cpu.x = self.mem.value_at_addr(addr);
                },
                opcodes::LDX_IMM => {
                    self.cpu.x = self.mem.value_at_addr(self.cpu.pc + 1);
                },
                opcodes::LDY_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc);
                    self.cpu.y = self.mem.value_at_addr(addr);
                },
                opcodes::LDY_IMM => {
                    self.cpu.y = self.mem.value_at_addr(self.cpu.pc + 1);
                },
                opcodes::SBC_IMM => {
                    // Subtract Memory to Accumulator with Carry
                    let operand: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    self.cpu.sub_from_a_with_carry(operand);
                },
                opcodes::SBC_ZP => {
                    // Subtract Memory to Accumulator with Carry
                    let operand: u8 = self.mem.indirect_value_at_addr(self.cpu.pc + 1);
                    self.cpu.sub_from_a_with_carry(operand);
                },
                opcodes::SEC => {
                    self.cpu.set_status_flag(cpu::CARRY_BIT);
                },
                opcodes::STA_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc);
                    self.mem.ram[addr as usize] = self.cpu.a;
                },
                opcodes::STA_ZP => {
                    let addr: u16 = self.mem.value_at_addr(self.cpu.pc + 1).into();
                    self.mem.ram[addr as usize] = self.cpu.a;
                },
                opcodes::TAX => {
                    // Transfer Accumulator to Index X
                    self.cpu.x = self.cpu.a;
                    self.cpu.check_negative(self.cpu.x);
                    self.cpu.check_zero(self.cpu.x);
                },
                opcodes::TXA => {
                    // Transfer Index X to Accumulator
                    self.cpu.a = self.cpu.x;
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                },
                opcodes::TAY => {
                    // Transfer Accumulator to Index Y
                    self.cpu.y = self.cpu.a;
                    self.cpu.check_negative(self.cpu.y);
                    self.cpu.check_zero(self.cpu.y);
                },
                opcodes::TYA => {
                    // Transfer Index Y to Accumulator
                    self.cpu.a = self.cpu.y;
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                },
                opcodes::TSX => {
                    // Transfer Stack Pointer to Index X
                    self.cpu.x = self.cpu.sp;
                    self.cpu.check_negative(self.cpu.x);
                    self.cpu.check_zero(self.cpu.x);
                },
                opcodes::TXS => {
                    // Transfer Index X to Stack Pointer
                    self.cpu.sp = self.cpu.x;
                    self.cpu.check_negative(self.cpu.sp);
                    self.cpu.check_zero(self.cpu.sp);
                },
                _ => panic!("Unkown opcode: 0x{:x}", opcode)
            }

            self.log(opcode);

            let size: u16 = self.lookup.size(opcode);
            if size == 0 {
                panic!("Opcode {:x} missing from lookup table, see opcode.rs", opcode);
            }
            self.cpu.pc += size;
        }
    }

    fn log(&self, opcode: u8) {
        let mut logline = String::with_capacity(80);

        logline.push_str(&format!("0x{:x} {}(0x{:x})", self.cpu.pc, self.lookup.name(opcode), opcode));
        // TODO: optional operands go here
        logline.push_str(&format!("\t\tta=0x{:x} x=0x{:x} y=0x{:x} sp=0x{:x}", self.cpu.a, self.cpu.x, self.cpu.y, self.cpu.sp));
        logline.push_str(&format!("\tN={} V={} Z={} C={}", self.cpu.negative_flag() as i32, self.cpu.overflow_flag() as i32, self.cpu.zero_flag() as i32, self.cpu.carry_flag() as i32));

        println!("{}", logline);
    }
}
