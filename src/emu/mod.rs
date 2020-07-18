pub mod cpu;
pub mod dbg;
pub mod io;
pub mod memory;

use cpu::opcodes;
use memory::mapper;

extern crate shrust;
use std::collections::HashSet;
use std::time::SystemTime;

pub struct Emulator {
    pub cpu: cpu::Cpu,
    pub mem: memory::Memory,
    lookup: Box<opcodes::Lookup>,
    iohandler: Box<dyn io::IOHandler>,
    logformatter: io::log::LogFormatter,
    stepping: bool,
    pub breakpoints: Box<HashSet<u16>>,
    should_log: bool,
    should_debug_on_infinite_loop: bool,
    verbose: bool,
    instruction_count: u64,
    cycle_count: u64,
}

impl Emulator {
    pub fn new() -> Emulator {
        Emulator::new_base(Box::new(io::CursesIOHandler::new()))
    }

    #[allow(dead_code)] // Only used by tests
    pub fn new_headless() -> Emulator {
        Emulator::new_base(Box::new(io::HeadlessIOHandler {}))
    }

    fn new_base(iohandler: Box<dyn io::IOHandler>) -> Emulator {
        let lookup: Box<opcodes::Lookup> = Box::new(opcodes::Lookup::new());

        Emulator {
            cpu: cpu::Cpu::new(),
            mem: memory::Memory::new(),
            lookup: lookup,
            iohandler: iohandler,
            logformatter: io::log::LogFormatter::new(30),
            stepping: false,
            breakpoints: Box::new(HashSet::new()),
            should_log: true,
            should_debug_on_infinite_loop: false,
            verbose: true,
            instruction_count: 0,
            cycle_count: 0,
        }
    }

    pub fn install_mapper(&mut self, mapper: Box<dyn mapper::MemoryMapper>) {
        self.mem.install_mapper(mapper);
    }

    pub fn toggle_verbose_mode(&mut self, verbose: bool) {
        self.verbose = verbose
    }

    #[allow(dead_code)] // Only used by tests
    pub fn toggle_quiet_mode(&mut self, quiet_mode: bool) {
        self.should_log = !quiet_mode;
    }

    pub fn toggle_debug_on_infinite_loop(&mut self, debug: bool) {
        self.should_debug_on_infinite_loop = debug
    }

