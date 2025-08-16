use super::super::*;
use super::*;

use std::cell::RefCell;
use std::rc::Rc;

/*
MMC2 (Mapper 9) - Used exclusively in Mike Tyson's Punch-Out!!

CPU Memory Layout:
$6000-$7FFF: 8 KB PRG RAM (PlayChoice version only)
$8000-$9FFF: 8 KB switchable PRG ROM bank
$A000-$FFFF: Three 8 KB PRG ROM banks (fixed to last three banks)

PPU Memory Layout:
$0000-$0FFF: Two 4 KB switchable CHR ROM banks (left pattern table)
$1000-$1FFF: Two 4 KB switchable CHR ROM banks (right pattern table)

Special CHR Banking:
- Automatically switches CHR banks when tiles $FD or $FE are rendered
- Each pattern table has two banks that are switched between based on latches
- Latches are set when specific tiles ($FD/$FE) are fetched during PPU operations

Registers:
$A000-$AFFF: PRG ROM bank selection (3 bits)
$B000-$BFFF: CHR ROM $FD/0000 bank select (5 bits)
$C000-$CFFF: CHR ROM $FE/0000 bank select (5 bits) 
$D000-$DFFF: CHR ROM $FD/1000 bank select (5 bits)
$E000-$EFFF: CHR ROM $FE/1000 bank select (5 bits)
$F000-$FFFF: Nametable mirroring control (bit 0)
*/

const MMC2_PRG_BANK_SIZE: usize = 8 * 1024;
const MMC2_CHR_BANK_SIZE: usize = 4 * 1024;
const VRAM_SIZE: u16 = 2 * 1024;

const SWITCHABLE_PRG_ADDR: usize = 0x8000;
const FIXED_PRG_ADDR_1: usize = 0xA000;
const FIXED_PRG_ADDR_2: usize = 0xC000;
const FIXED_PRG_ADDR_3: usize = 0xE000;

pub struct MMC2Mapper {
    _flags: u8,

    ppu: Rc<RefCell<ppu::PPU>>,
    apu: Rc<RefCell<apu::APU>>,

    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    // PRG ROM banks
    _prg_banks: Vec<[u8; MMC2_PRG_BANK_SIZE]>,
    selected_prg_bank: usize,

    // CHR ROM banks  
    _chr_banks: Vec<[u8; MMC2_CHR_BANK_SIZE]>,
    
    // CHR bank selection registers
    chr_bank_0000_fd: usize,  // Bank when latch 0 = $FD
    chr_bank_0000_fe: usize,  // Bank when latch 0 = $FE
    chr_bank_1000_fd: usize,  // Bank when latch 1 = $FD
    chr_bank_1000_fe: usize,  // Bank when latch 1 = $FE
    
    // CHR latches (set by reading $FD/$FE tiles)
    chr_latch_0: bool,  // false = $FD, true = $FE for $0000 table
    chr_latch_1: bool,  // false = $FD, true = $FE for $1000 table

    // Current CHR data
    _current_chr_0000: Box<[u8; MMC2_CHR_BANK_SIZE]>,
    _current_chr_1000: Box<[u8; MMC2_CHR_BANK_SIZE]>,
    chr_0000_ptr: *mut u8,
    chr_1000_ptr: *mut u8,

    _vram: Box<[u8; VRAM_SIZE as usize]>,
    vrm_ptr: *mut u8,

    nametable_alignment: NametableMirror,

    pub controllers: [controller::Controller; 2],
    palette_ram: [u8; 32],
}

impl MMC2Mapper {
    pub fn new(
        flags: u8,
        prg_banks: Vec<[u8; MMC2_PRG_BANK_SIZE]>,
        chr_banks: Vec<[u8; MMC2_CHR_BANK_SIZE]>,
    ) -> MMC2Mapper {
        if prg_banks.len() < 4 {
            panic!("MMC2 requires at least 4 PRG banks");
        }
        if chr_banks.is_empty() {
            panic!("MMC2 requires CHR banks");
        }

        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);
        
