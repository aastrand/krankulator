use super::super::super::io;
use super::{
    mirror_nametable_addr, mirroring_from_flags, NametableMirror, CPU_RAM_SIZE,
    PALETTE_MIRROR_CLEAR, PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, RESET_TARGET_ADDR,
    VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x2000; // 8KB
const CHR_8K: usize = 0x2000;
const CHR_RAM_SIZE: usize = 0x8000; // 32KB in 8KB banks
const WRAM_SIZE: usize = 0x2000; // trainer target at $7000

pub struct FrontFareastMapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_rom: Vec<[u8; CHR_8K]>,

    prg_pages: [usize; 4],
    chr_bank: usize,
    ffe_alt_mode: bool,
    mirroring: NametableMirror,

    irq_enabled: bool,
    irq_counter: u16,
    irq_pending: bool,

    chr_ram: Box<[u8; CHR_RAM_SIZE]>,
    wram: Box<[u8; WRAM_SIZE]>,
    vram: Vec<u8>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl FrontFareastMapper {
    pub fn new(flags: u8, prg_banks_16k: Vec<[u8; 16384]>, chr_banks_8k: Vec<[u8; 8192]>) -> Self {
        let mut prg_rom = vec![];
        for bank in &prg_banks_16k {
            prg_rom.push(<[u8; PRG_BANK_SIZE]>::try_from(&bank[0..PRG_BANK_SIZE]).unwrap());
            prg_rom.push(
                <[u8; PRG_BANK_SIZE]>::try_from(&bank[PRG_BANK_SIZE..2 * PRG_BANK_SIZE]).unwrap(),
            );
        }

        FrontFareastMapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            chr_rom: chr_banks_8k,
            prg_pages: [0, 1, 14, 15],
            chr_bank: 0,
            ffe_alt_mode: true,
            mirroring: mirroring_from_flags(flags),
            irq_enabled: false,
            irq_counter: 0,
            irq_pending: false,
            chr_ram: Box::new([0; CHR_RAM_SIZE]),
            wram: Box::new([0; WRAM_SIZE]),
            vram: vec![0; VRAM_SIZE as usize],
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
        }
    }

    fn has_chr_ram(&self) -> bool {
        self.chr_rom.is_empty()
    }

    fn prg_read(&self, addr: u16) -> u8 {
        let len = self.prg_rom.len().max(1);
        let slot = ((addr as usize) >> 13) & 3;
        let bank = self.prg_pages[slot] % len;
        self.prg_rom
            .get(bank)
            .map_or(0, |b| b[(addr as usize) & (PRG_BANK_SIZE - 1)])
    }

    fn chr_read(&self, addr: u16) -> u8 {
        if self.has_chr_ram() {
            self.chr_ram[(self.chr_bank * CHR_8K + addr as usize) & (CHR_RAM_SIZE - 1)]
        } else {
            let bank = self.chr_bank % self.chr_rom.len();
            self.chr_rom[bank][addr as usize & (CHR_8K - 1)]
        }
    }

    fn nt_addr(&self, addr: u16) -> usize {
        mirror_nametable_addr(addr, self.mirroring) as usize & 0x7FF
    }
}

