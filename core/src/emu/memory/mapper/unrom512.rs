use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;
const CHR_TOTAL: usize = 32 * 1024;
const BANK_SWITCHABLE_ADDR: usize = 0x8000;
const BANK_FIXED_ADDR: usize = 0xC000;

pub struct Unrom512Mapper {
    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    _prg_banks: Vec<[u8; PRG_BANK_SIZE]>,
    selected_prg_bank: usize,

    chr_ram: Vec<u8>,
    selected_chr_bank: usize,

    submapper: u8,
    has_bus_conflicts: bool,

    ppu: PpuBus,
    pub controllers: [controller::Controller; 2],
}

impl Unrom512Mapper {
    pub fn new(flags: u8, prg_banks: Vec<[u8; PRG_BANK_SIZE]>, submapper: u8) -> Self {
        if prg_banks.is_empty() {
            panic!("UNROM 512 requires at least one PRG bank");
        }

        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);

        let last_bank = prg_banks.len() - 1;
        mem[BANK_SWITCHABLE_ADDR..BANK_SWITCHABLE_ADDR + PRG_BANK_SIZE]
            .clone_from_slice(&prg_banks[0]);
        mem[BANK_FIXED_ADDR..BANK_FIXED_ADDR + PRG_BANK_SIZE]
            .clone_from_slice(&prg_banks[last_bank]);

        let addr_space_ptr = mem.as_mut_ptr();

        let has_bus_conflicts = submapper == 0 || submapper == 2;

        let chr_ram = vec![0u8; CHR_TOTAL];
        let ppu = PpuBus::new_ram(CHR_BANK_SIZE, NametableMirror::Lower);

        let mirroring = if flags & NAMETABLE_ALIGNMENT_BIT != 0 {
            NametableMirror::Vertical
        } else {
            NametableMirror::Horizontal
        };

        let mut mapper = Unrom512Mapper {
            _addr_space: mem,
            addr_space_ptr,
            _prg_banks: prg_banks,
            selected_prg_bank: 0,
            chr_ram,
            selected_chr_bank: 0,
            submapper,
            has_bus_conflicts,
            ppu,
            controllers: [controller::Controller::new(), controller::Controller::new()],
        };

        // Use 1-screen lower by default for submappers 0/1/2/4; header mirroring for submapper 3
        if submapper == 3 {
            mapper.ppu.mirroring = mirroring;
        } else {
            mapper.ppu.mirroring = NametableMirror::Lower;
        }

        mapper.apply_chr_bank(0);
        mapper
    }

    fn switch_prg_bank(&mut self, bank: u8) {
        let bank_index = (bank as usize) % self._prg_banks.len();
        if bank_index != self.selected_prg_bank {
            self.selected_prg_bank = bank_index;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self._prg_banks[bank_index].as_ptr(),
                    self.addr_space_ptr.offset(BANK_SWITCHABLE_ADDR as isize),
                    PRG_BANK_SIZE,
                );
            }
        }
    }

    fn apply_chr_bank(&mut self, bank: usize) {
        let num_chr_banks = self.chr_ram.len() / CHR_BANK_SIZE;
        let bank_index = bank % num_chr_banks.max(1);
        self.selected_chr_bank = bank_index;
    }

    fn chr_addr(&self, addr: u16) -> usize {
        self.selected_chr_bank * CHR_BANK_SIZE + (addr as usize)
    }

    fn bus_conflict(&self, addr: u16, value: u8) -> u8 {
        if self.has_bus_conflicts {
            let rom_byte = unsafe { *self.addr_space_ptr.offset(addr as isize) };
            value & rom_byte
        } else {
            value
        }
    }
}

