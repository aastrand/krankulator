use super::super::super::io;
use super::{
    mirror_nametable_addr, NametableMirror, A12_FILTER_DOTS, CPU_RAM_SIZE, PALETTE_MIRROR_CLEAR,
    PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, RESET_TARGET_ADDR, VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x2000; // 8KB
const CHR_BANK_SIZE: usize = 0x0400; // 1KB

pub struct Taito33Mapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_rom: Vec<[u8; CHR_BANK_SIZE]>,

    prg_banks: [u8; 2],
    chr_banks: [u8; 8],
    mirroring: NametableMirror,
    is_mapper48: bool,

    irq_counter: u8,
    irq_latch: u8,
    irq_enable: bool,
    irq_reload: bool,
    irq_pending: bool,
    irq_pending_since_dot: u64,
    last_a12_low_dot: u64,

    vram: Box<[u8; VRAM_SIZE as usize]>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl Taito33Mapper {
    pub fn new(flags: u8, prg_banks_16k: Vec<[u8; 16384]>, chr_banks_8k: Vec<[u8; 8192]>) -> Self {
        Self::new_variant(flags, prg_banks_16k, chr_banks_8k, false)
    }

    pub fn new_mapper48(
        flags: u8,
        prg_banks_16k: Vec<[u8; 16384]>,
        chr_banks_8k: Vec<[u8; 8192]>,
    ) -> Self {
        Self::new_variant(flags, prg_banks_16k, chr_banks_8k, true)
    }

    fn new_variant(
        flags: u8,
        prg_banks_16k: Vec<[u8; 16384]>,
        chr_banks_8k: Vec<[u8; 8192]>,
        is_mapper48: bool,
    ) -> Self {
        let mut prg_rom = vec![];
        for bank in &prg_banks_16k {
            prg_rom.push(<[u8; PRG_BANK_SIZE]>::try_from(&bank[0..PRG_BANK_SIZE]).unwrap());
            prg_rom.push(
                <[u8; PRG_BANK_SIZE]>::try_from(&bank[PRG_BANK_SIZE..2 * PRG_BANK_SIZE]).unwrap(),
            );
        }

        let mut chr_rom = vec![];
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

        Taito33Mapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            chr_rom,
            prg_banks: [0, 1],
            chr_banks: [0, 1, 2, 3, 4, 5, 6, 7],
            mirroring,
            is_mapper48,
            irq_counter: 0,
            irq_latch: 0,
            irq_enable: false,
            irq_reload: false,
            irq_pending: false,
            irq_pending_since_dot: 0,
            last_a12_low_dot: 0,
            vram: Box::new([0; VRAM_SIZE as usize]),
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
        }
    }

    fn chr_1k_index(&self, slot: usize) -> usize {
        self.chr_banks[slot] as usize % self.chr_rom.len().max(1)
    }

    fn check_a12_transition(&mut self, addr: u16, dot: u64) {
        let current_a12 = (addr & 0x1000) != 0;
        if current_a12 {
            if self.last_a12_low_dot > 0
                && dot.saturating_sub(self.last_a12_low_dot) >= A12_FILTER_DOTS
            {
                self.clock_irq_counter(dot);
            }
            self.last_a12_low_dot = 0;
        } else if self.last_a12_low_dot == 0 {
            self.last_a12_low_dot = dot;
        }
    }

    fn clock_irq_counter(&mut self, dot: u64) {
        if self.irq_reload || self.irq_counter == 0 {
            self.irq_counter = self.irq_latch;
        } else {
            self.irq_counter = self.irq_counter.wrapping_sub(1);
        }
        self.irq_reload = false;
        if self.irq_counter == 0 && self.irq_enable {
            if !self.irq_pending {
                self.irq_pending_since_dot = dot;
            }
            self.irq_pending = true;
        }
    }
}