    pub fn run(&mut self) {
        let mut last: u16 = 0xffff;

        self.iohandler.init();
        let mut logdata: Box<Vec<u16>> = Box::new(Vec::<u16>::new());
        let start_time = SystemTime::now();

        loop {
            if self.stepping
                || (!self.breakpoints.is_empty() && self.breakpoints.contains(&self.cpu.pc))
            {
                self.debug();
            }

            if self.cpu.pc == last {
                if self.mem.read_bus(last) != opcodes::BRK {
                    self.iohandler.log(&format!(
                        "infite loop detected on addr 0x{:x}!",
                        self.cpu.pc
                    ));
                    if self.should_debug_on_infinite_loop {
                        self.debug();
                    } else {
                        break;
                    }
                } else {
                    self.exit("reached probable end of code");
                    break;
                }
            }

            last = self.cpu.pc;
            let opcode = self.mem.read_bus(self.cpu.pc);

            logdata.clear();
            logdata.push(self.cpu.pc);

            match opcode {
                opcodes::AND_ABS => {
                    // Bitwise AND with accumulator
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    logdata.push(addr);
                    self.cpu.and(self.mem.read_bus(addr));
                }
                opcodes::AND_ABX => {
                    // Bitwise AND with accumulator
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    self.cpu.and(self.mem.read_bus(addr));
                }
                opcodes::AND_ABY => {
                    // Bitwise AND with accumulator
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    self.cpu.and(self.mem.read_bus(addr));
                }
                opcodes::AND_IMM => {
                    // Bitwise AND with accumulator
                    let operand: u8 = self.mem.read_bus(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.and(operand);
                }
                opcodes::AND_INX => {
                    // Bitwise AND with accumulator
                    let addr: u16 = self.mem.addr_idx_indirect(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    self.cpu.and(self.mem.read_bus(addr));
                }
                opcodes::AND_INY => {
                    // Bitwise AND with accumulator
                    let addr: u16 = self.mem.addr_indirect_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    self.cpu.and(self.mem.read_bus(addr));
                }
                opcodes::AND_ZP => {
                    // Bitwise AND with accumulator
                    let addr: u16 = self.mem.addr_zeropage(self.cpu.pc);
                    logdata.push(addr);
                    self.cpu.and(self.mem.read_bus(addr));
                }
                opcodes::AND_ZPX => {
                    // Bitwise AND with accumulator
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    self.cpu.and(self.mem.read_bus(addr));
                }

                opcodes::ADC_ABS => {
                    // Add Memory to Accumulator with Carry
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.add_to_a_with_carry(operand);
                }
                opcodes::ADC_ABX => {
                    // Add Memory to Accumulator with Carry
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.add_to_a_with_carry(operand);
                }
                opcodes::ADC_ABY => {
                    // Add Memory to Accumulator with Carry
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.add_to_a_with_carry(operand);
                }
                opcodes::ADC_IMM => {
                    // Add Memory to Accumulator with Carry
                    let operand: u8 = self.mem.read_bus(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.add_to_a_with_carry(operand);
                }
                opcodes::ADC_INX => {
                    // Add Memory to Accumulator with Carry
                    let addr: u16 = self.mem.addr_idx_indirect(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.add_to_a_with_carry(operand);
                }
                opcodes::ADC_INY => {
                    // Add Memory to Accumulator with Carry
                    let addr: u16 = self.mem.addr_indirect_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.add_to_a_with_carry(operand);
                }
                opcodes::ADC_ZP => {
                    // Add Memory to Accumulator with Carry
                    let addr: u16 = self.mem.addr_zeropage(self.cpu.pc);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.add_to_a_with_carry(operand);
                }
                opcodes::ADC_ZPX => {
                    // Add Memory to Accumulator with Carry
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.add_to_a_with_carry(operand);
                }

                opcodes::ASL => {
                    logdata.push(self.cpu.a as u16);
                    self.cpu.a = self.cpu.asl(self.cpu.a);
                    logdata.push(self.cpu.a as u16);
                }
                opcodes::ASL_ZP => {
                    let addr: u16 = self.mem.addr_zeropage(self.cpu.pc) as u16;
                    let value: u8 = self.mem.read_bus(addr);
                    logdata.push(value as u16);
                    let result: u8 = self.cpu.asl(value);
                    logdata.push(result as u16);
                    self.mem.write_bus(addr, result);
                }
                opcodes::ASL_ZPX => {
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.x);
                    let value: u8 = self.mem.read_bus(addr);
                    logdata.push(value as u16);
                    let result: u8 = self.cpu.asl(value);
                    logdata.push(result as u16);
                    self.mem.write_bus(addr, result);
                }
                opcodes::ASL_ABS => {
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    let value: u8 = self.mem.read_bus(addr);
                    logdata.push(value as u16);
                    let result: u8 = self.cpu.asl(value);
                    logdata.push(result as u16);
                    self.mem.write_bus(addr, result);
                }
                opcodes::ASL_ABX => {
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                    let value: u8 = self.mem.read_bus(addr);
                    logdata.push(value as u16);
                    let result: u8 = self.cpu.asl(value);
                    logdata.push(result as u16);
                    self.mem.write_bus(addr, result);
                }

                opcodes::BIT_ABS => {
                    // Test BITs
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.bit(operand);
                }
                opcodes::BIT_ZP => {
                    // Test BITs
                    let operand: u8 = self.mem.indirect_value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.bit(operand);
                }

                opcodes::BPL => {
                    let operand: i8 = self.mem.read_bus(self.cpu.pc + 1) as i8;
                    logdata.push(operand as u16);
                    // Branch on PLus)
                    if !self.cpu.negative_flag() {
                        self.cpu.pc = (self.cpu.pc as i16 + operand as i16) as u16;
                        logdata.push(self.cpu.pc + 2 as u16);
                    }
                }
                opcodes::BMI => {
                    let operand: i8 = self.mem.read_bus(self.cpu.pc + 1) as i8;
                    logdata.push(operand as u16);
                    // Branch on MInus
                    if self.cpu.negative_flag() {
                        self.cpu.pc = (self.cpu.pc as i16 + operand as i16) as u16;
                        logdata.push(self.cpu.pc + 2 as u16);
                    }
                }
                opcodes::BVC => {
                    let operand: i8 = self.mem.read_bus(self.cpu.pc + 1) as i8;
                    logdata.push(operand as u16);
                    // Branch on oVerflow Clear
                    if !self.cpu.overflow_flag() {
                        self.cpu.pc = (self.cpu.pc as i16 + operand as i16) as u16;
                        logdata.push(self.cpu.pc + 2 as u16);
                    }
                }
                opcodes::BVS => {
                    let operand: i8 = self.mem.read_bus(self.cpu.pc + 1) as i8;
                    logdata.push(operand as u16);
                    // Branch on oVerflow Set
                    if self.cpu.overflow_flag() {
                        self.cpu.pc = (self.cpu.pc as i16 + operand as i16) as u16;
                        logdata.push(self.cpu.pc + 2 as u16);
                    }
                }
                opcodes::BCC => {
                    let operand: i8 = self.mem.read_bus(self.cpu.pc + 1) as i8;
                    logdata.push(operand as u16);
                    // Branch on Carry Clear
                    if !self.cpu.carry_flag() {
                        self.cpu.pc = (self.cpu.pc as i16 + operand as i16) as u16;
                        logdata.push(self.cpu.pc + 2 as u16);
                    }
                }
                opcodes::BCS => {
                    let operand: i8 = self.mem.read_bus(self.cpu.pc + 1) as i8;
                    logdata.push(operand as u16);
                    // Branch on Carry Set
                    if self.cpu.carry_flag() {
                        self.cpu.pc = (self.cpu.pc as i16 + operand as i16) as u16;
                        logdata.push(self.cpu.pc + 2 as u16);
                    }
                }
                opcodes::BEQ => {
                    let operand: i8 = self.mem.read_bus(self.cpu.pc + 1) as i8;
                    logdata.push(operand as u16);
                    // Branch on EQual
                    if self.cpu.zero_flag() {
                        self.cpu.pc = (self.cpu.pc as i16 + operand as i16) as u16;
                        logdata.push(self.cpu.pc + 2 as u16);
                    }
                }
                opcodes::BNE => {
                    let operand: i8 = self.mem.read_bus(self.cpu.pc + 1) as i8;
                    logdata.push(operand as u16);
                    // Branch on Not Equal
                    if !self.cpu.zero_flag() {
                        self.cpu.pc = (self.cpu.pc as i16 + operand as i16) as u16;
                        logdata.push(self.cpu.pc + 2 as u16);
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
                    // Compensate for length addition
                    self.cpu.pc = self.cpu.pc.wrapping_sub(self.lookup.size(opcode));
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
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.a, operand);
                }
                opcodes::CMP_ABX => {
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.a, operand);
                }
                opcodes::CMP_ABY => {
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.a, operand);
                }
                opcodes::CMP_IMM => {
                    let operand: u8 = self.mem.read_bus(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.a, operand);
                }
                opcodes::CMP_INX => {
                    let addr: u16 = self.mem.addr_idx_indirect(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);

                    self.cpu.compare(self.cpu.a, operand);
                }
                opcodes::CMP_INY => {
                    let addr: u16 = self.mem.addr_indirect_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);

                    self.cpu.compare(self.cpu.a, operand);
                }
                opcodes::CMP_ZP => {
                    let operand: u8 = self.mem.indirect_value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.a, operand);
                }
                opcodes::CMP_ZPX => {
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.a, operand);
                }
                opcodes::CPX_ABS => {
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.x, operand);
                }
                opcodes::CPX_IMM => {
                    let operand: u8 = self.mem.read_bus(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.x, operand);
                }
                opcodes::CPX_ZP => {
                    let operand: u8 = self.mem.indirect_value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.x, operand);
                }
                opcodes::CPY_ABS => {
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.y, operand);
                }
                opcodes::CPY_IMM => {
                    let operand: u8 = self.mem.read_bus(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.y, operand);
                }
                opcodes::CPY_ZP => {
                    let operand: u8 = self.mem.indirect_value_at_addr(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.compare(self.cpu.y, operand);
                }

                opcodes::DEC_ABS => {
                    // DECrement memory
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    let value: u8 = operand.wrapping_sub(1);
                    self.mem.write_bus(addr, value);

                    self.cpu.check_negative(value);
                    self.cpu.check_zero(value);
                }
                opcodes::DEC_ABX => {
                    // DECrement memory
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    let value: u8 = operand.wrapping_sub(1);
                    self.mem.write_bus(addr, value);

                    self.cpu.check_negative(value);
                    self.cpu.check_zero(value);
                }
                opcodes::DEC_ZP => {
                    // DECrement memory
                    let addr: u16 = self.mem.addr_zeropage(self.cpu.pc);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    let value: u8 = operand.wrapping_sub(1);
                    self.mem.write_bus(addr, value);

                    self.cpu.check_negative(value);
                    self.cpu.check_zero(value);
                }
                opcodes::DEC_ZPX => {
                    // DECrement memory
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.x);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    let value: u8 = operand.wrapping_sub(1);
                    self.mem.write_bus(addr, value);

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

                opcodes::EOR_ABS => {
                    // bitwise Exclusive OR
                    let addr = self.mem.addr_absolute(self.cpu.pc);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.eor(operand);
                }
                opcodes::EOR_ABX => {
                    // bitwise Exclusive OR
                    let addr = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.eor(operand);
                }
                opcodes::EOR_ABY => {
                    // bitwise Exclusive OR
                    let addr = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.y);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.eor(operand);
                }
                opcodes::EOR_IMM => {
                    // bitwise Exclusive OR
                    let operand: u8 = self.mem.read_bus(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.eor(operand);
                }
                opcodes::EOR_INX => {
                    // bitwise Exclusive OR
                    let addr = self.mem.addr_idx_indirect(self.cpu.pc, self.cpu.x);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.eor(operand);
                }
                opcodes::EOR_INY => {
                    // bitwise Exclusive OR
                    let addr = self.mem.addr_indirect_idx(self.cpu.pc, self.cpu.y);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.eor(operand);
                }
                opcodes::EOR_ZP => {
                    // bitwise Exclusive OR
                    let addr = self.mem.addr_zeropage(self.cpu.pc);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.eor(operand);
                }
                opcodes::EOR_ZPX => {
                    // bitwise Exclusive OR
                    let addr = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.x);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.eor(operand);
                }

                opcodes::JMP_ABS => {
                    // JuMP to address
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    logdata.push(addr);
                    self.cpu.pc = addr;
                    // Compensate for length addition
                    self.cpu.pc = self.cpu.pc.wrapping_sub(self.lookup.size(opcode));
                }
                opcodes::JMP_IND => {
                    // JuMP to address stored in arg
                    let addr: u16 = self.mem.get_16b_addr(self.cpu.pc + 1);
                    logdata.push(addr);
                    let operand: u16 = self.mem.get_16b_addr(addr);
                    logdata.push(operand);
                    self.cpu.pc = operand;
                    // Compensate for length addition
                    self.cpu.pc = self.cpu.pc.wrapping_sub(self.lookup.size(opcode));
                }

                opcodes::JSR_ABS => {
                    // Jump to SubRoutine
                    self.push_pc_to_stack(2);
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    logdata.push(addr);
                    self.cpu.pc = addr;
                    // Compensate for length addition
                    self.cpu.pc = self.cpu.pc.wrapping_sub(self.lookup.size(opcode));
                }
                opcodes::RTS => {
                    // ReTurn from Subroutine
                    let lb: u8 = self.pull_from_stack();
                    let hb: u8 = self.pull_from_stack();

                    let addr: u16 = ((hb as u16) << 8) + ((lb as u16) & 0xff) + 1;
                    logdata.push(addr);

                    self.cpu.pc = addr;
                    // Compensate for length addition
                    self.cpu.pc = self.cpu.pc.wrapping_sub(self.lookup.size(opcode));
                }

                opcodes::INC_ABS => {
                    // INCrement memory
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    let value: u8 = operand.wrapping_add(1);
                    self.mem.write_bus(addr, value);

                    self.cpu.check_negative(value);
                    self.cpu.check_zero(value);
                }
                opcodes::INC_ABX => {
                    // INCrement memory
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    let value: u8 = operand.wrapping_add(1);
                    self.mem.write_bus(addr, value);

                    self.cpu.check_negative(value);
                    self.cpu.check_zero(value);
                }
                opcodes::INC_ZP => {
                    // INCrement memory
                    let addr: u16 = self.mem.addr_zeropage(self.cpu.pc);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    let value: u8 = operand.wrapping_add(1);
                    self.mem.write_bus(addr, value);

                    self.cpu.check_negative(value);
                    self.cpu.check_zero(value);
                }
                opcodes::INC_ZPX => {
                    // INCrement memory
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.x);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    let value: u8 = operand.wrapping_add(1);
                    self.mem.write_bus(addr, value);

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
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    logdata.push(addr);
                    self.cpu.a = self.load(addr);
                }
                opcodes::LDA_ABX => {
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    self.cpu.a = self.load(addr);
                }
                opcodes::LDA_ABY => {
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    self.cpu.a = self.load(addr);
                }
                opcodes::LDA_IMM => {
                    self.cpu.a = self.load(self.cpu.pc + 1);
                    logdata.push(self.cpu.a as u16);
                }
                opcodes::LDA_INX => {
                    let addr: u16 = self.mem.addr_idx_indirect(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    self.cpu.a = self.load(addr);
                }
                opcodes::LDA_INY => {
                    let addr: u16 = self.mem.addr_indirect_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    self.cpu.a = self.load(addr);
                }
                opcodes::LDA_ZP => {
                    let addr: u16 = self.mem.addr_zeropage(self.cpu.pc) as u16;
                    logdata.push(addr);
                    self.cpu.a = self.load(addr);
                }
                opcodes::LDA_ZPX => {
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    self.cpu.a = self.load(addr);
                }

                opcodes::LDX_ABS => {
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    logdata.push(addr);
                    self.cpu.x = self.load(addr);
                }
                opcodes::LDX_ABY => {
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    self.cpu.x = self.load(addr);
                }
                opcodes::LDX_IMM => {
                    self.cpu.x = self.load(self.cpu.pc + 1);
                    logdata.push(self.cpu.x as u16);
                }
                opcodes::LDX_ZP => {
                    let addr: u16 = self.mem.addr_zeropage(self.cpu.pc) as u16;
                    logdata.push(addr);
                    self.cpu.x = self.load(addr);
                }
                opcodes::LDX_ZPY => {
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    self.cpu.x = self.load(addr);
                }
                opcodes::LDY_ABS => {
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    logdata.push(addr);
                    self.cpu.y = self.load(addr);
                }
                opcodes::LDY_ABX => {
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    self.cpu.y = self.load(addr);
                }
                opcodes::LDY_IMM => {
                    self.cpu.y = self.load(self.cpu.pc + 1);
                    logdata.push(self.cpu.y as u16);
                }
                opcodes::LDY_ZP => {
                    let addr: u16 = self.mem.addr_zeropage(self.cpu.pc) as u16;
                    logdata.push(addr);
                    self.cpu.y = self.load(addr);
                }
                opcodes::LDY_ZPX => {
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    self.cpu.y = self.load(addr);
                }

                opcodes::LSR => {
                    logdata.push(self.cpu.a as u16);
                    self.cpu.a = self.cpu.lsr(self.cpu.a);
                    logdata.push(self.cpu.a as u16);
                }
                opcodes::LSR_ZP => {
                    let addr: u16 = self.mem.addr_zeropage(self.cpu.pc) as u16;
                    let value: u8 = self.mem.read_bus(addr);
                    logdata.push(value as u16);
                    let result: u8 = self.cpu.lsr(value);
                    logdata.push(result as u16);
                    self.mem.write_bus(addr, result);
                }
                opcodes::LSR_ZPX => {
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.x);
                    let value: u8 = self.mem.read_bus(addr);
                    logdata.push(value as u16);
                    let result: u8 = self.cpu.lsr(value);
                    logdata.push(result as u16);
                    self.mem.write_bus(addr, result);
                }
                opcodes::LSR_ABS => {
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    let value: u8 = self.mem.read_bus(addr);
                    logdata.push(value as u16);
                    let result: u8 = self.cpu.lsr(value);
                    logdata.push(result as u16);
                    self.mem.write_bus(addr, result);
                }
                opcodes::LSR_ABX => {
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                    let value: u8 = self.mem.read_bus(addr);
                    logdata.push(value as u16);
                    let result: u8 = self.cpu.lsr(value);
                    logdata.push(result as u16);
                    self.mem.write_bus(addr, result);
                }

                opcodes::NOP => {
                    // No operation
                }

                opcodes::ORA_ABS => {
                    // Bitwise OR with Accumulator
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.ora(operand);
                }
                opcodes::ORA_ABX => {
                    // Bitwise OR with Accumulator
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.ora(operand);
                }
                opcodes::ORA_ABY => {
                    // Bitwise OR with Accumulator
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.ora(operand);
                }
                opcodes::ORA_IMM => {
                    // Bitwise OR with Accumulator
                    let operand: u8 = self.mem.read_bus(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.ora(operand);
                }
                opcodes::ORA_INX => {
                    // Bitwise OR with Accumulator
                    let addr: u16 = self.mem.addr_idx_indirect(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.ora(operand);
                }
                opcodes::ORA_INY => {
                    // Bitwise OR with Accumulator
                    let addr: u16 = self.mem.addr_indirect_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.ora(operand);
                }
                opcodes::ORA_ZP => {
                    // Bitwise OR with Accumulator
                    let addr: u16 = self.mem.addr_zeropage(self.cpu.pc);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.ora(operand);
                }
                opcodes::ORA_ZPX => {
                    // Bitwise OR with Accumulator
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.ora(operand);
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
                    // Compensate for length addition
                    self.cpu.pc = self.cpu.pc.wrapping_sub(self.lookup.size(opcode));
                }

                opcodes::ROL => {
                    logdata.push(self.cpu.a as u16);
                    self.cpu.a = self.cpu.rol(self.cpu.a);
                    logdata.push(self.cpu.a as u16);
                }
                opcodes::ROL_ZP => {
                    let addr: u16 = self.mem.addr_zeropage(self.cpu.pc) as u16;
                    let value: u8 = self.mem.read_bus(addr);
                    logdata.push(value as u16);
                    let result: u8 = self.cpu.rol(value);
                    logdata.push(result as u16);
                    self.mem.write_bus(addr, result);
                }
                opcodes::ROL_ZPX => {
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.x);
                    let value: u8 = self.mem.read_bus(addr);
                    logdata.push(value as u16);
                    let result: u8 = self.cpu.rol(value);
                    logdata.push(result as u16);
                    self.mem.write_bus(addr, result);
                }
                opcodes::ROL_ABS => {
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    let value: u8 = self.mem.read_bus(addr);
                    logdata.push(value as u16);
                    let result: u8 = self.cpu.rol(value);
                    logdata.push(result as u16);
                    self.mem.write_bus(addr, result);
                }
                opcodes::ROL_ABX => {
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                    let value: u8 = self.mem.read_bus(addr);
                    logdata.push(value as u16);
                    let result: u8 = self.cpu.rol(value);
                    logdata.push(result as u16);
                    self.mem.write_bus(addr, result);
                }

                opcodes::ROR => {
                    logdata.push(self.cpu.a as u16);
                    self.cpu.a = self.cpu.ror(self.cpu.a);
                    logdata.push(self.cpu.a as u16);
                }
                opcodes::ROR_ZP => {
                    let addr: u16 = self.mem.addr_zeropage(self.cpu.pc) as u16;
                    let value: u8 = self.mem.read_bus(addr);
                    logdata.push(value as u16);
                    let result: u8 = self.cpu.ror(value);
                    logdata.push(result as u16);
                    self.mem.write_bus(addr, result);
                }
                opcodes::ROR_ZPX => {
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.x);
                    let value: u8 = self.mem.read_bus(addr);
                    logdata.push(value as u16);
                    let result: u8 = self.cpu.ror(value);
                    logdata.push(result as u16);
                    self.mem.write_bus(addr, result);
                }
                opcodes::ROR_ABS => {
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    let value: u8 = self.mem.read_bus(addr);
                    logdata.push(value as u16);
                    let result: u8 = self.cpu.ror(value);
                    logdata.push(result as u16);
                    self.mem.write_bus(addr, result);
                }
                opcodes::ROR_ABX => {
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                    let value: u8 = self.mem.read_bus(addr);
                    logdata.push(value as u16);
                    let result: u8 = self.cpu.ror(value);
                    logdata.push(result as u16);
                    self.mem.write_bus(addr, result);
                }

                opcodes::SBC_ABS => {
                    // Subtract Memory to Accumulator with Carry
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.sub_from_a_with_carry(operand);
                }
                opcodes::SBC_ABX => {
                    // Subtract Memory to Accumulator with Carry
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.sub_from_a_with_carry(operand);
                }
                opcodes::SBC_ABY => {
                    // Subtract Memory to Accumulator with Carry
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.sub_from_a_with_carry(operand);
                }
                opcodes::SBC_IMM => {
                    // Subtract Memory to Accumulator with Carry
                    let operand: u8 = self.mem.read_bus(self.cpu.pc + 1);
                    logdata.push(operand as u16);
                    self.cpu.sub_from_a_with_carry(operand);
                }
                opcodes::SBC_INX => {
                    // Subtract Memory to Accumulator with Carry
                    let addr: u16 = self.mem.addr_idx_indirect(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.sub_from_a_with_carry(operand);
                }
                opcodes::SBC_INY => {
                    // Subtract Memory to Accumulator with Carry
                    let addr: u16 = self.mem.addr_indirect_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.sub_from_a_with_carry(operand);
                }
                opcodes::SBC_ZP => {
                    // Subtract Memory to Accumulator with Carry
                    let addr: u16 = self.mem.addr_zeropage(self.cpu.pc);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
                    logdata.push(operand as u16);
                    self.cpu.sub_from_a_with_carry(operand);
                }
                opcodes::SBC_ZPX => {
                    // Subtract Memory to Accumulator with Carry
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    let operand: u8 = self.mem.read_bus(addr);
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
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    logdata.push(addr);
                    self.mem.write_bus(addr, self.cpu.a);
                }
                opcodes::STA_ABX => {
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    self.mem.write_bus(addr, self.cpu.a);
                }
                opcodes::STA_ZP => {
                    let addr: u16 = self.mem.addr_zeropage(self.cpu.pc);
                    logdata.push(addr);
                    self.mem.write_bus(addr, self.cpu.a);
                }
                opcodes::STA_ZPX => {
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    self.mem.write_bus(addr, self.cpu.a);
                }
                opcodes::STA_ABY => {
                    let addr: u16 = self.mem.addr_absolute_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    self.mem.write_bus(addr, self.cpu.a);
                }
                opcodes::STA_INX => {
                    let addr: u16 = self.mem.addr_idx_indirect(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    self.mem.write_bus(addr, self.cpu.a);
                }
                opcodes::STA_INY => {
                    let addr: u16 = self.mem.addr_indirect_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    self.mem.write_bus(addr, self.cpu.a);
                }

                opcodes::STX_ABS => {
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    logdata.push(addr);
                    self.mem.write_bus(addr, self.cpu.x);
                }
                opcodes::STX_ZP => {
                    let addr: u16 = self.mem.addr_zeropage(self.cpu.pc) as u16;
                    logdata.push(addr);
                    self.mem.write_bus(addr, self.cpu.x);
                }
                opcodes::STX_ZPY => {
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.y);
                    logdata.push(addr);
                    self.mem.write_bus(addr, self.cpu.x);
                }
                opcodes::STY_ABS => {
                    let addr: u16 = self.mem.addr_absolute(self.cpu.pc);
                    logdata.push(addr);
                    self.mem.write_bus(addr, self.cpu.y);
                }
                opcodes::STY_ZP => {
                    let addr: u16 = self.mem.addr_zeropage(self.cpu.pc);
                    logdata.push(addr);
                    self.mem.write_bus(addr, self.cpu.y);
                }
                opcodes::STY_ZPX => {
                    let addr: u16 = self.mem.addr_zeropage_idx(self.cpu.pc, self.cpu.x);
                    logdata.push(addr);
                    self.mem.write_bus(addr, self.cpu.y);
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
                    self.exit(&format!(
                        "{} (0x{:x}) not implemented!",
                        self.lookup.name(opcode),
                        opcode
                    ));
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

            if self.should_log {
                let log_line: String = self.logformatter.log_instruction(
                    opcode,
                    self.lookup.name(opcode),
                    self.cpu.register_str(),
                    self.cpu.status_str(),
                    &logdata,
                );
                if self.verbose {
                    self.iohandler.log(&log_line);
                }
            }

            // TODO: replace this with real memory mapping handlers
            //self.rng();

            //self.iohandler.input(&mut self.mem);
            //self.iohandler.display(&self.mem);

            self.cpu.pc += size;
            self.instruction_count = self.instruction_count + 1;
            self.cycle_count = self.cycle_count + self.lookup.cycles(opcode) as u64
        }

        self.iohandler.log(&format!(
            "Exiting after {} instructions, {} cycles ({:.1} MHz)",
            self.instruction_count,
            self.cycle_count,
            (self.cycle_count as f64 / start_time.elapsed().unwrap().as_secs_f64()) / 1_000_000.0
        ));
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

    fn load(&mut self, addr: u16) -> u8 {
        let val: u8 = self.mem.read_bus(addr);
        self.cpu.check_negative(val);
        self.cpu.check_zero(val);
        val
    }

    fn debug(&mut self) {
        self.iohandler.log(&format!(
            "entering debug mode after {} instructions ({} cycles)!",
            self.instruction_count, self.cycle_count
        ));

        if self.verbose {
            self.iohandler.log(&self.logformatter.replay());
        }

        self.iohandler
            .log(&self.logformatter.log_stack(&self.mem, self.cpu.sp));

        let opcode: u8 = self.mem.read_bus(self.cpu.pc);
        self.iohandler.log(&self.logformatter.log_monitor(
            opcode,
            self.lookup.name(opcode),
            self.cpu.pc,
            self.cpu.register_str(),
            self.cpu.status_str(),
        ));

        dbg::debug(self);
    }

    fn exit(&mut self, reason: &str) {
        self.iohandler.log(&"");
        self.iohandler.log(reason);

        let s = format!(
            "\nexited after {} instructions at cpu.pc=0x{:x}",
            self.instruction_count, self.cpu.pc
        );
        self.iohandler.exit(&s);
    }

    #[allow(dead_code)]
    fn rng(&mut self) {
        self.mem.write_bus(0xfe, rand::random::<u8>());
    }
}

