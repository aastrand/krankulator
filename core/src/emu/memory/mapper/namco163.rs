use super::super::super::io;
use super::{
    CPU_RAM_SIZE, PALETTE_MIRROR_CLEAR, PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START,
    PRG_RAM_8K, RESET_TARGET_ADDR, VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x2000; // 8KB
const CHR_BANK_SIZE: usize = 0x0400; // 1KB
const SOUND_RAM_SIZE: usize = 128;

pub struct Namco163Mapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_rom: Vec<[u8; CHR_BANK_SIZE]>,
    has_chr_ram: bool,

    prg_ram: Box<[u8; PRG_RAM_8K]>,
    sound_ram: [u8; SOUND_RAM_SIZE],
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    vram: Box<[u8; VRAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],

    prg_banks: [u8; 3],
    chr_banks: [u8; 12], // 0-7: CHR pattern, 8-11: nametable

    chr_ram_enable_lo: bool,
    chr_ram_enable_hi: bool,

    irq_counter: u16,
    irq_enabled: bool,
    irq_pending: bool,

    sound_addr: u8,
    sound_auto_increment: bool,
    sound_disable: bool,

    wram_protect: u8,

    audio_output: f32,
    audio_channel_index: u8,
    audio_timer: u8,

    has_battery: bool,
}

