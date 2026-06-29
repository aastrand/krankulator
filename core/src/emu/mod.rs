pub mod apu;
pub mod audio;
pub mod cpu;
pub mod dbg;
pub mod debug;
pub mod gfx;
pub mod io;
pub mod memory;
pub mod ppu;
pub mod region;
pub mod rewind;
pub mod savestate;

#[cfg(test)]
mod integration_tests;

use cpu::opcodes;

use std::collections::HashSet;

use self::audio::{AudioBackend, CapturingAudioOutput, SilentAudioOutput};
pub use self::region::{Region, RegionConfig};

// CPU initialization values (per NES power-on state)
const CPU_INIT_SP: u8 = 0xFD;
const CPU_INIT_STATUS: u8 = 0x34;

// PPU register warmup period — writes to PPU registers are ignored for this many CPU cycles after power-on/reset
const PPU_WARMUP_CPU_CYCLES: u64 = 29_658;

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

enum RewindStepResult {
    Continue,
    Done,
    Exit,
}

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

    pub region: RegionConfig,
    pub instructions: u64,
    pub cycles: u64,
    pub master_clock: u64,
    master_clock_sub: u64,
    instruction_start_dot: u64,
    instruction_start_sub: u64,
    cpu_bus_cycle_offset: u8,
    cpu_open_bus: u8,
    irq_sample_deadline: u64,
    irq_allowed: bool,
    branch_irq_suppressed: bool,

    should_trigger_nmi: bool,
    nmi_countdown: i8,
    pub audio: Box<dyn AudioBackend>,
    /// Until this CPU cycle count (exclusive), writes to PPU registers and OAM DMA are ignored.
    ppu_register_warmup_until_cpu_cycle: u64,
    rom_path: Option<String>,
    savestate_slot: u8,
    last_rendered_frame: u64,
    pub overlay: gfx::overlay::Overlay,
    pending_open_rom: Option<String>,
    rewind_buffer: rewind::RewindBuffer,
    rewind_capture_tick: bool,
    rewinding: bool,
    rewind_scratch: Vec<u8>,
    overscan: bool,
    static_noise: bool,
    noise_state: u32,
    debug_active: bool,
    paused: bool,
    debug_chr_snapshot: Vec<u8>,
    debug_palette_snapshot: [u8; 32],
    debug_chr_captured_this_frame: bool,
    paused_frame: Vec<u8>,
}

impl Emulator {
    pub fn new_headless(mapper: Box<dyn memory::MemoryMapper>) -> Emulator {
        Self::new_headless_with_region(mapper, Region::Ntsc)
    }

    pub fn new_headless_with_region(
        mapper: Box<dyn memory::MemoryMapper>,
        region: Region,
    ) -> Emulator {
        let audio = Box::new(SilentAudioOutput::new()) as Box<dyn AudioBackend>;
        let iohandler = Box::new(io::HeadlessIOHandler {});
        Emulator::new_with_region(iohandler, mapper, audio, region)
    }

    pub fn new_capturing(mapper: Box<dyn memory::MemoryMapper>) -> Emulator {
        Self::new_capturing_with_region(mapper, Region::Ntsc)
    }

    pub fn new_capturing_with_region(
        mapper: Box<dyn memory::MemoryMapper>,
        region: Region,
    ) -> Emulator {
        let audio = Box::new(CapturingAudioOutput::new()) as Box<dyn AudioBackend>;
        let iohandler = Box::new(io::HeadlessIOHandler {});
        Emulator::new_with_region(iohandler, mapper, audio, region)
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
        mapper: Box<dyn memory::MemoryMapper>,
        audio: Box<dyn AudioBackend>,
    ) -> Emulator {
        Self::new_with_region(iohandler, mapper, audio, Region::Ntsc)
    }

    pub fn new_with_region(
        mut iohandler: Box<dyn io::IOHandler>,
        mut mapper: Box<dyn memory::MemoryMapper>,
        audio: Box<dyn AudioBackend>,
        region: Region,
    ) -> Emulator {
        let lookup: Box<opcodes::Lookup> = Box::default();
        let region_config = region.config();
        let frame_budget_ms = region_config.frame_duration_nanos as f64 / 1_000_000.0;
        iohandler.set_frame_duration_nanos(region_config.frame_duration_nanos);
        iohandler.set_overscan_available(region_config.region != Region::Pal);

        let mut cpu = cpu::Cpu::new();
        cpu.pc = mapper.code_start();

        let buf = gfx::buf::Buffer::new();

        Emulator {
            cpu,
            lookup,
            mem: mapper,
            ppu: ppu::PPU::new_with_region(&region_config),
            apu: apu::APU::new_with_region(&region_config),
            buf: Box::new(buf),
            iohandler,
            stepping: false,
            breakpoints: Box::new(HashSet::new()),
            logformatter: io::log::LogFormatter::new(30),
            logdata: Box::new(Vec::<u16>::new()),
            should_log: true,
            should_debug_on_infinite_loop: false,
            should_exit_on_infinite_loop: true,
            verbose: true,
            region: region_config,
            instructions: 0,
            cycles: 0,
            master_clock: 0,
            master_clock_sub: 0,
            instruction_start_dot: 0,
            instruction_start_sub: 0,
            cpu_bus_cycle_offset: 0,
            cpu_open_bus: 0,
            irq_sample_deadline: 0,
            irq_allowed: false,
            branch_irq_suppressed: false,
            should_trigger_nmi: false,
            nmi_countdown: -1,
            audio,
            ppu_register_warmup_until_cpu_cycle: PPU_WARMUP_CPU_CYCLES,
            rom_path: None,
            savestate_slot: 0,
            last_rendered_frame: 0,
            overlay: {
                let mut o = gfx::overlay::Overlay::new();
                o.set_frame_budget_ms(frame_budget_ms);
                o
            },
            pending_open_rom: None,
            rewind_buffer: rewind::RewindBuffer::new(),
            rewind_capture_tick: false,
            rewinding: false,
            rewind_scratch: Vec::new(),
            overscan: false,
            static_noise: false,
            noise_state: 0x1234,
            debug_active: false,
            paused: false,
            debug_chr_snapshot: Vec::new(),
            debug_palette_snapshot: [0u8; 32],
            debug_chr_captured_this_frame: false,
            paused_frame: Vec::new(),
        }
    }

    pub fn set_static_noise(&mut self, enabled: bool) {
        self.static_noise = enabled;
    }

    pub fn toggle_debug(&mut self) {
        self.debug_active = !self.debug_active;
        self.apu.set_debug_capture(self.debug_active);
        self.mem.set_debug_capture(self.debug_active);
    }

    pub fn is_debug_active(&self) -> bool {
        self.debug_active
    }

