use super::super::super::io;
use super::{
    mirror_nametable_addr, NametableMirror, CPU_RAM_SIZE, PALETTE_MIRROR_CLEAR,
    PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, PRG_RAM_8K, RESET_TARGET_ADDR, VRAM_SIZE,
};
use crate::emu::apu::ChannelDebugState;
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x2000; // 8KB
const CHR_BANK_SIZE: usize = 0x0400; // 1KB
const WAVEFORM_SIZE: usize = 512;
const WAVEFORM_DECIMATION: u32 = 56;

struct Vrc6Pulse {
    volume: u8,
    duty: u8,
    mode: bool,
    period: u16,
    enabled: bool,
    timer: u16,
    phase: u8,
    output: u8,
}

impl Vrc6Pulse {
    fn new() -> Self {
        Self {
            volume: 0,
            duty: 0,
            mode: false,
            period: 0,
            enabled: false,
            timer: 1,
            phase: 0,
            output: 0,
        }
    }

    fn write_control(&mut self, value: u8) {
        self.mode = value & 0x80 != 0;
        self.duty = (value >> 4) & 0x07;
        self.volume = value & 0x0F;
    }

    fn write_freq_lo(&mut self, value: u8) {
        self.period = (self.period & 0xF00) | value as u16;
    }

    fn write_freq_hi(&mut self, value: u8) {
        self.period = (self.period & 0x0FF) | ((value as u16 & 0x0F) << 8);
        self.enabled = value & 0x80 != 0;
        if !self.enabled {
            self.phase = 0;
            self.output = 0;
        }
    }

    fn clock(&mut self, freq_shift: u8) {
        if !self.enabled {
            self.output = 0;
            return;
        }
        if self.timer == 0 {
            self.timer = (self.period >> freq_shift) + 1;
            self.phase = (self.phase + 1) & 0x0F;
        } else {
            self.timer -= 1;
        }
        self.output = if self.mode || self.phase <= self.duty {
            self.volume
        } else {
            0
        };
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_u8(self.volume);
        w.write_u8(self.duty);
        w.write_bool(self.mode);
        w.write_u16(self.period);
        w.write_bool(self.enabled);
        w.write_u16(self.timer);
        w.write_u8(self.phase);
        w.write_u8(self.output);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        self.volume = r.read_u8()?;
        self.duty = r.read_u8()?;
        self.mode = r.read_bool()?;
        self.period = r.read_u16()?;
        self.enabled = r.read_bool()?;
        self.timer = r.read_u16()?;
        self.phase = r.read_u8()?;
        self.output = r.read_u8()?;
        Ok(())
    }
}

struct Vrc6Sawtooth {
    rate: u8,
    period: u16,
    enabled: bool,
    timer: u16,
    step: u8,
    accumulator: u8,
    output: u8,
}

impl Vrc6Sawtooth {
    fn new() -> Self {
        Self {
            rate: 0,
            period: 0,
            enabled: false,
            timer: 1,
            step: 0,
            accumulator: 0,
            output: 0,
        }
    }

    fn write_rate(&mut self, value: u8) {
        self.rate = value & 0x3F;
    }

    fn write_freq_lo(&mut self, value: u8) {
        self.period = (self.period & 0xF00) | value as u16;
    }

    fn write_freq_hi(&mut self, value: u8) {
        self.period = (self.period & 0x0FF) | ((value as u16 & 0x0F) << 8);
        self.enabled = value & 0x80 != 0;
        if !self.enabled {
            self.accumulator = 0;
            self.step = 0;
            self.output = 0;
        }
    }

    fn clock(&mut self, freq_shift: u8) {
        if !self.enabled {
            self.output = 0;
            return;
        }
        if self.timer == 0 {
            self.timer = (self.period >> freq_shift) + 1;
            self.step = (self.step + 1) % 14;
            if self.step == 0 {
                self.accumulator = 0;
            } else if self.step.is_multiple_of(2) {
                self.accumulator = self.accumulator.wrapping_add(self.rate);
            }
        } else {
            self.timer -= 1;
        }
        self.output = self.accumulator >> 3;
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_u8(self.rate);
        w.write_u16(self.period);
        w.write_bool(self.enabled);
        w.write_u16(self.timer);
        w.write_u8(self.step);
        w.write_u8(self.accumulator);
        w.write_u8(self.output);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        self.rate = r.read_u8()?;
        self.period = r.read_u16()?;
        self.enabled = r.read_bool()?;
        self.timer = r.read_u16()?;
        self.step = r.read_u8()?;
        self.accumulator = r.read_u8()?;
        self.output = r.read_u8()?;
        Ok(())
    }
}

