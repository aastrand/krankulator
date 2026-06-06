use crate::emu::io::controller::Controller;
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

use super::{
    load_controllers, save_controllers, PALETTE_MIRROR_CLEAR, PALETTE_MIRROR_MASK, PALETTE_SIZE,
    PALETTE_START, RESET_TARGET_ADDR, VRAM_SIZE,
};

const PRG_RAM_SIZE: usize = 64 * 1024;
const EXRAM_SIZE: usize = 1024;

const DUTY_CYCLES: [u8; 4] = [
    0b01000000, // 12.5%
    0b01100000, // 25%
    0b01111000, // 50%
    0b10011111, // 75%
];

const LENGTH_COUNTER_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];

struct Mmc5Pulse {
    duty_cycle: u8,
    duty_step: u8,
    timer: u16,
    timer_value: u16,
    length_counter: u8,
    length_counter_halt: bool,
    volume: u8,
    constant_volume: bool,
    envelope_start: bool,
    envelope_divider: u8,
    envelope_decay_level: u8,
    enabled: bool,
    output: f32,
}

impl Mmc5Pulse {
    fn new() -> Self {
        Self {
            duty_cycle: 0,
            duty_step: 0,
            timer: 0,
            timer_value: 0,
            length_counter: 0,
            length_counter_halt: false,
            volume: 0,
            constant_volume: false,
            envelope_start: false,
            envelope_divider: 0,
            envelope_decay_level: 0,
            enabled: false,
            output: 0.0,
        }
    }

    fn set_control(&mut self, value: u8) {
        self.duty_cycle = (value >> 6) & 3;
        self.length_counter_halt = (value >> 5) & 1 != 0;
        self.constant_volume = (value >> 4) & 1 != 0;
        self.volume = value & 0x0F;
    }

    fn set_timer_low(&mut self, value: u8) {
        self.timer = (self.timer & 0xFF00) | value as u16;
    }

    fn set_timer_high(&mut self, value: u8) {
        self.timer = (self.timer & 0x00FF) | ((value & 0x07) as u16) << 8;
        self.timer_value = self.timer;
        if self.enabled {
            let idx = ((value >> 3) & 0x1F) as usize;
            self.length_counter = LENGTH_COUNTER_TABLE[idx];
        }
        self.duty_step = 0;
        self.envelope_start = true;
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.length_counter = 0;
        }
    }

    fn cycle(&mut self) {
        if self.timer_value == 0 {
            self.timer_value = self.timer;
            self.duty_step = (self.duty_step + 1) % 8;
        } else {
            self.timer_value -= 1;
        }
        self.generate_output();
    }

    fn generate_output(&mut self) {
        // MMC5 pulses: no sweep, no minimum period silencing
        if !self.enabled || self.length_counter == 0 {
            self.output = 0.0;
            return;
        }
        let duty_pattern = DUTY_CYCLES[self.duty_cycle as usize];
        let duty_bit = (duty_pattern >> (7 - self.duty_step)) & 1;
        if duty_bit == 0 {
            self.output = 0.0;
        } else {
            self.output = if self.constant_volume {
                self.volume
            } else {
                self.envelope_decay_level
            } as f32;
        }
    }

    fn clock_envelope(&mut self) {
        if self.envelope_start {
            self.envelope_start = false;
            self.envelope_decay_level = 15;
            self.envelope_divider = self.volume;
        } else if self.envelope_divider == 0 {
            self.envelope_divider = self.volume;
            if self.envelope_decay_level > 0 {
                self.envelope_decay_level -= 1;
            } else if self.length_counter_halt {
                self.envelope_decay_level = 15;
            }
        } else {
            self.envelope_divider -= 1;
        }
    }

    fn clock_length_counter(&mut self) {
        if !self.length_counter_halt && self.length_counter > 0 {
            self.length_counter -= 1;
        }
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_u8(self.duty_cycle);
        w.write_u8(self.duty_step);
        w.write_u16(self.timer);
        w.write_u16(self.timer_value);
        w.write_u8(self.length_counter);
        w.write_bool(self.length_counter_halt);
        w.write_u8(self.volume);
        w.write_bool(self.constant_volume);
        w.write_bool(self.envelope_start);
        w.write_u8(self.envelope_divider);
        w.write_u8(self.envelope_decay_level);
        w.write_bool(self.enabled);
        w.write_f32(self.output);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        self.duty_cycle = r.read_u8()?;
        self.duty_step = r.read_u8()?;
        self.timer = r.read_u16()?;
        self.timer_value = r.read_u16()?;
        self.length_counter = r.read_u8()?;
        self.length_counter_halt = r.read_bool()?;
        self.volume = r.read_u8()?;
        self.constant_volume = r.read_bool()?;
        self.envelope_start = r.read_bool()?;
        self.envelope_divider = r.read_u8()?;
        self.envelope_decay_level = r.read_u8()?;
        self.enabled = r.read_bool()?;
        self.output = r.read_f32()?;
        Ok(())
    }
}

pub struct MMC5Mapper {
    internal_ram: Box<[u8; 2048]>,
    prg_rom: Vec<[u8; 8192]>,
    prg_ram: Box<[u8; PRG_RAM_SIZE]>,
    chr_rom: Vec<[u8; 1024]>,
    chr_ram: Option<Vec<u8>>,

    vram: Box<[u8; VRAM_SIZE as usize]>,
    exram: Box<[u8; EXRAM_SIZE]>,
    palette_ram: [u8; PALETTE_SIZE],

    // PRG banking
    prg_mode: u8,
    prg_bank_regs: [u8; 5], // $5113-$5117
    ram_protect_1: u8,      // $5102
    ram_protect_2: u8,      // $5103

    // CHR banking
    chr_mode: u8,
    chr_bank_a: [u16; 8], // $5120-$5127
    chr_bank_b: [u16; 4], // $5128-$512B
    chr_upper_bits: u8,   // $5130
    large_sprites: bool,
    last_chr_write_was_b: bool,

    // Nametable mapping
    nt_mapping: u8, // $5105

    // Fill mode
    fill_tile: u8,  // $5106
    fill_color: u8, // $5107

    // ExRAM mode
    exram_mode: u8, // $5104

    // Scanline IRQ
    irq_scanline: u8, // $5203
    irq_enabled: bool,
    irq_pending: bool,
    irq_pending_since_dot: u64,
    in_frame: bool,
    scanline_counter: u8,
    last_ppu_read_addr: u16,
    nt_read_counter: u8,
    last_ppu_fetch_dot: u64,

    // Multiplier
    multiplicand: u8, // $5205
    multiplier: u8,   // $5206

    // Audio
    pulse1: Mmc5Pulse,
    pulse2: Mmc5Pulse,
    pcm_output: f32,
    pcm_read_mode: bool,
    audio_half_clock: bool,
    audio_frame_counter: u16,

    // Vertical split
    vsplit_mode: u8,   // $5200
    vsplit_scroll: u8, // $5201
    vsplit_bank: u8,   // $5202

    // ExRAM mode 1 extended attribute state
    exattr_fetch_counter: u8,
    exattr_nt_offset: u16,
    exattr_chr_bank: u16,

    // Controllers
    controllers: [Controller; 2],

    // Has battery
    has_battery: bool,

    // Tile counter for CHR A/B switching (counts NT byte fetches, not attribute fetches)
    split_tile_number: u8,
    need_in_frame: bool,

    // True during PPU rendering fetches (ppu_fetch), false during CPU PPUDATA access
    rendering: bool,
}

