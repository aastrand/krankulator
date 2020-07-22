pub mod mmc1;
pub mod nrom;

pub const MAX_RAM_SIZE: usize = 65536;
pub const RESET_TARGET_ADDR: usize = 0xfffc;

pub fn mirror_addr(addr: usize) -> usize {
    // System memory at $0000-$07FF is mirrored at $0800-$0FFF, $1000-$17FF, and $1800-$1FFF
    // - attempting to access memory at, for example, $0173 is the same as accessing memory at $0973, $1173, or $1973.
    if addr < 0x2000 {
        addr % 0x800
    } else if addr < 0x4000 {
        0x2000 + (addr % 0x8)
    } else {
        addr
    }
}

pub trait MemoryMapper {
    fn read_bus(&self, addr: usize) -> u8;
    fn write_bus(&mut self, addr: usize, value: u8);
    fn code_start(&self) -> u16;
}

pub struct IdentityMapper {
    ram: [u8; MAX_RAM_SIZE],
    code_start: u16,
}

impl IdentityMapper {
    pub fn new(code_start: u16) -> IdentityMapper {
        IdentityMapper {
            ram: *Box::new([0; MAX_RAM_SIZE]),
            code_start: code_start,
        }
    }
}

impl MemoryMapper for IdentityMapper {
    fn read_bus(&self, addr: usize) -> u8 {
        self.ram[addr as usize]
    }

    fn write_bus(&mut self, addr: usize, value: u8) {
        self.ram[addr as usize] = value
    }

    fn code_start(&self) -> u16 {
        self.code_start
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_bus() {
        let mut mapper = IdentityMapper::new(0);
        mapper.ram[0] = 42;
        assert_eq!(mapper.read_bus(0), 42);
    }

    #[test]
    fn test_write_bus() {
        let mut mapper = IdentityMapper::new(0);
        mapper.write_bus(1, 42);
        assert_eq!(mapper.ram[1], 42);
    }

    #[test]
    fn test_addr_mirroring() {
        assert_eq!(mirror_addr(0x973), 0x173);
        assert_eq!(mirror_addr(0x3002), 0x2002);
        assert_eq!(mirror_addr(0x8000), 0x8000);
    }
}
