use std::cmp::max;

const BANK_SIZE: usize = 16 * 1024;
const CPU_RAM_SIZE: usize = 2 * 1024;
const MMC_RAM_SIZE: usize = 8 * 1024;

const LOW_BANK_ADDR: usize = 0x8000;
const HIGH_BANK_ADDR: usize = 0xc000;

pub struct MMC1Mapper {
    cpu_ram: Box<[u8; CPU_RAM_SIZE]>,
    mmc_ram: Box<[u8; MMC_RAM_SIZE]>,

    banks: Box<Vec<Box<[u8; BANK_SIZE]>>>,
    low_bank: usize,
    high_bank: usize,

    reg0: u8,
    reg0_count: u8,

    reg1: u8,
    reg1_count: u8,

    reg2: u8,
    reg2_count: u8,

    reg3: u8,
    reg3_count: u8,
}

impl MMC1Mapper {
    // TODO: PRG RAM
    pub fn new(prg_banks: Vec<Box<[u8; BANK_SIZE]>>) -> MMC1Mapper {
        let mut mapper = MMC1Mapper {
            // 0x0000-0x07FF + mirroring to 0x1FFF
            cpu_ram: Box::new([0; CPU_RAM_SIZE]),

            // 0x6000-0x7FFF
            mmc_ram: Box::new([0; MMC_RAM_SIZE]),

            banks: Box::new(prg_banks),

            // 0x8000-0xBFFF
            low_bank: 0,
            //  0xC000-0xFFFF
            high_bank: 0,

            // Control
            // 0x8000-0x9FFF
            reg0: 0,
            reg0_count: 0,
            // CHR bank 0
            // 0xA000-0xBFFF
            reg1: 0,
            reg1_count: 0,
            // CHR bank 1
            // 0xC000-0xDFFF
            reg2: 0,
            reg2_count: 0,
            // PRG bank
            // 0xE000-0xFFFF
            reg3: 0,
            reg3_count: 0,
        };

        mapper.high_bank = max(0, mapper.banks.len() - 1);

        mapper
    }
}

impl super::MemoryMapper for MMC1Mapper {
    fn read_bus(&self, mut addr: usize) -> u8 {
        addr = super::mirror_addr(addr);

        if addr < 0x2000 {
            self.cpu_ram[addr]
        } else if addr >= 0x2000 && addr < 0x2008 {
            0
        } else if addr >= LOW_BANK_ADDR && addr < LOW_BANK_ADDR + BANK_SIZE {
            self.banks[self.low_bank][addr - LOW_BANK_ADDR as usize]
        } else if addr >= HIGH_BANK_ADDR && addr < HIGH_BANK_ADDR + BANK_SIZE {
            self.banks[self.high_bank][addr - HIGH_BANK_ADDR as usize]
        } else {
            0
        }
    }

    fn write_bus(&mut self, mut addr: usize, value: u8) {
        addr = super::mirror_addr(addr);

        if addr < 0x2000 {
            self.cpu_ram[addr as usize] = value
        }

        if addr >= 0x2000 && addr < 0x2008 {
            // TODO: ppu control registers?
        }
    }

    fn code_start(&self) -> u16 {
        ((self.read_bus(super::RESET_TARGET_ADDR + 1) as u16) << 8) as u16
            + self.read_bus(super::RESET_TARGET_ADDR) as u16
    }
}