impl MMC5Mapper {
    pub fn new(
        flags: u8,
        prg_banks_16k: Vec<[u8; 16384]>,
        chr_banks_8k: Vec<[u8; 8192]>,
        has_battery: bool,
        sram_data: Option<Vec<u8>>,
    ) -> Self {
        // Split 16KB PRG banks into 8KB banks
        let mut prg_rom: Vec<[u8; 8192]> = Vec::new();
        for bank in &prg_banks_16k {
            let mut lo = [0u8; 8192];
            let mut hi = [0u8; 8192];
            lo.copy_from_slice(&bank[0..8192]);
            hi.copy_from_slice(&bank[8192..16384]);
            prg_rom.push(lo);
            prg_rom.push(hi);
        }

        // Split 8KB CHR banks into 1KB banks
        let mut chr_rom: Vec<[u8; 1024]> = Vec::new();
        for bank in &chr_banks_8k {
            for i in 0..8 {
                let mut kb = [0u8; 1024];
                kb.copy_from_slice(&bank[i * 1024..(i + 1) * 1024]);
                chr_rom.push(kb);
            }
        }

        let chr_ram = if chr_rom.is_empty() {
            Some(vec![0u8; 8192])
        } else {
            None
        };

        let mut prg_ram = Box::new([0u8; PRG_RAM_SIZE]);
        if let Some(data) = sram_data {
            let len = data.len().min(PRG_RAM_SIZE);
            prg_ram[..len].copy_from_slice(&data[..len]);
        }

        let _ = flags; // mirroring is handled by $5105

        let last_prg_bank = if prg_rom.is_empty() {
            0
        } else {
            (prg_rom.len() - 1) as u8
        };

        let mut mapper = Self {
            internal_ram: Box::new([0u8; 2048]),
            prg_rom,
            prg_ram,
            chr_rom,
            chr_ram,
            vram: Box::new([0u8; VRAM_SIZE as usize]),
            exram: Box::new([0u8; EXRAM_SIZE]),
            palette_ram: [0x0F; PALETTE_SIZE],
            prg_mode: 3,
            prg_bank_regs: [0, 0, 0, 0, last_prg_bank | 0x80],
            ram_protect_1: 0,
            ram_protect_2: 0,
            chr_mode: 0,
            chr_bank_a: [0; 8],
            chr_bank_b: [0; 4],
            chr_upper_bits: 0,
            large_sprites: false,
            last_chr_write_was_b: false,
            nt_mapping: 0,
            fill_tile: 0,
            fill_color: 0,
            exram_mode: 0,
            irq_scanline: 0,
            irq_enabled: false,
            irq_pending: false,
            irq_pending_since_dot: 0,
            in_frame: false,
            scanline_counter: 0,
            last_ppu_read_addr: 0,
            nt_read_counter: 0,
            last_ppu_fetch_dot: 0,
            multiplicand: 0,
            multiplier: 0,
            pulse1: Mmc5Pulse::new(),
            pulse2: Mmc5Pulse::new(),
            pcm_output: 0.0,
            pcm_read_mode: false,
            audio_half_clock: false,
            audio_frame_counter: 0,
            vsplit_mode: 0,
            vsplit_scroll: 0,
            vsplit_bank: 0,
            exattr_fetch_counter: 0,
            exattr_nt_offset: 0,
            exattr_chr_bank: 0,
            controllers: [Controller::new(), Controller::new()],
            has_battery,
            split_tile_number: 0,
            need_in_frame: false,
            rendering: false,
        };
        // Default: last bank mapped to $E000
        mapper.prg_bank_regs[4] = last_prg_bank | 0x80;
        mapper
    }

    fn ram_writes_enabled(&self) -> bool {
        self.ram_protect_1 == 0x02 && self.ram_protect_2 == 0x01
    }

    fn resolve_prg_addr(&self, addr: u16) -> PrgSource {
        match self.prg_mode {
            0 => {
                // One 32KB bank at $8000-$FFFF
                let bank = (self.prg_bank_regs[4] & 0x7C) as usize; // bits 6:2, 32KB aligned
                let bank = bank % self.prg_rom.len().max(1);
                let offset = (addr - 0x8000) as usize;
                let sub_bank = bank + (offset / 8192);
                PrgSource::Rom(sub_bank % self.prg_rom.len().max(1), offset % 8192)
            }
            1 => {
                // Two 16KB banks
                if addr < 0xC000 {
                    let reg = self.prg_bank_regs[2];
                    let offset = (addr - 0x8000) as usize;
                    if reg & 0x80 != 0 {
                        let bank = ((reg & 0x7E) as usize) % self.prg_rom.len().max(1);
                        let sub = bank + (offset / 8192);
                        PrgSource::Rom(sub % self.prg_rom.len().max(1), offset % 8192)
                    } else {
                        let ram_bank = ((reg & 0x0E) as usize) * 8192;
                        PrgSource::Ram(ram_bank + offset)
                    }
                } else {
                    let bank = (self.prg_bank_regs[4] & 0x7E) as usize;
                    let bank = bank % self.prg_rom.len().max(1);
                    let offset = (addr - 0xC000) as usize;
                    let sub = bank + (offset / 8192);
                    PrgSource::Rom(sub % self.prg_rom.len().max(1), offset % 8192)
                }
            }
            2 => {
                // One 16KB + two 8KB
                if addr < 0xC000 {
                    let reg = self.prg_bank_regs[2];
                    let offset = (addr - 0x8000) as usize;
                    if reg & 0x80 != 0 {
                        let bank = ((reg & 0x7E) as usize) % self.prg_rom.len().max(1);
                        let sub = bank + (offset / 8192);
                        PrgSource::Rom(sub % self.prg_rom.len().max(1), offset % 8192)
                    } else {
                        let ram_bank = ((reg & 0x0E) as usize) * 8192;
                        PrgSource::Ram(ram_bank + offset)
                    }
                } else if addr < 0xE000 {
                    let reg = self.prg_bank_regs[3];
                    let offset = (addr - 0xC000) as usize;
                    if reg & 0x80 != 0 {
                        let bank = (reg & 0x7F) as usize % self.prg_rom.len().max(1);
                        PrgSource::Rom(bank, offset)
                    } else {
                        let ram_bank = ((reg & 0x07) as usize) * 8192;
                        PrgSource::Ram(ram_bank + offset)
                    }
                } else {
                    let bank = (self.prg_bank_regs[4] & 0x7F) as usize % self.prg_rom.len().max(1);
                    let offset = (addr - 0xE000) as usize;
                    PrgSource::Rom(bank, offset)
                }
            }
            3 => {
                // Four 8KB banks
                let (reg_idx, base) = if addr < 0xA000 {
                    (1, 0x8000u16)
                } else if addr < 0xC000 {
                    (2, 0xA000)
                } else if addr < 0xE000 {
                    (3, 0xC000)
                } else {
                    (4, 0xE000)
                };
                let reg = self.prg_bank_regs[reg_idx];
                let offset = (addr - base) as usize;
                if reg & 0x80 != 0 {
                    let bank = (reg & 0x7F) as usize % self.prg_rom.len().max(1);
                    PrgSource::Rom(bank, offset)
                } else {
                    let ram_bank = ((reg & 0x07) as usize) * 8192;
                    PrgSource::Ram(ram_bank + offset)
                }
            }
            _ => PrgSource::Rom(0, 0),
        }
    }

