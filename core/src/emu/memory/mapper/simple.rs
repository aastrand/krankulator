use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_16K: usize = 16 * 1024;
const PRG_32K: usize = 32 * 1024;
const CHR_8K: usize = 8 * 1024;
const CHR_4K: usize = 4 * 1024;
const PRG_ROM_ADDR: usize = 0x8000;

#[derive(Copy, Clone, PartialEq)]
enum Type {
    M70,
    M78,
    M86,
    M87,
    M89,
    M93,
    M94,
    M97,
    M152,
    M184,
    M185,
    M140,
    M180,
}

pub struct SimpleMapper {
    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    mapper_type: Type,
    prg_banks_16k: Vec<[u8; PRG_16K]>,
    prg_banks_32k: Vec<[u8; PRG_32K]>,
    chr_banks_8k: Vec<[u8; CHR_8K]>,
    chr_banks_4k: Vec<[u8; CHR_4K]>,
    selected_prg: usize,
    selected_chr: usize,
    selected_chr_hi: usize,

    chr_enabled: bool,
    submapper: u8,

    ppu: PpuBus,
    pub controllers: [controller::Controller; 2],
}

impl SimpleMapper {
    fn new_inner(
        mapper_type: Type,
        prg_banks_16k: Vec<[u8; PRG_16K]>,
        chr_banks_8k: Vec<[u8; CHR_8K]>,
        mirroring: NametableMirror,
    ) -> Self {
        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);

        let mut prg_banks_32k: Vec<[u8; PRG_32K]> = Vec::new();
        for i in (0..prg_banks_16k.len()).step_by(2) {
            let mut bank = [0; PRG_32K];
            bank[0..PRG_16K].copy_from_slice(&prg_banks_16k[i]);
            let j = if i + 1 < prg_banks_16k.len() {
                i + 1
            } else {
                i
            };
            bank[PRG_16K..PRG_32K].copy_from_slice(&prg_banks_16k[j]);
            prg_banks_32k.push(bank);
        }

        match mapper_type {
            Type::M180 => {
                if !prg_banks_16k.is_empty() {
                    mem[PRG_ROM_ADDR..PRG_ROM_ADDR + PRG_16K].copy_from_slice(&prg_banks_16k[0]);
                    let last = prg_banks_16k.len() - 1;
                    mem[PRG_ROM_ADDR + PRG_16K..PRG_ROM_ADDR + PRG_32K]
                        .copy_from_slice(&prg_banks_16k[last]);
                }
            }
            Type::M78 | Type::M152 | Type::M70 | Type::M89 | Type::M93 | Type::M94 => {
                if !prg_banks_16k.is_empty() {
                    mem[PRG_ROM_ADDR..PRG_ROM_ADDR + PRG_16K].copy_from_slice(&prg_banks_16k[0]);
                    let last = prg_banks_16k.len() - 1;
                    mem[PRG_ROM_ADDR + PRG_16K..PRG_ROM_ADDR + PRG_32K]
                        .copy_from_slice(&prg_banks_16k[last]);
                }
            }
            Type::M97 => {
                if !prg_banks_16k.is_empty() {
                    let last = prg_banks_16k.len() - 1;
                    mem[PRG_ROM_ADDR..PRG_ROM_ADDR + PRG_16K].copy_from_slice(&prg_banks_16k[last]);
                    mem[PRG_ROM_ADDR + PRG_16K..PRG_ROM_ADDR + PRG_32K]
                        .copy_from_slice(&prg_banks_16k[0]);
                }
            }
            _ => {
                if !prg_banks_32k.is_empty() {
                    mem[PRG_ROM_ADDR..PRG_ROM_ADDR + PRG_32K].copy_from_slice(&prg_banks_32k[0]);
                }
            }
        }

        let addr_space_ptr = mem.as_mut_ptr();

        let mut chr_banks_4k: Vec<[u8; CHR_4K]> = Vec::new();
        for bank in &chr_banks_8k {
            chr_banks_4k.push(<[u8; CHR_4K]>::try_from(&bank[..CHR_4K]).unwrap());
            chr_banks_4k.push(<[u8; CHR_4K]>::try_from(&bank[CHR_4K..]).unwrap());
        }

