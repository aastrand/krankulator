pub mod apu;
pub mod audio;
pub mod cpu;
pub mod dbg;
pub mod gfx;
pub mod io;
pub mod memory;
pub mod ppu;
pub mod savestate;

#[cfg(test)]
mod integration_tests;

use cpu::opcodes;

use std::collections::HashSet;

use self::audio::{AudioBackend, CapturingAudioOutput, SilentAudioOutput};

// CPU initialization values (per NES power-on state)
const CPU_INIT_SP: u8 = 0xFD;
const CPU_INIT_STATUS: u8 = 0x34;

// PPU register warmup period — writes to PPU registers are ignored for this many CPU cycles after power-on/reset
const PPU_WARMUP_CPU_CYCLES: u64 = 29_658;

// Input polling interval (~1000 Hz: 1,789,773 CPU Hz / 1790 ≈ 1ms)
const INPUT_POLL_INTERVAL: u64 = 1790;

// NES memory-mapped I/O addresses
const APU_STATUS_ADDR: u16 = 0x4015;
const CONTROLLER1_ADDR: u16 = 0x4016;
const CONTROLLER2_ADDR: u16 = 0x4017;
const APU_REG_START: u16 = 0x4000;
const IO_EXPANSION_START: u16 = 0x4018;
const IO_EXPANSION_END: u16 = 0x4100;
const MAPPER_START: u16 = 0x4020;

// Controller open bus behavior
const OPEN_BUS_UPPER_MASK: u8 = 0xE0;
const CONTROLLER_DATA_MASK: u8 = 0x1F;

// Save state slot count
const SAVESTATE_SLOT_COUNT: u8 = 4;

#[derive(PartialEq)]
pub enum CycleState {
    CpuAhead,
    CpuExecuted,
    Exiting,
}

#[derive(Clone, Copy, PartialEq)]
enum CpuPhase {
    Fetch,
    Execute,
    Interrupt,
    Dma,
}

#[derive(Clone, Copy, PartialEq)]
enum InterruptKind {
    Nmi,
    Irq,
    Brk,
}

#[derive(Clone, Copy, PartialEq)]
enum InstrType {
    Read,
    Write,
    Rmw,
    Implied,
    Accumulator,
    Branch,
    Push,
    Pull,
    Jsr,
    Rts,
    Rti,
    Brk,
    JmpAbs,
    JmpInd,
}

fn classify_instr_type(opcode: u8) -> InstrType {
    match opcode {
        opcodes::BPL | opcodes::BMI | opcodes::BVC | opcodes::BVS |
        opcodes::BCC | opcodes::BCS | opcodes::BEQ | opcodes::BNE => InstrType::Branch,

        opcodes::PHA | opcodes::PHP => InstrType::Push,
        opcodes::PLA | opcodes::PLP => InstrType::Pull,
        opcodes::JSR_ABS => InstrType::Jsr,
        opcodes::RTS => InstrType::Rts,
        opcodes::RTI => InstrType::Rti,
        opcodes::BRK => InstrType::Brk,
        opcodes::JMP_ABS => InstrType::JmpAbs,
        opcodes::JMP_IND => InstrType::JmpInd,

        opcodes::ASL | opcodes::LSR | opcodes::ROL | opcodes::ROR => InstrType::Accumulator,

        opcodes::CLC | opcodes::SEC | opcodes::CLI | opcodes::SEI |
        opcodes::CLV | opcodes::CLD | opcodes::SED |
        opcodes::TAX | opcodes::TXA | opcodes::TAY | opcodes::TYA |
        opcodes::TSX | opcodes::TXS |
        opcodes::DEX | opcodes::DEY | opcodes::INX | opcodes::INY |
        opcodes::NOP |
        opcodes::NOP_1A | opcodes::NOP_3A | opcodes::NOP_5A |
        opcodes::NOP_7A | opcodes::NOP_DA | opcodes::NOP_FA => InstrType::Implied,

        opcodes::STA_ZP | opcodes::STA_ZPX | opcodes::STA_ABS |
        opcodes::STA_ABX | opcodes::STA_ABY | opcodes::STA_INX | opcodes::STA_INY |
        opcodes::STX_ZP | opcodes::STX_ZPY | opcodes::STX_ABS |
        opcodes::STY_ZP | opcodes::STY_ZPX | opcodes::STY_ABS |
        opcodes::SAX_ZP | opcodes::SAX_ZPY | opcodes::SAX_ABS | opcodes::SAX_INX |
        opcodes::SHA_INY | opcodes::SHA_ABY | opcodes::TAS_ABY |
        opcodes::SHY_ABX | opcodes::SHX_ABY => InstrType::Write,

        opcodes::ASL_ZP | opcodes::ASL_ZPX | opcodes::ASL_ABS | opcodes::ASL_ABX |
        opcodes::LSR_ZP | opcodes::LSR_ZPX | opcodes::LSR_ABS | opcodes::LSR_ABX |
        opcodes::ROL_ZP | opcodes::ROL_ZPX | opcodes::ROL_ABS | opcodes::ROL_ABX |
        opcodes::ROR_ZP | opcodes::ROR_ZPX | opcodes::ROR_ABS | opcodes::ROR_ABX |
        opcodes::INC_ZP | opcodes::INC_ZPX | opcodes::INC_ABS | opcodes::INC_ABX |
        opcodes::DEC_ZP | opcodes::DEC_ZPX | opcodes::DEC_ABS | opcodes::DEC_ABX |
        opcodes::DCP_ZP | opcodes::DCP_ZPX | opcodes::DCP_ABS | opcodes::DCP_ABX |
        opcodes::DCP_ABY | opcodes::DCP_INX | opcodes::DCP_INY |
        opcodes::ISB_ZP | opcodes::ISB_ZPX | opcodes::ISB_ABS | opcodes::ISB_ABX |
        opcodes::ISB_ABY | opcodes::ISB_INX | opcodes::ISB_INY |
        opcodes::SLO_ZP | opcodes::SLO_ZPX | opcodes::SLO_ABS | opcodes::SLO_ABX |
        opcodes::SLO_ABY | opcodes::SLO_INX | opcodes::SLO_INY |
        opcodes::SRE_ZP | opcodes::SRE_ZPX | opcodes::SRE_ABS | opcodes::SRE_ABX |
        opcodes::SRE_ABY | opcodes::SRE_INX | opcodes::SRE_INY |
        opcodes::RLA_ZP | opcodes::RLA_ZPX | opcodes::RLA_ABS | opcodes::RLA_ABX |
        opcodes::RLA_ABY | opcodes::RLA_INX | opcodes::RLA_INY |
        opcodes::RRA_ZP | opcodes::RRA_ZPX | opcodes::RRA_ABS | opcodes::RRA_ABX |
        opcodes::RRA_ABY | opcodes::RRA_INX | opcodes::RRA_INY => InstrType::Rmw,

        _ => InstrType::Read,
    }
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

    pub instructions: u64,
    pub cycles: u64,
    pub master_clock: u64,
    cpu_open_bus: u8,

    should_trigger_nmi: bool,

    cpu_phase: CpuPhase,
    cpu_step: u8,
    cpu_opcode: u8,
    cpu_instr_pc: u16,
    cpu_addr: u16,
    cpu_data: u8,
    cpu_addr_lo: u8,
    cpu_page_crossed: bool,
    pending_nmi: bool,
    pending_irq: bool,
    irq_i_flag_sampled: bool,
    interrupt_kind: InterruptKind,
    interrupt_vector: u16,
    dma_pending: bool,
    dma_page: u8,
    dma_base: u16,
    dma_offset: u16,
    dma_value: u8,
    dma_align_remaining: u8,
    dma_cycle: u16,
    instruction_just_finished: bool,
    pub audio: Box<dyn AudioBackend>,
    /// Until this CPU cycle count (exclusive), writes to PPU registers and OAM DMA are ignored.
    ppu_register_warmup_until_cpu_cycle: u64,
    rom_path: Option<String>,
    savestate_slot: u8,
    last_rendered_frame: u64,
    pub overlay: gfx::overlay::Overlay,
    pending_open_rom: Option<String>,
}

impl Emulator {
    pub fn new_headless(mapper: Box<dyn memory::MemoryMapper>) -> Emulator {
        let audio = Box::new(SilentAudioOutput::new()) as Box<dyn AudioBackend>;
        let iohandler = Box::new(io::HeadlessIOHandler {});

        Emulator::new_with(iohandler, mapper, audio)
    }

    pub fn new_capturing(mapper: Box<dyn memory::MemoryMapper>) -> Emulator {
        let audio = Box::new(CapturingAudioOutput::new()) as Box<dyn AudioBackend>;
        let iohandler = Box::new(io::HeadlessIOHandler {});
        Emulator::new_with(iohandler, mapper, audio)
    }

    pub fn drain_captured_audio(&mut self) -> Vec<f32> {
        self.audio.drain_captured()
    }

    pub fn _new() -> Emulator {
        let mapper = Box::new(memory::IdentityMapper::new(memory::CODE_START_ADDR));
        Emulator::new_headless(mapper)
    }