impl MemoryMapper for Unrom512Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = mirror_addr(addr);
        unsafe { *self.addr_space_ptr.offset(addr as isize) }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);

        match page {
            0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
            0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                let effective = self.bus_conflict(addr, value);

                self.switch_prg_bank(effective & 0x1F);
                self.apply_chr_bank(((effective >> 5) & 0x03) as usize);

                if self.submapper == 3 {
                    // Submapper 3: bit 7 toggles H/V mirroring
                    // The header provides the base mirroring; bit 7 inverts it
                    // For simplicity, we just use bit 7 as H/V select
                    self.ppu.mirroring = if effective & 0x80 != 0 {
                        NametableMirror::Horizontal
                    } else {
                        NametableMirror::Vertical
                    };
                } else {
                    // Submappers 0/1/2/4: bit 7 selects 1-screen nametable
                    self.ppu.mirroring = if effective & 0x80 != 0 {
                        NametableMirror::Higher
                    } else {
                        NametableMirror::Lower
                    };
                }
            }
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        let addr = addr % super::MAX_VRAM_ADDR;
        if addr < 0x2000 {
            let idx = self.chr_addr(addr);
            if idx < self.chr_ram.len() {
                return self.chr_ram[idx];
            }
            return 0;
        }
        self.ppu.read(addr)
    }

    fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        let addr = addr % super::MAX_VRAM_ADDR;
        if addr < 0x2000 {
            let idx = self.chr_addr(addr);
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
            let idx = self.chr_addr(addr);
            if idx < self.chr_ram.len() {
                self.chr_ram[idx] = value;
            }
            return;
        }
        self.ppu.write(addr, value);
    }

    fn code_start(&mut self) -> u16 {
        ((self.cpu_read(super::RESET_TARGET_ADDR + 1) as u16) << 8) as u16
            + self.cpu_read(super::RESET_TARGET_ADDR) as u16
    }

    fn controllers(&mut self) -> &mut [controller::Controller; 2] {
        &mut self.controllers
    }

    fn poll_irq(&mut self) -> bool {
        false
    }

    fn mapper_id(&self) -> u8 {
        30
    }

    fn submapper_id(&self) -> u8 {
        self.submapper
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let ram = unsafe { std::slice::from_raw_parts(self.addr_space_ptr, MAX_RAM_SIZE) };
        w.write_bytes(ram);
        self.ppu.save_state(w);
        w.write_bytes(&self.chr_ram);
        w.write_u8(self.selected_prg_bank as u8);
        w.write_u8(self.selected_chr_bank as u8);
        save_mirroring(w, self.ppu.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let ram = unsafe { std::slice::from_raw_parts_mut(self.addr_space_ptr, MAX_RAM_SIZE) };
        r.read_bytes_into(ram)?;
        self.ppu.load_state(r)?;
        r.read_bytes_into(&mut self.chr_ram)?;
        self.selected_prg_bank = r.read_u8()? as usize;
        self.selected_chr_bank = r.read_u8()? as usize;
        self.ppu.mirroring = load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bank(fill: u8) -> [u8; PRG_BANK_SIZE] {
        [fill; PRG_BANK_SIZE]
    }

    #[test]
    fn test_prg_bank_switching() {
        let mut m: Box<dyn MemoryMapper> = Box::new(Unrom512Mapper::new(
            0,
            vec![
                make_bank(0x11),
                make_bank(0x22),
                make_bank(0x33),
                make_bank(0x44),
            ],
            1, // no bus conflicts
        ));

        assert_eq!(m.cpu_read(0x8000), 0x11);
        assert_eq!(m.cpu_read(0xC000), 0x44);

        m.cpu_write(0x8000, 0x01);
        assert_eq!(m.cpu_read(0x8000), 0x22);
        assert_eq!(m.cpu_read(0xC000), 0x44);
    }

    #[test]
    fn test_chr_ram_banking() {
        let mut m: Box<dyn MemoryMapper> =
            Box::new(Unrom512Mapper::new(0, vec![make_bank(0xFF)], 1));

        // Write to CHR bank 0
        m.ppu_write(0x0000, 0xAA);
        assert_eq!(m.ppu_read(0x0000), 0xAA);

        // Switch to CHR bank 1 (bits 5-6)
        m.cpu_write(0x8000, 0x20);
        assert_eq!(m.ppu_read(0x0000), 0x00);

        // Write to CHR bank 1
        m.ppu_write(0x0000, 0xBB);
        assert_eq!(m.ppu_read(0x0000), 0xBB);

        // Switch back to CHR bank 0
        m.cpu_write(0x8000, 0x00);
        assert_eq!(m.ppu_read(0x0000), 0xAA);
    }

    #[test]
    fn test_1screen_mirroring() {
        let mut m: Box<dyn MemoryMapper> =
            Box::new(Unrom512Mapper::new(0, vec![make_bank(0xFF)], 1));

        // Default: 1-screen lower
        m.ppu_write(0x2000, 0x42);
        assert_eq!(m.ppu_read(0x2400), 0x42);
        assert_eq!(m.ppu_read(0x2800), 0x42);

        // Bit 7 set: 1-screen upper
        m.cpu_write(0x8000, 0x80);
        m.ppu_write(0x2400, 0x55);
        assert_eq!(m.ppu_read(0x2000), 0x55);
        assert_eq!(m.ppu_read(0x2800), 0x55);
    }

    #[test]
    fn test_submapper3_hv_mirroring() {
        let mut m: Box<dyn MemoryMapper> = Box::new(Unrom512Mapper::new(
            1, // vertical from header
            vec![make_bank(0xFF)],
            3,
        ));

        // Bit 7 clear => vertical
        m.cpu_write(0x8000, 0x00);
        m.ppu_write(0x2000, 0xAA);
        assert_eq!(m.ppu_read(0x2800), 0xAA);

        // Bit 7 set => horizontal
        m.cpu_write(0x8000, 0x80);
        m.ppu_write(0x2000, 0xBB);
        assert_eq!(m.ppu_read(0x2400), 0xBB);
    }

    #[test]
    fn test_bus_conflicts_submapper0() {
        let mut m: Box<dyn MemoryMapper> = Box::new(Unrom512Mapper::new(
            0,
            vec![make_bank(0x01), make_bank(0xFF)],
            0,
        ));

        // 0x03 & 0x01 = 0x01 => bank 1
        m.cpu_write(0x8000, 0x03);
        assert_eq!(m.cpu_read(0x8000), 0xFF);
    }

    #[test]
    fn test_no_bus_conflicts_submapper1() {
        let mut m: Box<dyn MemoryMapper> = Box::new(Unrom512Mapper::new(
            0,
            vec![make_bank(0x00), make_bank(0xFF)],
            1,
        ));

        // No bus conflicts: value written directly
        m.cpu_write(0x8000, 0x01);
        assert_eq!(m.cpu_read(0x8000), 0xFF);
    }
}
