use super::super::super::io;
use super::{
    mirror_nametable_addr, NametableMirror, CPU_RAM_SIZE, PALETTE_MIRROR_CLEAR,
    PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, RESET_TARGET_ADDR, VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x2000; // 8KB
const CHR_BANK_SIZE: usize = 0x0400; // 1KB

pub struct Namco108Mapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_rom: Vec<[u8; CHR_BANK_SIZE]>,

    bank_select: u8,
    chr_banks: [u8; 6],
    prg_banks: [u8; 2],
    mirroring: NametableMirror,
    is_mapper88: bool,

    vram: Box<[u8; VRAM_SIZE as usize]>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl Namco108Mapper {
    pub fn new(
        flags: u8,
        prg_banks_16k: Vec<[u8; 16384]>,
        chr_banks_8k: Vec<[u8; 8192]>,
        is_mapper88: bool,
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

        Namco108Mapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            chr_rom,
            bank_select: 0,
            chr_banks: if is_mapper88 {
                [0, 2, 0x44, 0x45, 0x46, 0x47]
            } else {
                [0, 2, 4, 5, 6, 7]
            },
            prg_banks: [0, 1],
            mirroring,
            is_mapper88,
            vram: Box::new([0; VRAM_SIZE as usize]),
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
        }
    }

    fn chr_bank_index(&self, slot: usize) -> usize {
        self.chr_banks[slot] as usize % self.chr_rom.len().max(1)
    }
}

