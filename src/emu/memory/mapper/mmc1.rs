use super::super::super::apu;
use super::super::super::io;
use super::super::ppu;
use super::super::*;
use super::*;

use std::cell::RefCell;
use std::rc::Rc;

const BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: u16 = 4 * 1024;
const CPU_RAM_SIZE: usize = 2 * 1024;
const MMC_RAM_SIZE: usize = 8 * 1024;
const VRAM_SIZE: u16 = 2 * 1024;

const MMC_RAM_ADDR: u16 = 0x6000;
const LOW_BANK_ADDR: u16 = 0x8000;
const HIGH_BANK_ADDR: u16 = 0xc000;

const SR_INITIAL_VALUE: u8 = 0b10000;

pub struct MMC1Mapper {
    ppu: Rc<RefCell<ppu::PPU>>,
    apu: Rc<RefCell<apu::APU>>,

    _cpu_ram: Box<[u8; CPU_RAM_SIZE]>,
    cpu_ram_ptr: *mut u8,

    _mmc_ram: Box<[u8; MMC_RAM_SIZE]>,
    mmc_ram_ptr: *mut u8,
    mmc_ram_enabled: bool,

    banks: Vec<[u8; BANK_SIZE]>,
    low_bank_idx: usize,
    high_bank_idx: usize,

    chr_banks: Vec<[u8; CHR_BANK_SIZE as _]>,
    low_chr_bank_idx: usize,
    high_chr_bank_idx: usize,

    _vram: Box<[u8; VRAM_SIZE as usize]>,
    vrm_ptr: *mut u8,

    reg_write_shift_register: u8,
    reg_write_count: u8,

    reg0: u8,
    #[allow(dead_code)]
    reg1: u8,
    #[allow(dead_code)]
    reg2: u8,
    reg3: u8,

    pub controllers: [controller::Controller; 2],
}

