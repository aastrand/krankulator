pub mod cpu;
pub mod io;
pub mod memory;
pub mod opcodes;

use rand::Rng;

pub struct Emulator {
    pub cpu: cpu::Cpu,
    pub mem: memory::Memory,
    lookup: opcodes::Lookup,
    iohandler: Box<dyn io::IOHandler>,
    rng: rand::rngs::ThreadRng,
}

impl Emulator {
    pub fn new() -> Emulator {
        Emulator {
            cpu: cpu::Cpu::new(),
            mem: memory::Memory::new(),
            lookup: opcodes::Lookup::new(),
            iohandler: Box::new(io::CursesIOHandler::new()),
            rng: rand::thread_rng(),
        }
    }

    #[allow(dead_code)] // Only used by tests
    pub fn new_headless() -> Emulator {
        Emulator {
            cpu: cpu::Cpu::new(),
            mem: memory::Memory::new(),
            lookup: opcodes::Lookup::new(),
            iohandler: Box::new(io::HeadlessIOHandler {}),
            rng: rand::thread_rng(),
        }
    }

    pub fn install_rom(&mut self, rom: Vec<u8>, offset: u16) {
        let mut i: u32 = 0;
        for code in rom.iter() {
            self.mem.ram[(offset + i as u16) as usize] = *code;
            i += 1;
        }
    }

