pub mod apu;
pub mod audio;
pub mod cpu;
pub mod dbg;
pub mod gfx;
pub mod io;
pub mod memory;
pub mod ppu;
pub mod savestate;

use cpu::opcodes;

extern crate shrust;
use std::collections::HashSet;
use std::time::Instant;

use self::audio::{AudioBackend, AudioOutput, SilentAudioOutput};



#[derive(PartialEq)]
pub enum CycleState {
    CpuAhead,
    CpuExecuted,
    Exiting,
}

pub struct Emulator {
    pub cpu: cpu::Cpu,
    lookup: Box<opcodes::Lookup>,
    pub mem: Box<dyn memory::MemoryMapper>,
    pub ppu: ppu::PPU,
    pub apu: apu::APU,
    buf: Box<gfx::buf::Buffer>,

    iohandler: Box<dyn io::IOHandler>,

    stepping: bool,
    pub breakpoints: Box<HashSet<u16>>,

    logformatter: io::log::LogFormatter,
    #[allow(dead_code)]
    pub logdata: Box<Vec<u16>>,
    should_log: bool,
    should_debug_on_infinite_loop: bool,
    should_exit_on_infinite_loop: bool,
    verbose: bool,
    start_time: Instant,

    pub instructions: u64,
    pub cycles: u64,
    pub master_clock: u64,
    instruction_start_dot: u64,
    cpu_bus_cycle_offset: u8,

    should_trigger_nmi: bool,
    nmi_triggered_countdown: i8,
    pub audio: Box<dyn AudioBackend>,
    /// Until this CPU cycle count (exclusive), writes to PPU registers and OAM DMA are ignored.
    ppu_register_warmup_until_cpu_cycle: u64,
    rom_path: Option<String>,
    savestate_slot: u8,
    stats_base_cycles: u64,
    stats_base_frames: u64,
}

impl Emulator {
    pub fn new(mapper: Box<dyn memory::MemoryMapper>) -> Emulator {
        let audio = Box::new(AudioOutput::new(apu::SAMPLE_RATE)) as Box<dyn AudioBackend>;
        let iohandler = Box::new(io::WinitPixelsIOHandler::new(256, 240));

        Emulator::new_base(iohandler, mapper, audio)
    }

    pub fn new_headless(mapper: Box<dyn memory::MemoryMapper>) -> Emulator {
        let audio = Box::new(SilentAudioOutput::new()) as Box<dyn AudioBackend>;
        let iohandler = Box::new(io::HeadlessIOHandler {});

        Emulator::new_base(iohandler, mapper, audio)
    }

    pub fn _new() -> Emulator {
        let mapper = Box::new(memory::IdentityMapper::new(memory::CODE_START_ADDR));
        Emulator::new_headless(mapper)
    }

    fn new_base(
        iohandler: Box<dyn io::IOHandler>,
        mut mapper: Box<dyn memory::MemoryMapper>,
        audio: Box<dyn AudioBackend>,
    ) -> Emulator {
        let lookup: Box<opcodes::Lookup> = Box::new(opcodes::Lookup::new());

        let mut cpu = cpu::Cpu::new();
        cpu.pc = mapper.code_start();

        let buf = gfx::buf::Buffer::new();

        Emulator {
            cpu: cpu,
            lookup: lookup,
            mem: mapper,
            ppu: ppu::PPU::new(),
            apu: apu::APU::new(),
            buf: Box::new(buf),
            iohandler: iohandler,
            stepping: false,
            breakpoints: Box::new(HashSet::new()),
            logformatter: io::log::LogFormatter::new(30),
            logdata: Box::new(Vec::<u16>::new()),
            should_log: true,
            should_debug_on_infinite_loop: false,
            should_exit_on_infinite_loop: true,
            verbose: true,
            start_time: Instant::now(),
            instructions: 0,
            cycles: 0,
            master_clock: 0,
            instruction_start_dot: 0,
            cpu_bus_cycle_offset: 0,
            should_trigger_nmi: false,
            nmi_triggered_countdown: -1,
            audio: audio,
            // https://www.nesdev.org/wiki/PPU_power_up_state — model as ignoring host writes briefly after power on.
            ppu_register_warmup_until_cpu_cycle: 29_658,
            rom_path: None,
            savestate_slot: 0,
            stats_base_cycles: 0,
            stats_base_frames: 0,
        }
    }

