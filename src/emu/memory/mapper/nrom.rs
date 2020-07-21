/*
All Banks are fixed,

CPU $6000-$7FFF: Family Basic only: PRG RAM, mirrored as necessary to fill entire 8 KiB window, write protectable with an external switch
CPU $8000-$BFFF: First 16 KB of ROM.
CPU $C000-$FFFF: Last 16 KB of ROM (NROM-256) or mirror of $8000-$BFFF (NROM-128).
*/

const NROM_BANK_SIZE: usize = 16 * 1024;
const BANK_ONE_ADDR: usize = 0x8000;
const BANK_TWO_ADDR: usize = 0xC000;

pub struct NROMMapper {
    addr_space: Box<[u8; super::MAX_RAM_SIZE]>,
}

impl NROMMapper {
    // TODO: PRG RAM
    pub fn new(
        bank_one: [u8; NROM_BANK_SIZE],
        bank_two: Option<[u8; NROM_BANK_SIZE]>,
    ) -> NROMMapper {
        let mut mem: Box<[u8; super::MAX_RAM_SIZE]> = Box::new([0; super::MAX_RAM_SIZE]);

        mem[BANK_ONE_ADDR..BANK_ONE_ADDR + NROM_BANK_SIZE].clone_from_slice(&bank_one);

        let second = if bank_two.is_some() {
            bank_two.unwrap()
        } else {
            bank_one
        };
        mem[BANK_TWO_ADDR..BANK_TWO_ADDR + NROM_BANK_SIZE].clone_from_slice(&second);

        // Fake vblank
        mem[0x2002] = 0x80;

        NROMMapper { addr_space: mem }
    }
}

impl super::MemoryMapper for NROMMapper {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nrom_ram_mirroring() {
        let mut mapper: Box<dyn super::super::MemoryMapper> = Box::new(NROMMapper::new([0; 16384], None));
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