    pub fn new_with(
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
            instructions: 0,
            cycles: 0,
            master_clock: 0,
            cpu_open_bus: 0,
            should_trigger_nmi: false,
            cpu_phase: CpuPhase::Fetch,
            cpu_step: 0,
            cpu_opcode: 0,
            cpu_instr_pc: 0,
            cpu_addr: 0,
            cpu_data: 0,
            cpu_addr_lo: 0,
            cpu_page_crossed: false,
            pending_nmi: false,
            pending_irq: false,
            irq_i_flag_sampled: true,
            interrupt_kind: InterruptKind::Nmi,
            interrupt_vector: 0,
            dma_pending: false,
            dma_page: 0,
            dma_base: 0,
            dma_offset: 0,
            dma_value: 0,
            dma_align_remaining: 0,
            dma_cycle: 0,
            instruction_just_finished: false,
            audio: audio,
            // https://www.nesdev.org/wiki/PPU_power_up_state — model as ignoring host writes briefly after power on.
            ppu_register_warmup_until_cpu_cycle: PPU_WARMUP_CPU_CYCLES,
            rom_path: None,
            savestate_slot: 0,
            last_rendered_frame: 0,
            overlay: gfx::overlay::Overlay::new(),
            pending_open_rom: None,
        }
    }

    pub fn set_rom_path(&mut self, path: &str) {
        self.rom_path = Some(path.to_string());
    }

    pub fn take_pending_open_rom(&mut self) -> Option<String> {
        self.pending_open_rom.take()
    }

    pub fn load_rom(&mut self, mapper: Box<dyn memory::MemoryMapper>, path: &str) {
        self.mem = mapper;
        self.rom_path = Some(path.to_string());
        self.cpu.pc = self.mem.code_start();
        self.cpu.sp = CPU_INIT_SP;
        self.cpu.status = CPU_INIT_STATUS;
        self.cpu.a = 0;
        self.cpu.x = 0;
        self.cpu.y = 0;
        self.cpu.cycle = 0;
        self.cycles = 0;
        self.master_clock = 0;
        self.instructions = 0;
        self.ppu = ppu::PPU::new();
        self.apu.reset();
        self.audio.clear();
        self.cpu_phase = CpuPhase::Fetch;
        self.cpu_step = 0;
        self.pending_nmi = false;
        self.pending_irq = false;
        self.dma_pending = false;
        self.ppu_register_warmup_until_cpu_cycle = 29_658;
        self.savestate_slot = 0;
        self.last_rendered_frame = 0;
        self.overlay.set_banner(None);
        self.should_trigger_nmi = true;
        self.should_exit_on_infinite_loop = false;
    }

    pub fn toggle_verbose_mode(&mut self, verbose: bool) {
        self.verbose = verbose
    }

    #[allow(dead_code)] // Only used by tests
    pub fn toggle_quiet_mode(&mut self, quiet_mode: bool) {
        self.should_log = !quiet_mode;
    }

    pub fn init(&mut self) -> Result<(), String> {
        self.iohandler.init()
    }

    pub fn toggle_debug_on_infinite_loop(&mut self, debug: bool) {
        self.should_debug_on_infinite_loop = debug
    }

    pub fn toggle_should_exit_on_infinite_loop(&mut self, exit: bool) {
        self.should_exit_on_infinite_loop = exit;
    }

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
        self.ppu_register_warmup_until_cpu_cycle =
            self.cpu.cycle.wrapping_add(PPU_WARMUP_CPU_CYCLES);
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
        w.write_u64(0); // legacy: instruction_start_dot
        w.write_u8(0); // legacy: cpu_bus_cycle_offset
        w.write_bool(self.should_trigger_nmi);
        w.write_i8(if self.pending_nmi { 0 } else { -1 }); // legacy: nmi_countdown
        w.write_u64(self.ppu_register_warmup_until_cpu_cycle);
        w.write_u64(self.instructions);
        // v7: state machine fields
        w.write_u8(self.cpu_phase as u8);
        w.write_u8(self.cpu_step);
        w.write_u8(self.cpu_opcode);
        w.write_u16(self.cpu_instr_pc);
        w.write_u16(self.cpu_addr);
        w.write_u8(self.cpu_data);
        w.write_u8(self.cpu_addr_lo);
        w.write_bool(self.cpu_page_crossed);
        w.write_bool(self.pending_nmi);
        w.write_bool(self.pending_irq);
        w.write_bool(self.irq_i_flag_sampled);
        w.write_u8(self.interrupt_kind as u8);
        w.write_bool(self.dma_pending);
        w.write_u8(self.dma_page);
        w.write_u16(self.dma_base);
        w.write_u16(self.dma_offset);
        w.write_u8(self.dma_value);
        w.write_u8(self.dma_align_remaining);
        w.write_u16(self.dma_cycle);
        w.finish()
    }

    pub fn load_state_from_bytes(&mut self, data: &[u8]) -> std::io::Result<()> {
        let mut r = savestate::SavestateReader::new(data)?;
        self.load_state_from_reader(&mut r)?;
        self.audio.clear();
        self.last_rendered_frame = self.ppu.frames.saturating_sub(1);
        Ok(())
    }

    pub fn save_state_to_file(&mut self) {
        let path = self.savestate_path();
        let data = self.save_state_to_bytes();
        match std::fs::write(&path, &data) {
            Ok(()) => {
                self.overlay.toast("STATE SAVED".into());
            }
            Err(e) => {
                self.overlay.toast(format!("SAVE FAILED: {}", e));
            }
        }
    }

    pub fn load_state_from_file(&mut self) {
        let path = self.savestate_path();
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                self.overlay.toast(format!("LOAD FAILED: {}", e));
                return;
            }
        };
        if let Err(e) = self.load_state_from_bytes(&data) {
            self.overlay.toast(format!("LOAD FAILED: {}", e));
            return;
        }
        self.overlay.toast("STATE LOADED".into());
    }

    fn load_state_from_reader(
        &mut self,
        r: &mut savestate::SavestateReader,
    ) -> std::io::Result<()> {
        self.cpu.load_state(r)?;
        self.ppu.load_state(r)?;
        self.apu.load_state(r)?;
        let mapper_id = r.read_u8()?;
        if mapper_id != self.mem.mapper_id() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "mapper mismatch: save={} current={}",
                    mapper_id,
                    self.mem.mapper_id()
                ),
            ));
        }
        self.mem.load_state(r)?;
        self.cycles = r.read_u64()?;
        self.master_clock = r.read_u64()?;
        let _legacy_instruction_start_dot = r.read_u64()?;
        let _legacy_cpu_bus_cycle_offset = r.read_u8()?;
        self.should_trigger_nmi = r.read_bool()?;
        let legacy_nmi_countdown = r.read_i8()?;
        self.ppu_register_warmup_until_cpu_cycle = r.read_u64()?;
        self.instructions = r.read_u64()?;
        if r.version() >= 7 {
            self.cpu_phase = match r.read_u8()? {
                1 => CpuPhase::Execute,
                2 => CpuPhase::Interrupt,
                3 => CpuPhase::Dma,
                _ => CpuPhase::Fetch,
            };
            self.cpu_step = r.read_u8()?;
            self.cpu_opcode = r.read_u8()?;
            self.cpu_instr_pc = r.read_u16()?;
            self.cpu_addr = r.read_u16()?;
            self.cpu_data = r.read_u8()?;
            self.cpu_addr_lo = r.read_u8()?;
            self.cpu_page_crossed = r.read_bool()?;
            self.pending_nmi = r.read_bool()?;
            self.pending_irq = r.read_bool()?;
            self.irq_i_flag_sampled = r.read_bool()?;
            self.interrupt_kind = match r.read_u8()? {
                1 => InterruptKind::Irq,
                2 => InterruptKind::Brk,
                _ => InterruptKind::Nmi,
            };
            self.dma_pending = r.read_bool()?;
            self.dma_page = r.read_u8()?;
            self.dma_base = r.read_u16()?;
            self.dma_offset = r.read_u16()?;
            self.dma_value = r.read_u8()?;
            self.dma_align_remaining = r.read_u8()?;
            self.dma_cycle = r.read_u16()?;
        } else {
            // v6 and earlier: load at instruction boundary
            self.cpu_phase = CpuPhase::Fetch;
            self.cpu_step = 0;
            self.pending_nmi = legacy_nmi_countdown >= 0;
            self.pending_irq = false;
        }
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
        self.cpu_write_cycle(addr, value);
    }

    #[cfg(test)]
    pub fn run_for_cycles(&mut self, max_cycles: u64) {
        match self.iohandler.init() {
            Err(msg) => self.iohandler.log(msg),
            _ => {}
        }
        let start = self.cycles;
        loop {
            if self.cycles - start >= max_cycles {
                break;
            }
            if self.cycle() == CycleState::Exiting {
                break;
            }
        }
    }

    pub fn run(&mut self) {
        match self.iohandler.init() {
            Err(msg) => self.iohandler.log(msg),
            _ => {}
        }

        self.cpu.last_instruction = 0xffff;
        loop {
            if self.cycle() == CycleState::Exiting {
                break;
            }
        }

        self.exit();
    }

    pub fn run_one_frame(&mut self) -> bool {
        let target_frame = self.ppu.frames + 1;
        while self.ppu.frames < target_frame {
            if self.cycle() == CycleState::Exiting {
                return false;
            }
        }
        self.audio.flush();
        true
    }

    pub fn cycle(&mut self) -> CycleState {
        let mut state = CycleState::CpuAhead;

        match self.cpu_phase {
            CpuPhase::Fetch => {
                state = CycleState::CpuExecuted;

                if !self.breakpoints.is_empty() && self.breakpoints.contains(&self.cpu.pc) {
                    self.stepping = true;
                }

                if self.stepping {
                    self.debug();
                    if self.stepping {
                        state = CycleState::Exiting;
                    }
                }

                if (self.should_exit_on_infinite_loop || self.should_debug_on_infinite_loop)
                    && self.cpu.pc == self.cpu.last_instruction
                {
                    if self.mem.cpu_read(self.cpu.last_instruction as _) != opcodes::BRK {
                        let msg = format!("infite loop detected on addr 0x{:x}!", self.cpu.pc);
                        self.iohandler.log(msg);
                        if self.should_debug_on_infinite_loop {
                            self.debug();
                        }
                        if self.should_exit_on_infinite_loop {
                            state = CycleState::Exiting
                        }
                    } else if self.should_exit_on_infinite_loop {
                        self.iohandler.log(format!("reached probable end of code"));
                        state = CycleState::Exiting
                    }
                }

                if state != CycleState::Exiting
                    && self.should_exit_on_infinite_loop
                    && self.mem.cpu_read(self.cpu.pc) == opcodes::BRK
                {
                    let brk_vector = self.mem.get_16b_addr(memory::BRK_TARGET_ADDR);
                    if brk_vector == 0 {
                        state = CycleState::Exiting;
                    }
                }

                if state == CycleState::Exiting {
                    // Don't start the next instruction if we're exiting
                } else if self.dma_pending {
                    self.dma_pending = false;
                    self.start_dma();
                    self.step_dma();
                } else if self.pending_nmi {
                    self.pending_nmi = false;
                    self.start_interrupt(InterruptKind::Nmi);
                    self.step_interrupt();
                } else if self.pending_irq {
                    self.pending_irq = false;
                    self.start_interrupt(InterruptKind::Irq);
                    self.step_interrupt();
                } else {
                    self.step_fetch();
                }
            }
            CpuPhase::Execute => {
                self.step_execute();
            }
            CpuPhase::Interrupt => {
                self.step_interrupt();
            }
            CpuPhase::Dma => {
                self.step_dma();
            }
        }

        // PPU steps 3 dots
        let target_dot = self.master_clock + 3;
        self.sync_ppu_to_dot(target_dot);
        self.master_clock = target_dot;

        // Cycle the APU and mapper
        self.apu.cycle(self.master_clock, &mut *self.mem);
        self.mem.cpu_cycle();
        let samples = self.apu.get_audio_samples();
        if !samples.is_empty() {
            self.audio.push_samples(samples);
        }

        // NMI edge detection: consume rising edge recorded by PPU
        if let Some(_edge_dot) = self.ppu.nmi_rising_edge_dot.take() {
            if self.should_trigger_nmi {
                self.pending_nmi = true;
            }
        }

        if self.ppu.frames > self.last_rendered_frame {
            self.last_rendered_frame = self.ppu.frames;
            if let Some(ms) = self.iohandler.frame_time_ms() {
                self.overlay.set_frame_time(ms);
            }
            self.overlay.tick();
            self.overlay.draw(&mut self.buf);
            self.iohandler.render(&self.buf);
        }
        // ~1000 Hz input polling (1,789,773 CPU Hz / 1790 ≈ 1 ms)
        if self.cycles % INPUT_POLL_INTERVAL == 0 {
            let result = self.iohandler.poll(&mut *self.mem, &mut self.apu);
            if result.exit {
                state = CycleState::Exiting;
            }
            if result.reset {
                self.reset();
            }
            if self.rom_path.is_some() {
                if result.save_state {
                    self.save_state_to_file();
                }
                if result.load_state {
                    self.load_state_from_file();
                }
                if result.cycle_slot {
                    self.savestate_slot = (self.savestate_slot + 1) % SAVESTATE_SLOT_COUNT;
                    self.overlay.toast(format!("SLOT {}", self.savestate_slot));
                }
            }
            if result.toggle_overlay {
                self.overlay.toggle();
            }
            if result.open_rom.is_some() {
                self.pending_open_rom = result.open_rom;
                state = CycleState::Exiting;
            }
            for msg in result.toasts {
                self.overlay.toast(msg);
            }
        }

        self.cpu.cycle = self.cycles;
        self.cycles += 1;

        state
    }

    // ── Per-cycle state machine ──────────────────────────────────────

    fn cpu_read_cycle(&mut self, addr: u16) -> u8 {
        self.sync_ppu_for_access(addr, false);
        let ppu_reg = self.ppu_reg_cpu_addr(addr);
        let value = if let Some(reg) = ppu_reg {
            let v = self.ppu.read(reg, &*self.mem);
            if reg == ppu::DATA_ADDR {
                let a = self.ppu.get_current_vram_addr();
                self.mem.ppu_a12_transition(a, self.ppu.last_synced_dot);
            }
            v
        } else if addr == APU_STATUS_ADDR {
            self.apu.read(addr)
        } else if addr == CONTROLLER1_ADDR {
            (self.cpu_open_bus & OPEN_BUS_UPPER_MASK)
                | (self.mem.controllers()[0].poll() & CONTROLLER_DATA_MASK)
        } else if addr == CONTROLLER2_ADDR {
            (self.cpu_open_bus & OPEN_BUS_UPPER_MASK)
                | (self.mem.controllers()[1].poll() & CONTROLLER_DATA_MASK)
        } else if (APU_REG_START..APU_STATUS_ADDR).contains(&addr)
            || (IO_EXPANSION_START..IO_EXPANSION_END).contains(&addr)
        {
            self.cpu_open_bus
        } else {
            self.mem.cpu_read(addr)
        };
        self.cpu_open_bus = value;
        value
    }

    fn cpu_write_cycle(&mut self, addr: u16, value: u8) {
        self.cpu_open_bus = value;
        self.sync_ppu_for_access(addr, true);

        let ppu_reg = self.ppu_reg_cpu_addr(addr);

        if let Some(reg) = ppu_reg {
            if self.cpu.cycle < self.ppu_register_warmup_until_cpu_cycle {
                return;
            }
            if reg == ppu::CTRL_REG_ADDR {
                self.mem.notify_ppu_ctrl(value);
            }
            if reg == ppu::MASK_REG_ADDR {
                self.mem.notify_ppu_mask(value);
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
                self.mem
                    .ppu_a12_transition(ppu_addr, self.ppu.last_synced_dot);
            }
        } else if addr == ppu::OAM_DMA {
            if self.cpu.cycle < self.ppu_register_warmup_until_cpu_cycle {
                return;
            }
            self.dma_pending = true;
            self.dma_page = value;
        } else if addr == CONTROLLER1_ADDR {
            let strobe = value & 1 != 0;
            self.mem.controllers()[0].set_strobe(strobe);
            self.mem.controllers()[1].set_strobe(strobe);
        } else if (APU_REG_START..=APU_STATUS_ADDR).contains(&addr) || addr == CONTROLLER2_ADDR {
            let apu_cycle_tag = if addr == CONTROLLER2_ADDR {
                self.cycles
            } else {
                0
            };
            self.apu.write(addr, value, apu_cycle_tag);
        } else {
            self.mem.cpu_write(addr, value);
        }
    }

    fn sync_ppu_for_access(&mut self, addr: u16, is_write: bool) {
        if self.cpu_access_needs_ppu_sync(addr, is_write) {
            self.sync_ppu_to_dot(self.master_clock);
        }
    }

    fn step_fetch(&mut self) {
        self.log_init();
        self.cpu.last_instruction = self.cpu.pc;
        self.cpu_instr_pc = self.cpu.pc;
        self.irq_i_flag_sampled = self.cpu.interrupt_flag();
        self.cpu_opcode = self.cpu_read_cycle(self.cpu.pc);
        // PC is NOT advanced here — each addressing mode step handles PC advancement.
        // This keeps cpu.pc pointing at the opcode for external observation.
        self.cpu_phase = CpuPhase::Execute;
        self.cpu_step = 0;
        self.cpu_page_crossed = false;

        if self.cpu_opcode == opcodes::BRK {
            self.start_interrupt(InterruptKind::Brk);
        }
    }

    fn step_execute(&mut self) {
        let step = self.cpu_step;
        self.cpu_step += 1;

        let opcode = self.cpu_opcode;
        let instr_type = classify_instr_type(opcode);
        let addr_mode = self.lookup.mode(opcode);

        match instr_type {
            InstrType::Implied => self.step_implied(step),
            InstrType::Accumulator => self.step_accumulator(step),
            InstrType::Branch => self.step_branch(step),
            InstrType::Push => self.step_push(step),
            InstrType::Pull => self.step_pull(step),
            InstrType::Jsr => self.step_jsr(step),
            InstrType::Rts => self.step_rts(step),
            InstrType::Rti => self.step_rti(step),
            InstrType::Brk => {} // handled via Interrupt phase
            InstrType::JmpAbs => self.step_jmp_abs(step),
            InstrType::JmpInd => self.step_jmp_ind(step),
            InstrType::Read => match addr_mode {
                opcodes::ADDR_MODE_IMM => self.step_read_imm(step),
                opcodes::ADDR_MODE_ZP => self.step_read_zp(step),
                opcodes::ADDR_MODE_ZPX => { let idx = self.cpu.x; self.step_read_zpxy(step, idx); }
                opcodes::ADDR_MODE_ZPY => { let idx = self.cpu.y; self.step_read_zpxy(step, idx); }
                opcodes::ADDR_MODE_ABS => self.step_read_abs(step),
                opcodes::ADDR_MODE_ABX => { let idx = self.cpu.x; self.step_read_abxy(step, idx); }
                opcodes::ADDR_MODE_ABY => { let idx = self.cpu.y; self.step_read_abxy(step, idx); }
                opcodes::ADDR_MODE_INX => self.step_read_inx(step),
                opcodes::ADDR_MODE_INY => self.step_read_iny(step),
                _ => self.step_implied(step), // fallback for unknown NOPs
            },
            InstrType::Write => match addr_mode {
                opcodes::ADDR_MODE_ZP => self.step_write_zp(step),
                opcodes::ADDR_MODE_ZPX => { let idx = self.cpu.x; self.step_write_zpxy(step, idx); }
                opcodes::ADDR_MODE_ZPY => { let idx = self.cpu.y; self.step_write_zpxy(step, idx); }
                opcodes::ADDR_MODE_ABS => self.step_write_abs(step),
                opcodes::ADDR_MODE_ABX => { let idx = self.cpu.x; self.step_write_abxy(step, idx); }
                opcodes::ADDR_MODE_ABY => { let idx = self.cpu.y; self.step_write_abxy(step, idx); }
                opcodes::ADDR_MODE_INX => self.step_write_inx(step),
                opcodes::ADDR_MODE_INY => self.step_write_iny(step),
                _ => self.finish_instruction(),
            },
            InstrType::Rmw => match addr_mode {
                opcodes::ADDR_MODE_ZP => self.step_rmw_zp(step),
                opcodes::ADDR_MODE_ZPX => self.step_rmw_zpx(step),
                opcodes::ADDR_MODE_ABS => self.step_rmw_abs(step),
                opcodes::ADDR_MODE_ABX => self.step_rmw_abx(step),
                opcodes::ADDR_MODE_ABY => self.step_rmw_aby(step),
                opcodes::ADDR_MODE_INX => self.step_rmw_inx(step),
                opcodes::ADDR_MODE_INY => self.step_rmw_iny(step),
                _ => self.finish_instruction(),
            },
        }
    }

    // ── Addressing pattern step functions ─────────────────────────────

    fn step_implied(&mut self, step: u8) {
        debug_assert_eq!(step, 0);
        let _ = self.cpu_read_cycle(self.cpu.pc); // dummy read
        self.apply_implied_op();
        self.finish_instruction();
    }

    fn step_accumulator(&mut self, step: u8) {
        debug_assert_eq!(step, 0);
        let _ = self.cpu_read_cycle(self.cpu.pc); // dummy read
        match self.cpu_opcode {
            opcodes::ASL => { self.cpu.a = self.cpu.asl(self.cpu.a); }
            opcodes::LSR => { self.cpu.a = self.cpu.lsr(self.cpu.a); }
            opcodes::ROL => { self.cpu.a = self.cpu.rol(self.cpu.a); }
            opcodes::ROR => { self.cpu.a = self.cpu.ror(self.cpu.a); }
            _ => {}
        }
        self.finish_instruction();
    }

    fn step_read_imm(&mut self, step: u8) {
        debug_assert_eq!(step, 0);
        let value = self.cpu_read_cycle(self.cpu.pc);
        self.cpu.pc = self.cpu.pc.wrapping_add(1);
        self.apply_read_op(value);
        self.finish_instruction();
    }

    fn step_read_zp(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_addr = self.cpu_read_cycle(self.cpu.pc) as u16;
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let value = self.cpu_read_cycle(self.cpu_addr);
                self.apply_read_op(value);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_read_zpxy(&mut self, step: u8, idx: u8) {
        match step {
            0 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let _ = self.cpu_read_cycle(self.cpu_addr_lo as u16); // dummy read
                self.cpu_addr = self.cpu_addr_lo.wrapping_add(idx) as u16;
            }
            2 => {
                let value = self.cpu_read_cycle(self.cpu_addr);
                self.apply_read_op(value);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_read_abs(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let hi = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
                self.cpu_addr = memory::to_16b_addr(hi, self.cpu_addr_lo);
            }
            2 => {
                let value = self.cpu_read_cycle(self.cpu_addr);
                self.apply_read_op(value);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_read_abxy(&mut self, step: u8, idx: u8) {
        match step {
            0 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let hi = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
                self.cpu_page_crossed = (self.cpu_addr_lo as u16 + idx as u16) > 0xff;
                let lo_indexed = self.cpu_addr_lo.wrapping_add(idx);
                self.cpu_addr = memory::to_16b_addr(hi, lo_indexed);
                if self.cpu_page_crossed {
                    self.cpu_data = hi; // save high byte for page fix
                }
            }
            2 => {
                if self.cpu_page_crossed {
                    // dummy read from uncorrected address
                    let uncorrected = self.cpu_addr.wrapping_sub(0x100);
                    let _ = self.cpu_read_cycle(uncorrected);
                } else {
                    let value = self.cpu_read_cycle(self.cpu_addr);
                    self.apply_read_op(value);
                    self.finish_instruction();
                }
            }
            3 => {
                let value = self.cpu_read_cycle(self.cpu_addr);
                self.apply_read_op(value);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_read_inx(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_data = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let _ = self.cpu_read_cycle(self.cpu_data as u16); // dummy read
                self.cpu_data = self.cpu_data.wrapping_add(self.cpu.x);
            }
            2 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu_data as u16);
            }
            3 => {
                let hi = self.cpu_read_cycle(self.cpu_data.wrapping_add(1) as u16);
                self.cpu_addr = memory::to_16b_addr(hi, self.cpu_addr_lo);
            }
            4 => {
                let value = self.cpu_read_cycle(self.cpu_addr);
                self.apply_read_op(value);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_read_iny(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_data = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu_data as u16);
            }
            2 => {
                let hi = self.cpu_read_cycle(self.cpu_data.wrapping_add(1) as u16);
                let y = self.cpu.y;
                self.cpu_page_crossed = (self.cpu_addr_lo as u16 + y as u16) > 0xff;
                let lo_indexed = self.cpu_addr_lo.wrapping_add(y);
                let hi_fixed = if self.cpu_page_crossed { hi.wrapping_add(1) } else { hi };
                self.cpu_addr = memory::to_16b_addr(hi_fixed, lo_indexed);
                self.cpu_data = hi; // save for uncorrected address
            }
            3 => {
                if self.cpu_page_crossed {
                    let uncorrected = memory::to_16b_addr(self.cpu_data, (self.cpu_addr & 0xff) as u8);
                    let _ = self.cpu_read_cycle(uncorrected); // dummy read
                } else {
                    let value = self.cpu_read_cycle(self.cpu_addr);
                    self.apply_read_op(value);
                    self.finish_instruction();
                }
            }
            4 => {
                let value = self.cpu_read_cycle(self.cpu_addr);
                self.apply_read_op(value);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    // ── Write pattern steps ──────────────────────────────────────────

    fn get_write_value(&mut self) -> u8 {
        match self.cpu_opcode {
            opcodes::STA_ZP | opcodes::STA_ZPX | opcodes::STA_ABS |
            opcodes::STA_ABX | opcodes::STA_ABY | opcodes::STA_INX | opcodes::STA_INY => self.cpu.a,
            opcodes::STX_ZP | opcodes::STX_ZPY | opcodes::STX_ABS => self.cpu.x,
            opcodes::STY_ZP | opcodes::STY_ZPX | opcodes::STY_ABS => self.cpu.y,
            opcodes::SAX_ZP | opcodes::SAX_ZPY | opcodes::SAX_ABS | opcodes::SAX_INX => self.cpu.a & self.cpu.x,
            opcodes::SHA_INY | opcodes::SHA_ABY => {
                let high = ((self.cpu_addr >> 8) as u8).wrapping_add(1);
                self.cpu.a & self.cpu.x & high
            }
            opcodes::TAS_ABY => {
                self.cpu.sp = self.cpu.a & self.cpu.x;
                let high = ((self.cpu_addr >> 8) as u8).wrapping_add(1);
                self.cpu.sp & high
            }
            opcodes::SHY_ABX => {
                let high = ((self.cpu_addr >> 8) as u8).wrapping_add(1);
                self.cpu.y & high
            }
            opcodes::SHX_ABY => {
                let high = ((self.cpu_addr >> 8) as u8).wrapping_add(1);
                self.cpu.x & high
            }
            _ => 0,
        }
    }

    fn step_write_zp(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_addr = self.cpu_read_cycle(self.cpu.pc) as u16;
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let value = self.get_write_value();
                self.cpu_write_cycle(self.cpu_addr, value);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_write_zpxy(&mut self, step: u8, idx: u8) {
        match step {
            0 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let _ = self.cpu_read_cycle(self.cpu_addr_lo as u16); // dummy read
                self.cpu_addr = self.cpu_addr_lo.wrapping_add(idx) as u16;
            }
            2 => {
                let value = self.get_write_value();
                self.cpu_write_cycle(self.cpu_addr, value);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_write_abs(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let hi = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
                self.cpu_addr = memory::to_16b_addr(hi, self.cpu_addr_lo);
            }
            2 => {
                let value = self.get_write_value();
                self.cpu_write_cycle(self.cpu_addr, value);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_write_abxy(&mut self, step: u8, idx: u8) {
        match step {
            0 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let hi = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
                let lo_indexed = self.cpu_addr_lo.wrapping_add(idx);
                let page_crossed = (self.cpu_addr_lo as u16 + idx as u16) > 0xff;
                let hi_fixed = if page_crossed { hi.wrapping_add(1) } else { hi };
                self.cpu_addr = memory::to_16b_addr(hi_fixed, lo_indexed);
                self.cpu_data = hi; // save original high byte
            }
            2 => {
                // dummy read from uncorrected address (always, for writes)
                let uncorrected = memory::to_16b_addr(self.cpu_data, (self.cpu_addr & 0xff) as u8);
                let _ = self.cpu_read_cycle(uncorrected);
            }
            3 => {
                let value = self.get_write_value();
                self.cpu_write_cycle(self.cpu_addr, value);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_write_inx(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_data = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let _ = self.cpu_read_cycle(self.cpu_data as u16); // dummy read
                self.cpu_data = self.cpu_data.wrapping_add(self.cpu.x);
            }
            2 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu_data as u16);
            }
            3 => {
                let hi = self.cpu_read_cycle(self.cpu_data.wrapping_add(1) as u16);
                self.cpu_addr = memory::to_16b_addr(hi, self.cpu_addr_lo);
            }
            4 => {
                let value = self.get_write_value();
                self.cpu_write_cycle(self.cpu_addr, value);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_write_iny(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_data = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu_data as u16);
            }
            2 => {
                let hi = self.cpu_read_cycle(self.cpu_data.wrapping_add(1) as u16);
                let y = self.cpu.y;
                let page_crossed = (self.cpu_addr_lo as u16 + y as u16) > 0xff;
                let lo_indexed = self.cpu_addr_lo.wrapping_add(y);
                let hi_fixed = if page_crossed { hi.wrapping_add(1) } else { hi };
                self.cpu_addr = memory::to_16b_addr(hi_fixed, lo_indexed);
                self.cpu_data = hi; // save for uncorrected
            }
            3 => {
                // dummy read from uncorrected address (always, for writes)
                let uncorrected = memory::to_16b_addr(self.cpu_data, (self.cpu_addr & 0xff) as u8);
                let _ = self.cpu_read_cycle(uncorrected);
            }
            4 => {
                let value = self.get_write_value();
                self.cpu_write_cycle(self.cpu_addr, value);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    // ── RMW pattern steps ────────────────────────────────────────────

    fn step_rmw_zp(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_addr = self.cpu_read_cycle(self.cpu.pc) as u16;
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                self.cpu_data = self.cpu_read_cycle(self.cpu_addr);
            }
            2 => {
                self.cpu_write_cycle(self.cpu_addr, self.cpu_data); // dummy write old value
            }
            3 => {
                let result = self.apply_rmw_op(self.cpu_data);
                self.cpu_write_cycle(self.cpu_addr, result);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_rmw_zpx(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let _ = self.cpu_read_cycle(self.cpu_addr_lo as u16); // dummy read
                self.cpu_addr = self.cpu_addr_lo.wrapping_add(self.cpu.x) as u16;
            }
            2 => {
                self.cpu_data = self.cpu_read_cycle(self.cpu_addr);
            }
            3 => {
                self.cpu_write_cycle(self.cpu_addr, self.cpu_data); // dummy write
            }
            4 => {
                let result = self.apply_rmw_op(self.cpu_data);
                self.cpu_write_cycle(self.cpu_addr, result);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_rmw_abs(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let hi = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
                self.cpu_addr = memory::to_16b_addr(hi, self.cpu_addr_lo);
            }
            2 => {
                self.cpu_data = self.cpu_read_cycle(self.cpu_addr);
            }
            3 => {
                self.cpu_write_cycle(self.cpu_addr, self.cpu_data); // dummy write
            }
            4 => {
                let result = self.apply_rmw_op(self.cpu_data);
                self.cpu_write_cycle(self.cpu_addr, result);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_rmw_abx(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let hi = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
                let x = self.cpu.x;
                let page_crossed = (self.cpu_addr_lo as u16 + x as u16) > 0xff;
                let lo_indexed = self.cpu_addr_lo.wrapping_add(x);
                let hi_fixed = if page_crossed { hi.wrapping_add(1) } else { hi };
                self.cpu_addr = memory::to_16b_addr(hi_fixed, lo_indexed);
                self.cpu_data = hi; // save for uncorrected
            }
            2 => {
                let uncorrected = memory::to_16b_addr(self.cpu_data, (self.cpu_addr & 0xff) as u8);
                let _ = self.cpu_read_cycle(uncorrected); // dummy read
            }
            3 => {
                self.cpu_data = self.cpu_read_cycle(self.cpu_addr);
            }
            4 => {
                self.cpu_write_cycle(self.cpu_addr, self.cpu_data); // dummy write
            }
            5 => {
                let result = self.apply_rmw_op(self.cpu_data);
                self.cpu_write_cycle(self.cpu_addr, result);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_rmw_aby(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let hi = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
                let y = self.cpu.y;
                let page_crossed = (self.cpu_addr_lo as u16 + y as u16) > 0xff;
                let lo_indexed = self.cpu_addr_lo.wrapping_add(y);
                let hi_fixed = if page_crossed { hi.wrapping_add(1) } else { hi };
                self.cpu_addr = memory::to_16b_addr(hi_fixed, lo_indexed);
                self.cpu_data = hi;
            }
            2 => {
                let uncorrected = memory::to_16b_addr(self.cpu_data, (self.cpu_addr & 0xff) as u8);
                let _ = self.cpu_read_cycle(uncorrected);
            }
            3 => {
                self.cpu_data = self.cpu_read_cycle(self.cpu_addr);
            }
            4 => {
                self.cpu_write_cycle(self.cpu_addr, self.cpu_data);
            }
            5 => {
                let result = self.apply_rmw_op(self.cpu_data);
                self.cpu_write_cycle(self.cpu_addr, result);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_rmw_inx(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_data = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let _ = self.cpu_read_cycle(self.cpu_data as u16);
                self.cpu_data = self.cpu_data.wrapping_add(self.cpu.x);
            }
            2 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu_data as u16);
            }
            3 => {
                let hi = self.cpu_read_cycle(self.cpu_data.wrapping_add(1) as u16);
                self.cpu_addr = memory::to_16b_addr(hi, self.cpu_addr_lo);
            }
            4 => {
                self.cpu_data = self.cpu_read_cycle(self.cpu_addr);
            }
            5 => {
                self.cpu_write_cycle(self.cpu_addr, self.cpu_data);
            }
            6 => {
                let result = self.apply_rmw_op(self.cpu_data);
                self.cpu_write_cycle(self.cpu_addr, result);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_rmw_iny(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_data = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu_data as u16);
            }
            2 => {
                let hi = self.cpu_read_cycle(self.cpu_data.wrapping_add(1) as u16);
                let y = self.cpu.y;
                let page_crossed = (self.cpu_addr_lo as u16 + y as u16) > 0xff;
                let lo_indexed = self.cpu_addr_lo.wrapping_add(y);
                let hi_fixed = if page_crossed { hi.wrapping_add(1) } else { hi };
                self.cpu_addr = memory::to_16b_addr(hi_fixed, lo_indexed);
                self.cpu_data = hi;
            }
            3 => {
                let uncorrected = memory::to_16b_addr(self.cpu_data, (self.cpu_addr & 0xff) as u8);
                let _ = self.cpu_read_cycle(uncorrected);
            }
            4 => {
                self.cpu_data = self.cpu_read_cycle(self.cpu_addr);
            }
            5 => {
                self.cpu_write_cycle(self.cpu_addr, self.cpu_data);
            }
            6 => {
                let result = self.apply_rmw_op(self.cpu_data);
                self.cpu_write_cycle(self.cpu_addr, result);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    // ── Branch step ──────────────────────────────────────────────────

    fn step_branch(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_data = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
                let taken = match self.cpu_opcode {
                    opcodes::BPL => !self.cpu.negative_flag(),
                    opcodes::BMI => self.cpu.negative_flag(),
                    opcodes::BVC => !self.cpu.overflow_flag(),
                    opcodes::BVS => self.cpu.overflow_flag(),
                    opcodes::BCC => !self.cpu.carry_flag(),
                    opcodes::BCS => self.cpu.carry_flag(),
                    opcodes::BNE => !self.cpu.zero_flag(),
                    opcodes::BEQ => self.cpu.zero_flag(),
                    _ => false,
                };
                if !taken {
                    self.finish_instruction();
                }
            }
            1 => {
                // add offset to PC
                let offset = self.cpu_data as i8 as u16;
                let new_pc = self.cpu.pc.wrapping_add(offset);
                self.cpu_page_crossed = (new_pc & 0xFF00) != (self.cpu.pc & 0xFF00);
                self.cpu.pc = new_pc;
                if !self.cpu_page_crossed {
                    // taken, no page cross: suppress IRQ polling
                    self.finish_instruction_no_irq_poll();
                }
            }
            2 => {
                // page crossing fixup cycle (dummy read)
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    // ── Stack operations ─────────────────────────────────────────────

    fn step_push(&mut self, step: u8) {
        match step {
            0 => {
                let _ = self.cpu_read_cycle(self.cpu.pc); // dummy read
            }
            1 => {
                let value = match self.cpu_opcode {
                    opcodes::PHA => self.cpu.a,
                    opcodes::PHP => self.cpu.status | cpu::BREAK_BIT,
                    _ => 0,
                };
                let sp = self.cpu.sp;
                self.cpu_write_cycle(self.stack_addr(sp), value);
                self.cpu.sp = self.cpu.sp.wrapping_sub(1);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_pull(&mut self, step: u8) {
        match step {
            0 => {
                let _ = self.cpu_read_cycle(self.cpu.pc); // dummy read
            }
            1 => {
                // increment SP (dummy read from old stack location)
                self.cpu.sp = self.cpu.sp.wrapping_add(1);
            }
            2 => {
                let sp = self.cpu.sp;
                let value = self.cpu_read_cycle(self.stack_addr(sp));
                match self.cpu_opcode {
                    opcodes::PLA => {
                        self.cpu.a = value;
                        self.cpu.check_negative(value);
                        self.cpu.check_zero(value);
                    }
                    opcodes::PLP => {
                        self.cpu.status = value;
                        self.cpu.set_status_flag(cpu::IGNORE_BIT);
                        self.cpu.clear_status_flag(cpu::BREAK_BIT);
                    }
                    _ => {}
                }
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    // ── Control flow ─────────────────────────────────────────────────

    fn step_jsr(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                // internal operation (dummy read from stack)
            }
            2 => {
                // push PCH
                let pch = (self.cpu.pc >> 8) as u8;
                let sp = self.cpu.sp;
                self.cpu_write_cycle(self.stack_addr(sp), pch);
                self.cpu.sp = self.cpu.sp.wrapping_sub(1);
            }
            3 => {
                // push PCL
                let pcl = (self.cpu.pc & 0xff) as u8;
                let sp = self.cpu.sp;
                self.cpu_write_cycle(self.stack_addr(sp), pcl);
                self.cpu.sp = self.cpu.sp.wrapping_sub(1);
            }
            4 => {
                let hi = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = memory::to_16b_addr(hi, self.cpu_addr_lo);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_rts(&mut self, step: u8) {
        match step {
            0 => {
                let _ = self.cpu_read_cycle(self.cpu.pc); // dummy read
            }
            1 => {
                // increment SP
                self.cpu.sp = self.cpu.sp.wrapping_add(1);
            }
            2 => {
                // pull PCL
                let sp = self.cpu.sp;
                self.cpu_addr_lo = self.cpu_read_cycle(self.stack_addr(sp));
                self.cpu.sp = self.cpu.sp.wrapping_add(1);
            }
            3 => {
                // pull PCH
                let sp = self.cpu.sp;
                let hi = self.cpu_read_cycle(self.stack_addr(sp));
                self.cpu.pc = memory::to_16b_addr(hi, self.cpu_addr_lo);
            }
            4 => {
                // increment PC
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_rti(&mut self, step: u8) {
        match step {
            0 => {
                let _ = self.cpu_read_cycle(self.cpu.pc); // dummy read
            }
            1 => {
                // increment SP
                self.cpu.sp = self.cpu.sp.wrapping_add(1);
            }
            2 => {
                // pull P
                let sp = self.cpu.sp;
                let status = self.cpu_read_cycle(self.stack_addr(sp));
                self.cpu.status = status;
                self.cpu.set_status_flag(cpu::IGNORE_BIT);
                self.cpu.clear_status_flag(cpu::BREAK_BIT);
                self.cpu.sp = self.cpu.sp.wrapping_add(1);
                // RTI restores I flag early — update the sampled flag
                self.irq_i_flag_sampled = self.cpu.interrupt_flag();
            }
            3 => {
                // pull PCL
                let sp = self.cpu.sp;
                self.cpu_addr_lo = self.cpu_read_cycle(self.stack_addr(sp));
                self.cpu.sp = self.cpu.sp.wrapping_add(1);
            }
            4 => {
                // pull PCH
                let sp = self.cpu.sp;
                let hi = self.cpu_read_cycle(self.stack_addr(sp));
                self.cpu.pc = memory::to_16b_addr(hi, self.cpu_addr_lo);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_jmp_abs(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let hi = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = memory::to_16b_addr(hi, self.cpu_addr_lo);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    fn step_jmp_ind(&mut self, step: u8) {
        match step {
            0 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu.pc);
                self.cpu.pc = self.cpu.pc.wrapping_add(1);
            }
            1 => {
                let hi = self.cpu_read_cycle(self.cpu.pc);
                self.cpu_addr = memory::to_16b_addr(hi, self.cpu_addr_lo);
            }
            2 => {
                self.cpu_addr_lo = self.cpu_read_cycle(self.cpu_addr);
            }
            3 => {
                // 6502 page-wrapping bug: if pointer is at $xxFF, high byte comes from $xx00
                let hi_addr = if (self.cpu_addr & 0xff) == 0xff {
                    self.cpu_addr & 0xff00
                } else {
                    self.cpu_addr + 1
                };
                let hi = self.cpu_read_cycle(hi_addr);
                self.cpu.pc = memory::to_16b_addr(hi, self.cpu_addr_lo);
                self.finish_instruction();
            }
            _ => unreachable!()
        }
    }

    // ── Interrupt entry state machine ────────────────────────────────

    fn start_interrupt(&mut self, kind: InterruptKind) {
        self.cpu_phase = CpuPhase::Interrupt;
        self.interrupt_kind = kind;
        self.cpu_step = match kind {
            InterruptKind::Brk => 1, // fetch already consumed T0
            _ => 0,
        };
    }

    fn step_interrupt(&mut self) {
        let step = self.cpu_step;
        self.cpu_step += 1;

        match step {
            0 => {
                // T0: dummy read from PC (IRQ/NMI hijack opcode fetch)
                let _ = self.cpu_read_cycle(self.cpu.pc);
            }
            1 => {
                // T1: dummy read / BRK reads padding byte
                if self.interrupt_kind == InterruptKind::Brk {
                    let _ = self.cpu_read_cycle(self.cpu.pc);
                    self.cpu.pc = self.cpu.pc.wrapping_add(1);
                } else {
                    let _ = self.cpu_read_cycle(self.cpu.pc);
                }
            }
            2 => {
                // T2: push PCH
                let pch = (self.cpu.pc >> 8) as u8;
                let sp = self.cpu.sp;
                self.cpu_write_cycle(self.stack_addr(sp), pch);
                self.cpu.sp = self.cpu.sp.wrapping_sub(1);
            }
            3 => {
                // T3: push PCL
                let pcl = (self.cpu.pc & 0xff) as u8;
                let sp = self.cpu.sp;
                self.cpu_write_cycle(self.stack_addr(sp), pcl);
                self.cpu.sp = self.cpu.sp.wrapping_sub(1);
            }
            4 => {
                // T4: push P
                let status = if self.interrupt_kind == InterruptKind::Brk {
                    self.cpu.status | cpu::BREAK_BIT
                } else {
                    self.cpu.status & !cpu::BREAK_BIT
                };
                let sp = self.cpu.sp;
                self.cpu_write_cycle(self.stack_addr(sp), status);
                self.cpu.sp = self.cpu.sp.wrapping_sub(1);
                self.cpu.set_status_flag(cpu::INTERRUPT_BIT);
            }
            5 => {
                // T5: read vector low byte — NMI can hijack here
                let vector = if self.pending_nmi && self.interrupt_kind != InterruptKind::Nmi {
                    self.pending_nmi = false;
                    memory::NMI_TARGET_ADDR
                } else if self.interrupt_kind == InterruptKind::Nmi {
                    memory::NMI_TARGET_ADDR
                } else {
                    memory::BRK_TARGET_ADDR
                };
                self.interrupt_vector = vector;
                self.cpu_addr_lo = self.cpu_read_cycle(vector);
            }
            6 => {
                // T6: read vector high byte
                let hi = self.cpu_read_cycle(self.interrupt_vector + 1);
                self.cpu.pc = memory::to_16b_addr(hi, self.cpu_addr_lo);
                self.log_instruction(self.cpu_opcode, self.cpu_instr_pc);
                self.instructions += 1;
                self.instruction_just_finished = true;
                self.cpu_phase = CpuPhase::Fetch;
            }
            _ => unreachable!()
        }
    }

    // ── DMA state machine ────────────────────────────────────────────

    fn start_dma(&mut self) {
        self.cpu_phase = CpuPhase::Dma;
        self.dma_base = (self.dma_page as u16) << 8;
        self.dma_offset = 0;
        self.dma_cycle = 0;
        self.dma_align_remaining = if self.cycles % 2 == 1 { 2 } else { 1 };
    }

    fn step_dma(&mut self) {
        if self.dma_align_remaining > 0 {
            self.dma_align_remaining -= 1;
            return;
        }

        if self.dma_cycle % 2 == 0 {
            // Read from CPU bus
            self.dma_value = self.mem.cpu_read(self.dma_base + self.dma_offset);
        } else {
            // Write to OAM
            self.ppu.oam_dma_write(self.dma_offset as u8, self.dma_value);
            self.dma_offset += 1;
        }

        self.dma_cycle += 1;

        if self.dma_offset >= 256 {
            self.instruction_just_finished = true;
            self.cpu_phase = CpuPhase::Fetch;
        }
    }

    // ── Operation dispatch ───────────────────────────────────────────

    fn apply_implied_op(&mut self) {
        match self.cpu_opcode {
            opcodes::CLC => self.cpu.clear_status_flag(cpu::CARRY_BIT),
            opcodes::SEC => self.cpu.set_status_flag(cpu::CARRY_BIT),
            opcodes::CLI => self.cpu.clear_status_flag(cpu::INTERRUPT_BIT),
            opcodes::SEI => self.cpu.set_status_flag(cpu::INTERRUPT_BIT),
            opcodes::CLV => self.cpu.clear_status_flag(cpu::OVERFLOW_BIT),
            opcodes::CLD => self.cpu.clear_status_flag(cpu::DECIMAL_BIT),
            opcodes::SED => self.cpu.set_status_flag(cpu::DECIMAL_BIT),
            opcodes::TAX => {
                self.cpu.x = self.cpu.a;
                self.cpu.check_negative(self.cpu.x);
                self.cpu.check_zero(self.cpu.x);
            }
            opcodes::TXA => {
                self.cpu.a = self.cpu.x;
                self.cpu.check_negative(self.cpu.a);
                self.cpu.check_zero(self.cpu.a);
            }
            opcodes::TAY => {
                self.cpu.y = self.cpu.a;
                self.cpu.check_negative(self.cpu.y);
                self.cpu.check_zero(self.cpu.y);
            }
            opcodes::TYA => {
                self.cpu.a = self.cpu.y;
                self.cpu.check_negative(self.cpu.a);
                self.cpu.check_zero(self.cpu.a);
            }
            opcodes::TSX => {
                self.cpu.x = self.cpu.sp;
                self.cpu.check_negative(self.cpu.x);
                self.cpu.check_zero(self.cpu.x);
            }
            opcodes::TXS => {
                self.cpu.sp = self.cpu.x;
            }
            opcodes::DEX => {
                self.cpu.x = self.cpu.x.wrapping_sub(1);
                self.cpu.check_negative(self.cpu.x);
                self.cpu.check_zero(self.cpu.x);
            }
            opcodes::DEY => {
                self.cpu.y = self.cpu.y.wrapping_sub(1);
                self.cpu.check_negative(self.cpu.y);
                self.cpu.check_zero(self.cpu.y);
            }
            opcodes::INX => {
                self.cpu.x = self.cpu.x.wrapping_add(1);
                self.cpu.check_negative(self.cpu.x);
                self.cpu.check_zero(self.cpu.x);
            }
            opcodes::INY => {
                self.cpu.y = self.cpu.y.wrapping_add(1);
                self.cpu.check_negative(self.cpu.y);
                self.cpu.check_zero(self.cpu.y);
            }
            _ => {} // NOP variants
        }
    }

    fn apply_read_op(&mut self, value: u8) {
        match self.cpu_opcode {
            opcodes::LDA_IMM | opcodes::LDA_ZP | opcodes::LDA_ZPX | opcodes::LDA_ABS |
            opcodes::LDA_ABX | opcodes::LDA_ABY | opcodes::LDA_INX | opcodes::LDA_INY => {
                self.cpu.a = value;
                self.cpu.check_negative(value);
                self.cpu.check_zero(value);
            }
            opcodes::LDX_IMM | opcodes::LDX_ZP | opcodes::LDX_ZPY | opcodes::LDX_ABS | opcodes::LDX_ABY => {
                self.cpu.x = value;
                self.cpu.check_negative(value);
                self.cpu.check_zero(value);
            }
            opcodes::LDY_IMM | opcodes::LDY_ZP | opcodes::LDY_ZPX | opcodes::LDY_ABS | opcodes::LDY_ABX => {
                self.cpu.y = value;
                self.cpu.check_negative(value);
                self.cpu.check_zero(value);
            }
            opcodes::ADC_IMM | opcodes::ADC_ZP | opcodes::ADC_ZPX | opcodes::ADC_ABS |
            opcodes::ADC_ABX | opcodes::ADC_ABY | opcodes::ADC_INX | opcodes::ADC_INY => {
                self.cpu.add_to_a_with_carry(value);
            }
            opcodes::SBC_IMM | opcodes::SBC_ZP | opcodes::SBC_ZPX | opcodes::SBC_ABS |
            opcodes::SBC_ABX | opcodes::SBC_ABY | opcodes::SBC_INX | opcodes::SBC_INY |
            opcodes::SNC_IMM => {
                self.cpu.sub_from_a_with_carry(value);
            }
            opcodes::AND_IMM | opcodes::AND_ZP | opcodes::AND_ZPX | opcodes::AND_ABS |
            opcodes::AND_ABX | opcodes::AND_ABY | opcodes::AND_INX | opcodes::AND_INY => {
                self.cpu.and(value);
            }
            opcodes::ORA_IMM | opcodes::ORA_ZP | opcodes::ORA_ZPX | opcodes::ORA_ABS |
            opcodes::ORA_ABX | opcodes::ORA_ABY | opcodes::ORA_INX | opcodes::ORA_INY => {
                self.cpu.ora(value);
            }
            opcodes::EOR_IMM | opcodes::EOR_ZP | opcodes::EOR_ZPX | opcodes::EOR_ABS |
            opcodes::EOR_ABX | opcodes::EOR_ABY | opcodes::EOR_INX | opcodes::EOR_INY => {
                self.cpu.eor(value);
            }
            opcodes::CMP_IMM | opcodes::CMP_ZP | opcodes::CMP_ZPX | opcodes::CMP_ABS |
            opcodes::CMP_ABX | opcodes::CMP_ABY | opcodes::CMP_INX | opcodes::CMP_INY => {
                self.cpu.compare(self.cpu.a, value);
            }
            opcodes::CPX_IMM | opcodes::CPX_ZP | opcodes::CPX_ABS => {
                self.cpu.compare(self.cpu.x, value);
            }
            opcodes::CPY_IMM | opcodes::CPY_ZP | opcodes::CPY_ABS => {
                self.cpu.compare(self.cpu.y, value);
            }
            opcodes::BIT_ZP | opcodes::BIT_ABS => {
                self.cpu.bit(value);
            }
            opcodes::LAX_ZP | opcodes::LAX_ZPY | opcodes::LAX_ABS | opcodes::LAX_ABY |
            opcodes::LAX_INX | opcodes::LAX_INY | opcodes::LAX_IMM => {
                self.cpu.a = value;
                self.cpu.x = value;
                self.cpu.check_negative(value);
                self.cpu.check_zero(value);
            }
            opcodes::ANC_0B | opcodes::ANC_2B => {
                self.cpu.and(value);
                if self.cpu.negative_flag() {
                    self.cpu.set_status_flag(cpu::CARRY_BIT);
                } else {
                    self.cpu.clear_status_flag(cpu::CARRY_BIT);
                }
            }
            opcodes::ALR_IMM => {
                self.cpu.a &= value;
                let old_bit0 = self.cpu.a & 1;
                self.cpu.a >>= 1;
                self.cpu.check_negative(self.cpu.a);
                self.cpu.check_zero(self.cpu.a);
                if old_bit0 != 0 { self.cpu.set_status_flag(cpu::CARRY_BIT); }
                else { self.cpu.clear_status_flag(cpu::CARRY_BIT); }
            }
            opcodes::ARR_IMM => {
                self.cpu.a &= value;
                let old_carry = if self.cpu.carry_flag() { 1u8 } else { 0 };
                self.cpu.a = (self.cpu.a >> 1) | (old_carry << 7);
                self.cpu.check_negative(self.cpu.a);
                self.cpu.check_zero(self.cpu.a);
                let bit6 = (self.cpu.a >> 6) & 1;
                let bit5 = (self.cpu.a >> 5) & 1;
                if bit6 != 0 { self.cpu.set_status_flag(cpu::CARRY_BIT); }
                else { self.cpu.clear_status_flag(cpu::CARRY_BIT); }
                if (bit6 ^ bit5) != 0 { self.cpu.set_status_flag(cpu::OVERFLOW_BIT); }
                else { self.cpu.clear_status_flag(cpu::OVERFLOW_BIT); }
            }
            opcodes::XAA_IMM => {
                self.cpu.a = self.cpu.x & value;
                self.cpu.check_negative(self.cpu.a);
                self.cpu.check_zero(self.cpu.a);
            }
            opcodes::SBX_IMM => {
                let ax = self.cpu.a & self.cpu.x;
                let result = (ax as u16).wrapping_sub(value as u16);
                self.cpu.x = result as u8;
                self.cpu.check_negative(self.cpu.x);
                self.cpu.check_zero(self.cpu.x);
                if ax >= value { self.cpu.set_status_flag(cpu::CARRY_BIT); }
                else { self.cpu.clear_status_flag(cpu::CARRY_BIT); }
            }
            opcodes::LAS_ABY => {
                let v = value & self.cpu.sp;
                self.cpu.a = v;
                self.cpu.x = v;
                self.cpu.sp = v;
                self.cpu.check_negative(v);
                self.cpu.check_zero(v);
            }
            _ => {} // NOP reads
        }
    }

    fn apply_rmw_op(&mut self, value: u8) -> u8 {
        match self.cpu_opcode {
            opcodes::ASL_ZP | opcodes::ASL_ZPX | opcodes::ASL_ABS | opcodes::ASL_ABX =>
                self.cpu.asl(value),
            opcodes::LSR_ZP | opcodes::LSR_ZPX | opcodes::LSR_ABS | opcodes::LSR_ABX =>
                self.cpu.lsr(value),
            opcodes::ROL_ZP | opcodes::ROL_ZPX | opcodes::ROL_ABS | opcodes::ROL_ABX =>
                self.cpu.rol(value),
            opcodes::ROR_ZP | opcodes::ROR_ZPX | opcodes::ROR_ABS | opcodes::ROR_ABX =>
                self.cpu.ror(value),
            opcodes::INC_ZP | opcodes::INC_ZPX | opcodes::INC_ABS | opcodes::INC_ABX => {
                let result = value.wrapping_add(1);
                self.cpu.check_negative(result);
                self.cpu.check_zero(result);
                result
            }
            opcodes::DEC_ZP | opcodes::DEC_ZPX | opcodes::DEC_ABS | opcodes::DEC_ABX => {
                let result = value.wrapping_sub(1);
                self.cpu.check_negative(result);
                self.cpu.check_zero(result);
                result
            }
            opcodes::DCP_ZP | opcodes::DCP_ZPX | opcodes::DCP_ABS | opcodes::DCP_ABX |
            opcodes::DCP_ABY | opcodes::DCP_INX | opcodes::DCP_INY => {
                let result = value.wrapping_sub(1);
                self.cpu.compare(self.cpu.a, result);
                result
            }
            opcodes::ISB_ZP | opcodes::ISB_ZPX | opcodes::ISB_ABS | opcodes::ISB_ABX |
            opcodes::ISB_ABY | opcodes::ISB_INX | opcodes::ISB_INY => {
                let result = value.wrapping_add(1);
                self.cpu.sub_from_a_with_carry(result);
                result
            }
            opcodes::SLO_ZP | opcodes::SLO_ZPX | opcodes::SLO_ABS | opcodes::SLO_ABX |
            opcodes::SLO_ABY | opcodes::SLO_INX | opcodes::SLO_INY => {
                let result = self.cpu.asl(value);
                self.cpu.ora(result);
                result
            }
            opcodes::SRE_ZP | opcodes::SRE_ZPX | opcodes::SRE_ABS | opcodes::SRE_ABX |
            opcodes::SRE_ABY | opcodes::SRE_INX | opcodes::SRE_INY => {
                let result = self.cpu.lsr(value);
                self.cpu.eor(result);
                result
            }
            opcodes::RLA_ZP | opcodes::RLA_ZPX | opcodes::RLA_ABS | opcodes::RLA_ABX |
            opcodes::RLA_ABY | opcodes::RLA_INX | opcodes::RLA_INY => {
                let result = self.cpu.rol(value);
                self.cpu.and(result);
                result
            }
            opcodes::RRA_ZP | opcodes::RRA_ZPX | opcodes::RRA_ABS | opcodes::RRA_ABX |
            opcodes::RRA_ABY | opcodes::RRA_INX | opcodes::RRA_INY => {
                let result = self.cpu.ror(value);
                self.cpu.add_to_a_with_carry(result);
                result
            }
            _ => value,
        }
    }

    // ── Instruction completion ────────────────────────────────────────

    fn finish_instruction(&mut self) {
        self.log_instruction(self.cpu_opcode, self.cpu_instr_pc);
        self.instructions += 1;
        self.instruction_just_finished = true;

        if !self.irq_i_flag_sampled {
            if self.poll_irq_sources() {
                self.pending_irq = true;
            }
        }

        self.cpu_phase = CpuPhase::Fetch;
    }

    fn finish_instruction_no_irq_poll(&mut self) {
        self.log_instruction(self.cpu_opcode, self.cpu_instr_pc);
        self.instructions += 1;
        self.instruction_just_finished = true;
        self.cpu_phase = CpuPhase::Fetch;
    }

    fn poll_irq_sources(&mut self) -> bool {
        if self.mem.poll_irq() {
            return true;
        }
        if self.apu.frame_irq_pending() {
            return true;
        }
        if self.apu.dmc_irq_pending() {
            return true;
        }
        false
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

    fn ppu_reg_cpu_addr(&self, addr: u16) -> Option<u16> {
        if !self.mem.cpu_maps_ppu_registers() {
            return None;
        }
        if (ppu::REG_BASE..=ppu::REG_LAST).contains(&addr) {
            Some(addr)
        } else if (ppu::REG_MIRROR_START..=ppu::REG_MIRROR_END).contains(&addr)
            && self.mem.cpu_maps_ppu_register_mirrors()
        {
            Some(ppu::REG_BASE + (addr % 8))
        } else {
            None
        }
    }

    fn cpu_access_needs_ppu_sync(&self, addr: u16, is_write: bool) -> bool {
        self.ppu_reg_cpu_addr(addr).is_some()
            || addr == ppu::OAM_DMA
            || (is_write && addr >= MAPPER_START)
    }

    fn sync_ppu_to_dot(&mut self, target_dot: u64) {
        let mut cycle_260_scanlines: [u16; 4] = [0; 4];
        let mut cycle_260_count: usize = 0;

        while self.ppu.last_synced_dot < target_dot {
            let step = self
                .ppu
                .step_dot_with_rendering(&mut *self.mem, &mut self.buf);
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
    }


    // Old helper functions kept for tests
    fn stack_addr(&self, sp: u8) -> u16 {
        memory::STACK_BASE_OFFSET + (u16::from(sp) & 0xff)
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

        let mut ctx = io::DebugContext {
            cpu: &mut self.cpu,
            mem: &mut *self.mem,
            breakpoints: &mut self.breakpoints,
            stepping: &mut self.stepping,
            should_log: &mut self.should_log,
            verbose: &mut self.verbose,
            lookup: &self.lookup,
        };
        self.iohandler.on_debug(&mut ctx);
    }

    fn exit(&self) {
        if let (Some(rom_path), Some(sram)) = (&self.rom_path, self.mem.sram_data()) {
            let sav = io::loader::sav_path(rom_path);
            match std::fs::write(&sav, sram) {
                Ok(_) => println!("Saved battery RAM to {}", sav.display()),
                Err(e) => eprintln!("Failed to save battery RAM to {}: {}", sav.display(), e),
            }
        }

        let elapsed_secs = self.cycles as f64 / 1_789_773.0;
        let fps = if elapsed_secs > 0.0 {
            self.ppu.frames as f64 / elapsed_secs
        } else {
            0.0
        };
        self.iohandler.exit(format!(
            "Exiting after {} instructions, {} cycles, {} frames ({:.2} fps, {:.1}s)",
            self.instructions, self.cycles, self.ppu.frames, fps, elapsed_secs
        ));
    }
}

#[cfg(test)]
mod emu_tests {
    use super::*;

    #[test]
    fn test_master_clock_advances_3x_cpu() {
        let mut emu: Emulator = Emulator::_new();
        // In per-cycle model, put CPU in a state where it won't execute
        // Place a NOP at code start
        emu.mem.cpu_write(memory::CODE_START_ADDR, opcodes::NOP);
        emu.cycle();

        assert_eq!(emu.cycles, 1);
        assert_eq!(emu.master_clock, 3);
        assert_eq!(emu.ppu.last_synced_dot, 3);
    }

    #[test]
    fn test_ppu_sync_on_register_read() {
        let mut emu: Emulator = Emulator::_new();
        let start = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::LDA_ABS);
        emu.mem.cpu_write(start + 1, ppu::STATUS_REG_ADDR as u8);
        emu.mem.cpu_write(start + 2, (ppu::STATUS_REG_ADDR >> 8) as u8);

        // Run 4 cycles for LDA abs (fetch + 3 execute steps)
        for _ in 0..4 {
            emu.cycle();
        }

        // PPU should have synced before the register read on cycle 4
        // Plus 3 dots per cycle for 4 cycles = 12 dots total
        assert_eq!(emu.ppu.last_synced_dot, 12);
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
