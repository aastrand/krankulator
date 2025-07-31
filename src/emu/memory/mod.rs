pub mod mapper;
use super::apu;
use super::io::controller;
use super::ppu;

use std::cell::RefCell;
use std::rc::Rc;

pub const NMI_TARGET_ADDR: u16 = 0xfffa;
#[allow(dead_code)] // only used in tests
pub const RESET_TARGET_ADDR: u16 = 0xfffc;
pub const BRK_TARGET_ADDR: u16 = 0xfffe;
pub const CODE_START_ADDR: u16 = 0x600;
pub const STACK_START_ADDR: u16 = 0xff;

pub const MAX_RAM_SIZE: usize = 65536;
pub const MAX_VRAM_SIZE: usize = 0x4000;
pub const STACK_BASE_OFFSET: u16 = 0x100;

pub fn to_16b_addr(hb: u8, lb: u8) -> u16 {
    ((hb as u16) << 8) + (lb as u16)
}

pub fn addr_to_page(addr: u16) -> u16 {
    (addr >> 8) & 0xf0
}

pub trait MemoryMapper {
    fn cpu_read(&mut self, addr: u16) -> u8;
    fn cpu_write(&mut self, addr: u16, value: u8);
    fn ppu_read(&self, addr: u16) -> u8;
    #[allow(dead_code)]
    fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize);
    fn ppu_write(&mut self, addr: u16, value: u8);

    fn code_start(&mut self) -> u16;
    fn ppu(&self) -> Rc<RefCell<ppu::PPU>>;
    fn apu(&self) -> Rc<RefCell<apu::APU>>;
    fn controllers(&mut self) -> &mut [controller::Controller; 2];
    fn poll_irq(&mut self) -> bool;

    // Called on PPU cycle 260 of visible and pre-render scanlines for MMC3 IRQ counter
    fn ppu_cycle_260(&mut self, _scanline: u16) {
        // Default implementation does nothing
    }

    fn addr_absolute(&mut self, pc: u16) -> u16 {
        self.get_16b_addr(pc.wrapping_add(1) as _)
    }

    fn get_16b_addr(&mut self, offset: u16) -> u16 {
        to_16b_addr(self.cpu_read(offset.wrapping_add(1)), self.cpu_read(offset))
    }

    fn addr_absolute_idx(&mut self, pc: u16, idx: u8) -> (u16, bool) {
        let lb = self.cpu_read(pc.wrapping_add(1));
        (
            to_16b_addr(self.cpu_read(pc.wrapping_add(2)), lb).wrapping_add(idx as u16),
            // Did we cross the page boundary?
            (lb & 0xff) as u16 + idx as u16 > 0xff,
        )
    }

    fn addr_idx_indirect(&mut self, pc: u16, idx: u8) -> u16 {
        let value: u8 = self.cpu_read((pc + 1) as _).wrapping_add(idx);
        ((self.cpu_read(((value as u8).wrapping_add(1)) as u16) as u16) << 8)
            + self.cpu_read(value as u16) as u16
    }

    fn addr_indirect_idx(&mut self, pc: u16, idx: u8) -> (u16, bool) {
        let base = self.cpu_read(pc + 1);

        let lb = self.cpu_read(base as _);
        let lbidx = lb.wrapping_add(idx);
        let carry: u8 = if lbidx <= lb && idx > 0 { 1 } else { 0 };
        let hb = self
            .cpu_read((base as u8).wrapping_add(1) as _)
            .wrapping_add(carry);

        (to_16b_addr(hb, lbidx) as _, carry != 0)
    }

    fn addr_zeropage(&mut self, pc: u16) -> u16 {
        self.cpu_read(pc + 1) as _
    }

    fn addr_zeropage_idx(&mut self, pc: u16, idx: u8) -> u16 {
        self.cpu_read(pc + 1).wrapping_add(idx) as u16
    }

    fn stack_addr(&self, sp: u8) -> u16 {
        STACK_BASE_OFFSET + (u16::from(sp) & 0xff)
    }

    fn push_to_stack(&mut self, sp: u8, value: u8) {
        self.cpu_write(self.stack_addr(sp), value);
    }

    fn pull_from_stack(&mut self, sp: u8) -> u8 {
        self.cpu_read(self.stack_addr(sp))
    }

    fn raw_opcode(&mut self, addr: u16) -> [u8; 3] {
        [
            self.cpu_read(addr),
            self.cpu_read(addr.wrapping_add(1)),
            self.cpu_read(addr.wrapping_add(2)),
        ]
    }
}

