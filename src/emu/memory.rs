use std::boxed::Box;

const MAX_RAM_SIZE: usize = 65536;

pub struct Memory {
    pub ram: [u8; MAX_RAM_SIZE],
}

impl Memory {
    pub fn new() -> Memory {
        Memory {
            ram: *Box::new([0; MAX_RAM_SIZE]),
        }
    }

    pub fn get_16b_addr(&self, offset: u16) -> u16 {
        // little endian, so 2nd first
        ((self.ram[offset as usize + 2] as u16) << 8) + self.ram[offset as usize + 1] as u16
    }

    pub fn value_at_addr(&self, addr: u16) -> u8 {
        self.ram[addr as usize]
    }

    pub fn indirect_value_at_addr(&self, addr: u16) -> u8 {
        self.ram[self.ram[addr as usize] as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_16b_addr() {
        let mut memory: Memory = Memory::new();
        memory.ram[0x2001] = 0x11;
        memory.ram[0x2002] = 0x47;

        let value = memory.get_16b_addr(0x2000);

        assert_eq!(value, 0x4711);
    }

    #[test]
    fn test_value_at_addr() {
        let mut memory: Memory = Memory::new();
        memory.ram[0x2001] = 0x11;

        let value = memory.value_at_addr(0x2001);

        assert_eq!(value, 0x11);
    }

    #[test]
    fn test_indirect_value_at_addr() {
        let mut memory: Memory = Memory::new();
        memory.ram[0x2001] = 0x11;
        memory.ram[0x11] = 0x47;

        let value = memory.indirect_value_at_addr(0x2001);

        assert_eq!(value, 0x47);
    }
}
