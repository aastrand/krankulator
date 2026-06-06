use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_16K: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;
const CHR_TOTAL: usize = 32 * 1024;

pub struct Action53Mapper {
    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    prg_banks_16k: Vec<[u8; PRG_16K]>,

    chr_ram: Vec<u8>,

    reg_select: u8,
    reg_chr: u8,
    reg_inner: u8,
    reg_mode: u8,
    reg_outer: u8,

    ppu: PpuBus,
    pub controllers: [controller::Controller; 2],
}

impl Action53Mapper {
    pub fn new(flags: u8, prg_banks: Vec<[u8; PRG_16K]>) -> Self {
        if prg_banks.is_empty() {
            panic!("Action 53 requires at least one PRG bank");
        }

        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);
        let addr_space_ptr = mem.as_mut_ptr();

        let mirroring = mirroring_from_flags(flags);

        let num_16k = prg_banks.len();

        // Power-on: mode 3 (switchable $8000, fixed $C000 = UNROM style),
        // outer size 0 (32K window), outer bank = last pair.
        // This places the last 16K bank at $C000 where the reset vector lives.
        let reg_mode = 0x0C; // mode 3, outer size 0, mirroring 0
        let reg_outer = ((num_16k / 2).saturating_sub(1)) as u8;

        let mut mapper = Action53Mapper {
            _addr_space: mem,
            addr_space_ptr,
            prg_banks_16k: prg_banks,
            chr_ram: vec![0u8; CHR_TOTAL],
            reg_select: 0,
            reg_chr: 0,
            reg_inner: 0,
            reg_mode,
            reg_outer,
            ppu: PpuBus::new_ram(CHR_BANK_SIZE, mirroring),
            controllers: [controller::Controller::new(), controller::Controller::new()],
        };

        mapper.sync_prg();

        mapper
    }

    fn calc_prg_bank(&self, cpu_a14: usize) -> usize {
        // Reference: https://www.nesdev.org/wiki/Action_53_mapper/Reference_implementations
        let outer_bank = (self.reg_outer as usize) << 1;
        let mut current_bank = (self.reg_inner & 0x0F) as usize;
        let mut bank_mode = (self.reg_mode >> 2) as usize; // discard mirroring bits

        // In UNROM fixed bank? If so, treat as NROM (32K mode)
        if (bank_mode ^ cpu_a14) & 0x03 == 0x02 {
            bank_mode = 0;
        }

        // In 32K bank mode? Shift inner left and merge A14
        if (bank_mode & 0x02) == 0 {
            current_bank = (current_bank << 1) | cpu_a14;
        }

        // bank_size_masks: [0x01, 0x03, 0x07, 0x0F]
        let size_index = (bank_mode >> 2) & 3;
        let bank_size_mask = (2usize << size_index) - 1;

        ((current_bank & bank_size_mask) | (outer_bank & !bank_size_mask))
            % self.prg_banks_16k.len()
    }

    fn sync_prg(&mut self) {
        let low = self.calc_prg_bank(0);
        let high = self.calc_prg_bank(1);
        self.map_prg_16k(0x8000, low);
        self.map_prg_16k(0xC000, high);
    }

    fn map_prg_16k(&mut self, addr: usize, bank: usize) {
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.prg_banks_16k[bank].as_ptr(),
                self.addr_space_ptr.add(addr),
                PRG_16K,
            );
        }
    }

    fn sync_mirroring(&mut self) {
        let mirror_mode = self.reg_mode & 0x03;
        self.ppu.mirroring = match mirror_mode {
            0 => {
                if self.reg_chr & 0x10 != 0 {
                    NametableMirror::Higher
                } else {
                    NametableMirror::Lower
                }
            }
            1 => {
                if self.reg_inner & 0x10 != 0 {
                    NametableMirror::Higher
                } else {
                    NametableMirror::Lower
                }
            }
            2 => NametableMirror::Vertical,
            3 => NametableMirror::Horizontal,
            _ => unreachable!(),
        };
    }

    fn chr_offset(&self, addr: u16) -> usize {
        let bank = (self.reg_chr & 0x03) as usize;
        bank * CHR_BANK_SIZE + (addr as usize)
    }
}

