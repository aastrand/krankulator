use std::boxed::Box;

const MAX_RAM_SIZE: usize = 65536;

pub struct Memory {
    pub ram: [u8; MAX_RAM_SIZE]
}

impl Memory {

    pub fn new() -> Memory {
        Memory{ram: *Box::new([0; MAX_RAM_SIZE])}
    }

    pub fn get_16b_addr(&self, offset: u16) -> u16 {
        ((self.ram[offset as usize+2] as u16) << 8) + self.ram[offset as usize+1] as u16
    }
}
