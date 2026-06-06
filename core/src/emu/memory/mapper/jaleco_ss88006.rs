use super::super::super::io;
use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 8 * 1024;
const CHR_BANK_SIZE: usize = 1024;

pub struct JalecoSS88006Mapper {
    _cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    cpu_ram_ptr: *mut u8,

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    prg_regs: [usize; 3],

    prg_ram: Box<[u8; PRG_RAM_8K]>,
    prg_ram_enabled: bool,
    prg_ram_writable: bool,
    has_battery: bool,

    chr_rom: Vec<[u8; CHR_BANK_SIZE]>,
    chr_regs: [usize; 8],

    mirroring: NametableMirror,

    irq_latch: u16,
    irq_counter: u16,
    irq_enabled: bool,
    irq_pending: bool,
    irq_width_mask: u16,

    _vram: Box<[u8; VRAM_SIZE as usize]>,
    vram_ptr: *mut u8,
    palette_ram: [u8; PALETTE_SIZE],

    pub controllers: [controller::Controller; 2],
}

impl JalecoSS88006Mapper {
    pub fn new(
        flags: u8,
        prg_banks_16k: Vec<[u8; 16 * 1024]>,
        chr_banks_8k: Vec<[u8; io::loader::CHR_BANK_SIZE]>,
        has_battery: bool,
        sram_data: Option<Vec<u8>>,
    ) -> Self {
        let mut prg_rom: Vec<[u8; PRG_BANK_SIZE]> = Vec::new();
        for bank in &prg_banks_16k {
            let mut lo = [0u8; PRG_BANK_SIZE];
            let mut hi = [0u8; PRG_BANK_SIZE];
            lo.copy_from_slice(&bank[..PRG_BANK_SIZE]);
            hi.copy_from_slice(&bank[PRG_BANK_SIZE..]);
            prg_rom.push(lo);
            prg_rom.push(hi);
        }

        let mut chr_rom: Vec<[u8; CHR_BANK_SIZE]> = Vec::new();
        for bank in &chr_banks_8k {
            for i in 0..8 {
                let mut kb = [0u8; CHR_BANK_SIZE];
                kb.copy_from_slice(&bank[i * CHR_BANK_SIZE..(i + 1) * CHR_BANK_SIZE]);
                chr_rom.push(kb);
            }
        }

        let mut cpu_ram = Box::new([0u8; CPU_RAM_SIZE as usize]);
        let cpu_ram_ptr = cpu_ram.as_mut_ptr();

        let mut vram = Box::new([0u8; VRAM_SIZE as usize]);
        let vram_ptr = vram.as_mut_ptr();

        let mirroring = mirroring_from_flags(flags);

        JalecoSS88006Mapper {
            _cpu_ram: cpu_ram,
            cpu_ram_ptr,
            prg_rom,
            prg_regs: [0; 3],
            prg_ram: {
                let mut ram = Box::new([0; PRG_RAM_8K]);
                if let Some(data) = sram_data {
                    let len = data.len().min(PRG_RAM_8K);
                    ram[..len].copy_from_slice(&data[..len]);
                }
                ram
            },
            prg_ram_enabled: false,
            prg_ram_writable: false,
            has_battery,
            chr_rom,
            chr_regs: [0; 8],
            mirroring,
            irq_latch: 0,
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
            irq_width_mask: 0xFFFF,
            _vram: vram,
            vram_ptr,
            palette_ram: [0x0F; PALETTE_SIZE],
            controllers: [controller::Controller::new(), controller::Controller::new()],
        }
    }

    fn prg_bank(&self, idx: usize) -> usize {
        if self.prg_rom.is_empty() {
            return 0;
        }
        self.prg_regs[idx] % self.prg_rom.len()
    }

    fn chr_bank(&self, idx: usize) -> usize {
        if self.chr_rom.is_empty() {
            return 0;
        }
        self.chr_regs[idx] % self.chr_rom.len()
    }