#[cfg(test)]
mod emu_tests {
    use super::*;

    #[test]
    fn test_and_imm() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::AND_IMM);
        emu.mem.write_bus(start + 1, 0b1000_0000);
        emu.cpu.a = 0b1000_0001;
        emu.run();
        assert_eq!(emu.cpu.a, 128);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::AND_IMM);
        emu.mem.write_bus(start + 1, 0);
        emu.run();
        assert_eq!(emu.cpu.a, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_and_zpx() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::AND_ZPX);
        emu.mem.write_bus(start + 1, 0x01);
        emu.mem.write_bus(0x01, 0x01);
        emu.cpu.x = 0x01;
        emu.mem.write_bus(0x02, 0b1000_0000);
        emu.cpu.a = 0b1000_0001;
        emu.run();
        assert_eq!(emu.cpu.a, 128);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::AND_IMM);
        emu.mem.write_bus(0x02, 0);
        emu.run();
        assert_eq!(emu.cpu.a, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_bit_abs() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::BIT_ABS);
        emu.mem.write_bus(start + 1, 0x11);
        emu.mem.write_bus(start + 2, 0x47);

        emu.mem.write_bus(0x4711, 0b1100_0000);
        emu.cpu.a = 0b1000_0001;
        emu.run();

        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.overflow_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.write_bus(0x4711, 0b0100_0000);
        emu.cpu.a = 0b1000_0001;
        emu.run();

        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.overflow_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_bit_zp() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::BIT_ZP);
        emu.mem.write_bus(start + 1, 0x01);
        emu.mem.write_bus(0x01, 0b1100_0000);
        emu.cpu.a = 0b1000_0001;
        emu.run();

        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.overflow_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.write_bus(0x01, 0b0100_0000);
        emu.cpu.a = 0b1000_0001;
        emu.run();

        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.overflow_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_brk() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::BRK);
        emu.run();

        assert_eq!(emu.cpu.interrupt_flag(), false);
        assert_eq!(emu.cpu.pc, 0x600);

        emu.mem.write_bus(0xffff, 0x47);
        emu.mem.write_bus(0xfffe, 0x11);
        emu.cpu.set_status_flag(cpu::NEGATIVE_BIT);
        emu.run();

        assert_eq!(emu.cpu.interrupt_flag(), true);
        assert_eq!(emu.cpu.pc, 0x4711);
        assert_eq!(emu.mem.read_bus(0x1ff), 0x6);
        assert_eq!(emu.mem.read_bus(0x1fe), 0x2);
        assert_eq!(emu.mem.read_bus(0x1fd), 0b1011_0000);
    }

    #[test]
    fn test_clc() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::CLC);
        emu.cpu.set_status_flag(cpu::CARRY_BIT);
        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
    }

    #[test]
    fn test_cld() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::CLD);
        emu.cpu.set_status_flag(cpu::DECIMAL_BIT);
        emu.run();

        assert_eq!(emu.cpu.decimal_flag(), false);
    }

    #[test]
    fn test_cli() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::CLI);
        emu.cpu.set_status_flag(cpu::INTERRUPT_BIT);
        emu.run();

        assert_eq!(emu.cpu.interrupt_flag(), false);
    }

    #[test]
    fn test_clv() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::CLV);
        emu.cpu.set_status_flag(cpu::OVERFLOW_BIT);
        emu.run();

        assert_eq!(emu.cpu.overflow_flag(), false);
    }

    #[test]
    fn test_cmp_abs() {
        let start: u16 = memory::CODE_START_ADDR;
        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 0;
        emu.mem.write_bus(start, opcodes::CMP_ABS);
        emu.mem.write_bus(start + 1, 0x11);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 1;
        emu.mem.write_bus(start, opcodes::CMP_ABS);
        emu.mem.write_bus(start + 1, 0x11);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 0;
        emu.mem.write_bus(start, opcodes::CMP_ABS);
        emu.mem.write_bus(start + 1, 0x11);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 1);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_cmp_abx() {
        let start: u16 = memory::CODE_START_ADDR;
        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::CMP_ABX);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 1;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::CMP_ABX);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::CMP_ABX);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 1);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_cmp_aby() {
        let start: u16 = memory::CODE_START_ADDR;
        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 0;
        emu.cpu.y = 1;
        emu.mem.write_bus(start, opcodes::CMP_ABY);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 1;
        emu.cpu.y = 1;
        emu.mem.write_bus(start, opcodes::CMP_ABY);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 0;
        emu.cpu.y = 1;
        emu.mem.write_bus(start, opcodes::CMP_ABY);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 1);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_cmp_imm() {
        let start: u16 = memory::CODE_START_ADDR;
        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::CMP_IMM);
        emu.mem.write_bus(start + 1, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 1;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::CMP_IMM);
        emu.mem.write_bus(start + 1, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::CMP_IMM);
        emu.mem.write_bus(start + 1, 1);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_cmp_inx() {
        let start: u16 = memory::CODE_START_ADDR;
        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::CMP_INX);
        emu.mem.write_bus(start + 1, 0x41);
        emu.mem.write_bus(0x42, 0x11);
        emu.mem.write_bus(0x43, 0x47);
        emu.mem.write_bus(0x4711, 0);
        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 1;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::CMP_INX);
        emu.mem.write_bus(start + 1, 0x41);
        emu.mem.write_bus(0x42, 0x11);
        emu.mem.write_bus(0x43, 0x47);
        emu.mem.write_bus(0x4711, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::CMP_INX);
        emu.mem.write_bus(start + 1, 0x41);
        emu.mem.write_bus(0x42, 0x11);
        emu.mem.write_bus(0x43, 0x47);
        emu.mem.write_bus(0x4711, 1);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_cmp_iny() {
        let start: u16 = memory::CODE_START_ADDR;
        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 0;
        emu.cpu.y = 1;
        emu.mem.write_bus(start, opcodes::CMP_INY);
        emu.mem.write_bus(start + 1, 0x42);
        emu.mem.write_bus(0x42, 0x10);
        emu.mem.write_bus(0x43, 0x47);
        emu.mem.write_bus(0x4711, 0);
        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 1;
        emu.cpu.y = 1;
        emu.mem.write_bus(start, opcodes::CMP_INY);
        emu.mem.write_bus(start + 1, 0x42);
        emu.mem.write_bus(0x42, 0x10);
        emu.mem.write_bus(0x43, 0x47);
        emu.mem.write_bus(0x4711, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 0;
        emu.cpu.y = 1;
        emu.mem.write_bus(start, opcodes::CMP_INY);
        emu.mem.write_bus(start + 1, 0x42);
        emu.mem.write_bus(0x42, 0x10);
        emu.mem.write_bus(0x43, 0x47);
        emu.mem.write_bus(0x4711, 1);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_cmp_zpx() {
        let start: u16 = memory::CODE_START_ADDR;
        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::CMP_ZPX);
        emu.mem.write_bus(start + 1, 0x41);
        emu.mem.write_bus(0x42, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 1;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::CMP_ZPX);
        emu.mem.write_bus(start + 1, 0x41);
        emu.mem.write_bus(0x42, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::CMP_ZPX);
        emu.mem.write_bus(start + 1, 0x41);
        emu.mem.write_bus(0x42, 1);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_dec_zp() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::DEC_ZP);
        emu.mem.write_bus(start + 1, 0x01);
        emu.mem.write_bus(0x01, 0x00);
        emu.run();

        assert_eq!(emu.mem.read_bus(0x01), 0xff);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.write_bus(0x01, 1);
        emu.run();

        assert_eq!(emu.mem.read_bus(0x01), 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_eor_imm() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::EOR_IMM);
        emu.mem.write_bus(start + 1, 0b1000_1000);
        emu.cpu.a = 0b0000_1000;
        emu.run();

        assert_eq!(emu.cpu.a, 0b1000_0000);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_inc_zp() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::INC_ZP);
        emu.mem.write_bus(start + 1, 0x01);
        emu.mem.write_bus(0x01, 0xff);
        emu.run();

        assert_eq!(emu.mem.read_bus(0x01), 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.write_bus(0x01, 127);
        emu.run();

        assert_eq!(emu.mem.read_bus(0x01), 128);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_jmp_abs() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::JMP_ABS);
        emu.mem.write_bus(start + 1, 0x11);
        emu.mem.write_bus(start + 2, 0x47);
        emu.run();

        assert_eq!(emu.cpu.pc, 0x4711);
    }

    #[test]
    fn test_jmp_ind() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::JMP_IND);
        emu.mem.write_bus(start + 1, 0x11);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 0x42);

        emu.run();

        assert_eq!(emu.cpu.pc, 0x42);
    }

    #[test]
    fn test_jsr() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::JSR_ABS);
        emu.mem.write_bus(start + 1, 0x11);
        emu.mem.write_bus(start + 2, 0x47);

        emu.run();

        assert_eq!(emu.cpu.pc, 0x4711);
        assert_eq!(emu.cpu.sp, 0xfd);
        assert_eq!(emu.mem.read_bus(0x1ff), (memory::CODE_START_ADDR >> 8) as u8);
        assert_eq!(emu.mem.read_bus(0x1fe), 0x02);
    }

    #[test]
    fn test_lda_abs() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::LDA_ABS);
        emu.mem.write_bus(start + 1, 0x11);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 0x42);
        emu.run();

        assert_eq!(emu.cpu.a, 0x042);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_lda_abs_flags() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::LDA_ABS);
        emu.mem.write_bus(start + 1, 0x11);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 255);
        emu.run();

        assert_eq!(emu.cpu.a, 255);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.write_bus(0x4711, 0);
        emu.run();

        assert_eq!(emu.cpu.a, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_lda_abx() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::LDA_ABX);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 0x42);
        emu.run();

        assert_eq!(emu.cpu.a, 0x042);
    }

    #[test]
    fn test_lda_aby() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.y = 1;
        emu.mem.write_bus(start, opcodes::LDA_ABY);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 0x42);
        emu.run();

        assert_eq!(emu.cpu.a, 0x42);
    }

    #[test]
    fn test_lda_imm() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::LDA_IMM);
        emu.mem.write_bus(start + 1, 0x42);
        emu.run();

        assert_eq!(emu.cpu.a, 0x42);
    }

    #[test]
    fn test_lda_inx() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::LDA_INX);
        emu.mem.write_bus(start + 1, 0x41);
        emu.mem.write_bus(0x42, 0x15);
        emu.mem.write_bus(0x15, 0x12);
        emu.run();

        assert_eq!(emu.cpu.a, 0x12);
    }

    #[test]
    fn test_lda_iny() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.y = 1;
        emu.mem.write_bus(start, opcodes::LDA_INY);
        emu.mem.write_bus(start + 1, 0x16);
        emu.mem.write_bus(0x16, 0x10);
        emu.mem.write_bus(0x17, 0x42);
        emu.mem.write_bus(0x4211, 0x11);
        emu.run();

        assert_eq!(emu.cpu.a, 0x11);
    }

    #[test]
    fn test_lda_iny_wrap() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.y = 0x1;
        emu.mem.write_bus(start, opcodes::LDA_INY);
        emu.mem.write_bus(start + 1, 0x41);
        emu.mem.write_bus(0x41, 0xff);
        emu.mem.write_bus(0x42, 0x41);
        emu.mem.write_bus(0x4200, 0x47);
        emu.run();

        assert_eq!(emu.cpu.a, 0x47);
    }

    #[test]
    fn test_lda_zp() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::LDA_ZP);
        emu.mem.write_bus(start + 1, 0x42);
        emu.mem.write_bus(0x42, 0x15);
        emu.run();

        assert_eq!(emu.cpu.a, 0x15);
    }

    #[test]
    fn test_lda_zpx() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::LDA_ZPX);
        emu.mem.write_bus(start + 1, 0x41);
        emu.mem.write_bus(0x42, 0x15);
        emu.run();

        assert_eq!(emu.cpu.a, 0x15);
    }

    #[test]
    fn test_ldx_aby() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.y = 1;
        emu.mem.write_bus(start, opcodes::LDX_ABY);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 0x15);
        emu.run();

        assert_eq!(emu.cpu.x, 0x15);
    }

    #[test]
    fn test_ldx_zp() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::LDX_ZP);
        emu.mem.write_bus(start + 1, 0x41);
        emu.mem.write_bus(0x41, 0x15);
        emu.run();

        assert_eq!(emu.cpu.x, 0x15);
    }

    #[test]
    fn test_ldx_zp_flags() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::LDX_ZP);
        emu.mem.write_bus(start + 1, 0x41);
        emu.mem.write_bus(0x41, 255);
        emu.run();

        assert_eq!(emu.cpu.x, 255);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.write_bus(0x41, 0);
        emu.run();

        assert_eq!(emu.cpu.x, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_ldx_zpy() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 1;
        emu.cpu.y = 8;
        emu.mem.write_bus(start, opcodes::LDX_ZPY);
        emu.mem.write_bus(start + 1, 0x41);
        emu.mem.write_bus(0x41 + 8, 0x15);
        emu.run();

        assert_eq!(emu.cpu.x, 0x15);
    }

    #[test]
    fn test_ldy_abs() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::LDY_ABS);
        emu.mem.write_bus(start + 1, 0x11);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 0x15);
        emu.run();

        assert_eq!(emu.cpu.y, 0x15);
    }

    #[test]
    fn test_ldy_abx() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::LDY_ABX);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(start + 2, 0x47);
        emu.mem.write_bus(0x4711, 0x15);
        emu.run();

        assert_eq!(emu.cpu.y, 0x15);
    }

    #[test]
    fn test_ldy_zp() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::LDY_ZP);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(0x10, 0x15);
        emu.run();

        assert_eq!(emu.cpu.y, 0x15);
    }

    #[test]
    fn test_ldy_zpx() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 8;
        emu.mem.write_bus(start, opcodes::LDY_ZPX);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(0x18, 0x15);
        emu.run();

        assert_eq!(emu.cpu.y, 0x15);
    }

    #[test]
    fn test_ldy_imm() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::LDY_IMM);
        emu.mem.write_bus(start + 1, 0x15);
        emu.run();

        assert_eq!(emu.cpu.y, 0x15);
    }

    #[test]
    fn test_lsr() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 3;
        emu.mem.write_bus(start, opcodes::LSR);
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
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::NOP);

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
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0b1000_0000;
        emu.mem.write_bus(start, opcodes::ORA_IMM);
        emu.mem.write_bus(start + 1, 0b0000_0001);

        emu.run();
        assert_eq!(emu.cpu.a, 129);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.cpu.a = 0;
        emu.mem.write_bus(start, opcodes::ORA_IMM);
        emu.mem.write_bus(start + 1, 0);

        emu.run();
        assert_eq!(emu.cpu.a, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_pha() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0x42;
        emu.mem.write_bus(start, opcodes::PHA);
        emu.run();

        assert_eq!(emu.mem.read_bus(0x1ff), 0x42);
        assert_eq!(emu.cpu.sp, 0xfe);
    }

    #[test]
    fn test_pla() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.sp -= 1;
        emu.mem.write_bus(0x1ff, 0x42);
        emu.mem.write_bus(start, opcodes::PLA);
        emu.run();

        assert_eq!(emu.cpu.a, 0x42);
        assert_eq!(emu.cpu.sp, 0xff);
    }

    #[test]
    fn test_php() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.set_status_flag(cpu::CARRY_BIT);
        emu.mem.write_bus(start, opcodes::PHP);
        emu.run();

        assert_eq!(emu.mem.read_bus(0x1ff), 0b0011_0001);
        assert_eq!(emu.cpu.sp, 0xfe);
    }

    #[test]
    fn test_plp() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.sp -= 1;
        emu.mem.write_bus(0x1ff, 0x1);
        emu.mem.write_bus(start, opcodes::PLP);
        emu.run();

        assert_eq!(emu.cpu.status, 0b0010_0001);
        assert_eq!(emu.cpu.sp, 0xff);
    }

    #[test]
    fn test_plp_overflow() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::PLP);
        emu.mem.write_bus(0x100, 0x2);

        emu.run();

        assert_eq!(emu.cpu.status, 0b0010_0010);
        assert_eq!(emu.cpu.sp, 0x0);
        assert_eq!(emu.cpu.overflow_flag(), false);
    }

    #[test]
    fn test_rti() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::RTI);
        emu.mem.write_bus(0x1ff, 0x6);
        emu.mem.write_bus(0x1fe, 0x8);
        emu.mem.write_bus(0x1fd, 0b1011_0000);
        emu.cpu.set_status_flag(cpu::INTERRUPT_BIT);
        emu.cpu.sp = 0xfc;
        emu.run();

        assert_eq!(emu.cpu.status, 0b1010_0000);
        assert_eq!(emu.cpu.pc, 0x608);
    }

    #[test]
    fn test_rts() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::RTS);
        emu.cpu.sp = 0xfd;
        emu.mem.write_bus(0x1ff, (memory::CODE_START_ADDR >> 8) as u8);
        emu.mem.write_bus(0x1fe, 0x01);

        emu.run();
        assert_eq!(emu.cpu.pc, 0x0602);
        assert_eq!(emu.cpu.sp, 0xff);
    }

    #[test]
    fn test_sta_abx() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0x42;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::STA_ABX);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(start + 2, 0x47);
        emu.run();

        assert_eq!(emu.mem.read_bus(0x4711), 0x42);
    }

    #[test]
    fn test_sta_aby() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0x42;
        emu.cpu.y = 1;
        emu.mem.write_bus(start, opcodes::STA_ABY);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(start + 2, 0x47);
        emu.run();

        assert_eq!(emu.mem.read_bus(0x4711), 0x42);
    }

    #[test]
    fn test_sta_inx() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0x42;
        emu.cpu.x = 1;
        emu.mem.write_bus(start, opcodes::STA_INX);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(0x11, 0x42);

        emu.run();

        assert_eq!(emu.mem.read_bus(0x42), 0x42);
    }

    #[test]
    fn test_sta_iny() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0x42;
        emu.cpu.y = 1;
        emu.mem.write_bus(start, opcodes::STA_INY);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(0x10, 0x41);

        emu.run();

        assert_eq!(emu.mem.read_bus(0x42), 0x42);
    }

    #[test]
    fn test_sta_iny_wrap() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0x42;
        emu.cpu.y = 1;
        emu.mem.write_bus(start, opcodes::STA_INY);
        emu.mem.write_bus(start + 1, 0x10);
        emu.mem.write_bus(0x10, 0xff);
        emu.mem.write_bus(0x11, 0x0);

        emu.run();

        assert_eq!(emu.mem.read_bus(0x100), 0x42);
    }

    #[test]
    fn test_sta_zpx() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 0x01;
        emu.cpu.a = 0x42;
        emu.mem.write_bus(start, opcodes::STA_ZPX);
        emu.mem.write_bus(start + 1, 0x10);
        emu.run();

        assert_eq!(emu.mem.read_bus(0x11), 0x42);
    }

    #[test]
    fn test_stx_zp() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 0x42;
        emu.mem.write_bus(start, opcodes::STX_ZP);
        emu.mem.write_bus(start + 1, 0x11);
        emu.run();

        assert_eq!(emu.mem.read_bus(0x11), 0x42);
    }

    #[test]
    fn test_stx_zpy() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 0x42;
        emu.cpu.y = 0x1;

        emu.mem.write_bus(start, opcodes::STX_ZPY);
        emu.mem.write_bus(start + 1, 0x10);
        emu.run();

        assert_eq!(emu.mem.read_bus(0x11), 0x42);
    }

    #[test]
    fn test_sty_zp() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.y = 0x42;
        emu.mem.write_bus(start, opcodes::STY_ZP);
        emu.mem.write_bus(start + 1, 0x11);
        emu.run();

        assert_eq!(emu.mem.read_bus(0x11), 0x42);
    }

    #[test]
    fn test_sty_zpx() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.y = 0x42;
        emu.cpu.x = 0x1;

        emu.mem.write_bus(start, opcodes::STY_ZPX);
        emu.mem.write_bus(start + 1, 0x10);
        emu.run();

        assert_eq!(emu.mem.read_bus(0x11), 0x42);
    }

    #[test]
    fn test_sec() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::SEC);
        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
    }

    #[test]
    fn test_sed() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::SED);
        emu.run();

        assert_eq!(emu.cpu.decimal_flag(), true);
    }

    #[test]
    fn test_sei() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.write_bus(start, opcodes::SEI);
        emu.run();

        assert_eq!(emu.cpu.interrupt_flag(), true);
    }

    #[test]
    fn test_tsx() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.sp = 255;
        emu.mem.write_bus(start, opcodes::TSX);
        emu.run();

        assert_eq!(emu.cpu.x, 255);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::new_headless();
        emu.cpu.sp = 0;
        emu.mem.write_bus(start, opcodes::TSX);
        emu.run();

        assert_eq!(emu.cpu.x, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_txs() {
        let mut emu: Emulator = Emulator::new_headless();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 255;
        emu.mem.write_bus(start, opcodes::TXS);
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