impl MMC1Mapper {
    pub fn new(
        _flags: u8,
        prg_banks: Vec<[u8; BANK_SIZE]>,
        chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]>,
    ) -> MMC1Mapper {
        if prg_banks.len() < 2 {
            panic!("Expected at least two PRG banks");
        }
        let mut cpu_ram = Box::new([0; CPU_RAM_SIZE]);
        let cpu_ram_ptr = cpu_ram.as_mut_ptr();

        let mut mmc_ram = Box::new([0; MMC_RAM_SIZE]);
        let mmc_ram_ptr = mmc_ram.as_mut_ptr();

        // If CHR banks are 8K, split into 4K banks; otherwise, use as-is
        let mut chunked_chr_banks: Vec<[u8; CHR_BANK_SIZE as _]> = vec![];
        for chr_bank in chr_banks {
            if chr_bank.len() == CHR_BANK_SIZE as usize * 2 {
                let mut chunk: [u8; CHR_BANK_SIZE as _] = [0; CHR_BANK_SIZE as _];
                chunk.clone_from_slice(&chr_bank[0..CHR_BANK_SIZE as _]);
                chunked_chr_banks.push(chunk);
                let mut chunk2: [u8; CHR_BANK_SIZE as _] = [0; CHR_BANK_SIZE as _];
                chunk2.clone_from_slice(&chr_bank[CHR_BANK_SIZE as _..]);
                chunked_chr_banks.push(chunk2);
            } else {
                let mut chunk: [u8; CHR_BANK_SIZE as _] = [0; CHR_BANK_SIZE as _];
                chunk.clone_from_slice(&chr_bank[0..CHR_BANK_SIZE as _]);
                chunked_chr_banks.push(chunk);
            }
        }

        let mut mapper = MMC1Mapper {
            ppu: Rc::new(RefCell::new(ppu::PPU::new())),
            apu: Rc::new(RefCell::new(apu::APU::new())),

            _cpu_ram: cpu_ram,
            cpu_ram_ptr: cpu_ram_ptr,

            _mmc_ram: mmc_ram,
            mmc_ram_ptr: mmc_ram_ptr,
            mmc_ram_enabled: true,

            banks: prg_banks,
            low_bank_idx: 0,
            high_bank_idx: 1,

            chr_banks: chunked_chr_banks,
            low_chr_bank_idx: 0,
            high_chr_bank_idx: 1,

            _vram: Box::new([0; VRAM_SIZE as usize]),
            vrm_ptr: std::ptr::null_mut(),

            reg_write_shift_register: SR_INITIAL_VALUE,
            reg_write_count: 0,

            reg0: 0x0C, // per NESDev, after reset bits 2 and 3 are set
            reg1: 0,
            reg2: 0,
            reg3: 0,

            controllers: [controller::Controller::new(), controller::Controller::new()],
        };

        mapper.high_bank_idx = mapper.banks.len() - 1;
        mapper.high_chr_bank_idx = mapper.chr_banks.len() - 1;
        mapper.vrm_ptr = mapper._vram.as_mut_ptr();
        // Set initial banks according to control register
        mapper.update_banks(0x8000);
        // Ensure shift register and write count are initialized
        mapper.reg_write_shift_register = SR_INITIAL_VALUE;
        mapper.reg_write_count = 0;
        mapper
    }

    fn handle_register_write(&mut self, addr: u16, value: u8) {
        // Only bits 14 and 13 matter for register selection
        let reg_select = ((addr >> 13) & 0x03) as usize;
        if value & 0x80 != 0 {
            // Reset shift register and set control register to $0C (preserve all bits except 2 and 3)
            self.reg_write_shift_register = SR_INITIAL_VALUE;
            self.reg_write_count = 0;
            self.reg0 = (self.reg0 & 0xF9) | 0x0C;
            self.update_banks(addr);
            return;
        }
        // NESDev: shift right, input at bit 4, 5 bits total
        self.reg_write_shift_register = (self.reg_write_shift_register >> 1) | ((value & 1) << 4);
        self.reg_write_count += 1;
        if self.reg_write_count == 5 {
            let reg_value = self.reg_write_shift_register & 0x1F;
            match reg_select {
                0 => self.reg0 = reg_value,
                1 => self.reg1 = reg_value,
                2 => self.reg2 = reg_value,
                3 => self.reg3 = reg_value,
                _ => {}
            }
            self.update_banks(addr);
            self.reg_write_shift_register = SR_INITIAL_VALUE;
            self.reg_write_count = 0;
        }
    }

    fn update_banks(&mut self, _addr: u16) {
        // CHR bank switching
        let chr_mode = (self.reg0 >> 4) & 1;
        let chr_bank_mask = self.chr_banks.len().saturating_sub(1);
        if chr_mode == 0 {
            // 8K mode
            let bank = (((self.reg1 & 0b11110) as usize) & chr_bank_mask) % self.chr_banks.len();
            self.low_chr_bank_idx = bank;
            self.high_chr_bank_idx = (bank + 1) % self.chr_banks.len();
        } else {
            // 4K mode
            self.low_chr_bank_idx = ((self.reg1 as usize) & chr_bank_mask) % self.chr_banks.len();
            self.high_chr_bank_idx = ((self.reg2 as usize) & chr_bank_mask) % self.chr_banks.len();
        }
        // PRG bank switching
        let prg_mode = (self.reg0 >> 2) & 0b11;
        let prg_bank_mask = self.banks.len().saturating_sub(1);
        let prg_bank = (self.reg3 as usize) & prg_bank_mask;
        match prg_mode {
            0 | 1 => {
                // 32K mode: select even bank, next bank is odd
                let base_bank = (prg_bank & !1) & prg_bank_mask;
                self.low_bank_idx = base_bank;
                self.high_bank_idx = (base_bank + 1) & prg_bank_mask;
            }
            2 => {
                // Fix first bank at $8000, switch 16K at $C000
                self.low_bank_idx = 0;
                self.high_bank_idx = prg_bank;
            }
            3 => {
                // Switch 16K at $8000, fix last bank at $C000
                self.low_bank_idx = prg_bank;
                self.high_bank_idx = prg_bank_mask;
            }
            _ => {}
        }
        // RAM enable (bit 4 of reg3)
        self.mmc_ram_enabled = (self.reg3 & 0b10000) == 0;
    }

    fn _read_bus(&mut self, addr: u16) -> u8 {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);
        match page {
            0x0 | 0x10 => unsafe { *self.cpu_ram_ptr.offset(addr as _) },
            0x20 => self.ppu.borrow_mut().read(addr, self as _),
            0x40 => match addr {
                0x4014 => self.ppu.borrow_mut().read(addr, self as _),
                0x4015 => self.apu.borrow().read(addr),
                0x4016 => self.controllers[0].poll(),
                0x4017 => self.controllers[1].poll(),
                _ => 0,
            },
            0x60 | 0x70 => {
                if self.mmc_ram_enabled {
                    unsafe { *self.mmc_ram_ptr.offset((addr - MMC_RAM_ADDR) as _) }
                } else {
                    0
                }
            }
            0x80 | 0x90 | 0xa0 | 0xb0 => {
                self.banks[self.low_bank_idx][(addr % LOW_BANK_ADDR) as usize]
            }
            0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                self.banks[self.high_bank_idx][(addr % HIGH_BANK_ADDR) as usize]
            }
            _ => panic!("Read at addr {:X} not mapped", addr),
        }
    }

    fn _write_bus(&mut self, addr: u16, value: u8) {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);
        match page {
            0x0 | 0x10 => unsafe { *self.cpu_ram_ptr.offset(addr as _) = value },
            0x20 => {
                let should_write = self.ppu.borrow_mut().write(addr, value, self.cpu_ram_ptr);
                if let Some((addr, value)) = should_write {
                    self.ppu_write(addr, value);
                }
            }
            0x40 => {
                if addr == 0x4014 {
                    self.ppu.borrow_mut().write(addr, value, self.cpu_ram_ptr);
                } else if addr >= 0x4000 && addr <= 0x4017 {
                    self.apu.borrow_mut().write(addr, value);
                }
            }
            0x50 => {}
            0x60 | 0x70 => {
                if self.mmc_ram_enabled {
                    unsafe {
                        *self.mmc_ram_ptr.offset((addr - MMC_RAM_ADDR) as _) = value;
                    }
                }
            }
            0x80..=0xFF => self.handle_register_write(addr, value),
            _ => panic!("Write at addr {:X} not mapped", addr),
        }
    }

    fn _ppu_read(&self, addr: u16) -> u8 {
        let mut addr = addr;
        let page = addr_to_page(addr);
        match page {
            0x0 => self.chr_banks[self.low_chr_bank_idx][addr as usize],
            0x10 => self.chr_banks[self.high_chr_bank_idx][(addr % CHR_BANK_SIZE) as usize],
            0x20 => {
                addr = super::mirror_nametable_addr(addr, self.nametable_alignment()) % VRAM_SIZE;
                unsafe { *self.vrm_ptr.offset(addr as _) }
            }
            0x30 => unsafe { *self.vrm_ptr.offset((addr % VRAM_SIZE) as _) },
            _ => panic!("Addr not mapped for ppu_read: {:X}", addr),
        }
    }

    fn _ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        /*
        $0000-1FFF is normally mapped by the cartridge to a CHR-ROM or CHR-RAM, often with a bank switching mechanism.
        $2000-2FFF is normally mapped to the 2kB NES internal VRAM, providing 2 nametables with a mirroring configuration controlled by the cartridge, but it can be partly or fully remapped to RAM on the cartridge, allowing up to 4 simultaneous nametables.
        $3000-3EFF is usually a mirror of the 2kB region from $2000-2EFF. The PPU does not render from this address range, so this space has negligible utility.
        $3F00-3FFF is not configurable, always mapped to the internal palette control.
        */
        let mut addr = addr % MAX_VRAM_ADDR;
        let page = addr_to_page(addr);
        match page {
            0x0 => unsafe {
                std::ptr::copy(
                    self.chr_banks[self.low_chr_bank_idx]
                        .as_ptr()
                        .offset(addr as _),
                    dest,
                    size,
                )
            },
            0x10 => unsafe {
                std::ptr::copy(
                    self.chr_banks[self.high_chr_bank_idx]
                        .as_ptr()
                        .offset((addr % CHR_BANK_SIZE) as _),
                    dest,
                    size,
                )
            },
            0x20 => {
                addr = super::mirror_nametable_addr(addr, self.nametable_alignment()) % VRAM_SIZE;
                unsafe { std::ptr::copy(self.vrm_ptr.offset(addr as _), dest, size) }
            }
            0x30 => unsafe {
                std::ptr::copy(self.vrm_ptr.offset((addr % VRAM_SIZE) as _), dest, size)
            },

            _ => panic!("Addr not mapped for ppu_read: {:X}", addr),
        }
    }

    fn _ppu_write(&mut self, addr: u16, value: u8) {
        let mut addr = addr % MAX_VRAM_ADDR;
        let page = addr_to_page(addr);
        match page {
            0x0 => self.chr_banks[self.low_chr_bank_idx][addr as usize] = value,
            0x10 => self.chr_banks[self.high_chr_bank_idx][(addr % CHR_BANK_SIZE) as usize] = value,
            0x20 => {
                addr = super::mirror_nametable_addr(addr, self.nametable_alignment()) % VRAM_SIZE;
                unsafe { *self.vrm_ptr.offset(addr as _) = value }
            }
            0x30 => unsafe { *self.vrm_ptr.offset((addr % VRAM_SIZE) as _) = value },

            _ => panic!("Addr not mapped for ppu_write: {:X}", addr),
        }
    }

    fn nametable_alignment(&self) -> NametableMirror {
        match self.reg0 & 0b00011 {
            0 => NametableMirror::Lower,
            1 => NametableMirror::Higher,
            2 => NametableMirror::Vertical,
            3 => NametableMirror::Horizontal,
            _ => panic!("Can't happen"),
        }
    }
}