    pub fn debug_snapshot(&self) -> debug::DebugSnapshot {
        let regs = cpu::disasm::CpuRegs {
            x: self.cpu.x,
            y: self.cpu.y,
        };
        let (disasm, pc_idx) = cpu::disasm::disassemble_around(
            self.cpu.pc,
            debug::DISASM_CONTEXT,
            debug::DISASM_CONTEXT,
            &*self.mem,
            &self.lookup,
            Some(&regs),
        );
        let palette_data = if self.debug_chr_captured_this_frame {
            self.debug_palette_snapshot
        } else {
            let mut pal = [0u8; 32];
            for i in 0..32u16 {
                pal[i as usize] = self.mem.ppu_read(0x3F00 + i);
            }
            pal
        };
        debug::DebugSnapshot {
            cpu: debug::CpuSnapshot {
                pc: self.cpu.pc,
                a: self.cpu.a,
                x: self.cpu.x,
                y: self.cpu.y,
                sp: self.cpu.sp,
                status: self.cpu.status,
                cycle: self.cpu.cycle,
            },
            ppu: {
                let v = self.ppu.get_current_vram_addr();
                let coarse_x = v & 0x1F;
                let coarse_y = (v >> 5) & 0x1F;
                let fine_y = (v >> 12) & 0x07;
                let nt_select = ((v >> 10) & 0x03) as u8;
                let fine_x = self.ppu.fine_x();
                debug::PpuSnapshot {
                    ctrl: self.ppu.ppu_ctrl,
                    mask: self.ppu.ppu_mask,
                    status: self.ppu.ppu_status(),
                    v,
                    t: self.ppu.get_temp_vram_addr(),
                    fine_x,
                    scanline: self.ppu.scanline,
                    dot: self.ppu.cycle,
                    frame: self.ppu.frames,
                    scroll_x: coarse_x * 8 + fine_x as u16,
                    scroll_y: coarse_y * 8 + fine_y,
                    nametable_select: nt_select,
                }
            },
            apu: {
                let mut state = self.apu.debug_state();
                state.expansion_channels = self.mem.expansion_audio_debug();
                state
            },
            disasm,
            disasm_pc_index: pc_idx,
            palette: palette_data,
            stack: {
                let sp = self.cpu.sp;
                let top = sp.wrapping_add(1);
                let count = (0xFFu16 - sp as u16) as usize;
                let count = count.min(16);
                (0..count)
                    .map(|i| self.mem.cpu_peek(0x0100 + top as u16 + i as u16))
                    .collect()
            },
            oam: *self.ppu.oam_data(),
            sprites: {
                let chr = if self.debug_chr_snapshot.is_empty() {
                    None
                } else {
                    Some(self.debug_chr_snapshot.as_slice())
                };
                debug::render_sprites(
                    self.ppu.oam_data(),
                    &self.ppu,
                    &palette_data,
                    chr,
                    &*self.mem,
                )
            },
            nametables: {
                let chr = if self.debug_chr_snapshot.is_empty() {
                    None
                } else {
                    Some(self.debug_chr_snapshot.as_slice())
                };
                debug::render_all_nametables(&self.ppu, &palette_data, chr, &*self.mem)
            },
            pattern_tables: {
                let chr = if self.debug_chr_snapshot.is_empty() {
                    None
                } else {
                    Some(self.debug_chr_snapshot.as_slice())
                };
                [
                    debug::render_pattern_table(0x0000, &palette_data, &*self.mem, chr),
                    debug::render_pattern_table(0x1000, &palette_data, &*self.mem, chr),
                ]
            },
        }
    }

    pub fn set_overscan(&mut self, enabled: bool) {
        self.overscan = enabled;
        self.overlay.set_overscan(if enabled {
            gfx::buf::OVERSCAN_LINES as u8
        } else {
            0
        });
    }

    fn fill_static_noise(&mut self) {
        let data = &mut self.buf.data;
        let mut state = self.noise_state;
        let mut i = 0;
        while i + 2 < data.len() {
            // 32-bit LFSR with taps at 32,22,2,1 (maximal period)
            let bit = (state ^ (state >> 1) ^ (state >> 21) ^ (state >> 31)) & 1;
            state = (state >> 1) | (bit << 31);
            let luma = (state & 0xFF) as u8;
            // Slight blue tint like a real no-signal CRT
            data[i] = (luma as u16 * 200 / 256) as u8;
            data[i + 1] = (luma as u16 * 210 / 256) as u8;
            data[i + 2] = luma;
            i += 3;
        }
        self.noise_state = state;
    }

    pub fn set_rom_path(&mut self, path: &str) {
        self.rom_path = Some(path.to_string());
    }

    pub fn take_pending_open_rom(&mut self) -> Option<String> {
        self.pending_open_rom.take()
    }

    pub fn load_rom(&mut self, mapper: Box<dyn memory::MemoryMapper>, path: &str) {
        self.load_rom_with_region(mapper, path, self.region.region);
    }

