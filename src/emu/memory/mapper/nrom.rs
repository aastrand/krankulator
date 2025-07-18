use super::super::*;
use super::*;

use std::cell::RefCell;
use std::rc::Rc;

/*
All Banks are fixed,

CPU $6000-$7FFF: Family Basic only: PRG RAM, mirrored as necessary to fill entire 8 KiB window, write protectable with an external switch
CPU $8000-$BFFF: First 16 KB of ROM.
CPU $C000-$FFFF: Last 16 KB of ROM (NROM-256) or mirror of $8000-$BFFF (NROM-128).
*/

const NROM_PRG_BANK_SIZE: usize = 16 * 1024;
const NROM_CHR_BANK_SIZE: usize = 8 * 1024;
const VRAM_SIZE: u16 = 2 * 1024;

const BANK_ONE_ADDR: usize = 0x8000;
const BANK_TWO_ADDR: usize = 0xC000;

pub struct NROMMapper {
    _flags: u8,

    ppu: Rc<RefCell<ppu::PPU>>,
    apu: Rc<RefCell<apu::APU>>,

    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    _chr_bank: Box<[u8; NROM_CHR_BANK_SIZE]>,
    chr_ptr: *mut u8,

    _vram: Box<[u8; VRAM_SIZE as usize]>,
    vrm_ptr: *mut u8,

    nametable_alignment: NametableMirror,

    pub controllers: [controller::Controller; 2],
    palette_ram: [u8; 32],
}

impl NROMMapper {
    // TODO: PRG RAM
    pub fn new(
        flags: u8,
        bank_one: Box<[u8; NROM_PRG_BANK_SIZE]>,
        bank_two: Option<[u8; NROM_PRG_BANK_SIZE]>,
        chr_rom: Option<[u8; NROM_CHR_BANK_SIZE]>,
    ) -> NROMMapper {
        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);

        mem[BANK_ONE_ADDR..BANK_ONE_ADDR + NROM_PRG_BANK_SIZE].clone_from_slice(&*bank_one);

        let second = if bank_two.is_some() {
            bank_two.unwrap()
        } else {
            *bank_one
        };
        mem[BANK_TWO_ADDR..BANK_TWO_ADDR + NROM_PRG_BANK_SIZE].clone_from_slice(&second);

        let addr_space_ptr = mem.as_mut_ptr();

        let mut chr_bank = Box::new(chr_rom.unwrap_or([0; NROM_CHR_BANK_SIZE]));
        let chr_ptr = chr_bank.as_mut_ptr();

        let mut vram = Box::new([0; VRAM_SIZE as usize]);
        let vrm_ptr = vram.as_mut_ptr();

        let nametable_alignment = if flags & super::NAMETABLE_ALIGNMENT_BIT == 1 {
            NametableMirror::Horizontal
        } else {
            NametableMirror::Vertical
        };

        NROMMapper {
            _flags: flags,

            ppu: Rc::new(RefCell::new(ppu::PPU::new())),
            apu: Rc::new(RefCell::new(apu::APU::new())),

            _addr_space: mem,
            addr_space_ptr: addr_space_ptr,

            _chr_bank: chr_bank,
            chr_ptr: chr_ptr,

            _vram: vram,
            vrm_ptr: vrm_ptr,

            nametable_alignment: nametable_alignment,

            controllers: [controller::Controller::new(), controller::Controller::new()],
            palette_ram: [0x0F; 32],
        }
    }
}