impl MemoryMapper for MMC1Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        self._read_bus(addr)
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        self._write_bus(addr, value);
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        self._ppu_read(addr)
    }

    fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        /*
        $0000-1FFF is normally mapped by the cartridge to a CHR-ROM or CHR-RAM, often with a bank switching mechanism.
        $2000-2FFF is normally mapped to the 2kB NES internal VRAM, providing 2 nametables with a mirroring configuration controlled by the cartridge, but it can be partly or fully remapped to RAM on the cartridge, allowing up to 4 simultaneous nametables.
        $3000-3EFF is usually a mirror of the 2kB region from $2000-2EFF. The PPU does not render from this address range, so this space has negligible utility.
        $3F00-3FFF is not configurable, always mapped to the internal palette control.
        */
        self._ppu_copy(addr, dest, size);
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        self._ppu_write(addr, value);
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
    fn test_code_start() {
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for _ in 0..16 {
            prg_banks.push([0; BANK_SIZE]);
        }

        prg_banks[15][0x3ffc] = 0x11;
        prg_banks[15][0x3ffd] = 0x47;

        let mut chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]> = vec![];
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);

        let mut mapper: Box<dyn MemoryMapper> = Box::new(MMC1Mapper::new(0, prg_banks, chr_banks));

        assert_eq!(mapper.code_start(), 0x4711);
    }

    #[test]
    fn test_ram() {
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for _ in 0..16 {
            prg_banks.push([0; BANK_SIZE]);
        }

        let mut chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]> = vec![];
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);

        let mut mapper = MMC1Mapper::new(0, prg_banks, chr_banks);

        // CPU ram
        mapper._write_bus(0x1173, 0x42);
        assert_eq!(mapper._read_bus(0x1173), 0x42);
        assert_eq!(mapper._cpu_ram[0x173], 0x42);

        // PRG ram
        mapper._write_bus(0x6123, 0x11);
        assert_eq!(mapper._read_bus(0x6123), 0x11);
        assert_eq!(mapper._mmc_ram[0x0123], 0x11);
    }

    #[test]
    fn test_reset() {
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for _ in 0..16 {
            prg_banks.push([0; BANK_SIZE]);
        }

        let mut chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]> = vec![];
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);

        let mut mapper = MMC1Mapper::new(0, prg_banks, chr_banks);
        mapper.reg0 = 0b11001;

        mapper._write_bus(0x8000, 0x80);

        assert_eq!(mapper.reg0, 0b11101);
    }

    #[test]
    fn test_write_reg0() {
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for _ in 0..16 {
            // Init with non-zero
            prg_banks.push([1; BANK_SIZE]);
        }

        let mut chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]> = vec![];
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);

        let mut mapper = MMC1Mapper::new(0, prg_banks, chr_banks);

        // After power-on, shift register is 0x10, so first 5 writes will set reg0 to 0x10 >> 1 | (D0 << 4) ...
        // For 5 writes of value 0, reg0 will be 0x10 >> 5 = 0
        mapper._write_bus(0x8000, 0);
        mapper._write_bus(0xa000, 0);
        mapper._write_bus(0xe000, 0);
        mapper._write_bus(0xc000, 0);
        mapper._write_bus(0x8000, 0);
        assert_eq!(mapper.reg0, 0);

        // Now write 0b10101 (21) in LSB-first order
        mapper._write_bus(0x8000, 1);
        mapper._write_bus(0xa000, 0);
        mapper._write_bus(0xe000, 1);
        mapper._write_bus(0xc000, 0);
        mapper._write_bus(0x8000, 1);
        assert_eq!(mapper.reg0, 0b10101);
    }

    #[test]
    fn test_switch_low_prg_16() {
        // NESDev: PRG bank number is masked by number of banks - 1
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for b in 0..16 {
            prg_banks.push([b; BANK_SIZE]);
        }
        let mut chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]> = vec![];
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);
        let mut mapper = MMC1Mapper::new(0, prg_banks, chr_banks);
        // Set PRG mode 3 (0b01100, LSB-first: 0,0,1,1,0) to reg0
        mapper._write_bus(0x8000, 0);
        mapper._write_bus(0xa000, 0);
        mapper._write_bus(0xe000, 1);
        mapper._write_bus(0xc000, 1);
        mapper._write_bus(0x8000, 0);
        // Write 0b11111 (31) to reg3, LSB-first: 1,1,1,1,1
        for _ in 0..5 {
            mapper._write_bus(0xe000, 1);
        }
        assert_eq!(mapper.reg3, 0b11111);
        assert_eq!(mapper.low_bank_idx, 15);
    }

    #[test]
    fn test_switch_high_prg_16() {
        // NESDev: PRG bank number is masked by number of banks - 1
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for b in 0..16 {
            prg_banks.push([b; BANK_SIZE]);
        }
        let mut chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]> = vec![];
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);
        let mut mapper = MMC1Mapper::new(0, prg_banks, chr_banks);
        // Set PRG mode 3 (0b01100, LSB-first: 0,0,1,1,0) to reg0
        mapper._write_bus(0x8000, 0);
        mapper._write_bus(0xa000, 0);
        mapper._write_bus(0xe000, 1);
        mapper._write_bus(0xc000, 1);
        mapper._write_bus(0x8000, 0);
        // Write 0b01111 (15) to reg3, LSB-first: 1,1,1,1,0
        mapper._write_bus(0xe000, 1);
        mapper._write_bus(0xe000, 1);
        mapper._write_bus(0xe000, 1);
        mapper._write_bus(0xe000, 1);
        mapper._write_bus(0xe000, 0);
        assert_eq!(mapper.reg3, 0b01111);
        assert_eq!(mapper.high_bank_idx, 15);
    }

    #[test]
    fn test_switch_prg_32k() {
        // NESDev: PRG bank number is masked by number of banks - 1
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for b in 0..16 {
            prg_banks.push([b; BANK_SIZE]);
        }
        let mut chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]> = vec![];
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);
        let mut mapper = MMC1Mapper::new(0, prg_banks, chr_banks);
        // Set PRG mode 0 (0b00000, LSB-first: 0,0,0,0,0) to reg0
        for _ in 0..5 {
            mapper._write_bus(0x8000, 0);
        }
        // Write 0b01000 (8) to reg3, LSB-first: 0,0,0,1,0
        mapper._write_bus(0xe000, 0);
        mapper._write_bus(0xe000, 0);
        mapper._write_bus(0xe000, 0);
        mapper._write_bus(0xe000, 1);
        mapper._write_bus(0xe000, 0);
        assert_eq!(mapper.reg3, 0b01000);
        assert_eq!(mapper.low_bank_idx, 8);
    }

    #[test]
    fn test_switch_low_chr_4k() {
        // NESDev: CHR bank number is masked by number of banks - 1
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for b in 0..16 {
            prg_banks.push([b; BANK_SIZE]);
        }
        let mut chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]> = vec![];
        for b in 0..16 {
            chr_banks.push([b; io::loader::CHR_BANK_SIZE as _]);
        }
        let mut mapper = MMC1Mapper::new(0, prg_banks, chr_banks);
        // Set chr switching to 4k (write 0b10000 to reg0, LSB-first: 0,0,0,0,1)
        for _ in 0..4 {
            mapper._write_bus(0x8000, 0);
        }
        mapper._write_bus(0x8000, 1);
        assert_eq!(mapper.reg0, 0b10000);
        // Write 0b01000 (8) to reg1, LSB-first: 0,0,0,1,0
        mapper._write_bus(0xa000, 0);
        mapper._write_bus(0xa000, 0);
        mapper._write_bus(0xa000, 0);
        mapper._write_bus(0xa000, 1);
        mapper._write_bus(0xa000, 0);
        assert_eq!(mapper.reg1, 0b01000);
        // The low chr bank index should be 8 (mask 8 to 8)
        assert_eq!(mapper.low_chr_bank_idx, 8);
    }

    #[test]
    fn test_switch_high_chr_4k() {
        // NESDev: CHR bank number is masked by number of banks - 1
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for b in 0..16 {
            prg_banks.push([b; BANK_SIZE]);
        }
        let mut chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]> = vec![];
        for b in 0..16 {
            chr_banks.push([b; io::loader::CHR_BANK_SIZE as _]);
        }
        let mut mapper = MMC1Mapper::new(0, prg_banks, chr_banks);
        // Set chr switching to 4k (write 0b10000 to reg0)
        for _ in 0..4 {
            mapper._write_bus(0x8000, 0);
        }
        mapper._write_bus(0x8000, 1); // 0b10000
        assert_eq!(mapper.reg0, 0b10000);
        // switch chr high to bank 15 (write 0b01111 to reg2)
        for _ in 0..4 {
            mapper._write_bus(0xc000, 1);
        }
        mapper._write_bus(0xc000, 0); // 0b01111
        assert_eq!(mapper.reg2, 0b01111);
        // The high chr bank index should be 15 (mask 15 to 15)
        assert_eq!(mapper.high_chr_bank_idx, 15);
    }

    #[test]
    fn test_switch_8k() {
        // NESDev: CHR bank number is masked by number of banks - 1
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for b in 0..16 {
            prg_banks.push([b; BANK_SIZE]);
        }
        let mut chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]> = vec![];
        for b in 0..16 {
            chr_banks.push([b; io::loader::CHR_BANK_SIZE as _]);
        }
        let mut mapper = MMC1Mapper::new(0, prg_banks, chr_banks);
        // Set chr switching to 8k (write 0b00000 to reg0, LSB-first: 0,0,0,0,0)
        for _ in 0..5 {
            mapper._write_bus(0x8000, 0);
        }
        assert_eq!(mapper.reg0, 0b00000);
        // Write 0b01000 (8) to reg1, LSB-first: 0,0,0,1,0
        mapper._write_bus(0xa000, 0);
        mapper._write_bus(0xa000, 0);
        mapper._write_bus(0xa000, 0);
        mapper._write_bus(0xa000, 1);
        mapper._write_bus(0xa000, 0);
        assert_eq!(mapper.reg1, 0b01000);
        // In 8k mode, only reg1 is used, and low_chr_bank_idx should be 8
        assert_eq!(mapper.low_chr_bank_idx, 8);
        // high_chr_bank_idx should be 9 (bank+1)
        assert_eq!(mapper.high_chr_bank_idx, 9);
    }
}
