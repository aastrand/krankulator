use super::super::super::io;
use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: u16 = 4 * 1024;

pub struct MMC4Mapper {
    _cpu_ram: Box<[u8; CPU_RAM_SIZE as usize]>,
    cpu_ram_ptr: *mut u8,

    prg_banks: Vec<[u8; PRG_BANK_SIZE]>,
    prg_bank_idx: usize,

    prg_ram: Box<[u8; PRG_RAM_8K]>,
    has_battery: bool,

    chr_banks: Vec<[u8; CHR_BANK_SIZE as usize]>,
    chr_regs: [[usize; 2]; 2],
    latches: [usize; 2],

    mirroring: NametableMirror,

    _vram: Box<[u8; VRAM_SIZE as usize]>,
    vram_ptr: *mut u8,
    palette_ram: [u8; PALETTE_SIZE],

    pub controllers: [controller::Controller; 2],
}

impl MMC4Mapper {
    pub fn new(
        flags: u8,
        prg_banks_16k: Vec<[u8; PRG_BANK_SIZE]>,
        chr_banks_8k: Vec<[u8; io::loader::CHR_BANK_SIZE]>,
        has_battery: bool,
        sram_data: Option<Vec<u8>>,
    ) -> Self {
        let mut chr_banks: Vec<[u8; CHR_BANK_SIZE as usize]> = Vec::new();
        for bank in &chr_banks_8k {
            let mut lo = [0u8; CHR_BANK_SIZE as usize];
            let mut hi = [0u8; CHR_BANK_SIZE as usize];
            lo.copy_from_slice(&bank[..CHR_BANK_SIZE as usize]);
            hi.copy_from_slice(&bank[CHR_BANK_SIZE as usize..]);
            chr_banks.push(lo);
            chr_banks.push(hi);
        }

        let mut cpu_ram = Box::new([0u8; CPU_RAM_SIZE as usize]);
        let cpu_ram_ptr = cpu_ram.as_mut_ptr();

        let mut vram = Box::new([0u8; VRAM_SIZE as usize]);
        let vram_ptr = vram.as_mut_ptr();

        let mirroring = mirroring_from_flags(flags);

        MMC4Mapper {
            _cpu_ram: cpu_ram,
            cpu_ram_ptr,
            prg_banks: prg_banks_16k,
            prg_bank_idx: 0,
            prg_ram: {
                let mut ram = Box::new([0; PRG_RAM_8K]);
                if let Some(data) = sram_data {
                    let len = data.len().min(PRG_RAM_8K);
                    ram[..len].copy_from_slice(&data[..len]);
                }
                ram
            },
            has_battery,
            chr_banks,
            chr_regs: [[0; 2]; 2],
            latches: [1, 1],
            mirroring,
            _vram: vram,
            vram_ptr,
            palette_ram: [0x0F; PALETTE_SIZE],
            controllers: [controller::Controller::new(), controller::Controller::new()],
        }
    }

    fn active_chr_bank(&self, half: usize) -> usize {
        let bank = self.chr_regs[half][self.latches[half]];
        bank % self.chr_banks.len()
    }

    fn check_latch_trigger(&mut self, addr: u16) {
        match addr {
            0x0FD8..=0x0FDF => self.latches[0] = 0,
            0x0FE8..=0x0FEF => self.latches[0] = 1,
            0x1FD8..=0x1FDF => self.latches[1] = 0,
            0x1FE8..=0x1FEF => self.latches[1] = 1,
            _ => {}
        }
    }
}

