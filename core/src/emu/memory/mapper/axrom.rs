use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const AXROM_PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_SIZE: usize = 8 * 1024;
const PRG_ROM_ADDR: usize = 0x8000;

pub struct AxROMMapper {
    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    _prg_banks: Vec<[u8; AXROM_PRG_BANK_SIZE]>,
    selected_prg_bank: usize,
    has_bus_conflicts: bool,

    ppu: PpuBus,
    pub controllers: [controller::Controller; 2],
}

impl AxROMMapper {
    pub fn new(flags: u8, prg_banks: Vec<[u8; AXROM_PRG_BANK_SIZE]>, submapper: u8) -> AxROMMapper {
        if prg_banks.is_empty() {
            panic!("AxROM requires at least one PRG bank");
        }

        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);
        mem[PRG_ROM_ADDR..PRG_ROM_ADDR + AXROM_PRG_BANK_SIZE].clone_from_slice(&prg_banks[0]);
        let addr_space_ptr = mem.as_mut_ptr();

        // AxROM ignores iNES mirroring flag; uses single-screen mirroring controlled by bit 4
        let _ = flags;

        AxROMMapper {
            _addr_space: mem,
            addr_space_ptr,
            _prg_banks: prg_banks,
            selected_prg_bank: 0,
            has_bus_conflicts: submapper == 2,
            ppu: PpuBus::new_ram(CHR_SIZE, NametableMirror::Lower),
            controllers: [controller::Controller::new(), controller::Controller::new()],
        }
    }

    fn switch_prg_bank(&mut self, bank: u8) {
        let bank_index = (bank as usize) % self._prg_banks.len();
        if bank_index != self.selected_prg_bank {
            self.selected_prg_bank = bank_index;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self._prg_banks[bank_index].as_ptr(),
                    self.addr_space_ptr.offset(PRG_ROM_ADDR as isize),
                    AXROM_PRG_BANK_SIZE,
                );
            }
        }
    }

    fn bus_conflict(&self, addr: u16, value: u8) -> u8 {
        unsafe { super::bus_conflict(self.addr_space_ptr, addr, value) }
    }
}

