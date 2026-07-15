use super::super::super::io;
use super::{
    mirror_nametable_addr, mirroring_from_flags, NametableMirror, CPU_RAM_SIZE,
    PALETTE_MIRROR_CLEAR, PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, RESET_TARGET_ADDR,
    VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_SIZE: usize = 0x8000; // 32KB fixed
const CHR_RAM_SIZE: usize = 0x4000; // 16KB in two 8KB RAMs
const CHR_PAGE_SIZE: usize = 0x1000; // 4KB pages

pub struct CpromMapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: [u8; PRG_SIZE],
    chr_ram: Box<[u8; CHR_RAM_SIZE]>,
    selected_chr: u8,
    mirroring: NametableMirror,

    vram: Box<[u8; VRAM_SIZE as usize]>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl CpromMapper {
    pub fn new(flags: u8, prg_banks_16k: Vec<[u8; 16384]>) -> Self {
        let mut prg_rom = [0u8; PRG_SIZE];
        if !prg_banks_16k.is_empty() {
            prg_rom[0..16384].copy_from_slice(&prg_banks_16k[0]);
            let second = if prg_banks_16k.len() > 1 {
                &prg_banks_16k[1]
            } else {
                &prg_banks_16k[0]
            };
            prg_rom[16384..].copy_from_slice(second);
        }

        CpromMapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            chr_ram: Box::new([0; CHR_RAM_SIZE]),
            selected_chr: 0,
            mirroring: mirroring_from_flags(flags),
            vram: Box::new([0; VRAM_SIZE as usize]),
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
        }
    }

    fn chr_addr(&self, addr: u16) -> usize {
        if addr < 0x1000 {
            addr as usize
        } else {
            self.selected_chr as usize * CHR_PAGE_SIZE + (addr as usize & (CHR_PAGE_SIZE - 1))
        }
    }

    fn nt_addr(&self, addr: u16) -> usize {
        mirror_nametable_addr(addr, self.mirroring) as usize & 0x7FF
    }
}

impl MemoryMapper for CpromMapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x8000..=0xFFFF => self.prg_rom[(addr as usize) & (PRG_SIZE - 1)],
            _ => 0,
        }
    }

    fn cpu_peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x8000..=0xFFFF => self.prg_rom[(addr as usize) & (PRG_SIZE - 1)],
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize] = value,
            0x8000..=0xFFFF => {
                let value = value & self.prg_rom[(addr as usize) & (PRG_SIZE - 1)];
                self.selected_chr = value & 0x03;
            }
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr_ram[self.chr_addr(addr)],
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
                let src = self.chr_addr(addr);
                let copy_size = size.min(CHR_RAM_SIZE - src);
                unsafe { std::ptr::copy(self.chr_ram.as_ptr().add(src), dest, copy_size) }
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
            0x0000..=0x1FFF => {
                let a = self.chr_addr(addr);
                self.chr_ram[a] = value;
            }
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
        13
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.chr_ram);
        w.write_bytes(&*self.vram);
        w.write_bytes(&self.palette_ram);
        w.write_u8(self.selected_chr);
        super::save_mirroring(w, self.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut *self.chr_ram)?;
        r.read_bytes_into(&mut *self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        self.selected_chr = r.read_u8()?;
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper() -> CpromMapper {
        let mut prg = Vec::new();
        for i in 0..2u8 {
            let mut bank = [0xFFu8; 16384];
            bank[0] = i;
            prg.push(bank);
        }
        CpromMapper::new(1, prg)
    }

    #[test]
    fn test_chr_ram_banking() {
        let mut m = make_mapper();

        // Fill each 4KB page with a marker via banking
        for page in 0..4u8 {
            m.cpu_write(0x8001, page); // ROM=0xFF at $8001, no conflict masking
            m.ppu_write(0x1000, 0x10 + page);
        }
        for page in 0..4u8 {
            m.cpu_write(0x8001, page);
            assert_eq!(m.ppu_read(0x1000), 0x10 + page);
        }

        // $0000-$0FFF is fixed to page 0
        m.ppu_write(0x0000, 0x42);
        m.cpu_write(0x8001, 0);
        assert_eq!(m.ppu_read(0x1000), 0x42);
    }

    #[test]
    fn test_prg_fixed() {
        let mut m = make_mapper();
        assert_eq!(m.cpu_read(0x8000), 0);
        assert_eq!(m.cpu_read(0xC000), 1);
        m.cpu_write(0x8001, 3);
        assert_eq!(m.cpu_read(0x8000), 0);
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut m = make_mapper();
        m.cpu_write(0x8001, 2);
        m.ppu_write(0x1234, 0x99);

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);
        let data = w.finish();

        let mut m2 = make_mapper();
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.ppu_read(0x1234), 0x99);
    }
}
