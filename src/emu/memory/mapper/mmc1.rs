use std::cmp::max;

use std::cell::RefCell;
use std::rc::Rc;

use super::ppu;

const BANK_SIZE: usize = 16 * 1024;
const CPU_RAM_SIZE: usize = 2 * 1024;
const MMC_RAM_SIZE: usize = 8 * 1024;

const MMC_RAM_ADDR: usize = 0x6000;
const LOW_BANK_ADDR: usize = 0x8000;
const HIGH_BANK_ADDR: usize = 0xc000;

pub struct MMC1Mapper {
    ppu: Rc<RefCell<ppu::PPU>>,

    cpu_ram: Box<[u8; CPU_RAM_SIZE]>,
    mmc_ram: Box<[u8; MMC_RAM_SIZE]>,

    banks: Vec<[u8; BANK_SIZE]>,
    low_bank: *mut [u8; BANK_SIZE],
    high_bank: *mut [u8; BANK_SIZE],

    reg_write_count: u8,

    reg0: u8,
    #[allow(dead_code)]
    reg1: u8,
    #[allow(dead_code)]
    reg2: u8,
    reg3: u8,
}

impl MMC1Mapper {
    pub fn new(mut prg_banks: Vec<[u8; BANK_SIZE]>) -> MMC1Mapper {
        if prg_banks.len() < 2 {
            panic!("Expected at least two PRG banks");
        }
        if prg_banks.len() % 2 != 0 {
            panic!("Expected an even amount of PRG banks");
        }

        let low_bank_ptr: *mut [u8; BANK_SIZE] = unsafe { prg_banks.get_unchecked_mut(0) };

        let mut mapper = MMC1Mapper {
            ppu: Rc::new(RefCell::new(ppu::PPU::new())),

            // 0x0000-0x07FF + mirroring to 0x1FFF
            cpu_ram: Box::new([0; CPU_RAM_SIZE]),

            // 0x6000-0x7FFF
            mmc_ram: Box::new([0; MMC_RAM_SIZE]),

            banks: prg_banks,

            // 0x8000-0xBFFF
            low_bank: low_bank_ptr,
            //  0xC000-0xFFFF
            high_bank: low_bank_ptr,

            reg_write_count: 0,

            // Control
            // 0x8000-0x9FFF
            reg0: 0,
            // CHR bank 0
            // 0xA000-0xBFFF
            reg1: 0,
            // CHR bank 1
            // 0xC000-0xDFFF
            reg2: 0,
            // PRG bank
            // 0xE000-0xFFFF
            reg3: 0,
        };

        unsafe {
            let len = mapper.banks.len();
            mapper.high_bank = mapper.banks.get_unchecked_mut(max(0, len - 1));
        }

        mapper
    }

    fn check_register_reset(&mut self, value: u8) {
        /*
        The "reset" signal is generated by writing a byte value whose high bit is a 1
        to any of the four MMC1 registers.  This signal affects the bits of reg0 as
        follows:

            bit 0 - unknown
            bit 1 - unknown
            bit 2 - reset to logic 1
            bit 3 - reset to logic 1
            bit 4 - unaffected
        */
        if value & 0x80 == 0x80 {
            self.reg0 |= 0b00110;
            self.reg_write_count = 0;
        }
    }

    fn _read_bus(&self, mut addr: usize) -> u8 {
        addr = super::mirror_addr(addr);
        let page = super::addr_to_page(addr);

        match page {
            0x0 | 0x10 => self.cpu_ram[addr],
            0x20 => {
                // PPU registers
                if addr == 0x2002 {
                    0x80
                } else {
                    0
                }
            }
            0x40 => {
                // TODO: APU goes here, 0x40
                if addr > 0x4017 {
                    panic!("Write at addr {:X} not mapped", addr);
                }
                0
            }
            0x60 | 0x70 => self.mmc_ram[addr - MMC_RAM_ADDR],
            0x80 | 0x90 | 0xa0 | 0xb0 => unsafe { (*self.low_bank)[addr - LOW_BANK_ADDR] },
            0xc0 | 0xd0 | 0xe0 | 0xf0 => unsafe { (*self.high_bank)[addr - HIGH_BANK_ADDR] },
            _ => panic!("Read at addr {:X} not mapped", addr),
        }
    }