    fn write_register(&mut self, addr: u16, value: u8) {
        let nibble = value as usize & 0x0F;
        let reg = ((addr >> 12) & 0x07) as u8;
        let sub = addr & 0x03;

        match (reg, sub) {
            // PRG bank 0 ($8000-$9FFF window)
            (0, 0) => self.prg_regs[0] = (self.prg_regs[0] & 0x30) | nibble,
            (0, 1) => self.prg_regs[0] = (self.prg_regs[0] & 0x0F) | ((nibble & 0x03) << 4),
            // PRG bank 1 ($A000-$BFFF window)
            (0, 2) => self.prg_regs[1] = (self.prg_regs[1] & 0x30) | nibble,
            (0, 3) => self.prg_regs[1] = (self.prg_regs[1] & 0x0F) | ((nibble & 0x03) << 4),
            // PRG bank 2 ($C000-$DFFF window)
            (1, 0) => self.prg_regs[2] = (self.prg_regs[2] & 0x30) | nibble,
            (1, 1) => self.prg_regs[2] = (self.prg_regs[2] & 0x0F) | ((nibble & 0x03) << 4),
            // PRG RAM control
            (1, 2) => {
                self.prg_ram_enabled = nibble & 0x01 != 0;
                self.prg_ram_writable = nibble & 0x02 != 0;
            }
            (1, 3) => {}
            // CHR banks 0-7: each pair of addresses (even=low, odd=high) controls one bank
            (2..=5, s) => {
                let base = ((reg - 2) as usize) * 2;
                let chr_idx = base + (s as usize >> 1);
                if s & 1 == 0 {
                    self.chr_regs[chr_idx] = (self.chr_regs[chr_idx] & 0xF0) | nibble;
                } else {
                    self.chr_regs[chr_idx] = (self.chr_regs[chr_idx] & 0x0F) | (nibble << 4);
                }
            }
            // IRQ latch
            (6, 0) => self.irq_latch = (self.irq_latch & 0xFFF0) | nibble as u16,
            (6, 1) => self.irq_latch = (self.irq_latch & 0xFF0F) | (nibble as u16) << 4,
            (6, 2) => self.irq_latch = (self.irq_latch & 0xF0FF) | (nibble as u16) << 8,
            (6, 3) => self.irq_latch = (self.irq_latch & 0x0FFF) | (nibble as u16) << 12,
            // IRQ control
            (7, 0) => {
                self.irq_counter = self.irq_latch;
                self.irq_pending = false;
            }
            (7, 1) => {
                self.irq_pending = false;
                self.irq_enabled = nibble & 0x01 != 0;
                // Width: F (bit 3) > E (bit 2) > T (bit 1)
                self.irq_width_mask = if nibble & 0x08 != 0 {
                    0x000F
                } else if nibble & 0x04 != 0 {
                    0x0FFF
                } else if nibble & 0x02 != 0 {
                    0x00FF
                } else {
                    0xFFFF
                };
            }
            // Mirroring
            (7, 2) => {
                self.mirroring = match nibble & 0x03 {
                    0 => NametableMirror::Horizontal,
                    1 => NametableMirror::Vertical,
                    2 => NametableMirror::Lower,
                    _ => NametableMirror::Higher,
                };
            }
            // Sound ($F003) — not emulated
            (7, 3) => {}
            _ => {}
        }
    }
}

