use super::super::super::io;
use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 8 * 1024;
const CHR_BANK_SIZE: u16 = 4 * 1024;
const CPU_RAM_SIZE: usize = 2 * 1024;
const VRAM_SIZE: u16 = 2 * 1024;

pub struct MMC2Mapper {
    _cpu_ram: Box<[u8; CPU_RAM_SIZE]>,
    cpu_ram_ptr: *mut u8,

    prg_banks: Vec<[u8; PRG_BANK_SIZE]>,
    prg_bank_idx: usize,

    chr_banks: Vec<[u8; CHR_BANK_SIZE as usize]>,
    // chr_regs[half][latch_state]: bank index for each half (0=left, 1=right) x latch state (0=FD, 1=FE)
    chr_regs: [[usize; 2]; 2],
    // Current latch state per half: 0=FD, 1=FE
    latches: [usize; 2],

    mirroring: NametableMirror,

    _vram: Box<[u8; VRAM_SIZE as usize]>,
    vram_ptr: *mut u8,
    palette_ram: [u8; 32],

    pub controllers: [controller::Controller; 2],
}

impl MMC2Mapper {
    pub fn new(
        flags: u8,
        prg_banks_16k: Vec<[u8; 16 * 1024]>,
        chr_banks_8k: Vec<[u8; io::loader::CHR_BANK_SIZE]>,
    ) -> Self {
        let mut prg_banks: Vec<[u8; PRG_BANK_SIZE]> = Vec::new();
        for bank in &prg_banks_16k {
            let mut lo = [0u8; PRG_BANK_SIZE];
            let mut hi = [0u8; PRG_BANK_SIZE];
            lo.copy_from_slice(&bank[..PRG_BANK_SIZE]);
            hi.copy_from_slice(&bank[PRG_BANK_SIZE..]);
            prg_banks.push(lo);
            prg_banks.push(hi);
        }

        let mut chr_banks: Vec<[u8; CHR_BANK_SIZE as usize]> = Vec::new();
        for bank in &chr_banks_8k {
            let mut lo = [0u8; CHR_BANK_SIZE as usize];
            let mut hi = [0u8; CHR_BANK_SIZE as usize];
            lo.copy_from_slice(&bank[..CHR_BANK_SIZE as usize]);
            hi.copy_from_slice(&bank[CHR_BANK_SIZE as usize..]);
            chr_banks.push(lo);
            chr_banks.push(hi);
        }

        let mut cpu_ram = Box::new([0u8; CPU_RAM_SIZE]);
        let cpu_ram_ptr = cpu_ram.as_mut_ptr();

        let mut vram = Box::new([0u8; VRAM_SIZE as usize]);
        let vram_ptr = vram.as_mut_ptr();

        let mirroring = mirroring_from_flags(flags);

        MMC2Mapper {
            _cpu_ram: cpu_ram,
            cpu_ram_ptr,
            prg_banks,
            prg_bank_idx: 0,
            chr_banks,
            chr_regs: [[0; 2]; 2],
            latches: [1, 1],
            mirroring,
            _vram: vram,
            vram_ptr,
            palette_ram: [0x0F; 32],
            controllers: [controller::Controller::new(), controller::Controller::new()],
        }
    }

    fn active_chr_bank(&self, half: usize) -> usize {
        let bank = self.chr_regs[half][self.latches[half]];
        bank % self.chr_banks.len()
    }

    fn check_latch_trigger(&mut self, addr: u16) {
        match addr {
            0x0FD8 => self.latches[0] = 0,
            0x0FE8 => self.latches[0] = 1,
            0x1FD8..=0x1FDF => self.latches[1] = 0,
            0x1FE8..=0x1FEF => self.latches[1] = 1,
            _ => {}
        }
    }
}