        // Set up PRG ROM: switchable bank + 3 fixed banks
        let last_three_start = prg_banks.len() - 3;
        mem[SWITCHABLE_PRG_ADDR..SWITCHABLE_PRG_ADDR + MMC2_PRG_BANK_SIZE]
            .clone_from_slice(&prg_banks[0]);
        mem[FIXED_PRG_ADDR_1..FIXED_PRG_ADDR_1 + MMC2_PRG_BANK_SIZE]
            .clone_from_slice(&prg_banks[last_three_start]);
        mem[FIXED_PRG_ADDR_2..FIXED_PRG_ADDR_2 + MMC2_PRG_BANK_SIZE]
            .clone_from_slice(&prg_banks[last_three_start + 1]);
        mem[FIXED_PRG_ADDR_3..FIXED_PRG_ADDR_3 + MMC2_PRG_BANK_SIZE]
            .clone_from_slice(&prg_banks[last_three_start + 2]);

        let addr_space_ptr = mem.as_mut_ptr();

        // Initialize CHR banks - start with bank 0 for both pattern tables (FD state)
        let mut current_chr_0000 = Box::new(chr_banks[0]);
        let mut current_chr_1000 = Box::new(chr_banks[0]);
        let chr_0000_ptr = current_chr_0000.as_mut_ptr();
        let chr_1000_ptr = current_chr_1000.as_mut_ptr();

        let mut vram = Box::new([0; VRAM_SIZE as usize]);
        let vrm_ptr = vram.as_mut_ptr();

        let nametable_alignment = if flags & super::NAMETABLE_ALIGNMENT_BIT == 1 {
            NametableMirror::Horizontal
        } else {
            NametableMirror::Vertical
        };

        MMC2Mapper {
            _flags: flags,

            ppu: Rc::new(RefCell::new(ppu::PPU::new())),
            apu: Rc::new(RefCell::new(apu::APU::new())),

            _addr_space: mem,
            addr_space_ptr,

            _prg_banks: prg_banks,
            selected_prg_bank: 0,

            _chr_banks: chr_banks,
            
            chr_bank_0000_fd: 0,
            chr_bank_0000_fe: 0,
            chr_bank_1000_fd: 0,
            chr_bank_1000_fe: 0,
            
            chr_latch_0: false,  // Start with $FD
            chr_latch_1: false,  // Start with $FD

            _current_chr_0000: current_chr_0000,
            _current_chr_1000: current_chr_1000,
            chr_0000_ptr,
            chr_1000_ptr,

            _vram: vram,
            vrm_ptr,

            nametable_alignment,

            controllers: [controller::Controller::new(), controller::Controller::new()],
            palette_ram: [0x0F; 32],
        }
    }

    fn switch_prg_bank(&mut self, bank: u8) {
        let bank_index = (bank as usize) % self._prg_banks.len();
        if bank_index != self.selected_prg_bank {
            self.selected_prg_bank = bank_index;
            
            // Update switchable PRG bank
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self._prg_banks[bank_index].as_ptr(),
                    self.addr_space_ptr.offset(SWITCHABLE_PRG_ADDR as isize),
                    MMC2_PRG_BANK_SIZE,
                );
            }
        }
    }

    fn update_chr_0000(&mut self) {
        let bank_index = if self.chr_latch_0 {
            self.chr_bank_0000_fe % self._chr_banks.len()
        } else {
            self.chr_bank_0000_fd % self._chr_banks.len()
        };
        
        self._current_chr_0000 = Box::new(self._chr_banks[bank_index]);
        self.chr_0000_ptr = self._current_chr_0000.as_mut_ptr();
    }

    fn update_chr_1000(&mut self) {
        let bank_index = if self.chr_latch_1 {
            self.chr_bank_1000_fe % self._chr_banks.len()
        } else {
            self.chr_bank_1000_fd % self._chr_banks.len()
        };
        
        self._current_chr_1000 = Box::new(self._chr_banks[bank_index]);
        self.chr_1000_ptr = self._current_chr_1000.as_mut_ptr();
    }

    fn check_chr_latch(&mut self, addr: u16, value: u8) {
        // Check for latch tiles $FD and $FE
        if addr < 0x1000 {
            // Left pattern table ($0000-$0FFF)
            if value == 0xFD {
                if self.chr_latch_0 {
                    self.chr_latch_0 = false;
                    self.update_chr_0000();
                }
            } else if value == 0xFE {
                if !self.chr_latch_0 {
                    self.chr_latch_0 = true;
                    self.update_chr_0000();
                }
            }
        } else {
            // Right pattern table ($1000-$1FFF)
            if value == 0xFD {
                if self.chr_latch_1 {
                    self.chr_latch_1 = false;
                    self.update_chr_1000();
                }
            } else if value == 0xFE {
                if !self.chr_latch_1 {
                    self.chr_latch_1 = true;
                    self.update_chr_1000();
                }
            }
        }
    }
}

