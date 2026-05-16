use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const BNROM_PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_SIZE: usize = 8 * 1024;
const PRG_ROM_ADDR: usize = 0x8000;

pub struct BNROMMapper {
    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    _prg_banks: Vec<[u8; BNROM_PRG_BANK_SIZE]>,
    selected_prg_bank: usize,

    ppu: PpuBus,
    pub controllers: [controller::Controller; 2],
}

impl BNROMMapper {
    pub fn new(flags: u8, prg_banks: Vec<[u8; BNROM_PRG_BANK_SIZE]>) -> BNROMMapper {
        if prg_banks.is_empty() {
            panic!("BNROM requires at least one PRG bank");
        }

        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);
        mem[PRG_ROM_ADDR..PRG_ROM_ADDR + BNROM_PRG_BANK_SIZE].clone_from_slice(&prg_banks[0]);
        let addr_space_ptr = mem.as_mut_ptr();

        BNROMMapper {
            _addr_space: mem,
            addr_space_ptr,
            _prg_banks: prg_banks,
            selected_prg_bank: 0,
            ppu: PpuBus::new_ram(CHR_SIZE, mirroring_from_flags(flags)),
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
                    BNROM_PRG_BANK_SIZE,
                );
            }
        }
    }

    fn bus_conflict(&self, addr: u16, value: u8) -> u8 {
        let rom_byte = unsafe { *self.addr_space_ptr.offset(addr as isize) };
        value & rom_byte
    }
}

impl MemoryMapper for BNROMMapper {
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
                self.switch_prg_bank(effective);
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
        34
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let ram = unsafe { std::slice::from_raw_parts(self.addr_space_ptr, MAX_RAM_SIZE) };
        w.write_bytes(ram);
        self.ppu.save_state(w);
        w.write_u8(self.selected_prg_bank as u8);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let ram = unsafe { std::slice::from_raw_parts_mut(self.addr_space_ptr, MAX_RAM_SIZE) };
        r.read_bytes_into(ram)?;
        self.ppu.load_state(r)?;
        self.selected_prg_bank = r.read_u8()? as usize;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bank(fill: u8) -> [u8; BNROM_PRG_BANK_SIZE] {
        [fill; BNROM_PRG_BANK_SIZE]
    }

    fn make_bank_with_lut(bank_index: u8, fill: u8) -> [u8; BNROM_PRG_BANK_SIZE] {
        let mut bank = [fill; BNROM_PRG_BANK_SIZE];
        bank[0] = bank_index;
        bank[BNROM_PRG_BANK_SIZE - 1] = bank_index;
        bank
    }

    #[test]
    fn test_bnrom_prg_bank_switching() {
        let mut mapper: Box<dyn MemoryMapper> = Box::new(BNROMMapper::new(
            0,
            vec![
                make_bank_with_lut(0, 0xFF),
                make_bank_with_lut(1, 0xFF),
                make_bank_with_lut(2, 0xFF),
                make_bank_with_lut(3, 0xFF),
            ],
        ));

        assert_eq!(mapper.cpu_read(0x8001), 0xFF);

        mapper.cpu_write(0x8001, 0x01);
        assert_eq!(mapper.cpu_read(0x8001), 0xFF);

        mapper.cpu_write(0x8001, 0x02);
        assert_eq!(mapper.cpu_read(0x8001), 0xFF);

        mapper.cpu_write(0x8001, 0x03);
        assert_eq!(mapper.cpu_read(0x8001), 0xFF);

        assert_eq!(mapper.cpu_read(0x8000), 3);
    }

    #[test]
    fn test_bnrom_full_byte_bank_select() {
        let mut banks = Vec::new();
        for i in 0..8 {
            banks.push(make_bank(0xFF ^ i));
        }

        let mut mapper: Box<dyn MemoryMapper> = Box::new(BNROMMapper::new(0, banks));

        mapper.cpu_write(0x8000, 5);
        assert_eq!(mapper.cpu_read(0x8000), 0xFF ^ 5);

        // 7 & 0xFA (rom at $8000 in bank 5) = 2
        mapper.cpu_write(0x8000, 7);
        assert_eq!(mapper.cpu_read(0x8000), 0xFF ^ 2);
    }

    #[test]
    fn test_bnrom_bus_conflict_ands_with_rom() {
        let mut mapper: Box<dyn MemoryMapper> =
            Box::new(BNROMMapper::new(0, vec![make_bank(0x01), make_bank(0xFF)]));

        // 0x03 & 0x01 = 0x01
        mapper.cpu_write(0x8000, 0x03);
        assert_eq!(mapper.cpu_read(0x8000), 0xFF);

        // 0x00 & 0xFF = 0x00
        mapper.cpu_write(0x8000, 0x00);
        assert_eq!(mapper.cpu_read(0x8000), 0x01);
    }

    #[test]
    fn test_bnrom_bank_wraps_modulo_num_banks() {
        let mut mapper: Box<dyn MemoryMapper> =
            Box::new(BNROMMapper::new(0, vec![make_bank(0xFF), make_bank(0xAA)]));

        // Bank 2 should wrap to bank 0 with 2 banks
        mapper.cpu_write(0x8000, 0x02);
        assert_eq!(mapper.cpu_read(0x8000), 0xFF);

        // Bank 3 should wrap to bank 1
        mapper.cpu_write(0x8000, 0x03);
        assert_eq!(mapper.cpu_read(0x8000), 0xAA);
    }

    #[test]
    fn test_bnrom_chr_ram_read_write() {
        let mut mapper: Box<dyn MemoryMapper> = Box::new(BNROMMapper::new(0, vec![make_bank(0)]));

        mapper.ppu_write(0x0100, 0x42);
        assert_eq!(mapper.ppu_read(0x0100), 0x42);

        mapper.ppu_write(0x1FFF, 0x84);
        assert_eq!(mapper.ppu_read(0x1FFF), 0x84);
    }

    #[test]
    fn test_bnrom_nametable_mirroring_horizontal() {
        let mut mapper: Box<dyn MemoryMapper> = Box::new(BNROMMapper::new(0, vec![make_bank(0)]));

        mapper.ppu_write(0x2000, 0x11);
        // Horizontal: $2000 = $2400
        assert_eq!(mapper.ppu_read(0x2400), 0x11);
        // $2800 is a different nametable
        assert_ne!(mapper.ppu_read(0x2800), 0x11);
    }

    #[test]
    fn test_bnrom_nametable_mirroring_vertical() {
        let mut mapper: Box<dyn MemoryMapper> = Box::new(BNROMMapper::new(1, vec![make_bank(0)]));

        mapper.ppu_write(0x2000, 0x22);
        // Vertical: $2000 = $2800
        assert_eq!(mapper.ppu_read(0x2800), 0x22);
        // $2400 is a different nametable
        assert_ne!(mapper.ppu_read(0x2400), 0x22);
    }

    #[test]
    fn test_bnrom_palette_mirroring() {
        let mut mapper: Box<dyn MemoryMapper> = Box::new(BNROMMapper::new(0, vec![make_bank(0)]));

        // $3F10 mirrors to $3F00
        mapper.ppu_write(0x3F10, 0x30);
        assert_eq!(mapper.ppu_read(0x3F00), 0x30);

        mapper.ppu_write(0x3F04, 0x15);
        assert_eq!(mapper.ppu_read(0x3F04), 0x15);
    }
}