        let ppu = if chr_banks_8k.is_empty() {
            PpuBus::new_ram(CHR_8K, mirroring)
        } else {
            PpuBus::new_rom(&chr_banks_8k[0], mirroring)
        };

        SimpleMapper {
            _addr_space: mem,
            addr_space_ptr,
            mapper_type,
            prg_banks_16k,
            prg_banks_32k,
            chr_banks_8k,
            chr_banks_4k,
            selected_prg: 0,
            selected_chr: 0,
            selected_chr_hi: 1,
            chr_enabled: mapper_type != Type::M185 && mapper_type != Type::M93,
            submapper: 0,
            ppu,
            controllers: [controller::Controller::new(), controller::Controller::new()],
        }
    }

    pub fn mapper78(
        flags: u8,
        prg: Vec<[u8; PRG_16K]>,
        chr: Vec<[u8; CHR_8K]>,
        submapper: u8,
    ) -> Self {
        let mirroring = if submapper == 3 {
            mirroring_from_flags(flags)
        } else {
            NametableMirror::Lower
        };
        let mut m = Self::new_inner(Type::M78, prg, chr, mirroring);
        m.submapper = submapper;
        m
    }

    pub fn mapper87(flags: u8, prg: Vec<[u8; PRG_16K]>, chr: Vec<[u8; CHR_8K]>) -> Self {
        Self::new_inner(Type::M87, prg, chr, mirroring_from_flags(flags))
    }

    pub fn mapper185(
        flags: u8,
        prg: Vec<[u8; PRG_16K]>,
        chr: Vec<[u8; CHR_8K]>,
        submapper: u8,
    ) -> Self {
        let mut m = Self::new_inner(Type::M185, prg, chr, mirroring_from_flags(flags));
        m.submapper = submapper;
        m
    }

    pub fn mapper152(prg: Vec<[u8; PRG_16K]>, chr: Vec<[u8; CHR_8K]>) -> Self {
        Self::new_inner(Type::M152, prg, chr, NametableMirror::Lower)
    }

    pub fn mapper70(flags: u8, prg: Vec<[u8; PRG_16K]>, chr: Vec<[u8; CHR_8K]>) -> Self {
        Self::new_inner(Type::M70, prg, chr, mirroring_from_flags(flags))
    }

    pub fn mapper86(flags: u8, prg: Vec<[u8; PRG_16K]>, chr: Vec<[u8; CHR_8K]>) -> Self {
        Self::new_inner(Type::M86, prg, chr, mirroring_from_flags(flags))
    }

    pub fn mapper89(prg: Vec<[u8; PRG_16K]>, chr: Vec<[u8; CHR_8K]>) -> Self {
        Self::new_inner(Type::M89, prg, chr, NametableMirror::Lower)
    }

    pub fn mapper93(flags: u8, prg: Vec<[u8; PRG_16K]>, chr: Vec<[u8; CHR_8K]>) -> Self {
        Self::new_inner(Type::M93, prg, chr, mirroring_from_flags(flags))
    }

    pub fn mapper94(flags: u8, prg: Vec<[u8; PRG_16K]>, chr: Vec<[u8; CHR_8K]>) -> Self {
        Self::new_inner(Type::M94, prg, chr, mirroring_from_flags(flags))
    }

    pub fn mapper97(flags: u8, prg: Vec<[u8; PRG_16K]>, chr: Vec<[u8; CHR_8K]>) -> Self {
        Self::new_inner(Type::M97, prg, chr, mirroring_from_flags(flags))
    }

    pub fn mapper184(flags: u8, prg: Vec<[u8; PRG_16K]>, chr: Vec<[u8; CHR_8K]>) -> Self {
        Self::new_inner(Type::M184, prg, chr, mirroring_from_flags(flags))
    }

    pub fn mapper140(flags: u8, prg: Vec<[u8; PRG_16K]>, chr: Vec<[u8; CHR_8K]>) -> Self {
        Self::new_inner(Type::M140, prg, chr, mirroring_from_flags(flags))
    }

    pub fn mapper180(flags: u8, prg: Vec<[u8; PRG_16K]>, chr: Vec<[u8; CHR_8K]>) -> Self {
        Self::new_inner(Type::M180, prg, chr, mirroring_from_flags(flags))
    }

    fn bus_conflict(&self, addr: u16, value: u8) -> u8 {
        let rom_byte = unsafe { *self.addr_space_ptr.offset(addr as isize) };
        value & rom_byte
    }

    fn switch_chr_8k(&mut self, bank: usize) {
        let bank_index = bank % self.chr_banks_8k.len().max(1);
        if bank_index != self.selected_chr {
            self.selected_chr = bank_index;
            self.ppu.switch_chr_bank(&self.chr_banks_8k, bank_index);
        }
    }

    fn rebuild_chr_from_4k(&mut self, lo: usize, hi: usize) {
        if self.chr_banks_4k.is_empty() {
            return;
        }
        self.selected_chr = lo;
        self.selected_chr_hi = hi;
        let lo_idx = lo % self.chr_banks_4k.len();
        let hi_idx = hi % self.chr_banks_4k.len();
        let mut combined = [0u8; CHR_8K];
        combined[..CHR_4K].copy_from_slice(&self.chr_banks_4k[lo_idx]);
        combined[CHR_4K..].copy_from_slice(&self.chr_banks_4k[hi_idx]);
        self.ppu.switch_chr_bank(&[combined], 0);
    }

    fn switch_prg_16k_low(&mut self, bank: usize) {
        let bank_index = bank % self.prg_banks_16k.len().max(1);
        self.selected_prg = bank_index;
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.prg_banks_16k[bank_index].as_ptr(),
                self.addr_space_ptr.add(PRG_ROM_ADDR),
                PRG_16K,
            );
        }
    }

    fn switch_prg_16k_high(&mut self, bank: usize) {
        let bank_index = bank % self.prg_banks_16k.len().max(1);
        self.selected_prg = bank_index;
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.prg_banks_16k[bank_index].as_ptr(),
                self.addr_space_ptr.add(PRG_ROM_ADDR + PRG_16K),
                PRG_16K,
            );
        }
    }

    fn switch_prg_32k(&mut self, bank: usize) {
        let bank_index = bank % self.prg_banks_32k.len().max(1);
        self.selected_prg = bank_index;
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.prg_banks_32k[bank_index].as_ptr(),
                self.addr_space_ptr.add(PRG_ROM_ADDR),
                PRG_32K,
            );
        }
    }
}

