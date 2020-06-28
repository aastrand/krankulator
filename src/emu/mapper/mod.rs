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
    }}

impl MemoryMapper for IdentityMapper {
    fn read_bus(&self, addr: u16) -> u8 {
        self.ram[addr as usize]
    }

    fn write_bus(&mut self, addr: u16, value: u8) {
        self.ram[addr as usize] = value
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