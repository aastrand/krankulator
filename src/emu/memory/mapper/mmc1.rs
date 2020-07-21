const NROM_BANK_SIZE: usize = 16 * 1024;

pub struct MMC1Mapper {
    addr_space: Box<[u8; super::MAX_RAM_SIZE]>,
}

impl MMC1Mapper {
    // TODO: PRG RAM
    pub fn new(_banks: Vec<Box<[u8; NROM_BANK_SIZE]>>) -> MMC1Mapper {
        let mut mem: Box<[u8; super::MAX_RAM_SIZE]> = Box::new([0; super::MAX_RAM_SIZE]);

        // Fake vblank
        mem[0x2002] = 0x80;

        MMC1Mapper { addr_space: mem }
    }
}

impl super::MemoryMapper for MMC1Mapper {
    fn read_bus(&self, mut addr: u16) -> u8 {
        addr = super::mirror_addr(addr);
        self.addr_space[addr as usize]
    }

    fn write_bus(&mut self, mut addr: u16, value: u8) {
        addr = super::mirror_addr(addr);

        if addr >= 0x2000 && addr < 0x2008 {
            // TODO: ppu control registers?
        }

        self.addr_space[addr as usize] = value
    }

    fn code_start(&self) -> u16 {
        ((self.read_bus(super::RESET_TARGET_ADDR + 1) as u16) << 8) as u16
            + self.read_bus(super::RESET_TARGET_ADDR) as u16
    }
}
