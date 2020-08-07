use super::super::super::io;
use super::super::ppu;
use super::super::*;
use super::*;

use std::cell::RefCell;
use std::cmp::max;
use std::rc::Rc;

const BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: u16 = 4 * 1024;
const CPU_RAM_SIZE: usize = 2 * 1024;
const MMC_RAM_SIZE: usize = 8 * 1024;
const VRAM_SIZE: u16 = 2 * 1024;

const MMC_RAM_ADDR: u16 = 0x6000;
const LOW_BANK_ADDR: u16 = 0x8000;
const HIGH_BANK_ADDR: u16 = 0xc000;

const SR_INITIAL_VALUE: u8 = 0b1_0000;

pub struct MMC1Mapper {
    ppu: Rc<RefCell<ppu::PPU>>,

    _cpu_ram: Box<[u8; CPU_RAM_SIZE]>,
    cpu_ram_ptr: *mut u8,

    _mmc_ram: Box<[u8; MMC_RAM_SIZE]>,
    mmc_ram_ptr: *mut u8,

    banks: Vec<[u8; BANK_SIZE]>,
    low_bank: *mut u8,
    high_bank: *mut u8,

    chr_banks: Vec<[u8; CHR_BANK_SIZE as _]>,
    low_chr_bank: *mut u8,
    high_chr_bank: *mut u8,

    _vram: Box<[u8; VRAM_SIZE as usize]>,
    vrm_ptr: *mut u8,

