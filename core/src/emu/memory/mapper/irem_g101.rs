use super::super::super::io;
use super::{
    mirror_nametable_addr, NametableMirror, CPU_RAM_SIZE, PALETTE_MIRROR_CLEAR,
    PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, RESET_TARGET_ADDR, VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x2000; // 8KB
const CHR_BANK_SIZE: usize = 0x0400; // 1KB
const WRAM_SIZE: usize = 0x2000;

pub struct IremG101Mapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_rom: Vec<[u8; CHR_BANK_SIZE]>,

    prg_reg0: u8,
    prg_reg1: u8,
    prg_mode: bool,
    chr_regs: [u8; 8],
    mirroring: NametableMirror,
    is_submapper1: bool,

    wram: Box<[u8; WRAM_SIZE]>,
    vram: Vec<u8>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl IremG101Mapper {
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

        let is_submapper1 = submapper == 1;
        let mirroring = if is_submapper1 {
            NametableMirror::Lower
        } else if flags & 1 != 0 {
            NametableMirror::Vertical
        } else {
            NametableMirror::Horizontal
        };

        IremG101Mapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            chr_rom,
            prg_reg0: 0,
            prg_reg1: 1,
            prg_mode: false,
            chr_regs: [0, 1, 2, 3, 4, 5, 6, 7],
            mirroring,
            is_submapper1,
            wram: Box::new([0; WRAM_SIZE]),
            vram: vec![0; VRAM_SIZE as usize],
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
        }
    }

    fn prg_bank_at(&self, addr: u16) -> usize {
        let len = self.prg_rom.len().max(1);
        let bank = match (addr, self.prg_mode) {
            (0x8000..=0x9FFF, false) => self.prg_reg0 as usize,
            (0x8000..=0x9FFF, true) => len.saturating_sub(2),
            (0xA000..=0xBFFF, _) => self.prg_reg1 as usize,
            (0xC000..=0xDFFF, false) => len.saturating_sub(2),
            (0xC000..=0xDFFF, true) => self.prg_reg0 as usize,
            _ => len.saturating_sub(1),
        };
        bank % len
    }

    fn prg_read(&self, addr: u16) -> u8 {
        let bank = self.prg_bank_at(addr);
        self.prg_rom
            .get(bank)
            .map_or(0, |b| b[(addr as usize) & (PRG_BANK_SIZE - 1)])
    }

    fn chr_1k_index(&self, addr: u16) -> usize {
        let slot = (addr as usize >> 10) & 7;
        self.chr_regs[slot] as usize % self.chr_rom.len().max(1)
    }

    fn nt_addr(&self, addr: u16) -> usize {
        mirror_nametable_addr(addr, self.mirroring) as usize & 0x7FF
    }
}

impl MemoryMapper for IremG101Mapper {
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
            0x8000..=0x8FFF => self.prg_reg0 = value & 0x1F,
            0x9000..=0x9FFF => {
                if !self.is_submapper1 {
                    self.mirroring = if value & 1 != 0 {
                        NametableMirror::Horizontal
                    } else {
                        NametableMirror::Vertical
                    };
                    self.prg_mode = value & 2 != 0;
                }
            }
            0xA000..=0xAFFF => self.prg_reg1 = value & 0x1F,
            0xB000..=0xBFFF => self.chr_regs[(addr & 7) as usize] = value,
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
        32
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.wram);
        w.write_bytes(&self.vram);
        w.write_bytes(&self.palette_ram);
        w.write_u8(self.prg_reg0);
        w.write_u8(self.prg_reg1);
        w.write_bool(self.prg_mode);
        for &b in &self.chr_regs {
            w.write_u8(b);
        }
        super::save_mirroring(w, self.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut *self.wram)?;
        r.read_bytes_into(&mut self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        self.prg_reg0 = r.read_u8()?;
        self.prg_reg1 = r.read_u8()?;
        self.prg_mode = r.read_bool()?;
        for b in &mut self.chr_regs {
            *b = r.read_u8()?;
        }
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper(submapper: u8) -> IremG101Mapper {
        let mut prg = Vec::new();
        for i in 0..4u8 {
            let mut bank = [0u8; 16384];
            bank[0] = i * 2;
            bank[PRG_BANK_SIZE] = i * 2 + 1;
            prg.push(bank);
        }
        let mut chr = Vec::new();
        for i in 0..2u8 {
            let mut bank = [0u8; 8192];
            for k in 0..8 {
                bank[k * CHR_BANK_SIZE] = i * 8 + k as u8;
            }
            chr.push(bank);
        }
        IremG101Mapper::new(0, prg, chr, submapper)
    }

    #[test]
    fn test_prg_mode_0() {
        let mut m = make_mapper(0);

        // Power-on: reg0=0 at $8000, reg1=1 at $A000, fixed last two at $C000/$E000
        assert_eq!(m.cpu_read(0x8000), 0);
        assert_eq!(m.cpu_read(0xA000), 1);
        assert_eq!(m.cpu_read(0xC000), 6);
        assert_eq!(m.cpu_read(0xE000), 7);

        m.cpu_write(0x8000, 3);
        m.cpu_write(0xA000, 4);
        assert_eq!(m.cpu_read(0x8000), 3);
        assert_eq!(m.cpu_read(0xA000), 4);
    }

    #[test]
    fn test_prg_mode_1() {
        let mut m = make_mapper(0);

        m.cpu_write(0x8000, 3);
        m.cpu_write(0x9000, 0x02);
        // Mode 1: $8000 fixed to second-last, $C000 follows reg0
        assert_eq!(m.cpu_read(0x8000), 6);
        assert_eq!(m.cpu_read(0xC000), 3);
        assert_eq!(m.cpu_read(0xE000), 7);
    }

    #[test]
    fn test_chr_banking() {
        let mut m = make_mapper(0);

        m.cpu_write(0xB000, 9);
        m.cpu_write(0xB007, 12);
        assert_eq!(m.ppu_read(0x0000), 9);
        assert_eq!(m.ppu_read(0x1C00), 12);
    }

    #[test]
    fn test_mirroring_control() {
        let mut m = make_mapper(0);

        // Bit 0 = 0: vertical ($2800 mirrors $2000, $2400 is distinct)
        m.cpu_write(0x9000, 0x00);
        m.ppu_write(0x2000, 0x11);
        m.ppu_write(0x2400, 0x22);
        assert_eq!(m.ppu_read(0x2800), 0x11);
        assert_eq!(m.ppu_read(0x2000), 0x11);

        // Bit 0 = 1: horizontal ($2400 mirrors $2000)
        m.cpu_write(0x9000, 0x01);
        assert_eq!(m.ppu_read(0x2400), 0x11);
    }

    #[test]
    fn test_submapper1_ignores_9000() {
        let mut m = make_mapper(1);

        m.cpu_write(0x9000, 0x03);
        // Still mode 0 and one-screen
        m.cpu_write(0x8000, 3);
        assert_eq!(m.cpu_read(0x8000), 3);
        m.ppu_write(0x2000, 0x44);
        assert_eq!(m.ppu_read(0x2C00), 0x44);
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut m = make_mapper(0);
        m.cpu_write(0x8000, 2);
        m.cpu_write(0x9000, 0x02);
        m.cpu_write(0xB003, 5);

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);
        let data = w.finish();

        let mut m2 = make_mapper(0);
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.cpu_read(0x8000), m.cpu_read(0x8000));
        assert_eq!(m2.cpu_read(0xC000), m.cpu_read(0xC000));
        assert_eq!(m2.ppu_read(0x0C00), m.ppu_read(0x0C00));
    }
}