    fn resolve_chr_addr(&self, addr: u16) -> usize {
        let addr = addr as usize & 0x1FFF;
        let use_b = if self.rendering {
            if self.large_sprites {
                // 8x16: A banks for sprite fetches (tiles 32-39), B for BG
                !(self.split_tile_number >= 32 && self.split_tile_number < 40) && self.in_frame
            } else {
                // 8x8: always use A banks for rendering
                false
            }
        } else {
            // PPUDATA access: use whichever bank set was last written
            self.last_chr_write_was_b
        };

        let bank_index = match self.chr_mode {
            0 => {
                // 8KB mode
                let reg = if use_b {
                    self.chr_bank_b[3]
                } else {
                    self.chr_bank_a[7]
                };
                let base = (reg as usize) * 8;
                let sub = addr / 1024;
                base + sub
            }
            1 => {
                // 4KB mode
                let slot = addr / 4096;
                let reg = if use_b {
                    self.chr_bank_b[3]
                } else if slot == 0 {
                    self.chr_bank_a[3]
                } else {
                    self.chr_bank_a[7]
                };
                let base = (reg as usize) * 4;
                let sub = (addr % 4096) / 1024;
                base + sub
            }
            2 => {
                // 2KB mode: B uses registers 1,3 (mirrored across both halves)
                let slot = addr / 2048;
                let reg = if use_b {
                    if slot & 1 == 0 {
                        self.chr_bank_b[1]
                    } else {
                        self.chr_bank_b[3]
                    }
                } else {
                    match slot {
                        0 => self.chr_bank_a[1],
                        1 => self.chr_bank_a[3],
                        2 => self.chr_bank_a[5],
                        _ => self.chr_bank_a[7],
                    }
                };
                let base = (reg as usize) * 2;
                let sub = (addr % 2048) / 1024;
                base + sub
            }
            3 => {
                // 1KB mode
                let slot = addr / 1024;
                let reg = if use_b {
                    self.chr_bank_b[slot & 3]
                } else {
                    self.chr_bank_a[slot]
                };
                reg as usize
            }
            _ => 0,
        };

        if !self.chr_rom.is_empty() {
            let bank = bank_index % self.chr_rom.len();
            let offset = addr % 1024;
            bank * 1024 + offset
        } else {
            addr % 8192
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        if !self.chr_rom.is_empty() {
            let resolved = self.resolve_chr_addr(addr);
            let bank = resolved / 1024;
            let offset = resolved % 1024;
            if bank < self.chr_rom.len() {
                self.chr_rom[bank][offset]
            } else {
                0
            }
        } else if let Some(ref ram) = self.chr_ram {
            let a = (addr as usize) & 0x1FFF;
            if a < ram.len() {
                ram[a]
            } else {
                0
            }
        } else {
            0
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if let Some(ref mut ram) = self.chr_ram {
            let a = (addr as usize) & 0x1FFF;
            if a < ram.len() {
                ram[a] = value;
            }
        }
    }

    fn read_nametable(&self, addr: u16) -> u8 {
        let nt_addr = addr & 0x0FFF;
        let slot = (nt_addr / 0x400) as u8;
        let source = (self.nt_mapping >> (slot * 2)) & 3;
        let offset = (nt_addr & 0x3FF) as usize;

        match source {
            0 => self.vram[offset],
            1 => self.vram[0x400 + offset],
            2 => {
                if self.exram_mode <= 1 {
                    self.exram[offset]
                } else {
                    0
                }
            }
            3 => {
                // Fill mode
                if offset < 960 {
                    self.fill_tile
                } else {
                    // Attribute byte: replicate fill_color across all quadrants
                    let c = self.fill_color & 3;
                    c | (c << 2) | (c << 4) | (c << 6)
                }
            }
            _ => 0,
        }
    }

    fn write_nametable(&mut self, addr: u16, value: u8) {
        let nt_addr = addr & 0x0FFF;
        let slot = (nt_addr / 0x400) as u8;
        let source = (self.nt_mapping >> (slot * 2)) & 3;
        let offset = (nt_addr & 0x3FF) as usize;

        match source {
            0 => self.vram[offset] = value,
            1 => self.vram[0x400 + offset] = value,
            2 => {
                if self.exram_mode <= 1 {
                    self.exram[offset] = value;
                }
            }
            3 => {} // fill mode is read-only
            _ => {}
        }
    }

    fn detect_scanline(&mut self, addr: u16, dot: u64) {
        if (0x2000..=0x2FFF).contains(&addr) {
            if self.last_ppu_read_addr == addr {
                self.nt_read_counter += 1;
            } else {
                self.nt_read_counter = 0;
            }
            if self.nt_read_counter == 2 {
                self.split_tile_number = 0;
                if !self.in_frame && !self.need_in_frame {
                    self.need_in_frame = true;
                    self.scanline_counter = 0;
                } else {
                    self.scanline_counter = self.scanline_counter.wrapping_add(1);
                    if self.irq_scanline != 0 && self.irq_scanline == self.scanline_counter {
                        self.irq_pending = true;
                        self.irq_pending_since_dot = dot;
                    }
                }
            }
        } else {
            self.nt_read_counter = 0;
        }
        self.last_ppu_read_addr = addr;
    }
}

enum PrgSource {
    Rom(usize, usize),
    Ram(usize),
}

impl MemoryMapper for MMC5Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.internal_ram[(addr & 0x07FF) as usize],
            0x5010 => {
                // PCM status: bit 0 = IRQ pending (not implemented)
                0
            }
            0x5015 => {
                let mut val = 0u8;
                if self.pulse1.length_counter > 0 {
                    val |= 0x01;
                }
                if self.pulse2.length_counter > 0 {
                    val |= 0x02;
                }
                val
            }
            0x5100..=0x5104 => 0, // Write-only
            0x5105 => self.nt_mapping,
            0x5204 => {
                let mut val = 0u8;
                if self.irq_pending {
                    val |= 0x80;
                }
                if self.in_frame {
                    val |= 0x40;
                }
                self.irq_pending = false;
                val
            }
            0x5205 => {
                let product = (self.multiplicand as u16) * (self.multiplier as u16);
                product as u8
            }
            0x5206 => {
                let product = (self.multiplicand as u16) * (self.multiplier as u16);
                (product >> 8) as u8
            }
            0x5C00..=0x5FFF => {
                let offset = (addr - 0x5C00) as usize;
                if self.exram_mode >= 2 {
                    self.exram[offset]
                } else {
                    0
                }
            }
            0x6000..=0x7FFF => {
                let ram_bank = (self.prg_bank_regs[0] & 0x07) as usize;
                let offset = (addr - 0x6000) as usize;
                let ram_addr = (ram_bank * 8192 + offset) % PRG_RAM_SIZE;
                self.prg_ram[ram_addr]
            }
            0x8000..=0xFFFF => {
                if addr == 0xFFFA || addr == 0xFFFB {
                    self.in_frame = false;
                    self.need_in_frame = false;
                    self.last_ppu_read_addr = 0;
                    self.scanline_counter = 0;
                    self.irq_pending = false;
                }
                match self.resolve_prg_addr(addr) {
                    PrgSource::Rom(bank, offset) => {
                        if bank < self.prg_rom.len() {
                            self.prg_rom[bank][offset]
                        } else {
                            0
                        }
                    }
                    PrgSource::Ram(offset) => {
                        let ram_addr = offset % PRG_RAM_SIZE;
                        self.prg_ram[ram_addr]
                    }
                }
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                self.internal_ram[(addr & 0x07FF) as usize] = value;
            }
            // MMC5 pulse 1
            0x5000 => self.pulse1.set_control(value),
            0x5001 => {} // No sweep on MMC5
            0x5002 => self.pulse1.set_timer_low(value),
            0x5003 => self.pulse1.set_timer_high(value),
            // MMC5 pulse 2
            0x5004 => self.pulse2.set_control(value),
            0x5005 => {} // No sweep on MMC5
            0x5006 => self.pulse2.set_timer_low(value),
            0x5007 => self.pulse2.set_timer_high(value),
            0x5010 => {
                self.pcm_read_mode = value & 0x01 != 0;
            }
            0x5011 => {
                if !self.pcm_read_mode && value != 0 {
                    self.pcm_output = value as f32;
                }
            }
            0x5015 => {
                self.pulse1.set_enabled(value & 0x01 != 0);
                self.pulse2.set_enabled(value & 0x02 != 0);
            }
            0x5100 => self.prg_mode = value & 3,
            0x5101 => self.chr_mode = value & 3,
            0x5102 => self.ram_protect_1 = value & 3,
            0x5103 => self.ram_protect_2 = value & 3,
            0x5104 => self.exram_mode = value & 3,
            0x5105 => {
                self.nt_mapping = value;
            }
            0x5106 => self.fill_tile = value,
            0x5107 => self.fill_color = value & 3,
            0x5113 => self.prg_bank_regs[0] = value,
            0x5114 => self.prg_bank_regs[1] = value,
            0x5115 => self.prg_bank_regs[2] = value,
            0x5116 => self.prg_bank_regs[3] = value,
            0x5117 => self.prg_bank_regs[4] = value | 0x80, // Always ROM
            0x5120 => {
                self.chr_bank_a[0] = value as u16 | ((self.chr_upper_bits as u16) << 8);
                self.last_chr_write_was_b = false;
            }
            0x5121 => {
                self.chr_bank_a[1] = value as u16 | ((self.chr_upper_bits as u16) << 8);
                self.last_chr_write_was_b = false;
            }
            0x5122 => {
                self.chr_bank_a[2] = value as u16 | ((self.chr_upper_bits as u16) << 8);
                self.last_chr_write_was_b = false;
            }
            0x5123 => {
                self.chr_bank_a[3] = value as u16 | ((self.chr_upper_bits as u16) << 8);
                self.last_chr_write_was_b = false;
            }
            0x5124 => {
                self.chr_bank_a[4] = value as u16 | ((self.chr_upper_bits as u16) << 8);
                self.last_chr_write_was_b = false;
            }
            0x5125 => {
                self.chr_bank_a[5] = value as u16 | ((self.chr_upper_bits as u16) << 8);
                self.last_chr_write_was_b = false;
            }
            0x5126 => {
                self.chr_bank_a[6] = value as u16 | ((self.chr_upper_bits as u16) << 8);
                self.last_chr_write_was_b = false;
            }
            0x5127 => {
                self.chr_bank_a[7] = value as u16 | ((self.chr_upper_bits as u16) << 8);
                self.last_chr_write_was_b = false;
            }
            0x5128 => {
                self.chr_bank_b[0] = value as u16 | ((self.chr_upper_bits as u16) << 8);
                self.last_chr_write_was_b = true;
            }
            0x5129 => {
                self.chr_bank_b[1] = value as u16 | ((self.chr_upper_bits as u16) << 8);
                self.last_chr_write_was_b = true;
            }
            0x512A => {
                self.chr_bank_b[2] = value as u16 | ((self.chr_upper_bits as u16) << 8);
                self.last_chr_write_was_b = true;
            }
            0x512B => {
                self.chr_bank_b[3] = value as u16 | ((self.chr_upper_bits as u16) << 8);
                self.last_chr_write_was_b = true;
            }
            0x5130 => self.chr_upper_bits = value & 3,
            0x5200 => self.vsplit_mode = value,
            0x5201 => self.vsplit_scroll = value,
            0x5202 => self.vsplit_bank = value,
            0x5203 => {
                self.irq_scanline = value;
            }
            0x5204 => {
                self.irq_enabled = value & 0x80 != 0;
            }
            0x5205 => self.multiplicand = value,
            0x5206 => self.multiplier = value,
            0x5C00..=0x5FFF => {
                let offset = (addr - 0x5C00) as usize;
                match self.exram_mode {
                    0 | 1 => {
                        if self.in_frame {
                            self.exram[offset] = value;
                        } else {
                            self.exram[offset] = 0;
                        }
                    }
                    2 => self.exram[offset] = value,
                    _ => {} // Mode 3: read-only
                }
            }
            0x6000..=0x7FFF => {
                if self.ram_writes_enabled() {
                    let ram_bank = (self.prg_bank_regs[0] & 0x07) as usize;
                    let offset = (addr - 0x6000) as usize;
                    let ram_addr = (ram_bank * 8192 + offset) % PRG_RAM_SIZE;
                    self.prg_ram[ram_addr] = value;
                }
            }
            0x8000..=0xFFFF => {
                match self.resolve_prg_addr(addr) {
                    PrgSource::Ram(offset) => {
                        if self.ram_writes_enabled() {
                            let ram_addr = offset % PRG_RAM_SIZE;
                            self.prg_ram[ram_addr] = value;
                        }
                    }
                    PrgSource::Rom(_, _) => {} // Can't write to ROM
                }
            }
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        let addr = addr % 0x4000;
        if addr >= PALETTE_START {
            let mut idx = (addr as usize - PALETTE_START as usize) % PALETTE_SIZE;
            if idx & PALETTE_MIRROR_MASK == PALETTE_MIRROR_CLEAR {
                idx &= !PALETTE_MIRROR_CLEAR;
            }
            return self.palette_ram[idx];
        }
        if addr < 0x2000 {
            return self.read_chr(addr);
        }
        self.read_nametable(addr)
    }