    pub fn set_rom_path(&mut self, path: &str) {
        self.rom_path = Some(path.to_string());
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

    pub fn toggle_should_exit_on_infinite_loop(&mut self, exit: bool) {
        self.should_exit_on_infinite_loop = exit;
    }

    #[allow(dead_code)] // only used in tests
    pub fn toggle_should_trigger_nmi(&mut self, trigger: bool) {
        self.should_trigger_nmi = trigger;
    }

    #[allow(dead_code)] // only used in tests
    pub fn reset(&mut self) {
        let addr: u16 = self.mem.get_16b_addr(memory::RESET_TARGET_ADDR);
        self.cpu.set_status_flag(cpu::INTERRUPT_BIT);
        self.cpu.sp = self.cpu.sp.wrapping_sub(3);
        self.cpu.pc = addr;
        // Also reset the APU and clear any pending audio to avoid residual noise
        self.apu.reset();
        self.audio.clear();
        self.ppu_register_warmup_until_cpu_cycle = self.cpu.cycle.wrapping_add(29_658);
    }

    pub fn save_state_to_bytes(&self) -> Vec<u8> {
        let mut w = savestate::SavestateWriter::new();
        self.cpu.save_state(&mut w);
        self.ppu.save_state(&mut w);
        self.apu.save_state(&mut w);
        w.write_u8(self.mem.mapper_id());
        self.mem.save_state(&mut w);
        w.write_u64(self.cycles);
        w.write_u64(self.master_clock);
        w.write_u64(self.instruction_start_dot);
        w.write_u8(self.cpu_bus_cycle_offset);
        w.write_bool(self.should_trigger_nmi);
        w.write_i8(self.nmi_triggered_countdown);
        w.write_u64(self.ppu_register_warmup_until_cpu_cycle);
        w.write_u64(self.instructions);
        w.finish()
    }

    pub fn load_state_from_bytes(&mut self, data: &[u8]) -> std::io::Result<()> {
        let mut r = savestate::SavestateReader::new(data)?;
        self.load_state_from_reader(&mut r)?;
        self.audio.clear();
        self.start_time = Instant::now();
        self.stats_base_cycles = self.cycles;
        self.stats_base_frames = self.ppu.frames;
        Ok(())
    }

    pub fn save_state_to_file(&mut self) {
        let path = self.savestate_path();
        let data = self.save_state_to_bytes();
        match std::fs::write(&path, &data) {
            Ok(()) => println!("State saved to {} ({} bytes)", path, data.len()),
            Err(e) => println!("Failed to save state: {}", e),
        }
    }

    pub fn load_state_from_file(&mut self) {
        let path = self.savestate_path();
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                println!("Failed to load state: {}", e);
                return;
            }
        };
        if let Err(e) = self.load_state_from_bytes(&data) {
            println!("Failed to load state: {}", e);
            return;
        }
        println!("State loaded from {}", path);
    }

    fn load_state_from_reader(&mut self, r: &mut savestate::SavestateReader) -> std::io::Result<()> {
        self.cpu.load_state(r)?;
        self.ppu.load_state(r)?;
        self.apu.load_state(r)?;
        let mapper_id = r.read_u8()?;
        if mapper_id != self.mem.mapper_id() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                format!("mapper mismatch: save={} current={}", mapper_id, self.mem.mapper_id())));
        }
        self.mem.load_state(r)?;
        self.cycles = r.read_u64()?;
        self.master_clock = r.read_u64()?;
        self.instruction_start_dot = r.read_u64()?;
        self.cpu_bus_cycle_offset = r.read_u8()?;
        self.should_trigger_nmi = r.read_bool()?;
        self.nmi_triggered_countdown = r.read_i8()?;
        self.ppu_register_warmup_until_cpu_cycle = r.read_u64()?;
        self.instructions = r.read_u64()?;
        Ok(())
    }

    fn savestate_path(&self) -> String {
        match &self.rom_path {
            Some(p) => {
                let base = p.trim_end_matches(".nes").trim_end_matches(".NES");
                format!("{}.ss{}", base, self.savestate_slot)
            }
            None => format!("krankulator.ss{}", self.savestate_slot),
        }
    }

    #[cfg(test)]
    pub(crate) fn test_cpu_write(&mut self, addr: u16, value: u8) {
        self.cpu_write(addr, value);
    }

    pub fn run(&mut self) {
        match self.iohandler.init() {
            Err(msg) => self.iohandler.log(msg),
            _ => {}
        }
        self.start_time = Instant::now();

        loop {
            if self.cycle() == CycleState::Exiting {
                break;
            }
        }

        self.exit();
    }

    pub fn cycle(&mut self) -> CycleState {
        let mut state = CycleState::CpuAhead;
        if self.cpu.cycle == self.cycles {
            state = CycleState::CpuExecuted;

            if self.stepping
                || (!self.breakpoints.is_empty() && self.breakpoints.contains(&self.cpu.pc))
            {
                self.debug();
            }

            if (self.should_exit_on_infinite_loop || self.should_debug_on_infinite_loop)
                && self.cpu.pc == self.cpu.last_instruction
            {
                if self.mem.cpu_read(self.cpu.last_instruction as _) != opcodes::BRK {
                    let msg = format!("infite loop detected on addr 0x{:x}!", self.cpu.pc);
                    self.iohandler.log(msg);
                    if self.should_debug_on_infinite_loop {
                        self.debug();
                    } else if self.should_exit_on_infinite_loop {
                        state = CycleState::Exiting
                    }
                } else if self.should_exit_on_infinite_loop {
                    self.iohandler.log(format!("reached probable end of code"));
                    state = CycleState::Exiting
                }
            }

            if !self.service_pending_irq() {
                let opcode = self.execute_instruction();
                self.log_instruction(opcode, self.cpu.last_instruction);

                if self.nmi_triggered_countdown > 0 {
                    self.nmi_triggered_countdown = self.nmi_triggered_countdown.wrapping_sub(1)
                }
            }
        }

        let target_dot = self.master_clock + 3;
        let fire_vblank_nmi = self.sync_ppu_to_dot(target_dot);
        self.master_clock = target_dot;

        // Cycle the APU
        self.apu.cycle(&mut *self.mem);
        let samples = self.apu.get_audio_samples();
        if !samples.is_empty() {
            self.audio.push_samples(samples);
        }

        if self.should_trigger_nmi && (fire_vblank_nmi || self.nmi_triggered_countdown == 0) {
            self.trigger_nmi();
            self.nmi_triggered_countdown = -1;

            self.iohandler.render(&self.buf);
        }
        // Poll events at regular intervals (much less frequently than before)
        if self.cycles % 50000 == 0 {
            let result = self.iohandler.poll(&mut *self.mem, &mut self.apu, &mut self.cpu);
            if result.exit {
                state = CycleState::Exiting;
            }
            if result.save_state {
                self.save_state_to_file();
            }
            if result.load_state {
                self.load_state_from_file();
            }
            if result.cycle_slot {
                self.savestate_slot = (self.savestate_slot + 1) % 4;
                println!("Savestate slot: {}", self.savestate_slot);
            }
        }

        self.cycles += 1;

        state
    }

    #[cfg(debug_assertions)]
    pub fn log_init(&mut self) {
        self.logdata.clear();
    }

    #[cfg(not(debug_assertions))]
    pub fn log_init(&mut self) {}

    #[cfg(debug_assertions)]
    pub fn log_push(&mut self, data: u16) {
        self.logdata.push(data);
    }

    #[cfg(not(debug_assertions))]
    pub fn log_push(&mut self, _data: u16) {}

    #[cfg(debug_assertions)]
    pub fn log_instruction(&mut self, opcode: u8, pc: u16) {
        if self.should_log {
            let scanline = self.ppu.scanline;
            let cycle = self.ppu.cycle;
            let log_line: String = self.logformatter.log(self.logformatter.log_str(
                self.mem.raw_opcode(pc as _),
                self.lookup.name(opcode),
                self.lookup.size(opcode),
                pc,
                self.cpu.register_str(),
                self.cycles,
                self.cpu.status_str(),
                scanline,
                cycle,
                &self.logdata,
            ));
            if self.verbose {
                self.iohandler.log(log_line);
            }
        }
    }

    #[cfg(not(debug_assertions))]
    pub fn log_instruction(&mut self, _opcode: u8, _pc: u16) {}

    fn begin_cpu_instruction_timing(&mut self) {
        self.instruction_start_dot = self.master_clock;
        self.cpu_bus_cycle_offset = 0;
    }

    fn cpu_bus_access_dot(&self) -> u64 {
        self.instruction_start_dot + u64::from(self.cpu_bus_cycle_offset) * 3
    }

    /// CPU address decoded as a PPU register access, if this mapper exposes the NES PPU register bus.
    fn ppu_reg_cpu_addr(&self, addr: u16) -> Option<u16> {
        if !self.mem.cpu_maps_ppu_registers() {
            return None;
        }
        if (0x2000..=0x2007).contains(&addr) {
            Some(addr)
        } else if (0x2008..=0x3FFF).contains(&addr) && self.mem.cpu_maps_ppu_register_mirrors() {
            Some(0x2000 + (addr % 8))
        } else {
            None
        }
    }

    fn cpu_access_needs_ppu_sync(&self, addr: u16, is_write: bool) -> bool {
        self.ppu_reg_cpu_addr(addr).is_some()
            || addr == ppu::OAM_DMA
            || (is_write && addr >= 0x4020)
    }

    fn sync_ppu_to_dot(&mut self, target_dot: u64) -> bool {
        let mut cycle_260_scanlines: [u16; 4] = [0; 4];
        let mut cycle_260_count: usize = 0;
        let mut fire_vblank_nmi = false;

        while self.ppu.last_synced_dot < target_dot {
            let step = self
                .ppu
                .step_dot_with_rendering(&mut *self.mem, &mut self.buf);
            fire_vblank_nmi |= step.fire_vblank_nmi;
            if let Some(scanline) = step.ppu_cycle_260_scanline {
                if cycle_260_count < cycle_260_scanlines.len() {
                    cycle_260_scanlines[cycle_260_count] = scanline;
                    cycle_260_count += 1;
                }
            }
        }

        for i in 0..cycle_260_count {
            self.mem.ppu_cycle_260(cycle_260_scanlines[i]);
        }

        fire_vblank_nmi
    }

    fn sync_for_cpu_access(&mut self, addr: u16, is_write: bool) {
        if self.cpu_access_needs_ppu_sync(addr, is_write) {
            let target_dot = self.cpu_bus_access_dot();
            if self.sync_ppu_to_dot(target_dot) && self.should_trigger_nmi {
                self.nmi_triggered_countdown = 0;
            }
        }
    }

    fn service_pending_irq(&mut self) -> bool {
        if self.cpu.interrupt_flag() {
            return false;
        }

        if self.mem.poll_irq() {
            self.trigger_irq();
            return true;
        }

        false
    }

    fn cpu_read(&mut self, addr: u16) -> u8 {
        self.sync_for_cpu_access(addr, false);
        let ppu_reg = self.ppu_reg_cpu_addr(addr);
        let value = if let Some(reg) = ppu_reg {
            let v = self.ppu.read(reg, &*self.mem);
            if reg == ppu::DATA_ADDR {
                let a = self.ppu.get_current_vram_addr();
                self.mem.ppu_a12_transition(a);
            }
            v
        } else if addr == ppu::OAM_DMA {
            self.ppu.read(addr, &*self.mem)
        } else if addr == 0x4015 {
            self.apu.read(addr)
        } else if addr == 0x4016 {
            self.mem.controllers()[0].poll()
        } else if addr == 0x4017 {
            self.mem.controllers()[1].poll()
        } else {
            self.mem.cpu_read(addr)
        };
        self.cpu_bus_cycle_offset = self.cpu_bus_cycle_offset.wrapping_add(1);
        value
    }

    fn cpu_read_at(&mut self, addr: u16, cpu_cycle_offset: u8) -> u8 {
        self.cpu_bus_cycle_offset = cpu_cycle_offset;
        self.cpu_read(addr)
    }

    pub fn execute_instruction(&mut self) -> u8 {
        self.log_init();
        self.begin_cpu_instruction_timing();

        self.cpu.last_instruction = self.cpu.pc;
        let opcode = self.cpu_read_at(self.cpu.pc as _, 0);
        let size: u16 = self.lookup.size(opcode);

        match opcode {
            opcodes::AND_ABS
            | opcodes::AND_ABX
            | opcodes::AND_ABY
            | opcodes::AND_IMM
            | opcodes::AND_INX
            | opcodes::AND_INY
            | opcodes::AND_ZP
            | opcodes::AND_ZPX => {
                let addr = self.addr(opcode);
                let value = self.cpu_read(addr);
                self.cpu.and(value);
            }

            opcodes::ADC_ABS
            | opcodes::ADC_ABX
            | opcodes::ADC_ABY
            | opcodes::ADC_IMM
            | opcodes::ADC_INX
            | opcodes::ADC_INY
            | opcodes::ADC_ZP
            | opcodes::ADC_ZPX => {
                let addr = self.addr(opcode);
                self.adc(addr);
            }

            opcodes::ASL => {
                self.log_push(self.cpu.a as u16);
                self.cpu.a = self.cpu.asl(self.cpu.a);
                self.log_push(self.cpu.a as u16);
            }
            opcodes::ASL_ABS | opcodes::ASL_ABX | opcodes::ASL_ZP | opcodes::ASL_ZPX => {
                let addr = self.addr(opcode);
                self.asl(addr);
            }

            opcodes::BIT_ABS | opcodes::BIT_ZP => {
                // Test BITs
                let addr = self.addr(opcode);
                self.log_push(addr);
                let value = self.cpu_read(addr);
                self.cpu.bit(value);
            }

            opcodes::BPL => {
                let operand: i8 = self.cpu_read(self.cpu.pc + 1) as i8;
                self.log_push((operand as u16) & 0xff);
                // Branch on PLus)
                if !self.cpu.negative_flag() {
                    self.branch(operand);
                }
            }
            opcodes::BMI => {
                let operand: i8 = self.cpu_read(self.cpu.pc + 1) as i8;
                self.log_push((operand as u16) & 0xff);
                // Branch on MInus
                if self.cpu.negative_flag() {
                    self.branch(operand);
                }
            }
            opcodes::BVC => {
                let operand: i8 = self.cpu_read(self.cpu.pc + 1) as i8;
                self.log_push((operand as u16) & 0xff);
                // Branch on oVerflow Clear
                if !self.cpu.overflow_flag() {
                    self.branch(operand);
                }
            }
            opcodes::BVS => {
                let operand: i8 = self.cpu_read(self.cpu.pc + 1) as i8;
                self.log_push((operand as u16) & 0xff);
                // Branch on oVerflow Set
                if self.cpu.overflow_flag() {
                    self.branch(operand);
                }
            }
            opcodes::BCC => {
                let operand: i8 = self.cpu_read(self.cpu.pc + 1) as i8;
                self.log_push((operand as u16) & 0xff);
                // Branch on Carry Clear
                if !self.cpu.carry_flag() {
                    self.branch(operand);
                }
            }
            opcodes::BCS => {
                let operand: i8 = self.cpu_read(self.cpu.pc + 1) as i8;
                self.log_push((operand as u16) & 0xff);
                // Branch on Carry Set
                if self.cpu.carry_flag() {
                    self.branch(operand);
                }
            }
            opcodes::BEQ => {
                let operand: i8 = self.cpu_read(self.cpu.pc + 1) as i8;
                self.log_push((operand as u16) & 0xff);
                // Branch on EQual
                if self.cpu.zero_flag() {
                    self.branch(operand);
                }
            }
            opcodes::BNE => {
                let operand: i8 = self.cpu_read(self.cpu.pc + 1) as i8;
                self.log_push((operand as u16) & 0xff);
                // Branch on Not Equal
                if !self.cpu.zero_flag() {
                    self.branch(operand);
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
                let addr: u16 = self.get_16b_addr(memory::BRK_TARGET_ADDR);

                // BRK without a target gets ignored
                if addr > 0 {
                    self.push_pc_to_stack(2);
                    self.log_push(addr);
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

            opcodes::CMP_ABS
            | opcodes::CMP_ABX
            | opcodes::CMP_ABY
            | opcodes::CMP_IMM
            | opcodes::CMP_INX
            | opcodes::CMP_INY
            | opcodes::CMP_ZP
            | opcodes::CMP_ZPX => {
                let addr = self.addr(opcode);
                self.log_push(addr);
                let value = self.cpu_read(addr);
                self.log_push(value as u16);
                self.cpu.compare(self.cpu.a, value);
            }

            opcodes::CPX_ABS | opcodes::CPX_IMM | opcodes::CPX_ZP => {
                let addr = self.addr(opcode);
                let value = self.cpu_read(addr);
                self.cpu.compare(self.cpu.x, value);
            }

            opcodes::CPY_ABS | opcodes::CPY_IMM | opcodes::CPY_ZP => {
                let addr = self.addr(opcode);
                let value = self.cpu_read(addr);
                self.cpu.compare(self.cpu.y, value);
            }

            opcodes::DEC_ABS | opcodes::DEC_ABX | opcodes::DEC_ZP | opcodes::DEC_ZPX => {
                let addr = self.addr(opcode);
                self.dec(addr);
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

            opcodes::DCP_ABS
            | opcodes::DCP_ABX
            | opcodes::DCP_ABY
            | opcodes::DCP_INX
            | opcodes::DCP_INY
            | opcodes::DCP_ZP
            | opcodes::DCP_ZPX => {
                let addr = self.addr(opcode);
                self.dcp(addr);
            }

            opcodes::EOR_ABS
            | opcodes::EOR_ABX
            | opcodes::EOR_ABY
            | opcodes::EOR_IMM
            | opcodes::EOR_INX
            | opcodes::EOR_INY
            | opcodes::EOR_ZP
            | opcodes::EOR_ZPX => {
                let addr = self.addr(opcode);
                self.eor(addr);
            }

            opcodes::JMP_ABS => {
                // JuMP to address
                let addr: u16 = self.addr_absolute(self.cpu.pc);
                self.log_push(addr);
                self.cpu.pc = addr as _;
                // Compensate for length addition
                self.cpu.pc = self.cpu.pc.wrapping_sub(self.lookup.size(opcode));
            }
            opcodes::JMP_IND => {
                // JuMP to address stored in arg
                let addr: u16 = self.get_16b_addr(self.cpu.pc + 1);
                // AN INDIRECT JUMP MUST NEVER USE A
                // VECTOR BEGINNING ON THE LAST BYTE
                // OF A PAGE
                // For example if address $3000 contains $40, $30FF contains $80, and $3100 contains $50,
                // the result of JMP ($30FF) will be a transfer of control to $4080 rather than $5080 as you intended
                // i.e. the 6502 took the low byte of the address from $30FF and the high byte from $3000.
                let adjusted_addr = if (addr & 0xff) == 0xff {
                    addr & 0xff00
                } else {
                    addr + 1
                };

                let hb = self.cpu_read(adjusted_addr);
                let lb = self.cpu_read(addr);

                self.log_push(addr);

                let operand: u16 = memory::to_16b_addr(hb, lb);
                self.log_push(operand);
                self.cpu.pc = operand;
                // Compensate for length addition
                self.cpu.pc = self.cpu.pc.wrapping_sub(self.lookup.size(opcode));
            }

            opcodes::JSR_ABS => {
                // Jump to SubRoutine
                self.push_pc_to_stack(2);
                let addr: u16 = self.addr_absolute(self.cpu.pc);
                self.log_push(addr);
                self.cpu.pc = addr;
                // Compensate for length addition
                self.cpu.pc = self.cpu.pc.wrapping_sub(self.lookup.size(opcode));
            }

            opcodes::INC_ABS | opcodes::INC_ABX | opcodes::INC_ZP | opcodes::INC_ZPX => {
                let addr = self.addr(opcode);
                self.inc(addr);
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

            opcodes::ISB_ABS
            | opcodes::ISB_ABX
            | opcodes::ISB_ABY
            | opcodes::ISB_INX
            | opcodes::ISB_INY
            | opcodes::ISB_ZP
            | opcodes::ISB_ZPX => {
                let addr = self.addr(opcode);
                self.isb(addr);
            }

            opcodes::LAX_ABS
            | opcodes::LAX_ABY
            | opcodes::LAX_INX
            | opcodes::LAX_INY
            | opcodes::LAX_ZP
            | opcodes::LAX_ZPY => {
                let addr = self.addr(opcode);
                self.lax(addr);
            }

            opcodes::LDA_ABS
            | opcodes::LDA_ABX
            | opcodes::LDA_ABY
            | opcodes::LDA_IMM
            | opcodes::LDA_INX
            | opcodes::LDA_INY
            | opcodes::LDA_ZP
            | opcodes::LDA_ZPX => {
                let addr = self.addr(opcode);
                self.cpu.a = self.load(addr);
            }

            opcodes::LDX_ABS
            | opcodes::LDX_ABY
            | opcodes::LDX_IMM
            | opcodes::LDX_ZP
            | opcodes::LDX_ZPY => {
                let addr = self.addr(opcode);
                self.cpu.x = self.load(addr);
            }

            opcodes::LDY_ABS
            | opcodes::LDY_ABX
            | opcodes::LDY_IMM
            | opcodes::LDY_ZP
            | opcodes::LDY_ZPX => {
                let addr = self.addr(opcode);
                self.cpu.y = self.load(addr);
            }

            opcodes::LSR => {
                self.log_push(self.cpu.a as u16);
                self.cpu.a = self.cpu.lsr(self.cpu.a);
                self.log_push(self.cpu.a as u16);
            }
            opcodes::LSR_ABS | opcodes::LSR_ABX | opcodes::LSR_ZP | opcodes::LSR_ZPX => {
                let addr = self.addr(opcode);
                self.lsr(addr);
            }

            opcodes::NOP => {
                // No operation
            }

            opcodes::ORA_ABS
            | opcodes::ORA_ABX
            | opcodes::ORA_ABY
            | opcodes::ORA_IMM
            | opcodes::ORA_INX
            | opcodes::ORA_INY
            | opcodes::ORA_ZP
            | opcodes::ORA_ZPX => {
                let addr = self.addr(opcode);
                self.ora(addr);
            }

            opcodes::PHA => {
                // PusH Accumulator
                self.push_to_stack(self.cpu.a);
            }
            opcodes::PLA => {
                // PuLl Accumulator
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

            opcodes::RRA_ABS
            | opcodes::RRA_ABX
            | opcodes::RRA_ABY
            | opcodes::RRA_INX
            | opcodes::RRA_INY
            | opcodes::RRA_ZP
            | opcodes::RRA_ZPX => {
                let addr = self.addr(opcode);
                self.rra(addr);
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
                self.log_push(addr);

                self.cpu.pc = addr;
                // Compensate for length addition
                self.cpu.pc = self.cpu.pc.wrapping_sub(self.lookup.size(opcode));
            }

            opcodes::RTS => {
                // ReTurn from Subroutine
                let lb: u8 = self.pull_from_stack();
                let hb: u8 = self.pull_from_stack();

                let addr: u16 = ((hb as u16) << 8) + ((lb as u16) & 0xff) + 1;
                self.log_push(addr);

                self.cpu.pc = addr;
                // Compensate for length addition
                self.cpu.pc = self.cpu.pc.wrapping_sub(self.lookup.size(opcode));
            }

            opcodes::RLA_ABS
            | opcodes::RLA_ABX
            | opcodes::RLA_ABY
            | opcodes::RLA_INX
            | opcodes::RLA_INY
            | opcodes::RLA_ZP
            | opcodes::RLA_ZPX => {
                let addr = self.addr(opcode);
                self.rla(addr);
            }

            opcodes::ROL => {
                self.log_push(self.cpu.a as u16);
                self.cpu.a = self.cpu.rol(self.cpu.a);
                self.log_push(self.cpu.a as u16);
            }
            opcodes::ROL_ABS | opcodes::ROL_ABX | opcodes::ROL_ZP | opcodes::ROL_ZPX => {
                let addr = self.addr(opcode);
                self.rol(addr);
            }

            opcodes::ROR => {
                self.log_push(self.cpu.a as u16);
                self.cpu.a = self.cpu.ror(self.cpu.a);
                self.log_push(self.cpu.a as u16);
            }
            opcodes::ROR_ABS | opcodes::ROR_ABX | opcodes::ROR_ZP | opcodes::ROR_ZPX => {
                let addr = self.addr(opcode);
                self.ror(addr);
            }

            opcodes::SAX_ABS | opcodes::SAX_INX | opcodes::SAX_ZP | opcodes::SAX_ZPY => {
                let addr = self.addr(opcode);
                self.sax(addr);
            }

            opcodes::SBC_ABS
            | opcodes::SBC_ABX
            | opcodes::SBC_ABY
            | opcodes::SBC_IMM
            | opcodes::SBC_INX
            | opcodes::SBC_INY
            | opcodes::SBC_ZP
            | opcodes::SBC_ZPX
            | opcodes::SNC_IMM => {
                // Subtract Memory to Accumulator with Carry
                let addr = self.addr(opcode);
                self.sbc(addr);
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

            opcodes::SRE_ABS
            | opcodes::SRE_ABX
            | opcodes::SRE_ABY
            | opcodes::SRE_INX
            | opcodes::SRE_INY
            | opcodes::SRE_ZP
            | opcodes::SRE_ZPX => {
                let addr = self.addr(opcode);
                self.sre(addr);
            }

            opcodes::SLO_ABS
            | opcodes::SLO_ABX
            | opcodes::SLO_ABY
            | opcodes::SLO_INX
            | opcodes::SLO_INY
            | opcodes::SLO_ZP
            | opcodes::SLO_ZPX => {
                let addr = self.addr(opcode);
                self.slo(addr);
            }

            opcodes::STA_ABS
            | opcodes::STA_ABX
            | opcodes::STA_ABY
            | opcodes::STA_INX
            | opcodes::STA_INY
            | opcodes::STA_ZP
            | opcodes::STA_ZPX => {
                // Store hides page crossing penalties
                let addr = self.addr(opcode);
                self.log_push(addr);
                self.cpu_write(addr, self.cpu.a);
            }

            opcodes::STX_ABS | opcodes::STX_ZP | opcodes::STX_ZPY => {
                let addr = self.addr(opcode);
                self.log_push(addr);
                self.cpu_write(addr, self.cpu.x);
            }

            opcodes::STY_ABS | opcodes::STY_ZP | opcodes::STY_ZPX => {
                let addr = self.addr(opcode);
                self.log_push(addr);
                self.cpu_write(addr, self.cpu.y);
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
                // Infer page boundary penalty for certain unofficial NOPs
                if opcode & 0xf == 0xc && self.lookup.mode(opcode) != 0xff {
                    self.addr(opcode);
                }
            }
        }

        self.cpu.pc = self.cpu.pc.wrapping_add(size);
        self.instructions = self.instructions + 1;
        self.cpu.cycle += self.lookup.cycles(opcode) as u64;

        opcode
    }

    fn adc(&mut self, addr: u16) {
        // Add Memory to Accumulator with Carry
        self.log_push(addr);
        let operand: u8 = self.cpu_read(addr);
        self.log_push(operand as u16);
        self.cpu.add_to_a_with_carry(operand);
    }

    fn asl(&mut self, addr: u16) -> u8 {
        let value: u8 = self.cpu_read(addr);
        self.log_push(value as u16);
        let result: u8 = self.cpu.asl(value);
        self.log_push(result as u16);
        self.cpu_write(addr, result);

        result
    }

    fn branch(&mut self, operand: i8) {
        // Branching across page boundaries infers a cycle penalty
        if (self.cpu.pc.wrapping_add(2) & 0xff).wrapping_add(operand as u16) > 0xff {
            self.cpu.cycle += 1;
        }
        self.cpu.pc = self.cpu.pc.wrapping_add(operand as u16);
        self.log_push(self.cpu.pc + 2 as u16);
        // Branch taken is infers a cycle penalty
        self.cpu.cycle += 1;
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        self.sync_for_cpu_access(addr, true);

        let ppu_reg = self.ppu_reg_cpu_addr(addr);

        if let Some(reg) = ppu_reg {
            if self.cpu.cycle < self.ppu_register_warmup_until_cpu_cycle {
                self.cpu_bus_cycle_offset = self.cpu_bus_cycle_offset.wrapping_add(1);
                return;
            }
            if reg == ppu::CTRL_REG_ADDR {
                if (value & ppu::STATUS_VERTICAL_BLANK_BIT) == ppu::STATUS_VERTICAL_BLANK_BIT
                    && !self.ppu.vblank_nmi_is_enabled()
                    && self.ppu.is_in_vblank()
                {
                    self.nmi_triggered_countdown = 2;
                }
            }
            if let Some((waddr, wval)) = self.ppu.write(reg, value) {
                self.mem.ppu_write(waddr, wval);
            }
            if reg == ppu::ADDR_ADDR || reg == ppu::DATA_ADDR {
                let ppu_addr = if reg == ppu::ADDR_ADDR {
                    self.ppu.get_temp_vram_addr()
                } else {
                    self.ppu.get_current_vram_addr()
                };
                self.mem.ppu_a12_transition(ppu_addr);
            }
        } else if addr == ppu::OAM_DMA {
            if self.cpu.cycle < self.ppu_register_warmup_until_cpu_cycle {
                self.cpu_bus_cycle_offset = self.cpu_bus_cycle_offset.wrapping_add(1);
                return;
            }
            let base: u16 = (value as u16) << 8;
            for i in 0u16..256 {
                let byte = self.mem.cpu_read(base.wrapping_add(i));
                self.ppu.oam_dma_write(i as u8, byte);
            }
        } else if (0x4000..=0x4017).contains(&addr) && addr != ppu::OAM_DMA {
            let apu_cycle_tag = if addr == 0x4017 {
                self.cpu.cycle.wrapping_add(u64::from(self.cpu_bus_cycle_offset))
            } else {
                0
            };
            self.apu.write(addr, value, apu_cycle_tag);
        } else {
            self.mem.cpu_write(addr, value);
        }
        self.cpu_bus_cycle_offset = self.cpu_bus_cycle_offset.wrapping_add(1);
    }

    fn dec(&mut self, addr: u16) {
        // DECrement memory
        let operand: u8 = self.cpu_read(addr);
        self.log_push(operand as u16);
        let value: u8 = operand.wrapping_sub(1);
        self.cpu_write(addr, value);

        self.cpu.check_negative(value);
        self.cpu.check_zero(value);
    }

    fn dcp(&mut self, addr: u16) {
        let operand: u8 = self.cpu_read(addr);
        self.log_push(operand as u16);
        let value: u8 = operand.wrapping_sub(1);
        self.cpu_write(addr, value);

        self.cpu.check_negative(value);
        self.cpu.check_zero(value);

        self.cpu.compare(self.cpu.a, value);
    }

    fn eor(&mut self, addr: u16) {
        // bitwise Exclusive OR
        let operand: u8 = self.cpu_read(addr);
        self.log_push(operand as u16);
        self.cpu.eor(operand);
    }

    fn inc(&mut self, addr: u16) {
        // INCrement memory
        let operand: u8 = self.cpu_read(addr);
        self.log_push(operand as u16);
        let value: u8 = operand.wrapping_add(1);
        self.cpu_write(addr, value);

        self.cpu.check_negative(value);
        self.cpu.check_zero(value);
    }

    fn isb(&mut self, addr: u16) {
        let operand: u8 = self.cpu_read(addr);
        self.log_push(operand as u16);

        let value: u8 = operand.wrapping_add(1);
        self.cpu_write(addr, value);

        self.cpu.check_negative(value);
        self.cpu.check_zero(value);

        self.cpu.sub_from_a_with_carry(value);
    }

    fn lax(&mut self, addr: u16) {
        self.log_push(addr);
        let value = self.load(addr);
        self.cpu.a = value;
        self.cpu.x = value;
    }

    fn lsr(&mut self, addr: u16) -> u8 {
        let value: u8 = self.cpu_read(addr);
        self.log_push(value as u16);
        let result: u8 = self.cpu.lsr(value);
        self.log_push(result as u16);
        self.cpu_write(addr, result);

        result
    }

    fn ora(&mut self, addr: u16) {
        // Bitwise OR with Accumulator
        self.log_push(addr);
        let operand: u8 = self.cpu_read(addr);
        self.log_push(operand as u16);
        self.cpu.ora(operand);
    }

    fn rla(&mut self, addr: u16) {
        let result = self.rol(addr);
        self.cpu.and(result);
    }

    fn rol(&mut self, addr: u16) -> u8 {
        let value: u8 = self.cpu_read(addr);
        self.log_push(value as u16);
        let result: u8 = self.cpu.rol(value);
        self.log_push(result as u16);
        self.cpu_write(addr, result);

        result
    }

    fn ror(&mut self, addr: u16) -> u8 {
        let value: u8 = self.cpu_read(addr);
        self.log_push(value as u16);
        let result: u8 = self.cpu.ror(value);
        self.log_push(result as u16);
        self.cpu_write(addr, result);

        result
    }

    fn rra(&mut self, addr: u16) {
        let result = self.ror(addr);
        self.cpu.add_to_a_with_carry(result);
    }

    fn sax(&mut self, addr: u16) {
        self.log_push(addr);
        self.cpu_write(addr, self.cpu.a & self.cpu.x);
    }

    fn sbc(&mut self, addr: u16) {
        self.log_push(addr);
        let operand: u8 = self.cpu_read(addr);
        self.log_push(operand as u16);
        self.cpu.sub_from_a_with_carry(operand);
    }

    fn slo(&mut self, addr: u16) {
        let result = self.asl(addr);
        self.cpu.ora(result);
    }

    fn sre(&mut self, addr: u16) {
        let result = self.lsr(addr);
        self.cpu.eor(result);
    }

    fn push_pc_to_stack(&mut self, offset: u16) {
        let lb: u8 = ((self.cpu.pc.wrapping_add(offset)) & 0xff) as u8;
        let hb: u8 = ((self.cpu.pc.wrapping_add(offset)) >> 8) as u8;

        self.push_to_stack_addr(self.cpu.sp, hb);
        self.cpu.sp = self.cpu.sp.wrapping_sub(1);
        self.push_to_stack_addr(self.cpu.sp, lb);
        self.cpu.sp = self.cpu.sp.wrapping_sub(1);
    }

    fn pull_from_stack(&mut self) -> u8 {
        self.cpu.sp = self.cpu.sp.wrapping_add(1);
        self.pull_from_stack_addr(self.cpu.sp)
    }

    fn push_to_stack(&mut self, value: u8) {
        self.push_to_stack_addr(self.cpu.sp, value);
        self.cpu.sp = self.cpu.sp.wrapping_sub(1);
    }

    fn load(&mut self, addr: u16) -> u8 {
        self.log_push(addr);
        let val: u8 = self.cpu_read(addr);
        self.log_push(val as u16);
        self.cpu.check_negative(val);
        self.cpu.check_zero(val);
        val
    }

    fn get_16b_addr(&mut self, offset: u16) -> u16 {
        memory::to_16b_addr(self.cpu_read(offset.wrapping_add(1)), self.cpu_read(offset))
    }

    fn addr_absolute(&mut self, pc: u16) -> u16 {
        self.get_16b_addr(pc.wrapping_add(1))
    }

    fn addr_absolute_idx(&mut self, pc: u16, idx: u8) -> (u16, bool) {
        let lb = self.cpu_read(pc.wrapping_add(1));
        (
            memory::to_16b_addr(self.cpu_read(pc.wrapping_add(2)), lb).wrapping_add(idx as u16),
            (lb & 0xff) as u16 + idx as u16 > 0xff,
        )
    }

    fn addr_idx_indirect(&mut self, pc: u16, idx: u8) -> u16 {
        let value: u8 = self.cpu_read(pc + 1).wrapping_add(idx);
        ((self.cpu_read(value.wrapping_add(1) as u16) as u16) << 8)
            + self.cpu_read(value as u16) as u16
    }

    fn addr_indirect_idx(&mut self, pc: u16, idx: u8) -> (u16, bool) {
        let base = self.cpu_read(pc + 1);

        let lb = self.cpu_read(base as _);
        let lbidx = lb.wrapping_add(idx);
        let carry: u8 = if lbidx <= lb && idx > 0 { 1 } else { 0 };
        let hb = self
            .cpu_read((base as u8).wrapping_add(1) as _)
            .wrapping_add(carry);

        (memory::to_16b_addr(hb, lbidx) as _, carry != 0)
    }

    fn addr_zeropage(&mut self, pc: u16) -> u16 {
        self.cpu_read(pc + 1) as _
    }

    fn addr_zeropage_idx(&mut self, pc: u16, idx: u8) -> u16 {
        self.cpu_read(pc + 1).wrapping_add(idx) as u16
    }

    fn stack_addr(&self, sp: u8) -> u16 {
        memory::STACK_BASE_OFFSET + (u16::from(sp) & 0xff)
    }

    fn push_to_stack_addr(&mut self, sp: u8, value: u8) {
        self.cpu_write(self.stack_addr(sp), value);
    }

    fn pull_from_stack_addr(&mut self, sp: u8) -> u8 {
        self.cpu_read(self.stack_addr(sp))
    }

    fn addr(&mut self, opcode: u8) -> u16 {
        match self.lookup.mode(opcode) {
            opcodes::ADDR_MODE_ABS => self.addr_absolute(self.cpu.pc),
            opcodes::ADDR_MODE_ABX => {
                let (addr, page_boundary_penalty) = self.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                if self.lookup.page_boundary_penalty(opcode) && page_boundary_penalty {
                    self.cpu.cycle += 1;
                }
                addr
            }
            opcodes::ADDR_MODE_ABY => {
                let (addr, page_boundary_penalty) = self.addr_absolute_idx(self.cpu.pc, self.cpu.y);
                if self.lookup.page_boundary_penalty(opcode) && page_boundary_penalty {
                    self.cpu.cycle += 1;
                }
                addr
            }
            opcodes::ADDR_MODE_IMM => self.cpu.pc + 1,
            opcodes::ADDR_MODE_INX => self.addr_idx_indirect(self.cpu.pc, self.cpu.x),
            opcodes::ADDR_MODE_INY => {
                let (addr, page_boundary_penalty) = self.addr_indirect_idx(self.cpu.pc, self.cpu.y);
                if self.lookup.page_boundary_penalty(opcode) && page_boundary_penalty {
                    self.cpu.cycle += 1;
                }
                addr
            }
            opcodes::ADDR_MODE_NA => 0,
            opcodes::ADDR_MODE_ZP => self.addr_zeropage(self.cpu.pc),
            opcodes::ADDR_MODE_ZPX => self.addr_zeropage_idx(self.cpu.pc, self.cpu.x),
            opcodes::ADDR_MODE_ZPY => self.addr_zeropage_idx(self.cpu.pc, self.cpu.y),
            _ => panic!("Addressing mode not found for opcode {:x}", opcode),
        }
    }

    pub fn trigger_nmi(&mut self) {
        let addr: u16 = self.get_16b_addr(memory::NMI_TARGET_ADDR);
        self.push_pc_to_stack(0);
        // hardware interrupts IRQ & NMI will push the B flag as being 0.
        self.push_to_stack(self.cpu.status & !cpu::BREAK_BIT);

        // we set the I flag
        self.cpu.set_status_flag(cpu::INTERRUPT_BIT);

        self.cpu.pc = addr;
        self.cpu.cycle += 7;
    }

    pub fn trigger_irq(&mut self) {
        let addr: u16 = self.get_16b_addr(memory::BRK_TARGET_ADDR);
        self.push_pc_to_stack(0);
        // hardware interrupts IRQ & NMI will push the B flag as being 0.
        self.push_to_stack(self.cpu.status & !cpu::BREAK_BIT);
        self.cpu.set_status_flag(cpu::INTERRUPT_BIT);
        self.cpu.pc = addr;
        self.cpu.cycle += 7;
    }

    #[allow(dead_code)]
    pub fn get_audio_output(&mut self) -> Vec<f32> {
        self.apu.get_audio_samples().to_vec()
    }

    pub fn log_str(&mut self) -> String {
        let opcode: u8 = self.mem.cpu_read(self.cpu.pc);
        self.logformatter.log_str(
            self.mem.raw_opcode(self.cpu.pc),
            self.lookup.name(opcode),
            self.lookup.size(opcode),
            self.cpu.pc,
            self.cpu.register_str(),
            self.cycles,
            self.cpu.status_str(),
            self.ppu.scanline,
            self.ppu.cycle,
            &vec![],
        )
    }

    fn debug(&mut self) {
        self.iohandler.log(format!(
            "entering debug mode after {} instructions ({} cycles)!",
            self.instructions, self.cycles
        ));

        if !self.verbose {
            self.iohandler.log(self.logformatter.replay());
        }

        self.iohandler
            .log(self.logformatter.log_stack(&mut self.mem, self.cpu.sp));

        let logline = self.log_str();
        self.iohandler.log(logline);

        dbg::debug(self);
    }

    fn exit(&self) {
        if let (Some(rom_path), Some(sram)) = (&self.rom_path, self.mem.sram_data()) {
            let sav = io::loader::sav_path(rom_path);
            match std::fs::write(&sav, sram) {
                Ok(_) => println!("Saved battery RAM to {}", sav.display()),
                Err(e) => eprintln!("Failed to save battery RAM to {}: {}", sav.display(), e),
            }
        }

        let elapsed_secs = self.start_time.elapsed().as_secs_f64();
        let measured_cycles = self.cycles - self.stats_base_cycles;
        let measured_frames = self.ppu.frames - self.stats_base_frames;
        self.iohandler.exit(format!(
            "Exiting after {} instructions, {} cycles ({:.1} MHz) {:.1} avg fps",
            self.instructions,
            self.cycles,
            (measured_cycles as f64 / elapsed_secs) / 1_000_000.0,
            measured_frames as f64 / elapsed_secs
        ));
    }
}

#[cfg(test)]
mod emu_tests {
    use super::*;

    #[test]
    fn test_addr() {
        let mut emu: Emulator = Emulator::_new();
        emu.cpu.pc = 0x4711;

        assert_eq!(0x4712, emu.addr(opcodes::ADC_IMM));

        emu.mem.cpu_write(0x4712, 0x34);
        emu.mem.cpu_write(0x4713, 0x12);
        assert_eq!(0x1234, emu.addr(opcodes::ADC_ABS));

        emu.cpu.x = 1;
        assert_eq!(0x1235, emu.addr(opcodes::ADC_ABX));

        emu.cpu.y = 2;
        assert_eq!(0x1236, emu.addr(opcodes::ADC_ABY));

        assert_eq!(0, emu.addr(opcodes::ADC_INX));

        assert_eq!(2, emu.addr(opcodes::ADC_INY));

        assert_eq!(0, emu.addr(opcodes::NOP));

        assert_eq!(0x34, emu.addr(opcodes::ADC_ZP));

        assert_eq!(0x35, emu.addr(opcodes::ADC_ZPX));
    }

    #[test]
    fn test_master_clock_advances_3x_cpu() {
        let mut emu: Emulator = Emulator::_new();
        emu.cpu.cycle = 1;

        emu.cycle();

        assert_eq!(emu.cycles, 1);
        assert_eq!(emu.master_clock, 3);
        assert_eq!(emu.ppu.last_synced_dot, 3);
    }

    #[test]
    fn test_register_read_triggers_catch_up() {
        let mut emu: Emulator = Emulator::_new();
        emu.begin_cpu_instruction_timing();

        emu.cpu_read_at(ppu::STATUS_REG_ADDR, 2);

        assert_eq!(emu.ppu.last_synced_dot, 6);
    }

    #[test]
    fn test_register_write_uses_bus_cycle_timestamp() {
        let mut emu: Emulator = Emulator::_new();
        emu.begin_cpu_instruction_timing();
        emu.cpu_bus_cycle_offset = 3;

        emu.cpu_write(ppu::SCROLL_ADDR, 0x24);

        assert_eq!(emu.ppu.last_synced_dot, 9);
    }

    #[test]
    fn test_cpu_access_offsets_match_instruction_timing() {
        let mut emu: Emulator = Emulator::_new();
        let start = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::LDA_ABS);
        emu.mem.cpu_write(start + 1, ppu::STATUS_REG_ADDR as u8);
        emu.mem
            .cpu_write(start + 2, (ppu::STATUS_REG_ADDR >> 8) as u8);

        emu.execute_instruction();

        assert_eq!(emu.ppu.last_synced_dot, 9);
    }

    #[test]
    fn test_and_imm() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::AND_IMM);
        emu.mem.cpu_write(start + 1, 0b1000_0000);
        emu.cpu.a = 0b1000_0001;
        emu.run();
        assert_eq!(emu.cpu.a, 128);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::AND_IMM);
        emu.mem.cpu_write(start + 1, 0);
        emu.run();
        assert_eq!(emu.cpu.a, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_and_zpx() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::AND_ZPX);
        emu.mem.cpu_write(start + 1, 0x01);
        emu.mem.cpu_write(0x01, 0x01);
        emu.cpu.x = 0x01;
        emu.mem.cpu_write(0x02, 0b1000_0000);
        emu.cpu.a = 0b1000_0001;
        emu.run();
        assert_eq!(emu.cpu.a, 128);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::AND_IMM);
        emu.mem.cpu_write(0x02, 0);
        emu.run();
        assert_eq!(emu.cpu.a, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_adc_zp() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::ADC_ZP);
        emu.mem.cpu_write(start + 1, 0x01);
        emu.mem.cpu_write(0x01, 0x80);
        emu.cpu.a = 0x80;
        emu.run();
        assert_eq!(emu.cpu.a, 0x0);
        assert_eq!(emu.mem.cpu_read(0x1), 0x80);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
    }

    #[test]
    fn test_bit_abs() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::BIT_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);

        emu.mem.cpu_write(0x4711, 0b1100_0000);
        emu.cpu.a = 0b1000_0001;
        emu.run();

        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.overflow_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.cpu_write(0x4711, 0b0100_0000);
        emu.cpu.a = 0b1000_0001;
        emu.run();

        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.overflow_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_bit_zp() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::BIT_ZP);
        emu.mem.cpu_write(start + 1, 0x01);
        emu.mem.cpu_write(0x01, 0b1100_0000);
        emu.cpu.a = 0b1000_0001;
        emu.run();

        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.overflow_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.cpu_write(0x01, 0b0100_0000);
        emu.cpu.a = 0b1000_0001;
        emu.run();

        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.overflow_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_brk() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::BRK);
        emu.run();

        assert_eq!(emu.cpu.interrupt_flag(), false);
        assert_eq!(emu.cpu.pc, 0x600);

        emu.mem.cpu_write(0xffff, 0x47);
        emu.mem.cpu_write(0xfffe, 0x11);
        emu.cpu.set_status_flag(cpu::NEGATIVE_BIT);
        emu.run();

        assert_eq!(emu.cpu.interrupt_flag(), true);
        assert_eq!(emu.cpu.pc, 0x4711);
        assert_eq!(emu.mem.cpu_read(0x1ff), 0x6);
        assert_eq!(emu.mem.cpu_read(0x1fe), 0x2);
        assert_eq!(emu.mem.cpu_read(0x1fd), 0b1011_0000);
    }

    #[test]
    fn test_clc() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::CLC);
        emu.cpu.set_status_flag(cpu::CARRY_BIT);
        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
    }

    #[test]
    fn test_cld() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::CLD);
        emu.cpu.set_status_flag(cpu::DECIMAL_BIT);
        emu.run();

        assert_eq!(emu.cpu.decimal_flag(), false);
    }

    #[test]
    fn test_cli() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::CLI);
        emu.cpu.set_status_flag(cpu::INTERRUPT_BIT);
        emu.run();

        assert_eq!(emu.cpu.interrupt_flag(), false);
    }

    #[test]
    fn test_clv() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::CLV);
        emu.cpu.set_status_flag(cpu::OVERFLOW_BIT);
        emu.run();

        assert_eq!(emu.cpu.overflow_flag(), false);
    }

    #[test]
    fn test_cmp_abs() {
        let start: u16 = memory::CODE_START_ADDR;
        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.mem.cpu_write(start, opcodes::CMP_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.mem.cpu_write(start, opcodes::CMP_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 1);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_cmp_abx() {
        let start: u16 = memory::CODE_START_ADDR;
        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ABX);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 1;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ABX);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ABX);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 1);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_cmp_aby() {
        let start: u16 = memory::CODE_START_ADDR;
        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ABY);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 1;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ABY);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ABY);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 1);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_cmp_imm() {
        let start: u16 = memory::CODE_START_ADDR;
        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_IMM);
        emu.mem.cpu_write(start + 1, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 1;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_IMM);
        emu.mem.cpu_write(start + 1, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_IMM);
        emu.mem.cpu_write(start + 1, 1);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_cmp_inx() {
        let start: u16 = memory::CODE_START_ADDR;
        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_INX);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x42, 0x11);
        emu.mem.cpu_write(0x43, 0x47);
        emu.mem.cpu_write(0x4711, 0);
        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 1;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_INX);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x42, 0x11);
        emu.mem.cpu_write(0x43, 0x47);
        emu.mem.cpu_write(0x4711, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_INX);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x42, 0x11);
        emu.mem.cpu_write(0x43, 0x47);
        emu.mem.cpu_write(0x4711, 1);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_cmp_iny() {
        let start: u16 = memory::CODE_START_ADDR;
        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::CMP_INY);
        emu.mem.cpu_write(start + 1, 0x42);
        emu.mem.cpu_write(0x42, 0x10);
        emu.mem.cpu_write(0x43, 0x47);
        emu.mem.cpu_write(0x4711, 0);
        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 1;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::CMP_INY);
        emu.mem.cpu_write(start + 1, 0x42);
        emu.mem.cpu_write(0x42, 0x10);
        emu.mem.cpu_write(0x43, 0x47);
        emu.mem.cpu_write(0x4711, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::CMP_INY);
        emu.mem.cpu_write(start + 1, 0x42);
        emu.mem.cpu_write(0x42, 0x10);
        emu.mem.cpu_write(0x43, 0x47);
        emu.mem.cpu_write(0x4711, 1);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_cmp_zpx() {
        let start: u16 = memory::CODE_START_ADDR;
        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ZPX);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x42, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 1;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ZPX);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x42, 0);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ZPX);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x42, 1);

        emu.run();

        assert_eq!(emu.cpu.carry_flag(), false);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_dec_zp() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::DEC_ZP);
        emu.mem.cpu_write(start + 1, 0x01);
        emu.mem.cpu_write(0x01, 0x00);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x01), 0xff);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.cpu_write(0x01, 1);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x01), 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_dcp_zp() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::DCP_ZP);
        emu.mem.cpu_write(start + 1, 0x01);
        emu.mem.cpu_write(0x01, 0x00);
        emu.cpu.a = 0xff;
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x01), 0xff);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
        assert_eq!(emu.cpu.carry_flag(), true);

        emu.cpu.pc = start;
        emu.cpu.a = 0;
        emu.mem.cpu_write(0x01, 0x00);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x01), 0xff);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);
        assert_eq!(emu.cpu.carry_flag(), false);

        emu.cpu.pc = start;
        emu.cpu.a = 1;
        emu.mem.cpu_write(0x01, 0x01);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x01), 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);
        assert_eq!(emu.cpu.carry_flag(), true);
    }

    #[test]
    fn test_eor_imm() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::EOR_IMM);
        emu.mem.cpu_write(start + 1, 0b1000_1000);
        emu.cpu.a = 0b0000_1000;
        emu.run();

        assert_eq!(emu.cpu.a, 0b1000_0000);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_inc_zp() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::INC_ZP);
        emu.mem.cpu_write(start + 1, 0x01);
        emu.mem.cpu_write(0x01, 0xff);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x01), 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.cpu_write(0x01, 127);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x01), 128);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_isb() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::ISB_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);

        emu.cpu.a = 0x20;
        emu.mem.cpu_write(0x4711, 0x10);

        emu.run();

        // carry = false => sub 1 extra
        assert_eq!(emu.cpu.a, 0xe);
        assert_eq!(emu.mem.cpu_read(0x4711), 0x11);
    }

    #[test]
    fn test_jmp_abs() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::JMP_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.run();

        assert_eq!(emu.cpu.pc, 0x4711);
    }

    #[test]
    fn test_jmp_ind() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::JMP_IND);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0x42);

        emu.run();

        assert_eq!(emu.cpu.pc, 0x42);
    }

    #[test]
    fn test_jmp_ind_last_byte() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::JMP_IND);
        emu.mem.cpu_write(start + 1, 0xff);
        // 6502 JMP indirect: at page offset $FF, the high byte is read from $xx00, not $xx+100.
        emu.mem.cpu_write(start + 2, 0x10);
        emu.mem.cpu_write(0x10ff, 0x12);
        emu.mem.cpu_write(0x1000, 0x47);

        // should not be used (would be the hi byte if the CPU did not wrap within the page)
        emu.mem.cpu_write(0x1100, 0x11);

        emu.run();

        assert_eq!(emu.cpu.pc, 0x4712);
    }

    #[test]
    fn test_jsr() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::JSR_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);

        emu.run();

        assert_eq!(emu.cpu.pc, 0x4711);
        assert_eq!(emu.cpu.sp, 0xfd);
        assert_eq!(
            emu.mem.cpu_read(0x1ff),
            (memory::CODE_START_ADDR >> 8) as u8
        );
        assert_eq!(emu.mem.cpu_read(0x1fe), 0x02);
    }

    #[test]
    fn test_lda_abs() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::LDA_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0x42);
        emu.run();

        assert_eq!(emu.cpu.a, 0x042);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_lax_abs() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::LAX_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0x42);
        emu.run();

        assert_eq!(emu.cpu.a, 0x042);
        assert_eq!(emu.cpu.x, 0x042);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_lda_abs_flags() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::LDA_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 255);
        emu.run();

        assert_eq!(emu.cpu.a, 255);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.cpu_write(0x4711, 0);
        emu.run();

        assert_eq!(emu.cpu.a, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_lda_abx() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::LDA_ABX);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0x42);
        emu.run();

        assert_eq!(emu.cpu.a, 0x042);
    }

    #[test]
    fn test_lda_aby() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::LDA_ABY);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0x42);
        emu.run();

        assert_eq!(emu.cpu.a, 0x42);
    }

    #[test]
    fn test_lax_aby() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::LAX_ABY);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0x42);
        emu.run();

        assert_eq!(emu.cpu.a, 0x42);
        assert_eq!(emu.cpu.x, 0x42);
    }

    #[test]
    fn test_lda_imm() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::LDA_IMM);
        emu.mem.cpu_write(start + 1, 0x42);
        emu.run();

        assert_eq!(emu.cpu.a, 0x42);
    }

    #[test]
    fn test_lda_inx() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::LDA_INX);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x42, 0x15);
        emu.mem.cpu_write(0x15, 0x12);
        emu.run();

        assert_eq!(emu.cpu.a, 0x12);
    }

    #[test]
    fn test_lax_inx() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::LAX_INX);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x42, 0x15);
        emu.mem.cpu_write(0x15, 0x12);
        emu.run();

        assert_eq!(emu.cpu.a, 0x12);
        assert_eq!(emu.cpu.x, 0x12);
    }

    #[test]
    fn test_lda_iny() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::LDA_INY);
        emu.mem.cpu_write(start + 1, 0x16);
        emu.mem.cpu_write(0x16, 0x10);
        emu.mem.cpu_write(0x17, 0x42);
        emu.mem.cpu_write(0x4211, 0x11);
        emu.run();

        assert_eq!(emu.cpu.a, 0x11);
    }

    #[test]
    fn test_lax_iny() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::LAX_INY);
        emu.mem.cpu_write(start + 1, 0x16);
        emu.mem.cpu_write(0x16, 0x10);
        emu.mem.cpu_write(0x17, 0x42);
        emu.mem.cpu_write(0x4211, 0x11);
        emu.run();

        assert_eq!(emu.cpu.a, 0x11);
        assert_eq!(emu.cpu.x, 0x11);
    }

    #[test]
    fn test_lda_iny_wrap() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.y = 0x1;
        emu.mem.cpu_write(start, opcodes::LDA_INY);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x41, 0xff);
        emu.mem.cpu_write(0x42, 0x41);
        emu.mem.cpu_write(0x4200, 0x47);
        emu.run();

        assert_eq!(emu.cpu.a, 0x47);
    }

    #[test]
    fn test_lda_zp() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::LDA_ZP);
        emu.mem.cpu_write(start + 1, 0x42);
        emu.mem.cpu_write(0x42, 0x15);
        emu.run();

        assert_eq!(emu.cpu.a, 0x15);
    }

    #[test]
    fn test_lax_zp() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::LAX_ZP);
        emu.mem.cpu_write(start + 1, 0x42);
        emu.mem.cpu_write(0x42, 0x15);
        emu.run();

        assert_eq!(emu.cpu.a, 0x15);
        assert_eq!(emu.cpu.x, 0x15);
    }

    #[test]
    fn test_lax_zpy() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.y = 8;
        emu.mem.cpu_write(start, opcodes::LAX_ZPY);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x41 + 8, 0x15);
        emu.run();

        assert_eq!(emu.cpu.a, 0x15);
        assert_eq!(emu.cpu.x, 0x15);
    }

    #[test]
    fn test_lda_zpx() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::LDA_ZPX);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x42, 0x15);
        emu.run();

        assert_eq!(emu.cpu.a, 0x15);
    }

    #[test]
    fn test_ldx_aby() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::LDX_ABY);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0x15);
        emu.run();

        assert_eq!(emu.cpu.x, 0x15);
    }

    #[test]
    fn test_ldx_zp() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::LDX_ZP);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x41, 0x15);
        emu.run();

        assert_eq!(emu.cpu.x, 0x15);
    }

    #[test]
    fn test_ldx_zp_flags() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::LDX_ZP);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x41, 255);
        emu.run();

        assert_eq!(emu.cpu.x, 255);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.cpu_write(0x41, 0);
        emu.run();

        assert_eq!(emu.cpu.x, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_ldx_zpy() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 1;
        emu.cpu.y = 8;
        emu.mem.cpu_write(start, opcodes::LDX_ZPY);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x41 + 8, 0x15);
        emu.run();

        assert_eq!(emu.cpu.x, 0x15);
    }

    #[test]
    fn test_ldy_abs() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::LDY_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0x15);
        emu.run();

        assert_eq!(emu.cpu.y, 0x15);
    }

    #[test]
    fn test_ldy_abx() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::LDY_ABX);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0x15);
        emu.run();

        assert_eq!(emu.cpu.y, 0x15);
    }

    #[test]
    fn test_ldy_zp() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::LDY_ZP);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(0x10, 0x15);
        emu.run();

        assert_eq!(emu.cpu.y, 0x15);
    }

    #[test]
    fn test_ldy_zpx() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 8;

        emu.mem.cpu_write(start, opcodes::LDY_ZPX);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(0x18, 0x15);

        emu.run();

        assert_eq!(emu.cpu.y, 0x15);
    }

    #[test]
    fn test_ldy_imm() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::LDY_IMM);
        emu.mem.cpu_write(start + 1, 0x15);
        emu.run();

        assert_eq!(emu.cpu.y, 0x15);
    }

    #[test]
    fn test_lsr() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 3;
        emu.mem.cpu_write(start, opcodes::LSR);
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
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::NOP);

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
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0b1000_0000;
        emu.mem.cpu_write(start, opcodes::ORA_IMM);
        emu.mem.cpu_write(start + 1, 0b0000_0001);

        emu.run();
        assert_eq!(emu.cpu.a, 129);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.cpu.a = 0;
        emu.mem.cpu_write(start, opcodes::ORA_IMM);
        emu.mem.cpu_write(start + 1, 0);

        emu.run();
        assert_eq!(emu.cpu.a, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_pha() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0x42;
        emu.mem.cpu_write(start, opcodes::PHA);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x1ff), 0x42);
        assert_eq!(emu.cpu.sp, 0xfe);
    }

    #[test]
    fn test_pla() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.sp -= 1;
        emu.mem.cpu_write(0x1ff, 0x42);
        emu.mem.cpu_write(start, opcodes::PLA);
        emu.run();

        assert_eq!(emu.cpu.a, 0x42);
        assert_eq!(emu.cpu.sp, 0xff);
    }

    #[test]
    fn test_php() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.set_status_flag(cpu::CARRY_BIT);
        emu.mem.cpu_write(start, opcodes::PHP);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x1ff), 0b0011_0001);
        assert_eq!(emu.cpu.sp, 0xfe);
    }

    #[test]
    fn test_plp() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.sp -= 1;
        emu.mem.cpu_write(0x1ff, 0x1);
        emu.mem.cpu_write(start, opcodes::PLP);
        emu.run();

        assert_eq!(emu.cpu.status, 0b0010_0001);
        assert_eq!(emu.cpu.sp, 0xff);
    }

    #[test]
    fn test_plp_overflow() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::PLP);
        emu.mem.cpu_write(0x100, 0x2);

        emu.run();

        assert_eq!(emu.cpu.status, 0b0010_0010);
        assert_eq!(emu.cpu.sp, 0x0);
        assert_eq!(emu.cpu.overflow_flag(), false);
    }

    #[test]
    fn test_rla() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0b0000_1110;
        emu.mem.cpu_write(start, opcodes::RLA_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0b0111_1000);
        emu.cpu.set_status_flag(cpu::CARRY_BIT);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x4711), 0b1111_0001);
        assert_eq!(emu.cpu.a, 0b0000_0000);
    }

    #[test]
    fn test_rra() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::RRA_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);

        emu.cpu.a = 0x20;
        emu.mem.cpu_write(0x4711, 0b0000_0010);

        emu.run();

        assert_eq!(emu.cpu.a, 0x21);
        assert_eq!(emu.mem.cpu_read(0x4711), 0x1);
    }

    #[test]
    fn test_rti() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::RTI);
        emu.mem.cpu_write(0x1ff, 0x6);
        emu.mem.cpu_write(0x1fe, 0x8);
        emu.mem.cpu_write(0x1fd, 0b1011_0000);
        emu.cpu.set_status_flag(cpu::INTERRUPT_BIT);
        emu.cpu.sp = 0xfc;
        emu.run();

        assert_eq!(emu.cpu.status, 0b1010_0000);
        assert_eq!(emu.cpu.pc, 0x608);
    }

    #[test]
    fn test_rts() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::RTS);
        emu.cpu.sp = 0xfd;
        emu.mem
            .cpu_write(0x1ff, (memory::CODE_START_ADDR >> 8) as u8);
        emu.mem.cpu_write(0x1fe, 0x01);

        emu.run();
        assert_eq!(emu.cpu.pc, 0x0602);
        assert_eq!(emu.cpu.sp, 0xff);
    }

    #[test]
    fn test_sax_abs() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0b1111_0000;
        emu.cpu.x = 0b0001_1111;
        emu.mem.cpu_write(start, opcodes::SAX_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x4711), 0b0001_0000);
    }

    #[test]
    fn test_sax_inx() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0b1111_0001;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::SAX_INX);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(0x11, 0x42);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x42), 0b0000_0001);
    }

    #[test]
    fn test_sax_zp() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0b1111_0000;
        emu.cpu.x = 0b0001_1111;
        emu.mem.cpu_write(start, opcodes::SAX_ZP);
        emu.mem.cpu_write(start + 1, 0x42);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x42), 0b0001_0000);
    }

    #[test]
    fn test_sax_zpy() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0b1111_0000;
        emu.cpu.x = 0b0001_1111;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::SAX_ZPY);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x42), 0b0001_0000);
    }

    #[test]
    fn test_slo() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0b0000_1111;
        emu.mem.cpu_write(start, opcodes::SLO_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0b0111_1000);

        emu.run();

        assert_eq!(emu.cpu.a, 0b1111_1111);
        assert_eq!(emu.mem.cpu_read(0x4711), 0b1111_0000);
    }

    #[test]
    fn test_sre() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0b1111_1111;
        emu.mem.cpu_write(start, opcodes::SRE_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0b0001_1110);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x4711), 0b0000_1111);
        assert_eq!(emu.cpu.a, 0b1111_0000);
    }

    #[test]
    fn test_sta_abx() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0x42;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::STA_ABX);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x4711), 0x42);
    }

    #[test]
    fn test_sta_aby() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0x42;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::STA_ABY);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x4711), 0x42);
    }

    #[test]
    fn test_sta_inx() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0x42;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::STA_INX);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(0x11, 0x42);

        emu.run();

        assert_eq!(emu.mem.cpu_read(0x42), 0x42);
    }

    #[test]
    fn test_sta_iny() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0x42;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::STA_INY);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(0x10, 0x41);

        emu.run();

        assert_eq!(emu.mem.cpu_read(0x42), 0x42);
    }

    #[test]
    fn test_sta_iny_wrap() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.a = 0x42;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::STA_INY);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(0x10, 0xff);
        emu.mem.cpu_write(0x11, 0x0);

        emu.run();

        assert_eq!(emu.mem.cpu_read(0x100), 0x42);
    }

    #[test]
    fn test_sta_zpx() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 0x01;
        emu.cpu.a = 0x42;
        emu.mem.cpu_write(start, opcodes::STA_ZPX);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x11), 0x42);
    }

    #[test]
    fn test_stx_zp() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 0x42;
        emu.mem.cpu_write(start, opcodes::STX_ZP);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x11), 0x42);
    }

    #[test]
    fn test_stx_zpy() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 0x42;
        emu.cpu.y = 0x1;

        emu.mem.cpu_write(start, opcodes::STX_ZPY);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x11), 0x42);
    }

    #[test]
    fn test_sty_zp() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.y = 0x42;
        emu.mem.cpu_write(start, opcodes::STY_ZP);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x11), 0x42);
    }

    #[test]
    fn test_sty_zpx() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.y = 0x42;
        emu.cpu.x = 0x1;

        emu.mem.cpu_write(start, opcodes::STY_ZPX);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x11), 0x42);
    }

    #[test]
    fn test_sec() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::SEC);
        emu.run();

        assert_eq!(emu.cpu.carry_flag(), true);
    }

    #[test]
    fn test_sed() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::SED);
        emu.run();

        assert_eq!(emu.cpu.decimal_flag(), true);
    }

    #[test]
    fn test_sei() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::SEI);
        emu.run();

        assert_eq!(emu.cpu.interrupt_flag(), true);
    }

    #[test]
    fn test_tsx() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.sp = 255;
        emu.mem.cpu_write(start, opcodes::TSX);
        emu.run();

        assert_eq!(emu.cpu.x, 255);
        assert_eq!(emu.cpu.negative_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), false);

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.sp = 0;
        emu.mem.cpu_write(start, opcodes::TSX);
        emu.run();

        assert_eq!(emu.cpu.x, 0);
        assert_eq!(emu.cpu.negative_flag(), false);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_txs() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 255;
        emu.mem.cpu_write(start, opcodes::TXS);
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

    #[test]
    fn test_ppu_warmup_blocks_cpu_writes_until_deadline() {
        use crate::emu::ppu;
        let mut emu = Emulator::_new();
        assert_eq!(emu.ppu_register_warmup_until_cpu_cycle, 29_658);
        emu.test_cpu_write(ppu::CTRL_REG_ADDR, 0xFF);
        assert_eq!(emu.ppu.ppu_ctrl, 0);
        emu.cpu.cycle = 29_658;
        emu.test_cpu_write(ppu::CTRL_REG_ADDR, 0x80);
        assert_eq!(emu.ppu.ppu_ctrl, 0x80);
    }
}