    reg_write_shift_register: u8,

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
        mut prg_banks: Vec<[u8; BANK_SIZE]>,
        chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]>,
    ) -> MMC1Mapper {
        if prg_banks.len() < 2 {
            panic!("Expected at least two PRG banks");
        }
        let mut cpu_ram = Box::new([0; CPU_RAM_SIZE]);
        let cpu_ram_ptr = cpu_ram.as_mut_ptr();

        let mut mmc_ram = Box::new([0; MMC_RAM_SIZE]);
        let mmc_ram_ptr = mmc_ram.as_mut_ptr();

        let low_bank_ptr: *mut u8 = unsafe { prg_banks.get_unchecked_mut(0).as_mut_ptr() };

        let mut chunked_chr_banks: Vec<[u8; CHR_BANK_SIZE as _]> = vec![];
        for chr_bank in chr_banks {
            let mut chunk: [u8; CHR_BANK_SIZE as _] = [0; CHR_BANK_SIZE as _];
            chunk.clone_from_slice(&chr_bank[0..CHR_BANK_SIZE as _]);
            chunked_chr_banks.push(chunk);

            let mut chunk2: [u8; CHR_BANK_SIZE as _] = [0; CHR_BANK_SIZE as _];
            chunk2.clone_from_slice(&chr_bank[CHR_BANK_SIZE as _..io::loader::CHR_BANK_SIZE]);
            chunked_chr_banks.push(chunk2);
        }
        let chr_ptr: *mut u8 = unsafe { chunked_chr_banks.get_unchecked_mut(0).as_mut_ptr() };

        let mut vram = Box::new([0; VRAM_SIZE as usize]);
        let vrm_ptr = vram.as_mut_ptr();

        let mut mapper = MMC1Mapper {
            ppu: Rc::new(RefCell::new(ppu::PPU::new())),

            // 0x0000-0x07FF + mirroring to 0x1FFF
            _cpu_ram: cpu_ram,
            cpu_ram_ptr: cpu_ram_ptr,

            // 0x6000-0x7FFF
            _mmc_ram: mmc_ram,
            mmc_ram_ptr: mmc_ram_ptr,

            banks: prg_banks,

            // 0x8000-0xBFFF
            low_bank: low_bank_ptr,
            //  0xC000-0xFFFF
            high_bank: low_bank_ptr,

            // PPU $0000-$0FFF: 4 KB switchable CHR bank
            // PPU $1000-$1FFF: 4 KB switchable CHR bank
            chr_banks: chunked_chr_banks,
            low_chr_bank: chr_ptr,
            high_chr_bank: chr_ptr,

            _vram: vram,
            vrm_ptr: vrm_ptr,

            reg_write_shift_register: SR_INITIAL_VALUE,

            // Control
            // 0x8000-0x9FFF
            reg0: 0,
            // CHR bank 0
            // 0xA000-0xBFFF
            reg1: 0,
            // CHR bank 1
            // 0xC000-0xDFFF
            reg2: 0,
            // PRG bank
            // 0xE000-0xFFFF
            reg3: 0,

            //nametable_alignment: flags & super::NAMETABLE_ALIGNMENT_BIT,
            controllers: [controller::Controller::new(), controller::Controller::new()],
        };

        unsafe {
            let len = mapper.banks.len();
            mapper.high_bank = mapper.banks.get_unchecked_mut(max(0, len - 1)).as_mut_ptr();

            let len = mapper.chr_banks.len();
            mapper.high_chr_bank = mapper
                .chr_banks
                .get_unchecked_mut(max(0, len - 1))
                .as_mut_ptr();
        }

        mapper
    }

    fn handle_register_write(&mut self, value: u8) -> Option<u8> {
        let mut result = None;
        /*
        The "reset" signal is generated by writing a byte value whose high bit is a 1
        to any of the four MMC1 registers.  This signal affects the bits of reg0 as
        follows:

            bit 0 - unknown
            bit 1 - unknown
            bit 2 - reset to logic 1
            bit 3 - reset to logic 1
            bit 4 - unaffected
        */
        if value & 0x80 == 0x80 {
            self.reg0 |= 0b00110;
            self.reg_write_shift_register = SR_INITIAL_VALUE;
        //println!("Reset SR");
        } else {
            let was = self.reg_write_shift_register;

            self.reg_write_shift_register >>= 1;
            let mask = (value & 0b0000_0001) << 4;
            self.reg_write_shift_register |= mask;

            if was & 0b0000_0001 == 1 {
                result = Some(self.reg_write_shift_register);
                self.reg_write_shift_register = SR_INITIAL_VALUE;
            }
            //println!("SR now {:05b}", self.reg_write_shift_register);
        }

        result
    }

    fn _read_bus(&mut self, addr: u16) -> u8 {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);

        match page {
            0x0 | 0x10 => unsafe { *self.cpu_ram_ptr.offset(addr as _) },
            0x20 => {
                // PPU registers
                self.ppu.borrow_mut().read(addr, self as _)
            }
            0x40 => match addr {
                0x4014 => self.ppu.borrow_mut().read(addr, self as _),
                0x4016 => self.controllers[0].poll(),
                0x4017 => self.controllers[1].poll(),
                _ => 0, //unsafe { *self.cpu_ram_ptr.offset(addr as _) },
            },
            0x60 | 0x70 => unsafe { *self.mmc_ram_ptr.offset((addr % MMC_RAM_ADDR) as _) },
            0x80 | 0x90 | 0xa0 | 0xb0 => unsafe {
                *self.low_bank.offset((addr % LOW_BANK_ADDR) as _)
            },
            0xc0 | 0xd0 | 0xe0 | 0xf0 => unsafe {
                *self.high_bank.offset((addr % HIGH_BANK_ADDR) as _)
            },
            _ => panic!("Read at addr {:X} not mapped", addr),
        }
    }

    fn _write_bus(&mut self, addr: u16, value: u8) {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);

        //println!("Writing {:X} to {:X}", value, addr);
        match page {
            0x0 | 0x10 => unsafe { *self.cpu_ram_ptr.offset(addr as _) = value },
            0x20 => {
                //println!("Write to PPU reg {:X}: {:X}", addr, value);
                let should_write = self.ppu.borrow_mut().write(addr, value, self.cpu_ram_ptr);
                if let Some((addr, value)) = should_write {
                    self.ppu_write(addr, value);
                }
            }
            0x40 => {
                if addr == 0x4014 {
                    self.ppu.borrow_mut().write(addr, value, self.cpu_ram_ptr);
                }
            }
            0x50 => {} // ??
            0x60 | 0x70 => unsafe {
                *self.mmc_ram_ptr.offset((addr - MMC_RAM_ADDR) as _) = value;
            },
            0x80 | 0x90 => {
                //println!("Write to MMC reg0: {:X}", value);
                if let Some(result) = self.handle_register_write(value) {
                    /*
                    bit 2 - toggles between low PRGROM area switching and high
                        PRGROM area switching
                        0 = high PRGROM switching, 1 = low PRGROM switching

                    bit 3 - toggles between 16KB and 32KB PRGROM bank switching
                        0 = 32KB PRGROM switching, 1 = 16KB PRGROM switching
                    */
                    self.reg0 = result;
                    //println!("Set reg0 to {:X}", value);
                }
            }
            0xa0 | 0xb0 => {
                if let Some(result) = self.handle_register_write(value) {
                    self.reg1 = result;

                    // 4k switch
                    if self.reg0 & 0b0_1000 == 0b0_1000 {
                        // For carts with 8 KiB of CHR (be it ROM or RAM), MMC1 follows the common behavior of using only the low-order bits: 
                        // the bank number is in effect ANDed with 1.
                        // TODO: clean
                        let bank = if self.chr_banks.len() == 2 {
                            result & 0b0001
                        } else {
                            result & 0b1111
                        };
                        println!("Switched low chr bank to {} (4k mode)", (bank) as usize);
                        unsafe {
                            self.low_chr_bank = self.chr_banks
                                //.get_unchecked_mut(bank as usize)
                                [bank as usize]
                                .as_mut_ptr();
                        }
                    } else {
                        // 8k
                        let bank = result & 0b1110;
                        println!("Switched low chr bank to {} (8k mode)", (bank) as usize);
                        println!(
                            "Switched high chr bank to {} (8k mode)",
                            (bank + 1) as usize
                        );
                        unsafe {
                            self.low_chr_bank = self
                                .chr_banks
                                //.get_unchecked_mut((bank) as usize)
                                [bank as usize]
                                .as_mut_ptr();
                            self.high_chr_bank = self
                                .chr_banks
                                //.get_unchecked_mut((bank + 1) as usize)
                                [(bank + 1) as usize]
                                .as_mut_ptr();
                        }
                    }
                }
                //println!("Write to MMC reg1: {:X}", value);
            }
            0xc0 | 0xd0 => {
                if let Some(result) = self.handle_register_write(value) {
                    self.reg2 = result;

                    // 4k switch
                    if self.reg0 & 0b1000 == 0b1000 {
                        // TODO: clean
                        let bank = if self.chr_banks.len() == 2 {
                            result & 0b0001
                        } else {
                            result & 0b1111
                        };
                        println!("Switched high chr bank to {} (4k mode)", (bank) as usize);
                        unsafe {
                            self.high_chr_bank = self.chr_banks
                                //.get_unchecked_mut(bank as usize)
                                    [bank as usize]
                                .as_mut_ptr();
                        }
                    } else {
                        // 8k
                        let bank = result & 0b1110;
                        println!("Switched low chr bank to {} (8k mode)", (bank) as usize);
                        println!(
                            "Switched high chr bank to {} (8k mode)",
                            (bank + 1) as usize
                        );
                        unsafe {
                            self.low_chr_bank = self
                                .chr_banks
                                //.get_unchecked_mut((bank) as usize)
                                [bank as usize]
                                .as_mut_ptr();
                            self.high_chr_bank = self
                                .chr_banks
                                //.get_unchecked_mut((bank + 1) as usize)
                                [(bank + 1) as usize]
                                .as_mut_ptr();
                        }
                    }
                } //println!("Write to MMC reg2: {:X}", value);
            }
            0xe0 | 0xf0 => {
                //println!("Write to MMC reg3: {:X}", value);

                if let Some(result) = self.handle_register_write(value) {
                    self.reg3 = result;

                    // |++--- PRG ROM bank mode (0, 1: switch 32 KB at $8000, ignoring low bit of bank number;
                    // |                         2: fix first bank at $8000 and switch 16 KB bank at $C000;
                    // |                         3: fix last bank at $C000 and switch 16 KB bank at $8000)
                    match (self.reg0 & 0b01100) >> 2 {
                        0 | 1 => {
                            // 32k bank switching
                            let base_bank = result & 0b01110;
                            /*println!("Switched low bank to {} (32K mode)", base_bank as usize);
                            println!(
                                "Switched high bank to {} (32K mode)",
                                (base_bank + 1) as usize
                            );*/
                            unsafe {
                                self.low_bank = self
                                    .banks
                                    //.get_unchecked_mut(base_bank as usize)
                                    [base_bank as usize]
                                    .as_mut_ptr();
                                self.high_bank = self
                                    .banks
                                    //.get_unchecked_mut((base_bank + 1) as usize)
                                    [(base_bank +1 ) as usize]
                                    .as_mut_ptr();
                            }
                        }
                        3 => {
                            //println!("Switched low bank to {}", (result) as usize);
                            unsafe {
                                self.low_bank =
                                    //self.banks.get_unchecked_mut(result as usize).as_mut_ptr();
                                    self.banks[(result & 0b1111) as usize].as_mut_ptr();
                            }
                        }
                        2 => {
                            println!("Switched high bank to {}", result as usize);
                            unsafe {
                                self.high_bank =
                                    //self.banks.get_unchecked_mut(result as usize).as_mut_ptr();
                                    self.banks[(result & 0b1111) as usize].as_mut_ptr();
                            }
                        }
                        _ => panic!("Can't happen!"),
                    }
                    self.reg_write_shift_register = SR_INITIAL_VALUE;
                }
            }
            _ => panic!("Write at addr {:X} not mapped", addr),
        };
    }

    fn _ppu_read(&self, addr: u16) -> u8 {
        let mut addr = addr;
        let page = addr_to_page(addr);

        match page {
            0x0 => unsafe { *self.low_chr_bank.offset(addr as _) },
            0x10 => unsafe { *self.high_chr_bank.offset((addr % CHR_BANK_SIZE) as _) },
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
            0x0 => unsafe { std::ptr::copy(self.low_chr_bank.offset(addr as _), dest, size) },
            0x10 => unsafe {
                std::ptr::copy(
                    self.high_chr_bank.offset((addr % CHR_BANK_SIZE) as _),
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
            0x0 => unsafe { *self.low_chr_bank.offset(addr as _) = value },
            0x10 => unsafe { *self.high_chr_bank.offset((addr % CHR_BANK_SIZE) as _) = value },
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
        mapper.reg0 = 0b11101;

        mapper._write_bus(0x8000, 0x80);

        assert_eq!(mapper.reg0, 0b11111);
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

        // TODO: initial reg0 value might change in the future
        let mut mapper = MMC1Mapper::new(0, prg_banks, chr_banks);

        // Write to mmaped mmc reg, note that rom does not change
        mapper._write_bus(0x8000, 41);
        assert_eq!(mapper._read_bus(0x8000), 1);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xa000, 20);
        assert_eq!(mapper._read_bus(0xa000), 1);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xe000, 1);
        assert_eq!(mapper._read_bus(0xe000), 1);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xc000, 8);
        assert_eq!(mapper._read_bus(0xc000), 1);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0x8000, 21);
        assert_eq!(mapper._read_bus(0x8000), 1);
        assert_eq!(mapper.reg0, 0b10101);
    }

    #[test]
    fn test_switch_low_prg_16() {
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for b in 0..16 {
            // Init with bank num
            prg_banks.push([b; BANK_SIZE]);
        }

        let mut chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]> = vec![];
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);

        let mut mapper = MMC1Mapper::new(0, prg_banks, chr_banks);

        // Set prg switching to low 16k
        mapper._write_bus(0x8000, 40); // 0
        assert_eq!(mapper._read_bus(0x8000), 0);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xa000, 20); // 0
        assert_eq!(mapper._read_bus(0x8000), 0);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xe000, 9); // 1
        assert_eq!(mapper._read_bus(0x8000), 0);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xc000, 5); // 1
        assert_eq!(mapper._read_bus(0x8000), 0);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0x8000, 2); // 0
        assert_eq!(mapper._read_bus(0x8000), 0);
        assert_eq!(mapper.reg0, 0b01100);

        // switch to bank 15
        mapper._write_bus(0xc000, 41);
        assert_eq!(mapper._read_bus(0x8000), 0);

        mapper._write_bus(0xc000, 21);
        assert_eq!(mapper._read_bus(0x8000), 0);

        mapper._write_bus(0xc000, 11);
        assert_eq!(mapper._read_bus(0x8000), 0);

        mapper._write_bus(0xc000, 5);
        assert_eq!(mapper._read_bus(0x8000), 0);

        mapper._write_bus(0xe000, 2);
        assert_eq!(mapper._read_bus(0x8000), 15);
    }

    #[test]
    fn test_switch_high_prg_16() {
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for b in 0..16 {
            // Init with bank num
            prg_banks.push([b; BANK_SIZE]);
        }

        let mut chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]> = vec![];
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);

        let mut mapper = MMC1Mapper::new(0, prg_banks, chr_banks);

        // Set prg switching to high 16k
        mapper._write_bus(0x8000, 40); // 0
        assert_eq!(mapper._read_bus(0x8000), 0);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xa000, 20); // 0
        assert_eq!(mapper._read_bus(0x8000), 0);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xe000, 8); // 0
        assert_eq!(mapper._read_bus(0x8000), 0);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xc000, 5); // 1
        assert_eq!(mapper._read_bus(0x8000), 0);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0x8000, 2); // 0
        assert_eq!(mapper._read_bus(0x8000), 0);
        assert_eq!(mapper.reg0, 0b01000);

        // switch to bank 7
        mapper._write_bus(0xc000, 0x41);
        assert_eq!(mapper._read_bus(0xc000), 15);

        mapper._write_bus(0xc000, 21);
        assert_eq!(mapper._read_bus(0xc000), 15);

        mapper._write_bus(0xc000, 11);
        assert_eq!(mapper._read_bus(0xc000), 15);

        mapper._write_bus(0xc000, 4);
        assert_eq!(mapper._read_bus(0xc000), 15);

        mapper._write_bus(0xe000, 2);
        assert_eq!(mapper._read_bus(0xc000), 7);
    }

    #[test]
    fn test_switch_prg_32k() {
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for b in 0..16 {
            // Init with bank num
            prg_banks.push([b; BANK_SIZE]);
        }

        let mut chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]> = vec![];
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);
        chr_banks.push([0; io::loader::CHR_BANK_SIZE as _]);

        let mut mapper = MMC1Mapper::new(0, prg_banks, chr_banks);

        // Set prg switching to 32k
        mapper._write_bus(0x8000, 40); // 0
        assert_eq!(mapper._read_bus(0x8000), 0);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xa000, 20); // 0
        assert_eq!(mapper._read_bus(0x8000), 0);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xe000, 8); // 0
        assert_eq!(mapper._read_bus(0x8000), 0);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xc000, 4); // 0
        assert_eq!(mapper._read_bus(0x8000), 0);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0x8000, 2); // 0
        assert_eq!(mapper._read_bus(0x8000), 0);
        assert_eq!(mapper._read_bus(0xc000), 15);
        assert_eq!(mapper.reg0, 0b00000);

        // switch to bank 8
        mapper._write_bus(0xc000, 40);
        assert_eq!(mapper._read_bus(0xc000), 15);

        mapper._write_bus(0xc000, 20);
        assert_eq!(mapper._read_bus(0xc000), 15);

        mapper._write_bus(0xc000, 10);
        assert_eq!(mapper._read_bus(0xc000), 15);

        mapper._write_bus(0xc000, 5);
        assert_eq!(mapper._read_bus(0xc000), 15);

        mapper._write_bus(0xe000, 1); // ignored
        assert_eq!(mapper._read_bus(0x8000), 8);
        assert_eq!(mapper._read_bus(0xc000), 9);
    }

    #[test]
    fn test_switch_low_chr_4k() {
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for b in 0..16 {
            // Init with bank num
            prg_banks.push([b; BANK_SIZE]);
        }

        let mut chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]> = vec![];
        for b in 0..16 {
            // Init with bank num
            chr_banks.push([b; io::loader::CHR_BANK_SIZE as _]);
        }

        let mut mapper = MMC1Mapper::new(0, prg_banks, chr_banks);

        // Set chr switching to 4k
        mapper._write_bus(0x8000, 40); // 0
        assert_eq!(mapper._ppu_read(0x0000), 0);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xa000, 20); // 0
        assert_eq!(mapper._ppu_read(0x0000), 0);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xe000, 8); // 0
        assert_eq!(mapper._ppu_read(0x0000), 0);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xc000, 5); // 1
        assert_eq!(mapper._ppu_read(0x0000), 0);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0x8000, 2); // 0
        assert_eq!(mapper._ppu_read(0x0000), 0);
        assert_eq!(mapper._ppu_read(0x1000), 15);
        assert_eq!(mapper.reg0, 0b01000);

        // switch chr low to bank 8
        mapper._write_bus(0xc000, 40);
        assert_eq!(mapper._ppu_read(0x0000), 0);

        mapper._write_bus(0xc000, 20);
        assert_eq!(mapper._ppu_read(0x0000), 0);

        mapper._write_bus(0xc000, 10);
        assert_eq!(mapper._ppu_read(0x0000), 0);

        mapper._write_bus(0xc000, 5);
        assert_eq!(mapper._ppu_read(0x0000), 0);

        mapper._write_bus(0xa000, 2);
        assert_eq!(mapper._ppu_read(0x0000), 4);
        assert_eq!(mapper._ppu_read(0x1000), 15);
    }

    #[test]
    fn test_switch_high_chr_4k() {
        let mut prg_banks: Vec<[u8; BANK_SIZE]> = vec![];
        for b in 0..16 {
            // Init with bank num
            prg_banks.push([b; BANK_SIZE]);
        }

        let mut chr_banks: Vec<[u8; io::loader::CHR_BANK_SIZE as _]> = vec![];
        for b in 0..16 {
            // Init with bank num
            chr_banks.push([b; io::loader::CHR_BANK_SIZE as _]);
        }

        let mut mapper = MMC1Mapper::new(0, prg_banks, chr_banks);

        // Set chr switching to 4k
        mapper._write_bus(0x8000, 40); // 0
        assert_eq!(mapper._ppu_read(0x1000), 15);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xa000, 20); // 0
        assert_eq!(mapper._ppu_read(0x1000), 15);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xe000, 8); // 0
        assert_eq!(mapper._ppu_read(0x1000), 15);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0xc000, 5); // 1
        assert_eq!(mapper._ppu_read(0x1000), 15);
        assert_eq!(mapper.reg0, 0);

        mapper._write_bus(0x8000, 2); // 0
        assert_eq!(mapper._ppu_read(0x0000), 0);
        assert_eq!(mapper._ppu_read(0x1000), 15);
        assert_eq!(mapper.reg0, 0b01000);

        // switch chr high to bank 4
        mapper._write_bus(0xc000, 40);
        assert_eq!(mapper._ppu_read(0x1000), 15);

        mapper._write_bus(0xc000, 20);
        assert_eq!(mapper._ppu_read(0x1000), 15);

        mapper._write_bus(0xc000, 11);
        assert_eq!(mapper._ppu_read(0x1000), 15);

        mapper._write_bus(0xc000, 4);
        assert_eq!(mapper._ppu_read(0x1000), 15);

        mapper._write_bus(0xc000, 2);
        assert_eq!(mapper._ppu_read(0x0000), 0);
        assert_eq!(mapper._ppu_read(0x1000), 2);
    }

    // TODO: chr switching 8k
}
