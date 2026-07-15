use super::super::super::io;
use super::{
    mirror_nametable_addr, NametableMirror, CPU_RAM_SIZE, PALETTE_MIRROR_CLEAR,
    PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, RESET_TARGET_ADDR, VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x2000; // 8KB
const CHR_RAM_SIZE: usize = 0x2000;
const WRAM_SIZE: usize = 0x2000;

pub struct K1029Mapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,

    prg_pages: [usize; 4],
    chr_write_protect: bool,
    mirroring: NametableMirror,

    chr_ram: Box<[u8; CHR_RAM_SIZE]>,
    wram: Box<[u8; WRAM_SIZE]>,
    vram: Vec<u8>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl K1029Mapper {
    pub fn new(prg_banks_16k: Vec<[u8; 16384]>) -> Self {
        let mut prg_rom = vec![];
        for bank in &prg_banks_16k {
            prg_rom.push(<[u8; PRG_BANK_SIZE]>::try_from(&bank[0..PRG_BANK_SIZE]).unwrap());
            prg_rom.push(
                <[u8; PRG_BANK_SIZE]>::try_from(&bank[PRG_BANK_SIZE..2 * PRG_BANK_SIZE]).unwrap(),
            );
        }

        let mut mapper = K1029Mapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            prg_pages: [0, 1, 2, 3],
            chr_write_protect: true,
            mirroring: NametableMirror::Vertical,
            chr_ram: Box::new([0; CHR_RAM_SIZE]),
            wram: Box::new([0; WRAM_SIZE]),
            vram: vec![0; VRAM_SIZE as usize],
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
        };
        mapper.write_register(0x8000, 0);
        mapper
    }

    fn write_register(&mut self, addr: u16, value: u8) {
        self.mirroring = if value & 0x40 != 0 {
            NametableMirror::Horizontal
        } else {
            NametableMirror::Vertical
        };

        let sub_bank = (value >> 7) as usize;
        let mut bank = ((value & 0x7F) as usize) << 1;
        let mode = addr & 0x03;

        self.chr_write_protect = mode == 0 || mode == 3;

        match mode {
            0 => {
                self.prg_pages = [
                    bank ^ sub_bank,
                    (bank + 1) ^ sub_bank,
                    (bank + 2) ^ sub_bank,
                    (bank + 3) ^ sub_bank,
                ];
            }
            1 | 3 => {
                bank |= sub_bank;
                self.prg_pages[0] = bank;
                self.prg_pages[1] = bank + 1;
                let high = (if mode == 3 { bank } else { bank | 0x0E }) | sub_bank;
                self.prg_pages[2] = high;
                self.prg_pages[3] = high + 1;
            }
            _ => {
                bank |= sub_bank;
                self.prg_pages = [bank; 4];
            }
        }
    }

    fn prg_read(&self, addr: u16) -> u8 {
        let len = self.prg_rom.len().max(1);
        let slot = ((addr as usize) >> 13) & 3;
        let bank = self.prg_pages[slot] % len;
        self.prg_rom
            .get(bank)
            .map_or(0, |b| b[(addr as usize) & (PRG_BANK_SIZE - 1)])
    }

    fn nt_addr(&self, addr: u16) -> usize {
        mirror_nametable_addr(addr, self.mirroring) as usize & 0x7FF
    }
}

impl MemoryMapper for K1029Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        self.cpu_peek(addr)
    }

    fn cpu_peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x6000..=0x7FFF => self.wram[(addr - 0x6000) as usize],
            0x8000..=0xFFFF => self.prg_read(addr),
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize] = value,
            0x6000..=0x7FFF => self.wram[(addr - 0x6000) as usize] = value,
            0x8000..=0xFFFF => self.write_register(addr, value),
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
            0x0000..=0x1FFF => {
                if !self.chr_write_protect {
                    self.chr_ram[addr as usize] = value;
                }
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
        15
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.wram);
        w.write_bytes(&*self.chr_ram);
        w.write_bytes(&self.vram);
        w.write_bytes(&self.palette_ram);
        for &p in &self.prg_pages {
            w.write_u16(p as u16);
        }
        w.write_bool(self.chr_write_protect);
        super::save_mirroring(w, self.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut *self.wram)?;
        r.read_bytes_into(&mut *self.chr_ram)?;
        r.read_bytes_into(&mut self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        for p in &mut self.prg_pages {
            *p = r.read_u16()? as usize;
        }
        self.chr_write_protect = r.read_bool()?;
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper() -> K1029Mapper {
        let mut prg = Vec::new();
        for i in 0..16u8 {
            let mut bank = [0u8; 16384];
            bank[0] = i * 2;
            bank[PRG_BANK_SIZE] = i * 2 + 1;
            prg.push(bank);
        }
        K1029Mapper::new(prg)
    }

    #[test]
    fn test_mode_0_nrom256() {
        let mut m = make_mapper();
        m.cpu_write(0x8000, 2); // 8KB bank base 4
        assert_eq!(m.cpu_read(0x8000), 4);
        assert_eq!(m.cpu_read(0xA000), 5);
        assert_eq!(m.cpu_read(0xC000), 6);
        assert_eq!(m.cpu_read(0xE000), 7);
    }

    #[test]
    fn test_mode_1_unrom() {
        let mut m = make_mapper();
        m.cpu_write(0x8001, 2);
        assert_eq!(m.cpu_read(0x8000), 4);
        assert_eq!(m.cpu_read(0xA000), 5);
        // Upper half fixed UNROM-style
        assert_eq!(m.cpu_read(0xC000), 4 | 0x0E);
        assert_eq!(m.cpu_read(0xE000), (4 | 0x0E) + 1);
    }

    #[test]
    fn test_mode_2_nrom64() {
        let mut m = make_mapper();
        m.cpu_write(0x8002, 3); // 8KB bank 6 mirrored
        assert_eq!(m.cpu_read(0x8000), 6);
        assert_eq!(m.cpu_read(0xA000), 6);
        assert_eq!(m.cpu_read(0xC000), 6);
        assert_eq!(m.cpu_read(0xE000), 6);
    }

    #[test]
    fn test_chr_write_protect_by_mode() {
        let mut m = make_mapper();
        m.cpu_write(0x8000, 0); // mode 0: protected
        m.ppu_write(0x0000, 0x42);
        assert_eq!(m.ppu_read(0x0000), 0);

        m.cpu_write(0x8001, 0); // mode 1: writable
        m.ppu_write(0x0000, 0x42);
        assert_eq!(m.ppu_read(0x0000), 0x42);
    }
}
