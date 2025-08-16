use super::super::*;
use super::*;

use std::cell::RefCell;
use std::rc::Rc;

/*
UxROM (Mapper 2)

CPU $8000-$BFFF: 16 KB switchable PRG ROM bank
CPU $C000-$FFFF: 16 KB PRG ROM bank, fixed to the last bank

Bank switching:
- Write to $8000-$FFFF selects 16 KB PRG ROM bank for $8000-$BFFF
- Bank number is in bits 3-0 for UOROM (up to 4MB), bits 2-0 for UNROM (up to 256KB)

CHR:
- No CHR ROM banking (8 KB CHR-RAM is typical)

Mirroring:
- Fixed horizontal or vertical mirroring set by hardwired solder pads
*/

const UXROM_PRG_BANK_SIZE: usize = 16 * 1024;
const UXROM_CHR_SIZE: usize = 8 * 1024;
const VRAM_SIZE: u16 = 2 * 1024;

const BANK_SWITCHABLE_ADDR: usize = 0x8000;
const BANK_FIXED_ADDR: usize = 0xC000;

pub struct UxROMMapper {
    _flags: u8,

    ppu: Rc<RefCell<ppu::PPU>>,
    apu: Rc<RefCell<apu::APU>>,

    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    // PRG ROM banks
    _prg_rom: Vec<[u8; UXROM_PRG_BANK_SIZE]>,
    selected_bank: usize,

    // CHR RAM (no CHR ROM banking)
    _chr_ram: Box<[u8; UXROM_CHR_SIZE]>,
    chr_ptr: *mut u8,

    _vram: Box<[u8; VRAM_SIZE as usize]>,
    vrm_ptr: *mut u8,

    nametable_alignment: NametableMirror,

    pub controllers: [controller::Controller; 2],
    palette_ram: [u8; 32],
}

impl UxROMMapper {
    pub fn new(flags: u8, prg_banks: Vec<[u8; UXROM_PRG_BANK_SIZE]>) -> UxROMMapper {
        if prg_banks.is_empty() {
            panic!("UxROM requires at least one PRG bank");
        }

        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);

        // Initialize with first bank switchable, last bank fixed
        let last_bank = prg_banks.len() - 1;
        mem[BANK_SWITCHABLE_ADDR..BANK_SWITCHABLE_ADDR + UXROM_PRG_BANK_SIZE]
            .clone_from_slice(&prg_banks[0]);
        mem[BANK_FIXED_ADDR..BANK_FIXED_ADDR + UXROM_PRG_BANK_SIZE]
            .clone_from_slice(&prg_banks[last_bank]);

        let addr_space_ptr = mem.as_mut_ptr();

        // CHR RAM (writable)
        let mut chr_ram = Box::new([0; UXROM_CHR_SIZE]);
        let chr_ptr = chr_ram.as_mut_ptr();

        let mut vram = Box::new([0; VRAM_SIZE as usize]);
        let vrm_ptr = vram.as_mut_ptr();

        let nametable_alignment = if flags & super::NAMETABLE_ALIGNMENT_BIT == 1 {
            NametableMirror::Horizontal
        } else {
            NametableMirror::Vertical
        };

        UxROMMapper {
            _flags: flags,

            ppu: Rc::new(RefCell::new(ppu::PPU::new())),
            apu: Rc::new(RefCell::new(apu::APU::new())),

            _addr_space: mem,
            addr_space_ptr,

            _prg_rom: prg_banks,
            selected_bank: 0,

            _chr_ram: chr_ram,
            chr_ptr,

            _vram: vram,
            vrm_ptr,

            nametable_alignment,

            controllers: [controller::Controller::new(), controller::Controller::new()],
            palette_ram: [0x0F; 32],
        }
    }

    fn switch_prg_bank(&mut self, bank: u8) {
        let bank_index = (bank as usize) % self._prg_rom.len();
        self.selected_bank = bank_index;

        // Update the switchable bank in memory
        unsafe {
            std::ptr::copy_nonoverlapping(
                self._prg_rom[bank_index].as_ptr(),
                self.addr_space_ptr.offset(BANK_SWITCHABLE_ADDR as isize),
                UXROM_PRG_BANK_SIZE,
            );
        }
    }
}

impl MemoryMapper for UxROMMapper {
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
            0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as _) = value },
            0x20 => {
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
            // Bank switching register at $8000-$FFFF
            0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                // Use bits 3-0 for bank selection (supports up to 16 banks)
                // Some variants use only bits 2-0 (UNROM) for 8 banks
                self.switch_prg_bank(value & 0x0F);
            }
            _ => { /* Ignore writes to unmapped areas */ }
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
            // CHR RAM (writable)
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
            _ => panic!("Addr not mapped for ppu_copy: {:X}", addr),
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
            // CHR RAM is writable
            0x0 | 0x10 => unsafe { *self.chr_ptr.offset(addr as _) = value },
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

    fn poll_irq(&mut self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uxrom_bank_switching() {
        let bank1 = [1; UXROM_PRG_BANK_SIZE];
        let bank2 = [2; UXROM_PRG_BANK_SIZE];
        let bank3 = [3; UXROM_PRG_BANK_SIZE];

        let mut mapper: Box<dyn MemoryMapper> =
            Box::new(UxROMMapper::new(0, vec![bank1, bank2, bank3]));

        // Initially bank 0 should be selected for $8000-$BFFF
        assert_eq!(mapper.cpu_read(0x8000), 1);

        // Last bank (bank 2) should be fixed at $C000-$FFFF
        assert_eq!(mapper.cpu_read(0xC000), 3);

        // Switch to bank 1
        mapper.cpu_write(0x8000, 1);
        assert_eq!(mapper.cpu_read(0x8000), 2);

        // Fixed bank should remain unchanged
        assert_eq!(mapper.cpu_read(0xC000), 3);

        // Switch to bank 2
        mapper.cpu_write(0x8000, 2);
        assert_eq!(mapper.cpu_read(0x8000), 3);

        // Fixed bank should still be unchanged
        assert_eq!(mapper.cpu_read(0xC000), 3);
    }

    #[test]
    fn test_uxrom_chr_ram_write() {
        let bank1 = [0; UXROM_PRG_BANK_SIZE];
        let mut mapper: Box<dyn MemoryMapper> = Box::new(UxROMMapper::new(0, vec![bank1]));

        // CHR RAM should be writable
        mapper.ppu_write(0x0100, 0x42);
        assert_eq!(mapper.ppu_read(0x0100), 0x42);

        // Test another address
        mapper.ppu_write(0x1FFF, 0x84);
        assert_eq!(mapper.ppu_read(0x1FFF), 0x84);
    }

    #[test]
    fn test_uxrom_mirroring() {
        let bank1 = [0; UXROM_PRG_BANK_SIZE];

        // Test vertical mirroring (flags = 0)
        let mut mapper_v: Box<dyn MemoryMapper> = Box::new(UxROMMapper::new(0, vec![bank1]));

        // Test horizontal mirroring (flags = 1)
        let mut mapper_h: Box<dyn MemoryMapper> = Box::new(UxROMMapper::new(1, vec![bank1]));

        // Write to nametable and verify mirroring behavior
        mapper_v.ppu_write(0x2000, 0x10);
        mapper_h.ppu_write(0x2000, 0x20);

        // The specific mirroring behavior is handled by the mirror_nametable_addr function
        // This test just ensures the mappers can be created with different mirroring flags
        assert_eq!(mapper_v.ppu_read(0x2000), 0x10);
        assert_eq!(mapper_h.ppu_read(0x2000), 0x20);
    }
}
