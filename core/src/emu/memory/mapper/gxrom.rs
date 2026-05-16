use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const GXROM_PRG_BANK_SIZE: usize = 32 * 1024;
const GXROM_CHR_BANK_SIZE: usize = 8 * 1024;
const PRG_ROM_ADDR: usize = 0x8000;

pub struct GxROMMapper {
    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    _prg_banks: Vec<[u8; GXROM_PRG_BANK_SIZE]>,
    selected_prg_bank: usize,

    chr_banks: Vec<[u8; GXROM_CHR_BANK_SIZE]>,
    selected_chr_bank: usize,

    ppu: PpuBus,
    pub controllers: [controller::Controller; 2],
}

impl GxROMMapper {
    pub fn new(
        flags: u8,
        prg_banks: Vec<[u8; GXROM_PRG_BANK_SIZE]>,
        chr_banks: Vec<[u8; GXROM_CHR_BANK_SIZE]>,
    ) -> GxROMMapper {
        if prg_banks.is_empty() {
            panic!("GxROM requires at least one PRG bank");
        }
        if chr_banks.is_empty() {
            panic!("GxROM requires at least one CHR bank");
        }

        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);
        mem[PRG_ROM_ADDR..PRG_ROM_ADDR + GXROM_PRG_BANK_SIZE].clone_from_slice(&prg_banks[0]);
        let addr_space_ptr = mem.as_mut_ptr();

        let ppu = PpuBus::new_rom(&chr_banks[0], mirroring_from_flags(flags));

        GxROMMapper {
            _addr_space: mem,
            addr_space_ptr,
            _prg_banks: prg_banks,
            selected_prg_bank: 0,
            chr_banks,
            selected_chr_bank: 0,
            ppu,
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
                    GXROM_PRG_BANK_SIZE,
                );
            }
        }
    }

    fn switch_chr_bank(&mut self, bank: u8) {
        let bank_index = (bank as usize) % self.chr_banks.len();
        if bank_index != self.selected_chr_bank {
            self.selected_chr_bank = bank_index;
            self.ppu.switch_chr_bank(&self.chr_banks, bank_index);
        }
    }

    fn bus_conflict(&self, addr: u16, value: u8) -> u8 {
        let rom_byte = unsafe { *self.addr_space_ptr.offset(addr as isize) };
        value & rom_byte
    }
}

impl MemoryMapper for GxROMMapper {
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
                self.switch_prg_bank((effective >> 4) & 0x03);
                self.switch_chr_bank(effective & 0x03);
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
        66
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let ram = unsafe { std::slice::from_raw_parts(self.addr_space_ptr, MAX_RAM_SIZE) };
        w.write_bytes(ram);
        self.ppu.save_state(w);
        w.write_u8(self.selected_prg_bank as u8);
        w.write_u8(self.selected_chr_bank as u8);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let ram = unsafe { std::slice::from_raw_parts_mut(self.addr_space_ptr, MAX_RAM_SIZE) };
        r.read_bytes_into(ram)?;
        self.ppu.load_state(r)?;
        self.selected_prg_bank = r.read_u8()? as usize;
        self.selected_chr_bank = r.read_u8()? as usize;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_prg(fill: u8) -> [u8; GXROM_PRG_BANK_SIZE] {
        [fill; GXROM_PRG_BANK_SIZE]
    }

    fn make_chr(fill: u8) -> [u8; GXROM_CHR_BANK_SIZE] {
        [fill; GXROM_CHR_BANK_SIZE]
    }

    #[test]
    fn test_gxrom_prg_and_chr_bank_switching() {
        let mut mapper: Box<dyn MemoryMapper> = Box::new(GxROMMapper::new(
            0,
            vec![
                make_prg(0xFF),
                make_prg(0xFF),
                make_prg(0xFF),
                make_prg(0xFF),
            ],
            vec![
                make_chr(0x10),
                make_chr(0x20),
                make_chr(0x30),
                make_chr(0x40),
            ],
        ));

        mapper.cpu_write(0x8000, 0b0001_0010);
        assert_eq!(mapper.ppu_read(0x0000), 0x30);

        mapper.cpu_write(0x8000, 0b0011_0001);
        assert_eq!(mapper.ppu_read(0x0000), 0x20);
    }