impl MemoryMapper for AxROMMapper {
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
                let effective = if self.has_bus_conflicts {
                    self.bus_conflict(addr, value)
                } else {
                    value
                };
                self.switch_prg_bank(effective & 0x0F);
                self.ppu.mirroring = if effective & 0x10 != 0 {
                    NametableMirror::Higher
                } else {
                    NametableMirror::Lower
                };
            }
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        self.ppu.read(addr)
    }

    fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        self.ppu.copy(addr, dest, size);
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
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
        7
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let ram = unsafe { std::slice::from_raw_parts(self.addr_space_ptr, MAX_RAM_SIZE) };
        w.write_bytes(ram);
        self.ppu.save_state(w);
        w.write_u8(self.selected_prg_bank as u8);
        save_mirroring(w, self.ppu.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let ram = unsafe { std::slice::from_raw_parts_mut(self.addr_space_ptr, MAX_RAM_SIZE) };
        r.read_bytes_into(ram)?;
        self.ppu.load_state(r)?;
        self.selected_prg_bank = r.read_u8()? as usize;
        self.ppu.mirroring = load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_axrom_prg_bank_switching() {
        let bank1 = [1; AXROM_PRG_BANK_SIZE];
        let bank2 = [2; AXROM_PRG_BANK_SIZE];
        let bank3 = [3; AXROM_PRG_BANK_SIZE];

        let mut mapper: Box<dyn MemoryMapper> =
            Box::new(AxROMMapper::new(0, vec![bank1, bank2, bank3], 0));

        // Initially bank 0 should be selected
        assert_eq!(mapper.cpu_read(0x8000), 1);
        assert_eq!(mapper.cpu_read(0xFFFF), 1);

        // Switch to bank 1
        mapper.cpu_write(0x8000, 1);
        assert_eq!(mapper.cpu_read(0x8000), 2);

        // Switch to bank 2
        mapper.cpu_write(0x8000, 2);
        assert_eq!(mapper.cpu_read(0x8000), 3);

        // Test wrapping - bank 3 should wrap to bank 0
        mapper.cpu_write(0x8000, 3);
        assert_eq!(mapper.cpu_read(0x8000), 1);
    }

    #[test]
    fn test_axrom_single_screen_mirroring() {
        let bank1 = [0; AXROM_PRG_BANK_SIZE];
        let mut mapper: Box<dyn MemoryMapper> = Box::new(AxROMMapper::new(0, vec![bank1], 0));

        // Write to nametable $2000
        mapper.ppu_write(0x2000, 0x42);

        // With single-screen mirroring, all nametable addresses should map to the same location
        assert_eq!(mapper.ppu_read(0x2000), 0x42);
        assert_eq!(mapper.ppu_read(0x2400), 0x42); // different nametable
        assert_eq!(mapper.ppu_read(0x2800), 0x42); // different nametable
        assert_eq!(mapper.ppu_read(0x2C00), 0x42); // different nametable

        // Switch to the other single-screen page
        mapper.cpu_write(0x8000, 0x10); // set bit 4 to switch page

        // Now the same addresses should read different values (new page)
        assert_ne!(mapper.ppu_read(0x2000), 0x42);

        // Write to the new page
        mapper.ppu_write(0x2000, 0x84);
        assert_eq!(mapper.ppu_read(0x2000), 0x84);
        assert_eq!(mapper.ppu_read(0x2400), 0x84);
        assert_eq!(mapper.ppu_read(0x2800), 0x84);
        assert_eq!(mapper.ppu_read(0x2C00), 0x84);

        // Switch back to page 0
        mapper.cpu_write(0x8000, 0x00);
        assert_eq!(mapper.ppu_read(0x2000), 0x42);
    }

    #[test]
    fn test_axrom_chr_ram_write() {
        let bank1 = [0; AXROM_PRG_BANK_SIZE];
        let mut mapper: Box<dyn MemoryMapper> = Box::new(AxROMMapper::new(0, vec![bank1], 0));

        mapper.ppu_write(0x0100, 0x55);
        assert_eq!(mapper.ppu_read(0x0100), 0x55);

        mapper.ppu_write(0x1FFF, 0xAA);
        assert_eq!(mapper.ppu_read(0x1FFF), 0xAA);
    }

    #[test]
    fn test_axrom_combined_bank_and_mirroring() {
        let bank1 = [1; AXROM_PRG_BANK_SIZE];
        let bank2 = [2; AXROM_PRG_BANK_SIZE];

        let mut mapper: Box<dyn MemoryMapper> =
            Box::new(AxROMMapper::new(0, vec![bank1, bank2], 0));

        // 0x11 = PRG bank 1, single-screen page 1
        mapper.cpu_write(0x8000, 0x11);
        assert_eq!(mapper.cpu_read(0x8000), 2);

        mapper.ppu_write(0x2000, 0x33);
        assert_eq!(mapper.ppu_read(0x2400), 0x33);

        // 0x00 = PRG bank 0, single-screen page 0
        mapper.cpu_write(0x8000, 0x00);
        assert_eq!(mapper.cpu_read(0x8000), 1);
        assert_ne!(mapper.ppu_read(0x2000), 0x33);
    }

    #[test]
    fn test_axrom_4bit_bank_select() {
        let banks: Vec<[u8; AXROM_PRG_BANK_SIZE]> =
            (0u8..16).map(|i| [i; AXROM_PRG_BANK_SIZE]).collect();

        let mut mapper: Box<dyn MemoryMapper> = Box::new(AxROMMapper::new(0, banks, 0));

        mapper.cpu_write(0x8000, 0x08);
        assert_eq!(mapper.cpu_read(0x8000), 8);

        mapper.cpu_write(0x8000, 0x0F);
        assert_eq!(mapper.cpu_read(0x8000), 15);
    }

    #[test]
    fn test_axrom_bus_conflicts_submapper_2() {
        let mut bank = [0xFF_u8; AXROM_PRG_BANK_SIZE];
        bank[0] = 0x03;
        let bank1 = [1; AXROM_PRG_BANK_SIZE];

        let mut mapper: Box<dyn MemoryMapper> = Box::new(AxROMMapper::new(0, vec![bank, bank1], 2));

        // Write 0x01 to $8000 where ROM byte is 0x03: AND = 0x01 -> bank 1
        mapper.cpu_write(0x8000, 0x01);
        assert_eq!(mapper.cpu_read(0x8000), 1);

        // Switch back to bank 0
        mapper.cpu_write(0x8000, 0x00);
        // Write 0x01 to $8000 where ROM byte is 0x03: AND = 0x01 -> bank 1
        mapper.cpu_write(0x8000, 0x05);
        assert_eq!(mapper.cpu_read(0x8000), 1); // 0x05 & 0x03 = 0x01
    }

    #[test]
    fn test_axrom_no_bus_conflicts_submapper_0() {
        let mut bank = [0x00_u8; AXROM_PRG_BANK_SIZE];
        bank[0] = 0x00;
        let bank1 = [1; AXROM_PRG_BANK_SIZE];

        let mut mapper: Box<dyn MemoryMapper> = Box::new(AxROMMapper::new(0, vec![bank, bank1], 0));

        // Without bus conflicts, ROM byte doesn't matter
        mapper.cpu_write(0x8000, 0x01);
        assert_eq!(mapper.cpu_read(0x8000), 1);
    }
}