impl MemoryMapper for MMC2Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);
        match page {
            0x00 | 0x10 => unsafe { *self.cpu_ram_ptr.offset(addr as _) },
            0x20 | 0x40 => 0,
            0x60 | 0x70 => 0, // no PRG RAM on MMC2
            0x80 | 0x90 => {
                self.prg_banks[self.prg_bank_idx][(addr - 0x8000) as usize]
            }
            0xA0 | 0xB0 => {
                let fixed = self.prg_banks.len() - 3;
                self.prg_banks[fixed][(addr - 0xA000) as usize]
            }
            0xC0 | 0xD0 => {
                let fixed = self.prg_banks.len() - 2;
                self.prg_banks[fixed][(addr - 0xC000) as usize]
            }
            0xE0 | 0xF0 => {
                let fixed = self.prg_banks.len() - 1;
                self.prg_banks[fixed][(addr - 0xE000) as usize]
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);
        match page {
            0x00 | 0x10 => unsafe { *self.cpu_ram_ptr.offset(addr as _) = value },
            0x20 | 0x40 | 0x50 | 0x60 | 0x70 => {}
            0x80..=0xF0 => {
                let reg = (addr >> 12) & 0x0F;
                match reg {
                    0xA => {
                        self.prg_bank_idx =
                            (value as usize & 0x0F) % self.prg_banks.len();
                    }
                    0xB => {
                        self.chr_regs[0][0] = value as usize & 0x1F;
                    }
                    0xC => {
                        self.chr_regs[0][1] = value as usize & 0x1F;
                    }
                    0xD => {
                        self.chr_regs[1][0] = value as usize & 0x1F;
                    }
                    0xE => {
                        self.chr_regs[1][1] = value as usize & 0x1F;
                    }
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
        if addr >= 0x3F00 {
            let mut idx = (addr as usize - 0x3F00) % 32;
            if idx & 0x13 == 0x10 {
                idx &= !0x10;
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

        // Check if this address triggers a new latch change
        if addr < 0x2000 {
            self.check_latch_trigger(addr);
        }

        value
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        let addr = addr % MAX_VRAM_ADDR;
        match addr_to_page(addr) {
            0x00 => {
                let bank = self.active_chr_bank(0);
                unsafe {
                    std::ptr::copy(
                        self.chr_banks[bank].as_ptr().offset(addr as _),
                        dest,
                        size,
                    )
                }
            }
            0x10 => {
                let bank = self.active_chr_bank(1);
                unsafe {
                    std::ptr::copy(
                        self.chr_banks[bank]
                            .as_ptr()
                            .offset((addr - 0x1000) as _),
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
        if addr >= 0x3F00 {
            let mut idx = (addr as usize - 0x3F00) % 32;
            if idx & 0x13 == 0x10 {
                idx &= !0x10;
            }
            self.palette_ram[idx] = value;
            return;
        }
        match addr_to_page(addr) {
            0x00 | 0x10 => {} // CHR-ROM, not writable
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
        9
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let cpu_ram = unsafe { std::slice::from_raw_parts(self.cpu_ram_ptr, CPU_RAM_SIZE) };
        w.write_bytes(cpu_ram);
        w.write_u8(self.prg_bank_idx as u8);
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
        let cpu_ram = unsafe { std::slice::from_raw_parts_mut(self.cpu_ram_ptr, CPU_RAM_SIZE) };
        r.read_bytes_into(cpu_ram)?;
        self.prg_bank_idx = r.read_u8()? as usize;
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

    fn make_mapper(prg_16k_count: usize, chr_8k_count: usize) -> MMC2Mapper {
        let mut prg_banks = Vec::new();
        for i in 0..prg_16k_count {
            let mut bank = [0u8; 16 * 1024];
            bank[0] = i as u8;
            prg_banks.push(bank);
        }
        let mut chr_banks = Vec::new();
        for i in 0..chr_8k_count {
            let mut bank = [0u8; io::loader::CHR_BANK_SIZE];
            // Fill low 4K half with i*2, high 4K half with i*2+1
            for b in 0..4096 {
                bank[b] = (i * 2) as u8;
            }
            for b in 4096..8192 {
                bank[b] = (i * 2 + 1) as u8;
            }
            chr_banks.push(bank);
        }
        MMC2Mapper::new(0x01, prg_banks, chr_banks)
    }

    #[test]
    fn test_prg_banking() {
        let mut m = make_mapper(8, 16);
        // 8 x 16KB = 16 x 8KB banks
        // $8000-$9FFF = switchable, $A000+ = fixed to last 3

        // Write PRG bank 5 to $A000
        m.cpu_write(0xA000, 5);
        assert_eq!(m.prg_bank_idx, 5);

        // Fixed banks
        assert_eq!(m.prg_banks.len(), 16);
        // $A000 = bank 13, $C000 = bank 14, $E000 = bank 15
    }

    #[test]
    fn test_prg_bank_masking() {
        let mut m = make_mapper(8, 16);
        // 16 8KB banks, mask = 0x0F
        m.cpu_write(0xA000, 0x0F);
        assert_eq!(m.prg_bank_idx, 15);

        // Value > bank count wraps
        m.cpu_write(0xA000, 0x03);
        assert_eq!(m.prg_bank_idx, 3);
    }

    #[test]
    fn test_chr_register_writes() {
        let mut m = make_mapper(8, 16);

        m.cpu_write(0xB000, 3);  // CHR left/FD
        m.cpu_write(0xC000, 7);  // CHR left/FE
        m.cpu_write(0xD000, 10); // CHR right/FD
        m.cpu_write(0xE000, 15); // CHR right/FE

        assert_eq!(m.chr_regs[0][0], 3);
        assert_eq!(m.chr_regs[0][1], 7);
        assert_eq!(m.chr_regs[1][0], 10);
        assert_eq!(m.chr_regs[1][1], 15);
    }

    #[test]
    fn test_chr_register_masking() {
        let mut m = make_mapper(8, 16);

        m.cpu_write(0xB000, 0xFF);
        assert_eq!(m.chr_regs[0][0], 0x1F);
    }

    #[test]
    fn test_mirroring_control() {
        let mut m = make_mapper(8, 16);

        m.cpu_write(0xF000, 0);
        assert_eq!(m.mirroring, NametableMirror::Vertical);

        m.cpu_write(0xF000, 1);
        assert_eq!(m.mirroring, NametableMirror::Horizontal);
    }

    #[test]
    fn test_initial_latch_state() {
        let m = make_mapper(8, 16);
        // Both latches start in FE state (index 1)
        assert_eq!(m.latches[0], 1);
        assert_eq!(m.latches[1], 1);
    }

    #[test]
    fn test_latch_left_fd_trigger() {
        let mut m = make_mapper(8, 16);
        m.cpu_write(0xB000, 2);  // FD bank
        m.cpu_write(0xC000, 5);  // FE bank

        // Initial state: latch 0 = FE, so active bank = chr_regs[0][1] = 5
        assert_eq!(m.active_chr_bank(0), 5);

        // Fetch from $0FD8 triggers latch to FD — but deferred
        m.ppu_fetch(0x0FD8, 0);

        // Now latch has been set, but it's applied immediately on next read
        // since check_latch_trigger sets it directly
        assert_eq!(m.latches[0], 0);
        assert_eq!(m.active_chr_bank(0), 2);
    }

    #[test]
    fn test_latch_left_fe_trigger() {
        let mut m = make_mapper(8, 16);
        m.cpu_write(0xB000, 2);  // FD bank
        m.cpu_write(0xC000, 5);  // FE bank

        // Switch to FD first
        m.ppu_fetch(0x0FD8, 0);
        assert_eq!(m.latches[0], 0);

        // Now trigger FE
        m.ppu_fetch(0x0FE8, 0);
        assert_eq!(m.latches[0], 1);
        assert_eq!(m.active_chr_bank(0), 5);
    }

    #[test]
    fn test_latch_left_only_exact_address() {
        let mut m = make_mapper(8, 16);
        m.cpu_write(0xB000, 2);

        // $0FD7 should NOT trigger
        m.ppu_fetch(0x0FD7, 0);
        assert_eq!(m.latches[0], 1);

        // $0FD9 should NOT trigger
        m.ppu_fetch(0x0FD9, 0);
        assert_eq!(m.latches[0], 1);

        // $0FD8 SHOULD trigger
        m.ppu_fetch(0x0FD8, 0);
        assert_eq!(m.latches[0], 0);
    }

    #[test]
    fn test_latch_right_fd_range() {
        let mut m = make_mapper(8, 16);
        m.cpu_write(0xD000, 3);

        // $1FD8-$1FDF should all trigger right latch to FD
        for addr in 0x1FD8..=0x1FDF {
            m.latches[1] = 1; // reset
            m.ppu_fetch(addr, 0);
            assert_eq!(m.latches[1], 0, "addr {:#06X} should trigger right FD latch", addr);
        }

        // $1FD7 should NOT trigger
        m.latches[1] = 1;
        m.ppu_fetch(0x1FD7, 0);
        assert_eq!(m.latches[1], 1);

        // $1FE0 should NOT trigger FD
        m.latches[1] = 1;
        m.ppu_fetch(0x1FE0, 0);
        assert_eq!(m.latches[1], 1);
    }

    #[test]
    fn test_latch_right_fe_range() {
        let mut m = make_mapper(8, 16);
        m.cpu_write(0xE000, 7);

        // $1FE8-$1FEF should all trigger right latch to FE
        for addr in 0x1FE8..=0x1FEF {
            m.latches[1] = 0; // reset to FD
            m.ppu_fetch(addr, 0);
            assert_eq!(m.latches[1], 1, "addr {:#06X} should trigger right FE latch", addr);
        }
    }

    #[test]
    fn test_deferred_latch_reads_old_bank() {
        let mut m = make_mapper(8, 16);

        // Set up distinguishable banks
        m.chr_banks[2][0x0FD8] = 0xAA;
        m.chr_banks[5][0x0FD8] = 0xBB;

        m.cpu_write(0xB000, 2);  // FD bank = 2
        m.cpu_write(0xC000, 5);  // FE bank = 5

        // Initial latch = FE, so active bank for left half = 5
        let val = m.ppu_fetch(0x0FD8, 0);
        // The read should come from the OLD bank (FE=5) since the latch
        // update is deferred
        assert_eq!(val, 0xBB);

        // After the fetch, latch is now FD
        assert_eq!(m.latches[0], 0);
    }

    #[test]
    fn test_no_prg_ram() {
        let mut m = make_mapper(8, 16);
        // Writes to $6000-$7FFF should be ignored
        m.cpu_write(0x6000, 0x42);
        assert_eq!(m.cpu_read(0x6000), 0);
    }

    #[test]
    fn test_register_mirroring() {
        let mut m = make_mapper(8, 16);

        // Writes anywhere in $A000-$AFFF should hit PRG bank register
        m.cpu_write(0xA123, 3);
        assert_eq!(m.prg_bank_idx, 3);

        m.cpu_write(0xAFFF, 7);
        assert_eq!(m.prg_bank_idx, 7);
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut m = make_mapper(8, 16);
        m.cpu_write(0xA000, 5);
        m.cpu_write(0xB000, 3);
        m.cpu_write(0xC000, 7);
        m.cpu_write(0xD000, 10);
        m.cpu_write(0xE000, 15);
        m.cpu_write(0xF000, 1);
        m.ppu_fetch(0x0FD8, 0); // trigger left latch to FD

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);

        let mut m2 = make_mapper(8, 16);
        let data = w.finish();
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.prg_bank_idx, 5);
        assert_eq!(m2.chr_regs[0][0], 3);
        assert_eq!(m2.chr_regs[0][1], 7);
        assert_eq!(m2.chr_regs[1][0], 10);
        assert_eq!(m2.chr_regs[1][1], 15);
        assert_eq!(m2.latches[0], 0);
        assert_eq!(m2.latches[1], 1);
        assert_eq!(m2.mirroring, NametableMirror::Horizontal);
    }

    #[test]
    fn test_chr_read_through_latch() {
        let mut m = make_mapper(8, 16);

        // Put distinct data in different banks
        m.chr_banks[0][0x100] = 0x11;
        m.chr_banks[1][0x100] = 0x22;

        m.cpu_write(0xB000, 0); // FD bank = 0
        m.cpu_write(0xC000, 1); // FE bank = 1

        // Latch starts at FE (1), so we read bank 1
        assert_eq!(m.ppu_read(0x0100), 0x22);

        // Switch to FD
        m.ppu_fetch(0x0FD8, 0);

        // Now reads from bank 0
        assert_eq!(m.ppu_read(0x0100), 0x11);
    }

    #[test]
    fn test_independent_latches() {
        let mut m = make_mapper(8, 16);

        // Trigger left latch to FD
        m.ppu_fetch(0x0FD8, 0);
        assert_eq!(m.latches[0], 0);
        assert_eq!(m.latches[1], 1); // right unchanged

        // Trigger right latch to FD
        m.ppu_fetch(0x1FD8, 0);
        assert_eq!(m.latches[0], 0); // left unchanged
        assert_eq!(m.latches[1], 0);
    }
}
