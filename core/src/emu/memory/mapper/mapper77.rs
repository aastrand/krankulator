use super::super::super::io;
use super::{
    mirror_nametable_addr, mirroring_from_flags, NametableMirror, CPU_RAM_SIZE,
    PALETTE_MIRROR_CLEAR, PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, RESET_TARGET_ADDR,
    VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x8000; // 32KB
const CHR_BANK_SIZE: usize = 0x0800; // 2KB
const CHR_RAM_SIZE: usize = 0x1800; // 6KB at $0800-$1FFF
const FOUR_SCREEN_VRAM_SIZE: usize = 0x1000;

pub struct Mapper77 {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_rom: Vec<[u8; CHR_BANK_SIZE]>,

    selected_prg: u8,
    selected_chr: u8,
    mirroring: NametableMirror,
    four_screen: bool,

    chr_ram: Box<[u8; CHR_RAM_SIZE]>,
    vram: Vec<u8>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl Mapper77 {
    pub fn new(flags: u8, prg_banks_16k: Vec<[u8; 16384]>, chr_banks_8k: Vec<[u8; 8192]>) -> Self {
        let mut prg_rom = vec![];
        for pair in prg_banks_16k.chunks(2) {
            let mut bank = [0u8; PRG_BANK_SIZE];
            bank[0..16384].copy_from_slice(&pair[0]);
            let second = if pair.len() > 1 { &pair[1] } else { &pair[0] };
            bank[16384..].copy_from_slice(second);
            prg_rom.push(bank);
        }

        let mut chr_rom = vec![];
        for bank in &chr_banks_8k {
            for i in 0..4 {
                chr_rom.push(
                    <[u8; CHR_BANK_SIZE]>::try_from(
                        &bank[i * CHR_BANK_SIZE..(i + 1) * CHR_BANK_SIZE],
                    )
                    .unwrap(),
                );
            }
        }

        let four_screen = flags & 0x08 != 0;
        let vram_size = if four_screen {
            FOUR_SCREEN_VRAM_SIZE
        } else {
            VRAM_SIZE as usize
        };

        Mapper77 {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            chr_rom,
            selected_prg: 0,
            selected_chr: 0,
            mirroring: mirroring_from_flags(flags),
            four_screen,
            chr_ram: Box::new([0; CHR_RAM_SIZE]),
            vram: vec![0; vram_size],
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
        }
    }

    fn prg_read(&self, addr: u16) -> u8 {
        let bank = self.selected_prg as usize % self.prg_rom.len().max(1);
        self.prg_rom
            .get(bank)
            .map_or(0, |b| b[(addr as usize) & (PRG_BANK_SIZE - 1)])
    }

    fn nt_addr(&self, addr: u16) -> usize {
        if self.four_screen {
            (addr & 0xFFF) as usize
        } else {
            mirror_nametable_addr(addr, self.mirroring) as usize & 0x7FF
        }
    }
}

impl MemoryMapper for Mapper77 {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x8000..=0xFFFF => self.prg_read(addr),
            _ => 0,
        }
    }

    fn cpu_peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x8000..=0xFFFF => self.prg_read(addr),
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize] = value,
            0x8000..=0xFFFF => {
                let value = value & self.prg_read(addr);
                self.selected_prg = value & 0x0F;
                self.selected_chr = (value >> 4) & 0x0F;
            }
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x07FF => {
                let bank = self.selected_chr as usize % self.chr_rom.len().max(1);
                self.chr_rom
                    .get(bank)
                    .map_or(0, |b| b[addr as usize & (CHR_BANK_SIZE - 1)])
            }
            0x0800..=0x1FFF => self.chr_ram[addr as usize - 0x0800],
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
            0x0000..=0x07FF => {
                let bank = self.selected_chr as usize % self.chr_rom.len().max(1);
                if let Some(b) = self.chr_rom.get(bank) {
                    let offset = addr as usize & (CHR_BANK_SIZE - 1);
                    let copy_size = size.min(CHR_BANK_SIZE - offset);
                    unsafe { std::ptr::copy(b.as_ptr().add(offset), dest, copy_size) }
                }
            }
            0x0800..=0x1FFF => {
                let offset = addr as usize - 0x0800;
                let copy_size = size.min(CHR_RAM_SIZE - offset);
                unsafe { std::ptr::copy(self.chr_ram.as_ptr().add(offset), dest, copy_size) }
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
            0x0000..=0x07FF => {}
            0x0800..=0x1FFF => self.chr_ram[addr as usize - 0x0800] = value,
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
        77
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.chr_ram);
        w.write_bytes(&self.vram);
        w.write_bytes(&self.palette_ram);
        w.write_u8(self.selected_prg);
        w.write_u8(self.selected_chr);
        super::save_mirroring(w, self.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut *self.chr_ram)?;
        r.read_bytes_into(&mut self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        self.selected_prg = r.read_u8()?;
        self.selected_chr = r.read_u8()?;
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper() -> Mapper77 {
        let mut prg = Vec::new();
        for i in 0..8u8 {
            let mut bank = [0xFFu8; 16384];
            bank[0] = i;
            prg.push(bank);
        }
        let mut chr = Vec::new();
        for i in 0..2u8 {
            let mut bank = [0u8; 8192];
            for k in 0..4 {
                bank[k * CHR_BANK_SIZE] = i * 4 + k as u8;
            }
            chr.push(bank);
        }
        Mapper77::new(0x08, prg, chr)
    }

    #[test]
    fn test_prg_and_chr_banking() {
        let mut m = make_mapper();

        assert_eq!(m.cpu_read(0x8000), 0);
        // Write to $8001 (ROM=0xFF) to avoid bus conflict masking
        m.cpu_write(0x8001, 0x31); // PRG 32KB bank 1, CHR 2KB bank 3
        assert_eq!(m.cpu_read(0x8000), 2); // 32KB bank 1 = 16KB banks 2,3
        assert_eq!(m.ppu_read(0x0000), 3);
    }

    #[test]
    fn test_chr_ram_and_four_screen() {
        let mut m = make_mapper();

        m.ppu_write(0x0800, 0x5A);
        assert_eq!(m.ppu_read(0x0800), 0x5A);
        // CHR-ROM region not writable
        m.ppu_write(0x0000, 0x77);
        assert_eq!(m.ppu_read(0x0000), 0);

        // Four-screen: all four nametables distinct
        m.ppu_write(0x2000, 1);
        m.ppu_write(0x2400, 2);
        m.ppu_write(0x2800, 3);
        m.ppu_write(0x2C00, 4);
        assert_eq!(m.ppu_read(0x2000), 1);
        assert_eq!(m.ppu_read(0x2400), 2);
        assert_eq!(m.ppu_read(0x2800), 3);
        assert_eq!(m.ppu_read(0x2C00), 4);
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut m = make_mapper();
        m.cpu_write(0x8001, 0x21);
        m.ppu_write(0x0900, 0xAB);

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);
        let data = w.finish();

        let mut m2 = make_mapper();
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.cpu_read(0x8000), m.cpu_read(0x8000));
        assert_eq!(m2.ppu_read(0x0000), m.ppu_read(0x0000));
        assert_eq!(m2.ppu_read(0x0900), 0xAB);
    }
}
