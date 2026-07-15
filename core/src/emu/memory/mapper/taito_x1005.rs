use super::super::super::io;
use super::{
    mirror_nametable_addr, mirroring_from_flags, NametableMirror, CPU_RAM_SIZE,
    PALETTE_MIRROR_CLEAR, PALETTE_MIRROR_MASK, PALETTE_SIZE, PALETTE_START, RESET_TARGET_ADDR,
    VRAM_SIZE,
};
use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 0x2000; // 8KB
const CHR_BANK_SIZE: usize = 0x0400; // 1KB
const RAM_SIZE: usize = 0x80; // 128 bytes at $7F00, mirrored

const RAM_ENABLE_VALUE: u8 = 0xA3;

pub struct TaitoX1005Mapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_rom: Vec<[u8; CHR_BANK_SIZE]>,

    prg_regs: [u8; 3],
    chr_regs: [u8; 6],
    ram_permission: u8,
    mirroring: NametableMirror,
    // Mapper 207: CHR A17 drives CIRAM A10 (bit 7 of $7EF0/$7EF1)
    alternate_mirroring: bool,
    nt_pages: [u8; 4],

    ram: [u8; RAM_SIZE],
    vram: Vec<u8>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl TaitoX1005Mapper {
    pub fn new(
        flags: u8,
        prg_banks_16k: Vec<[u8; 16384]>,
        chr_banks_8k: Vec<[u8; 8192]>,
        alternate_mirroring: bool,
        sram_data: Option<Vec<u8>>,
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

        let mut ram = [0u8; RAM_SIZE];
        if let Some(data) = sram_data {
            let len = data.len().min(RAM_SIZE);
            ram[..len].copy_from_slice(&data[..len]);
        }

        TaitoX1005Mapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            chr_rom,
            prg_regs: [0, 0, 0],
            chr_regs: [0; 6],
            ram_permission: 0,
            mirroring: mirroring_from_flags(flags),
            alternate_mirroring,
            nt_pages: [0; 4],
            ram,
            vram: vec![0; VRAM_SIZE as usize],
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
        }
    }

    fn ram_enabled(&self) -> bool {
        self.ram_permission == RAM_ENABLE_VALUE
    }

    fn prg_bank_at(&self, addr: u16) -> usize {
        let len = self.prg_rom.len().max(1);
        let bank = match addr {
            0x8000..=0x9FFF => self.prg_regs[0] as usize,
            0xA000..=0xBFFF => self.prg_regs[1] as usize,
            0xC000..=0xDFFF => self.prg_regs[2] as usize,
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
        let idx = match slot {
            0 => self.chr_regs[0] as usize,
            1 => self.chr_regs[0] as usize + 1,
            2 => self.chr_regs[1] as usize,
            3 => self.chr_regs[1] as usize + 1,
            s => self.chr_regs[s - 2] as usize,
        };
        idx % self.chr_rom.len().max(1)
    }

    fn nt_addr(&self, addr: u16) -> usize {
        if self.alternate_mirroring {
            let page = self.nt_pages[((addr >> 10) & 3) as usize] as usize;
            (page << 10 | (addr & 0x03FF) as usize) & 0x7FF
        } else {
            mirror_nametable_addr(addr, self.mirroring) as usize & 0x7FF
        }
    }

    fn write_register(&mut self, addr: u16, value: u8) {
        match addr {
            0x7EF0 => {
                self.chr_regs[0] = value;
                if self.alternate_mirroring {
                    self.nt_pages[0] = value >> 7;
                    self.nt_pages[1] = value >> 7;
                }
            }
            0x7EF1 => {
                self.chr_regs[1] = value;
                if self.alternate_mirroring {
                    self.nt_pages[2] = value >> 7;
                    self.nt_pages[3] = value >> 7;
                }
            }
            0x7EF2..=0x7EF5 => self.chr_regs[(addr - 0x7EF0) as usize] = value,
            0x7EF6 | 0x7EF7 => {
                if !self.alternate_mirroring {
                    self.mirroring = if value & 0x01 != 0 {
                        NametableMirror::Vertical
                    } else {
                        NametableMirror::Horizontal
                    };
                }
            }
            0x7EF8 | 0x7EF9 => self.ram_permission = value,
            0x7EFA | 0x7EFB => self.prg_regs[0] = value,
            0x7EFC | 0x7EFD => self.prg_regs[1] = value,
            0x7EFE | 0x7EFF => self.prg_regs[2] = value,
            _ => {}
        }
    }
}

