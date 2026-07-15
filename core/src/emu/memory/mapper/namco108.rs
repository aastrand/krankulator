use super::super::super::io;
use super::{
    mirror_nametable_addr, NametableMirror, CPU_RAM_SIZE, PALETTE_MIRROR_CLEAR,
    PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, RESET_TARGET_ADDR, VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x2000; // 8KB
const CHR_BANK_SIZE: usize = 0x0400; // 1KB

const FOUR_SCREEN_VRAM_SIZE: usize = 0x1000; // 4KB

#[derive(Copy, Clone, PartialEq)]
pub enum Namco108Variant {
    M206,
    M88,
    M76,
    M95,
    M154,
}

pub struct Namco108Mapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_rom: Vec<[u8; CHR_BANK_SIZE]>,

    bank_select: u8,
    chr_banks: [u8; 6],
    prg_banks: [u8; 2],
    mirroring: NametableMirror,
    variant: Namco108Variant,
    four_screen: bool,

    vram: Vec<u8>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl Namco108Mapper {
    pub fn new(
        flags: u8,
        prg_banks_16k: Vec<[u8; 16384]>,
        chr_banks_8k: Vec<[u8; 8192]>,
        variant: Namco108Variant,
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

        let four_screen = flags & 0x08 != 0;
        let mirroring = if flags & 1 != 0 {
            NametableMirror::Vertical
        } else {
            NametableMirror::Horizontal
        };

        let vram_size = if four_screen {
            FOUR_SCREEN_VRAM_SIZE
        } else {
            VRAM_SIZE as usize
        };

        Namco108Mapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            chr_rom,
            bank_select: 0,
            chr_banks: match variant {
                Namco108Variant::M88 | Namco108Variant::M154 => [0, 2, 0x44, 0x45, 0x46, 0x47],
                Namco108Variant::M76 => [0, 0, 0, 1, 2, 3],
                _ => [0, 2, 4, 5, 6, 7],
            },
            prg_banks: [0, 1],
            mirroring,
            variant,
            four_screen,
            vram: vec![0; vram_size],
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
        }
    }

    fn chr_1k_index(&self, addr: u16) -> usize {
        let slot_1k = (addr as usize >> 10) & 7;
        let idx = if self.variant == Namco108Variant::M76 {
            // NAMCOT-3446: regs 2-5 select 2KB banks at $0000/$0800/$1000/$1800
            let reg = self.chr_banks[2 + (slot_1k >> 1)] as usize;
            reg * 2 + (slot_1k & 1)
        } else {
            match slot_1k {
                0 => self.chr_banks[0] as usize,
                1 => self.chr_banks[0] as usize + 1,
                2 => self.chr_banks[1] as usize,
                3 => self.chr_banks[1] as usize + 1,
                s => self.chr_banks[s - 2] as usize,
            }
        };
        idx % self.chr_rom.len().max(1)
    }

    fn nt_addr(&self, addr: u16) -> usize {
        if self.four_screen {
            (addr & 0xFFF) as usize
        } else if self.variant == Namco108Variant::M95 {
            // NAMCOT-3425: CIRAM A10 is driven by CHR A15, i.e. bit 5 of the
            // 2KB CHR register covering the mirrored nametable range
            let reg = if addr & 0x0800 == 0 {
                self.chr_banks[0]
            } else {
                self.chr_banks[1]
            };
            let page = ((reg >> 5) & 1) as usize;
            page << 10 | (addr & 0x03FF) as usize
        } else {
            mirror_nametable_addr(addr, self.mirroring) as usize & 0x7FF
        }
    }

    fn is_mapper88_chr(&self) -> bool {
        matches!(self.variant, Namco108Variant::M88 | Namco108Variant::M154)
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

    fn cpu_peek(&self, addr: u16) -> u8 {
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
            0x8000..=0xFFFF => {
                // NAMCOT-3453: one-screen select decoded across the whole range
                if self.variant == Namco108Variant::M154 {
                    self.mirroring = if value & 0x40 != 0 {
                        NametableMirror::Higher
                    } else {
                        NametableMirror::Lower
                    };
                }
                if addr > 0x9FFF {
                    return;
                }
                if addr & 1 == 0 {
                    self.bank_select = value & 0x07;
                } else {
                    let reg = self.bank_select as usize;
                    let is_m88 = self.is_mapper88_chr();
                    let v = if is_m88 {
                        if reg < 2 {
                            value & 0x3F
                        } else {
                            (value & 0x3F) | 0x40
                        }
                    } else {
                        value
                    };
                    match reg {
                        0 => self.chr_banks[0] = if is_m88 { v & 0xFE } else { v & 0x3E },
                        1 => self.chr_banks[1] = if is_m88 { v & 0xFE } else { v & 0x3E },
                        2 => self.chr_banks[2] = if is_m88 { v } else { v & 0x3F },
                        3 => self.chr_banks[3] = if is_m88 { v } else { v & 0x3F },
                        4 => self.chr_banks[4] = if is_m88 { v } else { v & 0x3F },
                        5 => self.chr_banks[5] = if is_m88 { v } else { v & 0x3F },
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
            0x0000..=0x1FFF => {
                let idx = self.chr_1k_index(addr);
                self.chr_rom
                    .get(idx)
                    .map_or(0, |b| b[addr as usize & 0x3FF])
            }
            0x2000..=0x3EFF => self.vram[self.nt_addr(addr)],
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
                let bank_idx = self.chr_1k_index(addr);
                if let Some(b) = self.chr_rom.get(bank_idx) {
                    let offset = addr as usize & 0x3FF;
                    let copy_size = size.min(CHR_BANK_SIZE - offset);
                    unsafe { std::ptr::copy(b.as_ptr().add(offset), dest, copy_size) }
                }
            }
            0x2000..=0x3EFF => {
                let vram_addr = self.nt_addr(addr);
                let copy_size = size.min(self.vram.len() - vram_addr);
                unsafe { std::ptr::copy(self.vram.as_ptr().add(vram_addr), dest, copy_size) }
            }
            _ => {}
        }
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {} // CHR ROM, not writable
            0x2000..=0x3EFF => {
                let idx = self.nt_addr(addr);
                self.vram[idx] = value;
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
        match self.variant {
            Namco108Variant::M206 => 206,
            Namco108Variant::M88 => 88,
            Namco108Variant::M76 => 76,
            Namco108Variant::M95 => 95,
            Namco108Variant::M154 => 154,
        }
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&self.vram);
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
        r.read_bytes_into(&mut self.vram)?;
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

    fn make_variant(
        num_prg_16k: usize,
        num_chr_8k: usize,
        variant: Namco108Variant,
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

        Box::new(Namco108Mapper::new(0, prg, chr, variant))
    }

    fn make_mapper(
        num_prg_16k: usize,
        num_chr_8k: usize,
        is_mapper88: bool,
    ) -> Box<dyn MemoryMapper> {
        make_variant(
            num_prg_16k,
            num_chr_8k,
            if is_mapper88 {
                Namco108Variant::M88
            } else {
                Namco108Variant::M206
            },
        )
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
    fn test_mapper76_2k_chr_banks() {
        let mut m = make_variant(2, 4, Namco108Variant::M76); // 32 x 1KB CHR banks

        // Reg 2 selects the 2KB bank at $0000 (value in 2KB units)
        m.cpu_write(0x8000, 2);
        m.cpu_write(0x8001, 3); // 2KB bank 3 = 1KB banks 6,7
        assert_eq!(m.ppu_read(0x0000), 6);
        assert_eq!(m.ppu_read(0x0400), 7);

        // Reg 5 selects the 2KB bank at $1800
        m.cpu_write(0x8000, 5);
        m.cpu_write(0x8001, 8); // 2KB bank 8 = 1KB banks 16,17
        assert_eq!(m.ppu_read(0x1800), 16);
        assert_eq!(m.ppu_read(0x1C00), 17);
    }

    #[test]
    fn test_mapper95_nametable_from_chr_bit5() {
        let mut m = make_variant(2, 4, Namco108Variant::M95);

        // Bit 5 of reg 0 drives CIRAM A10 for $2000-$27FF; reg 1 for $2800-$2FFF
        m.cpu_write(0x8000, 0);
        m.cpu_write(0x8001, 0x20); // bit 5 set -> page 1
        m.cpu_write(0x8000, 1);
        m.cpu_write(0x8001, 0x00); // bit 5 clear -> page 0

        m.ppu_write(0x2000, 0xAA);
        // $2000 goes to page 1, $2800 to page 0: no aliasing
        assert_eq!(m.ppu_read(0x2000), 0xAA);
        assert_eq!(m.ppu_read(0x2800), 0x00);

        // $2400 shares page 1 with $2000
        assert_eq!(m.ppu_read(0x2400), 0xAA);
    }

    #[test]
    fn test_mapper154_one_screen_select() {
        let mut m = make_variant(2, 4, Namco108Variant::M154);

        // Bit 6 clear: one-screen A
        m.cpu_write(0x8000, 0x00);
        m.ppu_write(0x2000, 0x11);
        assert_eq!(m.ppu_read(0x2C00), 0x11);

        // Bit 6 set (decoded over the whole $8000-$FFFF range): one-screen B
        m.cpu_write(0xC000, 0x40);
        m.ppu_write(0x2000, 0x22);
        assert_eq!(m.ppu_read(0x2C00), 0x22);
        // Page A content still intact underneath
        m.cpu_write(0xC000, 0x00);
        assert_eq!(m.ppu_read(0x2000), 0x11);
    }

    #[test]
    fn test_mapper154_chr_forced_bit6() {
        let mut m = make_variant(2, 16, Namco108Variant::M154); // 128 x 1KB CHR banks

        // Like mapper 88: 1KB regs (2-5) get bit 6 forced on
        m.cpu_write(0x8000, 2);
        m.cpu_write(0x8001, 2); // stored as 0x42 = 1KB bank 66
        assert_eq!(m.ppu_read(0x1000), 66);
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
