use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const UXROM_PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_SIZE: usize = 8 * 1024;
const BANK_SWITCHABLE_ADDR: usize = 0x8000;
const BANK_FIXED_ADDR: usize = 0xC000;

pub struct UxROMMapper {
    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    _prg_rom: Vec<[u8; UXROM_PRG_BANK_SIZE]>,
    selected_bank: usize,

    ppu: PpuBus,
    pub controllers: [controller::Controller; 2],
}

impl UxROMMapper {
    pub fn new(flags: u8, prg_banks: Vec<[u8; UXROM_PRG_BANK_SIZE]>) -> UxROMMapper {
        if prg_banks.is_empty() {
            panic!("UxROM requires at least one PRG bank");
        }

        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);

        let last_bank = prg_banks.len() - 1;
        mem[BANK_SWITCHABLE_ADDR..BANK_SWITCHABLE_ADDR + UXROM_PRG_BANK_SIZE]
            .clone_from_slice(&prg_banks[0]);
        mem[BANK_FIXED_ADDR..BANK_FIXED_ADDR + UXROM_PRG_BANK_SIZE]
            .clone_from_slice(&prg_banks[last_bank]);

        let addr_space_ptr = mem.as_mut_ptr();

        UxROMMapper {
            _addr_space: mem,
            addr_space_ptr,
            _prg_rom: prg_banks,
            selected_bank: 0,
            ppu: PpuBus::new_ram(CHR_SIZE, mirroring_from_flags(flags)),
            controllers: [controller::Controller::new(), controller::Controller::new()],
        }
    }

    fn switch_prg_bank(&mut self, bank: u8) {
        let bank_index = (bank as usize) % self._prg_rom.len();
        self.selected_bank = bank_index;
        unsafe {
            std::ptr::copy_nonoverlapping(
                self._prg_rom[bank_index].as_ptr(),
                self.addr_space_ptr.offset(BANK_SWITCHABLE_ADDR as isize),
                UXROM_PRG_BANK_SIZE,
            );
        }
    }
}

impl MemoryMapper for UxROMMapper {
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
                self.switch_prg_bank(value & 0x0F);
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
        2
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let ram = unsafe { std::slice::from_raw_parts(self.addr_space_ptr, MAX_RAM_SIZE) };
        w.write_bytes(ram);
        self.ppu.save_state(w);
        w.write_u8(self.selected_bank as u8);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let ram = unsafe { std::slice::from_raw_parts_mut(self.addr_space_ptr, MAX_RAM_SIZE) };
        r.read_bytes_into(ram)?;
        self.ppu.load_state(r)?;
        self.selected_bank = r.read_u8()? as usize;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uxrom_bank_switching() {
        let bank1 = [1; UXROM_PRG_BANK_SIZE];
        let bank2 = [2; UXROM_PRG_BANK_SIZE];
        let bank3 = [3; UXROM_PRG_BANK_SIZE];

        let mut mapper: Box<dyn MemoryMapper> =
            Box::new(UxROMMapper::new(0, vec![bank1, bank2, bank3]));

        // Initially bank 0 should be selected for $8000-$BFFF
        assert_eq!(mapper.cpu_read(0x8000), 1);

        // Last bank (bank 2) should be fixed at $C000-$FFFF
        assert_eq!(mapper.cpu_read(0xC000), 3);

        // Switch to bank 1
        mapper.cpu_write(0x8000, 1);
        assert_eq!(mapper.cpu_read(0x8000), 2);

        // Fixed bank should remain unchanged
        assert_eq!(mapper.cpu_read(0xC000), 3);

        // Switch to bank 2
        mapper.cpu_write(0x8000, 2);
        assert_eq!(mapper.cpu_read(0x8000), 3);

        // Fixed bank should still be unchanged
        assert_eq!(mapper.cpu_read(0xC000), 3);
    }

    #[test]
    fn test_uxrom_chr_ram_write() {
        let bank1 = [0; UXROM_PRG_BANK_SIZE];
        let mut mapper: Box<dyn MemoryMapper> = Box::new(UxROMMapper::new(0, vec![bank1]));

        mapper.ppu_write(0x0100, 0x42);
        assert_eq!(mapper.ppu_read(0x0100), 0x42);

        mapper.ppu_write(0x1FFF, 0x84);
        assert_eq!(mapper.ppu_read(0x1FFF), 0x84);
    }

    #[test]
    fn test_uxrom_mirroring() {
        let bank1 = [0; UXROM_PRG_BANK_SIZE];

        let mut mapper_h: Box<dyn MemoryMapper> = Box::new(UxROMMapper::new(0, vec![bank1]));
        let mut mapper_v: Box<dyn MemoryMapper> = Box::new(UxROMMapper::new(1, vec![bank1]));

        mapper_h.ppu_write(0x2000, 0x10);
        mapper_v.ppu_write(0x2000, 0x20);

        // Horizontal: $2000 = $2400
        assert_eq!(mapper_h.ppu_read(0x2400), 0x10);

        // Vertical: $2000 = $2800
        assert_eq!(mapper_v.ppu_read(0x2800), 0x20);
    }
}
