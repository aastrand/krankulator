pub const MAX_RAM_SIZE: usize = 65536;

pub trait MemoryMapper {
    fn read_bus(&self, addr: u16) -> u8;
    fn write_bus(&mut self, addr: u16, value: u8);
}

pub struct IdentityMapper {
    ram: [u8; MAX_RAM_SIZE],
}

impl IdentityMapper {
    pub fn new() -> IdentityMapper {
        IdentityMapper {
            ram: *Box::new([0; MAX_RAM_SIZE]),
        }
    }
}

impl MemoryMapper for IdentityMapper {
    fn read_bus(&self, addr: u16) -> u8 {
        self.ram[addr as usize]
    }

    fn write_bus(&mut self, addr: u16, value: u8) {
        self.ram[addr as usize] = value
    }
}

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
    addr_space: Box<[u8; MAX_RAM_SIZE]>,
}

impl NROMMapper {
    // TODO: PRG RAM
    pub fn new(
        bank_one: [u8; NROM_BANK_SIZE],
        bank_two: Option<[u8; NROM_BANK_SIZE]>,
    ) -> NROMMapper {
        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);

        mem[BANK_ONE_ADDR..BANK_ONE_ADDR + NROM_BANK_SIZE].clone_from_slice(&bank_one);

        let second = if bank_two.is_some() {
            bank_two.unwrap()
        } else {
            bank_one
        };
        mem[BANK_TWO_ADDR..BANK_TWO_ADDR + NROM_BANK_SIZE].clone_from_slice(&second);

        NROMMapper { addr_space: mem }
    }
}

impl MemoryMapper for NROMMapper {
    fn read_bus(&self, addr: u16) -> u8 {
        self.addr_space[addr as usize]
    }

    fn write_bus(&mut self, addr: u16, value: u8) {
        self.addr_space[addr as usize] = value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_bus() {
        let mut mapper = IdentityMapper::new();
        mapper.ram[0] = 42;
        assert_eq!(mapper.read_bus(0), 42);
    }

    #[test]
    fn test_write_bus() {
        let mut mapper = IdentityMapper::new();
        mapper.write_bus(1, 42);
        assert_eq!(mapper.ram[1], 42);
    }
}