struct WaveformCapture {
    buffers: [Vec<f32>; 3],
    write_pos: usize,
    decimation_counter: u32,
}

impl WaveformCapture {
    fn new() -> Self {
        Self {
            buffers: [
                vec![0.0; WAVEFORM_SIZE],
                vec![0.0; WAVEFORM_SIZE],
                vec![0.0; WAVEFORM_SIZE],
            ],
            write_pos: 0,
            decimation_counter: 0,
        }
    }

    fn push(&mut self, samples: [f32; 3]) {
        self.decimation_counter += 1;
        if self.decimation_counter < WAVEFORM_DECIMATION {
            return;
        }
        self.decimation_counter = 0;
        for (i, &s) in samples.iter().enumerate() {
            self.buffers[i][self.write_pos] = s;
        }
        self.write_pos = (self.write_pos + 1) % WAVEFORM_SIZE;
    }

    fn read_buffer(&self, ch: usize) -> Vec<f32> {
        let mut out = vec![0.0; WAVEFORM_SIZE];
        let pos = self.write_pos;
        let (tail, head) = self.buffers[ch].split_at(pos);
        out[..head.len()].copy_from_slice(head);
        out[head.len()..].copy_from_slice(tail);
        out
    }
}

pub struct Vrc6Mapper {
    controllers: [io::controller::Controller; 2],
    mapper_id: u8,

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_rom: Vec<[u8; CHR_BANK_SIZE]>,
    prg_ram: Box<[u8; PRG_RAM_8K]>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    has_battery: bool,
    vram: Box<[u8; VRAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],

    prg_bank_16k: u8,
    prg_bank_8k: u8,
    chr_banks: [u8; 8],
    ppu_banking_style: u8,
    mirroring: NametableMirror,

    irq_latch: u8,
    irq_counter: u8,
    irq_prescaler: i16,
    irq_mode_cycle: bool,
    irq_enable: bool,
    irq_enable_after_ack: bool,
    irq_pending: bool,

    pulse1: Vrc6Pulse,
    pulse2: Vrc6Pulse,
    saw: Vrc6Sawtooth,
    freq_halt: bool,
    freq_shift: u8,

    debug_capture: Option<WaveformCapture>,
}

impl Vrc6Mapper {
    pub fn new(
        flags: u8,
        prg_banks_16k: Vec<[u8; 16384]>,
        chr_banks_8k: Vec<[u8; 8192]>,
        has_battery: bool,
        sram_data: Option<Vec<u8>>,
        mapper_id: u8,
    ) -> Self {
        let mut prg_rom = vec![];
        for bank in &prg_banks_16k {
            prg_rom.push(<[u8; PRG_BANK_SIZE]>::try_from(&bank[0..PRG_BANK_SIZE]).unwrap());
            prg_rom.push(
                <[u8; PRG_BANK_SIZE]>::try_from(&bank[PRG_BANK_SIZE..2 * PRG_BANK_SIZE]).unwrap(),
            );
        }

        let mut chr_rom: Vec<[u8; CHR_BANK_SIZE]> = vec![];
        for bank in &chr_banks_8k {
            for i in 0..8 {
                chr_rom.push(
                    <[u8; CHR_BANK_SIZE]>::try_from(
                        &bank[i * CHR_BANK_SIZE..(i + 1) * CHR_BANK_SIZE],
                    )
                    .unwrap(),
                );
            }
        }

        let mirroring = if flags & 1 != 0 {
            NametableMirror::Vertical
        } else {
            NametableMirror::Horizontal
        };

        Vrc6Mapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            mapper_id,
            prg_rom,
            chr_rom,
            prg_ram: {
                let mut ram = Box::new([0; PRG_RAM_8K]);
                if let Some(data) = sram_data {
                    let len = data.len().min(PRG_RAM_8K);
                    ram[..len].copy_from_slice(&data[..len]);
                }
                ram
            },
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            has_battery,
            vram: Box::new([0; VRAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],

            prg_bank_16k: 0,
            prg_bank_8k: 0,
            chr_banks: [0; 8],
            ppu_banking_style: 0,
            mirroring,

            irq_latch: 0,
            irq_counter: 0,
            irq_prescaler: 341,
            irq_mode_cycle: false,
            irq_enable: false,
            irq_enable_after_ack: false,
            irq_pending: false,

            pulse1: Vrc6Pulse::new(),
            pulse2: Vrc6Pulse::new(),
            saw: Vrc6Sawtooth::new(),
            freq_halt: false,
            freq_shift: 0,

            debug_capture: None,
        }
    }