    fn ppu_fetch(&mut self, addr: u16, dot: u64) -> u8 {
        self.rendering = true;
        self.last_ppu_fetch_dot = dot;
        let addr = addr % 0x4000;

        let is_nt_range = (0x2000..=0x2FFF).contains(&addr);
        let is_tile_fetch = is_nt_range && (addr & 0x03FF) < 0x03C0;

        // Step 1: tile counter + frame entry (before scanline detection)
        if is_tile_fetch {
            self.split_tile_number = self.split_tile_number.wrapping_add(1);
            if self.in_frame {
                // CHR banks may need updating as split_tile_number crosses 32/40
            } else if self.need_in_frame {
                self.need_in_frame = false;
                self.in_frame = true;
            }
        }

        // Step 2: scanline detection (called for ALL PPU reads)
        self.detect_scanline(addr, dot);

        // Nametable/attribute range
        if is_nt_range {
            let is_sprite_fetch = self.split_tile_number >= 32 && self.split_tile_number < 40;

            // Vertical split mode
            if self.vsplit_mode & 0x80 != 0 && !is_sprite_fetch {
                let col = (self.split_tile_number.wrapping_sub(1)) % 42;
                let delimiter = self.vsplit_mode & 0x1F;
                let right_side = self.vsplit_mode & 0x40 != 0;
                let in_split = if right_side {
                    col >= delimiter
                } else {
                    col < delimiter
                };

                if in_split && is_tile_fetch {
                    let scroll_y = self.vsplit_scroll as u16;
                    let tile_y = (self.scanline_counter as u16 + scroll_y) / 8;
                    let tile_addr = (tile_y % 30) * 32 + col as u16;
                    let tile_id = self.exram[tile_addr as usize % EXRAM_SIZE];
                    self.exattr_fetch_counter = 3;
                    self.exattr_nt_offset = tile_addr;
                    return tile_id;
                }

                if in_split && !is_tile_fetch && self.exattr_fetch_counter == 3 {
                    self.exattr_fetch_counter = 2;
                    let tile_addr = self.exattr_nt_offset;
                    let attr_x = (tile_addr % 32) / 4;
                    let attr_y = (tile_addr / 32) / 4;
                    let attr_addr = 0x3C0 + attr_y * 8 + attr_x;
                    return self.exram[attr_addr as usize % EXRAM_SIZE];
                }
            }

            // ExRAM mode 1: extended attributes for BG tiles
            if self.exram_mode == 1 && !is_sprite_fetch {
                if is_tile_fetch {
                    self.exattr_nt_offset = addr & 0x03FF;
                    self.exattr_fetch_counter = 3;
                } else if self.exattr_fetch_counter == 3 {
                    let exram_byte = self.exram[self.exattr_nt_offset as usize % EXRAM_SIZE];
                    let palette = (exram_byte >> 6) & 3;
                    self.exattr_chr_bank =
                        ((exram_byte & 0x3F) as u16) | ((self.chr_upper_bits as u16) << 6);
                    self.exattr_fetch_counter = 2;
                    return palette | (palette << 2) | (palette << 4) | (palette << 6);
                }
            }

            return self.ppu_read(addr);
        }

        // Pattern table range ($0000-$1FFF)
        if self.exattr_fetch_counter > 0 && self.exattr_fetch_counter <= 2 {
            self.exattr_fetch_counter -= 1;

            if self.exram_mode == 1 || self.vsplit_mode & 0x80 != 0 {
                let chr_bank = if self.vsplit_mode & 0x80 != 0 && self.exattr_fetch_counter <= 1 {
                    (self.vsplit_bank as u16) | ((self.chr_upper_bits as u16) << 8)
                } else {
                    self.exattr_chr_bank
                };
                let bank_base = (chr_bank as usize) * 4096;
                let chr_offset = addr as usize & 0x0FFF;
                let abs_addr = bank_base + chr_offset;
                let bank_1k = abs_addr / 1024;
                let offset_1k = abs_addr % 1024;
                if !self.chr_rom.is_empty() && bank_1k < self.chr_rom.len() {
                    return self.chr_rom[bank_1k][offset_1k];
                } else if !self.chr_rom.is_empty() {
                    return self.chr_rom[bank_1k % self.chr_rom.len()][offset_1k];
                }
                return 0;
            }
        }

        self.ppu_read(addr)
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    unsafe fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        for i in 0..size {
            let byte = self.ppu_read(addr + i as u16);
            unsafe {
                *dest.add(i) = byte;
            }
        }
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        let addr = addr % 0x4000;
        if addr >= PALETTE_START {
            let mut idx = (addr as usize - PALETTE_START as usize) % PALETTE_SIZE;
            if idx & PALETTE_MIRROR_MASK == PALETTE_MIRROR_CLEAR {
                idx &= !PALETTE_MIRROR_CLEAR;
            }
            self.palette_ram[idx] = value;
            return;
        }
        if addr < 0x2000 {
            self.write_chr(addr, value);
            return;
        }
        self.write_nametable(addr, value);
    }

    fn code_start(&mut self) -> u16 {
        let lo = self.cpu_read(RESET_TARGET_ADDR);
        let hi = self.cpu_read(RESET_TARGET_ADDR + 1);
        ((hi as u16) << 8) | lo as u16
    }

    fn controllers(&mut self) -> &mut [Controller; 2] {
        &mut self.controllers
    }

    fn poll_irq(&mut self) -> bool {
        self.irq_pending && self.irq_enabled
    }

    fn poll_irq_at_dot(&self, deadline_dot: u64) -> bool {
        self.irq_pending && self.irq_pending_since_dot <= deadline_dot
    }

    fn sram_data(&self) -> Option<&[u8]> {
        if self.has_battery {
            Some(&self.prg_ram[..8192])
        } else {
            None
        }
    }

    fn mapper_id(&self) -> u8 {
        5
    }

    fn cpu_maps_ppu_registers(&self) -> bool {
        true
    }

    fn cpu_maps_ppu_register_mirrors(&self) -> bool {
        true
    }

    fn cpu_cycle(&mut self, ppu_dot: u64) {
        // PPU idle detection: if 9+ PPU dots have passed since the last
        // ppu_fetch, the PPU is idle (VBlank). We use the actual dot gap
        // rather than a CPU-cycle counter to avoid false expirations when
        // mid-instruction PPU syncs push the PPU ahead of the mapper's
        // cpu_cycle cadence.
        if self.last_ppu_fetch_dot > 0 && ppu_dot >= self.last_ppu_fetch_dot + 9 && self.in_frame {
            self.in_frame = false;
            self.rendering = false;
            self.nt_read_counter = 0;
            self.last_ppu_read_addr = 0;
        }

        // MMC5 pulses tick every CPU cycle (no half-rate divider unlike APU)
        self.pulse1.cycle();
        self.pulse2.cycle();

        // MMC5 internal 240Hz frame counter (independent of APU $4017).
        // Envelope clocked at 240Hz (every quarter-frame tick).
        // Length counter clocked at 240Hz too (2x the APU's 120Hz rate).
        self.audio_frame_counter += 1;
        if self.audio_frame_counter == 3729
            || self.audio_frame_counter == 7457
            || self.audio_frame_counter == 11186
            || self.audio_frame_counter >= 14915
        {
            self.pulse1.clock_envelope();
            self.pulse2.clock_envelope();
            self.pulse1.clock_length_counter();
            self.pulse2.clock_length_counter();
            if self.audio_frame_counter >= 14915 {
                self.audio_frame_counter = 0;
            }
        }
    }

    fn notify_ppu_ctrl(&mut self, value: u8) {
        self.large_sprites = value & 0x20 != 0;
    }

    fn notify_ppu_mask(&mut self, value: u8) {
        let rendering_enabled = value & 0x18 != 0;
        if !rendering_enabled {
            self.in_frame = false;
            self.irq_pending = false;
            self.scanline_counter = 0;
            self.last_ppu_read_addr = 0;
            self.nt_read_counter = 0;
        }
    }