pub struct IdentityMapper {
    _ram: Box<[u8; MAX_RAM_SIZE]>,
    ram_ptr: *mut u8,
    _vram: Box<[u8; MAX_VRAM_SIZE]>,
    vram_ptr: *mut u8,
    code_start: u16,
    controllers: [controller::Controller; 2],
}

impl IdentityMapper {
    pub fn new(code_start: u16) -> IdentityMapper {
        let mut ram = Box::new([0; MAX_RAM_SIZE]);
        let ram_ptr = ram.as_mut_ptr();

        let mut vram = Box::new([0; MAX_VRAM_SIZE]);
        let vram_ptr = vram.as_mut_ptr();
        IdentityMapper {
            _ram: ram,
            ram_ptr,
            _vram: vram,
            vram_ptr: vram_ptr,
            code_start: code_start,
            controllers: [controller::Controller::new(), controller::Controller::new()],
        }
    }
}

impl MemoryMapper for IdentityMapper {
    #[inline]
    fn cpu_read(&mut self, addr: u16) -> u8 {
        unsafe { *self.ram_ptr.offset(addr as _) }
    }

    #[inline]
    fn cpu_write(&mut self, addr: u16, value: u8) {
        unsafe { *self.ram_ptr.offset(addr as _) = value }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        unsafe { *self.vram_ptr.offset(addr as _) }
    }

    fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        unsafe { std::ptr::copy(self.vram_ptr.offset(addr as _), dest, size) }
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        unsafe { *self.vram_ptr.offset(addr as _) = value }
    }

    fn code_start(&mut self) -> u16 {
        self.code_start
    }

    fn ppu(&self) -> Rc<RefCell<ppu::PPU>> {
        Rc::new(RefCell::new(ppu::PPU::new()))
    }

    fn apu(&self) -> Rc<RefCell<apu::APU>> {
        Rc::new(RefCell::new(apu::APU::new()))
    }

    fn controllers(&mut self) -> &mut [controller::Controller; 2] {
        &mut self.controllers
    }

    fn poll_irq(&mut self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_addr_absolute() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.cpu_write(0x2001, 0x11);
        memory.cpu_write(0x2002, 0x47);

        let value = memory.addr_absolute(0x2000);

        assert_eq!(value, 0x4711);
    }

    #[test]
    fn test_addr_absolute_idx() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.cpu_write(0x2001, 0x10);
        memory.cpu_write(0x2002, 0x47);

        let value = memory.addr_absolute_idx(0x2000, 1);

        assert_eq!(value.0, 0x4711);
        assert_eq!(value.1, false);
    }

    #[test]
    fn test_addr_absolute_idx_crossed_boundary() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.cpu_write(0x2001, 0xff);
        memory.cpu_write(0x2002, 0x46);

        let value = memory.addr_absolute_idx(0x2000, 0x12);

        assert_eq!(value.0, 0x4711);
        assert_eq!(value.1, true);
    }

    #[test]
    fn test_addr_idx_indirect() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.cpu_write(0x2001, 0x41);
        memory.cpu_write(0x42, 0x11);
        memory.cpu_write(0x43, 0x47);

        let value = memory.addr_idx_indirect(0x2000, 0x1);

        assert_eq!(value, 0x4711);
    }

    #[test]
    fn test_addr_idx_indirect_zp() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.cpu_write(0x2001, 0xff);
        memory.cpu_write(0xff, 0x0);
        memory.cpu_write(0x00, 0x4);

        let value = memory.addr_idx_indirect(0x2000, 0x0);

        assert_eq!(value, 0x400);
    }

    #[test]
    fn test_addr_idx_indirect_wrap() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.cpu_write(0x2001, 0x43);
        memory.cpu_write(0x42, 0x11);
        memory.cpu_write(0x43, 0x47);

        let value = memory.addr_idx_indirect(0x2000, 0xff);

        assert_eq!(value, 0x4711);
    }

    #[test]
    fn test_addr_indirect_idx() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.cpu_write(0x2001, 0x42);
        memory.cpu_write(0x42, 0x10);
        memory.cpu_write(0x43, 0x47);

        let value = memory.addr_indirect_idx(0x2000, 0x1);

        assert_eq!(value.0, 0x4711);
        assert_eq!(value.1, false);
    }

    #[test]
    fn test_addr_indirect_idx_wrap_with_carry() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.cpu_write(0x2001, 0x42);
        memory.cpu_write(0x42, 0x12);
        memory.cpu_write(0x43, 0x46);

        let value = memory.addr_indirect_idx(0x2000, 0xff);

        assert_eq!(value.0, 0x4711);
        assert_eq!(value.1, true);
    }

    #[test]
    fn test_addr_indirect_idx_overflow() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.cpu_write(0x2001, 0xff);
        memory.cpu_write(0xff, 0x10);
        memory.cpu_write(0x00, 0x47);

        let value = memory.addr_indirect_idx(0x2000, 0x1);

        assert_eq!(value.0, 0x4711);
        assert_eq!(value.1, false);
    }

    #[test]
    fn test_addr_indirect_idx_overflow_ff() {
        // D959  B1 FF     LDA ($FF),Y = 0146 @ 0245 = 12  A:01 X:65 Y:FF P:E5 SP:FA PPU: 77,215 CYC:8824
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.cpu_write(0x2001, 0xff);
        memory.cpu_write(0xff, 0x46);
        memory.cpu_write(0x00, 0x01);

        let value = memory.addr_indirect_idx(0x2000, 0xff);

        assert_eq!(value.0, 0x0245);
        assert_eq!(value.1, true);
    }

    #[test]
    fn test_addr_zeropage() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.cpu_write(0x2001, 0x11);

        let value = memory.addr_zeropage(0x2000);

        assert_eq!(value, 0x11);
    }

    #[test]
    fn test_addr_zeropage_idx() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.cpu_write(0x2001, 0x10);

        let value = memory.addr_zeropage_idx(0x2000, 0x1);

        assert_eq!(value, 0x11);
    }

    #[test]
    fn test_addr_zeropage_idx_wrap() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.cpu_write(0x2001, 0x43);

        let value = memory.addr_zeropage_idx(0x2000, 0xff);

        assert_eq!(value, 0x42);
    }

    #[test]
    fn test_get_16b_addr() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.cpu_write(0x2001, 0x11);
        memory.cpu_write(0x2002, 0x47);

        let value = memory.get_16b_addr(0x2001);

        assert_eq!(value, 0x4711);
    }

    #[test]
    fn test_value_at_addr() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.cpu_write(0x2001, 0x11);

        let value = memory.cpu_read(0x2001);

        assert_eq!(value, 0x11);
    }

    #[test]
    fn test_push_to_stack() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.push_to_stack(0xff, 0x42);

        assert_eq!(memory.cpu_read(0x1ff), 0x42);
    }

    #[test]
    fn test_pull_from_stack() {
        let mut memory: Box<dyn MemoryMapper> = Box::new(IdentityMapper::new(0));
        memory.cpu_write(0x1ff, 0x42);
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
        memory.cpu_write(0x200, 0xff);

        assert_eq!(memory.cpu_read(0x200), 0xff);
    }

    #[test]
    fn test_to_16b_addr() {
        assert_eq!(to_16b_addr(0x47, 0x11), 0x4711);
    }
    #[test]
    fn test_addr_to_page() {
        assert_eq!(addr_to_page(0x80), 0x0);
        assert_eq!(addr_to_page(0x8000), 0x80);
        assert_eq!(addr_to_page(0x1234), 0x10);
        assert_eq!(addr_to_page(0xffff), 0xf0);
    }
}
