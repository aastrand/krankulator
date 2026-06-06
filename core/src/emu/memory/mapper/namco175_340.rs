use super::super::super::io;
use super::{
    mirror_nametable_addr, NametableMirror, CPU_RAM_SIZE, PALETTE_MIRROR_CLEAR,
    PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, PRG_RAM_8K, RESET_TARGET_ADDR, VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x2000; // 8KB
const CHR_BANK_SIZE: usize = 0x0400; // 1KB

pub struct Namco175_340Mapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_rom: Vec<[u8; CHR_BANK_SIZE]>,

    prg_banks: [u8; 3],
    chr_banks: [u8; 8],
    mirroring: NametableMirror,

    is_namco340: bool,
    prg_ram: Box<[u8; PRG_RAM_8K]>,
    prg_ram_enabled: bool,

    vram: Box<[u8; VRAM_SIZE as usize]>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl Namco175_340Mapper {
    pub fn new(
        flags: u8,
        prg_banks_16k: Vec<[u8; 16384]>,
        chr_banks_8k: Vec<[u8; 8192]>,
        submapper: u8,
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

        let is_namco340 = submapper == 2;

        let mirroring = if flags & 1 != 0 {
            NametableMirror::Vertical
        } else {
            NametableMirror::Horizontal
        };

        Namco175_340Mapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            chr_rom,
            prg_banks: [0, 1, 2],
            chr_banks: [0, 1, 2, 3, 4, 5, 6, 7],
            mirroring,
            is_namco340,
            prg_ram: Box::new([0; PRG_RAM_8K]),
            prg_ram_enabled: !is_namco340,
            vram: Box::new([0; VRAM_SIZE as usize]),
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
        }
    }

    fn chr_1k_index(&self, slot: usize) -> usize {
        self.chr_banks[slot] as usize % self.chr_rom.len().max(1)
    }

    fn prg_bank_index(&self, bank: u8) -> usize {
        (bank & 0x3F) as usize % self.prg_rom.len().max(1)
    }
}

impl MemoryMapper for Namco175_340Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x6000..=0x7FFF => {
                if self.prg_ram_enabled {
                    self.prg_ram[(addr - 0x6000) as usize]
                } else {
                    0
                }
            }
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
            0x6000..=0x7FFF => {
                if self.prg_ram_enabled {
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
            0xC000..=0xC7FF => {
                if !self.is_namco340 {
                    self.prg_ram_enabled = value & 1 != 0;
                }
            }
            0xE000..=0xE7FF => {
                self.prg_banks[0] = value & 0x3F;
                if self.is_namco340 {
                    self.mirroring = match (value >> 6) & 3 {
                        0 => NametableMirror::Lower,
                        1 => NametableMirror::Vertical,
                        2 => NametableMirror::Higher,
                        _ => NametableMirror::Horizontal,
                    };
                }
            }
            0xE800..=0xEFFF => self.prg_banks[1] = value & 0x3F,
            0xF000..=0xF7FF => self.prg_banks[2] = value & 0x3F,
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
        false
    }

    fn mapper_id(&self) -> u8 {
        210
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.vram);
        w.write_bytes(&self.palette_ram);
        w.write_bytes(&*self.prg_ram);
        for &b in &self.chr_banks {
            w.write_u8(b);
        }
        for &b in &self.prg_banks {
            w.write_u8(b);
        }
        w.write_bool(self.prg_ram_enabled);
        super::save_mirroring(w, self.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut *self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        r.read_bytes_into(&mut *self.prg_ram)?;
        for b in &mut self.chr_banks {
            *b = r.read_u8()?;
        }
        for b in &mut self.prg_banks {
            *b = r.read_u8()?;
        }
        self.prg_ram_enabled = r.read_bool()?;
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper(num_prg_16k: usize, num_chr_8k: usize, submapper: u8) -> Box<dyn MemoryMapper> {
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

        Box::new(Namco175_340Mapper::new(0, prg, chr, submapper))
    }

    #[test]
    fn test_prg_banking() {
        let mut m = make_mapper(8, 1, 1);

        assert_eq!(m.cpu_read(0x8000), 0);
        assert_eq!(m.cpu_read(0xA000), 1);
        assert_eq!(m.cpu_read(0xC000), 2);

        m.cpu_write(0xE000, 5);
        assert_eq!(m.cpu_read(0x8000), 5);

        m.cpu_write(0xE800, 7);
        assert_eq!(m.cpu_read(0xA000), 7);

        m.cpu_write(0xF000, 10);
        assert_eq!(m.cpu_read(0xC000), 10);

        assert_eq!(m.cpu_read(0xE000), (8 * 2 - 1));
    }

    #[test]
    fn test_chr_banking() {
        let mut m = make_mapper(2, 4, 1);

        m.cpu_write(0x8000, 10);
        assert_eq!(m.ppu_read(0x0000), 10);

        m.cpu_write(0x8800, 20);
        assert_eq!(m.ppu_read(0x0400), 20);

        m.cpu_write(0x9000, 5);
        assert_eq!(m.ppu_read(0x0800), 5);

        m.cpu_write(0xB800, 30);
        assert_eq!(m.ppu_read(0x1C00), 30);
    }

    #[test]
    fn test_namco175_prg_ram() {
        let mut m = make_mapper(2, 1, 1);

        m.cpu_write(0xC000, 0x01);
        m.cpu_write(0x6000, 0xAB);
        assert_eq!(m.cpu_read(0x6000), 0xAB);

        m.cpu_write(0xC000, 0x00);
        assert_eq!(m.cpu_read(0x6000), 0x00);
    }

    #[test]
    fn test_namco340_no_prg_ram() {
        let mut m = make_mapper(2, 1, 2);

        m.cpu_write(0x6000, 0xAB);
        assert_eq!(m.cpu_read(0x6000), 0x00);
    }

    #[test]
    fn test_namco340_mirroring() {
        let mut m = make_mapper(4, 1, 2);

        m.cpu_write(0xE000, 0x40);
        m.ppu_write(0x2000, 0xAA);
        assert_eq!(m.ppu_read(0x2800), 0xAA);

        m.cpu_write(0xE000, 0xC0);
        m.ppu_write(0x2000, 0xBB);
        assert_eq!(m.ppu_read(0x2400), 0xBB);
    }

    #[test]
    fn test_namco175_hardwired_mirroring() {
        let mut m = make_mapper(4, 1, 1);

        m.ppu_write(0x2000, 0xAA);
        assert_eq!(m.ppu_read(0x2400), 0xAA);

        m.cpu_write(0xE000, 0x40);
        m.ppu_write(0x2800, 0xBB);
        assert_eq!(m.ppu_read(0x2000), 0xAA);
        assert_eq!(m.ppu_read(0x2400), 0xAA);
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut m = make_mapper(4, 2, 2);
        m.cpu_write(0xE000, 0xC3);
        m.cpu_write(0x8000, 10);
        m.cpu_write(0xA800, 15);

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);
        let data = w.finish();

        let mut m2 = make_mapper(4, 2, 2);
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.cpu_read(0x8000), m.cpu_read(0x8000));
        assert_eq!(m2.ppu_read(0x0000), m.ppu_read(0x0000));
        assert_eq!(m2.ppu_read(0x1400), m.ppu_read(0x1400));
    }
}