    fn audio_expansion_output(&self) -> f32 {
        let pulse_sum = self.pulse1.output + self.pulse2.output;
        let pulse_out = if pulse_sum > 0.0 {
            95.88 / (8128.0 / pulse_sum + 100.0)
        } else {
            0.0
        };
        let pcm_out = if self.pcm_output > 0.0 {
            159.79 / (1.0 / (self.pcm_output / 45276.0) + 100.0)
        } else {
            0.0
        };
        -(pulse_out + pcm_out)
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        save_controllers(w, &self.controllers);
        w.write_bytes(&*self.internal_ram);

        // PRG state
        w.write_u8(self.prg_mode);
        for &r in &self.prg_bank_regs {
            w.write_u8(r);
        }
        w.write_u8(self.ram_protect_1);
        w.write_u8(self.ram_protect_2);
        w.write_bytes(&*self.prg_ram);

        // CHR state
        w.write_u8(self.chr_mode);
        for &r in &self.chr_bank_a {
            w.write_u16(r);
        }
        for &r in &self.chr_bank_b {
            w.write_u16(r);
        }
        w.write_u8(self.chr_upper_bits);
        w.write_bool(self.large_sprites);
        w.write_bool(self.last_chr_write_was_b);
        if let Some(ref ram) = self.chr_ram {
            w.write_bool(true);
            w.write_bytes(ram);
        } else {
            w.write_bool(false);
        }

        // Nametable / ExRAM
        w.write_u8(self.nt_mapping);
        w.write_u8(self.fill_tile);
        w.write_u8(self.fill_color);
        w.write_u8(self.exram_mode);
        w.write_bytes(&*self.vram);
        w.write_bytes(&*self.exram);
        w.write_bytes(&self.palette_ram);

        // IRQ
        w.write_u8(self.irq_scanline);
        w.write_bool(self.irq_enabled);
        w.write_bool(self.irq_pending);
        w.write_u64(self.irq_pending_since_dot);
        w.write_bool(self.in_frame);
        w.write_u8(self.scanline_counter);
        w.write_u16(self.last_ppu_read_addr);
        w.write_u8(self.nt_read_counter);
        w.write_u64(self.last_ppu_fetch_dot);

        // Multiplier
        w.write_u8(self.multiplicand);
        w.write_u8(self.multiplier);

        // Audio
        self.pulse1.save_state(w);
        self.pulse2.save_state(w);
        w.write_f32(self.pcm_output);
        w.write_bool(self.pcm_read_mode);
        w.write_bool(self.audio_half_clock);
        w.write_u16(self.audio_frame_counter);

        // Vertical split
        w.write_u8(self.vsplit_mode);
        w.write_u8(self.vsplit_scroll);
        w.write_u8(self.vsplit_bank);

        // ExRAM mode 1 state
        w.write_u8(self.exattr_fetch_counter);
        w.write_u16(self.exattr_nt_offset);
        w.write_u16(self.exattr_chr_bank);

        // Fetch tracking
        w.write_u8(self.split_tile_number);
        w.write_bool(self.need_in_frame);
        w.write_bool(self.rendering);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        load_controllers(r, &mut self.controllers)?;
        r.read_bytes_into(&mut *self.internal_ram)?;

        self.prg_mode = r.read_u8()?;
        for i in 0..5 {
            self.prg_bank_regs[i] = r.read_u8()?;
        }
        self.ram_protect_1 = r.read_u8()?;
        self.ram_protect_2 = r.read_u8()?;
        r.read_bytes_into(&mut *self.prg_ram)?;

        self.chr_mode = r.read_u8()?;
        for i in 0..8 {
            self.chr_bank_a[i] = r.read_u16()?;
        }
        for i in 0..4 {
            self.chr_bank_b[i] = r.read_u16()?;
        }
        self.chr_upper_bits = r.read_u8()?;
        self.large_sprites = r.read_bool()?;
        self.last_chr_write_was_b = r.read_bool()?;
        let has_chr_ram = r.read_bool()?;
        if has_chr_ram {
            if let Some(ref mut ram) = self.chr_ram {
                r.read_bytes_into(ram)?;
            } else {
                let _ = r.read_bytes()?;
            }
        }

        self.nt_mapping = r.read_u8()?;
        self.fill_tile = r.read_u8()?;
        self.fill_color = r.read_u8()?;
        self.exram_mode = r.read_u8()?;
        r.read_bytes_into(&mut *self.vram)?;
        r.read_bytes_into(&mut *self.exram)?;
        r.read_bytes_into(&mut self.palette_ram)?;

        self.irq_scanline = r.read_u8()?;
        self.irq_enabled = r.read_bool()?;
        self.irq_pending = r.read_bool()?;
        if r.version() >= 7 {
            self.irq_pending_since_dot = r.read_u64()?;
        } else {
            self.irq_pending_since_dot = 0;
        }
        self.in_frame = r.read_bool()?;
        self.scanline_counter = r.read_u8()?;
        self.last_ppu_read_addr = r.read_u16()?;
        self.nt_read_counter = r.read_u8()?;
        if r.version() >= 7 {
            self.last_ppu_fetch_dot = r.read_u64()?;
        } else {
            let _old_idle_counter = r.read_u8()?;
            self.last_ppu_fetch_dot = 0;
        }

        self.multiplicand = r.read_u8()?;
        self.multiplier = r.read_u8()?;

        self.pulse1.load_state(r)?;
        self.pulse2.load_state(r)?;
        self.pcm_output = r.read_f32()?;
        self.pcm_read_mode = r.read_bool()?;
        self.audio_half_clock = r.read_bool()?;
        self.audio_frame_counter = r.read_u16()?;

        self.vsplit_mode = r.read_u8()?;
        self.vsplit_scroll = r.read_u8()?;
        self.vsplit_bank = r.read_u8()?;

        self.exattr_fetch_counter = r.read_u8()?;
        self.exattr_nt_offset = r.read_u16()?;
        self.exattr_chr_bank = r.read_u16()?;

        self.split_tile_number = r.read_u8()?;
        self.need_in_frame = r.read_bool()?;
        self.rendering = r.read_bool()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emu::savestate::{SavestateReader, SavestateWriter};

    fn make_mapper(prg_count: usize, chr_count: usize) -> MMC5Mapper {
        let mut prg_banks = Vec::new();
        for i in 0..prg_count {
            let mut bank = [0u8; 16384];
            bank[0] = i as u8;
            // Put a reset vector pointing to $8000
            bank[16384 - 4] = 0x00; // FFFC low
            bank[16384 - 3] = 0x80; // FFFC high
            prg_banks.push(bank);
        }
        let mut chr_banks = Vec::new();
        for i in 0..chr_count {
            let mut bank = [0u8; 8192];
            for j in 0..8 {
                bank[j * 1024] = (i * 8 + j) as u8;
            }
            chr_banks.push(bank);
        }
        MMC5Mapper::new(0, prg_banks, chr_banks, false, None)
    }

    #[test]
    fn test_prg_mode_3_last_bank_fixed() {
        let mut m = make_mapper(4, 1);
        // Mode 3: four 8KB banks. $5117 always ROM, defaults to last bank.
        assert_eq!(m.prg_bank_regs[4] & 0x80, 0x80);
        // Last 8KB bank of 4×16KB = 8 banks, so index 7
        let val = m.cpu_read(0xFFFC);
        // Should be reading from last PRG ROM bank
        assert_eq!(val, 0x00); // reset vector low byte
    }

    #[test]
    fn test_prg_mode_3_bank_switching() {
        let mut m = make_mapper(4, 1);
        // Write distinguishing bytes into each 8KB bank
        for i in 0..m.prg_rom.len() {
            m.prg_rom[i][0] = i as u8 + 0x10;
        }
        // Mode 3
        m.cpu_write(0x5100, 3);
        // Map bank 2 to $8000
        m.cpu_write(0x5114, 0x82);
        assert_eq!(m.cpu_read(0x8000), 0x12);
        // Map bank 5 to $A000
        m.cpu_write(0x5115, 0x85);
        assert_eq!(m.cpu_read(0xA000), 0x15);
    }

    #[test]
    fn test_prg_mode_0_32kb() {
        let mut m = make_mapper(4, 1);
        for i in 0..m.prg_rom.len() {
            m.prg_rom[i][0] = i as u8 + 0x20;
        }
        m.cpu_write(0x5100, 0); // 32KB mode
                                // $5117: select 32KB-aligned bank. bits 6:2. bank 0 means first 4 8KB banks.
        m.cpu_write(0x5117, 0x80); // bank 0 (ROM bit set)
        assert_eq!(m.cpu_read(0x8000), 0x20);
        assert_eq!(m.cpu_read(0xA000), 0x21);
    }

    #[test]
    fn test_prg_ram_protect() {
        let mut m = make_mapper(2, 1);
        // RAM writes should be blocked by default
        m.cpu_write(0x5100, 3);
        m.cpu_write(0x5114, 0x00); // RAM bank 0 at $8000
        m.cpu_write(0x8000, 0x42);
        assert_eq!(m.cpu_read(0x8000), 0); // Write should have been blocked

        // Enable RAM writes
        m.cpu_write(0x5102, 0x02);
        m.cpu_write(0x5103, 0x01);
        m.cpu_write(0x8000, 0x42);
        assert_eq!(m.cpu_read(0x8000), 0x42);
    }

    #[test]
    fn test_prg_6000_ram() {
        let mut m = make_mapper(2, 1);
        m.cpu_write(0x5102, 0x02);
        m.cpu_write(0x5103, 0x01);
        m.cpu_write(0x5113, 0x00);
        m.cpu_write(0x6000, 0xAB);
        assert_eq!(m.cpu_read(0x6000), 0xAB);
    }

    #[test]
    fn test_chr_mode_3_1kb() {
        let mut m = make_mapper(2, 4);
        m.cpu_write(0x5101, 3); // 1KB mode
        m.cpu_write(0x5120, 5); // Map CHR bank 5 to $0000-$03FF
        m.last_chr_write_was_b = false;
        let val = m.ppu_read(0x0000);
        assert_eq!(val, 5); // First byte of bank 5 should be 5 (set in make_mapper)
    }

    #[test]
    fn test_chr_bank_b_used_for_bg() {
        let mut m = make_mapper(2, 4);
        m.cpu_write(0x5101, 3); // 1KB mode
        m.large_sprites = true;
        // A banks for sprites
        m.cpu_write(0x5120, 0);
        // B banks for BG
        m.cpu_write(0x5128, 10);
        // Outside sprite tile range and in_frame: should use B banks
        m.in_frame = true;
        m.split_tile_number = 0; // BG range
        let _ = m.ppu_read(0x0000);
        // B bank 10 should be used
    }

    #[test]
    fn test_nametable_mapping() {
        let mut m = make_mapper(2, 1);
        // Map all 4 nametables to CIRAM page 0
        m.cpu_write(0x5105, 0x00);
        m.ppu_write(0x2042, 0xAB);
        assert_eq!(m.ppu_read(0x2042), 0xAB);
        // Mirror should also read same value
        assert_eq!(m.ppu_read(0x2442), 0xAB);
        assert_eq!(m.ppu_read(0x2842), 0xAB);
    }

    #[test]
    fn test_nametable_mapping_split() {
        let mut m = make_mapper(2, 1);
        // NT0=CIRAM0, NT1=CIRAM1, NT2=CIRAM0, NT3=CIRAM1 (vertical mirroring)
        m.cpu_write(0x5105, 0x44); // binary: 01 00 01 00
        m.ppu_write(0x2000, 0x11);
        m.ppu_write(0x2400, 0x22);
        assert_eq!(m.ppu_read(0x2000), 0x11);
        assert_eq!(m.ppu_read(0x2400), 0x22);
        // $2800 mirrors $2000 (CIRAM0)
        assert_eq!(m.ppu_read(0x2800), 0x11);
    }

    #[test]
    fn test_fill_mode_nametable() {
        let mut m = make_mapper(2, 1);
        m.cpu_write(0x5106, 0x42); // fill tile
        m.cpu_write(0x5107, 0x02); // fill color
                                   // Map NT0 to fill mode (source 3)
        m.cpu_write(0x5105, 0x03);
        assert_eq!(m.ppu_read(0x2000), 0x42); // tile
                                              // Attribute area: fill color replicated
        let attr = m.ppu_read(0x23C0);
        assert_eq!(attr, 0xAA); // color 2 in all quadrants = 10_10_10_10
    }

    #[test]
    fn test_exram_modes() {
        let mut m = make_mapper(2, 1);
        // Mode 2: read/write
        m.cpu_write(0x5104, 2);
        m.cpu_write(0x5C00, 0x42);
        assert_eq!(m.cpu_read(0x5C00), 0x42);

        // Mode 3: read-only (writes blocked)
        m.cpu_write(0x5104, 3);
        m.cpu_write(0x5C00, 0xFF);
        assert_eq!(m.cpu_read(0x5C00), 0x42); // Should still be old value
    }

    #[test]
    fn test_multiplier() {
        let mut m = make_mapper(2, 1);
        m.cpu_write(0x5205, 20);
        m.cpu_write(0x5206, 13);
        // 20 * 13 = 260 = 0x0104
        assert_eq!(m.cpu_read(0x5205), 0x04);
        assert_eq!(m.cpu_read(0x5206), 0x01);
    }

    #[test]
    fn test_multiplier_max() {
        let mut m = make_mapper(2, 1);
        m.cpu_write(0x5205, 0xFF);
        m.cpu_write(0x5206, 0xFF);
        // 255 * 255 = 65025 = 0xFE01
        assert_eq!(m.cpu_read(0x5205), 0x01);
        assert_eq!(m.cpu_read(0x5206), 0xFE);
    }

    #[test]
    fn test_irq_status_register() {
        let mut m = make_mapper(2, 1);
        // Initially: no IRQ pending, not in frame
        let status = m.cpu_read(0x5204);
        assert_eq!(status & 0x80, 0);
        assert_eq!(status & 0x40, 0);

        // Set pending and in_frame manually
        m.irq_pending = true;
        m.in_frame = true;
        let status = m.cpu_read(0x5204);
        assert_eq!(status & 0x80, 0x80);
        assert_eq!(status & 0x40, 0x40);
        // Reading should clear pending
        let status2 = m.cpu_read(0x5204);
        assert_eq!(status2 & 0x80, 0);
    }

    fn simulate_scanline_boundary(m: &mut MMC5Mapper) {
        // A real scanline has many varied fetches between boundaries.
        // Reset the consecutive-read counter with a pattern table read,
        // then 3 consecutive identical NT reads trigger the boundary.
        m.detect_scanline(0x0000, 0); // pattern fetch resets counter
        let addr = 0x2000;
        m.detect_scanline(addr, 0);
        m.detect_scanline(addr, 0);
        m.detect_scanline(addr, 0);
    }

    #[test]
    fn test_irq_scanline_counter() {
        let mut m = make_mapper(2, 1);
        m.cpu_write(0x5203, 5); // IRQ on scanline 5
        m.cpu_write(0x5204, 0x80); // Enable IRQ

        // Simulate scanline boundaries
        for _ in 0..7 {
            simulate_scanline_boundary(&mut m);
        }

        assert!(m.irq_pending);
        assert!(m.poll_irq());
    }

    #[test]
    fn test_irq_not_fired_when_disabled() {
        let mut m = make_mapper(2, 1);
        m.cpu_write(0x5203, 2);
        m.cpu_write(0x5204, 0x00); // Disabled

        for _ in 0..4 {
            simulate_scanline_boundary(&mut m);
        }

        assert!(m.irq_pending); // Pending is set regardless
        assert!(!m.poll_irq()); // But poll_irq checks enabled
    }

    #[test]
    fn test_irq_scanline_zero_never_fires() {
        let mut m = make_mapper(2, 1);
        m.cpu_write(0x5203, 0); // $5203=0 is special: comparison never matches
        m.cpu_write(0x5204, 0x80);

        for _ in 0..260 {
            simulate_scanline_boundary(&mut m);
        }

        assert!(!m.irq_pending);
    }

    #[test]
    fn test_irq_fires_on_exact_target_scanline() {
        let mut m = make_mapper(2, 1);
        m.cpu_write(0x5203, 3); // IRQ on scanline 3
        m.cpu_write(0x5204, 0x80);

        // Boundary 1: frame entry (need_in_frame, counter=0)
        simulate_scanline_boundary(&mut m);
        assert!(!m.irq_pending);
        assert_eq!(m.scanline_counter, 0);

        // Boundary 2: counter 0→1
        simulate_scanline_boundary(&mut m);
        assert!(!m.irq_pending);
        assert_eq!(m.scanline_counter, 1);

        // Boundary 3: counter 1→2
        simulate_scanline_boundary(&mut m);
        assert!(!m.irq_pending);
        assert_eq!(m.scanline_counter, 2);

        // Boundary 4: counter 2→3 = target → IRQ fires
        simulate_scanline_boundary(&mut m);
        assert!(m.irq_pending);
        assert_eq!(m.scanline_counter, 3);

        // Verify target 1 fires on the 2nd in-frame boundary
        let mut m2 = make_mapper(2, 1);
        m2.cpu_write(0x5203, 1);
        m2.cpu_write(0x5204, 0x80);
        simulate_scanline_boundary(&mut m2); // frame entry
        assert!(!m2.irq_pending);
        simulate_scanline_boundary(&mut m2); // counter 0→1 = target
        assert!(m2.irq_pending);
    }

    #[test]
    fn test_idle_counter_resets_rendering_and_nt_state() {
        let mut m = make_mapper(2, 1);
        m.in_frame = true;
        m.rendering = true;
        m.nt_read_counter = 2;
        m.last_ppu_read_addr = 0x2000;
        m.last_ppu_fetch_dot = 100;

        // 9 PPU dots after last fetch = idle expiry
        m.cpu_cycle(109);

        assert!(!m.in_frame);
        assert!(!m.rendering);
        assert_eq!(m.nt_read_counter, 0);
        assert_eq!(m.last_ppu_read_addr, 0);
    }

    #[test]
    fn test_tile_counter_resets_on_scanline_boundary() {
        let mut m = make_mapper(2, 1);
        m.in_frame = true;
        m.split_tile_number = 42; // End of previous scanline

        // Scanline boundary should reset tile counter
        simulate_scanline_boundary(&mut m);
        assert_eq!(m.split_tile_number, 0);
    }

    #[test]
    fn test_nmi_vector_clears_in_frame() {
        let mut m = make_mapper(2, 1);
        m.in_frame = true;
        m.scanline_counter = 10;
        m.irq_pending = true;
        let _ = m.cpu_read(0xFFFA);
        assert!(!m.in_frame);
        assert_eq!(m.scanline_counter, 0);
        assert!(!m.irq_pending);
    }

    #[test]
    fn test_ppu_mask_rendering_disabled_clears_frame() {
        let mut m = make_mapper(2, 1);
        m.in_frame = true;
        m.scanline_counter = 10;
        m.irq_pending = true;

        // Disable rendering (bits 3-4 both clear)
        m.notify_ppu_mask(0x00);
        assert!(!m.in_frame);
        assert_eq!(m.scanline_counter, 0);
        assert!(!m.irq_pending);
    }

    #[test]
    fn test_ppu_mask_rendering_enabled_preserves_frame() {
        let mut m = make_mapper(2, 1);
        m.in_frame = true;
        m.scanline_counter = 10;
        m.irq_pending = true;

        // BG enabled (bit 3) — rendering still on
        m.notify_ppu_mask(0x08);
        assert!(m.in_frame);
        assert_eq!(m.scanline_counter, 10);
        assert!(m.irq_pending);
    }

    #[test]
    fn test_dot_based_idle_detection() {
        let mut m = make_mapper(2, 1);
        m.in_frame = true;
        m.last_ppu_fetch_dot = 100;

        // 8 dots later: not yet idle
        m.cpu_cycle(108);
        assert!(m.in_frame);

        // 9 dots later: idle
        m.cpu_cycle(109);
        assert!(!m.in_frame);
    }

    #[test]
    fn test_ppu_fetch_resets_idle() {
        let mut m = make_mapper(2, 1);
        m.in_frame = true;
        m.last_ppu_fetch_dot = 100;

        // A PPU fetch at dot 108 resets the idle baseline
        m.ppu_fetch(0x2000, 108);
        assert_eq!(m.last_ppu_fetch_dot, 108);

        // 8 dots after new fetch: still in frame
        m.cpu_cycle(116);
        assert!(m.in_frame);

        // 9 dots after new fetch: idle
        m.cpu_cycle(117);
        assert!(!m.in_frame);
    }

    #[test]
    fn test_mmc5_pulse_basic() {
        let mut m = make_mapper(2, 1);
        m.cpu_write(0x5015, 0x01); // Enable pulse 1
        m.cpu_write(0x5000, 0x3F); // Duty 0, halt, constant vol 15
        m.cpu_write(0x5002, 0x10); // Timer low
        m.cpu_write(0x5003, 0x08); // Timer high + length

        // Cycle a few times
        for _ in 0..100 {
            m.cpu_cycle(0);
        }

        let output = m.audio_expansion_output();
        assert!(output <= 0.0); // Inverted polarity
    }

    #[test]
    fn test_mmc5_pulse_disabled() {
        let mut m = make_mapper(2, 1);
        // Don't enable pulse
        m.cpu_write(0x5000, 0x3F);
        m.cpu_write(0x5002, 0x10);
        m.cpu_write(0x5003, 0x08);

        for _ in 0..100 {
            m.cpu_cycle(0);
        }

        assert_eq!(m.audio_expansion_output(), 0.0);
    }

    #[test]
    fn test_mmc5_expansion_audio_levels() {
        let mut m = make_mapper(2, 1);

        // Single pulse at max volume: should match APU pulse formula
        // 95.88 / (8128/15 + 100) ≈ 0.148
        m.cpu_write(0x5015, 0x01);
        m.cpu_write(0x5000, 0x7F); // Duty 1 (50%), halt, constant vol 15
        m.cpu_write(0x5002, 0x10);
        m.cpu_write(0x5003, 0x08);
        let mut max_output = 0.0_f32;
        for _ in 0..200 {
            m.cpu_cycle(0);
            max_output = max_output.max(m.audio_expansion_output().abs());
        }
        assert!(
            max_output > 0.1 && max_output < 0.2,
            "single pulse max: {max_output}"
        );

        // Both pulses at max volume: ≈ 0.258
        let mut m = make_mapper(2, 1);
        m.cpu_write(0x5015, 0x03);
        m.cpu_write(0x5000, 0x7F); // Duty 1 (50%)
        m.cpu_write(0x5002, 0x10);
        m.cpu_write(0x5003, 0x08);
        m.cpu_write(0x5004, 0x7F);
        m.cpu_write(0x5006, 0x10);
        m.cpu_write(0x5007, 0x08);
        let mut max_output = 0.0_f32;
        for _ in 0..200 {
            m.cpu_cycle(0);
            max_output = max_output.max(m.audio_expansion_output().abs());
        }
        assert!(
            max_output > 0.2 && max_output < 0.3,
            "both pulses max: {max_output}"
        );

        // PCM at max (255): should stay well under 1.0
        let mut m = make_mapper(2, 1);
        m.cpu_write(0x5011, 0xFF);
        let output = m.audio_expansion_output().abs();
        assert!(output > 0.0 && output < 1.0, "pcm max: {output}");

        // PCM at mid (128): less than max
        let mut m2 = make_mapper(2, 1);
        m2.cpu_write(0x5011, 0x80);
        let mid = m2.audio_expansion_output().abs();
        assert!(
            mid < output,
            "pcm mid ({mid}) should be less than max ({output})"
        );

        // Combined max pulses + max PCM: still in sane range
        let mut m = make_mapper(2, 1);
        m.cpu_write(0x5015, 0x03);
        m.cpu_write(0x5000, 0x7F);
        m.cpu_write(0x5002, 0x10);
        m.cpu_write(0x5003, 0x08);
        m.cpu_write(0x5004, 0x7F);
        m.cpu_write(0x5006, 0x10);
        m.cpu_write(0x5007, 0x08);
        m.cpu_write(0x5011, 0xFF);
        let mut max_output = 0.0_f32;
        for _ in 0..200 {
            m.cpu_cycle(0);
            max_output = max_output.max(m.audio_expansion_output().abs());
        }
        assert!(max_output < 1.0, "combined max: {max_output}");
    }

    #[test]
    fn test_notify_ppu_ctrl() {
        let mut m = make_mapper(2, 1);
        assert!(!m.large_sprites);
        m.notify_ppu_ctrl(0x20); // bit 5 = 8x16 sprites
        assert!(m.large_sprites);
        m.notify_ppu_ctrl(0x00);
        assert!(!m.large_sprites);
    }

    #[test]
    fn test_palette_ram() {
        let mut m = make_mapper(2, 1);
        m.ppu_write(0x3F00, 0x30);
        assert_eq!(m.ppu_read(0x3F00), 0x30);
        // Mirror: $3F10 mirrors $3F00
        m.ppu_write(0x3F10, 0x15);
        assert_eq!(m.ppu_read(0x3F00), 0x15);
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut m = make_mapper(2, 2);
        m.cpu_write(0x5100, 2);
        m.cpu_write(0x5101, 3);
        m.cpu_write(0x5105, 0x44);
        m.cpu_write(0x5106, 0xAB);
        m.cpu_write(0x5203, 42);
        m.cpu_write(0x5204, 0x80);
        m.cpu_write(0x5205, 7);
        m.cpu_write(0x5206, 8);
        m.irq_pending = true;
        m.in_frame = true;
        m.scanline_counter = 10;

        // Enable RAM and write something
        m.cpu_write(0x5102, 0x02);
        m.cpu_write(0x5103, 0x01);
        m.cpu_write(0x6000, 0x77);

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);
        let data = w.finish();

        let mut m2 = make_mapper(2, 2);
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.prg_mode, 2);
        assert_eq!(m2.chr_mode, 3);
        assert_eq!(m2.nt_mapping, 0x44);
        assert_eq!(m2.fill_tile, 0xAB);
        assert_eq!(m2.irq_scanline, 42);
        assert!(m2.irq_enabled);
        assert!(m2.irq_pending);
        assert!(m2.in_frame);
        assert_eq!(m2.scanline_counter, 10);
        assert_eq!(m2.multiplicand, 7);
        assert_eq!(m2.multiplier, 8);
        assert_eq!(m2.cpu_read(0x6000), 0x77);
    }

    #[test]
    fn test_mapper_id() {
        let m = make_mapper(2, 1);
        assert_eq!(m.mapper_id(), 5);
    }

    #[test]
    fn test_prg_mode_2_layout() {
        let mut m = make_mapper(4, 1);
        for i in 0..m.prg_rom.len() {
            m.prg_rom[i][0] = i as u8 + 0x30;
        }
        m.cpu_write(0x5100, 2); // 16KB + 8KB + 8KB

        // $8000-$BFFF: 16KB from $5115
        m.cpu_write(0x5115, 0x82); // ROM bank 2 (16KB aligned, so banks 2-3)
        assert_eq!(m.cpu_read(0x8000), 0x32);

        // $C000-$DFFF: 8KB from $5116
        m.cpu_write(0x5116, 0x85);
        assert_eq!(m.cpu_read(0xC000), 0x35);

        // $E000-$FFFF: 8KB from $5117 (always ROM)
        m.cpu_write(0x5117, 0x86);
        assert_eq!(m.cpu_read(0xE000), 0x36);
    }

    #[test]
    fn test_prg_mode_1_layout() {
        let mut m = make_mapper(4, 1);
        for i in 0..m.prg_rom.len() {
            m.prg_rom[i][0] = i as u8 + 0x40;
        }
        m.cpu_write(0x5100, 1); // Two 16KB banks

        // $8000-$BFFF from $5115
        m.cpu_write(0x5115, 0x84); // ROM bank 4 (16KB aligned)
        assert_eq!(m.cpu_read(0x8000), 0x44);

        // $C000-$FFFF from $5117
        m.cpu_write(0x5117, 0x86); // ROM bank 6 (16KB aligned)
        assert_eq!(m.cpu_read(0xC000), 0x46);
    }

    #[test]
    fn test_exram_nametable_source() {
        let mut m = make_mapper(2, 1);
        m.cpu_write(0x5104, 0); // ExRAM mode 0: nametable data
                                // Map NT0 to ExRAM (source 2)
        m.cpu_write(0x5105, 0x02);

        // Write to ExRAM via $5C00
        m.in_frame = true; // ExRAM writes only work in-frame for mode 0
        m.cpu_write(0x5C00, 0x55);

        // Reading nametable should come from ExRAM
        assert_eq!(m.ppu_read(0x2000), 0x55);
    }

    #[test]
    fn test_chr_mode_0_8kb() {
        let mut m = make_mapper(2, 4);
        m.cpu_write(0x5101, 0); // 8KB mode
        m.cpu_write(0x5127, 1); // CHR bank 1 (= 8KB, maps to 1KB banks 8-15)
        m.last_chr_write_was_b = false;

        // Bank 1 means 1KB banks 8..15
        // First byte of 1KB bank 8 should be 8 (set in make_mapper)
        let val = m.ppu_read(0x0000);
        assert_eq!(val, 8);
    }

    #[test]
    fn test_chr_b_bank_2kb_mode_uses_correct_registers() {
        let mut m = make_mapper(2, 4);
        m.cpu_write(0x5101, 2); // 2KB mode
        m.rendering = false; // PPUDATA path so we can control use_b
                             // Write B registers: B[0]=$5128, B[1]=$5129, B[2]=$512A, B[3]=$512B
        m.cpu_write(0x5128, 0); // B[0] — should NOT be used
        m.cpu_write(0x5129, 3); // B[1] — used for slots 0,2
        m.cpu_write(0x512A, 0); // B[2] — should NOT be used
        m.cpu_write(0x512B, 7); // B[3] — used for slots 1,3
                                // last_chr_write_was_b is true after writing $512B

        // Slot 0 ($0000-$07FF): should use B[1]=3, base = 3*2 = bank 6
        let val = m.ppu_read(0x0000);
        assert_eq!(val, 6); // First byte of 1KB bank 6

        // Slot 1 ($0800-$0FFF): should use B[3]=7, base = 7*2 = bank 14
        let val = m.ppu_read(0x0800);
        assert_eq!(val, 14); // First byte of 1KB bank 14

        // Slot 2 ($1000-$17FF): mirrors slot 0, should use B[1]=3
        let val = m.ppu_read(0x1000);
        assert_eq!(val, 6);

        // Slot 3 ($1800-$1FFF): mirrors slot 1, should use B[3]=7
        let val = m.ppu_read(0x1800);
        assert_eq!(val, 14);
    }

    #[test]
    fn test_pcm_channel() {
        let mut m = make_mapper(2, 1);
        // Write mode (default)
        m.cpu_write(0x5011, 0x80);
        assert!(m.audio_expansion_output() < 0.0); // Inverted polarity

        // Value 0 is ignored
        m.cpu_write(0x5011, 0x00);
        assert!(m.audio_expansion_output() < 0.0); // Still 0x80

        // Read mode blocks writes
        m.cpu_write(0x5010, 0x01);
        m.cpu_write(0x5011, 0x40);
        assert!(m.audio_expansion_output() < 0.0); // Still 0x80
    }

    #[test]
    fn test_status_register_5015() {
        let mut m = make_mapper(2, 1);
        m.cpu_write(0x5015, 0x03); // Enable both pulses
        m.cpu_write(0x5000, 0x3F); // Duty 0, halt, constant vol 15
        m.cpu_write(0x5002, 0x10);
        m.cpu_write(0x5003, 0x08); // Sets length counter

        let status = m.cpu_read(0x5015);
        assert_eq!(status & 0x01, 0x01); // Pulse 1 length > 0
    }

    #[test]
    fn test_delayed_frame_entry() {
        let mut m = make_mapper(2, 1);
        // Simulate first scanline boundary via 3 consecutive identical reads
        simulate_scanline_boundary(&mut m);
        assert!(!m.in_frame);
        assert!(m.need_in_frame);

        // A nametable tile fetch should convert need_in_frame to in_frame
        m.ppu_fetch(0x2000, 0);
        assert!(m.in_frame);
        assert!(!m.need_in_frame);
    }

    #[test]
    fn test_split_tile_number_sprite_detection() {
        let mut m = make_mapper(2, 4);
        m.cpu_write(0x5101, 3);
        m.large_sprites = true;
        m.in_frame = true;
        m.rendering = true;

        // A banks for sprites
        m.cpu_write(0x5120, 5);
        // B banks for BG
        m.cpu_write(0x5128, 10);

        // In sprite tile range (32-39): should use A banks
        m.split_tile_number = 33;
        let val = m.ppu_read(0x0000);
        assert_eq!(val, 5);

        // In BG tile range: should use B banks
        m.split_tile_number = 10;
        let val = m.ppu_read(0x0000);
        assert_eq!(val, 10);
    }

    #[test]
    fn test_8x8_rendering_always_uses_a_banks() {
        let mut m = make_mapper(2, 4);
        m.cpu_write(0x5101, 3); // 1KB mode
        m.large_sprites = false; // 8x8 mode
        m.rendering = true;

        m.cpu_write(0x5120, 5); // A bank
        m.cpu_write(0x5128, 10); // B bank (last written)
                                 // During rendering in 8x8 mode, A banks are always used
        let val = m.ppu_read(0x0000);
        assert_eq!(val, 5);
    }

    #[test]
    fn test_8x8_ppudata_uses_last_written_set() {
        let mut m = make_mapper(2, 4);
        m.cpu_write(0x5101, 3);
        m.large_sprites = false; // 8x8 mode
        m.rendering = false; // PPUDATA access

        m.cpu_write(0x5120, 5); // A bank
                                // After writing A, PPUDATA should use A banks
        let val = m.ppu_read(0x0000);
        assert_eq!(val, 5);

        m.cpu_write(0x5128, 10); // B bank (now last written)
                                 // After writing B, PPUDATA should use B banks
        let val = m.ppu_read(0x0000);
        assert_eq!(val, 10);
    }

    #[test]
    fn test_vsplit_registers() {
        let mut m = make_mapper(2, 1);
        m.cpu_write(0x5200, 0xC5); // Enable, right side, delimiter 5
        m.cpu_write(0x5201, 0x10); // Scroll offset 16
        m.cpu_write(0x5202, 0x03); // CHR bank 3
        assert_eq!(m.vsplit_mode, 0xC5);
        assert_eq!(m.vsplit_scroll, 0x10);
        assert_eq!(m.vsplit_bank, 0x03);
    }

    #[test]
    fn test_exattr_mode1_attribute_override() {
        let mut m = make_mapper(2, 4);
        m.cpu_write(0x5104, 1); // ExRAM mode 1
        m.in_frame = true;
        m.split_tile_number = 0;

        // Write extended attribute data to ExRAM: palette 2, CHR bank 3
        // palette=2 is bits 7:6 = 0b10, chr_bank=3 is bits 5:0 = 0b000011
        // byte = 0b10_000011 = 0x83
        m.cpu_write(0x5104, 2); // Temporarily switch to mode 2 to write ExRAM
        m.cpu_write(0x5C00, 0x83);
        m.cpu_write(0x5104, 1); // Back to mode 1

        // Simulate a tile fetch at NT address $2000
        let _tile = m.ppu_fetch(0x2000, 0);

        // Next read in NT range should be attribute override
        let attr = m.ppu_fetch(0x23C0, 0);
        // Palette 2 replicated: 0b10_10_10_10 = 0xAA
        assert_eq!(attr, 0xAA);
    }
}