impl MemoryMapper for NROMMapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = mirror_addr(addr);
        let page = addr_to_page(addr);

        match page {
            0x20 => {
                if addr >= 0x2000 && addr < 0x2008 {
                    self.ppu.borrow_mut().read(addr, self as _)
                } else {
                    unsafe { *self.addr_space_ptr.offset(addr as _) }
                }
            }
            0x40 => match addr {
                0x4014 => self.ppu.borrow_mut().read(addr, self as _),
                0x4015 => self.apu.borrow_mut().read(addr),
                0x4016 => self.controllers[0].poll(),
                0x4017 => self.controllers[1].poll(),
                _ => unsafe { *self.addr_space_ptr.offset(addr as _) },
            },
            _ => unsafe { *self.addr_space_ptr.offset(addr as _) },
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);

        match page {
            // Note: 0x60 is for PRG ram
            0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as _) = value },
            0x20 => {
                //println!("Write to PPU reg {:X}: {:X}", addr, value);
                let should_write = self
                    .ppu
                    .borrow_mut()
                    .write(addr, value, self.addr_space_ptr);
                if let Some((addr, value)) = should_write {
                    self.ppu_write(addr, value);
                }
            }
            0x40 => {
                if addr == 0x4014 {
                    self.ppu
                        .borrow_mut()
                        .write(addr, value, self.addr_space_ptr);
                } else if addr >= 0x4000 && addr <= 0x4017 {
                    self.apu.borrow_mut().write(addr, value);
                }
            }
            _ => { /*panic!("Write at addr {:X} not mapped", addr),*/ }
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        let mut addr = addr;
        let page = addr_to_page(addr);
        if addr >= 0x3F00 && addr < 0x4000 {
            let mut palette_addr = (addr as usize - 0x3F00) % 32;
            if palette_addr & 0x13 == 0x10 {
                palette_addr &= !0x10;
            }
            return self.palette_ram[palette_addr];
        }
        match page {
            0x0 | 0x10 => unsafe { *self.chr_ptr.offset(addr as _) },
            0x20 => {
                addr = super::mirror_nametable_addr(addr, self.nametable_alignment) % VRAM_SIZE;
                unsafe { *self.vrm_ptr.offset(addr as _) }
            }
            0x30 => unsafe { *self.vrm_ptr.offset((addr % VRAM_SIZE) as _) },
            _ => panic!("Addr {:X} not mapped for ppu_read!", addr),
        }
    }

    fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        /*
        $0000-1FFF is normally mapped by the cartridge to a CHR-ROM or CHR-RAM, often with a bank switching mechanism.
        $2000-2FFF is normally mapped to the 2kB NES internal VRAM, providing 2 nametables with a mirroring configuration controlled by the cartridge, but it can be partly or fully remapped to RAM on the cartridge, allowing up to 4 simultaneous nametables.
        $3000-3EFF is usually a mirror of the 2kB region from $2000-2EFF. The PPU does not render from this address range, so this space has negligible utility.
        $3F00-3FFF is not configurable, always mapped to the internal palette control.
        */
        let mut addr = addr % MAX_VRAM_ADDR;
        let page = addr_to_page(addr);
        match page {
            0x0 | 0x10 => unsafe { std::ptr::copy(self.chr_ptr.offset(addr as _), dest, size) },
            0x20 => {
                addr = super::mirror_nametable_addr(addr, self.nametable_alignment) % VRAM_SIZE;
                unsafe { std::ptr::copy(self.vrm_ptr.offset(addr as _), dest, size) }
            }
            0x30 => unsafe {
                std::ptr::copy(self.vrm_ptr.offset((addr % VRAM_SIZE) as _), dest, size)
            },

            _ => panic!("Addr not mapped for ppu_read: {:X}", addr),
        }
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        let mut addr = addr % MAX_VRAM_ADDR;
        if addr >= 0x3F00 && addr < 0x4000 {
            let mut palette_addr = (addr as usize - 0x3F00) % 32;
            if palette_addr & 0x13 == 0x10 {
                palette_addr &= !0x10;
            }
            self.palette_ram[palette_addr] = value;
            return;
        }
        let page = addr_to_page(addr);
        match page {
            0x20 => {
                addr = super::mirror_nametable_addr(addr, self.nametable_alignment) % VRAM_SIZE;
                unsafe { *self.vrm_ptr.offset(addr as _) = value }
            }
            0x30 => unsafe { *self.vrm_ptr.offset((addr % VRAM_SIZE) as _) = value },

            _ => panic!("Addr not mapped for ppu_write: {:X}", addr),
        }
    }

    fn code_start(&mut self) -> u16 {
        ((self.cpu_read(super::RESET_TARGET_ADDR + 1) as u16) << 8) as u16
            + self.cpu_read(super::RESET_TARGET_ADDR) as u16
    }

    fn ppu(&self) -> Rc<RefCell<ppu::PPU>> {
        Rc::clone(&self.ppu)
    }

    fn apu(&self) -> Rc<RefCell<apu::APU>> {
        Rc::clone(&self.apu)
    }

    fn controllers(&mut self) -> &mut [controller::Controller; 2] {
        &mut self.controllers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nrom_ram_mirroring() {
        let mut mapper: Box<dyn MemoryMapper> = Box::new(NROMMapper::new(
            0,
            Box::new([0; 16384]),
            None,
            Some([0; 8192]),
        ));
        mapper.cpu_write(0x173, 0x42);

        assert_eq!(mapper.cpu_read(0x173), 0x42);
        assert_eq!(mapper.cpu_read(0x973), 0x42);
        assert_eq!(mapper.cpu_read(0x1173), 0x42);
        assert_eq!(mapper.cpu_read(0x1973), 0x42);

        mapper.cpu_write(0x2001, 0x11);
        assert_eq!(mapper.ppu().borrow().ppu_mask, 0x11);
        mapper.cpu_write(0x2009, 0x42);
        assert_eq!(mapper.ppu().borrow().ppu_mask, 0x42);

        // a write to $3451 is the same as a write to $2001.

        mapper.cpu_write(0x3451, 0x32);
        assert_eq!(mapper.ppu().borrow().ppu_mask, 0x32);
    }
}