impl MemoryMapper for FrontFareastMapper {
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
            0x42FE => {
                self.ffe_alt_mode = value & 0x80 == 0;
                self.mirroring = if value & 0x10 != 0 {
                    NametableMirror::Higher
                } else {
                    NametableMirror::Lower
                };
            }
            0x42FF => {
                self.mirroring = if value & 0x10 != 0 {
                    NametableMirror::Horizontal
                } else {
                    NametableMirror::Vertical
                };
            }
            0x4501 => {
                self.irq_enabled = false;
                self.irq_pending = false;
            }
            0x4502 => {
                self.irq_counter = (self.irq_counter & 0xFF00) | value as u16;
                self.irq_pending = false;
            }
            0x4503 => {
                self.irq_counter = (self.irq_counter & 0x00FF) | ((value as u16) << 8);
                self.irq_enabled = true;
                self.irq_pending = false;
            }
            0x8000..=0xFFFF => {
                let mut value = value;
                if self.has_chr_ram() || self.ffe_alt_mode {
                    let page = ((value & 0xFC) >> 1) as usize;
                    self.prg_pages[0] = page;
                    self.prg_pages[1] = page + 1;
                    value &= 0x03;
                }
                self.chr_bank = value as usize;
            }
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.chr_read(addr),
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
                if self.has_chr_ram() {
                    let src = (self.chr_bank * CHR_8K + addr as usize) & (CHR_RAM_SIZE - 1);
                    let copy_size = size.min(CHR_RAM_SIZE - src);
                    unsafe { std::ptr::copy(self.chr_ram.as_ptr().add(src), dest, copy_size) }
                } else {
                    let bank = self.chr_bank % self.chr_rom.len();
                    let offset = addr as usize & (CHR_8K - 1);
                    let copy_size = size.min(CHR_8K - offset);
                    unsafe {
                        std::ptr::copy(self.chr_rom[bank].as_ptr().add(offset), dest, copy_size)
                    }
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
            0x0000..=0x1FFF => {
                if self.has_chr_ram() {
                    let a = (self.chr_bank * CHR_8K + addr as usize) & (CHR_RAM_SIZE - 1);
                    self.chr_ram[a] = value;
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

    fn cpu_cycle(&mut self, _ppu_dot: u64) {
        if self.irq_enabled {
            self.irq_counter = self.irq_counter.wrapping_add(1);
            if self.irq_counter == 0 {
                self.irq_enabled = false;
                self.irq_pending = true;
            }
        }
    }

    fn poll_irq(&mut self) -> bool {
        if self.irq_pending {
            self.irq_pending = false;
            return true;
        }
        false
    }

    fn mapper_id(&self) -> u8 {
        6
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
        w.write_u8(self.chr_bank as u8);
        w.write_bool(self.ffe_alt_mode);
        w.write_bool(self.irq_enabled);
        w.write_u16(self.irq_counter);
        w.write_bool(self.irq_pending);
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
        self.chr_bank = r.read_u8()? as usize;
        self.ffe_alt_mode = r.read_bool()?;
        self.irq_enabled = r.read_bool()?;
        self.irq_counter = r.read_u16()?;
        self.irq_pending = r.read_bool()?;
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper() -> FrontFareastMapper {
        let mut prg = Vec::new();
        for i in 0..16u8 {
            let mut bank = [0u8; 16384];
            bank[0] = i * 2;
            bank[PRG_BANK_SIZE] = i * 2 + 1;
            prg.push(bank);
        }
        FrontFareastMapper::new(0, prg, vec![])
    }

    #[test]
    fn test_power_on_and_banking() {
        let mut m = make_mapper();
        assert_eq!(m.cpu_read(0x8000), 0);
        assert_eq!(m.cpu_read(0xC000), 14);
        assert_eq!(m.cpu_read(0xE000), 15);

        m.cpu_write(0x8000, 3 << 2); // 16KB bank 3
        assert_eq!(m.cpu_read(0x8000), 6);
        assert_eq!(m.cpu_read(0xA000), 7);
        assert_eq!(m.cpu_read(0xC000), 14); // fixed
    }

    #[test]
    fn test_chr_ram_banking() {
        let mut m = make_mapper();
        for bank in 0..4u8 {
            m.cpu_write(0x8000, bank);
            m.ppu_write(0x0000, 0x20 + bank);
        }
        for bank in 0..4u8 {
            m.cpu_write(0x8000, bank);
            assert_eq!(m.ppu_read(0x0000), 0x20 + bank);
        }
    }

    #[test]
    fn test_irq_up_counter() {
        let mut m = make_mapper();
        m.cpu_write(0x4502, 0xFD); // low
        m.cpu_write(0x4503, 0xFF); // high + enable -> counter = 0xFFFD

        m.cpu_cycle(0);
        m.cpu_cycle(0);
        assert!(!m.poll_irq());
        m.cpu_cycle(0); // wraps to 0
        assert!(m.poll_irq());
    }

    #[test]
    fn test_one_screen_mirroring() {
        let mut m = make_mapper();
        m.cpu_write(0x42FE, 0x10); // one-screen B
        m.ppu_write(0x2000, 0x33);
        assert_eq!(m.ppu_read(0x2C00), 0x33);
    }
}
