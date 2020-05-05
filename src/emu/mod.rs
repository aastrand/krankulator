pub mod cpu;
pub mod memory;
pub mod opcodes;

pub struct Emulator {
    pub cpu: cpu::Cpu,
    pub mem: memory::Memory,
    pub lookup: opcodes::Lookup,
}

impl Emulator {
    pub fn new() -> Emulator {
        Emulator {
            cpu: cpu::Cpu::new(),
            mem: memory::Memory::new(),
            lookup: opcodes::Lookup::new(),
        }
    }

    pub fn install_rom(&mut self, rom: Vec<u8>) {
        let mut i = 0;
        for code in rom.iter() {
            self.mem.ram[memory::CODE_START_ADDR as usize + i] = *code;
            i += 1;
        }
    }

    pub fn run(&mut self) {
        loop {
            let opcode = self.mem.ram[self.cpu.pc as usize];
            let mut logdata: Vec<u16> = Vec::<u16>::new();
            logdata.push(self.cpu.pc);

            match opcode {
                opcodes::ADC_IMM => {
                    // Add Memory to Accumulator with Carry
                    let operand: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.add_to_a_with_carry(operand);
                }
                opcodes::ADC_ZP => {
                    // Add Memory to Accumulator with Carry
                    let operand: u8 = self.mem.indirect_value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.add_to_a_with_carry(operand);
                }

                opcodes::BPL => {
                    let operand: i8 = self.mem.value_at_addr(self.cpu.pc + 1) as i8;
                    logdata.push(operand as u16);
                    // Branch on PLus)
                    if !self.cpu.negative_flag() {
                        self.cpu.pc = (self.cpu.pc as i16 + operand as i16) as u16;
                    }
                }
                opcodes::BMI => {
                    let operand: i8 = self.mem.value_at_addr(self.cpu.pc + 1) as i8;
                    logdata.push(operand as u16);
                    // Branch on MInus
                    if self.cpu.negative_flag() {
                        self.cpu.pc = (self.cpu.pc as i16 + operand as i16) as u16;
                    }
                }
                opcodes::BVC => {
                    let operand: i8 = self.mem.value_at_addr(self.cpu.pc + 1) as i8;
                    logdata.push(operand as u16);
                    // Branch on oVerflow Clear
                    if !self.cpu.overflow_flag() {
                        self.cpu.pc = (self.cpu.pc as i16 + operand as i16) as u16;
                    }
                }
                opcodes::BVS => {
                    let operand: i8 = self.mem.value_at_addr(self.cpu.pc + 1) as i8;
                    logdata.push(operand as u16);
                    // ranch on oVerflow Set
                    if self.cpu.overflow_flag() {
                        self.cpu.pc = (self.cpu.pc as i16 + operand as i16) as u16;
                    }
                }
                opcodes::BCC => {
                    let operand: i8 = self.mem.value_at_addr(self.cpu.pc + 1) as i8;
                    logdata.push(operand as u16);
                    // Branch on Carry Clear
                    if !self.cpu.carry_flag() {
                        self.cpu.pc = (self.cpu.pc as i16 + operand as i16) as u16;
                    }
                }
                opcodes::BCS => {
                    let operand: i8 = self.mem.value_at_addr(self.cpu.pc + 1) as i8;
                    logdata.push(operand as u16);
                    // Branch on Carry Set
                    if self.cpu.carry_flag() {
                        self.cpu.pc = (self.cpu.pc as i16 + operand as i16) as u16;
                    }
                }
                opcodes::BEQ => {
                    let operand: i8 = self.mem.value_at_addr(self.cpu.pc + 1) as i8;
                    logdata.push(operand as u16);
                    // Branch on EQual
                    if self.cpu.zero_flag() {
                        self.cpu.pc = (self.cpu.pc as i16 + operand as i16) as u16;
                    }
                }
                opcodes::BNE => {
                    let operand: i8 = self.mem.value_at_addr(self.cpu.pc + 1) as i8;
                    logdata.push(operand as u16);
                    // Branch on Not Equal
                    if !self.cpu.zero_flag() {
                        self.cpu.pc = (self.cpu.pc as i16 + operand as i16) as u16;
                    }
                }

                opcodes::BRK => {
                    println!("BRK");
                    break;
                }
                opcodes::CLC => {
                    self.cpu.clear_status_flag(cpu::CARRY_BIT);
                }

                opcodes::CMP_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    let operand: u8 = self.mem.value_at_addr(addr);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.a, operand);
                }
                opcodes::CMP_IMM => {
                    let operand: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.a, operand);
                }
                opcodes::CMP_ZP => {
                    let operand: u8 = self.mem.indirect_value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.a, operand);
                }
                opcodes::CPX_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    let operand: u8 = self.mem.value_at_addr(self.cpu.pc);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.x, operand);
                }
                opcodes::CPX_IMM => {
                    let operand: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.x, operand);
                }
                opcodes::CPX_ZP => {
                    let operand: u8 = self.mem.indirect_value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.x, operand);
                }
                opcodes::CPY_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    let operand: u8 = self.mem.value_at_addr(addr);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.y, operand);
                }
                opcodes::CPY_IMM => {
                    let operand: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.y, operand);
                }
                opcodes::CPY_ZP => {
                    let operand: u8 = self.mem.indirect_value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.y, operand);
                }

                opcodes::DEX => {
                    // Decrement Index X by One
                    self.cpu.x = self.cpu.x.wrapping_sub(1);
                    // Increment and decrement instructions do not affect the carry flag.
                    self.cpu.check_negative(self.cpu.x);
                    self.cpu.check_zero(self.cpu.x);
                }
                opcodes::DEY => {
                    // Decrement Index Y by One
                    self.cpu.y = self.cpu.y.wrapping_sub(1);
                    // Increment and decrement instructions do not affect the carry flag.
                    self.cpu.check_negative(self.cpu.y);
                    self.cpu.check_zero(self.cpu.y);
                }

                opcodes::INX => {
                    // Increment Index X by One
                    self.cpu.x = self.cpu.x.wrapping_add(1);
                    // Increment and decrement instructions do not affect the carry flag.
                    self.cpu.check_negative(self.cpu.x);
                    self.cpu.check_zero(self.cpu.x);
                }
                opcodes::INY => {
                    // Increment Index Y by One
                    self.cpu.y = self.cpu.y.wrapping_add(1);
                    // Increment and decrement instructions do not affect the carry flag.
                    self.cpu.check_negative(self.cpu.y);
                    self.cpu.check_zero(self.cpu.y);
                }

                opcodes::LDA_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    self.cpu.a = self.mem.value_at_addr(addr);
                }
                opcodes::LDA_ABX => {
                    let addr: u16 = self
                        .mem
                        .get_16b_addr(self.cpu.pc + 1)
                        .wrapping_add(self.cpu.x as u16);
                    logdata.push(addr);
                    self.cpu.a = self.mem.value_at_addr(addr);
                }
                opcodes::LDA_ABY => {
                    let addr: u16 = self
                        .mem
                        .get_16b_addr(self.cpu.pc + 1)
                        .wrapping_add(self.cpu.y as u16);
                    logdata.push(addr);
                    self.cpu.a = self.mem.value_at_addr(addr);
                }
                opcodes::LDA_IMM => {
                    let value: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    logdata.push(value as u16);
                    self.cpu.a = value;
                }
                opcodes::LDA_INX => {
                    let value: u8 = self
                        .mem
                        .value_at_addr(self.cpu.pc + 1)
                        .wrapping_add(self.cpu.x);
                    let addr: u16 = self.mem.get_16b_addr(value as u16);
                    logdata.push(addr);
                    self.cpu.a = self.mem.value_at_addr(addr);
                }
                opcodes::LDA_INY => {
                    let value: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    let addr: u16 = value.wrapping_add(self.cpu.y) as u16;
                    logdata.push(addr);
                    self.cpu.a = self.mem.value_at_addr(addr);
                }
                opcodes::LDA_ZP => {
                    let addr: u16 = self.mem.value_at_addr(self.cpu.pc + 1) as u16;
                    logdata.push(addr);
                    self.cpu.a = self.mem.value_at_addr(addr);
                }
                opcodes::LDA_ZPX => {
                    let addr: u16 = self
                        .mem
                        .value_at_addr(self.cpu.pc + 1)
                        .wrapping_add(self.cpu.x) as u16;
                    logdata.push(addr);
                    self.cpu.a = self.mem.value_at_addr(addr);
                }

                opcodes::LDX_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    self.cpu.x = self.mem.value_at_addr(addr);
                }
                opcodes::LDX_IMM => {
                    let value: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    logdata.push(value as u16);
                    self.cpu.x = value;
                }
                opcodes::LDY_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    self.cpu.y = self.mem.value_at_addr(addr);
                }
                opcodes::LDY_IMM => {
                    let value: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    logdata.push(value as u16);
                    self.cpu.y = value;
                }
                opcodes::SBC_IMM => {
                    // Subtract Memory to Accumulator with Carry
                    let operand: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.sub_from_a_with_carry(operand);
                }
                opcodes::SBC_ZP => {
                    // Subtract Memory to Accumulator with Carry
                    let operand: u8 = self.mem.indirect_value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.sub_from_a_with_carry(operand);
                }
                opcodes::SEC => {
                    self.cpu.set_status_flag(cpu::CARRY_BIT);
                }

                opcodes::PHA => {
                    // PusH Accumulator
                    self.mem.push_to_stack(self.cpu.sp, self.cpu.a);
                    self.cpu.sp = self.cpu.sp.wrapping_sub(1);
                }
                opcodes::PLA => {
                    // PuLl Accumulator
                    if self.cpu.sp == 0xff {
                        self.cpu.set_status_flag(cpu::OVERFLOW_BIT);
                    }
                    self.cpu.sp = self.cpu.sp.wrapping_add(1);
                    self.cpu.a = self.mem.pull_from_stack(self.cpu.sp);
                }
                opcodes::PHP => {
                    // PusH Processor status
                    self.mem.push_to_stack(self.cpu.sp, self.cpu.status);
                    self.cpu.sp = self.cpu.sp.wrapping_sub(1);
                }
                opcodes::PLP => {
                    // PuLl Processor status
                    if self.cpu.sp == 0xff {
                        self.cpu.set_status_flag(cpu::OVERFLOW_BIT);
                    }
                    self.cpu.sp = self.cpu.sp.wrapping_add(1);
                    self.cpu.status = self.mem.pull_from_stack(self.cpu.sp);
                }

                opcodes::STA_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    self.mem.ram[addr as usize] = self.cpu.a;
                }
                opcodes::STA_ZP => {
                    let addr: u16 = self.mem.value_at_addr(self.cpu.pc + 1).into();
                    logdata.push(addr);
                    self.mem.ram[addr as usize] = self.cpu.a;
                }
                opcodes::STA_ABY => {
                    let addr: u16 = self
                        .mem
                        .get_16b_addr(self.cpu.pc + 1)
                        .wrapping_add(self.cpu.y as u16);
                    logdata.push(addr);
                    self.mem.ram[(addr) as usize] = self.cpu.a;
                }
                opcodes::STX_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    self.mem.ram[addr as usize] = self.cpu.x;
                }
                opcodes::STX_ZP => {
                    let addr: u16 = self.mem.value_at_addr(self.cpu.pc + 1).into();
                    logdata.push(addr);
                    self.mem.ram[addr as usize] = self.cpu.x;
                }
                opcodes::STY_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    self.mem.ram[addr as usize] = self.cpu.y;
                }
                opcodes::STY_ZP => {
                    let addr: u16 = self.mem.value_at_addr(self.cpu.pc + 1).into();
                    logdata.push(addr);
                    self.mem.ram[addr as usize] = self.cpu.y;
                }
                opcodes::TAX => {
                    // Transfer Accumulator to Index X
                    self.cpu.x = self.cpu.a;
                    self.cpu.check_negative(self.cpu.x);
                    self.cpu.check_zero(self.cpu.x);
                }
                opcodes::TXA => {
                    // Transfer Index X to Accumulator
                    self.cpu.a = self.cpu.x;
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                }
                opcodes::TAY => {
                    // Transfer Accumulator to Index Y
                    self.cpu.y = self.cpu.a;
                    self.cpu.check_negative(self.cpu.y);
                    self.cpu.check_zero(self.cpu.y);
                }
                opcodes::TYA => {
                    // Transfer Index Y to Accumulator
                    self.cpu.a = self.cpu.y;
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                }
                opcodes::TSX => {
                    // Transfer Stack Pointer to Index X
                    self.cpu.x = self.cpu.sp;
                    self.cpu.check_negative(self.cpu.x);
                    self.cpu.check_zero(self.cpu.x);
                }
                opcodes::TXS => {
                    // Transfer Index X to Stack Pointer
                    self.cpu.sp = self.cpu.x;
                    self.cpu.check_negative(self.cpu.sp);
                    self.cpu.check_zero(self.cpu.sp);
                }
                _ => panic!("Unkown opcode: 0x{:x}", opcode),
            }

            self.log(opcode, logdata);

            let size: u16 = self.lookup.size(opcode);
            if size == 0 {
                panic!(
                    "Opcode 0x{:x} missing from lookup table, see opcode.rs",
                    opcode
                );
            }
            self.cpu.pc += size;
        }
    }

    fn log(&self, opcode: u8, logdata: Vec<u16>) {
        let mut logline = String::with_capacity(80);

        logline.push_str(&format!(
            "0x{:x}: {} (0x{:x})",
            logdata[0],
            self.lookup.name(opcode),
            opcode
        ));

        if logdata.len() > 1 {
            logline.push_str(&format!(" arg=0x{:x}\t", logdata[1]));
        } else {
            logline.push_str(" \t\t");
        }

        logline.push_str(&format!(
            "\ta=0x{:x} x=0x{:x} y=0x{:x} sp=0x{:x}",
            self.cpu.a, self.cpu.x, self.cpu.y, self.cpu.sp
        ));
        logline.push_str(&format!(
            "\tN={} V={} Z={} C={}",
            self.cpu.negative_flag() as i32,
            self.cpu.overflow_flag() as i32,
            self.cpu.zero_flag() as i32,
            self.cpu.carry_flag() as i32
        ));

        println!("{}", logline);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: add basic unittests for all instructions
    // could clean up the integration test inputs, then

    #[test]
    fn test_install_rom() {
        let mut emu: Emulator = Emulator::new();
        let start: usize = memory::CODE_START_ADDR as usize;

        let mut code: Vec<u8> = Vec::<u8>::new();
        code.push(0x47);
        code.push(0x11);
        emu.install_rom(code);

        assert_eq!(emu.mem.ram[start], 0x47);
        assert_eq!(emu.mem.ram[start + 1], 0x11);
    }

    #[test]
    fn test_lda_abs() {
        let mut emu: Emulator = Emulator::new();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::LDA_ABS;
        emu.mem.ram[start + 1] = 0x11;
        emu.mem.ram[start + 2] = 0x47;
        emu.mem.ram[0x4711] = 0x42;
        emu.run();

        assert_eq!(emu.cpu.a, 0x042);
    }

    #[test]
    fn test_lda_abx() {
        let mut emu: Emulator = Emulator::new();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.x = 1;
        emu.mem.ram[start] = opcodes::LDA_ABX;
        emu.mem.ram[start + 1] = 0x10;
        emu.mem.ram[start + 2] = 0x47;
        emu.mem.ram[0x4711] = 0x42;
        emu.run();

        assert_eq!(emu.cpu.a, 0x042);
    }

    #[test]
    fn test_lda_aby() {
        let mut emu: Emulator = Emulator::new();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.y = 1;
        emu.mem.ram[start] = opcodes::LDA_ABY;
        emu.mem.ram[start + 1] = 0x10;
        emu.mem.ram[start + 2] = 0x47;
        emu.mem.ram[0x4711] = 0x42;
        emu.run();

        assert_eq!(emu.cpu.a, 0x42);
    }

    #[test]
    fn test_lda_imm() {
        let mut emu: Emulator = Emulator::new();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::LDA_IMM;
        emu.mem.ram[start + 1] = 0x42;
        emu.run();

        assert_eq!(emu.cpu.a, 0x42);
    }

    #[test]
    fn test_lda_inx() {
        let mut emu: Emulator = Emulator::new();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.x = 1;
        emu.mem.ram[start] = opcodes::LDA_INX;
        emu.mem.ram[start + 1] = 0x41;
        emu.mem.ram[0x42] = 0x15;
        emu.mem.ram[0x15] = 0x12;
        emu.run();

        assert_eq!(emu.cpu.a, 0x12);
    }

    #[test]
    fn test_lda_iny() {
        let mut emu: Emulator = Emulator::new();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.y = 1;
        emu.mem.ram[start] = opcodes::LDA_INY;
        emu.mem.ram[start + 1] = 0x41;
        emu.mem.ram[0x42] = 0x15;
        emu.run();

        assert_eq!(emu.cpu.a, 0x15);
    }

    #[test]
    fn test_lda_zp() {
        let mut emu: Emulator = Emulator::new();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::LDA_ZP;
        emu.mem.ram[start + 1] = 0x42;
        emu.mem.ram[0x42] = 0x15;
        emu.run();

        assert_eq!(emu.cpu.a, 0x15);
    }

    #[test]
    fn test_lda_zpx() {
        let mut emu: Emulator = Emulator::new();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.x = 1;
        emu.mem.ram[start] = opcodes::LDA_ZPX;
        emu.mem.ram[start + 1] = 0x41;
        emu.mem.ram[0x42] = 0x15;
        emu.run();

        assert_eq!(emu.cpu.a, 0x15);
    }

    #[test]
    fn test_pha() {
        let mut emu: Emulator = Emulator::new();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.a = 0x42;
        emu.mem.ram[start] = opcodes::PHA;
        emu.run();

        assert_eq!(emu.mem.ram[0x1ff as usize], 0x42);
        assert_eq!(emu.cpu.sp, 0xfe);
    }

    #[test]
    fn test_pla() {
        let mut emu: Emulator = Emulator::new();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.sp -= 1;
        emu.mem.ram[0x1ff] = 0x42;
        emu.mem.ram[start] = opcodes::PLA;
        emu.run();

        assert_eq!(emu.cpu.a, 0x42);
        assert_eq!(emu.cpu.sp, 0xff);
    }

    #[test]
    fn test_php() {
        let mut emu: Emulator = Emulator::new();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.status = 0x1;
        emu.mem.ram[start] = opcodes::PHP;
        emu.run();

        assert_eq!(emu.mem.ram[0x1ff as usize], 0x1);
        assert_eq!(emu.cpu.sp, 0xfe);
    }

    #[test]
    fn test_plp() {
        let mut emu: Emulator = Emulator::new();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.sp -= 1;
        emu.mem.ram[0x1ff] = 0x1;
        emu.mem.ram[start] = opcodes::PLP;
        emu.run();

        assert_eq!(emu.cpu.status, 0x1);
        assert_eq!(emu.cpu.sp, 0xff);
    }

    #[test]
    fn test_sta_aby() {
        let mut emu: Emulator = Emulator::new();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.a = 0x42;
        emu.cpu.y = 1;
        emu.mem.ram[start] = opcodes::STA_ABY;
        emu.mem.ram[start + 1] = 0x10;
        emu.mem.ram[start + 2] = 0x47;
        emu.run();

        assert_eq!(emu.mem.ram[0x4711], 0x42);
    }
}
