use std::cell::RefCell;
use std::rc::Rc;

use super::super::*;

/*
All Banks are fixed,

CPU $6000-$7FFF: Family Basic only: PRG RAM, mirrored as necessary to fill entire 8 KiB window, write protectable with an external switch
CPU $8000-$BFFF: First 16 KB of ROM.
CPU $C000-$FFFF: Last 16 KB of ROM (NROM-256) or mirror of $8000-$BFFF (NROM-128).
*/

const NROM_PRG_BANK_SIZE: usize = 16 * 1024;
const NROM_CHR_BANK_SIZE: usize = 8 * 1024;
const BANK_ONE_ADDR: usize = 0x8000;
const BANK_TWO_ADDR: usize = 0xC000;

pub struct NROMMapper {
    ppu: Rc<RefCell<ppu::PPU>>,
    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,
    _chr_bank: Box<[u8; NROM_CHR_BANK_SIZE]>,
}

impl NROMMapper {
    // TODO: PRG RAM
    pub fn new(
        bank_one: Box<[u8; NROM_PRG_BANK_SIZE]>,
        bank_two: Option<[u8; NROM_PRG_BANK_SIZE]>,
        chr_rom: Option<[u8; NROM_CHR_BANK_SIZE]>,
    ) -> NROMMapper {
        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);

        mem[BANK_ONE_ADDR..BANK_ONE_ADDR + NROM_PRG_BANK_SIZE].clone_from_slice(&*bank_one);

        let second = if bank_two.is_some() {
            bank_two.unwrap()
        } else {
            *bank_one
        };
        mem[BANK_TWO_ADDR..BANK_TWO_ADDR + NROM_PRG_BANK_SIZE].clone_from_slice(&second);

        let addr_space_ptr = mem.as_mut_ptr();

        NROMMapper {
            ppu: Rc::new(RefCell::new(ppu::PPU::new())),
            _addr_space: mem,
            addr_space_ptr: addr_space_ptr,
            _chr_bank: Box::new(chr_rom.unwrap_or([0; NROM_CHR_BANK_SIZE])),
        }
    }
}

impl MemoryMapper for NROMMapper {
    fn read_bus(&self, addr: u16) -> u8 {
        let addr = super::mirror_addr(addr);
        if addr >= 0x2000 && addr < 0x2008 {
            self.ppu.borrow_mut().read(addr)
        } else {
            unsafe { *self.addr_space_ptr.offset(addr as _) }
        }
    }

    fn write_bus(&mut self, addr: u16, value: u8) {
        let addr = super::mirror_addr(addr);

        if addr >= 0x2000 && addr < 0x2008 {
            self.ppu.borrow_mut().write(addr, value);
        }

        // TODO: check that we are within ram bounds
        unsafe {
            *self.addr_space_ptr.offset(addr as _) = value
        }
    }

    fn code_start(&self) -> u16 {
        ((self.read_bus(super::RESET_TARGET_ADDR + 1) as u16) << 8) as u16
            + self.read_bus(super::RESET_TARGET_ADDR) as u16
    }

    fn install_ppu(&mut self, ppu: Rc<RefCell<ppu::PPU>>) {
        self.ppu = ppu;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nrom_ram_mirroring() {
        let mut mapper: Box<dyn MemoryMapper> =
            Box::new(NROMMapper::new(Box::new([0; 16384]), None, Some([0; 8192])));
        mapper.write_bus(0x173, 0x42);

        assert_eq!(mapper.read_bus(0x173), 0x42);
        assert_eq!(mapper.read_bus(0x973), 0x42);
        assert_eq!(mapper.read_bus(0x1173), 0x42);
        assert_eq!(mapper.read_bus(0x1973), 0x42);

        mapper.write_bus(0x2000, 0x80);
        mapper.write_bus(0x2001, 0x11);

        assert_eq!(mapper.read_bus(0x2000), 0x80);
        assert_eq!(mapper.read_bus(0x2001), 0x11);

        assert_eq!(mapper.read_bus(0x2008), 0x80);
        assert_eq!(mapper.read_bus(0x2009), 0x11);

        assert_eq!(mapper.read_bus(0x2010), 0x80);
        assert_eq!(mapper.read_bus(0x2011), 0x11);

        // a write to $3456 is the same as a write to $2006.

        mapper.write_bus(0x3456, 0x32);
        assert_eq!(mapper.read_bus(0x3456), 0x32);
        assert_eq!(mapper.read_bus(0x2006), 0x32);
    }
}