impl Namco163Mapper {
    pub fn new(
        flags: u8,
        prg_banks_16k: Vec<[u8; 16384]>,
        chr_banks_8k: Vec<[u8; 8192]>,
        has_battery: bool,
        sram_data: Option<Vec<u8>>,
        _submapper: u8,
    ) -> Self {
        let mut prg_rom = vec![];
        for bank in &prg_banks_16k {
            prg_rom.push(<[u8; PRG_BANK_SIZE]>::try_from(&bank[0..PRG_BANK_SIZE]).unwrap());
            prg_rom.push(
                <[u8; PRG_BANK_SIZE]>::try_from(&bank[PRG_BANK_SIZE..2 * PRG_BANK_SIZE]).unwrap(),
            );
        }

        let has_chr_ram = chr_banks_8k.is_empty();
        let mut chr_rom = vec![];
        if has_chr_ram {
            for _ in 0..8 {
                chr_rom.push([0u8; CHR_BANK_SIZE]);
            }
        } else {
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
        }

        let _ = flags;

        let mut prg_ram = Box::new([0u8; PRG_RAM_8K]);
        if let Some(data) = sram_data {
            let len = data.len().min(PRG_RAM_8K);
            prg_ram[..len].copy_from_slice(&data[..len]);
        }

        Namco163Mapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            chr_rom,
            has_chr_ram,
            prg_ram,
            sound_ram: [0; SOUND_RAM_SIZE],
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            vram: Box::new([0; VRAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
            prg_banks: [0, 1, 2],
            chr_banks: [0; 12],
            chr_ram_enable_lo: false,
            chr_ram_enable_hi: false,
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
            sound_addr: 0,
            sound_auto_increment: false,
            sound_disable: false,
            wram_protect: 0,
            audio_output: 0.0,
            audio_channel_index: 0,
            audio_timer: 0,
            has_battery,
        }
    }

    fn prg_bank_index(&self, bank: u8) -> usize {
        (bank & 0x3F) as usize % self.prg_rom.len().max(1)
    }

    fn chr_1k_index(&self, bank: u8) -> usize {
        bank as usize % self.chr_rom.len().max(1)
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let slot = (addr >> 10) as usize & 7;
        let bank_val = self.chr_banks[slot];
        let is_upper_half = slot >= 4;

        let use_ram = self.has_chr_ram
            && bank_val >= 0xE0
            && if is_upper_half {
                self.chr_ram_enable_hi
            } else {
                self.chr_ram_enable_lo
            };

        if use_ram {
            let ram_addr = ((bank_val & 0x1F) as usize * CHR_BANK_SIZE) + (addr as usize & 0x3FF);
            let idx = ram_addr % (self.chr_rom.len() * CHR_BANK_SIZE);
            self.chr_rom[idx / CHR_BANK_SIZE][idx % CHR_BANK_SIZE]
        } else {
            let bank = self.chr_1k_index(bank_val);
            self.chr_rom
                .get(bank)
                .map_or(0, |b| b[addr as usize & 0x3FF])
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if !self.has_chr_ram {
            return;
        }
        let slot = (addr >> 10) as usize & 7;
        let bank_val = self.chr_banks[slot];
        let is_upper_half = slot >= 4;

        let use_ram = bank_val >= 0xE0
            && if is_upper_half {
                self.chr_ram_enable_hi
            } else {
                self.chr_ram_enable_lo
            };

        if use_ram {
            let ram_addr = ((bank_val & 0x1F) as usize * CHR_BANK_SIZE) + (addr as usize & 0x3FF);
            let idx = ram_addr % (self.chr_rom.len() * CHR_BANK_SIZE);
            self.chr_rom[idx / CHR_BANK_SIZE][idx % CHR_BANK_SIZE] = value;
        }
    }

    fn read_nt(&self, addr: u16) -> u8 {
        let slot = ((addr >> 10) & 3) as usize;
        let bank_val = self.chr_banks[8 + slot];
        let offset = addr as usize & 0x3FF;

        if bank_val >= 0xE0 {
            let page = (bank_val & 1) as usize;
            self.vram[page * 0x400 + offset]
        } else {
            let bank = self.chr_1k_index(bank_val);
            self.chr_rom.get(bank).map_or(0, |b| b[offset])
        }
    }

    fn write_nt(&mut self, addr: u16, value: u8) {
        let slot = ((addr >> 10) & 3) as usize;
        let bank_val = self.chr_banks[8 + slot];
        let offset = addr as usize & 0x3FF;

        if bank_val >= 0xE0 {
            let page = (bank_val & 1) as usize;
            self.vram[page * 0x400 + offset] = value;
        }
    }

    fn clock_audio(&mut self) {
        if self.sound_disable {
            return;
        }

        self.audio_timer += 1;
        if self.audio_timer < 15 {
            return;
        }
        self.audio_timer = 0;

        let num_active = ((self.sound_ram[0x7F] >> 4) & 7) + 1;
        let first_active = 8 - num_active;

        if self.audio_channel_index < first_active {
            self.audio_channel_index = first_active;
        }

        let ch = self.audio_channel_index;
        let base = 0x40 + (ch as usize) * 8;

        let freq = self.sound_ram[base] as u32
            | ((self.sound_ram[base + 2] as u32) << 8)
            | (((self.sound_ram[base + 4] & 0x03) as u32) << 16);

        let phase = self.sound_ram[base + 1] as u32
            | ((self.sound_ram[base + 3] as u32) << 8)
            | ((self.sound_ram[base + 5] as u32) << 16);

        let wave_length = 256 - ((self.sound_ram[base + 4] & 0xFC) as u32);
        let wave_addr = self.sound_ram[base + 6] as u32;
        let volume = (self.sound_ram[base + 7] & 0x0F) as f32;

        let new_phase = phase.wrapping_add(freq);
        let masked_phase = if wave_length > 0 {
            new_phase % (wave_length << 16)
        } else {
            0
        };

        self.sound_ram[base + 1] = masked_phase as u8;
        self.sound_ram[base + 3] = (masked_phase >> 8) as u8;
        self.sound_ram[base + 5] = (masked_phase >> 16) as u8;

        let sample_index = (wave_addr + (masked_phase >> 16)) as usize;
        let byte_index = (sample_index / 2) % SOUND_RAM_SIZE;
        let sample = if sample_index & 1 != 0 {
            (self.sound_ram[byte_index] >> 4) & 0x0F
        } else {
            self.sound_ram[byte_index] & 0x0F
        };

        let output = (sample as f32 - 8.0) * volume;

        self.audio_channel_index += 1;
        if self.audio_channel_index >= 8 {
            self.audio_channel_index = first_active;
        }

        let scale = 1.0 / (num_active as f32 * 15.0 * 8.0);
        self.audio_output += (output * scale - self.audio_output) * 0.5;
    }

    fn wram_writable(&self) -> bool {
        self.wram_protect >= 0x40 && self.wram_protect <= 0x4F
    }
}

impl MemoryMapper for Namco163Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x4800..=0x4FFF => {
                let val = self.sound_ram[(self.sound_addr & 0x7F) as usize];
                if self.sound_auto_increment {
                    self.sound_addr = (self.sound_addr + 1) & 0x7F;
                }
                val
            }
            0x5000..=0x57FF => self.irq_counter as u8,
            0x5800..=0x5FFF => {
                ((self.irq_counter >> 8) as u8) | if self.irq_enabled { 0x80 } else { 0 }
            }
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize],
            0x8000..=0x9FFF => {
                let bank = self.prg_bank_index(self.prg_banks[0]);
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0x8000) as usize])
            }
            0xA000..=0xBFFF => {
                let bank = self.prg_bank_index(self.prg_banks[1]);
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0xA000) as usize])
            }
            0xC000..=0xDFFF => {
                let bank = self.prg_bank_index(self.prg_banks[2]);
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

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize] = value,
            0x4800..=0x4FFF => {
                self.sound_ram[(self.sound_addr & 0x7F) as usize] = value;
                if self.sound_auto_increment {
                    self.sound_addr = (self.sound_addr + 1) & 0x7F;
                }
            }
            0x5000..=0x57FF => {
                self.irq_counter = (self.irq_counter & 0xFF00) | value as u16;
                self.irq_pending = false;
            }
            0x5800..=0x5FFF => {
                self.irq_counter = (self.irq_counter & 0x00FF) | (((value & 0x7F) as u16) << 8);
                self.irq_enabled = value & 0x80 != 0;
                self.irq_pending = false;
            }
            0x6000..=0x7FFF => {
                if self.wram_writable() {
                    self.prg_ram[(addr - 0x6000) as usize] = value;
                }
            }
            0x8000..=0x87FF => self.chr_banks[0] = value,
            0x8800..=0x8FFF => self.chr_banks[1] = value,
            0x9000..=0x97FF => self.chr_banks[2] = value,
            0x9800..=0x9FFF => self.chr_banks[3] = value,
            0xA000..=0xA7FF => self.chr_banks[4] = value,
            0xA800..=0xAFFF => self.chr_banks[5] = value,
            0xB000..=0xB7FF => self.chr_banks[6] = value,
            0xB800..=0xBFFF => self.chr_banks[7] = value,
            0xC000..=0xC7FF => self.chr_banks[8] = value,
            0xC800..=0xCFFF => self.chr_banks[9] = value,
            0xD000..=0xD7FF => self.chr_banks[10] = value,
            0xD800..=0xDFFF => self.chr_banks[11] = value,
            0xE000..=0xE7FF => {
                self.prg_banks[0] = value & 0x3F;
                self.sound_disable = value & 0x40 != 0;
            }
            0xE800..=0xEFFF => {
                self.prg_banks[1] = value & 0x3F;
                self.chr_ram_enable_lo = value & 0x40 == 0;
                self.chr_ram_enable_hi = value & 0x80 == 0;
            }
            0xF000..=0xF7FF => {
                self.prg_banks[2] = value & 0x3F;
            }
            0xF800..=0xFFFF => {
                self.sound_addr = value & 0x7F;
                self.sound_auto_increment = value & 0x80 != 0;
                self.wram_protect = value;
            }
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.read_chr(addr),
            0x2000..=0x3EFF => self.read_nt(addr),
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
                let bank_val = self.chr_banks[slot];
                let is_upper_half = slot >= 4;

                let use_ram = self.has_chr_ram
                    && bank_val >= 0xE0
                    && if is_upper_half {
                        self.chr_ram_enable_hi
                    } else {
                        self.chr_ram_enable_lo
                    };

                if use_ram {
                    let ram_base =
                        (bank_val & 0x1F) as usize * CHR_BANK_SIZE + (addr as usize & 0x3FF);
                    let total = self.chr_rom.len() * CHR_BANK_SIZE;
                    let idx = ram_base % total;
                    let bank_i = idx / CHR_BANK_SIZE;
                    let offset = idx % CHR_BANK_SIZE;
                    if let Some(b) = self.chr_rom.get(bank_i) {
                        let copy_size = size.min(CHR_BANK_SIZE - offset);
                        unsafe { std::ptr::copy(b.as_ptr().add(offset), dest, copy_size) }
                    }
                } else {
                    let bank = self.chr_1k_index(bank_val);
                    if let Some(b) = self.chr_rom.get(bank) {
                        let offset = addr as usize & 0x3FF;
                        let copy_size = size.min(CHR_BANK_SIZE - offset);
                        unsafe { std::ptr::copy(b.as_ptr().add(offset), dest, copy_size) }
                    }
                }
            }
            0x2000..=0x3EFF => {
                let slot = ((addr >> 10) & 3) as usize;
                let bank_val = self.chr_banks[8 + slot];
                let offset = addr as usize & 0x3FF;

                if bank_val >= 0xE0 {
                    let page = (bank_val & 1) as usize;
                    let vram_addr = page * 0x400 + offset;
                    let copy_size = size.min(VRAM_SIZE as usize - vram_addr);
                    unsafe { std::ptr::copy(self.vram.as_ptr().add(vram_addr), dest, copy_size) }
                } else {
                    let bank = self.chr_1k_index(bank_val);
                    if let Some(b) = self.chr_rom.get(bank) {
                        let copy_size = size.min(CHR_BANK_SIZE - offset);
                        unsafe { std::ptr::copy(b.as_ptr().add(offset), dest, copy_size) }
                    }
                }
            }
            _ => {}
        }
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.write_chr(addr, value),
            0x2000..=0x3EFF => self.write_nt(addr, value),
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

    fn cpu_cycle(&mut self, _ppu_dot: u64) {
        if self.irq_enabled && self.irq_counter < 0x7FFF {
            self.irq_counter += 1;
            if self.irq_counter >= 0x7FFF {
                self.irq_pending = true;
            }
        }

        self.clock_audio();
    }

    fn audio_expansion_output(&self) -> f32 {
        self.audio_output
    }

    fn sram_data(&self) -> Option<&[u8]> {
        if self.has_battery {
            Some(&*self.prg_ram)
        } else {
            None
        }
    }

    fn sram_data_mut(&mut self) -> Option<&mut [u8]> {
        if self.has_battery {
            Some(&mut *self.prg_ram)
        } else {
            None
        }
    }

    fn mapper_id(&self) -> u8 {
        19
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        super::save_controllers(w, &self.controllers);
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.vram);
        w.write_bytes(&self.palette_ram);
        w.write_bytes(&*self.prg_ram);
        w.write_bytes(&self.sound_ram);

        for bank in &self.chr_rom {
            w.write_bytes(bank);
        }

        for &b in &self.chr_banks {
            w.write_u8(b);
        }
        for &b in &self.prg_banks {
            w.write_u8(b);
        }

        w.write_bool(self.chr_ram_enable_lo);
        w.write_bool(self.chr_ram_enable_hi);

        w.write_u16(self.irq_counter);
        w.write_bool(self.irq_enabled);
        w.write_bool(self.irq_pending);

        w.write_u8(self.sound_addr);
        w.write_bool(self.sound_auto_increment);
        w.write_bool(self.sound_disable);
        w.write_u8(self.wram_protect);

        w.write_f32(self.audio_output);
        w.write_u8(self.audio_channel_index);
        w.write_u8(self.audio_timer);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        super::load_controllers(r, &mut self.controllers)?;
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut *self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        r.read_bytes_into(&mut *self.prg_ram)?;
        r.read_bytes_into(&mut self.sound_ram)?;

        for bank in &mut self.chr_rom {
            r.read_bytes_into(bank)?;
        }

        for b in &mut self.chr_banks {
            *b = r.read_u8()?;
        }
        for b in &mut self.prg_banks {
            *b = r.read_u8()?;
        }

        self.chr_ram_enable_lo = r.read_bool()?;
        self.chr_ram_enable_hi = r.read_bool()?;

        self.irq_counter = r.read_u16()?;
        self.irq_enabled = r.read_bool()?;
        self.irq_pending = r.read_bool()?;

        self.sound_addr = r.read_u8()?;
        self.sound_auto_increment = r.read_bool()?;
        self.sound_disable = r.read_bool()?;
        self.wram_protect = r.read_u8()?;

        self.audio_output = r.read_f32()?;
        self.audio_channel_index = r.read_u8()?;
        self.audio_timer = r.read_u8()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper(num_prg_16k: usize, num_chr_8k: usize) -> Box<dyn MemoryMapper> {
        let mut prg = Vec::new();
        for i in 0..num_prg_16k {
            let mut bank = [0u8; 16384];
            bank[0] = (i * 2) as u8;
            bank[PRG_BANK_SIZE] = (i * 2 + 1) as u8;
            prg.push(bank);
        }

        let mut chr = Vec::new();
        for i in 0..num_chr_8k {
            let mut bank = [0u8; 8192];
            for k in 0..8 {
                bank[k * CHR_BANK_SIZE] = (i * 8 + k) as u8;
            }
            chr.push(bank);
        }

        Box::new(Namco163Mapper::new(0, prg, chr, false, None, 0))
    }

    fn make_chr_ram_mapper(num_prg_16k: usize) -> Box<dyn MemoryMapper> {
        let mut prg = Vec::new();
        for i in 0..num_prg_16k {
            let mut bank = [0u8; 16384];
            bank[0] = (i * 2) as u8;
            prg.push(bank);
        }
        Box::new(Namco163Mapper::new(0, prg, vec![], false, None, 0))
    }

    #[test]
    fn test_prg_banking() {
        let mut m = make_mapper(8, 1);

        assert_eq!(m.cpu_read(0x8000), 0);
        assert_eq!(m.cpu_read(0xA000), 1);
        assert_eq!(m.cpu_read(0xC000), 2);
        assert_eq!(m.cpu_read(0xE000), (8 * 2 - 1));

        m.cpu_write(0xE000, 5);
        assert_eq!(m.cpu_read(0x8000), 5);

        m.cpu_write(0xE800, 7);
        assert_eq!(m.cpu_read(0xA000), 7);

        m.cpu_write(0xF000, 10);
        assert_eq!(m.cpu_read(0xC000), 10);
    }

    #[test]
    fn test_chr_banking() {
        let mut m = make_mapper(2, 4);

        m.cpu_write(0x8000, 10);
        assert_eq!(m.ppu_read(0x0000), 10);

        m.cpu_write(0x8800, 20);
        assert_eq!(m.ppu_read(0x0400), 20);

        m.cpu_write(0xB800, 30);
        assert_eq!(m.ppu_read(0x1C00), 30);
    }

    #[test]
    fn test_nametable_ciram() {
        let mut m = make_mapper(2, 1);

        m.cpu_write(0xC000, 0xE0);
        m.cpu_write(0xC800, 0xE1);
        m.cpu_write(0xD000, 0xE0);
        m.cpu_write(0xD800, 0xE1);

        m.ppu_write(0x2000, 0xAA);
        assert_eq!(m.ppu_read(0x2000), 0xAA);
        assert_eq!(m.ppu_read(0x2800), 0xAA);

        m.ppu_write(0x2400, 0xBB);
        assert_eq!(m.ppu_read(0x2400), 0xBB);
        assert_eq!(m.ppu_read(0x2C00), 0xBB);
    }

    #[test]
    fn test_nametable_chr_rom() {
        let mut m = make_mapper(2, 4);

        m.cpu_write(0xC000, 5);
        let val = m.ppu_read(0x2000);
        assert_eq!(val, 5);
    }

    #[test]
    fn test_irq_counter() {
        let mut m = make_mapper(2, 1);

        m.cpu_write(0x5000, 0xFD);
        m.cpu_write(0x5800, 0xFF);
        assert!(!m.poll_irq());

        m.cpu_cycle(0);
        assert!(!m.poll_irq());

        m.cpu_cycle(0);
        assert!(m.poll_irq());

        m.cpu_write(0x5000, 0);
        assert!(!m.poll_irq());
    }

    #[test]
    fn test_sound_ram_readwrite() {
        let mut m = make_mapper(2, 1);

        m.cpu_write(0xF800, 0x80 | 0x10);
        m.cpu_write(0x4800, 0xAB);
        m.cpu_write(0x4800, 0xCD);

        m.cpu_write(0xF800, 0x10);
        let val = m.cpu_read(0x4800);
        assert_eq!(val, 0xAB);

        m.cpu_write(0xF800, 0x80 | 0x10);
        let val = m.cpu_read(0x4800);
        assert_eq!(val, 0xAB);
        let val = m.cpu_read(0x4800);
        assert_eq!(val, 0xCD);
    }

    #[test]
    fn test_wram_protect() {
        let mut m = make_mapper(2, 1);

        m.cpu_write(0xF800, 0x00);
        m.cpu_write(0x6000, 0xAA);
        assert_eq!(m.cpu_read(0x6000), 0);

        m.cpu_write(0xF800, 0x40);
        m.cpu_write(0x6000, 0xAA);
        assert_eq!(m.cpu_read(0x6000), 0xAA);

        m.cpu_write(0xF800, 0x50);
        m.cpu_write(0x6000, 0xBB);
        assert_eq!(m.cpu_read(0x6000), 0xAA);
    }

    #[test]
    fn test_chr_ram_enable() {
        let mut m = make_chr_ram_mapper(2);

        m.cpu_write(0xE800, 0x00);
        m.cpu_write(0x8000, 0xE0);

        m.ppu_write(0x0000, 0x42);
        assert_eq!(m.ppu_read(0x0000), 0x42);
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut m = make_mapper(4, 2);
        m.cpu_write(0xE000, 3);
        m.cpu_write(0x8000, 10);
        m.cpu_write(0xC000, 0xE0);
        m.ppu_write(0x2000, 0xAB);

        m.cpu_write(0xF800, 0x80);
        m.cpu_write(0x4800, 0xDE);

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);
        let data = w.finish();

        let mut m2 = make_mapper(4, 2);
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.cpu_read(0x8000), m.cpu_read(0x8000));
        assert_eq!(m2.ppu_read(0x0000), m.ppu_read(0x0000));
        assert_eq!(m2.ppu_read(0x2000), m.ppu_read(0x2000));

        m2.cpu_write(0xF800, 0x00);
        let val = m2.cpu_read(0x4800);
        assert_eq!(val, 0xDE);
    }
}
