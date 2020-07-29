pub mod mapper;

use std::cell::RefCell;
use std::rc::Rc;

use super::ppu;

pub const NMI_TARGET_ADDR: u16 = 0xfffa;
pub const BRK_TARGET_ADDR: u16 = 0xfffe;
pub const CODE_START_ADDR: u16 = 0x600;
pub const STACK_START_ADDR: u16 = 0xff;

pub const MAX_RAM_SIZE: usize = 65536;
pub const STACK_BASE_OFFSET: u16 = 0x100;

pub fn to_16b_addr(hb: u8, lb: u8) -> u16 {
    ((hb as u16) << 8) + (lb as u16)
}

pub trait MemoryMapper {
    fn read_bus(&self, addr: u16) -> u8;
    fn write_bus(&mut self, addr: u16, value: u8);
    fn code_start(&self) -> u16;
    fn install_ppu(&mut self, ppu: Rc<RefCell<ppu::PPU>>);

    fn addr_absolute(&self, pc: u16) -> u16 {
        self.get_16b_addr(pc.wrapping_add(1) as _)
    }

    fn get_16b_addr(&self, offset: u16) -> u16 {
        to_16b_addr(self.read_bus(offset.wrapping_add(1)), self.read_bus(offset))
    }

    fn addr_absolute_idx(&self, pc: u16, idx: u8) -> (u16, bool) {
        let lb = self.read_bus(pc.wrapping_add(1));
        (
            to_16b_addr(self.read_bus(pc.wrapping_add(2)), lb).wrapping_add(idx as u16),
            // Did we cross the page boundary?
            (lb & 0xff) as u16 + idx as u16 > 0xff,
        )
    }

    fn addr_idx_indirect(&self, pc: u16, idx: u8) -> u16 {
        let value: u8 = self.read_bus((pc + 1) as _).wrapping_add(idx);
        ((self.read_bus(((value as u8).wrapping_add(1)) as u16) as u16) << 8)
            + self.read_bus(value as u16) as u16
    }

    fn addr_indirect_idx(&self, pc: u16, idx: u8) -> (u16, bool) {
        let base = self.read_bus(pc + 1);

        let lb = self.read_bus(base as _);
        let lbidx = lb.wrapping_add(idx);
        let carry: u8 = if lbidx <= lb && idx > 0 { 1 } else { 0 };
        let hb = self
            .read_bus((base as u8).wrapping_add(1) as _)
            .wrapping_add(carry);

        (to_16b_addr(hb, lbidx) as _, carry != 0)
    }

    fn addr_zeropage(&self, pc: u16) -> u16 {
        self.read_bus(pc + 1) as _
    }

    fn addr_zeropage_idx(&self, pc: u16, idx: u8) -> u16 {
        self.read_bus(pc + 1).wrapping_add(idx) as u16
    }

    fn stack_addr(&self, sp: u8) -> u16 {
        STACK_BASE_OFFSET + (u16::from(sp) & 0xff)
    }

    fn push_to_stack(&mut self, sp: u8, value: u8) {
        self.write_bus(self.stack_addr(sp), value);
    }

    fn pull_from_stack(&mut self, sp: u8) -> u8 {
        self.read_bus(self.stack_addr(sp))
    }

    fn raw_opcode(&self, addr: u16) -> [u8; 3] {
        [
            self.read_bus(addr),
            self.read_bus(addr.wrapping_add(1)),
            self.read_bus(addr.wrapping_add(2)),
        ]
    }
}

pub struct IdentityMapper {
    _ram: Box<[u8; MAX_RAM_SIZE]>,
    ram_ptr: *mut u8,
    code_start: u16,
}

impl IdentityMapper {
    pub fn new(code_start: u16) -> IdentityMapper {
        let mut ram = Box::new([0; MAX_RAM_SIZE]);
        let ram_ptr = ram.as_mut_ptr();
        IdentityMapper {
            _ram: ram,
            ram_ptr,
            code_start: code_start,
        }
    }
}

impl MemoryMapper for IdentityMapper {
    #[inline]
    fn read_bus(&self, addr: u16) -> u8 {
        unsafe { *self.ram_ptr.offset(addr as _) }
    }

    #[inline]
    fn write_bus(&mut self, addr: u16, value: u8) {
        unsafe { *self.ram_ptr.offset(addr as _) = value }
    }

    fn code_start(&self) -> u16 {
        self.code_start
    }

    fn install_ppu(&mut self, _ppu: Rc<RefCell<ppu::PPU>>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_addr_absolute() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x2001, 0x11);
        memory.write_bus(0x2002, 0x47);

        let value = memory.addr_absolute(0x2000);