    #[test]
    fn test_gxrom_bit_extraction() {
        let mut mapper: Box<dyn MemoryMapper> = Box::new(GxROMMapper::new(
            0,
            vec![
                make_prg(0xFF),
                make_prg(0xFF),
                make_prg(0xFF),
                make_prg(0xFF),
            ],
            vec![
                make_chr(0xA0),
                make_chr(0xB0),
                make_chr(0xC0),
                make_chr(0xD0),
            ],
        ));

        // PRG bank 2, CHR bank 3
        mapper.cpu_write(0x8000, 0b0010_0011);
        assert_eq!(mapper.ppu_read(0x0000), 0xD0);

        // PRG bank 0, CHR bank 0
        mapper.cpu_write(0x8000, 0b0000_0000);
        assert_eq!(mapper.ppu_read(0x0000), 0xA0);

        // bits 7-6 and 3-2 are unused
        mapper.cpu_write(0x8000, 0b1100_1100);
        assert_eq!(mapper.ppu_read(0x0000), 0xA0);
    }

    #[test]
    fn test_gxrom_bus_conflict_ands_with_rom() {
        let mut mapper: Box<dyn MemoryMapper> = Box::new(GxROMMapper::new(
            0,
            vec![make_prg(0b0011_0011), make_prg(0xFF)],
            vec![make_chr(0xAA), make_chr(0xBB)],
        ));

        // 0b0001_0001 & 0b0011_0011 = 0b0001_0001 => PRG 1, CHR 1
        mapper.cpu_write(0x8000, 0b0001_0001);
        assert_eq!(mapper.cpu_read(0x8000), 0xFF);
        assert_eq!(mapper.ppu_read(0x0000), 0xBB);

        // 0b0000_0000 & 0xFF = 0 => PRG 0, CHR 0
        mapper.cpu_write(0x8000, 0b0000_0000);
        assert_eq!(mapper.cpu_read(0x8000), 0b0011_0011);
        assert_eq!(mapper.ppu_read(0x0000), 0xAA);
    }

    #[test]
    fn test_gxrom_chr_rom_read_only() {
        let mut mapper: Box<dyn MemoryMapper> = Box::new(GxROMMapper::new(
            0,
            vec![make_prg(0xFF)],
            vec![make_chr(0x55)],
        ));

        assert_eq!(mapper.ppu_read(0x0100), 0x55);
        mapper.ppu_write(0x0100, 0xAA);
        assert_eq!(mapper.ppu_read(0x0100), 0x55);
    }

    #[test]
    fn test_gxrom_bank_wraps_modulo_num_banks() {
        let mut mapper: Box<dyn MemoryMapper> = Box::new(GxROMMapper::new(
            0,
            vec![make_prg(0xFF), make_prg(0xEE)],
            vec![make_chr(0xAA), make_chr(0xBB)],
        ));

        // PRG bank 2 should wrap to bank 0 with 2 banks
        mapper.cpu_write(0x8000, 0b0010_0000);
        assert_eq!(mapper.cpu_read(0x8000), 0xFF);

        // PRG bank 3 should wrap to bank 1
        mapper.cpu_write(0x8000, 0b0011_0000);
        assert_eq!(mapper.cpu_read(0x8000), 0xEE);
    }

    #[test]
    fn test_gxrom_nametable_mirroring() {
        let mut mapper: Box<dyn MemoryMapper> =
            Box::new(GxROMMapper::new(0, vec![make_prg(0xFF)], vec![make_chr(0)]));

        mapper.ppu_write(0x2000, 0x42);
        // Horizontal: $2000 = $2400
        assert_eq!(mapper.ppu_read(0x2400), 0x42);
        // $2800 is a different nametable
        assert_ne!(mapper.ppu_read(0x2800), 0x42);
    }

    #[test]
    fn test_gxrom_palette_mirroring() {
        let mut mapper: Box<dyn MemoryMapper> =
            Box::new(GxROMMapper::new(0, vec![make_prg(0xFF)], vec![make_chr(0)]));

        // $3F10 mirrors to $3F00
        mapper.ppu_write(0x3F10, 0x30);
        assert_eq!(mapper.ppu_read(0x3F00), 0x30);
    }
}