impl MemoryMapper for TaitoX1005Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x7F00..=0x7FFF if self.ram_enabled() => self.ram[(addr & 0x7F) as usize],
            0x8000..=0xFFFF => self.prg_read(addr),
            _ => 0,
        }
    }

    fn cpu_peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize],
            0x7F00..=0x7FFF if self.ram_enabled() => self.ram[(addr & 0x7F) as usize],
            0x8000..=0xFFFF => self.prg_read(addr),
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => self.cpu_ram[(addr & 0x7FF) as usize] = value,
            0x7EF0..=0x7EFF => self.write_register(addr, value),
            0x7F00..=0x7FFF if self.ram_enabled() => {
                self.ram[(addr & 0x7F) as usize] = value;
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
        if self.alternate_mirroring {
            207
        } else {
            80
        }
    }

    fn sram_data(&self) -> Option<&[u8]> {
        Some(&self.ram)
    }

    fn sram_data_mut(&mut self) -> Option<&mut [u8]> {
        Some(&mut self.ram)
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&self.ram);
        w.write_bytes(&self.vram);
        w.write_bytes(&self.palette_ram);
        for &b in &self.prg_regs {
            w.write_u8(b);
        }
        for &b in &self.chr_regs {
            w.write_u8(b);
        }
        w.write_u8(self.ram_permission);
        w.write_bytes(&self.nt_pages);
        super::save_mirroring(w, self.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut self.ram)?;
        r.read_bytes_into(&mut self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        for b in &mut self.prg_regs {
            *b = r.read_u8()?;
        }
        for b in &mut self.chr_regs {
            *b = r.read_u8()?;
        }
        self.ram_permission = r.read_u8()?;
        r.read_bytes_into(&mut self.nt_pages)?;
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper(alternate: bool) -> TaitoX1005Mapper {
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
        TaitoX1005Mapper::new(0, prg, chr, alternate, None)
    }

    #[test]
    fn test_prg_banking() {
        let mut m = make_mapper(false);

        assert_eq!(m.cpu_read(0xE000), 7); // fixed last

        m.cpu_write(0x7EFA, 3);
        m.cpu_write(0x7EFC, 4);
        m.cpu_write(0x7EFE, 5);
        assert_eq!(m.cpu_read(0x8000), 3);
        assert_eq!(m.cpu_read(0xA000), 4);
        assert_eq!(m.cpu_read(0xC000), 5);
    }

    #[test]
    fn test_chr_banking() {
        let mut m = make_mapper(false);

        m.cpu_write(0x7EF0, 4); // 2KB: 1KB banks 4,5 at $0000
        m.cpu_write(0x7EF1, 6); // 2KB: 1KB banks 6,7 at $0800
        m.cpu_write(0x7EF2, 10);
        assert_eq!(m.ppu_read(0x0000), 4);
        assert_eq!(m.ppu_read(0x0400), 5);
        assert_eq!(m.ppu_read(0x0800), 6);
        assert_eq!(m.ppu_read(0x1000), 10);
    }

    #[test]
    fn test_ram_permission() {
        let mut m = make_mapper(false);

        m.cpu_write(0x7F00, 0x42);
        assert_eq!(m.cpu_read(0x7F00), 0);

        m.cpu_write(0x7EF8, 0xA3);
        m.cpu_write(0x7F00, 0x42);
        assert_eq!(m.cpu_read(0x7F00), 0x42);
        // 128 bytes mirrored: $7F80 aliases $7F00
        assert_eq!(m.cpu_read(0x7F80), 0x42);

        m.cpu_write(0x7EF8, 0x00);
        assert_eq!(m.cpu_read(0x7F00), 0);
    }

    #[test]
    fn test_mapper207_nt_from_chr_regs() {
        let mut m = make_mapper(true);

        m.cpu_write(0x7EF0, 0x80); // NT0/NT1 -> page 1
        m.cpu_write(0x7EF1, 0x00); // NT2/NT3 -> page 0

        m.ppu_write(0x2000, 0xAA);
        assert_eq!(m.ppu_read(0x2400), 0xAA);
        assert_eq!(m.ppu_read(0x2800), 0x00);

        // Mirroring reg is ignored on 207
        m.cpu_write(0x7EF6, 0x01);
        assert_eq!(m.ppu_read(0x2400), 0xAA);
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut m = make_mapper(false);
        m.cpu_write(0x7EFA, 2);
        m.cpu_write(0x7EF8, 0xA3);
        m.cpu_write(0x7F10, 0x55);

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);
        let data = w.finish();

        let mut m2 = make_mapper(false);
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.cpu_read(0x8000), m.cpu_read(0x8000));
        assert_eq!(m2.cpu_read(0x7F10), 0x55);
    }
}
