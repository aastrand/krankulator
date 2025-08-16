use super::super::*;
use super::*;

use std::cell::RefCell;
use std::rc::Rc;

/*
CNROM (Mapper 3)

CPU $6000-$7FFF: Optional 2 KiB PRG-RAM (rare)
CPU $8000-$FFFF: 32 KB unbanked PRG-ROM

PPU $0000-$1FFF: 8 KB switchable CHR-ROM window

Bank switching:
- CHR bank selection via writes to $8000-$FFFF
- Register: "D[..DC ..BA]" controls CHR bank
- Supports up to 32 KiB CHR-ROM (4 banks of 8 KB)
- Subject to AND-type bus conflicts

Mirroring:
- Fixed nametable arrangement controlled by solder pads
*/

const CNROM_PRG_SIZE: usize = 32 * 1024;
const CNROM_CHR_BANK_SIZE: usize = 8 * 1024;
const VRAM_SIZE: u16 = 2 * 1024;

const PRG_ROM_ADDR: usize = 0x8000;

pub struct CNROMMapper {
    _flags: u8,

    ppu: Rc<RefCell<ppu::PPU>>,
    apu: Rc<RefCell<apu::APU>>,

    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    // CHR ROM banks
    _chr_banks: Vec<[u8; CNROM_CHR_BANK_SIZE]>,
    selected_chr_bank: usize,
    
    // Current CHR data pointer
    _current_chr_bank: Box<[u8; CNROM_CHR_BANK_SIZE]>,
    chr_ptr: *mut u8,

    _vram: Box<[u8; VRAM_SIZE as usize]>,
    vrm_ptr: *mut u8,

    nametable_alignment: NametableMirror,

    pub controllers: [controller::Controller; 2],
    palette_ram: [u8; 32],
}

impl CNROMMapper {
    pub fn new(
        flags: u8,
        prg_rom: [u8; CNROM_PRG_SIZE],
        chr_banks: Vec<[u8; CNROM_CHR_BANK_SIZE]>,
    ) -> CNROMMapper {
        if chr_banks.is_empty() {
            panic!("CNROM requires at least one CHR bank");
        }

        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);
        
        // Load 32KB PRG ROM at $8000-$FFFF
        mem[PRG_ROM_ADDR..PRG_ROM_ADDR + CNROM_PRG_SIZE].clone_from_slice(&prg_rom);

        let addr_space_ptr = mem.as_mut_ptr();

        // Start with the first CHR bank
        let mut current_chr_bank = Box::new(chr_banks[0]);
        let chr_ptr = current_chr_bank.as_mut_ptr();

        let mut vram = Box::new([0; VRAM_SIZE as usize]);
        let vrm_ptr = vram.as_mut_ptr();

        let nametable_alignment = if flags & super::NAMETABLE_ALIGNMENT_BIT == 1 {
            NametableMirror::Horizontal
        } else {
            NametableMirror::Vertical
        };

        CNROMMapper {
            _flags: flags,

            ppu: Rc::new(RefCell::new(ppu::PPU::new())),
            apu: Rc::new(RefCell::new(apu::APU::new())),

            _addr_space: mem,
            addr_space_ptr,

            _chr_banks: chr_banks,
            selected_chr_bank: 0,

            _current_chr_bank: current_chr_bank,
            chr_ptr,

            _vram: vram,
            vrm_ptr,

            nametable_alignment,

            controllers: [controller::Controller::new(), controller::Controller::new()],
            palette_ram: [0x0F; 32],
        }
    }

    fn switch_chr_bank(&mut self, bank: u8) {
        let bank_index = (bank as usize) % self._chr_banks.len();
        if bank_index != self.selected_chr_bank {
            self.selected_chr_bank = bank_index;
            
            // Update the current CHR bank
            self._current_chr_bank = Box::new(self._chr_banks[bank_index]);
            self.chr_ptr = self._current_chr_bank.as_mut_ptr();
        }
    }
}

