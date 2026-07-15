use super::super::super::io;
use super::{
    mirror_nametable_addr, NametableMirror, CPU_RAM_SIZE, PALETTE_MIRROR_CLEAR,
    PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, RESET_TARGET_ADDR, VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x4000; // 16KB
const CHR_BANK_SIZE: usize = 0x0400; // 1KB
const WRAM_SIZE: usize = 0x2000;

pub struct DaouMapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_rom: Vec<[u8; CHR_BANK_SIZE]>,

    prg_reg: u8,
    chr_low: [u8; 8],
    chr_high: [u8; 8],
    mirroring: NametableMirror,

    wram: Box<[u8; WRAM_SIZE]>,
    vram: Vec<u8>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl DaouMapper {
    pub fn new(prg_banks_16k: Vec<[u8; 16384]>, chr_banks_8k: Vec<[u8; 8192]>) -> Self {
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

        DaouMapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom: prg_banks_16k,
            chr_rom,
            prg_reg: 0,
            chr_low: [0; 8],
            chr_high: [0; 8],
            mirroring: NametableMirror::Lower,
            wram: Box::new([0; WRAM_SIZE]),
            vram: vec![0; VRAM_SIZE as usize],
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
        }
    }

    fn prg_read(&self, addr: u16) -> u8 {
        let len = self.prg_rom.len().max(1);
        let bank = if addr < 0xC000 {
            self.prg_reg as usize % len
        } else {
            len - 1
        };
        self.prg_rom
            .get(bank)
            .map_or(0, |b| b[(addr as usize) & (PRG_BANK_SIZE - 1)])
    }

    fn chr_1k_index(&self, addr: u16) -> usize {
        let slot = (addr as usize >> 10) & 7;
        let idx = ((self.chr_high[slot] as usize) << 8) | self.chr_low[slot] as usize;
        idx % self.chr_rom.len().max(1)
    }

    fn nt_addr(&self, addr: u16) -> usize {
        mirror_nametable_addr(addr, self.mirroring) as usize & 0x7FF
    }

    fn write_register(&mut self, addr: u16, value: u8) {
        match addr {
            0xC000..=0xC00F => {
                let slot = (addr & 0x03) as usize + if addr >= 0xC008 { 4 } else { 0 };
                if addr & 0x04 != 0 {
                    self.chr_high[slot] = value;
                } else {
                    self.chr_low[slot] = value;
                }
            }
            0xC010 => self.prg_reg = value,
            0xC014 => {
                self.mirroring = if value & 0x01 != 0 {
                    NametableMirror::Horizontal
                } else {
                    NametableMirror::Vertical
                };
            }
            _ => {}
        }
    }
}

impl MemoryMapper for DaouMapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x6000..=0x7FFF => self.wram[(addr - 0x6000) as usize],
            0x8000..=0xFFFF => self.prg_read(addr),
            _ => 0,
        }
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
            0xC000..=0xC014 => self.write_register(addr, value),
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
                let idx = self.chr_1k_index(addr);
                if let Some(b) = self.chr_rom.get(idx) {
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
            0x0000..=0x1FFF => {}
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
        156
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.wram);
        w.write_bytes(&self.vram);
        w.write_bytes(&self.palette_ram);
        w.write_u8(self.prg_reg);
        w.write_bytes(&self.chr_low);
        w.write_bytes(&self.chr_high);
        super::save_mirroring(w, self.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut *self.wram)?;
        r.read_bytes_into(&mut self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        self.prg_reg = r.read_u8()?;
        r.read_bytes_into(&mut self.chr_low)?;
        r.read_bytes_into(&mut self.chr_high)?;
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper() -> DaouMapper {
        let mut prg = Vec::new();
        for i in 0..4u8 {
            let mut bank = [0u8; 16384];
            bank[0] = i;
            prg.push(bank);
        }
        let mut chr = Vec::new();
        for i in 0..4u8 {
            let mut bank = [0u8; 8192];
            for k in 0..8 {
                bank[k * CHR_BANK_SIZE] = i * 8 + k as u8;
            }
            chr.push(bank);
        }
        DaouMapper::new(prg, chr)
    }

    #[test]
    fn test_prg_and_chr_banking() {
        let mut m = make_mapper();

        assert_eq!(m.cpu_read(0xC000), 3);
        m.cpu_write(0xC010, 2);
        assert_eq!(m.cpu_read(0x8000), 2);

        // CHR slot 0: low reg at $C000
        m.cpu_write(0xC000, 9);
        assert_eq!(m.ppu_read(0x0000), 9);
        // CHR slot 4: low reg at $C008
        m.cpu_write(0xC008, 17);
        assert_eq!(m.ppu_read(0x1000), 17);
        // High reg contributes bit 8 (wraps via modulo with only 32 banks)
        m.cpu_write(0xC004, 1);
        assert_eq!(m.ppu_read(0x0000), ((256 + 9) % 32) as u8);
    }

    #[test]
    fn test_default_one_screen() {
        let mut m = make_mapper();
        m.ppu_write(0x2000, 0x42);
        assert_eq!(m.ppu_read(0x2C00), 0x42);

        m.cpu_write(0xC014, 0);
        m.ppu_write(0x2000, 0x11);
        m.ppu_write(0x2400, 0x22);
        assert_eq!(m.ppu_read(0x2800), 0x11); // vertical
    }
}
