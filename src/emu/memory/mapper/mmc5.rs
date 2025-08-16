use super::super::*;
use super::*;

use std::cell::RefCell;
use std::rc::Rc;

/*
MMC5 (Mapper 5) - Most complex Nintendo mapper, used in games like Castlevania III

This is a simplified implementation focusing on the most critical features:
- PRG ROM banking (mode 3: 8K + 8K + 16K fixed)
- CHR ROM banking (mode 0: 8K pages)
- Basic ExRAM functionality
- IRQ support (scanline counter)

Full MMC5 features not implemented in this basic version:
- Extended attribute modes
- Vertical split screen
- Audio channels
- Complex CHR modes
- Fill mode
- Multiplication register

Memory Layout:
$5000-$5015: Audio registers (not implemented)
$5100-$5130: Configuration registers
$5C00-$5FFF: ExRAM (1KB)
$6000-$7FFF: PRG RAM 
$8000-$9FFF: 8KB PRG ROM bank (switchable)
$A000-$BFFF: 8KB PRG ROM bank (switchable) 
$C000-$DFFF: 8KB PRG ROM bank (switchable)
$E000-$FFFF: 8KB PRG ROM bank (fixed to last bank)

PPU $0000-$1FFF: CHR ROM/RAM (8KB switchable)
*/

const MMC5_PRG_BANK_SIZE: usize = 8 * 1024;
const MMC5_CHR_BANK_SIZE: usize = 8 * 1024;
const MMC5_EXRAM_SIZE: usize = 1024;
const VRAM_SIZE: u16 = 2 * 1024;

const PRG_BANK_8000: usize = 0x8000;
const PRG_BANK_A000: usize = 0xA000;
const PRG_BANK_C000: usize = 0xC000;
const PRG_BANK_E000: usize = 0xE000;

pub struct MMC5Mapper {
    _flags: u8,

    ppu: Rc<RefCell<ppu::PPU>>,
    apu: Rc<RefCell<apu::APU>>,

    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    // PRG ROM banks
    _prg_banks: Vec<[u8; MMC5_PRG_BANK_SIZE]>,
    prg_bank_8000: usize,
    prg_bank_a000: usize,
    prg_bank_c000: usize,
    // E000 is always fixed to last bank

    // CHR ROM/RAM
    _chr_banks: Vec<[u8; MMC5_CHR_BANK_SIZE]>,
    selected_chr_bank: usize,
    _current_chr_bank: Box<[u8; MMC5_CHR_BANK_SIZE]>,
    chr_ptr: *mut u8,

    // ExRAM (Extended RAM)
    _exram: Box<[u8; MMC5_EXRAM_SIZE]>,
    exram_ptr: *mut u8,
    exram_mode: u8,  // 0-3, controls how ExRAM is used

    // Configuration
    prg_mode: u8,     // PRG ROM banking mode (0-3)
    chr_mode: u8,     // CHR ROM banking mode (0-3)

    // IRQ
    irq_counter: u8,
    irq_target: u8,
    irq_enabled: bool,
    irq_pending: bool,
    in_frame: bool,

    _vram: Box<[u8; VRAM_SIZE as usize]>,
    vrm_ptr: *mut u8,

    nametable_alignment: NametableMirror,

    pub controllers: [controller::Controller; 2],
    palette_ram: [u8; 32],
}

impl MMC5Mapper {
    pub fn new(
        flags: u8,
        prg_banks: Vec<[u8; MMC5_PRG_BANK_SIZE]>,
        chr_banks: Vec<[u8; MMC5_CHR_BANK_SIZE]>,
    ) -> MMC5Mapper {
        if prg_banks.len() < 4 {
            panic!("MMC5 requires at least 4 PRG banks");
        }

        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);
        