impl MemoryMapper for MMC4Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);
        match page {
            0x00 | 0x10 => unsafe { *self.cpu_ram_ptr.offset(addr as _) },
            0x20 | 0x40 => 0,
            0x60 | 0x70 => self.prg_ram[(addr - 0x6000) as usize],
            0x80 | 0x90 | 0xA0 | 0xB0 => {
                self.prg_banks[self.prg_bank_idx][(addr - 0x8000) as usize]
            }
            0xC0 | 0xD0 | 0xE0 | 0xF0 => {
                let fixed = self.prg_banks.len() - 1;
                self.prg_banks[fixed][(addr - 0xC000) as usize]
            }
            _ => 0,
        }
    }

    fn cpu_peek(&self, addr: u16) -> u8 {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);
        match page {
            0x00 | 0x10 => unsafe { *self.cpu_ram_ptr.offset(addr as _) },
            0x20 | 0x40 => 0,
            0x60 | 0x70 => self.prg_ram[(addr - 0x6000) as usize],
            0x80 | 0x90 | 0xA0 | 0xB0 => {
                self.prg_banks[self.prg_bank_idx][(addr - 0x8000) as usize]
            }
            0xC0 | 0xD0 | 0xE0 | 0xF0 => {
                let fixed = self.prg_banks.len() - 1;
                self.prg_banks[fixed][(addr - 0xC000) as usize]
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
                self.prg_ram[(addr - 0x6000) as usize] = value;
            }
            0x80..=0xF0 => {
                let reg = (addr >> 12) & 0x0F;
                match reg {
                    0xA => {
                        self.prg_bank_idx = (value as usize & 0x0F) % self.prg_banks.len();
                    }
                    0xB => self.chr_regs[0][0] = value as usize & 0x1F,
                    0xC => self.chr_regs[0][1] = value as usize & 0x1F,
                    0xD => self.chr_regs[1][0] = value as usize & 0x1F,
                    0xE => self.chr_regs[1][1] = value as usize & 0x1F,
                    0xF => {
                        self.mirroring = if value & 1 != 0 {
                            NametableMirror::Horizontal
                        } else {
                            NametableMirror::Vertical
                        };
                    }
                    _ => {}
                }
            }
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
            0x00 => {
                let bank = self.active_chr_bank(0);
                self.chr_banks[bank][addr as usize]
            }
            0x10 => {
                let bank = self.active_chr_bank(1);
                self.chr_banks[bank][(addr - 0x1000) as usize]
            }
            0x20 | 0x30 => {
                let a = mirror_nametable_addr(addr, self.mirroring) % VRAM_SIZE;
                unsafe { *self.vram_ptr.offset(a as _) }
            }
            _ => 0,
        }
    }

    fn ppu_fetch(&mut self, addr: u16, _dot: u64) -> u8 {
        let addr = addr % MAX_VRAM_ADDR;
        let value = self.ppu_read(addr);
        if addr < 0x2000 {
            self.check_latch_trigger(addr);
        }
        value
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    unsafe fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        let addr = addr % MAX_VRAM_ADDR;
        match addr_to_page(addr) {
            0x00 => {
                let bank = self.active_chr_bank(0);
                unsafe {
                    std::ptr::copy(self.chr_banks[bank].as_ptr().offset(addr as _), dest, size)
                }
            }
            0x10 => {
                let bank = self.active_chr_bank(1);
                unsafe {
                    std::ptr::copy(
                        self.chr_banks[bank].as_ptr().offset((addr - 0x1000) as _),
                        dest,
                        size,
                    )
                }
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
        false
    }

    fn mapper_id(&self) -> u8 {
        10
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
        w.write_u8(self.prg_bank_idx as u8);
        w.write_bytes(&*self.prg_ram);
        for half in 0..2 {
            for latch in 0..2 {
                w.write_u8(self.chr_regs[half][latch] as u8);
            }
        }
        w.write_u8(self.latches[0] as u8);
        w.write_u8(self.latches[1] as u8);
        save_mirroring(w, self.mirroring);
        let vram = unsafe { std::slice::from_raw_parts(self.vram_ptr, VRAM_SIZE as usize) };
        w.write_bytes(vram);
        w.write_bytes(&self.palette_ram);
        save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let cpu_ram =
            unsafe { std::slice::from_raw_parts_mut(self.cpu_ram_ptr, CPU_RAM_SIZE as usize) };
        r.read_bytes_into(cpu_ram)?;
        self.prg_bank_idx = r.read_u8()? as usize;
        r.read_bytes_into(&mut *self.prg_ram)?;
        for half in 0..2 {
            for latch in 0..2 {
                self.chr_regs[half][latch] = r.read_u8()? as usize;
            }
        }
        self.latches[0] = r.read_u8()? as usize;
        self.latches[1] = r.read_u8()? as usize;
        self.mirroring = load_mirroring(r)?;
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

    fn make_mapper(prg_16k_count: usize, chr_8k_count: usize) -> MMC4Mapper {
        let mut prg_banks = Vec::new();
        for i in 0..prg_16k_count {
            let mut bank = [0u8; PRG_BANK_SIZE];
            bank[0] = i as u8;
            prg_banks.push(bank);
        }
        let mut chr_banks = Vec::new();
        for i in 0..chr_8k_count {
            let mut bank = [0u8; io::loader::CHR_BANK_SIZE];
            for b in 0..4096 {
                bank[b] = (i * 2) as u8;
            }
            for b in 4096..8192 {
                bank[b] = (i * 2 + 1) as u8;
            }
            chr_banks.push(bank);
        }
        MMC4Mapper::new(0x01, prg_banks, chr_banks, false, None)
    }

    #[test]
    fn test_prg_banking_16k() {
        let mut m = make_mapper(8, 16);
        m.cpu_write(0xA000, 3);
        assert_eq!(m.prg_bank_idx, 3);

        // Verify switchable bank covers $8000-$BFFF
        m.prg_banks[3][0] = 0xAA;
        m.prg_banks[3][PRG_BANK_SIZE - 1] = 0xBB;
        assert_eq!(m.cpu_read(0x8000), 0xAA);
        assert_eq!(m.cpu_read(0xBFFF), 0xBB);
    }

    #[test]
    fn test_fixed_bank_last() {
        let mut m = make_mapper(8, 16);
        m.prg_banks[7][0] = 0xCC;
        assert_eq!(m.cpu_read(0xC000), 0xCC);
    }

    #[test]
    fn test_prg_ram() {
        let mut m = make_mapper(8, 16);
        m.cpu_write(0x6000, 0x42);
        assert_eq!(m.cpu_read(0x6000), 0x42);
        m.cpu_write(0x7FFF, 0x99);
        assert_eq!(m.cpu_read(0x7FFF), 0x99);
    }

    #[test]
    fn test_latch_left_range_trigger() {
        let mut m = make_mapper(8, 16);

        // MMC4 triggers on range $0FD8-$0FDF for left half
        for addr in 0x0FD8..=0x0FDF {
            m.latches[0] = 1;
            m.ppu_fetch(addr, 0);
            assert_eq!(m.latches[0], 0, "addr {addr:#06X} should trigger left FD");
        }

        for addr in 0x0FE8..=0x0FEF {
            m.latches[0] = 0;
            m.ppu_fetch(addr, 0);
            assert_eq!(m.latches[0], 1, "addr {addr:#06X} should trigger left FE");
        }
    }

    #[test]
    fn test_sram_battery() {
        let sram = vec![0x42; PRG_RAM_8K];
        let m = MMC4Mapper::new(
            0x01,
            vec![[0; PRG_BANK_SIZE]; 2],
            vec![[0; io::loader::CHR_BANK_SIZE]; 2],
            true,
            Some(sram),
        );
        assert_eq!(m.sram_data().unwrap()[0], 0x42);
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut m = make_mapper(8, 16);
        m.cpu_write(0xA000, 5);
        m.cpu_write(0xB000, 3);
        m.cpu_write(0xC000, 7);
        m.cpu_write(0xF000, 1);
        m.cpu_write(0x6000, 0x42);
        m.ppu_fetch(0x0FD8, 0);

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);

        let mut m2 = make_mapper(8, 16);
        let data = w.finish();
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.prg_bank_idx, 5);
        assert_eq!(m2.chr_regs[0][0], 3);
        assert_eq!(m2.chr_regs[0][1], 7);
        assert_eq!(m2.latches[0], 0);
        assert_eq!(m2.mirroring, NametableMirror::Horizontal);
        assert_eq!(m2.cpu_read(0x6000), 0x42);
    }
}
