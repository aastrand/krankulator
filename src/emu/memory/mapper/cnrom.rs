use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const CNROM_PRG_SIZE: usize = 32 * 1024;
const CNROM_CHR_BANK_SIZE: usize = 8 * 1024;
const PRG_ROM_ADDR: usize = 0x8000;

pub struct CNROMMapper {
    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    chr_banks: Vec<[u8; CNROM_CHR_BANK_SIZE]>,
    selected_chr_bank: usize,

    ppu: PpuBus,
    pub controllers: [controller::Controller; 2],
}

impl CNROMMapper {
    pub fn new(
        flags: u8,
        prg_rom: [u8; CNROM_PRG_SIZE],
        chr_banks: Vec<[u8; CNROM_CHR_BANK_SIZE]>,
    ) -> CNROMMapper {
        if chr_banks.is_empty() {
            panic!("CNROM requires at least one CHR bank");
        }

        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);
        mem[PRG_ROM_ADDR..PRG_ROM_ADDR + CNROM_PRG_SIZE].clone_from_slice(&prg_rom);
        let addr_space_ptr = mem.as_mut_ptr();

        let ppu = PpuBus::new_rom(&chr_banks[0], mirroring_from_flags(flags));

        CNROMMapper {
            _addr_space: mem,
            addr_space_ptr,
            chr_banks,
            selected_chr_bank: 0,
            ppu,
            controllers: [controller::Controller::new(), controller::Controller::new()],
        }
    }

    fn switch_chr_bank(&mut self, bank: u8) {
        let bank_index = (bank as usize) % self.chr_banks.len();
        if bank_index != self.selected_chr_bank {
            self.selected_chr_bank = bank_index;
            self.ppu.switch_chr_bank(&self.chr_banks, bank_index);
        }
    }
}

impl MemoryMapper for CNROMMapper {
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
                self.switch_chr_bank(value & 0x03);
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
        3
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let ram = unsafe { std::slice::from_raw_parts(self.addr_space_ptr, MAX_RAM_SIZE) };
        w.write_bytes(ram);
        self.ppu.save_state(w);
        w.write_u8(self.selected_chr_bank as u8);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let ram = unsafe { std::slice::from_raw_parts_mut(self.addr_space_ptr, MAX_RAM_SIZE) };
        r.read_bytes_into(ram)?;
        self.ppu.load_state(r)?;
        self.selected_chr_bank = r.read_u8()? as usize;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cnrom_chr_bank_switching() {
        let prg_rom = [0; CNROM_PRG_SIZE];
        let chr_bank1 = [1; CNROM_CHR_BANK_SIZE];
        let chr_bank2 = [2; CNROM_CHR_BANK_SIZE];
        let chr_bank3 = [3; CNROM_CHR_BANK_SIZE];
        let chr_bank4 = [4; CNROM_CHR_BANK_SIZE];

        let mut mapper: Box<dyn MemoryMapper> = Box::new(CNROMMapper::new(
            0,
            prg_rom,
            vec![chr_bank1, chr_bank2, chr_bank3, chr_bank4],
        ));

        // Initially CHR bank 0 should be selected
        assert_eq!(mapper.ppu_read(0x0000), 1);

        // Switch to CHR bank 1
        mapper.cpu_write(0x8000, 1);
        assert_eq!(mapper.ppu_read(0x0000), 2);

        // Switch to CHR bank 2
        mapper.cpu_write(0x8000, 2);
        assert_eq!(mapper.ppu_read(0x0000), 3);

        // Switch to CHR bank 3
        mapper.cpu_write(0x8000, 3);
        assert_eq!(mapper.ppu_read(0x0000), 4);

        // Test wrapping - bank 4 should wrap to bank 0
        mapper.cpu_write(0x8000, 4);
        assert_eq!(mapper.ppu_read(0x0000), 1);
    }

    #[test]
    fn test_cnrom_prg_rom_fixed() {
        let mut prg_rom = [0; CNROM_PRG_SIZE];
        prg_rom[0] = 0x42;
        prg_rom[CNROM_PRG_SIZE - 1] = 0x84;

        let chr_bank = [0; CNROM_CHR_BANK_SIZE];

        let mut mapper: Box<dyn MemoryMapper> =
            Box::new(CNROMMapper::new(0, prg_rom, vec![chr_bank]));

        assert_eq!(mapper.cpu_read(0x8000), 0x42);
        assert_eq!(mapper.cpu_read(0xFFFF), 0x84);

        // PRG ROM should remain fixed after CHR bank switch
        mapper.cpu_write(0x8000, 1);
        assert_eq!(mapper.cpu_read(0x8000), 0x42);
        assert_eq!(mapper.cpu_read(0xFFFF), 0x84);
    }

    #[test]
    fn test_cnrom_chr_rom_read_only() {
        let prg_rom = [0; CNROM_PRG_SIZE];
        let chr_bank = [0x55; CNROM_CHR_BANK_SIZE];

        let mut mapper: Box<dyn MemoryMapper> =
            Box::new(CNROMMapper::new(0, prg_rom, vec![chr_bank]));

        assert_eq!(mapper.ppu_read(0x0100), 0x55);
        mapper.ppu_write(0x0100, 0xAA);
        assert_eq!(mapper.ppu_read(0x0100), 0x55);
    }

    #[test]
    fn test_cnrom_mirroring() {
        let prg_rom = [0; CNROM_PRG_SIZE];
        let chr_bank = [0; CNROM_CHR_BANK_SIZE];

        let mut mapper_h: Box<dyn MemoryMapper> =
            Box::new(CNROMMapper::new(0, prg_rom, vec![chr_bank]));

        let mut mapper_v: Box<dyn MemoryMapper> =
            Box::new(CNROMMapper::new(1, prg_rom, vec![chr_bank]));

        mapper_h.ppu_write(0x2000, 0x20);
        mapper_v.ppu_write(0x2000, 0x10);

        assert_eq!(mapper_h.ppu_read(0x2000), 0x20);
        assert_eq!(mapper_v.ppu_read(0x2000), 0x10);
    }
}