        // Initialize PRG banks in mode 3 (8K + 8K + 8K + 8K fixed)
        let last_bank = prg_banks.len() - 1;
        mem[PRG_BANK_8000..PRG_BANK_8000 + MMC5_PRG_BANK_SIZE]
            .clone_from_slice(&prg_banks[0]);
        mem[PRG_BANK_A000..PRG_BANK_A000 + MMC5_PRG_BANK_SIZE]
            .clone_from_slice(&prg_banks[1]);
        mem[PRG_BANK_C000..PRG_BANK_C000 + MMC5_PRG_BANK_SIZE]
            .clone_from_slice(&prg_banks[last_bank - 1]);
        mem[PRG_BANK_E000..PRG_BANK_E000 + MMC5_PRG_BANK_SIZE]
            .clone_from_slice(&prg_banks[last_bank]);

        let addr_space_ptr = mem.as_mut_ptr();

        // Initialize CHR
        let mut current_chr_bank = if chr_banks.is_empty() {
            Box::new([0; MMC5_CHR_BANK_SIZE])  // CHR RAM
        } else {
            Box::new(chr_banks[0])
        };
        let chr_ptr = current_chr_bank.as_mut_ptr();

        // Initialize ExRAM
        let mut exram = Box::new([0; MMC5_EXRAM_SIZE]);
        let exram_ptr = exram.as_mut_ptr();

        let mut vram = Box::new([0; VRAM_SIZE as usize]);
        let vrm_ptr = vram.as_mut_ptr();

        let nametable_alignment = if flags & super::NAMETABLE_ALIGNMENT_BIT == 1 {
            NametableMirror::Horizontal
        } else {
            NametableMirror::Vertical
        };

        MMC5Mapper {
            _flags: flags,

            ppu: Rc::new(RefCell::new(ppu::PPU::new())),
            apu: Rc::new(RefCell::new(apu::APU::new())),

            _addr_space: mem,
            addr_space_ptr,

            _prg_banks: prg_banks,
            prg_bank_8000: 0,
            prg_bank_a000: 1,
            prg_bank_c000: last_bank - 1,

            _chr_banks: chr_banks,
            selected_chr_bank: 0,
            _current_chr_bank: current_chr_bank,
            chr_ptr,

            _exram: exram,
            exram_ptr,
            exram_mode: 0,

            prg_mode: 3,  // Default to mode 3
            chr_mode: 0,  // Default to mode 0

            irq_counter: 0,
            irq_target: 0,
            irq_enabled: false,
            irq_pending: false,
            in_frame: false,

            _vram: vram,
            vrm_ptr,

            nametable_alignment,

            controllers: [controller::Controller::new(), controller::Controller::new()],
            palette_ram: [0x0F; 32],
        }
    }

    fn switch_prg_bank(&mut self, bank_addr: usize, bank: u8) {
        let bank_index = (bank as usize) % self._prg_banks.len();
        
        match bank_addr {
            PRG_BANK_8000 => {
                self.prg_bank_8000 = bank_index;
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        self._prg_banks[bank_index].as_ptr(),
                        self.addr_space_ptr.offset(PRG_BANK_8000 as isize),
                        MMC5_PRG_BANK_SIZE,
                    );
                }
            }
            PRG_BANK_A000 => {
                self.prg_bank_a000 = bank_index;
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        self._prg_banks[bank_index].as_ptr(),
                        self.addr_space_ptr.offset(PRG_BANK_A000 as isize),
                        MMC5_PRG_BANK_SIZE,
                    );
                }
            }
            PRG_BANK_C000 => {
                self.prg_bank_c000 = bank_index;
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        self._prg_banks[bank_index].as_ptr(),
                        self.addr_space_ptr.offset(PRG_BANK_C000 as isize),
                        MMC5_PRG_BANK_SIZE,
                    );
                }
            }
            _ => {}
        }
    }

    fn switch_chr_bank(&mut self, bank: u8) {
        if self._chr_banks.is_empty() {
            return; // CHR RAM mode
        }
        
        let bank_index = (bank as usize) % self._chr_banks.len();
        if bank_index != self.selected_chr_bank {
            self.selected_chr_bank = bank_index;
            self._current_chr_bank = Box::new(self._chr_banks[bank_index]);
            self.chr_ptr = self._current_chr_bank.as_mut_ptr();
        }
    }
}