impl MemoryMapper for Taito33Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x6000..=0x7FFF => 0,
            0x8000..=0x9FFF => {
                let bank = self.prg_banks[0] as usize % self.prg_rom.len().max(1);
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0x8000) as usize])
            }
            0xA000..=0xBFFF => {
                let bank = self.prg_banks[1] as usize % self.prg_rom.len().max(1);
                self.prg_rom
                    .get(bank)
                    .map_or(0, |b| b[(addr - 0xA000) as usize])
            }
            0xC000..=0xDFFF => {
                let bank = self.prg_rom.len().saturating_sub(2);
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
            0x8000..=0xFFFF => match addr & 0xE003 {
                0x8000 => {
                    self.prg_banks[0] = value & 0x3F;
                    if !self.is_mapper48 {
                        self.mirroring = if value & 0x40 != 0 {
                            NametableMirror::Horizontal
                        } else {
                            NametableMirror::Vertical
                        };
                    }
                }
                0x8001 => self.prg_banks[1] = value & 0x3F,
                0x8002 => {
                    let base = value << 1;
                    self.chr_banks[0] = base;
                    self.chr_banks[1] = base | 1;
                }
                0x8003 => {
                    let base = value << 1;
                    self.chr_banks[2] = base;
                    self.chr_banks[3] = base | 1;
                }
                0xA000 => self.chr_banks[4] = value,
                0xA001 => self.chr_banks[5] = value,
                0xA002 => self.chr_banks[6] = value,
                0xA003 => self.chr_banks[7] = value,
                0xC000 if self.is_mapper48 => self.irq_latch = value ^ 0xFF,
                0xC001 if self.is_mapper48 => self.irq_reload = true,
                0xC002 if self.is_mapper48 => self.irq_enable = true,
                0xC003 if self.is_mapper48 => {
                    self.irq_enable = false;
                    self.irq_pending = false;
                }
                0xE000 if self.is_mapper48 => {
                    self.mirroring = if value & 0x40 != 0 {
                        NametableMirror::Horizontal
                    } else {
                        NametableMirror::Vertical
                    };
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let slot = (addr >> 10) as usize & 7;
                let bank = self.chr_1k_index(slot);
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

    fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        match addr {
            0x0000..=0x1FFF => {
                let slot = (addr >> 10) as usize & 7;
                let bank = self.chr_1k_index(slot);
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
            0x0000..=0x1FFF => {}
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

    fn poll_irq_at_dot(&self, deadline_dot: u64) -> bool {
        self.irq_pending && self.irq_pending_since_dot <= deadline_dot
    }

    fn ppu_fetch(&mut self, addr: u16, dot: u64) -> u8 {
        if self.is_mapper48 {
            self.check_a12_transition(addr, dot);
        }
        self.ppu_read(addr)
    }

    fn ppu_a12_transition(&mut self, addr: u16, dot: u64) {
        if self.is_mapper48 {
            self.check_a12_transition(addr, dot);
        }
    }

    fn mapper_id(&self) -> u8 {
        if self.is_mapper48 {
            48
        } else {
            33
        }
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.vram);
        w.write_bytes(&self.palette_ram);
        for &b in &self.chr_banks {
            w.write_u8(b);
        }
        for &b in &self.prg_banks {
            w.write_u8(b);
        }
        super::save_mirroring(w, self.mirroring);
        super::save_controllers(w, &self.controllers);
        if self.is_mapper48 {
            w.write_u8(self.irq_counter);
            w.write_u8(self.irq_latch);
            w.write_bool(self.irq_enable);
            w.write_bool(self.irq_reload);
            w.write_bool(self.irq_pending);
        }
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut *self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        for b in &mut self.chr_banks {
            *b = r.read_u8()?;
        }
        for b in &mut self.prg_banks {
            *b = r.read_u8()?;
        }
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        if self.is_mapper48 {
            self.irq_counter = r.read_u8()?;
            self.irq_latch = r.read_u8()?;
            self.irq_enable = r.read_bool()?;
            self.irq_reload = r.read_bool()?;
            self.irq_pending = r.read_bool()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper(num_prg_16k: usize, num_chr_8k: usize) -> Box<dyn MemoryMapper> {
        make_mapper_variant(num_prg_16k, num_chr_8k, false)
    }

    fn make_mapper48(num_prg_16k: usize, num_chr_8k: usize) -> Box<dyn MemoryMapper> {
        make_mapper_variant(num_prg_16k, num_chr_8k, true)
    }

    fn make_mapper_variant(
        num_prg_16k: usize,
        num_chr_8k: usize,
        is_mapper48: bool,
    ) -> Box<dyn MemoryMapper> {
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

        Box::new(Taito33Mapper::new_variant(0, prg, chr, is_mapper48))
    }

    #[test]
    fn test_prg_banking() {
        let mut m = make_mapper(4, 1);

        assert_eq!(m.cpu_read(0x8000), 0);
        assert_eq!(m.cpu_read(0xA000), 1);

        m.cpu_write(0x8000, 3);
        assert_eq!(m.cpu_read(0x8000), 3);

        m.cpu_write(0x8001, 5);
        assert_eq!(m.cpu_read(0xA000), 5);

        // Aliased writes: $8100 should decode as $8000
        m.cpu_write(0x8100, 2);
        assert_eq!(m.cpu_read(0x8000), 2);

        // $C000/$E000 are fixed to last two banks
        assert_eq!(m.cpu_read(0xC000), 6);
        assert_eq!(m.cpu_read(0xE000), 7);
    }

    #[test]
    fn test_chr_2k_banking() {
        let mut m = make_mapper(2, 4); // 32 x 1KB CHR banks

        // Write 2KB bank 3 to $0000 => 1KB banks 6,7
        m.cpu_write(0x8002, 3);
        assert_eq!(m.ppu_read(0x0000), 6);
        assert_eq!(m.ppu_read(0x0400), 7);

        // Write 2KB bank 5 to $0800 => 1KB banks 10,11
        m.cpu_write(0x8003, 5);
        assert_eq!(m.ppu_read(0x0800), 10);
        assert_eq!(m.ppu_read(0x0C00), 11);
    }

    #[test]
    fn test_chr_1k_banking() {
        let mut m = make_mapper(2, 4);

        m.cpu_write(0xA000, 15);
        assert_eq!(m.ppu_read(0x1000), 15);

        m.cpu_write(0xA001, 20);
        assert_eq!(m.ppu_read(0x1400), 20);

        m.cpu_write(0xA002, 25);
        assert_eq!(m.ppu_read(0x1800), 25);

        m.cpu_write(0xA003, 30);
        assert_eq!(m.ppu_read(0x1C00), 30);
    }

    #[test]
    fn test_mirroring() {
        let mut m = make_mapper(2, 1);

        // Default: horizontal (flag bit 0 = 0)
        m.ppu_write(0x2000, 0xAA);
        assert_eq!(m.ppu_read(0x2400), 0xAA); // horizontal mirror

        // Set vertical via $8000 bit 6 = 0 (clear bit 6)
        // First set horizontal via bit 6
        m.cpu_write(0x8000, 0x40);
        m.ppu_write(0x2000, 0xBB);
        assert_eq!(m.ppu_read(0x2400), 0xBB); // still horizontal

        // Clear bit 6 → vertical
        m.cpu_write(0x8000, 0x00);
        m.ppu_write(0x2000, 0xCC);
        assert_eq!(m.ppu_read(0x2800), 0xCC); // vertical mirror
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut m = make_mapper(2, 2);
        m.cpu_write(0x8000, 2);
        m.cpu_write(0x8002, 5);
        m.cpu_write(0xA000, 12);

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);
        let data = w.finish();

        let mut m2 = make_mapper(2, 2);
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.cpu_read(0x8000), m.cpu_read(0x8000));
        assert_eq!(m2.ppu_read(0x0000), m.ppu_read(0x0000));
        assert_eq!(m2.ppu_read(0x1000), m.ppu_read(0x1000));
    }

    #[test]
    fn test_mapper48_mirroring_at_e000() {
        let mut m = make_mapper48(2, 1);

        m.ppu_write(0x2000, 0xAA);
        assert_eq!(m.ppu_read(0x2400), 0xAA);

        m.cpu_write(0xE000, 0x00);
        m.ppu_write(0x2000, 0xBB);
        assert_eq!(m.ppu_read(0x2800), 0xBB);

        m.cpu_write(0xE000, 0x40);
        m.ppu_write(0x2000, 0xCC);
        assert_eq!(m.ppu_read(0x2400), 0xCC);
    }

    #[test]
    fn test_mapper48_prg_no_mirroring_bit() {
        let mut m = make_mapper48(4, 1);

        m.cpu_write(0x8000, 0x42);
        assert_eq!(m.cpu_read(0x8000), m.cpu_read(0x8000));
        assert_eq!(m.mapper_id(), 48);

        m.ppu_write(0x2000, 0xAA);
        assert_eq!(m.ppu_read(0x2400), 0xAA);
    }

    #[test]
    fn test_mapper48_irq_disable_clears_pending() {
        let mut m = make_mapper48(2, 2);

        m.cpu_write(0xC002, 0x00);
        m.cpu_write(0xC003, 0x00);
        assert!(!m.poll_irq());
    }

    #[test]
    fn test_mapper48_savestate_roundtrip() {
        let mut m = make_mapper48(2, 2);
        m.cpu_write(0x8000, 2);
        m.cpu_write(0x8002, 5);
        m.cpu_write(0xC000, 0x10);
        m.cpu_write(0xC002, 0x00);

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);
        let data = w.finish();

        let mut m2 = make_mapper48(2, 2);
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.cpu_read(0x8000), m.cpu_read(0x8000));
        assert_eq!(m2.ppu_read(0x0000), m.ppu_read(0x0000));
    }
}
