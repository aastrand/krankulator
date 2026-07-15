use super::super::super::io;
use super::{
    mirror_nametable_addr, mirroring_from_flags, NametableMirror, CPU_RAM_SIZE,
    PALETTE_MIRROR_CLEAR, PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, RESET_TARGET_ADDR,
    VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x8000; // 32KB
const CHR_RAM_SIZE: usize = 0x8000; // 32KB
const CHR_PAGE_SIZE: usize = 0x1000; // 4KB

pub struct OekaKidsMapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,

    selected_prg: u8,
    outer_chr: u8,
    inner_chr: u8,
    last_fetch_addr: u16,
    mirroring: NametableMirror,

    chr_ram: Box<[u8; CHR_RAM_SIZE]>,
    vram: Vec<u8>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl OekaKidsMapper {
    pub fn new(flags: u8, prg_banks_16k: Vec<[u8; 16384]>) -> Self {
        let mut prg_rom = vec![];
        for pair in prg_banks_16k.chunks(2) {
            let mut bank = [0u8; PRG_BANK_SIZE];
            bank[0..16384].copy_from_slice(&pair[0]);
            let second = if pair.len() > 1 { &pair[1] } else { &pair[0] };
            bank[16384..].copy_from_slice(second);
            prg_rom.push(bank);
        }

        OekaKidsMapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            selected_prg: 0,
            outer_chr: 0,
            inner_chr: 0,
            last_fetch_addr: 0,
            mirroring: mirroring_from_flags(flags),
            chr_ram: Box::new([0; CHR_RAM_SIZE]),
            vram: vec![0; VRAM_SIZE as usize],
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

    fn chr_addr(&self, addr: u16) -> usize {
        let page = if addr < 0x1000 {
            (self.outer_chr | self.inner_chr) as usize
        } else {
            (self.outer_chr | 0x03) as usize
        };
        (page * CHR_PAGE_SIZE + (addr as usize & (CHR_PAGE_SIZE - 1))) & (CHR_RAM_SIZE - 1)
    }

    fn nt_addr(&self, addr: u16) -> usize {
        mirror_nametable_addr(addr, self.mirroring) as usize & 0x7FF
    }

    fn latch_vram_addr(&mut self, addr: u16) {
        // The act of moving the PPU address into $2xxx latches A9-A8 as the
        // inner 4KB CHR-RAM bank
        if (self.last_fetch_addr & 0x3000) != 0x2000 && (addr & 0x3000) == 0x2000 {
            self.inner_chr = ((addr >> 8) & 0x03) as u8;
        }
        self.last_fetch_addr = addr;
    }
}

impl MemoryMapper for OekaKidsMapper {
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
                self.selected_prg = value & 0x03;
                self.outer_chr = value & 0x04;
            }
            _ => {}
        }
    }

    fn ppu_fetch(&mut self, addr: u16, _dot: u64) -> u8 {
        self.latch_vram_addr(addr);
        self.ppu_read(addr)
    }

    fn notify_vram_addr(&mut self, addr: u16) {
        self.latch_vram_addr(addr);
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
        96
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&*self.chr_ram);
        w.write_bytes(&self.vram);
        w.write_bytes(&self.palette_ram);
        w.write_u8(self.selected_prg);
        w.write_u8(self.outer_chr);
        w.write_u8(self.inner_chr);
        w.write_u16(self.last_fetch_addr);
        super::save_mirroring(w, self.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut *self.chr_ram)?;
        r.read_bytes_into(&mut self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        self.selected_prg = r.read_u8()?;
        self.outer_chr = r.read_u8()?;
        self.inner_chr = r.read_u8()?;
        self.last_fetch_addr = r.read_u16()?;
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper() -> OekaKidsMapper {
        let mut prg = Vec::new();
        for i in 0..8u8 {
            let mut bank = [0xFFu8; 16384];
            bank[0] = i;
            prg.push(bank);
        }
        OekaKidsMapper::new(0, prg)
    }

    #[test]
    fn test_prg_banking() {
        let mut m = make_mapper();
        assert_eq!(m.cpu_read(0x8000), 0);
        m.cpu_write(0x8001, 0x02); // 32KB bank 2 (write to 0xFF ROM byte)
        assert_eq!(m.cpu_read(0x8000), 4);
    }

    #[test]
    fn test_chr_latch_from_nt_fetch() {
        let mut m = make_mapper();

        // Write markers into each 4KB page via the latch
        for page in 0..3u16 {
            m.ppu_fetch(0x0000, 0); // leave $2xxx
            m.ppu_fetch(0x2000 | (page << 8), 0); // latch inner bank
            m.ppu_write(0x0000, 0x10 + page as u8);
        }

        for page in 0..3u16 {
            m.ppu_fetch(0x0000, 0);
            m.ppu_fetch(0x2000 | (page << 8), 0);
            assert_eq!(m.ppu_read(0x0000), 0x10 + page as u8);
        }

        // Only the transition into $2xxx latches: staying in $2xxx does not
        m.ppu_fetch(0x0000, 0);
        m.ppu_fetch(0x2000, 0); // latch 0
        m.ppu_fetch(0x2300, 0); // no latch (still in $2xxx)
        assert_eq!(m.ppu_read(0x0000), 0x10);
    }

    #[test]
    fn test_upper_chr_page_semi_fixed() {
        let mut m = make_mapper();

        // Page at $1000 is outer|3
        m.ppu_write(0x1000, 0x77);
        m.ppu_fetch(0x0000, 0);
        m.ppu_fetch(0x2300, 0); // inner = 3
        assert_eq!(m.ppu_read(0x0000), 0x77); // inner 3 aliases upper page
    }
}