impl MemoryMapper for MMC5Mapper {
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
            0x50 => {
                match addr {
                    0x5204 => {
                        // IRQ status register
                        let result = if self.irq_pending { 0x80 } else { 0x00 } |
                                   if self.in_frame { 0x40 } else { 0x00 };
                        self.irq_pending = false; // Reading clears IRQ
                        result
                    }
                    0x5C00..=0x5FFF => {
                        // ExRAM read
                        unsafe { *self.exram_ptr.offset((addr - 0x5C00) as isize) }
                    }
                    _ => 0, // Other MMC5 registers return 0 when read
                }
            }
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
            0x50 => {
                match addr {
                    0x5100 => {
                        // PRG Mode register
                        self.prg_mode = value & 3;
                    }
                    0x5101 => {
                        // CHR Mode register  
                        self.chr_mode = value & 3;
                    }
                    0x5104 => {
                        // ExRAM Mode register
                        self.exram_mode = value & 3;
                    }
                    0x5113 => {
                        // PRG Bank 0 ($6000-$7FFF) - PRG RAM banking
                        // For simplicity, we don't implement PRG RAM banking
                    }
                    0x5114 => {
                        // PRG Bank 1 ($8000-$9FFF)
                        self.switch_prg_bank(PRG_BANK_8000, value);
                    }
                    0x5115 => {
                        // PRG Bank 2 ($A000-$BFFF) 
                        self.switch_prg_bank(PRG_BANK_A000, value);
                    }
                    0x5116 => {
                        // PRG Bank 3 ($C000-$DFFF)
                        self.switch_prg_bank(PRG_BANK_C000, value);
                    }
                    0x5117 => {
                        // PRG Bank 4 ($E000-$FFFF) - usually fixed
                        // Some games might switch this, so we allow it
                        let bank_index = (value as usize) % self._prg_banks.len();
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                self._prg_banks[bank_index].as_ptr(),
                                self.addr_space_ptr.offset(PRG_BANK_E000 as isize),
                                MMC5_PRG_BANK_SIZE,
                            );
                        }
                    }
                    0x5120 => {
                        // CHR Bank 0 (background, $0000-$1FFF in 8K mode)
                        self.switch_chr_bank(value);
                    }
                    0x5203 => {
                        // IRQ Target
                        self.irq_target = value;
                    }
                    0x5204 => {
                        // IRQ Enable
                        self.irq_enabled = (value & 0x80) != 0;
                        if !self.irq_enabled {
                            self.irq_pending = false;
                        }
                    }
                    0x5C00..=0x5FFF => {
                        // ExRAM write
                        unsafe { *self.exram_ptr.offset((addr - 0x5C00) as isize) = value; }
                    }
                    _ => {
                        // Other MMC5 registers - not implemented
                    }
                }
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
            0x0 | 0x10 => {
                // CHR ROM/RAM
                if self._chr_banks.is_empty() {
                    // CHR RAM mode - return writable RAM
                    unsafe { *self.chr_ptr.offset(addr as _) }
                } else {
                    // CHR ROM mode
                    unsafe { *self.chr_ptr.offset(addr as _) }
                }
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
            0x0 | 0x10 => {
                // CHR RAM is writable, CHR ROM is not
                if self._chr_banks.is_empty() {
                    unsafe { *self.chr_ptr.offset(addr as _) = value }
                }
                // CHR ROM writes are ignored
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
        self.irq_pending
    }

    // MMC5 IRQ is triggered on specific scanlines
    fn ppu_cycle_260(&mut self, scanline: u16) {
        if scanline == 240 {
            self.in_frame = false;
        } else if scanline < 240 {
            self.in_frame = true;
            
            if self.irq_enabled {
                if self.irq_counter == self.irq_target {
                    self.irq_pending = true;
                }
                self.irq_counter = self.irq_counter.wrapping_add(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mmc5_prg_bank_switching() {
        let prg_bank1 = [1; MMC5_PRG_BANK_SIZE];
        let prg_bank2 = [2; MMC5_PRG_BANK_SIZE];
        let prg_bank3 = [3; MMC5_PRG_BANK_SIZE];
        let prg_bank4 = [4; MMC5_PRG_BANK_SIZE];
        let prg_bank5 = [5; MMC5_PRG_BANK_SIZE];
        
        let mut mapper: Box<dyn MemoryMapper> = Box::new(MMC5Mapper::new(
            0,
            vec![prg_bank1, prg_bank2, prg_bank3, prg_bank4, prg_bank5],
            vec![],
        ));

        // Initially should have banks 0,1,3,4 mapped
        assert_eq!(mapper.cpu_read(0x8000), 1);  // Bank 0
        assert_eq!(mapper.cpu_read(0xA000), 2);  // Bank 1  
        assert_eq!(mapper.cpu_read(0xC000), 4);  // Bank 3 (last-1)
        assert_eq!(mapper.cpu_read(0xE000), 5);  // Bank 4 (last)

        // Switch bank at $8000
        mapper.cpu_write(0x5114, 2);
        assert_eq!(mapper.cpu_read(0x8000), 3);

        // Switch bank at $A000
        mapper.cpu_write(0x5115, 3);
        assert_eq!(mapper.cpu_read(0xA000), 4);

        // Switch bank at $C000
        mapper.cpu_write(0x5116, 0);
        assert_eq!(mapper.cpu_read(0xC000), 1);
    }

    #[test]
    fn test_mmc5_chr_bank_switching() {
        let prg_banks = vec![[0; MMC5_PRG_BANK_SIZE]; 4];
        let chr_bank1 = [1; MMC5_CHR_BANK_SIZE];
        let chr_bank2 = [2; MMC5_CHR_BANK_SIZE];
        
        let mut mapper: Box<dyn MemoryMapper> = Box::new(MMC5Mapper::new(
            0,
            prg_banks,
            vec![chr_bank1, chr_bank2],
        ));

        // Initially should have bank 0
        assert_eq!(mapper.ppu_read(0x0000), 1);

        // Switch to bank 1
        mapper.cpu_write(0x5120, 1);
        assert_eq!(mapper.ppu_read(0x0000), 2);
    }

    #[test]
    fn test_mmc5_exram() {
        let prg_banks = vec![[0; MMC5_PRG_BANK_SIZE]; 4];
        
        let mut mapper: Box<dyn MemoryMapper> = Box::new(MMC5Mapper::new(
            0,
            prg_banks,
            vec![],
        ));

        // Write to ExRAM
        mapper.cpu_write(0x5C00, 0x42);
        mapper.cpu_write(0x5DFF, 0x84);

        // Read from ExRAM
        assert_eq!(mapper.cpu_read(0x5C00), 0x42);
        assert_eq!(mapper.cpu_read(0x5DFF), 0x84);
    }

    #[test]
    fn test_mmc5_irq() {
        let prg_banks = vec![[0; MMC5_PRG_BANK_SIZE]; 4];
        
        let mut mapper: Box<dyn MemoryMapper> = Box::new(MMC5Mapper::new(
            0,
            prg_banks,
            vec![],
        ));

        // Set IRQ target
        mapper.cpu_write(0x5203, 5);
        
        // Enable IRQ
        mapper.cpu_write(0x5204, 0x80);

        // Initially no IRQ
        assert!(!mapper.poll_irq());

        // Simulate scanlines until IRQ
        for scanline in 0..10 {
            mapper.ppu_cycle_260(scanline);
            if scanline == 5 {
                assert!(mapper.poll_irq());
                break;
            }
        }
    }
}