    pub fn run(&mut self) {
        let mut count: u64 = 0;
        let mut last: u16 = 0xfff;

        self.iohandler.init();

        loop {
            if self.cpu.pc == last {
                if self.mem.value_at_addr(last) != opcodes::BRK {
                    self.exit(&format!("infite loop detected!"), count);
                    self.log_stack();
                } else {
                    self.exit("reached probable end of code", count);
                }

                break;
            }

            last = self.cpu.pc;
            let opcode = self.mem.ram[self.cpu.pc as usize];
            let mut logdata: Vec<u16> = Vec::<u16>::new();
            logdata.push(self.cpu.pc);

            match opcode {
                opcodes::AND_IMM => {
                    // Bitwise AND with accumulator
                    let operand: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.a &= operand;

                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                }
                opcodes::AND_ZPX => {
                    // Bitwise AND with accumulator
                    let addr: u16 = self
                        .mem
                        .value_at_addr(self.cpu.pc + 1)
                        .wrapping_add(self.cpu.x) as u16;
                    logdata.push(addr);
                    self.cpu.a &= self.mem.value_at_addr(addr);
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                }

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

                opcodes::BIT_ZP => {
                    // Test BITs
                    let operand: u8 = self.mem.indirect_value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);

                    // Bits 7 and 6 of operand are transfered to bit 7 and 6 of SR (N,V);
                    let mask: u8 = 0b1100_0000;
                    self.cpu.status = (self.cpu.status & !mask) | (operand & mask);
                    // The zeroflag is set to the result of operand AND accumulator.
                    self.cpu.check_zero(self.cpu.a & operand);
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
                    /*
                    In the 7 clock cycles it takes BRK to execute, the padding byte is actually
                    fetched, but the CPU does nothing with it. The diagram below will show the
                    bus operations that take place during the execution of BRK:

                    cc	addr	data
                    --	----	----
                    1	PC	00	;BRK opcode
                    2	PC+1	??	;the padding byte, ignored by the CPU
                    3	S	PCH	;high byte of PC
                    4	S-1	PCL	;low byte of PC
                    5	S-2	P	;status flags with B flag set
                    6	FFFE	??	;low byte of target address
                    7	FFFF	??	;high byte of target address
                    */
                    let addr: u16 = self.mem.get_16b_addr(memory::BRK_TARGET_ADDR);

                    // BRK without a target gets ignored
                    if addr > 0 {
                        self.push_pc_to_stack(2);
                        logdata.push(addr);
                        // software instructions BRK & PHP will push the B flag as being 1.
                        // hardware interrupts IRQ & NMI will push the B flag as being 0.
                        self.push_to_stack(self.cpu.status | cpu::BREAK_BIT);

                        // we set the I flag
                        self.cpu.set_status_flag(cpu::INTERRUPT_BIT);

                        self.cpu.pc = addr;
                    }
                }
                opcodes::CLC => {
                    self.cpu.clear_status_flag(cpu::CARRY_BIT);
                }
                opcodes::CLV => {
                    self.cpu.clear_status_flag(cpu::OVERFLOW_BIT);
                }
                opcodes::CLD => {
                    self.cpu.clear_status_flag(cpu::DECIMAL_BIT);
                }
                opcodes::CLI => {
                    self.cpu.clear_status_flag(cpu::INTERRUPT_BIT);
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

                opcodes::DEC_ZP => {
                    // DECrement memory
                    let addr: u16 = self.cpu.pc + 1;
                    let operand: u8 = self.mem.indirect_value_at_addr(addr);
                    logdata.push(operand as u16);
                    let value: u8 = operand.wrapping_sub(1);
                    self.mem.store_indirect(addr, value);

                    self.cpu.check_negative(value);
                    self.cpu.check_zero(value);
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

                /*
                TODO:
                pub const EOR_ZP: u8 = 0x45;
                pub const EOR_ZPX: u8 = 0x55;
                pub const EOR_ABS: u8 = 0x4d;
                pub const EOR_ABX: u8 = 0x5d;
                pub const EOR_ABY: u8 = 0x59;
                pub const EOR_INX: u8 = 0x41;
                pub const EOR_INY: u8 = 0x51;*/
                opcodes::EOR_IMM => {
                    // bitwise Exclusive OR
                    let operand: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.a = self.cpu.a ^ operand;
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                }

                opcodes::JMP_ABS => {
                    // JuMP to address
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    self.cpu.pc = addr;
                }
                opcodes::JMP_IND => {
                    // JuMP to address stored in arg
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    let operand: u16 = self.mem.get_16b_addr(addr);
                    logdata.push(operand);
                    self.cpu.pc = operand;
                }

                opcodes::JSR => {
                    // Jump to SubRoutine
                    self.push_pc_to_stack(2);
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    self.cpu.pc = addr;
                }
                opcodes::RTS => {
                    // ReTurn from Subroutine
                    let lb: u8 = self.pull_from_stack();
                    let hb: u8 = self.pull_from_stack();

                    let addr: u16 = ((hb as u16) << 8) + ((lb as u16) & 0xff) + 1;
                    logdata.push(addr);

                    self.cpu.pc = addr;
                }

                opcodes::INC_ZP => {
                    // INCrement memory
                    let addr: u16 = self.cpu.pc + 1;
                    let operand: u8 = self.mem.indirect_value_at_addr(addr);
                    logdata.push(operand as u16);
                    let value: u8 = operand.wrapping_add(1);
                    self.mem.store_indirect(addr, value);

                    self.cpu.check_negative(value);
                    self.cpu.check_zero(value);
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
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                }
                opcodes::LDA_ABX => {
                    let addr: u16 = self
                        .mem
                        .get_16b_addr(self.cpu.pc + 1)
                        .wrapping_add(self.cpu.x as u16);
                    logdata.push(addr);
                    self.cpu.a = self.mem.value_at_addr(addr);
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                }
                opcodes::LDA_ABY => {
                    let addr: u16 = self
                        .mem
                        .get_16b_addr(self.cpu.pc + 1)
                        .wrapping_add(self.cpu.y as u16);
                    logdata.push(addr);
                    self.cpu.a = self.mem.value_at_addr(addr);
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                }
                opcodes::LDA_IMM => {
                    let value: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    logdata.push(value as u16);
                    self.cpu.a = value;
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                }
                opcodes::LDA_INX => {
                    let value: u8 = self
                        .mem
                        .value_at_addr(self.cpu.pc + 1)
                        .wrapping_add(self.cpu.x);
                    let addr: u16 = self.mem.get_16b_addr(value as u16);
                    logdata.push(addr);
                    self.cpu.a = self.mem.value_at_addr(addr);
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                }
                opcodes::LDA_INY => {
                    let value: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    let addr: u16 = value.wrapping_add(self.cpu.y) as u16;
                    logdata.push(addr);
                    self.cpu.a = self.mem.value_at_addr(addr);
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                }
                opcodes::LDA_ZP => {
                    let addr: u16 = self.mem.value_at_addr(self.cpu.pc + 1) as u16;
                    logdata.push(addr);
                    self.cpu.a = self.mem.value_at_addr(addr);
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                }
                opcodes::LDA_ZPX => {
                    let addr: u16 = self
                        .mem
                        .value_at_addr(self.cpu.pc + 1)
                        .wrapping_add(self.cpu.x) as u16;
                    logdata.push(addr);
                    self.cpu.a = self.mem.value_at_addr(addr);
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                }

                opcodes::LDX_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    self.cpu.x = self.mem.value_at_addr(addr);
                    self.cpu.check_negative(self.cpu.x);
                    self.cpu.check_zero(self.cpu.x);
                }
                opcodes::LDX_IMM => {
                    let value: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    logdata.push(value as u16);
                    self.cpu.x = value;
                    self.cpu.check_negative(self.cpu.x);
                    self.cpu.check_zero(self.cpu.x);
                }
                opcodes::LDX_ZP => {
                    let addr: u16 = self.mem.value_at_addr(self.cpu.pc + 1) as u16;
                    logdata.push(addr);
                    self.cpu.x = self.mem.value_at_addr(addr);
                    self.cpu.check_negative(self.cpu.x);
                    self.cpu.check_zero(self.cpu.x);
                }
                opcodes::LDX_ZPY => {
                    let addr: u16 = self
                        .mem
                        .value_at_addr(self.cpu.pc + 1)
                        .wrapping_add(self.cpu.y) as u16;
                    logdata.push(addr);
                    self.cpu.x = self.mem.value_at_addr(addr);
                    self.cpu.check_negative(self.cpu.x);
                    self.cpu.check_zero(self.cpu.x);
                }
                opcodes::LDY_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    self.cpu.y = self.mem.value_at_addr(addr);
                    self.cpu.check_negative(self.cpu.y);
                    self.cpu.check_zero(self.cpu.y);
                }
                opcodes::LDY_IMM => {
                    let value: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    logdata.push(value as u16);
                    self.cpu.y = value;
                    self.cpu.check_negative(self.cpu.y);
                    self.cpu.check_zero(self.cpu.y);
                }

                opcodes::LSR => {
                    // Logical Shift Right
                    // LSR shifts all bits right one position.
                    // 0 is shifted into bit 7 and the original bit 0 is shifted into the Carry.
                    let b0: u8 = self.cpu.a & 0b0000_0001;
                    self.cpu.a = self.cpu.a >> 1;

                    if b0 == 1 {
                        self.cpu.set_status_flag(cpu::CARRY_BIT);
                    } else {
                        self.cpu.clear_status_flag(cpu::CARRY_BIT);
                    }

                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                }

                opcodes::NOP => {
                    // No operation
                }

                opcodes::ORA_IMM => {
                    // Bitwise OR with Accumulator
                    let operand: u8 = self.mem.value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.a |= operand;

                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                }

                opcodes::PHA => {
                    // PusH Accumulator
                    self.push_to_stack(self.cpu.a);
                }
                opcodes::PLA => {
                    // PuLl Accumulator
                    if self.cpu.sp == 0xff {
                        self.cpu.set_status_flag(cpu::OVERFLOW_BIT);
                    }
                    self.cpu.a = self.pull_from_stack();
                    self.cpu.check_negative(self.cpu.a);
                    self.cpu.check_zero(self.cpu.a);
                }
                opcodes::PHP => {
                    // PusH Processor status
                    // software instructions BRK & PHP will push the B flag as being 1.
                    // hardware interrupts IRQ & NMI will push the B flag as being 0.
                    self.push_to_stack(self.cpu.status | cpu::BREAK_BIT);
                }
                opcodes::PLP => {
                    // PuLl Processor status
                    self.cpu.status = self.pull_from_stack();

                    // TODO: should we set ignore?
                    self.cpu.set_status_flag(cpu::IGNORE_BIT);
                    // when the flags are restored (via PLP or RTI), the B bit is discarded.
                    self.cpu.clear_status_flag(cpu::BREAK_BIT);
                }

                opcodes::RTI => {
                    // RTI retrieves the Processor Status Word (flags)
                    // and the Program Counter from the stack in that order
                    // (interrupts push the PC first and then the PSW).
                    self.cpu.status = self.pull_from_stack();
                    // TODO: should we set ignore?
                    self.cpu.set_status_flag(cpu::IGNORE_BIT);
                    // when the flags are restored (via PLP or RTI), the B bit is discarded.
                    self.cpu.clear_status_flag(cpu::BREAK_BIT);

                    // Note that unlike RTS, the return address on the stack
                    // is the actual address rather than the address-1.
                    let lb: u8 = self.pull_from_stack();
                    let hb: u8 = self.pull_from_stack();

                    let addr: u16 = ((hb as u16) << 8) + ((lb as u16) & 0xff);
                    logdata.push(addr);

                    self.cpu.pc = addr;
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
                opcodes::SED => {
                    self.cpu.set_status_flag(cpu::DECIMAL_BIT);
                }
                opcodes::SEI => {
                    self.cpu.set_status_flag(cpu::INTERRUPT_BIT);
                }

                opcodes::STA_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    self.mem.store(addr, self.cpu.a);
                }
                opcodes::STA_ZP => {
                    let addr: u16 = self.mem.value_at_addr(self.cpu.pc + 1).into();
                    logdata.push(addr);
                    self.mem.store(addr, self.cpu.a);
                }
                opcodes::STA_ZPX => {
                    let addr: u16 = self
                        .mem
                        .value_at_addr(self.cpu.pc + 1)
                        .wrapping_add(self.cpu.x) as u16;
                    logdata.push(addr);
                    self.mem.store(addr, self.cpu.a);
                }
                opcodes::STA_ABY => {
                    let addr: u16 = self
                        .mem
                        .get_16b_addr(self.cpu.pc + 1)
                        .wrapping_add(self.cpu.y as u16);
                    logdata.push(addr);
                    self.mem.store(addr, self.cpu.a);
                }
                opcodes::STA_INX => {
                    let value: u8 = self
                        .mem
                        .value_at_addr(self.cpu.pc + 1)
                        .wrapping_add(self.cpu.x);
                    let addr: u16 = self.mem.get_16b_addr(value as u16);
                    logdata.push(addr);
                    self.mem.store(addr, self.cpu.a);
                }
                opcodes::STA_INY => {
                    let value: u8 = self.mem.indirect_value_at_addr(self.cpu.pc + 1);
                    let addr: u16 = value.wrapping_add(self.cpu.y) as u16;
                    logdata.push(addr);
                    self.mem.store(addr, self.cpu.a);
                }

                opcodes::STX_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    self.mem.store(addr, self.cpu.x);
                }
                opcodes::STX_ZP => {
                    let addr: u16 = self.mem.value_at_addr(self.cpu.pc + 1).into();
                    logdata.push(addr);
                    self.mem.store(addr, self.cpu.x);
                }
                opcodes::STY_ABS => {
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    self.mem.store(addr, self.cpu.y);
                }
                opcodes::STY_ZP => {
                    let addr: u16 = self.mem.value_at_addr(self.cpu.pc + 1).into();
                    logdata.push(addr);
                    self.mem.store(addr, self.cpu.y);
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
                    // TSX sets NZ - TXS does not
                }
                _ => {
                    self.exit(&format!("unkown opcode: 0x{:x}", opcode), count);
                    break;
                }
            }