impl MemoryMapper for JalecoSS88006Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);
        match page {
            0x00 | 0x10 => unsafe { *self.cpu_ram_ptr.offset(addr as _) },
            0x20 | 0x40 => 0,
            0x60 | 0x70 => {
                if self.prg_ram_enabled {
                    self.prg_ram[(addr - 0x6000) as usize]
                } else {
                    0
                }
            }
            0x80 | 0x90 => {
                let bank = self.prg_bank(0);
                self.prg_rom[bank][(addr - 0x8000) as usize]
            }
            0xA0 | 0xB0 => {
                let bank = self.prg_bank(1);
                self.prg_rom[bank][(addr - 0xA000) as usize]
            }
            0xC0 | 0xD0 => {
                let bank = self.prg_bank(2);
                self.prg_rom[bank][(addr - 0xC000) as usize]
            }
            0xE0 | 0xF0 => {
                let fixed = self.prg_rom.len() - 1;
                self.prg_rom[fixed][(addr - 0xE000) as usize]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);
        match page {
            0x00 | 0x10 => unsafe { *self.cpu_ram_ptr.offset(addr as _) = value },
            0x20 | 0x40 | 0x50 => {}
            0x60 | 0x70 => {
                if self.prg_ram_enabled && self.prg_ram_writable {
                    self.prg_ram[(addr - 0x6000) as usize] = value;
                }
            }
            0x80..=0xF0 => self.write_register(addr, value),
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        let addr = addr % MAX_VRAM_ADDR;
        if addr >= PALETTE_START {
            let mut idx = (addr as usize - PALETTE_START as usize) % PALETTE_SIZE;
            if idx & PALETTE_MIRROR_MASK == PALETTE_MIRROR_CLEAR {
                idx &= !PALETTE_MIRROR_CLEAR;
            }
            return self.palette_ram[idx];
        }
        match addr_to_page(addr) {
            0x00 | 0x10 => {
                if self.chr_rom.is_empty() {
                    return 0;
                }
                let chr_idx = (addr as usize) / CHR_BANK_SIZE;
                let bank = self.chr_bank(chr_idx);
                self.chr_rom[bank][(addr as usize) % CHR_BANK_SIZE]
            }
            0x20 | 0x30 => {
                let a = mirror_nametable_addr(addr, self.mirroring) % VRAM_SIZE;
                unsafe { *self.vram_ptr.offset(a as _) }
            }
            _ => 0,
        }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    unsafe fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        let addr = addr % MAX_VRAM_ADDR;
        match addr_to_page(addr) {
            0x00 | 0x10 => {
                if self.chr_rom.is_empty() {
                    return;
                }
                let chr_idx = (addr as usize) / CHR_BANK_SIZE;
                let bank = self.chr_bank(chr_idx);
                let offset = (addr as usize) % CHR_BANK_SIZE;
                unsafe { std::ptr::copy(self.chr_rom[bank].as_ptr().add(offset), dest, size) }
            }
            0x20 | 0x30 => {
                let a = mirror_nametable_addr(addr, self.mirroring) % VRAM_SIZE;
                unsafe { std::ptr::copy(self.vram_ptr.offset(a as _), dest, size) }
            }
            _ => {}
        }
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        let addr = addr % MAX_VRAM_ADDR;
        if addr >= PALETTE_START {
            let mut idx = (addr as usize - PALETTE_START as usize) % PALETTE_SIZE;
            if idx & PALETTE_MIRROR_MASK == PALETTE_MIRROR_CLEAR {
                idx &= !PALETTE_MIRROR_CLEAR;
            }
            self.palette_ram[idx] = value;
            return;
        }
        match addr_to_page(addr) {
            0x00 | 0x10 => {}
            0x20 | 0x30 => {
                let a = mirror_nametable_addr(addr, self.mirroring) % VRAM_SIZE;
                unsafe { *self.vram_ptr.offset(a as _) = value }
            }
            _ => {}
        }
    }

    fn code_start(&mut self) -> u16 {
        ((self.cpu_read(super::RESET_TARGET_ADDR + 1) as u16) << 8)
            + self.cpu_read(super::RESET_TARGET_ADDR) as u16
    }

    fn controllers(&mut self) -> &mut [controller::Controller; 2] {
        &mut self.controllers
    }

    fn poll_irq(&mut self) -> bool {
        self.irq_pending
    }

    fn cpu_cycle(&mut self, _ppu_dot: u64) {
        if !self.irq_enabled {
            return;
        }
        let active = self.irq_counter & self.irq_width_mask;
        let decremented = active.wrapping_sub(1) & self.irq_width_mask;
        if decremented == 0 {
            self.irq_pending = true;
        }
        self.irq_counter = (self.irq_counter & !self.irq_width_mask) | decremented;
    }

    fn mapper_id(&self) -> u8 {
        18
    }

    fn sram_data(&self) -> Option<&[u8]> {
        if self.has_battery {
            Some(&self.prg_ram[..])
        } else {
            None
        }
    }

    fn sram_data_mut(&mut self) -> Option<&mut [u8]> {
        if self.has_battery {
            Some(&mut self.prg_ram[..])
        } else {
            None
        }
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let cpu_ram =
            unsafe { std::slice::from_raw_parts(self.cpu_ram_ptr, CPU_RAM_SIZE as usize) };
        w.write_bytes(cpu_ram);
        for r in &self.prg_regs {
            w.write_u8(*r as u8);
        }
        w.write_bytes(&*self.prg_ram);
        w.write_u8(self.prg_ram_enabled as u8);
        w.write_u8(self.prg_ram_writable as u8);
        for r in &self.chr_regs {
            w.write_u8(*r as u8);
        }
        save_mirroring(w, self.mirroring);
        w.write_u16(self.irq_latch);
        w.write_u16(self.irq_counter);
        w.write_u8(self.irq_enabled as u8);
        w.write_u8(self.irq_pending as u8);
        w.write_u16(self.irq_width_mask);
        let vram = unsafe { std::slice::from_raw_parts(self.vram_ptr, VRAM_SIZE as usize) };
        w.write_bytes(vram);
        w.write_bytes(&self.palette_ram);
        save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let cpu_ram =
            unsafe { std::slice::from_raw_parts_mut(self.cpu_ram_ptr, CPU_RAM_SIZE as usize) };
        r.read_bytes_into(cpu_ram)?;
        for reg in &mut self.prg_regs {
            *reg = r.read_u8()? as usize;
        }
        r.read_bytes_into(&mut *self.prg_ram)?;
        self.prg_ram_enabled = r.read_u8()? != 0;
        self.prg_ram_writable = r.read_u8()? != 0;
        for reg in &mut self.chr_regs {
            *reg = r.read_u8()? as usize;
        }
        self.mirroring = load_mirroring(r)?;
        self.irq_latch = r.read_u16()?;
        self.irq_counter = r.read_u16()?;
        self.irq_enabled = r.read_u8()? != 0;
        self.irq_pending = r.read_u8()? != 0;
        self.irq_width_mask = r.read_u16()?;
        let vram = unsafe { std::slice::from_raw_parts_mut(self.vram_ptr, VRAM_SIZE as usize) };
        r.read_bytes_into(vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper(prg_16k_count: usize, chr_8k_count: usize) -> JalecoSS88006Mapper {
        let mut prg_banks = Vec::new();
        for i in 0..prg_16k_count {
            let mut bank = [0u8; 16 * 1024];
            bank[0] = i as u8;
            prg_banks.push(bank);
        }
        let mut chr_banks = Vec::new();
        for i in 0..chr_8k_count {
            let mut bank = [0u8; io::loader::CHR_BANK_SIZE];
            for k in 0..8 {
                bank[k * 1024] = (i * 8 + k) as u8;
            }
            chr_banks.push(bank);
        }
        JalecoSS88006Mapper::new(0x01, prg_banks, chr_banks, false, None)
    }

    #[test]
    fn test_prg_nibble_banking() {
        let mut m = make_mapper(16, 8);
        // Set PRG bank 0 to 0x15 (low nibble 5, high nibble 1)
        m.cpu_write(0x8000, 0x05);
        m.cpu_write(0x8001, 0x01);
        assert_eq!(m.prg_regs[0], 0x15);
    }

    #[test]
    fn test_chr_nibble_banking() {
        let mut m = make_mapper(16, 32);
        // Set CHR bank 0 to 0xAB: $A000=low, $A001=high
        m.cpu_write(0xA000, 0x0B);
        m.cpu_write(0xA001, 0x0A);
        assert_eq!(m.chr_regs[0], 0xAB);

        // CHR bank 1: $A002=low, $A003=high
        m.cpu_write(0xA002, 0x0C);
        m.cpu_write(0xA003, 0x03);
        assert_eq!(m.chr_regs[1], 0x3C);

        // CHR bank 2: $B000=low, $B001=high
        m.cpu_write(0xB000, 0x05);
        m.cpu_write(0xB001, 0x02);
        assert_eq!(m.chr_regs[2], 0x25);
    }

    #[test]
    fn test_prg_ram_enable() {
        let mut m = make_mapper(16, 8);
        // PRG RAM disabled by default
        m.cpu_write(0x6000, 0x42);
        assert_eq!(m.cpu_read(0x6000), 0);

        // Enable read only
        m.cpu_write(0x9002, 0x01);
        m.prg_ram[0] = 0x99;
        assert_eq!(m.cpu_read(0x6000), 0x99);

        // Write still blocked
        m.cpu_write(0x6000, 0x42);
        assert_eq!(m.cpu_read(0x6000), 0x99);

        // Enable write
        m.cpu_write(0x9002, 0x03);
        m.cpu_write(0x6000, 0x42);
        assert_eq!(m.cpu_read(0x6000), 0x42);
    }

    #[test]
    fn test_irq_16bit_countdown() {
        let mut m = make_mapper(16, 8);
        // Set latch to 3
        m.cpu_write(0xE000, 0x03);
        // Reload and enable (16-bit mode)
        m.cpu_write(0xF000, 0x00);
        m.cpu_write(0xF001, 0x01);

        assert!(!m.irq_pending);
        m.cpu_cycle(0); // 3 -> 2
        assert!(!m.irq_pending);
        m.cpu_cycle(0); // 2 -> 1
        assert!(!m.irq_pending);
        m.cpu_cycle(0); // 1 -> 0 (fires)
        assert!(m.irq_pending);
    }

    #[test]
    fn test_irq_4bit_mode() {
        let mut m = make_mapper(16, 8);
        // Set latch to 0x0012 — in 4-bit mode only low nibble (2) counts
        m.cpu_write(0xE000, 0x02);
        m.cpu_write(0xE001, 0x01);
        // Reload
        m.cpu_write(0xF000, 0x00);
        // Enable with 4-bit mode (F bit = 0x08)
        m.cpu_write(0xF001, 0x09);

        assert!(!m.irq_pending);
        m.cpu_cycle(0); // 2 -> 1
        assert!(!m.irq_pending);
        m.cpu_cycle(0); // 1 -> 0 (fires)
        assert!(m.irq_pending);
    }

    #[test]
    fn test_irq_acknowledge() {
        let mut m = make_mapper(16, 8);
        m.irq_pending = true;
        m.cpu_write(0xF000, 0x00); // acknowledge
        assert!(!m.irq_pending);
    }

    #[test]
    fn test_mirroring() {
        let mut m = make_mapper(16, 8);
        m.cpu_write(0xF002, 0x00);
        assert_eq!(m.mirroring, NametableMirror::Horizontal);
        m.cpu_write(0xF002, 0x01);
        assert_eq!(m.mirroring, NametableMirror::Vertical);
        m.cpu_write(0xF002, 0x02);
        assert_eq!(m.mirroring, NametableMirror::Lower);
        m.cpu_write(0xF002, 0x03);
        assert_eq!(m.mirroring, NametableMirror::Higher);
    }

    #[test]
    fn test_fixed_last_prg() {
        let mut m = make_mapper(16, 8);
        // 32 x 8KB banks, last is index 31
        m.prg_rom[31][0] = 0xDD;
        assert_eq!(m.cpu_read(0xE000), 0xDD);
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut m = make_mapper(16, 8);
        m.cpu_write(0x8000, 0x05);
        m.cpu_write(0x8001, 0x01);
        m.cpu_write(0xA000, 0x0B);
        m.cpu_write(0xA001, 0x0A);
        m.cpu_write(0xF002, 0x02);
        m.irq_pending = true;
        m.irq_counter = 0x1234;

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);

        let mut m2 = make_mapper(16, 8);
        let data = w.finish();
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.prg_regs[0], 0x15);
        assert_eq!(m2.chr_regs[0], 0xAB);
        assert_eq!(m2.mirroring, NametableMirror::Lower);
        assert!(m2.irq_pending);
        assert_eq!(m2.irq_counter, 0x1234);
    }
}