    pub fn load_rom_with_region(
        &mut self,
        mapper: Box<dyn memory::MemoryMapper>,
        path: &str,
        region: Region,
    ) {
        let region_config = region.config();
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
        self.master_clock_sub = 0;
        self.instructions = 0;
        self.ppu = ppu::PPU::new_with_region(&region_config);
        self.apu = apu::APU::new_with_region(&region_config);
        self.audio.clear();
        self.nmi_countdown = -1;
        self.ppu_register_warmup_until_cpu_cycle = PPU_WARMUP_CPU_CYCLES;
        self.savestate_slot = 0;
        self.last_rendered_frame = 0;
        self.overlay.set_banner(None);
        self.static_noise = false;
        self.should_trigger_nmi = true;
        self.should_exit_on_infinite_loop = false;
        self.rewind_buffer.clear();
        self.rewind_capture_tick = false;
        self.rewinding = false;
        self.region = region_config;
        self.iohandler
            .set_frame_duration_nanos(self.region.frame_duration_nanos);
        self.iohandler
            .set_overscan_available(self.region.region != Region::Pal);
        self.overlay
            .set_frame_budget_ms(self.region.frame_duration_nanos as f64 / 1_000_000.0);
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
        w.write_u64(self.instruction_start_dot);
        w.write_u8(self.cpu_bus_cycle_offset);
        w.write_bool(self.should_trigger_nmi);
        w.write_i8(self.nmi_countdown);
        w.write_u64(self.ppu_register_warmup_until_cpu_cycle);
        w.write_u64(self.instructions);
        w.write_u8(self.region.region.to_byte());
        w.write_u64(self.master_clock_sub);
        w.write_u64(self.instruction_start_sub);
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
                self.overlay.toast(format!("SAVE FAILED: {e}"));
            }
        }
    }

    pub fn load_state_from_file(&mut self) {
        let path = self.savestate_path();
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                self.overlay.toast(format!("LOAD FAILED: {e}"));
                return;
            }
        };
        if let Err(e) = self.load_state_from_bytes(&data) {
            self.overlay.toast(format!("LOAD FAILED: {e}"));
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
        self.instruction_start_dot = r.read_u64()?;
        self.cpu_bus_cycle_offset = r.read_u8()?;
        self.should_trigger_nmi = r.read_bool()?;
        self.nmi_countdown = r.read_i8()?;
        self.ppu_register_warmup_until_cpu_cycle = r.read_u64()?;
        self.instructions = r.read_u64()?;
        if r.version() >= 8 {
            let region_byte = r.read_u8()?;
            let saved_region = Region::from_byte(region_byte).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown region byte: {region_byte}"),
                )
            })?;
            if saved_region != self.region.region {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "region mismatch: save={:?} current={:?}",
                        saved_region, self.region.region
                    ),
                ));
            }
            self.master_clock_sub = r.read_u64()?;
            self.instruction_start_sub = r.read_u64()?;
        } else {
            self.master_clock_sub = 0;
            self.instruction_start_sub = 0;
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
        self.cpu_write(addr, value);
    }

    #[cfg(test)]
    pub fn run_for_cycles(&mut self, max_cycles: u64) {
        if let Err(msg) = self.iohandler.init() {
            self.iohandler.log(msg)
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

    pub fn shutdown(&self) {
        self.exit();
    }

    pub fn step(&mut self) -> bool {
        if self.paused {
            return self.paused_step() != CycleState::Exiting;
        }
        self.run_one_frame()
    }

    pub fn run(&mut self) {
        if let Err(msg) = self.init() {
            self.iohandler.log(msg)
        }

        loop {
            if self.paused {
                if self.paused_step() == CycleState::Exiting {
                    break;
                }
                continue;
            }
            if self.rewinding {
                match self.rewind_step() {
                    RewindStepResult::Continue => continue,
                    RewindStepResult::Done => {
                        self.rewinding = false;
                        self.rewind_buffer.finish_rewind();
                        self.overlay.set_rewind_status(None);
                        self.audio.clear();
                        continue;
                    }
                    RewindStepResult::Exit => break,
                }
            }
            if self.cycle() == CycleState::Exiting {
                break;
            }
        }

        self.shutdown();
    }

    fn paused_step(&mut self) -> CycleState {
        if let Some(ms) = self.iohandler.frame_time_ms() {
            self.overlay.set_frame_time(ms);
        }
        if !self.paused_frame.is_empty() {
            self.buf.data.copy_from_slice(&self.paused_frame);
        }
        self.overlay.tick();
        self.overlay.draw(&mut self.buf);
        if self.overscan && self.region.region != region::Region::Pal {
            self.buf.mask_overscan();
        }
        if self.debug_active {
            let snapshot = self.debug_snapshot();
            self.iohandler.set_debug_snapshot(snapshot);
        }
        self.iohandler.render(&self.buf);
        let result = self.iohandler.poll(&mut *self.mem, &mut self.apu);
        if result.exit {
            return CycleState::Exiting;
        }
        if result.toggle_pause {
            self.paused = false;
            self.overlay.toast("RESUMED".into());
        }
        if result.toggle_debug {
            self.toggle_debug();
        }
        if result.open_rom.is_some() {
            self.pending_open_rom = result.open_rom;
            return CycleState::Exiting;
        }
        CycleState::CpuAhead
    }

    fn run_cycle_core(&mut self) {
        if self.cpu.cycle == self.cycles && !self.service_pending_irq() {
            let cycle_before = self.cpu.cycle;
            let i_flag_before = self.cpu.interrupt_flag();
            let opcode = self.execute_instruction();
            self.log_instruction(opcode, self.cpu.last_instruction);

            let actual_cycles = self.cpu.cycle - cycle_before;
            let penultimate = actual_cycles.saturating_sub(2);
            let deadline_sub =
                self.instruction_start_sub + penultimate * self.region.master_clocks_per_cpu;
            self.irq_sample_deadline =
                self.instruction_start_dot + deadline_sub / self.region.master_clocks_per_ppu + 1;

            self.irq_allowed = if opcode == opcodes::RTI {
                !self.cpu.interrupt_flag()
            } else {
                !i_flag_before
            };

            if self.branch_irq_suppressed {
                self.irq_allowed = false;
            }
            if self.nmi_countdown > 0 {
                self.nmi_countdown -= 1;
            }
        }

        let ppu_dot_before_step = self.ppu.last_synced_dot;
        self.master_clock_sub += self.region.master_clocks_per_cpu;
        let ppu_advance = self.master_clock_sub / self.region.master_clocks_per_ppu;
        self.master_clock_sub %= self.region.master_clocks_per_ppu;
        let target_dot = self.master_clock + ppu_advance;
        self.sync_ppu_to_dot(target_dot);
        self.master_clock = target_dot;

        self.apu.cycle(self.master_clock, &mut *self.mem);
        self.mem.cpu_cycle(self.ppu.last_synced_dot);

        if let Some(edge_dot) = self.ppu.nmi_rising_edge_dot.take() {
            if self.should_trigger_nmi && self.nmi_countdown < 0 {
                if edge_dot < ppu_dot_before_step {
                    self.nmi_countdown = if edge_dot <= self.irq_sample_deadline {
                        1
                    } else {
                        2
                    };
                } else {
                    self.nmi_countdown = if edge_dot <= self.irq_sample_deadline {
                        0
                    } else {
                        1
                    };
                }
            }
        }
        if self.should_trigger_nmi && self.nmi_countdown == 0 {
            self.trigger_nmi();
            self.nmi_countdown = -1;
        }

        self.cycles += 1;
    }

    fn reemulate_frame(&mut self) {
        let target_frame = self.ppu.frames + 1;
        while self.ppu.frames < target_frame {
            self.run_cycle_core();
            self.apu.get_audio_samples();
        }
        self.audio.clear();
    }

    fn rewind_step(&mut self) -> RewindStepResult {
        if self.rewind_buffer.step_back_into(&mut self.rewind_scratch) {
            let scratch = std::mem::take(&mut self.rewind_scratch);
            let _ = self.load_state_from_bytes(&scratch);
            self.rewind_scratch = scratch;
            self.reemulate_frame();
        }
        let secs = self.rewind_buffer.rewind_remaining() as f64 / rewind::CAPTURES_PER_SECOND;
        self.overlay.set_rewind_status(Some(format!("{secs:.1}s")));
        self.overlay.draw(&mut self.buf);
        if self.overscan && self.region.region != region::Region::Pal {
            self.buf.mask_overscan();
        }
        self.iohandler.render(&self.buf);
        let result = self.iohandler.poll(&mut *self.mem, &mut self.apu);
        if result.exit {
            return RewindStepResult::Exit;
        }
        if result.rewind {
            RewindStepResult::Continue
        } else {
            RewindStepResult::Done
        }
    }

    pub fn run_one_frame(&mut self) -> bool {
        if self.rewinding {
            match self.rewind_step() {
                RewindStepResult::Continue => return true,
                RewindStepResult::Done => {
                    self.rewinding = false;
                    self.rewind_buffer.finish_rewind();
                    self.overlay.set_rewind_status(None);
                    self.audio.clear();
                    // Fall through to run a normal frame immediately (matches desktop run() behavior)
                }
                RewindStepResult::Exit => return false,
            }
        }
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
        if self.cpu.cycle == self.cycles {
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
                && !self.ppu.vblank_nmi_is_enabled()
                && (self.cpu.interrupt_flag() || !self.mem.poll_irq())
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
                    self.iohandler
                        .log("reached probable end of code".to_string());
                    state = CycleState::Exiting
                }
            }
        }

        self.run_cycle_core();

        if self.debug_active {
            if self.ppu.scanline >= 240 && !self.debug_chr_captured_this_frame {
                self.debug_chr_snapshot.resize(0x2000, 0);
                for (i, byte) in self.debug_chr_snapshot.iter_mut().enumerate() {
                    *byte = self.mem.ppu_read(i as u16);
                }
                for i in 0..32u16 {
                    self.debug_palette_snapshot[i as usize] = self.mem.ppu_read(0x3F00 + i);
                }
                self.debug_chr_captured_this_frame = true;
            }
            if self.ppu.scanline == 0 && self.debug_chr_captured_this_frame {
                self.debug_chr_captured_this_frame = false;
            }
        }

        let samples = self.apu.get_audio_samples();
        if !samples.is_empty() {
            self.audio.push_samples(samples);
        }

        if self.ppu.frames > self.last_rendered_frame {
            self.last_rendered_frame = self.ppu.frames;
            if self.static_noise {
                self.fill_static_noise();
            }
            if !self.rewinding {
                self.rewind_capture_tick = !self.rewind_capture_tick;
                if self.rewind_capture_tick {
                    let state = self.save_state_to_bytes();
                    self.rewind_buffer.push(&state);
                }
            }
            if let Some(ms) = self.iohandler.frame_time_ms() {
                self.overlay.set_frame_time(ms);
            }
            self.overlay.tick();
            self.overlay.draw(&mut self.buf);
            if self.overscan && self.region.region != region::Region::Pal {
                self.buf.mask_overscan();
            }
            if self.debug_active {
                let snapshot = self.debug_snapshot();
                self.iohandler.set_debug_snapshot(snapshot);
            }
            self.iohandler.render(&self.buf);
        }
        // ~1000 Hz input polling (1,789,773 CPU Hz / 1790 ≈ 1 ms)
        if self.cycles.is_multiple_of(self.region.input_poll_interval) {
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
            if result.rewind && !self.rewind_buffer.is_empty() {
                self.rewinding = true;
                self.rewind_buffer.begin_rewind();
                self.audio.clear();
            }
            self.overlay.set_fast_forward(result.fast_forward);
            if result.toggle_overlay {
                self.overlay.toggle();
            }
            if result.toggle_debug {
                self.toggle_debug();
            }
            if result.toggle_pause {
                self.paused = true;
                self.paused_frame = self.buf.data.clone();
                self.audio.clear();
                self.overlay.toast("PAUSED".into());
            }
            if let Some(overscan) = result.set_overscan {
                self.overscan = overscan;
            }
            if result.open_rom.is_some() {
                self.pending_open_rom = result.open_rom;
                state = CycleState::Exiting;
            }
            for msg in result.toasts {
                self.overlay.toast(msg);
            }
        }

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
        self.instruction_start_sub = self.master_clock_sub;
        self.cpu_bus_cycle_offset = 0;
        self.branch_irq_suppressed = false;
    }

    fn cpu_bus_access_dot(&self) -> u64 {
        let total_sub = self.instruction_start_sub
            + u64::from(self.cpu_bus_cycle_offset) * self.region.master_clocks_per_cpu;
        self.instruction_start_dot + total_sub / self.region.master_clocks_per_ppu
    }

    /// CPU address decoded as a PPU register access, if this mapper exposes the NES PPU register bus.
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

        for scanline in &cycle_260_scanlines[..cycle_260_count] {
            self.mem.ppu_cycle_260(*scanline);
        }
    }

    fn sync_for_cpu_access(&mut self, addr: u16, is_write: bool) {
        if self.cpu_access_needs_ppu_sync(addr, is_write) {
            let target_dot = self.cpu_bus_access_dot();
            self.sync_ppu_to_dot(target_dot);
        }
    }

    fn service_pending_irq(&mut self) -> bool {
        if !self.irq_allowed {
            return false;
        }

        if self.mem.poll_irq() && self.mem.poll_irq_at_dot(self.irq_sample_deadline) {
            self.trigger_irq();
            self.irq_allowed = false;
            return true;
        }

        if self.apu.frame_irq_at_dot(self.irq_sample_deadline) {
            self.trigger_irq();
            self.irq_allowed = false;
            return true;
        }

        if self.apu.dmc_irq_pending() {
            self.trigger_irq();
            self.irq_allowed = false;
            return true;
        }

        false
    }

    fn cpu_read(&mut self, addr: u16) -> u8 {
        self.sync_for_cpu_access(addr, false);
        let ppu_reg = self.ppu_reg_cpu_addr(addr);
        // 2A03-internal registers ($4015, $4016, $4017) don't drive the external data bus
        let (value, updates_bus) = if let Some(reg) = ppu_reg {
            let v = self.ppu.read(reg, &*self.mem);
            if reg == ppu::DATA_ADDR {
                let a = self.ppu.get_current_vram_addr();
                self.mem.ppu_a12_transition(a, self.ppu.last_synced_dot);
            }
            (v, true)
        } else if addr == APU_STATUS_ADDR {
            let v = (self.apu.read(addr) & 0xDF) | (self.cpu_open_bus & 0x20);
            (v, false)
        } else if addr == CONTROLLER1_ADDR {
            let v = (self.cpu_open_bus & OPEN_BUS_UPPER_MASK)
                | (self.mem.controllers()[0].poll() & CONTROLLER_DATA_MASK);
            (v, false)
        } else if addr == CONTROLLER2_ADDR {
            let v = (self.cpu_open_bus & OPEN_BUS_UPPER_MASK)
                | (self.mem.controllers()[1].poll() & CONTROLLER_DATA_MASK);
            (v, false)
        } else if (APU_REG_START..APU_STATUS_ADDR).contains(&addr)
            || (IO_EXPANSION_START..IO_EXPANSION_END).contains(&addr)
            || self.mem.is_cpu_open_bus(addr)
        {
            (self.cpu_open_bus, false)
        } else {
            (self.mem.cpu_read(addr), true)
        };
        if updates_bus {
            self.cpu_open_bus = value;
        }
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
                self.cpu_read(self.cpu.pc.wrapping_add(1));
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
                // Cycle 2: dummy read of the byte after BRK (the "signature byte")
                self.cpu_read(self.cpu.pc.wrapping_add(1));

                // Sync PPU to the vector fetch point (cycle 5 of 7) so NMI
                // edges that arrive during BRK can hijack the vector.
                let vf_sub = self.instruction_start_sub + 3 * self.region.master_clocks_per_cpu;
                let vector_fetch_dot =
                    self.instruction_start_dot + vf_sub / self.region.master_clocks_per_ppu;
                self.sync_ppu_to_dot(vector_fetch_dot);
                let addr = self.nmi_hijack_vector();

                if addr > 0 {
                    self.push_pc_to_stack(2);
                    self.log_push(addr);
                    self.push_to_stack(self.cpu.status | cpu::BREAK_BIT);
                    self.cpu.set_status_flag(cpu::INTERRUPT_BIT);
                    self.cpu.pc = addr;
                }
                self.cpu.pc = self.cpu.pc.wrapping_sub(self.lookup.size(opcode));
            }
            opcodes::CLC => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.clear_status_flag(cpu::CARRY_BIT);
            }
            opcodes::CLV => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.clear_status_flag(cpu::OVERFLOW_BIT);
            }
            opcodes::CLD => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.clear_status_flag(cpu::DECIMAL_BIT);
            }
            opcodes::CLI => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
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
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.x = self.cpu.x.wrapping_sub(1);
                self.cpu.check_negative(self.cpu.x);
                self.cpu.check_zero(self.cpu.x);
            }
            opcodes::DEY => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.y = self.cpu.y.wrapping_sub(1);
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
                // 6502 JSR cycle sequence:
                // 2: fetch ADL from PC+1
                // 3: internal (dummy read from stack)
                // 4: push PCH, SP--
                // 5: push PCL, SP--
                // 6: fetch ADH from PC+2
                let adl = self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu_read(self.stack_addr(self.cpu.sp));
                self.push_pc_to_stack(2);
                let adh = self.cpu_read(self.cpu.pc.wrapping_add(2));
                let addr = memory::to_16b_addr(adh, adl);
                self.log_push(addr);
                self.cpu.pc = addr;
                self.cpu.pc = self.cpu.pc.wrapping_sub(self.lookup.size(opcode));
            }

            opcodes::INC_ABS | opcodes::INC_ABX | opcodes::INC_ZP | opcodes::INC_ZPX => {
                let addr = self.addr(opcode);
                self.inc(addr);
            }

            opcodes::INX => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.x = self.cpu.x.wrapping_add(1);
                self.cpu.check_negative(self.cpu.x);
                self.cpu.check_zero(self.cpu.x);
            }
            opcodes::INY => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.y = self.cpu.y.wrapping_add(1);
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
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.log_push(self.cpu.a as u16);
                self.cpu.a = self.cpu.lsr(self.cpu.a);
                self.log_push(self.cpu.a as u16);
            }
            opcodes::LSR_ABS | opcodes::LSR_ABX | opcodes::LSR_ZP | opcodes::LSR_ZPX => {
                let addr = self.addr(opcode);
                self.lsr(addr);
            }

            opcodes::NOP => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
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
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.push_to_stack(self.cpu.a);
            }
            opcodes::PLA => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.a = self.pull_from_stack();
                self.cpu.check_negative(self.cpu.a);
                self.cpu.check_zero(self.cpu.a);
            }
            opcodes::PHP => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.push_to_stack(self.cpu.status | cpu::BREAK_BIT);
            }
            opcodes::PLP => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.status = self.pull_from_stack();
                self.cpu.set_status_flag(cpu::IGNORE_BIT);
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
                // Cycle 2: dummy read of the byte after RTI
                self.cpu_read(self.cpu.pc.wrapping_add(1));

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
                // Cycle 2: dummy read of the byte after RTS
                self.cpu_read(self.cpu.pc.wrapping_add(1));

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
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.log_push(self.cpu.a as u16);
                self.cpu.a = self.cpu.rol(self.cpu.a);
                self.log_push(self.cpu.a as u16);
            }
            opcodes::ROL_ABS | opcodes::ROL_ABX | opcodes::ROL_ZP | opcodes::ROL_ZPX => {
                let addr = self.addr(opcode);
                self.rol(addr);
            }

            opcodes::ROR => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
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
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.set_status_flag(cpu::CARRY_BIT);
            }
            opcodes::SED => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.set_status_flag(cpu::DECIMAL_BIT);
            }
            opcodes::SEI => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
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
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.x = self.cpu.a;
                self.cpu.check_negative(self.cpu.x);
                self.cpu.check_zero(self.cpu.x);
            }
            opcodes::TXA => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.a = self.cpu.x;
                self.cpu.check_negative(self.cpu.a);
                self.cpu.check_zero(self.cpu.a);
            }
            opcodes::TAY => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.y = self.cpu.a;
                self.cpu.check_negative(self.cpu.y);
                self.cpu.check_zero(self.cpu.y);
            }
            opcodes::TYA => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.a = self.cpu.y;
                self.cpu.check_negative(self.cpu.a);
                self.cpu.check_zero(self.cpu.a);
            }
            opcodes::TSX => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.x = self.cpu.sp;
                self.cpu.check_negative(self.cpu.x);
                self.cpu.check_zero(self.cpu.x);
            }
            opcodes::TXS => {
                self.cpu_read(self.cpu.pc.wrapping_add(1));
                self.cpu.sp = self.cpu.x;
            }
            opcodes::ANC_0B | opcodes::ANC_2B => {
                let addr = self.addr(opcode);
                let value = self.cpu_read(addr);
                self.cpu.and(value);
                if self.cpu.negative_flag() {
                    self.cpu.set_status_flag(cpu::CARRY_BIT);
                } else {
                    self.cpu.clear_status_flag(cpu::CARRY_BIT);
                }
            }
            opcodes::ALR_IMM => {
                let addr = self.addr(opcode);
                let value = self.cpu_read(addr);
                self.cpu.a &= value;
                let old_bit0 = self.cpu.a & 1;
                self.cpu.a >>= 1;
                self.cpu.check_negative(self.cpu.a);
                self.cpu.check_zero(self.cpu.a);
                if old_bit0 != 0 {
                    self.cpu.set_status_flag(cpu::CARRY_BIT);
                } else {
                    self.cpu.clear_status_flag(cpu::CARRY_BIT);
                }
            }
            opcodes::ARR_IMM => {
                let addr = self.addr(opcode);
                let value = self.cpu_read(addr);
                self.cpu.a &= value;
                let old_carry = if self.cpu.carry_flag() { 1u8 } else { 0u8 };
                self.cpu.a = (self.cpu.a >> 1) | (old_carry << 7);
                self.cpu.check_negative(self.cpu.a);
                self.cpu.check_zero(self.cpu.a);
                let bit6 = (self.cpu.a >> 6) & 1;
                let bit5 = (self.cpu.a >> 5) & 1;
                if bit6 != 0 {
                    self.cpu.set_status_flag(cpu::CARRY_BIT);
                } else {
                    self.cpu.clear_status_flag(cpu::CARRY_BIT);
                }
                if (bit6 ^ bit5) != 0 {
                    self.cpu.set_status_flag(cpu::OVERFLOW_BIT);
                } else {
                    self.cpu.clear_status_flag(cpu::OVERFLOW_BIT);
                }
            }
            opcodes::XAA_IMM => {
                let addr = self.addr(opcode);
                let value = self.cpu_read(addr);
                self.cpu.a = self.cpu.x & value;
                self.cpu.check_negative(self.cpu.a);
                self.cpu.check_zero(self.cpu.a);
            }
            opcodes::LAX_IMM => {
                let addr = self.addr(opcode);
                let value = self.cpu_read(addr);
                self.cpu.a = value;
                self.cpu.x = value;
                self.cpu.check_negative(value);
                self.cpu.check_zero(value);
            }
            opcodes::SBX_IMM => {
                let addr = self.addr(opcode);
                let value = self.cpu_read(addr);
                let ax = self.cpu.a & self.cpu.x;
                let result = (ax as u16).wrapping_sub(value as u16);
                self.cpu.x = result as u8;
                self.cpu.check_negative(self.cpu.x);
                self.cpu.check_zero(self.cpu.x);
                if ax >= value {
                    self.cpu.set_status_flag(cpu::CARRY_BIT);
                } else {
                    self.cpu.clear_status_flag(cpu::CARRY_BIT);
                }
            }
            opcodes::SHA_INY => {
                let (addr, uncorrected, crossed) = self.addr_indirect_idx(self.cpu.pc, self.cpu.y);
                self.cpu_read(uncorrected);
                let high = (uncorrected >> 8) as u8;
                let value = self.cpu.a & self.cpu.x & high.wrapping_add(1);
                let write_addr = if crossed {
                    memory::to_16b_addr(value, addr as u8)
                } else {
                    addr
                };
                self.cpu_write(write_addr, value);
            }
            opcodes::SHA_ABY => {
                let (addr, uncorrected, crossed) = self.addr_absolute_idx(self.cpu.pc, self.cpu.y);
                self.cpu_read(uncorrected);
                let high = (uncorrected >> 8) as u8;
                let value = self.cpu.a & self.cpu.x & high.wrapping_add(1);
                let write_addr = if crossed {
                    memory::to_16b_addr(value, addr as u8)
                } else {
                    addr
                };
                self.cpu_write(write_addr, value);
            }
            opcodes::TAS_ABY => {
                let (addr, uncorrected, crossed) = self.addr_absolute_idx(self.cpu.pc, self.cpu.y);
                self.cpu_read(uncorrected);
                self.cpu.sp = self.cpu.a & self.cpu.x;
                let high = (uncorrected >> 8) as u8;
                let value = self.cpu.sp & high.wrapping_add(1);
                let write_addr = if crossed {
                    memory::to_16b_addr(value, addr as u8)
                } else {
                    addr
                };
                self.cpu_write(write_addr, value);
            }
            opcodes::SHY_ABX => {
                let (addr, uncorrected, crossed) = self.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                self.cpu_read(uncorrected);
                let high = (uncorrected >> 8) as u8;
                let value = self.cpu.y & high.wrapping_add(1);
                let write_addr = if crossed {
                    memory::to_16b_addr(value, addr as u8)
                } else {
                    addr
                };
                self.cpu_write(write_addr, value);
            }
            opcodes::SHX_ABY => {
                let (addr, uncorrected, crossed) = self.addr_absolute_idx(self.cpu.pc, self.cpu.y);
                self.cpu_read(uncorrected);
                let high = (uncorrected >> 8) as u8;
                let value = self.cpu.x & high.wrapping_add(1);
                let write_addr = if crossed {
                    memory::to_16b_addr(value, addr as u8)
                } else {
                    addr
                };
                self.cpu_write(write_addr, value);
            }
            opcodes::LAS_ABY => {
                let addr = self.addr(opcode);
                let value = self.cpu_read(addr) & self.cpu.sp;
                self.cpu.a = value;
                self.cpu.x = value;
                self.cpu.sp = value;
                self.cpu.check_negative(value);
                self.cpu.check_zero(value);
            }
            _ => {
                let mode = self.lookup.mode(opcode);
                if mode != opcodes::ADDR_MODE_NA {
                    let addr = self.addr(opcode);
                    self.cpu_read(addr);
                } else {
                    self.cpu_read(self.cpu.pc.wrapping_add(1));
                }
            }
        }

        self.cpu.pc = self.cpu.pc.wrapping_add(size);
        self.instructions += 1;
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

    // RMW instructions write the original value back before writing the result.
    // This double-write is observable via PPU/APU register side effects.
    fn asl(&mut self, addr: u16) -> u8 {
        let value: u8 = self.cpu_read(addr);
        self.log_push(value as u16);
        let result: u8 = self.cpu.asl(value);
        self.log_push(result as u16);
        self.cpu_write(addr, value);
        self.cpu_write(addr, result);

        result
    }

    fn branch(&mut self, operand: i8) {
        let page_cross = (self.cpu.pc.wrapping_add(2) & 0xff).wrapping_add(operand as u16) > 0xff;
        if page_cross {
            self.cpu.cycle += 1;
        } else {
            self.branch_irq_suppressed = true;
        }
        self.cpu.pc = self.cpu.pc.wrapping_add(operand as u16);
        self.log_push(self.cpu.pc.wrapping_add(2));
        self.cpu.cycle += 1;
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        self.cpu_open_bus = value;
        self.sync_for_cpu_access(addr, true);

        let ppu_reg = self.ppu_reg_cpu_addr(addr);

        if let Some(reg) = ppu_reg {
            if self.cpu.cycle < self.ppu_register_warmup_until_cpu_cycle {
                self.cpu_bus_cycle_offset = self.cpu_bus_cycle_offset.wrapping_add(1);
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
            if reg == ppu::DATA_ADDR || (reg == ppu::ADDR_ADDR && !self.ppu.write_toggle()) {
                self.mem
                    .ppu_a12_transition(self.ppu.get_current_vram_addr(), self.ppu.last_synced_dot);
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
            let alignment: u64 = if self.cpu.cycle % 2 == 1 { 1 } else { 0 };
            self.cpu.cycle += 512 + 1 + alignment;
        } else if addr == CONTROLLER1_ADDR {
            let strobe = value & 1 != 0;
            self.mem.controllers()[0].set_strobe(strobe);
            self.mem.controllers()[1].set_strobe(strobe);
        } else if (APU_REG_START..=APU_STATUS_ADDR).contains(&addr) || addr == CONTROLLER2_ADDR {
            let apu_cycle_tag = if addr == CONTROLLER2_ADDR {
                self.cpu
                    .cycle
                    .wrapping_add(u64::from(self.cpu_bus_cycle_offset))
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
        self.cpu_write(addr, operand);
        self.cpu_write(addr, value);

        self.cpu.check_negative(value);
        self.cpu.check_zero(value);
    }

    fn dcp(&mut self, addr: u16) {
        let operand: u8 = self.cpu_read(addr);
        self.log_push(operand as u16);
        let value: u8 = operand.wrapping_sub(1);
        self.cpu_write(addr, operand);
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
        self.cpu_write(addr, operand);
        self.cpu_write(addr, value);

        self.cpu.check_negative(value);
        self.cpu.check_zero(value);
    }

    fn isb(&mut self, addr: u16) {
        let operand: u8 = self.cpu_read(addr);
        self.log_push(operand as u16);

        let value: u8 = operand.wrapping_add(1);
        self.cpu_write(addr, operand);
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
        self.cpu_write(addr, value);
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
        self.cpu_write(addr, value);
        self.cpu_write(addr, result);

        result
    }

    fn ror(&mut self, addr: u16) -> u8 {
        let value: u8 = self.cpu_read(addr);
        self.log_push(value as u16);
        let result: u8 = self.cpu.ror(value);
        self.log_push(result as u16);
        self.cpu_write(addr, value);
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
        let lb = self.cpu_read(offset);
        let hb = self.cpu_read(offset.wrapping_add(1));
        memory::to_16b_addr(hb, lb)
    }

    fn addr_absolute(&mut self, pc: u16) -> u16 {
        self.get_16b_addr(pc.wrapping_add(1))
    }

    fn addr_absolute_idx(&mut self, pc: u16, idx: u8) -> (u16, u16, bool) {
        let lb = self.cpu_read(pc.wrapping_add(1));
        let hb = self.cpu_read(pc.wrapping_add(2));
        let base = memory::to_16b_addr(hb, lb);
        let addr = base.wrapping_add(idx as u16);
        let crossed = (lb as u16 + idx as u16) > 0xff;
        let uncorrected = memory::to_16b_addr(hb, lb.wrapping_add(idx));
        (addr, uncorrected, crossed)
    }

    fn addr_idx_indirect(&mut self, pc: u16, idx: u8) -> u16 {
        let value: u8 = self.cpu_read(pc + 1).wrapping_add(idx);
        ((self.cpu_read(value.wrapping_add(1) as u16) as u16) << 8)
            + self.cpu_read(value as u16) as u16
    }

    fn addr_indirect_idx(&mut self, pc: u16, idx: u8) -> (u16, u16, bool) {
        let base = self.cpu_read(pc + 1);

        let lb = self.cpu_read(base as _);
        let hb = self.cpu_read(base.wrapping_add(1) as _);
        let lbidx = lb.wrapping_add(idx);
        let crossed = (lb as u16 + idx as u16) > 0xff;
        let addr = memory::to_16b_addr(hb.wrapping_add(if crossed { 1 } else { 0 }), lbidx);
        let uncorrected = memory::to_16b_addr(hb, lbidx);

        (addr, uncorrected, crossed)
    }

    fn addr_zeropage(&mut self, pc: u16) -> u16 {
        self.cpu_read(pc.wrapping_add(1)) as _
    }

    fn addr_zeropage_idx(&mut self, pc: u16, idx: u8) -> u16 {
        self.cpu_read(pc.wrapping_add(1)).wrapping_add(idx) as u16
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
                let (addr, uncorrected, crossed) = self.addr_absolute_idx(self.cpu.pc, self.cpu.x);
                let has_penalty = self.lookup.page_boundary_penalty(opcode);
                if has_penalty && crossed {
                    self.cpu_read(uncorrected);
                    self.cpu.cycle += 1;
                } else if !has_penalty {
                    self.cpu_read(uncorrected);
                }
                addr
            }
            opcodes::ADDR_MODE_ABY => {
                let (addr, uncorrected, crossed) = self.addr_absolute_idx(self.cpu.pc, self.cpu.y);
                let has_penalty = self.lookup.page_boundary_penalty(opcode);
                if has_penalty && crossed {
                    self.cpu_read(uncorrected);
                    self.cpu.cycle += 1;
                } else if !has_penalty {
                    self.cpu_read(uncorrected);
                }
                addr
            }
            opcodes::ADDR_MODE_IMM => self.cpu.pc + 1,
            opcodes::ADDR_MODE_INX => self.addr_idx_indirect(self.cpu.pc, self.cpu.x),
            opcodes::ADDR_MODE_INY => {
                let (addr, uncorrected, crossed) = self.addr_indirect_idx(self.cpu.pc, self.cpu.y);
                let has_penalty = self.lookup.page_boundary_penalty(opcode);
                if has_penalty && crossed {
                    self.cpu_read(uncorrected);
                    self.cpu.cycle += 1;
                } else if !has_penalty {
                    self.cpu_read(uncorrected);
                }
                addr
            }
            opcodes::ADDR_MODE_NA => 0,
            opcodes::ADDR_MODE_ZP => self.addr_zeropage(self.cpu.pc),
            opcodes::ADDR_MODE_ZPX => self.addr_zeropage_idx(self.cpu.pc, self.cpu.x),
            opcodes::ADDR_MODE_ZPY => self.addr_zeropage_idx(self.cpu.pc, self.cpu.y),
            _ => panic!("Addressing mode not found for opcode {opcode:x}"),
        }
    }

    fn nmi_hijack_vector(&mut self) -> u16 {
        if self.nmi_countdown >= 0 || self.ppu.nmi_rising_edge_dot.is_some() {
            self.nmi_countdown = -1;
            self.ppu.nmi_rising_edge_dot = None;
            self.get_16b_addr(memory::NMI_TARGET_ADDR)
        } else {
            self.get_16b_addr(memory::BRK_TARGET_ADDR)
        }
    }

    pub fn trigger_nmi(&mut self) {
        let addr: u16 = self.get_16b_addr(memory::NMI_TARGET_ADDR);
        self.push_pc_to_stack(0);
        self.push_to_stack(self.cpu.status & !cpu::BREAK_BIT);
        self.cpu.set_status_flag(cpu::INTERRUPT_BIT);
        self.cpu.pc = addr;
        self.cpu.cycle += 7;
    }

    pub fn trigger_irq(&mut self) {
        self.push_pc_to_stack(0);
        self.push_to_stack(self.cpu.status & !cpu::BREAK_BIT);
        self.cpu.set_status_flag(cpu::INTERRUPT_BIT);
        let addr = self.nmi_hijack_vector();
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
            &[],
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

        let elapsed_secs = self.cycles as f64 / self.region.cpu_clock_rate;
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
        assert!(emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::AND_IMM);
        emu.mem.cpu_write(start + 1, 0);
        emu.run();
        assert_eq!(emu.cpu.a, 0);
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());
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
        assert!(emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::AND_IMM);
        emu.mem.cpu_write(0x02, 0);
        emu.run();
        assert_eq!(emu.cpu.a, 0);
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());
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
        assert!(emu.cpu.carry_flag());
        assert!(emu.cpu.zero_flag());
        assert!(!emu.cpu.negative_flag());
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

        assert!(emu.cpu.negative_flag());
        assert!(emu.cpu.overflow_flag());
        assert!(!emu.cpu.zero_flag());

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.cpu_write(0x4711, 0b0100_0000);
        emu.cpu.a = 0b1000_0001;
        emu.run();

        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.overflow_flag());
        assert!(emu.cpu.zero_flag());
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

        assert!(emu.cpu.negative_flag());
        assert!(emu.cpu.overflow_flag());
        assert!(!emu.cpu.zero_flag());

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.cpu_write(0x01, 0b0100_0000);
        emu.cpu.a = 0b1000_0001;
        emu.run();

        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.overflow_flag());
        assert!(emu.cpu.zero_flag());
    }

    #[test]
    fn test_brk() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::BRK);
        emu.run();

        assert!(!emu.cpu.interrupt_flag());
        assert_eq!(emu.cpu.pc, 0x600);

        emu.mem.cpu_write(0xffff, 0x47);
        emu.mem.cpu_write(0xfffe, 0x11);
        emu.cpu.set_status_flag(cpu::NEGATIVE_BIT);
        emu.run();

        assert!(emu.cpu.interrupt_flag());
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

        assert!(!emu.cpu.carry_flag());
    }

    #[test]
    fn test_cld() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::CLD);
        emu.cpu.set_status_flag(cpu::DECIMAL_BIT);
        emu.run();

        assert!(!emu.cpu.decimal_flag());
    }

    #[test]
    fn test_cli() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::CLI);
        emu.cpu.set_status_flag(cpu::INTERRUPT_BIT);
        emu.run();

        assert!(!emu.cpu.interrupt_flag());
    }

    #[test]
    fn test_clv() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::CLV);
        emu.cpu.set_status_flag(cpu::OVERFLOW_BIT);
        emu.run();

        assert!(!emu.cpu.overflow_flag());
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

        assert!(emu.cpu.carry_flag());
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0);

        emu.run();

        assert!(emu.cpu.carry_flag());
        assert!(!emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.mem.cpu_write(start, opcodes::CMP_ABS);
        emu.mem.cpu_write(start + 1, 0x11);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 1);

        emu.run();

        assert!(!emu.cpu.carry_flag());
        assert!(emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());
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

        assert!(emu.cpu.carry_flag());
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 1;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ABX);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0);

        emu.run();

        assert!(emu.cpu.carry_flag());
        assert!(!emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ABX);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 1);

        emu.run();

        assert!(!emu.cpu.carry_flag());
        assert!(emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());
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

        assert!(emu.cpu.carry_flag());
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 1;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ABY);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 0);

        emu.run();

        assert!(emu.cpu.carry_flag());
        assert!(!emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ABY);
        emu.mem.cpu_write(start + 1, 0x10);
        emu.mem.cpu_write(start + 2, 0x47);
        emu.mem.cpu_write(0x4711, 1);

        emu.run();

        assert!(!emu.cpu.carry_flag());
        assert!(emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());
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

        assert!(emu.cpu.carry_flag());
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 1;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_IMM);
        emu.mem.cpu_write(start + 1, 0);

        emu.run();

        assert!(emu.cpu.carry_flag());
        assert!(!emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_IMM);
        emu.mem.cpu_write(start + 1, 1);

        emu.run();

        assert!(!emu.cpu.carry_flag());
        assert!(emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());
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

        assert!(emu.cpu.carry_flag());
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 1;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_INX);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x42, 0x11);
        emu.mem.cpu_write(0x43, 0x47);
        emu.mem.cpu_write(0x4711, 0);

        emu.run();

        assert!(emu.cpu.carry_flag());
        assert!(!emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_INX);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x42, 0x11);
        emu.mem.cpu_write(0x43, 0x47);
        emu.mem.cpu_write(0x4711, 1);

        emu.run();

        assert!(!emu.cpu.carry_flag());
        assert!(emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());
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

        assert!(emu.cpu.carry_flag());
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 1;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::CMP_INY);
        emu.mem.cpu_write(start + 1, 0x42);
        emu.mem.cpu_write(0x42, 0x10);
        emu.mem.cpu_write(0x43, 0x47);
        emu.mem.cpu_write(0x4711, 0);

        emu.run();

        assert!(emu.cpu.carry_flag());
        assert!(!emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.y = 1;
        emu.mem.cpu_write(start, opcodes::CMP_INY);
        emu.mem.cpu_write(start + 1, 0x42);
        emu.mem.cpu_write(0x42, 0x10);
        emu.mem.cpu_write(0x43, 0x47);
        emu.mem.cpu_write(0x4711, 1);

        emu.run();

        assert!(!emu.cpu.carry_flag());
        assert!(emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());
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

        assert!(emu.cpu.carry_flag());
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 1;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ZPX);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x42, 0);

        emu.run();

        assert!(emu.cpu.carry_flag());
        assert!(!emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.a = 0;
        emu.cpu.x = 1;
        emu.mem.cpu_write(start, opcodes::CMP_ZPX);
        emu.mem.cpu_write(start + 1, 0x41);
        emu.mem.cpu_write(0x42, 1);

        emu.run();

        assert!(!emu.cpu.carry_flag());
        assert!(emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());
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
        assert!(emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.cpu_write(0x01, 1);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x01), 0);
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());
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
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());
        assert!(emu.cpu.carry_flag());

        emu.cpu.pc = start;
        emu.cpu.a = 0;
        emu.mem.cpu_write(0x01, 0x00);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x01), 0xff);
        assert!(!emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());
        assert!(!emu.cpu.carry_flag());

        emu.cpu.pc = start;
        emu.cpu.a = 1;
        emu.mem.cpu_write(0x01, 0x01);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x01), 0);
        assert!(!emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());
        assert!(emu.cpu.carry_flag());
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
        assert!(emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());
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
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.cpu_write(0x01, 127);
        emu.run();

        assert_eq!(emu.mem.cpu_read(0x01), 128);
        assert!(emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());
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
        assert!(!emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());
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
        assert!(!emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());
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
        assert!(emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.cpu_write(0x4711, 0);
        emu.run();

        assert_eq!(emu.cpu.a, 0);
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());
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
        assert!(emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.mem.cpu_write(0x41, 0);
        emu.run();

        assert_eq!(emu.cpu.x, 0);
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());
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
        assert!(!emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());
        assert!(emu.cpu.carry_flag());

        emu.cpu.a = 0;
        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.run();

        assert_eq!(emu.cpu.a, 0x0);
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());
        assert!(!emu.cpu.carry_flag());
    }

    #[test]
    fn test_nop() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::NOP);

        emu.run();

        assert_eq!(emu.cpu.pc, start + 1);
        assert_eq!(emu.cpu.a, 0x0);
        assert_eq!(emu.cpu.x, 0x0);
        assert_eq!(emu.cpu.y, 0x0);
        assert_eq!(emu.cpu.sp, 0xff);
        assert_eq!(emu.cpu.status, 0b0010_0000);
    }

    #[test]
    fn test_nop_abs_reads_target_clears_vbl() {
        let mut emu: Emulator = Emulator::_new();
        let start = memory::CODE_START_ADDR;

        emu.ppu.set_vblank_for_test(true);
        assert!(emu.ppu.is_in_vblank());

        emu.mem.cpu_write(start, opcodes::NOP_0C);
        emu.mem.cpu_write(start + 1, ppu::STATUS_REG_ADDR as u8);
        emu.mem
            .cpu_write(start + 2, (ppu::STATUS_REG_ADDR >> 8) as u8);
        // LDA $2002 after NOP to read back the status
        emu.mem.cpu_write(start + 3, opcodes::LDA_ABS);
        emu.mem.cpu_write(start + 4, ppu::STATUS_REG_ADDR as u8);
        emu.mem
            .cpu_write(start + 5, (ppu::STATUS_REG_ADDR >> 8) as u8);

        emu.execute_instruction(); // NOP $2002
        emu.execute_instruction(); // LDA $2002

        assert_eq!(
            emu.cpu.a & ppu::STATUS_VERTICAL_BLANK_BIT,
            0,
            "NOP abs targeting $2002 must clear VBL flag"
        );
    }

    #[test]
    fn test_nop_abs_does_not_modify_registers() {
        let mut emu: Emulator = Emulator::_new();
        let start = memory::CODE_START_ADDR;
        emu.cpu.a = 0x42;
        emu.cpu.x = 0x11;
        emu.cpu.y = 0x22;

        emu.mem.cpu_write(start, opcodes::NOP_0C);
        emu.mem.cpu_write(start + 1, 0x00);
        emu.mem.cpu_write(start + 2, 0x00);

        emu.execute_instruction();

        assert_eq!(emu.cpu.a, 0x42);
        assert_eq!(emu.cpu.x, 0x11);
        assert_eq!(emu.cpu.y, 0x22);
        assert_eq!(emu.cpu.pc, start + 3);
    }

    #[test]
    fn test_nop_zp_reads_target_address() {
        let mut emu: Emulator = Emulator::_new();
        let start = memory::CODE_START_ADDR;

        emu.mem.cpu_write(0x42, 0xAA);
        emu.mem.cpu_write(start, opcodes::NOP_04);
        emu.mem.cpu_write(start + 1, 0x42);

        emu.execute_instruction();

        assert_eq!(emu.cpu.pc, start + 2);
        assert_eq!(emu.cpu.a, 0);
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
        assert!(emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());

        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.cpu.a = 0;
        emu.mem.cpu_write(start, opcodes::ORA_IMM);
        emu.mem.cpu_write(start + 1, 0);

        emu.run();
        assert_eq!(emu.cpu.a, 0);
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());
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
        assert!(!emu.cpu.overflow_flag());
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

        assert!(emu.cpu.carry_flag());
    }

    #[test]
    fn test_sed() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::SED);
        emu.run();

        assert!(emu.cpu.decimal_flag());
    }

    #[test]
    fn test_sei() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.mem.cpu_write(start, opcodes::SEI);
        emu.run();

        assert!(emu.cpu.interrupt_flag());
    }

    #[test]
    fn test_tsx() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.sp = 255;
        emu.mem.cpu_write(start, opcodes::TSX);
        emu.run();

        assert_eq!(emu.cpu.x, 255);
        assert!(emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());

        let mut emu: Emulator = Emulator::_new();
        emu.cpu.sp = 0;
        emu.mem.cpu_write(start, opcodes::TSX);
        emu.run();

        assert_eq!(emu.cpu.x, 0);
        assert!(!emu.cpu.negative_flag());
        assert!(emu.cpu.zero_flag());
    }

    #[test]
    fn test_txs() {
        let mut emu: Emulator = Emulator::_new();
        let start: u16 = memory::CODE_START_ADDR;
        emu.cpu.x = 255;
        emu.mem.cpu_write(start, opcodes::TXS);
        emu.run();

        assert_eq!(emu.cpu.sp, 255);
        assert!(!emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());
        emu.cpu.x = 0;
        emu.cpu.pc = memory::CODE_START_ADDR;
        emu.run();

        assert_eq!(emu.cpu.sp, 0);
        assert!(!emu.cpu.negative_flag());
        assert!(!emu.cpu.zero_flag());
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

    #[test]
    fn test_ntsc_master_clock_sub_always_zero() {
        let mut emu = Emulator::_new();
        assert_eq!(emu.region.region, region::Region::Ntsc);
        for _ in 0..100 {
            emu.cycle();
            assert_eq!(emu.master_clock_sub, 0, "NTSC sub should always be 0");
        }
    }

    #[test]
    fn test_pal_master_clock_sub_pattern() {
        let mapper = Box::new(memory::IdentityMapper::new(memory::CODE_START_ADDR));
        let audio = Box::new(SilentAudioOutput::new()) as Box<dyn AudioBackend>;
        let io = Box::new(io::HeadlessIOHandler {});
        let mut emu = Emulator::new_with_region(io, mapper, audio, region::Region::Pal);
        assert_eq!(emu.region.region, region::Region::Pal);

        let mut total_ppu_advance: u64 = 0;
        for i in 0..5 {
            let before = emu.master_clock;
            emu.cycle();
            total_ppu_advance += emu.master_clock - before;
            if i < 4 {
                assert_eq!(emu.master_clock_sub, (i + 1) as u64, "sub after cycle {i}");
            }
        }
        assert_eq!(emu.master_clock_sub, 0, "sub resets after 5 cycles");
        assert_eq!(total_ppu_advance, 16, "5 PAL CPU cycles = 16 PPU dots");
    }

    #[test]
    fn test_pal_ppu_scanline_count() {
        let mapper = Box::new(memory::IdentityMapper::new(memory::CODE_START_ADDR));
        let audio = Box::new(SilentAudioOutput::new()) as Box<dyn AudioBackend>;
        let io = Box::new(io::HeadlessIOHandler {});
        let emu = Emulator::new_with_region(io, mapper, audio, region::Region::Pal);
        assert_eq!(emu.ppu.pre_render_scanline, 311);
        assert_eq!(emu.ppu.num_scanlines, 312);
    }

    #[test]
    fn test_savestate_region_roundtrip() {
        let mapper = Box::new(memory::IdentityMapper::new(memory::CODE_START_ADDR));
        let audio = Box::new(SilentAudioOutput::new()) as Box<dyn AudioBackend>;
        let io = Box::new(io::HeadlessIOHandler {});
        let mut emu = Emulator::new_with_region(io, mapper, audio, region::Region::Pal);
        for _ in 0..100 {
            emu.cycle();
        }
        let saved = emu.save_state_to_bytes();
        let mc_before = emu.master_clock;
        let sub_before = emu.master_clock_sub;

        let mapper2 = Box::new(memory::IdentityMapper::new(memory::CODE_START_ADDR));
        let audio2 = Box::new(SilentAudioOutput::new()) as Box<dyn AudioBackend>;
        let io2 = Box::new(io::HeadlessIOHandler {});
        let mut emu2 = Emulator::new_with_region(io2, mapper2, audio2, region::Region::Pal);
        emu2.load_state_from_bytes(&saved).unwrap();
        assert_eq!(emu2.master_clock, mc_before);
        assert_eq!(emu2.master_clock_sub, sub_before);
    }

    #[test]
    fn test_savestate_region_mismatch_rejected() {
        let mapper = Box::new(memory::IdentityMapper::new(memory::CODE_START_ADDR));
        let audio = Box::new(SilentAudioOutput::new()) as Box<dyn AudioBackend>;
        let io = Box::new(io::HeadlessIOHandler {});
        let emu = Emulator::new_with_region(io, mapper, audio, region::Region::Pal);
        let saved = emu.save_state_to_bytes();

        let mut emu_ntsc = Emulator::_new();
        let result = emu_ntsc.load_state_from_bytes(&saved);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("region mismatch"));
    }

    #[test]
    fn test_savestate_v7_loads_as_ntsc() {
        let mut emu = Emulator::_new();
        for _ in 0..100 {
            emu.cycle();
        }
        let saved = emu.save_state_to_bytes();
        // Patch version byte back to 7 and strip the 3 new fields (u8 + u64 + u64 = 17 bytes)
        let mut v7_data = saved[..saved.len() - 17].to_vec();
        v7_data[4] = 7;
        let mut emu2 = Emulator::_new();
        emu2.load_state_from_bytes(&v7_data).unwrap();
        assert_eq!(emu2.master_clock_sub, 0);
        assert_eq!(emu2.instruction_start_sub, 0);
    }

    #[test]
    fn test_nmi_breaks_branch_to_self_loop() {
        let mut emu = Emulator::_new();
        emu.toggle_should_trigger_nmi(true);
        emu.toggle_should_exit_on_infinite_loop(false);
        emu.should_log = false;
        emu.ppu_register_warmup_until_cpu_cycle = 0;

        let code_addr: u16 = 0x0700;
        let nmi_handler: u16 = 0x0800;
        let marker_addr: u16 = 0x0010;

        // LDA #$80; STA $2000 (enable NMI); NOP; NOP; BNE $-2 (branch to self)
        // The NOPs shift timing so VBL edge falls after the BNE's IRQ sample
        // deadline, forcing nmi_countdown=1 (deferred) instead of 0 (immediate).
        emu.mem.cpu_write(code_addr, opcodes::LDA_IMM);
        emu.mem.cpu_write(code_addr + 1, 0x80);
        emu.mem.cpu_write(code_addr + 2, opcodes::STA_ABS);
        emu.mem.cpu_write(code_addr + 3, ppu::CTRL_REG_ADDR as u8);
        emu.mem
            .cpu_write(code_addr + 4, (ppu::CTRL_REG_ADDR >> 8) as u8);
        emu.mem.cpu_write(code_addr + 5, opcodes::NOP);
        emu.mem.cpu_write(code_addr + 6, opcodes::NOP);
        emu.mem.cpu_write(code_addr + 7, opcodes::BNE);
        emu.mem.cpu_write(code_addr + 8, 0xFE); // -2: branch to self

        // NMI handler: write marker and BRK
        emu.mem.cpu_write(nmi_handler, opcodes::LDA_IMM);
        emu.mem.cpu_write(nmi_handler + 1, 0x42);
        emu.mem.cpu_write(nmi_handler + 2, opcodes::STA_ZP);
        emu.mem.cpu_write(nmi_handler + 3, marker_addr as u8);
        emu.mem.cpu_write(nmi_handler + 4, opcodes::BRK);

        // Set NMI vector
        emu.mem
            .cpu_write(memory::NMI_TARGET_ADDR, nmi_handler as u8);
        emu.mem
            .cpu_write(memory::NMI_TARGET_ADDR + 1, (nmi_handler >> 8) as u8);

        emu.cpu.pc = code_addr;
        // Run enough cycles to cross VBL (scanline 241 × 341 dots / 3 ≈ 27,400 cycles)
        emu.run_for_cycles(35000);

        assert_eq!(
            emu.mem.cpu_read(marker_addr),
            0x42,
            "NMI handler should have written marker"
        );
    }
}
