use std::boxed::Box;

pub const MAX_RAM_SIZE: usize = 65536;
pub const BRK_TARGET_ADDR: u16 = 0xfffe;
pub const CODE_START_ADDR: u16 = 0x600;
pub const STACK_BASE_OFFSET: u16 = 0x100;
pub const STACK_START_ADDR: u8 = 0xff;

pub struct Memory {
    pub ram: [u8; MAX_RAM_SIZE],
}

impl Memory {
    pub fn new() -> Memory {
        Memory {
            ram: *Box::new([0; MAX_RAM_SIZE]),
        }
    }

    pub fn addr_absolute(&self, pc: u16) -> u16 {
        self.get_16b_addr(pc + 1) as u16
    }

    pub fn addr_absolute_idx(&self, pc: u16, idx: u8) -> u16 {
        self.get_16b_addr(pc + 1).wrapping_add(idx as u16)
    }

    pub fn addr_idx_indirect(&self, pc: u16, idx: u8) -> u16 {
        let value: u8 = self.value_at_addr(pc + 1).wrapping_add(idx);
        self.get_16b_addr(value as u16)
    }

    pub fn addr_indirect_idx(&self, pc: u16, idx: u8) -> u16 {
        let base = self.value_at_addr(pc + 1);

        let lb = self.value_at_addr(base as u16);
        let lbidx = lb.wrapping_add(idx);
        let carry: u8 = if lbidx <= lb && idx > 0 { 1 } else { 0 };
        let hb = self.value_at_addr((base + 1) as u16).wrapping_add(carry);

        Memory::to_16b_addr(hb, lbidx)
    }

    pub fn addr_zeropage(&self, pc: u16) -> u16 {
        self.value_at_addr(pc + 1) as u16
    }

    pub fn addr_zeropage_idx(&self, pc: u16, idx: u8) -> u16 {
        self.value_at_addr(pc + 1).wrapping_add(idx) as u16
    }

    pub fn get_16b_addr(&self, offset: u16) -> u16 {
        // little endian, so 2nd first
        ((self.ram[offset as usize + 1] as u16) << 8) + self.ram[offset as usize] as u16
    }

    pub fn value_at_addr(&self, addr: u16) -> u8 {
        self.ram[addr as usize]
    }

    pub fn indirect_value_at_addr(&self, addr: u16) -> u8 {
        self.ram[self.ram[addr as usize] as usize]
    }

    pub fn stack_addr(&self, sp: u8) -> u16 {
        STACK_BASE_OFFSET + (u16::from(sp) & 0xff)
    }

    pub fn push_to_stack(&mut self, sp: u8, value: u8) {
        self.ram[self.stack_addr(sp) as usize] = value;
    }

    pub fn pull_from_stack(&mut self, sp: u8) -> u8 {
        self.ram[self.stack_addr(sp) as usize]
    }

    pub fn store(&mut self, addr: u16, value: u8) {
        self.ram[addr as usize] = value;
    }
    pub fn to_16b_addr(hb: u8, lb: u8) -> u16 {
        ((hb as u16) << 8) + ((lb as u16) & 0xff)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_addr_absolute() {
        let mut memory: Memory = Memory::new();
        memory.ram[0x2001] = 0x11;
        memory.ram[0x2002] = 0x47;

        let value = memory.addr_absolute(0x2000);

        assert_eq!(value, 0x4711);
    }

    #[test]
    fn test_addr_absolute_idx() {
        let mut memory: Memory = Memory::new();
        memory.ram[0x2001] = 0x10;
        memory.ram[0x2002] = 0x47;

        let value = memory.addr_absolute_idx(0x2000, 1);

        assert_eq!(value, 0x4711);
    }

    #[test]
    fn test_addr_idx_indirect() {
        let mut memory: Memory = Memory::new();
        memory.ram[0x2001] = 0x41;
        memory.ram[0x42] = 0x11;
        memory.ram[0x43] = 0x47;

        let value = memory.addr_idx_indirect(0x2000, 0x1);

        assert_eq!(value, 0x4711);
    }

    #[test]
    fn test_addr_idx_indirect_wrap() {
        let mut memory: Memory = Memory::new();
        memory.ram[0x2001] = 0x43;
        memory.ram[0x42] = 0x11;
        memory.ram[0x43] = 0x47;

        let value = memory.addr_idx_indirect(0x2000, 0xff);

        assert_eq!(value, 0x4711);
    }

    #[test]
    fn test_addr_indirect_idx() {
        let mut memory: Memory = Memory::new();
        memory.ram[0x2001] = 0x42;
        memory.ram[0x42] = 0x10;
        memory.ram[0x43] = 0x47;

        let value = memory.addr_indirect_idx(0x2000, 0x1);

        assert_eq!(value, 0x4711);
    }

    #[test]
    fn test_addr_indirect_idx_wrap_with_carry() {
        let mut memory: Memory = Memory::new();
        memory.ram[0x2001] = 0x42;
        memory.ram[0x42] = 0x12;
        memory.ram[0x43] = 0x46;

        let value = memory.addr_indirect_idx(0x2000, 0xff);

        assert_eq!(value, 0x4711);
    }

    #[test]
    fn test_addr_zeropage() {
        let mut memory: Memory = Memory::new();
        memory.ram[0x2001] = 0x11;

        let value = memory.addr_zeropage(0x2000);

        assert_eq!(value, 0x11);
    }

    #[test]
    fn test_addr_zeropage_idx() {
        let mut memory: Memory = Memory::new();
        memory.ram[0x2001] = 0x10;

        let value = memory.addr_zeropage_idx(0x2000, 0x1);

        assert_eq!(value, 0x11);
    }

    #[test]
    fn test_addr_zeropage_idx_wrap() {
        let mut memory: Memory = Memory::new();
        memory.ram[0x2001] = 0x43;

        let value = memory.addr_zeropage_idx(0x2000, 0xff);

        assert_eq!(value, 0x42);
    }

    #[test]
    fn test_get_16b_addr() {
        let mut memory: Memory = Memory::new();
        memory.ram[0x2001] = 0x11;
        memory.ram[0x2002] = 0x47;

        let value = memory.get_16b_addr(0x2001);

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

    #[test]
    fn test_push_to_stack() {
        let mut memory: Memory = Memory::new();
        memory.push_to_stack(0xff, 0x42);

        assert_eq!(memory.ram[0x1ff as usize], 0x42);
    }

    #[test]
    fn test_pull_from_stack() {
        let mut memory: Memory = Memory::new();
        memory.ram[0x1ff as usize] = 0x42;
        let value: u8 = memory.pull_from_stack(0xff);

        assert_eq!(value, 0x42);
    }

    #[test]
    fn test_stack_addr() {
        let memory: Memory = Memory::new();

        assert_eq!(memory.stack_addr(0xfd), 0x1fd);
    }

    #[test]
    fn test_store() {
        let mut memory: Memory = Memory::new();
        memory.store(0x200, 0xff);

        assert_eq!(memory.ram[0x200], 0xff);
    }

    #[test]
    fn test_to_16b_addr() {
        assert_eq!(Memory::to_16b_addr(0x47, 0x11), 0x4711);
    }
}