            let size: u16 = self.lookup.size(opcode);
            if size > 3 {
                let oops = format!(
                    "opcode 0x{:x} missing from lookup table, see opcode.rs",
                    opcode
                );
                self.iohandler.exit(&oops);
                break;
            }

            self.log(opcode, logdata);
            self.rng();
            self.iohandler.input(&mut self.mem);
            self.iohandler.display(&self.mem);

            self.cpu.pc += size;
            count = count + 1;
        }
    }

    fn exit(&mut self, reason: &str, count: u64) {
        self.iohandler.log(&"");
        self.iohandler.log(reason);

        let s = format!(
            "\nexited after {} instructions at cpu.pc=0x{:x}",
            count, self.cpu.pc
        );
        self.iohandler.exit(&s);
    }

    fn log_stack(&self) {
        let mut addr: u16 = 0x1ff;
        self.iohandler.log(&format!("stack contents:"));
        let mut buf = String::with_capacity(100);
        let mut cols = 0;

        loop {
            if addr == self.mem.stack_addr(self.cpu.sp) {
                self.iohandler.log(&buf);
                break;
            }
            buf.push_str(&format!(
                "0x{:x} = 0x{:x} \t",
                addr,
                self.mem.value_at_addr(addr)
            ));
            cols += 1;
            addr = addr.wrapping_sub(1);

            if cols > 8 {
                self.iohandler.log(&buf);
                buf = String::with_capacity(100);
                cols = 0;
            }
        }
    }

    fn push_pc_to_stack(&mut self, offset: u16) {
        let lb: u8 = ((self.cpu.pc + offset) & 0xff) as u8;
        let hb: u8 = ((self.cpu.pc + offset) >> 8) as u8;

        self.mem.push_to_stack(self.cpu.sp, hb);
        self.cpu.sp = self.cpu.sp.wrapping_sub(1);
        self.mem.push_to_stack(self.cpu.sp, lb);
        self.cpu.sp = self.cpu.sp.wrapping_sub(1);
    }

    fn pull_from_stack(&mut self) -> u8 {
        self.cpu.sp = self.cpu.sp.wrapping_add(1);
        self.mem.pull_from_stack(self.cpu.sp)
    }

    fn push_to_stack(&mut self, value: u8) {
        self.mem.push_to_stack(self.cpu.sp, value);
        self.cpu.sp = self.cpu.sp.wrapping_sub(1);
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
            "\tN={} V={} Z={} C={} st={:#010b} (0x{:x})",
            self.cpu.negative_flag() as i32,
            self.cpu.overflow_flag() as i32,
            self.cpu.zero_flag() as i32,
            self.cpu.carry_flag() as i32,
            self.cpu.status,
            self.cpu.status
        ));

        self.iohandler.log(&logline);
    }

    fn rng(&mut self) {
        self.mem.ram[0xfe] = self.rng.gen();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: add basic unittests for all instructions
    // could clean up the integration test inputs, then

    #[test]
    fn test_install_rom() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;

        let mut code: Vec<u8> = Vec::<u8>::new();
        code.push(0x47);
        code.push(0x11);
        emu.install_rom(code, memory::CODE_START_ADDR);

        assert_eq!(emu.mem.ram[start], 0x47);
        assert_eq!(emu.mem.ram[start + 1], 0x11);
    }

    #[test]
    fn test_and_imm() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::AND_IMM;
        emu.mem.ram[start + 1] = 0b1000_0000;
        emu.cpu.a = 0b1000_0001;
        emu.run();
        assert_eq!(emu.cpu.a, 128);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.ram[start] = opcodes::AND_IMM;
        emu.mem.ram[start + 1] = 0;
        emu.run();
        assert_eq!(emu.cpu.a, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_and_zpx() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::AND_ZPX;
        emu.mem.ram[start + 1] = 0x01;
        emu.mem.ram[0x01] = 0x01;
        emu.cpu.x = 0x01;
        emu.mem.ram[0x02] = 0b1000_0000;
        emu.cpu.a = 0b1000_0001;
        emu.run();
        assert_eq!(emu.cpu.a, 128);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.ram[start] = opcodes::AND_IMM;
        emu.mem.ram[0x02] = 0;
        emu.run();
        assert_eq!(emu.cpu.a, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_bit_zp() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::BIT_ZP;
        emu.mem.ram[start + 1] = 0x01;
        emu.mem.ram[0x01] = 0b1100_0000;
        emu.cpu.a = 0b1000_0001;
        emu.run();

        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.overflow_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.ram[0x01] = 0b0100_0000;
        emu.cpu.a = 0b1000_0001;
        emu.run();

        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.overflow_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_brk() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::BRK;
        emu.run();

        assert_eq!(emu.cpu.interrupt_flag(), false);
        assert_eq!(emu.cpu.pc, 0x600);

        emu.mem.ram[0xffff] = 0x47;
        emu.mem.ram[0xfffe] = 0x11;
        emu.cpu.set_status_flag(cpu::NEGATIVE_BIT);
        emu.run();

        assert_eq!(emu.cpu.interrupt_flag(), true);
        assert_eq!(emu.cpu.pc, 0x4711);
        assert_eq!(emu.mem.ram[0x1ff], 0x6);
        assert_eq!(emu.mem.ram[0x1fe], 0x2);
        assert_eq!(emu.mem.ram[0x1fd], 0b1011_0000);
    }

    #[test]
    fn test_clc() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::CLC;
        emu.cpu.set_status_flag(cpu::CARRY_BIT);
        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
    }

    #[test]
    fn test_cld() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::CLD;
        emu.cpu.set_status_flag(cpu::DECIMAL_BIT);
        emu.run();

        assert_eq!(emu.cpu.decimal_flag(), false);
    }

    #[test]
    fn test_cli() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::CLI;
        emu.cpu.set_status_flag(cpu::INTERRUPT_BIT);
        emu.run();

        assert_eq!(emu.cpu.interrupt_flag(), false);
    }

    #[test]
    fn test_clv() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::CLV;
        emu.cpu.set_status_flag(cpu::OVERFLOW_BIT);
        emu.run();

        assert_eq!(emu.cpu.overflow_flag(), false);
    }

    #[test]
    fn test_dec_zp() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::DEC_ZP;
        emu.mem.ram[start + 1] = 0x01;
        emu.mem.ram[0x01] = 0x00;
        emu.run();

        assert_eq!(emu.mem.ram[0x01], 0xff);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.ram[0x01] = 1;
        emu.run();

        assert_eq!(emu.mem.ram[0x01], 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_eor_imm() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::EOR_IMM;
        emu.mem.ram[start + 1] = 0b1000_1000;
        emu.cpu.a = 0b0000_1000;
        emu.run();

        assert_eq!(emu.cpu.a, 0b1000_0000);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_inc_zp() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::INC_ZP;
        emu.mem.ram[start + 1] = 0x01;
        emu.mem.ram[0x01] = 0xff;
        emu.run();

        assert_eq!(emu.mem.ram[0x01], 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.ram[0x01] = 127;
        emu.run();

        assert_eq!(emu.mem.ram[0x01], 128);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_jmp_abs() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::JMP_ABS;
        emu.mem.ram[start + 1] = 0x11;
        emu.mem.ram[start + 2] = 0x47;
        emu.run();

        assert_eq!(emu.cpu.pc, 0x4711);
    }

    #[test]
    fn test_jmp_ind() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::JMP_IND;
        emu.mem.ram[start + 1] = 0x11;
        emu.mem.ram[start + 2] = 0x47;
        emu.mem.ram[0x4711] = 0x42;

        emu.run();

        assert_eq!(emu.cpu.pc, 0x42);
    }

    #[test]
    fn test_jsr() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::JSR;
        emu.mem.ram[start + 1] = 0x11;
        emu.mem.ram[start + 2] = 0x47;

        emu.run();

        assert_eq!(emu.cpu.pc, 0x4711);
        assert_eq!(emu.cpu.sp, 0xfd);
        assert_eq!(emu.mem.ram[0x1ff], (memory::CODE_START_ADDR >> 8) as u8);
        assert_eq!(emu.mem.ram[0x1fe], 0x02);
    }

    #[test]
    fn test_lda_abs() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::LDA_ABS;
        emu.mem.ram[start + 1] = 0x11;
        emu.mem.ram[start + 2] = 0x47;
        emu.mem.ram[0x4711] = 0x42;
        emu.run();

        assert_eq!(emu.cpu.a, 0x042);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_lda_abs_flags() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::LDA_ABS;
        emu.mem.ram[start + 1] = 0x11;
        emu.mem.ram[start + 2] = 0x47;
        emu.mem.ram[0x4711] = 255;
        emu.run();

        assert_eq!(emu.cpu.a, 255);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.ram[0x4711] = 0;
        emu.run();

        assert_eq!(emu.cpu.a, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_lda_abx() {
        let mut emu: Emulator = Emulator::new_headless();
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
        let mut emu: Emulator = Emulator::new_headless();
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
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::LDA_IMM;
        emu.mem.ram[start + 1] = 0x42;
        emu.run();

        assert_eq!(emu.cpu.a, 0x42);
    }

    #[test]
    fn test_lda_inx() {
        let mut emu: Emulator = Emulator::new_headless();
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
        let mut emu: Emulator = Emulator::new_headless();
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
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::LDA_ZP;
        emu.mem.ram[start + 1] = 0x42;
        emu.mem.ram[0x42] = 0x15;
        emu.run();

        assert_eq!(emu.cpu.a, 0x15);
    }

    #[test]
    fn test_lda_zpx() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.x = 1;
        emu.mem.ram[start] = opcodes::LDA_ZPX;
        emu.mem.ram[start + 1] = 0x41;
        emu.mem.ram[0x42] = 0x15;
        emu.run();

        assert_eq!(emu.cpu.a, 0x15);
    }

    #[test]
    fn test_ldx_zp() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.x = 1;
        emu.mem.ram[start] = opcodes::LDX_ZP;
        emu.mem.ram[start + 1] = 0x41;
        emu.mem.ram[0x41] = 0x15;
        emu.run();

        assert_eq!(emu.cpu.x, 0x15);
    }

    #[test]
    fn test_ldx_zp_flags() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.x = 1;
        emu.mem.ram[start] = opcodes::LDX_ZP;
        emu.mem.ram[start + 1] = 0x41;
        emu.mem.ram[0x41] = 255;
        emu.run();

        assert_eq!(emu.cpu.x, 255);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.ram[0x41] = 0;
        emu.run();

        assert_eq!(emu.cpu.x, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_ldx_zpy() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.x = 1;
        emu.cpu.y = 8;
        emu.mem.ram[start] = opcodes::LDX_ZPY;
        emu.mem.ram[start + 1] = 0x41;
        emu.mem.ram[0x41 + 8] = 0x15;
        emu.run();

        assert_eq!(emu.cpu.x, 0x15);
    }

    #[test]
    fn test_lsr() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.a = 3;
        emu.mem.ram[start] = opcodes::LSR;
        emu.run();

        assert_eq!(emu.cpu.a, 0x1);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);
        assert_eq!(emu.cpu.carry_flag(), true);

        emu.cpu.a = 0;
        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.run();

        assert_eq!(emu.cpu.a, 0x0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
        assert_eq!(emu.cpu.carry_flag(), false);
    }

    #[test]
    fn test_nop() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::NOP;

        emu.run();

        assert_eq!(emu.cpu.pc, start as u16 + 1);
        assert_eq!(emu.cpu.a, 0x0);
        assert_eq!(emu.cpu.x, 0x0);
        assert_eq!(emu.cpu.y, 0x0);
        assert_eq!(emu.cpu.sp, 0xff);
        assert_eq!(emu.cpu.status, 0b0010_0000);
    }

    #[test]
    fn test_ora_imm() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.a = 0b1000_0000;
        emu.mem.ram[start] = opcodes::ORA_IMM;
        emu.mem.ram[start + 1] = 0b0000_0001;

        emu.run();
        assert_eq!(emu.cpu.a, 129);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.cpu.a = 0;
        emu.mem.ram[start] = opcodes::ORA_IMM;
        emu.mem.ram[start + 1] = 0;

        emu.run();
        assert_eq!(emu.cpu.a, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_pha() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.a = 0x42;
        emu.mem.ram[start] = opcodes::PHA;
        emu.run();

        assert_eq!(emu.mem.ram[0x1ff as usize], 0x42);
        assert_eq!(emu.cpu.sp, 0xfe);
    }

    #[test]
    fn test_pla() {
        let mut emu: Emulator = Emulator::new_headless();
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
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.set_status_flag(cpu::CARRY_BIT);
        emu.mem.ram[start] = opcodes::PHP;
        emu.run();

        assert_eq!(emu.mem.ram[0x1ff as usize], 0b0011_0001);
        assert_eq!(emu.cpu.sp, 0xfe);
    }

    #[test]
    fn test_plp() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.sp -= 1;
        emu.mem.ram[0x1ff] = 0x1;
        emu.mem.ram[start] = opcodes::PLP;
        emu.run();

        assert_eq!(emu.cpu.status, 0b0010_0001);
        assert_eq!(emu.cpu.sp, 0xff);
    }

    #[test]
    fn test_plp_overflow() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::PLP;
        emu.mem.ram[0x100] = 0x2;

        emu.run();

        assert_eq!(emu.cpu.status, 0b0010_0010);
        assert_eq!(emu.cpu.sp, 0x0);
        assert_eq!(emu.cpu.overflow_flag(), false);
    }

    #[test]
    fn test_rti() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::RTI;
        emu.mem.ram[0x1ff] = 0x6;
        emu.mem.ram[0x1fe] = 0x8;
        emu.mem.ram[0x1fd] = 0b1011_0000;
        emu.cpu.set_status_flag(cpu::INTERRUPT_BIT);
        emu.cpu.sp = 0xfc;
        emu.run();

        assert_eq!(emu.cpu.status, 0b1010_0000);
        assert_eq!(emu.cpu.pc, 0x608);
    }

    #[test]
    fn test_rts() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::RTS;
        emu.cpu.sp = 0xfd;
        emu.mem.ram[0x1ff] = (memory::CODE_START_ADDR >> 8) as u8;
        emu.mem.ram[0x1fe] = 0x01;

        emu.run();
        assert_eq!(emu.cpu.pc, 0x0602);
        assert_eq!(emu.cpu.sp, 0xff);
    }

    #[test]
    fn test_sta_aby() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.a = 0x42;
        emu.cpu.y = 1;
        emu.mem.ram[start] = opcodes::STA_ABY;
        emu.mem.ram[start + 1] = 0x10;
        emu.mem.ram[start + 2] = 0x47;
        emu.run();

        assert_eq!(emu.mem.ram[0x4711], 0x42);
    }

    #[test]
    fn test_sta_inx() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.a = 0x42;
        emu.cpu.x = 1;
        emu.mem.ram[start] = opcodes::STA_INX;
        emu.mem.ram[start + 1] = 0x10;
        emu.mem.ram[0x11] = 0x42;

        emu.run();

        assert_eq!(emu.mem.ram[0x42], 0x42);
    }

    #[test]
    fn test_sta_iny() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.a = 0x42;
        emu.cpu.y = 1;
        emu.mem.ram[start] = opcodes::STA_INY;
        emu.mem.ram[start + 1] = 0x10;
        emu.mem.ram[0x10] = 0x41;

        emu.run();

        assert_eq!(emu.mem.ram[0x42], 0x42);
    }

    #[test]
    fn test_sta_zpx() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.x = 0x01;
        emu.cpu.a = 0x42;
        emu.mem.ram[start] = opcodes::STA_ZPX;
        emu.mem.ram[start + 1] = 0x10;
        emu.run();

        assert_eq!(emu.mem.ram[0x11], 0x42);
    }

    #[test]
    fn test_sec() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::SEC;
        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
    }

    #[test]
    fn test_sed() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::SED;
        emu.run();

        assert_eq!(emu.cpu.decimal_flag(), true);
    }

    #[test]
    fn test_sei() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.mem.ram[start] = opcodes::SEI;
        emu.run();

        assert_eq!(emu.cpu.interrupt_flag(), true);
    }

    #[test]
    fn test_tsx() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.sp = 255;
        emu.mem.ram[start] = opcodes::TSX;
        emu.run();

        assert_eq!(emu.cpu.x, 255);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.sp = 0;
        emu.mem.ram[start] = opcodes::TSX;
        emu.run();

        assert_eq!(emu.cpu.x, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_txs() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: usize = memory::CODE_START_ADDR as usize;
        emu.cpu.x = 255;
        emu.mem.ram[start] = opcodes::TXS;
        emu.run();

        assert_eq!(emu.cpu.sp, 255);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);
        emu.cpu.x = 0;
        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.run();

        assert_eq!(emu.cpu.sp, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);
    }
}