impl MemoryMapper for Namco108Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x6000..=0x7FFF => 0, // no PRG RAM
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
            0x8000..=0x9FFF => {
                if addr & 1 == 0 {
                    self.bank_select = value & 0x07;
                } else {
                    let reg = self.bank_select as usize;
                    let v = if self.is_mapper88 {
                        if reg < 2 {
                            value & 0x3F
                        } else {
                            (value & 0x3F) | 0x40
                        }
                    } else {
                        value
                    };
                    match reg {
                        0 => self.chr_banks[0] = if self.is_mapper88 { v & 0xFE } else { v & 0x3E },
                        1 => self.chr_banks[1] = if self.is_mapper88 { v & 0xFE } else { v & 0x3E },
                        2 => self.chr_banks[2] = if self.is_mapper88 { v } else { v & 0x3F },
                        3 => self.chr_banks[3] = if self.is_mapper88 { v } else { v & 0x3F },
                        4 => self.chr_banks[4] = if self.is_mapper88 { v } else { v & 0x3F },
                        5 => self.chr_banks[5] = if self.is_mapper88 { v } else { v & 0x3F },
                        6 => self.prg_banks[0] = value & 0x0F,
                        7 => self.prg_banks[1] = value & 0x0F,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x07FF => {
                let bank = self.chr_bank_index(0);
                let sub_bank = if addr < 0x0400 { bank } else { bank + 1 };
                let idx = sub_bank % self.chr_rom.len().max(1);
                self.chr_rom
                    .get(idx)
                    .map_or(0, |b| b[addr as usize & 0x3FF])
            }
            0x0800..=0x0FFF => {
                let bank = self.chr_bank_index(1);
                let within_2k = addr as usize & 0x3FF;
                let sub_bank = if addr < 0x0C00 { bank } else { bank + 1 };
                let idx = sub_bank % self.chr_rom.len().max(1);
                self.chr_rom.get(idx).map_or(0, |b| b[within_2k])
            }
            0x1000..=0x13FF => {
                let bank = self.chr_bank_index(2);
                self.chr_rom
                    .get(bank)
                    .map_or(0, |b| b[addr as usize & 0x3FF])
            }
            0x1400..=0x17FF => {
                let bank = self.chr_bank_index(3);
                self.chr_rom
                    .get(bank)
                    .map_or(0, |b| b[addr as usize & 0x3FF])
            }
            0x1800..=0x1BFF => {
                let bank = self.chr_bank_index(4);
                self.chr_rom
                    .get(bank)
                    .map_or(0, |b| b[addr as usize & 0x3FF])
            }
            0x1C00..=0x1FFF => {
                let bank = self.chr_bank_index(5);
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
                let slot = (addr as usize) >> 10;
                let bank_idx = match slot {
                    0 => self.chr_bank_index(0),
                    1 => (self.chr_bank_index(0) + 1) % self.chr_rom.len().max(1),
                    2 => self.chr_bank_index(1),
                    3 => (self.chr_bank_index(1) + 1) % self.chr_rom.len().max(1),
                    4 => self.chr_bank_index(2),
                    5 => self.chr_bank_index(3),
                    6 => self.chr_bank_index(4),
                    7 => self.chr_bank_index(5),
                    _ => 0,
                };
                if let Some(b) = self.chr_rom.get(bank_idx) {
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
            0x0000..=0x1FFF => {} // CHR ROM, not writable
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
        if self.is_mapper88 {
            88
        } else {
            206
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
        w.write_u8(self.bank_select);
        super::save_mirroring(w, self.mirroring);
        super::save_controllers(w, &self.controllers);
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
        self.bank_select = r.read_u8()?;
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper(
        num_prg_16k: usize,
        num_chr_8k: usize,
        is_mapper88: bool,
    ) -> Box<dyn MemoryMapper> {
        let mut prg = Vec::new();
        for i in 0..num_prg_16k {
            let mut bank = [0u8; 16384];
            bank[0] = i as u8;
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

        Box::new(Namco108Mapper::new(0, prg, chr, is_mapper88))
    }

    #[test]
    fn test_prg_banking() {
        let mut m = make_mapper(4, 1, false); // 8 x 8KB PRG banks

        // Default: bank 0 at $8000, bank 1 at $A000, last-2 at $C000, last-1 at $E000
        assert_eq!(m.cpu_read(0x8000), 0); // 8KB bank 0: first half of 16KB bank 0
        assert_eq!(m.cpu_read(0xA000), 1); // 8KB bank 1: second half of 16KB bank 0

        // Select register 6, write PRG bank 3
        m.cpu_write(0x8000, 6);
        m.cpu_write(0x8001, 3);
        // 8KB bank 3 = second half of 16KB bank 1, first byte = (1*2+1) = 3
        assert_eq!(m.cpu_read(0x8000), 3);
    }

    #[test]
    fn test_chr_2k_banks() {
        let mut m = make_mapper(2, 4, false); // 32 x 1KB CHR banks

        // Default: CHR reg 0 = 0 (2KB at $0000), reg 1 = 2 (2KB at $0800)
        // reg 2 = 4 (1KB at $1000), reg 3 = 5 (1KB at $1400)
        // reg 4 = 6 (1KB at $1800), reg 5 = 7 (1KB at $1C00)

        // Select register 0, write CHR bank 4 (2KB)
        m.cpu_write(0x8000, 0);
        m.cpu_write(0x8001, 4);
        assert_eq!(m.ppu_read(0x0000), 4); // 1KB bank 4
        assert_eq!(m.ppu_read(0x0400), 5); // 1KB bank 5

        // Select register 2, write CHR bank 10 (1KB)
        m.cpu_write(0x8000, 2);
        m.cpu_write(0x8001, 10);
        assert_eq!(m.ppu_read(0x1000), 10);
    }

    #[test]
    fn test_mapper88_chr_offset() {
        let mut m = make_mapper(2, 4, true);

        // Mapper 88: 1KB regs (2-5) get bit 6 forced on, effectively offsetting into upper CHR
        m.cpu_write(0x8000, 2);
        m.cpu_write(0x8001, 2); // becomes 2 | 0x40 = 0x42
        let bank = (0x42 & 0x3F) as usize; // 2
        assert_eq!(m.ppu_read(0x1000), bank as u8);
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut m = make_mapper(2, 2, false);
        m.cpu_write(0x8000, 6);
        m.cpu_write(0x8001, 2);
        m.cpu_write(0x8000, 0);
        m.cpu_write(0x8001, 4);

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);
        let data = w.finish();

        let mut m2 = make_mapper(2, 2, false);
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.cpu_read(0x8000), m.cpu_read(0x8000));
        assert_eq!(m2.ppu_read(0x0000), m.ppu_read(0x0000));
    }
}