impl MemoryMapper for SimpleMapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = mirror_addr(addr);
        unsafe { *self.addr_space_ptr.offset(addr as isize) }
    }

    fn cpu_peek(&self, addr: u16) -> u8 {
        let addr = mirror_addr(addr);
        unsafe { *self.addr_space_ptr.offset(addr as isize) }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);

        match self.mapper_type {
            Type::M78 => match page {
                0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
                0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                    let value = self.bus_conflict(addr, value);
                    if self.submapper == 3 {
                        self.ppu.mirroring = if value & 0x08 != 0 {
                            NametableMirror::Horizontal
                        } else {
                            NametableMirror::Vertical
                        };
                    } else {
                        self.ppu.mirroring = if value & 0x08 != 0 {
                            NametableMirror::Higher
                        } else {
                            NametableMirror::Lower
                        };
                    }
                    self.switch_prg_16k_low((value & 0x07) as usize);
                    self.switch_chr_8k(((value >> 4) & 0x0F) as usize);
                }
                _ => {}
            },
            Type::M87 => match page {
                0x0 | 0x10 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
                0x60 | 0x70 => {
                    let bank = ((value & 1) << 1) | ((value >> 1) & 1);
                    self.switch_chr_8k(bank as usize);
                }
                _ => {}
            },
            Type::M185 => match page {
                0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
                0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                    let value = self.bus_conflict(addr, value);
                    self.chr_enabled = match self.submapper {
                        4 => (value & 0x03) == 0x00,
                        5 => (value & 0x03) == 0x01,
                        6 => (value & 0x03) == 0x02,
                        7 => (value & 0x03) == 0x03,
                        _ => (value & 0x33) != 0x00,
                    };
                }
                _ => {}
            },
            Type::M70 => match page {
                0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
                0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                    let value = self.bus_conflict(addr, value);
                    if !self.chr_banks_8k.is_empty() {
                        self.switch_chr_8k((value & 0x0F) as usize);
                    }
                    self.switch_prg_16k_low(((value >> 4) & 0x0F) as usize);
                }
                _ => {}
            },
            Type::M86 => match page {
                0x0 | 0x10 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
                0x60 => {
                    self.switch_prg_32k(((value >> 4) & 0x03) as usize);
                    if !self.chr_banks_8k.is_empty() {
                        let chr = (((value >> 6) & 0x01) << 2) | (value & 0x03);
                        self.switch_chr_8k(chr as usize);
                    }
                }
                _ => {}
            },
            Type::M89 => match page {
                0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
                0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                    let value = self.bus_conflict(addr, value);
                    self.ppu.mirroring = if value & 0x08 != 0 {
                        NametableMirror::Higher
                    } else {
                        NametableMirror::Lower
                    };
                    self.switch_prg_16k_low(((value >> 4) & 0x07) as usize);
                    if !self.chr_banks_8k.is_empty() {
                        let chr = (value & 0x07) | ((value >> 4) & 0x08);
                        self.switch_chr_8k(chr as usize);
                    }
                }
                _ => {}
            },
            Type::M93 => match page {
                0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
                0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                    let value = self.bus_conflict(addr, value);
                    self.switch_prg_16k_low(((value >> 4) & 0x07) as usize);
                    self.chr_enabled = value & 0x01 != 0;
                }
                _ => {}
            },
            Type::M94 => match page {
                0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
                0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                    let value = self.bus_conflict(addr, value);
                    self.switch_prg_16k_low(((value >> 2) & 0x07) as usize);
                }
                _ => {}
            },
            Type::M97 => match page {
                0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
                0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                    self.ppu.mirroring = match (value >> 6) & 0x03 {
                        0 => NametableMirror::Lower,
                        1 => NametableMirror::Horizontal,
                        2 => NametableMirror::Vertical,
                        _ => NametableMirror::Higher,
                    };
                    self.switch_prg_16k_high((value & 0x0F) as usize);
                }
                _ => {}
            },
            Type::M152 => match page {
                0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
                0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                    let value = self.bus_conflict(addr, value);
                    self.ppu.mirroring = if value & 0x80 != 0 {
                        NametableMirror::Higher
                    } else {
                        NametableMirror::Lower
                    };
                    let chr = (value & 0x0F) as usize;
                    if !self.chr_banks_8k.is_empty() {
                        self.switch_chr_8k(chr);
                    }
                    self.switch_prg_16k_low(((value >> 4) & 0x07) as usize);
                }
                _ => {}
            },
            Type::M184 => match page {
                0x0 | 0x10 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
                0x60 | 0x70 => {
                    let lo = (value & 0x07) as usize;
                    let hi = 4 + ((value >> 4) & 0x07) as usize;
                    self.rebuild_chr_from_4k(lo, hi);
                }
                _ => {}
            },
            Type::M140 => match page {
                0x0 | 0x10 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
                0x60 | 0x70 => {
                    let prg = ((value >> 4) & 0x03) as usize;
                    let chr = (value & 0x0F) as usize;
                    self.switch_prg_32k(prg);
                    if !self.chr_banks_8k.is_empty() {
                        self.switch_chr_8k(chr);
                    }
                }
                _ => {}
            },
            Type::M180 => match page {
                0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
                0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                    let value = self.bus_conflict(addr, value);
                    self.switch_prg_16k_high((value & 0x07) as usize);
                }
                _ => {}
            },
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        if !self.chr_enabled && addr < 0x2000 {
            return 0xFF;
        }
        self.ppu.read(addr)
    }

    unsafe fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        if !self.chr_enabled && addr < 0x2000 {
            unsafe { std::ptr::write_bytes(dest, 0xFF, size) }
            return;
        }
        self.ppu.copy(addr, dest, size);
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        if self.mapper_type == Type::M93 && !self.chr_enabled && addr < 0x2000 {
            return;
        }
        self.ppu.write(addr, value);
    }

    fn code_start(&mut self) -> u16 {
        ((self.cpu_read(super::RESET_TARGET_ADDR + 1) as u16) << 8)
            + self.cpu_read(super::RESET_TARGET_ADDR) as u16
    }

    fn controllers(&mut self) -> &mut [controller::Controller; 2] {
        &mut self.controllers
    }

    fn poll_irq(&mut self) -> bool {
        false
    }

    fn mapper_id(&self) -> u8 {
        match self.mapper_type {
            Type::M70 => 70,
            Type::M78 => 78,
            Type::M86 => 86,
            Type::M87 => 87,
            Type::M89 => 89,
            Type::M93 => 93,
            Type::M94 => 94,
            Type::M97 => 97,
            Type::M140 => 140,
            Type::M152 => 152,
            Type::M180 => 180,
            Type::M184 => 184,
            Type::M185 => 185,
        }
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let ram = unsafe { std::slice::from_raw_parts(self.addr_space_ptr, MAX_RAM_SIZE) };
        w.write_bytes(ram);
        self.ppu.save_state(w);
        w.write_u8(self.selected_prg as u8);
        w.write_u8(self.selected_chr as u8);
        w.write_u8(self.selected_chr_hi as u8);
        w.write_bool(self.chr_enabled);
        save_mirroring(w, self.ppu.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let ram = unsafe { std::slice::from_raw_parts_mut(self.addr_space_ptr, MAX_RAM_SIZE) };
        r.read_bytes_into(ram)?;
        self.ppu.load_state(r)?;
        self.selected_prg = r.read_u8()? as usize;
        self.selected_chr = r.read_u8()? as usize;
        self.selected_chr_hi = r.read_u8()? as usize;
        self.chr_enabled = r.read_bool()?;
        self.ppu.mirroring = load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mapper87_chr_swap() {
        let prg = [0u8; PRG_16K];
        let chr0 = [0x11u8; CHR_8K];
        let chr1 = [0x22u8; CHR_8K];

        let mut m: Box<dyn MemoryMapper> =
            Box::new(SimpleMapper::mapper87(0, vec![prg, prg], vec![chr0, chr1]));

        assert_eq!(m.ppu_read(0x0000), 0x11);
        // value=2: bit0=0, bit1=1 => swapped=(0<<1)|(1)=1
        m.cpu_write(0x6000, 0x02);
        assert_eq!(m.ppu_read(0x0000), 0x22);
        // value=0 => bank 0
        m.cpu_write(0x6000, 0x00);
        assert_eq!(m.ppu_read(0x0000), 0x11);
    }

    #[test]
    fn test_mapper152_prg_and_mirroring() {
        let mut prg_banks = Vec::new();
        for i in 0..4u8 {
            let mut bank = [0xFFu8; PRG_16K];
            bank[0] = i;
            prg_banks.push(bank);
        }
        let chr = [0u8; CHR_8K];

        let mut m: Box<dyn MemoryMapper> = Box::new(SimpleMapper::mapper152(prg_banks, vec![chr]));

        m.cpu_write(0x8001, 0x20); // PRG=2, CHR=0, mirror=Lower (write to 0x8001 where ROM=0xFF)
        assert_eq!(m.cpu_read(0x8000), 2);
        // Last bank fixed at $C000
        assert_eq!(m.cpu_read(0xC000), 3);
    }

    #[test]
    fn test_mapper140_prg_chr() {
        let mut prg_banks = Vec::new();
        for i in 0..4u8 {
            let mut bank = [0u8; PRG_16K];
            bank[0] = i;
            prg_banks.push(bank);
        }
        let chr0 = [0xAAu8; CHR_8K];
        let chr1 = [0xBBu8; CHR_8K];

        let mut m: Box<dyn MemoryMapper> =
            Box::new(SimpleMapper::mapper140(0, prg_banks, vec![chr0, chr1]));

        m.cpu_write(0x6000, 0x11); // PRG bank 1, CHR bank 1
        assert_eq!(m.ppu_read(0x0000), 0xBB);
    }

    #[test]
    fn test_mapper180_fixed_first() {
        let mut prg_banks = Vec::new();
        for i in 0..4u8 {
            let mut bank = [0xFFu8; PRG_16K];
            bank[0] = i;
            prg_banks.push(bank);
        }

        let mut m: Box<dyn MemoryMapper> = Box::new(SimpleMapper::mapper180(0, prg_banks, vec![]));

        assert_eq!(m.cpu_read(0x8000), 0);
        m.cpu_write(0x8001, 2); // write to 0x8001 where ROM=0xFF for clean bus conflict
        assert_eq!(m.cpu_read(0xC000), 2);
        assert_eq!(m.cpu_read(0x8000), 0); // still fixed
    }

    #[test]
    fn test_mapper184_4k_chr() {
        let mut chr0 = [0u8; CHR_8K];
        chr0[..CHR_4K].fill(0x11); // 4K bank 0
        chr0[CHR_4K..].fill(0x22); // 4K bank 1
        let mut chr1 = [0u8; CHR_8K];
        chr1[..CHR_4K].fill(0x33); // 4K bank 2
        chr1[CHR_4K..].fill(0x44); // 4K bank 3

        let prg = [0u8; PRG_16K];
        let mut m: Box<dyn MemoryMapper> =
            Box::new(SimpleMapper::mapper184(0, vec![prg, prg], vec![chr0, chr1]));

        // Initial: lo=bank0, hi=bank1
        assert_eq!(m.ppu_read(0x0000), 0x11);

        // Write lo=1, hi=0 (with hi offset to second chip at index 4)
        // Actually mapper 184 hi bank adds 4, so value $01 => lo=1, hi=4
        m.cpu_write(0x6000, 0x01);
        assert_eq!(m.ppu_read(0x0000), 0x22); // 4K bank 1
    }

    fn numbered_prg(n: usize) -> Vec<[u8; PRG_16K]> {
        let mut banks = Vec::new();
        for i in 0..n {
            let mut bank = [0xFFu8; PRG_16K];
            bank[0] = i as u8;
            banks.push(bank);
        }
        banks
    }

    fn numbered_chr(n: usize) -> Vec<[u8; CHR_8K]> {
        (0..n).map(|i| [(i * 0x11) as u8; CHR_8K]).collect()
    }

    #[test]
    fn test_mapper70_prg_chr() {
        let mut m: Box<dyn MemoryMapper> =
            Box::new(SimpleMapper::mapper70(0, numbered_prg(8), numbered_chr(4)));

        assert_eq!(m.cpu_read(0x8000), 0);
        assert_eq!(m.cpu_read(0xC000), 7);
        m.cpu_write(0x8001, 0x23); // PRG 2, CHR 3 (ROM=0xFF at $8001)
        assert_eq!(m.cpu_read(0x8000), 2);
        assert_eq!(m.cpu_read(0xC000), 7);
        assert_eq!(m.ppu_read(0x0000), 0x33);
    }

    #[test]
    fn test_mapper86_prg_chr_split_field() {
        let mut m: Box<dyn MemoryMapper> =
            Box::new(SimpleMapper::mapper86(0, numbered_prg(8), numbered_chr(8)));

        // CHR bank = bit 6 << 2 | bits 1-0; PRG 32KB = bits 5-4 ($6000, no conflicts)
        m.cpu_write(0x6000, 0x45); // CHR = 4|1 = 5, PRG 32KB bank 0
        assert_eq!(m.ppu_read(0x0000), 0x55);
        m.cpu_write(0x6000, 0x10); // PRG 32KB bank 1
        assert_eq!(m.cpu_read(0x8000), 2);
    }

    #[test]
    fn test_mapper89_chr_bit7_and_mirroring() {
        let mut m: Box<dyn MemoryMapper> =
            Box::new(SimpleMapper::mapper89(numbered_prg(8), numbered_chr(16)));

        // [CPPP MCCC]: bit7 = CHR bit 3, bits 6-4 PRG, bit 3 one-screen select
        m.cpu_write(0x8001, 0x25); // PRG 2, CHR 5, 1ScA
        assert_eq!(m.cpu_read(0x8000), 2);
        assert_eq!(m.ppu_read(0x0000), 0x55);

        m.cpu_write(0x8001, 0x81); // CHR 9 (bit7 set)
        assert_eq!(m.ppu_read(0x0000), 0x99);

        // One-screen select
        m.cpu_write(0x8001, 0x00);
        m.ppu_write(0x2000, 0x11);
        m.cpu_write(0x8001, 0x08);
        m.ppu_write(0x2000, 0x22);
        assert_eq!(m.ppu_read(0x2C00), 0x22);
        m.cpu_write(0x8001, 0x00);
        assert_eq!(m.ppu_read(0x2C00), 0x11);
    }

    #[test]
    fn test_mapper93_prg_and_chr_ram_enable() {
        let mut m: Box<dyn MemoryMapper> =
            Box::new(SimpleMapper::mapper93(0, numbered_prg(8), vec![]));

        // CHR-RAM starts disabled: writes dropped, reads open bus
        m.ppu_write(0x0000, 0x42);
        assert_eq!(m.ppu_read(0x0000), 0xFF);

        m.cpu_write(0x8001, 0x31); // PRG 3, CHR-RAM enabled
        assert_eq!(m.cpu_read(0x8000), 3);
        assert_eq!(m.cpu_read(0xC000), 7);
        m.ppu_write(0x0000, 0x42);
        assert_eq!(m.ppu_read(0x0000), 0x42);

        m.cpu_write(0x8001, 0x30); // disable again
        m.ppu_write(0x0000, 0x99);
        assert_eq!(m.ppu_read(0x0000), 0xFF);
        m.cpu_write(0x8001, 0x31);
        assert_eq!(m.ppu_read(0x0000), 0x42);
    }

    #[test]
    fn test_mapper94_shifted_prg() {
        let mut m: Box<dyn MemoryMapper> =
            Box::new(SimpleMapper::mapper94(0, numbered_prg(8), vec![]));

        assert_eq!(m.cpu_read(0x8000), 0);
        m.cpu_write(0x8001, 3 << 2);
        assert_eq!(m.cpu_read(0x8000), 3);
        assert_eq!(m.cpu_read(0xC000), 7);
    }

    #[test]
    fn test_mapper97_fixed_8000_switch_c000() {
        let mut m: Box<dyn MemoryMapper> =
            Box::new(SimpleMapper::mapper97(0, numbered_prg(8), vec![]));

        // Power-on: last bank at $8000, bank 0 at $C000
        assert_eq!(m.cpu_read(0x8000), 7);
        assert_eq!(m.cpu_read(0xC000), 0);

        m.cpu_write(0xC000, 0x03); // register decoded across $8000-$FFFF
        assert_eq!(m.cpu_read(0xC000), 3);
        assert_eq!(m.cpu_read(0x8000), 7);

        // Mirroring bits 7-6: %01 horizontal, %10 vertical
        m.cpu_write(0x8000, 0x40);
        m.ppu_write(0x2000, 0x11);
        assert_eq!(m.ppu_read(0x2400), 0x11);
        m.cpu_write(0x8000, 0x80);
        assert_eq!(m.ppu_read(0x2800), 0x11);
    }

    #[test]
    fn test_mapper185_protection() {
        let prg = [0xFFu8; PRG_16K];
        let chr = [0x55u8; CHR_8K];

        let mut m: Box<dyn MemoryMapper> =
            Box::new(SimpleMapper::mapper185(0, vec![prg, prg], vec![chr], 0));

        // CHR starts disabled for mapper 185
        assert_eq!(m.ppu_read(0x0000), 0xFF);
        // Write nonzero value to enable CHR (submapper 0 heuristic)
        m.cpu_write(0x8000, 0x01);
        assert_eq!(m.ppu_read(0x0000), 0x55);
        // Write 0 to disable CHR
        m.cpu_write(0x8000, 0x00);
        assert_eq!(m.ppu_read(0x0000), 0xFF);
    }
}