impl MemoryMapper for CNROMMapper {
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
            // CHR bank switching register at $8000-$FFFF
            0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                // Use bits 1-0 for CHR bank selection (supports up to 4 banks)
                self.switch_chr_bank(value & 0x03);
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
            // CHR ROM (read-only)
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
            // CHR ROM is read-only, ignore writes
            0x0 | 0x10 => { /* CHR ROM is read-only */ }
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
    fn test_cnrom_chr_bank_switching() {
        let prg_rom = [0; CNROM_PRG_SIZE];
        let chr_bank1 = [1; CNROM_CHR_BANK_SIZE];
        let chr_bank2 = [2; CNROM_CHR_BANK_SIZE];
        let chr_bank3 = [3; CNROM_CHR_BANK_SIZE];
        let chr_bank4 = [4; CNROM_CHR_BANK_SIZE];
        
        let mut mapper: Box<dyn MemoryMapper> = Box::new(CNROMMapper::new(
            0,
            prg_rom,
            vec![chr_bank1, chr_bank2, chr_bank3, chr_bank4],
        ));

        // Initially CHR bank 0 should be selected
        assert_eq!(mapper.ppu_read(0x0000), 1);
        
        // Switch to CHR bank 1
        mapper.cpu_write(0x8000, 1);
        assert_eq!(mapper.ppu_read(0x0000), 2);

        // Switch to CHR bank 2
        mapper.cpu_write(0x8000, 2);
        assert_eq!(mapper.ppu_read(0x0000), 3);

        // Switch to CHR bank 3
        mapper.cpu_write(0x8000, 3);
        assert_eq!(mapper.ppu_read(0x0000), 4);

        // Test wrapping - bank 4 should wrap to bank 0
        mapper.cpu_write(0x8000, 4);
        assert_eq!(mapper.ppu_read(0x0000), 1);
    }

    #[test]
    fn test_cnrom_prg_rom_fixed() {
        let mut prg_rom = [0; CNROM_PRG_SIZE];
        prg_rom[0] = 0x42;  // At $8000
        prg_rom[CNROM_PRG_SIZE - 1] = 0x84;  // At $FFFF
        
        let chr_bank = [0; CNROM_CHR_BANK_SIZE];
        
        let mut mapper: Box<dyn MemoryMapper> = Box::new(CNROMMapper::new(
            0,
            prg_rom,
            vec![chr_bank],
        ));

        // PRG ROM should be fixed at $8000-$FFFF
        assert_eq!(mapper.cpu_read(0x8000), 0x42);
        assert_eq!(mapper.cpu_read(0xFFFF), 0x84);

        // CHR bank switching should not affect PRG ROM
        mapper.cpu_write(0x8000, 1);
        assert_eq!(mapper.cpu_read(0x8000), 0x42);
        assert_eq!(mapper.cpu_read(0xFFFF), 0x84);
    }

    #[test]
    fn test_cnrom_chr_rom_read_only() {
        let prg_rom = [0; CNROM_PRG_SIZE];
        let chr_bank = [0x55; CNROM_CHR_BANK_SIZE];
        
        let mut mapper: Box<dyn MemoryMapper> = Box::new(CNROMMapper::new(
            0,
            prg_rom,
            vec![chr_bank],
        ));

        // CHR ROM should be readable
        assert_eq!(mapper.ppu_read(0x0100), 0x55);
        
        // CHR ROM should be read-only (writes ignored)
        mapper.ppu_write(0x0100, 0xAA);
        assert_eq!(mapper.ppu_read(0x0100), 0x55);
    }

    #[test]
    fn test_cnrom_mirroring() {
        let prg_rom = [0; CNROM_PRG_SIZE];
        let chr_bank = [0; CNROM_CHR_BANK_SIZE];
        
        // Test vertical mirroring (flags = 0)
        let mut mapper_v: Box<dyn MemoryMapper> = Box::new(CNROMMapper::new(
            0,
            prg_rom,
            vec![chr_bank],
        ));
        
        // Test horizontal mirroring (flags = 1)
        let mut mapper_h: Box<dyn MemoryMapper> = Box::new(CNROMMapper::new(
            1,
            prg_rom,
            vec![chr_bank],
        ));

        // Write to nametable and verify mirroring behavior
        mapper_v.ppu_write(0x2000, 0x10);
        mapper_h.ppu_write(0x2000, 0x20);
        
        // The specific mirroring behavior is handled by the mirror_nametable_addr function
        assert_eq!(mapper_v.ppu_read(0x2000), 0x10);
        assert_eq!(mapper_h.ppu_read(0x2000), 0x20);
    }
}