    fn decode_reg(&self, addr: u16) -> (u16, u8) {
        let base = addr & 0xF000;
        let reg = if self.mapper_id == 26 {
            (((addr >> 1) & 1) | ((addr & 1) << 1)) as u8
        } else {
            (addr & 3) as u8
        };
        (base, reg)
    }

    fn prg_16k_index(&self, bank: u8) -> usize {
        let idx = (bank as usize & 0x0F) * 2;
        idx % self.prg_rom.len().max(1)
    }

    fn prg_8k_index(&self, bank: u8) -> usize {
        (bank as usize & 0x1F) % self.prg_rom.len().max(1)
    }

    fn chr_index(&self, bank: u8) -> usize {
        bank as usize % self.chr_rom.len().max(1)
    }

    fn prg_ram_enabled(&self) -> bool {
        self.ppu_banking_style & 0x80 != 0
    }

    fn clock_irq_counter(&mut self) {
        if self.irq_counter == 0xFF {
            self.irq_counter = self.irq_latch;
            self.irq_pending = true;
        } else {
            self.irq_counter += 1;
        }
    }

    fn handle_write(&mut self, addr: u16, value: u8) {
        let (base, reg) = self.decode_reg(addr);

        match (base, reg) {
            (0x8000, _) => self.prg_bank_16k = value & 0x0F,
            (0xC000, _) => self.prg_bank_8k = value & 0x1F,

            (0x9000, 0) => self.pulse1.write_control(value),
            (0x9000, 1) => self.pulse1.write_freq_lo(value),
            (0x9000, 2) => self.pulse1.write_freq_hi(value),
            (0x9000, 3) => {
                self.freq_halt = value & 0x01 != 0;
                self.freq_shift = if value & 0x04 != 0 {
                    8
                } else if value & 0x02 != 0 {
                    4
                } else {
                    0
                };
            }

            (0xA000, 0) => self.pulse2.write_control(value),
            (0xA000, 1) => self.pulse2.write_freq_lo(value),
            (0xA000, 2) => self.pulse2.write_freq_hi(value),

            (0xB000, 0) => self.saw.write_rate(value),
            (0xB000, 1) => self.saw.write_freq_lo(value),
            (0xB000, 2) => self.saw.write_freq_hi(value),
            (0xB000, 3) => {
                self.ppu_banking_style = value;
                self.mirroring = match (value >> 2) & 0x03 {
                    0 => NametableMirror::Vertical,
                    1 => NametableMirror::Horizontal,
                    2 => NametableMirror::Lower,
                    3 => NametableMirror::Higher,
                    _ => unreachable!(),
                };
            }

            (0xD000, r @ 0..=3) => self.chr_banks[r as usize] = value,
            (0xE000, r @ 0..=3) => self.chr_banks[4 + r as usize] = value,

            (0xF000, 0) => self.irq_latch = value,
            (0xF000, 1) => {
                self.irq_pending = false;
                self.irq_enable_after_ack = (value & 0x01) != 0;
                self.irq_enable = (value & 0x02) != 0;
                self.irq_mode_cycle = (value & 0x04) != 0;
                if self.irq_enable {
                    self.irq_counter = self.irq_latch;
                    self.irq_prescaler = 341;
                }
            }
            (0xF000, 2) => {
                self.irq_pending = false;
                self.irq_enable = self.irq_enable_after_ack;
            }

            _ => {}
        }
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0x9FFF => {
                let bank = self.prg_16k_index(self.prg_bank_16k);
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0x8000) as usize])
            }
            0xA000..=0xBFFF => {
                let bank = self.prg_16k_index(self.prg_bank_16k) + 1;
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0xA000) as usize])
            }
            0xC000..=0xDFFF => {
                let bank = self.prg_8k_index(self.prg_bank_8k);
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0xC000) as usize])
            }
            0xE000..=0xFFFF => {
                let bank = self.prg_rom.len().saturating_sub(1);
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0xE000) as usize])
            }
            _ => 0,
        }
    }
}