        assert_eq!(value, 0x4711);
    }

    #[test]
    fn test_addr_absolute_idx() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x2001, 0x10);
        memory.write_bus(0x2002, 0x47);

        let value = memory.addr_absolute_idx(0x2000, 1);

        assert_eq!(value.0, 0x4711);
        assert_eq!(value.1, false);
    }

    #[test]
    fn test_addr_absolute_idx_crossed_boundary() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x2001, 0xff);
        memory.write_bus(0x2002, 0x46);

        let value = memory.addr_absolute_idx(0x2000, 0x12);

        assert_eq!(value.0, 0x4711);
        assert_eq!(value.1, true);
    }

    #[test]
    fn test_addr_idx_indirect() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x2001, 0x41);
        memory.write_bus(0x42, 0x11);
        memory.write_bus(0x43, 0x47);

        let value = memory.addr_idx_indirect(0x2000, 0x1);

        assert_eq!(value, 0x4711);
    }

    #[test]
    fn test_addr_idx_indirect_zp() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x2001, 0xff);
        memory.write_bus(0xff, 0x0);
        memory.write_bus(0x00, 0x4);

        let value = memory.addr_idx_indirect(0x2000, 0x0);

        assert_eq!(value, 0x400);
    }

    #[test]
    fn test_addr_idx_indirect_wrap() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x2001, 0x43);
        memory.write_bus(0x42, 0x11);
        memory.write_bus(0x43, 0x47);

        let value = memory.addr_idx_indirect(0x2000, 0xff);

        assert_eq!(value, 0x4711);
    }

    #[test]
    fn test_addr_indirect_idx() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x2001, 0x42);
        memory.write_bus(0x42, 0x10);
        memory.write_bus(0x43, 0x47);

        let value = memory.addr_indirect_idx(0x2000, 0x1);

        assert_eq!(value.0, 0x4711);
        assert_eq!(value.1, false);
    }

    #[test]
    fn test_addr_indirect_idx_wrap_with_carry() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x2001, 0x42);
        memory.write_bus(0x42, 0x12);
        memory.write_bus(0x43, 0x46);

        let value = memory.addr_indirect_idx(0x2000, 0xff);

        assert_eq!(value.0, 0x4711);
        assert_eq!(value.1, true);
    }

    #[test]
    fn test_addr_indirect_idx_overflow() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x2001, 0xff);
        memory.write_bus(0xff, 0x10);
        memory.write_bus(0x00, 0x47);

        let value = memory.addr_indirect_idx(0x2000, 0x1);

        assert_eq!(value.0, 0x4711);
        assert_eq!(value.1, false);
    }

    #[test]
    fn test_addr_indirect_idx_overflow_ff() {
        // D959  B1 FF     LDA ($FF),Y = 0146 @ 0245 = 12  A:01 X:65 Y:FF P:E5 SP:FA PPU: 77,215 CYC:8824
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x2001, 0xff);
        memory.write_bus(0xff, 0x46);
        memory.write_bus(0x00, 0x01);

        let value = memory.addr_indirect_idx(0x2000, 0xff);

        assert_eq!(value.0, 0x0245);
        assert_eq!(value.1, true);
    }

    #[test]
    fn test_addr_zeropage() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x2001, 0x11);

        let value = memory.addr_zeropage(0x2000);

        assert_eq!(value, 0x11);
    }

    #[test]
    fn test_addr_zeropage_idx() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x2001, 0x10);

        let value = memory.addr_zeropage_idx(0x2000, 0x1);

        assert_eq!(value, 0x11);
    }

    #[test]
    fn test_addr_zeropage_idx_wrap() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x2001, 0x43);

        let value = memory.addr_zeropage_idx(0x2000, 0xff);

        assert_eq!(value, 0x42);
    }

    #[test]
    fn test_get_16b_addr() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x2001, 0x11);
        memory.write_bus(0x2002, 0x47);

        let value = memory.get_16b_addr(0x2001);

        assert_eq!(value, 0x4711);
    }

    #[test]
    fn test_value_at_addr() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x2001, 0x11);

        let value = memory.read_bus(0x2001);

        assert_eq!(value, 0x11);
    }

    #[test]
    fn test_push_to_stack() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.push_to_stack(0xff, 0x42);

        assert_eq!(memory.read_bus(0x1ff), 0x42);
    }

    #[test]
    fn test_pull_from_stack() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x1ff, 0x42);
        let value: u8 = memory.pull_from_stack(0xff);

        assert_eq!(value, 0x42);
    }

    #[test]
    fn test_stack_addr() {
        let memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));

        assert_eq!(memory.stack_addr(0xfd), 0x1fd);
    }

    #[test]
    fn test_store() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.write_bus(0x200, 0xff);

        assert_eq!(memory.read_bus(0x200), 0xff);
    }

    #[test]
    fn test_to_16b_addr() {
        assert_eq!(to_16b_addr(0x47, 0x11), 0x4711);
    }
}