    fn _write_bus(&mut self, mut addr: usize, value: u8) {
        addr = super::mirror_addr(addr);
        let page = super::addr_to_page(addr);

        match page {
            0x0 | 0x10 => self.cpu_ram[addr] = value,
            0x20 => {
                // TODO: PPU registers
                //println!("Write to PPU reg {:X}: {:X}", addr, value);
            }
            0x40 => {
                // TODO: APU goes here, 0x40
                //println!("Write to APU   reg {:X}: {:X}", addr, value);
                if addr > 0x4017 {
                    panic!("Write at addr {:X} not mapped", addr);
                }
            }
            0x50 => {} // ??
            0x60 | 0x70 => {
                self.mmc_ram[addr - MMC_RAM_ADDR] = value;
            }
            0x80 | 0x90 => {
                self.reg_write_count = self.reg_write_count + 1;
                self.check_register_reset(value);
                //println!("Write to MMC reg0: {:X}", value);
                if self.reg_write_count == 5 {
                    /*
                    bit 2 - toggles between low PRGROM area switching and high
                        PRGROM area switching
                        0 = high PRGROM switching, 1 = low PRGROM switching

                    bit 3 - toggles between 16KB and 32KB PRGROM bank switching
                        0 = 32KB PRGROM switching, 1 = 16KB PRGROM switching
                    */
                    self.reg0 = value;
                    self.reg_write_count = 0;
                    //println!("Set reg0 to {:X}", value);
                }
            }
            0xa0 | 0xb0 => {
                self.reg_write_count = (self.reg_write_count + 1) % 5;
                self.check_register_reset(value);
                //println!("Write to MMC reg1: {:X}", value);
            }
            0xc0 | 0xd0 => {
                self.reg_write_count = (self.reg_write_count + 1) % 5;
                self.check_register_reset(value);
                //println!("Write to MMC reg2: {:X}", value);
            }
            0xe0 | 0xf0 => {
                self.reg_write_count = self.reg_write_count + 1;
                self.check_register_reset(value);
                //println!("Write to MMC reg3: {:X}", value);

                if self.reg_write_count == 5 {
                    self.reg3 = value;

                    if self.reg3 & 0b100 == 0b100 {
                        // 32k bank switching
                        let base_bank = value & 0b110;
                        unsafe {
                            self.low_bank = self.banks.get_unchecked_mut((base_bank * 2) as usize);
                            self.high_bank = self.banks.get_unchecked_mut(((base_bank * 2) + 1) as usize);
                        }
                    //println!("Switched low bank to {:X} (32K mode)", self.low_bank);
                    //println!("Switched high bank to {:X} (32K mode)", self.high_bank);
                    } else if self.reg3 & 0b10 == 0b10 {
                        unsafe {
                            self.low_bank = self.banks.get_unchecked_mut((value & 0xf) as usize);
                        }
                    //println!("Switched low bank to {:X}", self.low_bank)
                    } else {
                        unsafe {
                            self.high_bank = self.banks.get_unchecked_mut((value & 0xf) as usize);
                        }
                        //println!("Switched high bank to {:X}", self.high_bank);
                    }

                    self.reg_write_count = 0;
                }
            }
            _ => panic!("Write at addr {:X} not mapped", addr),
        };
    }
}

impl super::MemoryMapper for MMC1Mapper {
    fn read_bus(&self, addr: usize) -> u8 {
        self._read_bus(addr)
    }

    fn write_bus(&mut self, addr: usize, value: u8) {
        self._write_bus(addr, value);
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
    fn test_code_start() {
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for _ in 0..16 {
            prg_banks.push([0; BANK_SIZE]);
        }

        prg_banks[15][0x3ffc] = 0x11;
        prg_banks[15][0x3ffd] = 0x47;

        let mapper: Box<dyn super::super::MemoryMapper> = Box::new(MMC1Mapper::new(prg_banks));

        assert_eq!(mapper.code_start(), 0x4711);
    }

    #[test]
    fn test_ram() {
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for _ in 0..16 {
            prg_banks.push([0; BANK_SIZE]);
        }

        let mut mapper = MMC1Mapper::new(prg_banks);

        // CPU ram
        mapper._write_bus(0x1173, 0x42);
        assert_eq!(mapper._read_bus(0x1173), 0x42);
        assert_eq!(mapper.cpu_ram[0x173], 0x42);

        // PRG ram
        mapper._write_bus(0x6123, 0x11);
        assert_eq!(mapper._read_bus(0x6123), 0x11);
        assert_eq!(mapper.mmc_ram[0x0123], 0x11);
    }

    #[test]
    fn test_reset() {
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for _ in 0..16 {
            prg_banks.push([0; BANK_SIZE]);
        }

        let mut mapper = MMC1Mapper::new(prg_banks);
        mapper.reg0 = 0b11101;

        mapper._write_bus(0x8000, 0x80);

        assert_eq!(mapper.reg0, 0b11111);
    }

    #[test]
    fn test_write_reg0() {
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for _ in 0..16 {
            // Init with non-zero
            prg_banks.push([1; BANK_SIZE]);
        }

        // TODO: initial reg0 value might change in the future
        let mut mapper = MMC1Mapper::new(prg_banks);

        // Write to mmaped mmc reg, note that rom does not change
        mapper._write_bus(0x8000, 32);
        assert_eq!(mapper._read_bus(0x8000), 1);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xa000, 42);
        assert_eq!(mapper._read_bus(0xa000), 1);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xe000, 6);
        assert_eq!(mapper._read_bus(0xe000), 1);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xc000, 9);
        assert_eq!(mapper._read_bus(0xc000), 1);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0x8000, 0b00100);
        assert_eq!(mapper._read_bus(0x8000), 1);
        assert_eq!(mapper.reg0, 0b00100);
    }

    // TODO:
    // * test for writing reg3
    // * test for 16k low switching
    // * test for 16k high switching
    // * test for 32k switchig
}