impl MemoryMapper for Vrc6Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x6000..=0x7FFF if self.prg_ram_enabled() => self.prg_ram[(addr - 0x6000) as usize],
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => 0,
        }
    }

    fn cpu_peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x6000..=0x7FFF if self.prg_ram_enabled() => self.prg_ram[(addr - 0x6000) as usize],
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize] = value,
            0x6000..=0x7FFF if self.prg_ram_enabled() => {
                self.prg_ram[(addr - 0x6000) as usize] = value;
            }
            0x8000..=0xFFFF => self.handle_write(addr, value),
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let slot = (addr >> 10) as usize & 7;
                let bank = self.chr_index(self.chr_banks[slot]);
                self.chr_rom
                    .get(bank)
                    .map_or(0, |b| b[addr as usize & 0x3FF])
            }
            0x2000..=0x3EFF => {
                let mirrored = mirror_nametable_addr(addr, self.mirroring);
                self.vram[(mirrored & 0x7FF) as usize]
            }
            0x3F00..=0x3FFF => {
                let mut idx = (addr as usize - PALETTE_START as usize) % PALETTE_SIZE;
                if idx & PALETTE_MIRROR_MASK == PALETTE_MIRROR_CLEAR {
                    idx &= !PALETTE_MIRROR_CLEAR;
                }
                self.palette_ram[idx]
            }
            _ => 0,
        }
    }

    unsafe fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        match addr {
            0x0000..=0x1FFF => {
                let slot = (addr >> 10) as usize & 7;
                let bank = self.chr_index(self.chr_banks[slot]);
                if let Some(b) = self.chr_rom.get(bank) {
                    let offset = addr as usize & 0x3FF;
                    let copy_size = size.min(CHR_BANK_SIZE - offset);
                    unsafe { std::ptr::copy(b.as_ptr().add(offset), dest, copy_size) }
                }
            }
            0x2000..=0x3EFF => {
                let mirrored = mirror_nametable_addr(addr, self.mirroring);
                let vram_addr = (mirrored & 0x7FF) as usize;
                let copy_size = size.min(VRAM_SIZE as usize - vram_addr);
                unsafe { std::ptr::copy(self.vram.as_ptr().add(vram_addr), dest, copy_size) }
            }
            _ => {}
        }
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x2000..=0x3EFF => {
                let mirrored = mirror_nametable_addr(addr, self.mirroring);
                self.vram[(mirrored & 0x7FF) as usize] = value;
            }
            0x3F00..=0x3FFF => {
                let mut idx = (addr as usize - PALETTE_START as usize) % PALETTE_SIZE;
                if idx & PALETTE_MIRROR_MASK == PALETTE_MIRROR_CLEAR {
                    idx &= !PALETTE_MIRROR_CLEAR;
                }
                self.palette_ram[idx] = value;
            }
            _ => {}
        }
    }

    fn cpu_cycle(&mut self, _ppu_dot: u64) {
        if self.irq_enable {
            if self.irq_mode_cycle {
                self.clock_irq_counter();
            } else {
                self.irq_prescaler -= 3;
                if self.irq_prescaler <= 0 {
                    self.irq_prescaler += 341;
                    self.clock_irq_counter();
                }
            }
        }

        if !self.freq_halt {
            self.pulse1.clock(self.freq_shift);
            self.pulse2.clock(self.freq_shift);
            self.saw.clock(self.freq_shift);
        }

        if let Some(ref mut capture) = self.debug_capture {
            capture.push([
                self.pulse1.output as f32,
                self.pulse2.output as f32,
                self.saw.output as f32,
            ]);
        }
    }

    fn audio_expansion_output(&self) -> f32 {
        let p1 = self.pulse1.output as f32;
        let p2 = self.pulse2.output as f32;
        let saw = self.saw.output as f32;
        (p1 + p2 + saw) / 61.0
    }

    fn code_start(&mut self) -> u16 {
        let lo = self.cpu_read(RESET_TARGET_ADDR);
        let hi = self.cpu_read(RESET_TARGET_ADDR + 1);
        ((hi as u16) << 8) | lo as u16
    }

    fn controllers(&mut self) -> &mut [io::controller::Controller; 2] {
        &mut self.controllers
    }

    fn poll_irq(&mut self) -> bool {
        self.irq_pending
    }

    fn sram_data(&self) -> Option<&[u8]> {
        if self.has_battery {
            Some(&self.prg_ram[..])
        } else {
            None
        }
    }

    fn sram_data_mut(&mut self) -> Option<&mut [u8]> {
        if self.has_battery {
            Some(&mut self.prg_ram[..])
        } else {
            None
        }
    }

    fn mapper_id(&self) -> u8 {
        self.mapper_id
    }

    fn set_debug_capture(&mut self, on: bool) {
        if on && self.debug_capture.is_none() {
            self.debug_capture = Some(WaveformCapture::new());
        } else if !on {
            self.debug_capture = None;
        }
    }

    fn expansion_audio_debug(&self) -> Vec<ChannelDebugState> {
        let empty = vec![0.0; WAVEFORM_SIZE];
        let read_buf = |ch: usize| -> Vec<f32> {
            self.debug_capture
                .as_ref()
                .map(|c| c.read_buffer(ch))
                .unwrap_or_else(|| empty.clone())
        };
        vec![
            ChannelDebugState {
                name: "VP1",
                enabled: self.pulse1.enabled,
                length_counter: 0,
                waveform: read_buf(0),
            },
            ChannelDebugState {
                name: "VP2",
                enabled: self.pulse2.enabled,
                length_counter: 0,
                waveform: read_buf(1),
            },
            ChannelDebugState {
                name: "Saw",
                enabled: self.saw.enabled,
                length_counter: 0,
                waveform: read_buf(2),
            },
        ]
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.prg_ram);
        w.write_bytes(&*self.vram);
        w.write_bytes(&self.palette_ram);
        for v in &self.chr_rom {
            w.write_bytes(v);
        }
        w.write_u8(self.prg_bank_16k);
        w.write_u8(self.prg_bank_8k);
        for &b in &self.chr_banks {
            w.write_u8(b);
        }
        w.write_u8(self.ppu_banking_style);
        super::save_mirroring(w, self.mirroring);

        w.write_u8(self.irq_latch);
        w.write_u8(self.irq_counter);
        w.write_u16(self.irq_prescaler as u16);
        w.write_bool(self.irq_mode_cycle);
        w.write_bool(self.irq_enable);
        w.write_bool(self.irq_enable_after_ack);
        w.write_bool(self.irq_pending);

        self.pulse1.save_state(w);
        self.pulse2.save_state(w);
        self.saw.save_state(w);
        w.write_bool(self.freq_halt);
        w.write_u8(self.freq_shift);

        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut *self.prg_ram)?;
        r.read_bytes_into(&mut *self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        for v in &mut self.chr_rom {
            r.read_bytes_into(v)?;
        }
        self.prg_bank_16k = r.read_u8()?;
        self.prg_bank_8k = r.read_u8()?;
        for b in &mut self.chr_banks {
            *b = r.read_u8()?;
        }
        self.ppu_banking_style = r.read_u8()?;
        self.mirroring = super::load_mirroring(r)?;

        self.irq_latch = r.read_u8()?;
        self.irq_counter = r.read_u8()?;
        self.irq_prescaler = r.read_u16()? as i16;
        self.irq_mode_cycle = r.read_bool()?;
        self.irq_enable = r.read_bool()?;
        self.irq_enable_after_ack = r.read_bool()?;
        self.irq_pending = r.read_bool()?;

        self.pulse1.load_state(r)?;
        self.pulse2.load_state(r)?;
        self.saw.load_state(r)?;
        self.freq_halt = r.read_bool()?;
        self.freq_shift = r.read_u8()?;

        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper(mapper_id: u8, prg_count: usize, chr_count: usize) -> Vrc6Mapper {
        let mut prg_banks = vec![];
        for i in 0..prg_count {
            let mut bank = [0u8; 16384];
            let lo = (i * 2) as u8;
            bank[0] = lo;
            bank[PRG_BANK_SIZE] = lo + 1;
            prg_banks.push(bank);
        }
        let mut chr_banks = vec![];
        for i in 0..chr_count {
            let mut bank = [0u8; 8192];
            for k in 0..8 {
                bank[k * CHR_BANK_SIZE] = (i * 8 + k) as u8;
            }
            chr_banks.push(bank);
        }
        Vrc6Mapper::new(1, prg_banks, chr_banks, true, None, mapper_id)
    }

    #[test]
    fn test_prg_banking() {
        let mut m = make_mapper(24, 8, 4);
        // 16KB bank 0 = 8KB banks 0+1
        assert_eq!(m.cpu_read(0x8000), 0);
        assert_eq!(m.cpu_read(0xA000), 1);
        // Last 8KB bank fixed
        assert_eq!(m.cpu_read(0xE000), 15);

        // Switch 16KB bank
        m.cpu_write(0x8000, 2);
        assert_eq!(m.cpu_read(0x8000), 4);
        assert_eq!(m.cpu_read(0xA000), 5);

        // Switch 8KB bank at $C000
        m.cpu_write(0xC000, 3);
        assert_eq!(m.cpu_read(0xC000), 3);
    }

    #[test]
    fn test_chr_banking() {
        let mut m = make_mapper(24, 4, 4);
        assert_eq!(m.ppu_read(0x0000), 0);

        m.cpu_write(0xD000, 5);
        assert_eq!(m.ppu_read(0x0000), 5);

        m.cpu_write(0xD001, 10);
        assert_eq!(m.ppu_read(0x0400), 10);
    }

    #[test]
    fn test_mapper26_addr_swap() {
        let mut m = make_mapper(26, 4, 4);
        // $9001 on mapper 26 swaps A0/A1 → decoded as reg 2 (freq_hi for pulse1)
        // $9002 on mapper 26 swaps → decoded as reg 1 (freq_lo for pulse1)
        m.pulse1.period = 0;
        m.cpu_write(0x9002, 0xAB); // mapper 26: reg 1 = freq_lo
        assert_eq!(m.pulse1.period & 0xFF, 0xAB);
    }

    #[test]
    fn test_mirroring() {
        let mut m = make_mapper(24, 4, 4);
        // Default vertical
        assert_eq!(m.mirroring, NametableMirror::Vertical);

        // $B003 sets mirroring
        m.cpu_write(0xB003, 0x04); // horizontal
        assert_eq!(m.mirroring, NametableMirror::Horizontal);

        m.cpu_write(0xB003, 0x08); // one-screen A
        assert_eq!(m.mirroring, NametableMirror::Lower);

        m.cpu_write(0xB003, 0x0C); // one-screen B
        assert_eq!(m.mirroring, NametableMirror::Higher);
    }

    #[test]
    fn test_pulse_channel() {
        let mut p = Vrc6Pulse::new();
        p.write_control(0x8F); // mode=1, duty=0, volume=15
        p.write_freq_lo(0x00);
        p.write_freq_hi(0x80); // enabled, period=0

        // Timer starts at 1, first clock decrements to 0
        p.clock(0);
        // Timer was 1, decremented to 0 — no phase advance yet
        // mode=1 always outputs volume once phase advances
        p.clock(0);
        // Timer was 0, now reloads to (0>>0)+1=1 and phase advances 0→1
        assert_eq!(p.output, 15);

        // Disable
        p.write_freq_hi(0x00);
        assert_eq!(p.phase, 0); // phase reset to 0 on disable
        p.clock(0);
        assert_eq!(p.output, 0);
    }

    #[test]
    fn test_pulse_duty_cycle() {
        let mut p = Vrc6Pulse::new();
        p.write_control(0x0F); // mode=0, duty=0, volume=15
        p.write_freq_lo(0x00);
        p.write_freq_hi(0x80); // enabled, period=0
        p.timer = 0; // force immediate phase advance

        // duty=0: phase <= 0 outputs, so only phase 0 (1/16)
        // With period=0, timer reloads to 1, so phase advances every 2 clocks
        let mut high_count = 0;
        for _ in 0..64 {
            p.clock(0);
            if p.output > 0 {
                high_count += 1;
            }
        }
        // 64 clocks / 2 = 32 phase advances = 2 full 16-step cycles
        // 1 high phase per cycle × 2 ticks per phase = 4 high outputs
        assert_eq!(high_count, 4);
    }

    #[test]
    fn test_sawtooth_channel() {
        let mut s = Vrc6Sawtooth::new();
        s.write_rate(2);
        s.write_freq_lo(0x00);
        s.write_freq_hi(0x80); // enabled, period=0
        s.timer = 0; // force immediate step advance

        // With period=0, timer reloads to 1, so steps advance every 2 clocks.
        // Step sequence (after increment): 1,2,3,...,13,0,1,...
        // Rate added on even steps: 2,4,6,8,10,12 → 6 additions of 2 = 12
        // Output = accumulator >> 3
        let mut saw_outputs = vec![];
        for _ in 0..28 {
            s.clock(0);
            saw_outputs.push(s.output);
        }
        // After 28 clocks = 14 step advances = one full sawtooth cycle
        // The last step wraps to 0 and resets accumulator
        assert_eq!(*saw_outputs.last().unwrap(), 0);
        // Peak should have been (6 * 2) >> 3 = 1
        assert!(saw_outputs.iter().any(|&o| o > 0));
    }

    #[test]
    fn test_freq_halt() {
        let mut m = make_mapper(24, 4, 4);
        m.pulse1.write_control(0x8F);
        m.pulse1.write_freq_lo(0x00);
        m.pulse1.write_freq_hi(0x80);

        // Enable freq halt via $9003
        m.cpu_write(0x9003, 0x01);
        assert!(m.freq_halt);

        let output_before = m.pulse1.output;
        m.cpu_cycle(0);
        // With halt, audio channels should not have been clocked
        // (output may or may not change depending on state, but
        // the important thing is the channel wasn't clocked)
        assert_eq!(m.pulse1.output, output_before);
    }

    #[test]
    fn test_irq_cycle_mode() {
        let mut m = make_mapper(24, 4, 4);
        // Set latch to 0xFE
        m.cpu_write(0xF000, 0xFE);
        // Enable IRQ in cycle mode
        m.cpu_write(0xF001, 0x06); // E=1, M=1

        assert!(!m.irq_pending);
        // Counter starts at 0xFE, needs 2 cycles to wrap
        m.cpu_cycle(0); // counter = 0xFF
        assert!(!m.irq_pending);
        m.cpu_cycle(0); // counter wraps, pending
        assert!(m.irq_pending);
        assert_eq!(m.irq_counter, 0xFE); // reloaded from latch
    }

    #[test]
    fn test_irq_acknowledge() {
        let mut m = make_mapper(24, 4, 4);
        m.irq_pending = true;
        m.irq_enable_after_ack = true;

        // $F002 acknowledges and restores enable
        m.cpu_write(0xF002, 0x00);
        assert!(!m.irq_pending);
        assert!(m.irq_enable);
    }

    #[test]
    fn test_prg_ram_enable() {
        let mut m = make_mapper(24, 4, 4);
        // PRG-RAM disabled by default
        m.prg_ram[0] = 0x42;
        assert_eq!(m.cpu_read(0x6000), 0);

        // Enable via $B003 bit 7
        m.cpu_write(0xB003, 0x80);
        assert_eq!(m.cpu_read(0x6000), 0x42);

        // Write to PRG-RAM
        m.cpu_write(0x6000, 0x99);
        assert_eq!(m.prg_ram[0], 0x99);
    }

    #[test]
    fn test_audio_expansion_output_range() {
        let mut m = make_mapper(24, 4, 4);
        // All channels silent
        assert_eq!(m.audio_expansion_output(), 0.0);

        // Max output
        m.pulse1.output = 15;
        m.pulse2.output = 15;
        m.saw.output = 31;
        let max = m.audio_expansion_output();
        assert!((max - 1.0).abs() < 0.001);
    }
}
