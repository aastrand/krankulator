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

pub struct IremH3001Mapper {
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_rom: Vec<[u8; CHR_BANK_SIZE]>,

    prg_regs: [u8; 3],
    chr_regs: [u8; 8],
    mirroring: NametableMirror,

    irq_enabled: bool,
    irq_counter: u16,
    irq_reload: u16,
    irq_pending: bool,

    vram: Vec<u8>,
    cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    palette_ram: [u8; PALETTE_SIZE],
}

impl IremH3001Mapper {
    pub fn new(flags: u8, prg_banks_16k: Vec<[u8; 16384]>, chr_banks_8k: Vec<[u8; 8192]>) -> Self {
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

        // Power-on: banks 0, 1, second-last; games rely on these values
        let second_last = prg_rom.len().saturating_sub(2) as u8;

        IremH3001Mapper {
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            chr_rom,
            prg_regs: [0, 1, second_last],
            chr_regs: [0, 1, 2, 3, 4, 5, 6, 7],
            mirroring: mirroring_from_flags(flags),
            irq_enabled: false,
            irq_counter: 0,
            irq_reload: 0,
            irq_pending: false,
            vram: vec![0; VRAM_SIZE as usize],
            cpu_ram: Box::new([0; CPU_RAM_SIZE as usize]),
            palette_ram: [0x0F; PALETTE_SIZE],
        }
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
        self.chr_regs[slot] as usize % self.chr_rom.len().max(1)
    }

    fn nt_addr(&self, addr: u16) -> usize {
        mirror_nametable_addr(addr, self.mirroring) as usize & 0x7FF
    }

    fn write_register(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000 => self.prg_regs[0] = value,
            0x9001 => {
                self.mirroring = if value & 0x80 != 0 {
                    NametableMirror::Horizontal
                } else {
                    NametableMirror::Vertical
                };
            }
            0x9003 => {
                self.irq_enabled = value & 0x80 != 0;
                self.irq_pending = false;
            }
            0x9004 => {
                self.irq_counter = self.irq_reload;
                self.irq_pending = false;
            }
            0x9005 => self.irq_reload = (self.irq_reload & 0x00FF) | ((value as u16) << 8),
            0x9006 => self.irq_reload = (self.irq_reload & 0xFF00) | value as u16,
            0xA000 => self.prg_regs[1] = value,
            0xB000..=0xB007 => self.chr_regs[(addr & 7) as usize] = value,
            0xC000 => self.prg_regs[2] = value,
            _ => {}
        }
    }
}

impl MemoryMapper for IremH3001Mapper {
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
            0x8000..=0xFFFF => self.write_register(addr, value),
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

    fn cpu_cycle(&mut self, _ppu_dot: u64) {
        if self.irq_enabled {
            self.irq_counter = self.irq_counter.wrapping_sub(1);
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
        65
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&*self.cpu_ram);
        w.write_bytes(&self.vram);
        w.write_bytes(&self.palette_ram);
        for &b in &self.prg_regs {
            w.write_u8(b);
        }
        for &b in &self.chr_regs {
            w.write_u8(b);
        }
        w.write_bool(self.irq_enabled);
        w.write_u16(self.irq_counter);
        w.write_u16(self.irq_reload);
        w.write_bool(self.irq_pending);
        super::save_mirroring(w, self.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut *self.cpu_ram)?;
        r.read_bytes_into(&mut self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        for b in &mut self.prg_regs {
            *b = r.read_u8()?;
        }
        for b in &mut self.chr_regs {
            *b = r.read_u8()?;
        }
        self.irq_enabled = r.read_bool()?;
        self.irq_counter = r.read_u16()?;
        self.irq_reload = r.read_u16()?;
        self.irq_pending = r.read_bool()?;
        self.mirroring = super::load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper() -> IremH3001Mapper {
        let mut prg = Vec::new();
        for i in 0..8u8 {
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
        IremH3001Mapper::new(0, prg, chr)
    }

    #[test]
    fn test_power_on_banks() {
        let mut m = make_mapper();
        assert_eq!(m.cpu_read(0x8000), 0);
        assert_eq!(m.cpu_read(0xA000), 1);
        assert_eq!(m.cpu_read(0xC000), 14); // second-last of 16
        assert_eq!(m.cpu_read(0xE000), 15);
    }

    #[test]
    fn test_prg_and_chr_banking() {
        let mut m = make_mapper();
        m.cpu_write(0x8000, 5);
        m.cpu_write(0xA000, 6);
        m.cpu_write(0xC000, 7);
        assert_eq!(m.cpu_read(0x8000), 5);
        assert_eq!(m.cpu_read(0xA000), 6);
        assert_eq!(m.cpu_read(0xC000), 7);

        m.cpu_write(0xB003, 9);
        assert_eq!(m.ppu_read(0x0C00), 9);
    }

    #[test]
    fn test_irq_countdown() {
        let mut m = make_mapper();
        m.cpu_write(0x9005, 0x00); // reload high
        m.cpu_write(0x9006, 0x05); // reload low
        m.cpu_write(0x9004, 0); // copy reload to counter
        m.cpu_write(0x9003, 0x80); // enable

        for _ in 0..4 {
            m.cpu_cycle(0);
            assert!(!m.poll_irq());
        }
        m.cpu_cycle(0);
        assert!(m.poll_irq());
        // Fires once, then disables itself
        m.cpu_cycle(0);
        assert!(!m.poll_irq());
    }

    #[test]
    fn test_irq_ack() {
        let mut m = make_mapper();
        m.cpu_write(0x9005, 0);
        m.cpu_write(0x9006, 1);
        m.cpu_write(0x9004, 0);
        m.cpu_write(0x9003, 0x80);
        m.cpu_cycle(0);
        // Ack via $9003 before polling
        m.cpu_write(0x9003, 0x00);
        assert!(!m.poll_irq());
    }
}