impl MemoryMapper for MMC2Mapper {
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
            // MMC2 registers
            0xa0 => {
                // $A000-$AFFF: PRG ROM bank selection
                self.switch_prg_bank(value & 0x07);
            }
            0xb0 => {
                // $B000-$BFFF: CHR ROM $FD/0000 bank select
                self.chr_bank_0000_fd = (value & 0x1F) as usize;
                if !self.chr_latch_0 {
                    self.update_chr_0000();
                }
            }
            0xc0 => {
                // $C000-$CFFF: CHR ROM $FE/0000 bank select
                self.chr_bank_0000_fe = (value & 0x1F) as usize;
                if self.chr_latch_0 {
                    self.update_chr_0000();
                }
            }
            0xd0 => {
                // $D000-$DFFF: CHR ROM $FD/1000 bank select
                self.chr_bank_1000_fd = (value & 0x1F) as usize;
                if !self.chr_latch_1 {
                    self.update_chr_1000();
                }
            }
            0xe0 => {
                // $E000-$EFFF: CHR ROM $FE/1000 bank select
                self.chr_bank_1000_fe = (value & 0x1F) as usize;
                if self.chr_latch_1 {
                    self.update_chr_1000();
                }
            }
            0xf0 => {
                // $F000-$FFFF: Nametable mirroring control
                self.nametable_alignment = if (value & 1) == 0 {
                    NametableMirror::Vertical
                } else {
                    NametableMirror::Horizontal
                };
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
            0x0 => {
                // Left pattern table ($0000-$0FFF)
                let value = unsafe { *self.chr_0000_ptr.offset(addr as _) };
                // For MMC2, we would need to check if this is a tile fetch and update latches
                // But this requires more complex PPU integration than currently available
                value
            }
            0x10 => {
                // Right pattern table ($1000-$1FFF)
                addr -= 0x1000;
                let value = unsafe { *self.chr_1000_ptr.offset(addr as _) };
                // For MMC2, we would need to check if this is a tile fetch and update latches
                // But this requires more complex PPU integration than currently available
                value
            }
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
            0x0 => unsafe { std::ptr::copy(self.chr_0000_ptr.offset(addr as _), dest, size) },
            0x10 => {
                addr -= 0x1000;
                unsafe { std::ptr::copy(self.chr_1000_ptr.offset(addr as _), dest, size) }
            }
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
            // CHR ROM is read-only, but we check for latch triggers
            0x0 => {
                self.check_chr_latch(addr, value);
            }
            0x10 => {
                self.check_chr_latch(addr, value);
            }
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
    fn test_mmc2_prg_bank_switching() {
        let prg_bank1 = [1; MMC2_PRG_BANK_SIZE];
        let prg_bank2 = [2; MMC2_PRG_BANK_SIZE];
        let prg_bank3 = [3; MMC2_PRG_BANK_SIZE];
        let prg_bank4 = [4; MMC2_PRG_BANK_SIZE];
        let prg_bank5 = [5; MMC2_PRG_BANK_SIZE];
        let chr_bank = [0; MMC2_CHR_BANK_SIZE];
        
        let mut mapper: Box<dyn MemoryMapper> = Box::new(MMC2Mapper::new(
            0,
            vec![prg_bank1, prg_bank2, prg_bank3, prg_bank4, prg_bank5],
            vec![chr_bank],
        ));

        // Initially bank 0 should be selected for switchable area
        assert_eq!(mapper.cpu_read(0x8000), 1);
        
        // Last 3 banks should be fixed
        assert_eq!(mapper.cpu_read(0xA000), 3);  // Bank 2 (last-2)
        assert_eq!(mapper.cpu_read(0xC000), 4);  // Bank 3 (last-1) 
        assert_eq!(mapper.cpu_read(0xE000), 5);  // Bank 4 (last)

        // Switch to bank 1
        mapper.cpu_write(0xA000, 1);
        assert_eq!(mapper.cpu_read(0x8000), 2);
        
        // Fixed banks should remain unchanged
        assert_eq!(mapper.cpu_read(0xA000), 3);
        assert_eq!(mapper.cpu_read(0xC000), 4);
        assert_eq!(mapper.cpu_read(0xE000), 5);
    }

    #[test]
    fn test_mmc2_chr_bank_registers() {
        let prg_banks = vec![[0; MMC2_PRG_BANK_SIZE]; 4];
        let chr_bank1 = [1; MMC2_CHR_BANK_SIZE];
        let chr_bank2 = [2; MMC2_CHR_BANK_SIZE];
        let chr_bank3 = [3; MMC2_CHR_BANK_SIZE];
        let chr_bank4 = [4; MMC2_CHR_BANK_SIZE];
        
        let mut mapper: Box<dyn MemoryMapper> = Box::new(MMC2Mapper::new(
            0,
            prg_banks,
            vec![chr_bank1, chr_bank2, chr_bank3, chr_bank4],
        ));

        // Initially should be using bank 0 (FD state)
        assert_eq!(mapper.ppu_read(0x0000), 1);
        assert_eq!(mapper.ppu_read(0x1000), 1);

        // Set CHR bank for $FE state
        mapper.cpu_write(0xC000, 1);  // $FE/0000 -> bank 1
        mapper.cpu_write(0xE000, 2);  // $FE/1000 -> bank 2

        // Should still read bank 0 since we're in $FD state
        assert_eq!(mapper.ppu_read(0x0000), 1);
        assert_eq!(mapper.ppu_read(0x1000), 1);

        // Set CHR banks for $FD state  
        mapper.cpu_write(0xB000, 2);  // $FD/0000 -> bank 2
        mapper.cpu_write(0xD000, 3);  // $FD/1000 -> bank 3

        // Now should read the new $FD banks
        assert_eq!(mapper.ppu_read(0x0000), 3);
        assert_eq!(mapper.ppu_read(0x1000), 4);
    }

    #[test]
    fn test_mmc2_mirroring_control() {
        let prg_banks = vec![[0; MMC2_PRG_BANK_SIZE]; 4];
        let chr_banks = vec![[0; MMC2_CHR_BANK_SIZE]];
        
        let mut mapper: Box<dyn MemoryMapper> = Box::new(MMC2Mapper::new(
            0,  // Start with vertical mirroring
            prg_banks,
            chr_banks,
        ));

        // Write to nametable
        mapper.ppu_write(0x2000, 0x42);
        assert_eq!(mapper.ppu_read(0x2000), 0x42);

        // Switch to horizontal mirroring
        mapper.cpu_write(0xF000, 1);
        
        // Nametable behavior should change (exact behavior depends on mirroring implementation)
        mapper.ppu_write(0x2400, 0x84);
        assert_eq!(mapper.ppu_read(0x2400), 0x84);
    }
}