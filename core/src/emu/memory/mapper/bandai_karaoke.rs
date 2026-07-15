use super::super::super::io;
use super::{
    mirror_nametable_addr, NametableMirror, CPU_RAM_SIZE, PALETTE_MIRROR_CLEAR,
    PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, RESET_TARGET_ADDR, VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x4000; // 16KB
const CHR_RAM_SIZE: usize = 0x2000;

pub struct BandaiKaraokeMapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,

    prg_reg: usize,
    prg_mapped: bool,
    mirroring: NametableMirror,

    chr_ram: Box<[u8; CHR_RAM_SIZE]>,
    vram: Vec<u8>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl BandaiKaraokeMapper {
    pub fn new(flags: u8, prg_banks_16k: Vec<[u8; 16384]>) -> Self {
        BandaiKaraokeMapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom: prg_banks_16k,
            prg_reg: 0,
            prg_mapped: true,
            mirroring: if flags & 1 != 0 {
                NametableMirror::Vertical
            } else {
                NametableMirror::Horizontal
            },
            chr_ram: Box::new([0; CHR_RAM_SIZE]),
            vram: vec![0; VRAM_SIZE as usize],
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
        }
    }

    fn prg_read(&self, addr: u16) -> u8 {
        let len = self.prg_rom.len().max(1);
        if addr < 0xC000 {
            if !self.prg_mapped {
                return 0xFF; // open bus: expansion cart not present
            }
            let bank = self.prg_reg % len;
            self.prg_rom
                .get(bank)
                .map_or(0, |b| b[(addr as usize) & (PRG_BANK_SIZE - 1)])
        } else {
            // Fixed to the last bank of the internal (first 128KB) ROM
            let bank = 7.min(len - 1);
            self.prg_rom
                .get(bank)
                .map_or(0, |b| b[(addr as usize) & (PRG_BANK_SIZE - 1)])
        }
    }

    fn nt_addr(&self, addr: u16) -> usize {
        mirror_nametable_addr(addr, self.mirroring) as usize & 0x7FF
    }
}

impl MemoryMapper for BandaiKaraokeMapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        self.cpu_peek(addr)
    }

    fn cpu_peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            // Microphone: bits 0/1 = A/B buttons (active low), bit 2 = mic ADC
            0x6000..=0x7FFF => 0xFB,
            0x8000..=0xFFFF => self.prg_read(addr),
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize] = value,
            0x8000..=0xFFFF => {
                let value = value & self.prg_read(addr);
                if value & 0x10 != 0 {
                    // Internal ROM
                    self.prg_reg = (value & 0x07) as usize;
                    self.prg_mapped = true;
                } else if self.prg_rom.len() > 8 {
                    // Expansion cart ROM (second 128KB)
                    self.prg_reg = ((value & 0x07) | 0x08) as usize;
                    self.prg_mapped = true;
                } else {
                    self.prg_mapped = false;
                }
                self.mirroring = if value & 0x20 != 0 {
                    NametableMirror::Horizontal
                } else {
                    NametableMirror::Vertical
                };
            }
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr_ram[addr as usize],
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
                let offset = addr as usize;
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
            0x0000..=0x1FFF => self.chr_ram[addr as usize] = value,
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
        188
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.chr_ram);
        w.write_bytes(&self.vram);
        w.write_bytes(&self.palette_ram);
        w.write_u8(self.prg_reg as u8);
        w.write_bool(self.prg_mapped);
        super::save_mirroring(w, self.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut *self.chr_ram)?;
        r.read_bytes_into(&mut self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        self.prg_reg = r.read_u8()? as usize;
        self.prg_mapped = r.read_bool()?;
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper(num_prg: usize) -> BandaiKaraokeMapper {
        let mut prg = Vec::new();
        for i in 0..num_prg {
            let mut bank = [0xFFu8; 16384];
            bank[0] = i as u8;
            prg.push(bank);
        }
        BandaiKaraokeMapper::new(0, prg)
    }

    #[test]
    fn test_internal_and_expansion_banks() {
        let mut m = make_mapper(16);

        m.cpu_write(0x8001, 0x13); // internal, bank 3
        assert_eq!(m.cpu_read(0x8000), 3);
        assert_eq!(m.cpu_read(0xC000), 7); // fixed internal last

        m.cpu_write(0x8001, 0x03); // expansion, bank 8+3
        assert_eq!(m.cpu_read(0x8000), 11);
    }

    #[test]
    fn test_no_expansion_open_bus() {
        let mut m = make_mapper(8);
        m.cpu_write(0x8001, 0x02); // expansion select without expansion ROM
        assert_eq!(m.cpu_read(0x8000), 0xFF);
        m.cpu_write(0xC001, 0x11); // back to internal (write via fixed bank ROM=0xFF)
        assert_eq!(m.cpu_read(0x8000), 1);
    }

    #[test]
    fn test_microphone_reads() {
        let mut m = make_mapper(8);
        assert_eq!(m.cpu_read(0x6000), 0xFB);
    }
}
