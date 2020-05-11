pub mod cpu;
pub mod memory;
pub mod opcodes;

use pancurses;
use rand::Rng;
use std::{thread, time};

trait IOHandler {
    fn init(&mut self);
    fn log(&self, logline: &str);
    fn display(&self, mem: &memory::Memory);
    fn input(&mut self, mem: &mut memory::Memory);
    fn exit(&self, s: &str);
}

pub struct HeadlessIOHandler {}

impl IOHandler for HeadlessIOHandler {
    fn init(&mut self) {}

    fn log(&self, logline: &str) {
        println!("{}", logline);
    }

    #[allow(unused_variables)]
    fn input(&mut self, mem: &mut memory::Memory) {
        // TOOO
    }

    #[allow(unused_variables)]
    fn display(&self, mem: &memory::Memory) {}

    fn exit(&self, s: &str) {
        println!("{}", s);
    }
}

pub struct CursesIOHandler {
    window: pancurses::Window,
}

impl CursesIOHandler {
    fn new() -> CursesIOHandler {
        CursesIOHandler {
            window: pancurses::initscr(),
        }
    }
}

impl IOHandler for CursesIOHandler {
    fn init(&mut self) {
        self.window.timeout(0);
    }

    fn log(&self, logline: &str) {
        self.window.mvaddstr(0, 0, logline);
    }

    fn input(&mut self, mem: &mut memory::Memory) {
        // TODO: Map more input
        match self.window.getch() {
            Some(pancurses::Input::Character(c)) => {
                mem.ram[0xff] = c.to_ascii_lowercase() as u8;
            }
            _ => {}
        }
        self.window.refresh();
    }

    fn display(&self, mem: &memory::Memory) {
        // Display is stored as a 32x32 screen from 0x200 and onwards
        let base: u16 = 0x0200;
        let offset: i32 = 3;
        self.window.attron(pancurses::A_REVERSE);

        // Apple location is stored at 0x0100
        let apple_addr: u16 = mem.get_16b_addr(0x00);
        for y in 0..31 {
            for x in 0..31 {
                let addr: u16 = base + (y * 32) + x;
                let chr: char = if addr == apple_addr {
                    'O'
                } else {
                    let value: u8 = mem.ram[addr as usize];
                    if value == 1 {
                        '#'
                    } else {
                        ' '
                    }
                };
                self.window.mvaddch(offset + y as i32, x as i32, chr);
            }
        }
        self.window.attroff(pancurses::A_REVERSE);
        thread::sleep(time::Duration::from_micros(10));
    }

    fn exit(&self, s: &str) {
        self.window.mvaddstr(1, 0, s);
        pancurses::endwin();
    }
}

pub struct Emulator {
    pub cpu: cpu::Cpu,
    pub mem: memory::Memory,
    lookup: opcodes::Lookup,
    iohandler: Box<dyn IOHandler>,
    rng: rand::rngs::ThreadRng,
}

impl Emulator {
    pub fn new() -> Emulator {
        Emulator {
            cpu: cpu::Cpu::new(),
            mem: memory::Memory::new(),
            lookup: opcodes::Lookup::new(),
            iohandler: Box::new(CursesIOHandler::new()),
            rng: rand::thread_rng(),
        }
    }

    #[allow(dead_code)] // Only used by tests
    pub fn new_headless() -> Emulator {
        Emulator {
            cpu: cpu::Cpu::new(),
            mem: memory::Memory::new(),
            lookup: opcodes::Lookup::new(),
            iohandler: Box::new(HeadlessIOHandler {}),
            rng: rand::thread_rng(),
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
        let mut count: u64 = 0;

        self.iohandler.init();

        loop {
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
                    let s = format!("BRK after {} instructions", count);
                    self.iohandler.exit(&s);
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
                    let lb: u8 = ((self.cpu.pc + 2) & 0xff) as u8;
                    let hb: u8 = ((self.cpu.pc + 2) >> 8) as u8;

                    self.mem.push_to_stack(self.cpu.sp, hb);
                    self.cpu.sp = self.cpu.sp.wrapping_sub(1);
                    self.mem.push_to_stack(self.cpu.sp, lb);
                    self.cpu.sp = self.cpu.sp.wrapping_sub(1);

                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    self.cpu.pc = addr;
                }
                opcodes::RTS => {
                    // ReTurn from Subroutine
                    self.cpu.sp = self.cpu.sp.wrapping_add(1);
                    let lb: u8 = self.mem.pull_from_stack(self.cpu.sp);
                    self.cpu.sp = self.cpu.sp.wrapping_add(1);
                    let hb: u8 = self.mem.pull_from_stack(self.cpu.sp);

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
                opcodes::LDX_ZP => {
                    let addr: u16 = self.mem.value_at_addr(self.cpu.pc + 1) as u16;
                    logdata.push(addr);
                    self.cpu.x = self.mem.value_at_addr(addr);
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
                    self.cpu.check_negative(self.cpu.sp);
                    self.cpu.check_zero(self.cpu.sp);
                }
                _ => panic!("Unkown opcode: 0x{:x}", opcode),
            }

            self.log(opcode, logdata);
            self.rng();
            self.iohandler.input(&mut self.mem);
            self.iohandler.display(&self.mem);

            let size: u16 = self.lookup.size(opcode);
            if size > 3 {
                let oops = format!(
                    "Opcode 0x{:x} missing from lookup table, see opcode.rs",
                    opcode
                );
                self.iohandler.exit(&oops);
                panic!(oops);
            }
            self.cpu.pc += size;
            count = count + 1;
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
        emu.install_rom(code);

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
        assert_eq!(emu.cpu.status, 0x0);
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
        emu.cpu.status = 0x1;
        emu.mem.ram[start] = opcodes::PHP;
        emu.run();

        assert_eq!(emu.mem.ram[0x1ff as usize], 0x1);
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

        assert_eq!(emu.cpu.status, 0x1);
        assert_eq!(emu.cpu.sp, 0xff);
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
}