impl MemoryMapper for Action53Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = mirror_addr(addr);
        unsafe { *self.addr_space_ptr.offset(addr as isize) }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);

        match page {
            0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
            0x50 => {
                // $5000-$5FFF: register select
                self.reg_select = value;
            }
            0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                let reg_id = self.reg_select & 0x81;
                match reg_id {
                    0x00 => {
                        self.reg_chr = value;
                        self.sync_mirroring();
                    }
                    0x01 => {
                        self.reg_inner = value;
                        self.sync_prg();
                        self.sync_mirroring();
                    }
                    0x80 => {
                        self.reg_mode = value;
                        self.sync_prg();
                        self.sync_mirroring();
                    }
                    0x81 => {
                        self.reg_outer = value;
                        self.sync_prg();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        let addr = addr % super::MAX_VRAM_ADDR;
        if addr < 0x2000 {
            let idx = self.chr_offset(addr);
            if idx < self.chr_ram.len() {
                return self.chr_ram[idx];
            }
            return 0;
        }
        self.ppu.read(addr)
    }

    unsafe fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        let addr = addr % super::MAX_VRAM_ADDR;
        if addr < 0x2000 {
            let idx = self.chr_offset(addr);
            if idx + size <= self.chr_ram.len() {
                unsafe {
                    std::ptr::copy(self.chr_ram.as_ptr().add(idx), dest, size);
                }
            }
            return;
        }
        self.ppu.copy(addr, dest, size);
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        let addr = addr % super::MAX_VRAM_ADDR;
        if addr < 0x2000 {
            let idx = self.chr_offset(addr);
            if idx < self.chr_ram.len() {
                self.chr_ram[idx] = value;
            }
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
        28
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let ram = unsafe { std::slice::from_raw_parts(self.addr_space_ptr, MAX_RAM_SIZE) };
        w.write_bytes(ram);
        self.ppu.save_state(w);
        w.write_bytes(&self.chr_ram);
        w.write_u8(self.reg_select);
        w.write_u8(self.reg_chr);
        w.write_u8(self.reg_inner);
        w.write_u8(self.reg_mode);
        w.write_u8(self.reg_outer);
        save_mirroring(w, self.ppu.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let ram = unsafe { std::slice::from_raw_parts_mut(self.addr_space_ptr, MAX_RAM_SIZE) };
        r.read_bytes_into(ram)?;
        self.ppu.load_state(r)?;
        r.read_bytes_into(&mut self.chr_ram)?;
        self.reg_select = r.read_u8()?;
        self.reg_chr = r.read_u8()?;
        self.reg_inner = r.read_u8()?;
        self.reg_mode = r.read_u8()?;
        self.reg_outer = r.read_u8()?;
        self.ppu.mirroring = load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bank(fill: u8) -> [u8; PRG_16K] {
        [fill; PRG_16K]
    }

    fn write_reg(m: &mut Box<dyn MemoryMapper>, reg: u8, value: u8) {
        m.cpu_write(0x5000, reg);
        m.cpu_write(0x8000, value);
    }

    #[test]
    fn test_32k_switching_mode() {
        let mut banks = Vec::new();
        for i in 0..8u8 {
            banks.push(make_bank(i));
        }

        let mut m: Box<dyn MemoryMapper> = Box::new(Action53Mapper::new(0, banks));

        // Set mode: 32K switching (mode 0), 256K outer size, vertical mirroring
        write_reg(&mut m, 0x80, 0b0011_0010);
        // Set outer bank 0
        write_reg(&mut m, 0x81, 0x00);
        // Set inner bank 0
        write_reg(&mut m, 0x01, 0x00);
        assert_eq!(m.cpu_read(0x8000), 0);
        assert_eq!(m.cpu_read(0xC000), 1);

        // Switch to inner bank 1
        write_reg(&mut m, 0x01, 0x01);
        assert_eq!(m.cpu_read(0x8000), 2);
        assert_eq!(m.cpu_read(0xC000), 3);
    }

    #[test]
    fn test_unrom_style_mode3() {
        let mut banks = Vec::new();
        for i in 0..4u8 {
            banks.push(make_bank(i * 10));
        }

        let mut m: Box<dyn MemoryMapper> = Box::new(Action53Mapper::new(0, banks));

        // Mode 3 (UNROM #2 style): switchable $8000, fixed $C000
        // outer size = 0 (32K window), so inner has 1 bit
        write_reg(&mut m, 0x80, 0b0000_1110);
        write_reg(&mut m, 0x81, 0x00);
        write_reg(&mut m, 0x01, 0x00);
        assert_eq!(m.cpu_read(0x8000), 0);
        // Fixed bank = last within window
        assert_eq!(m.cpu_read(0xC000), 10);

        write_reg(&mut m, 0x01, 0x01);
        assert_eq!(m.cpu_read(0x8000), 10);
    }

    #[test]
    fn test_chr_ram_banking() {
        let mut m: Box<dyn MemoryMapper> = Box::new(Action53Mapper::new(0, vec![make_bank(0xFF)]));

        // Write to CHR bank 0
        m.ppu_write(0x0000, 0xAA);
        assert_eq!(m.ppu_read(0x0000), 0xAA);

        // Switch to CHR bank 1
        write_reg(&mut m, 0x00, 0x01);
        assert_eq!(m.ppu_read(0x0000), 0x00);
        m.ppu_write(0x0000, 0xBB);

        // Switch back to CHR bank 0
        write_reg(&mut m, 0x00, 0x00);
        assert_eq!(m.ppu_read(0x0000), 0xAA);
    }

    #[test]
    fn test_mirroring_modes() {
        let mut m: Box<dyn MemoryMapper> = Box::new(Action53Mapper::new(0, vec![make_bank(0xFF)]));

        // Mode bits 1-0 = 2 => vertical
        write_reg(&mut m, 0x80, 0x02);
        m.ppu_write(0x2000, 0x42);
        assert_eq!(m.ppu_read(0x2800), 0x42);

        // Mode bits 1-0 = 3 => horizontal
        write_reg(&mut m, 0x80, 0x03);
        m.ppu_write(0x2000, 0x55);
        assert_eq!(m.ppu_read(0x2400), 0x55);
    }

    #[test]
    fn test_register_select() {
        let mut m: Box<dyn MemoryMapper> = Box::new(Action53Mapper::new(0, vec![make_bank(0xFF)]));

        // Select register $00 (CHR), write value
        m.cpu_write(0x5000, 0x00);
        m.cpu_write(0x8000, 0x02);

        // CHR bank should be 2
        m.ppu_write(0x0000, 0x42);
        assert_eq!(m.ppu_read(0x0000), 0x42);

        // Switch to CHR bank 0 to verify bank 2 is separate
        m.cpu_write(0x5000, 0x00);
        m.cpu_write(0x8000, 0x00);
        assert_ne!(m.ppu_read(0x0000), 0x42);
    }
}
