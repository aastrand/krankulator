use super::super::super::apu;
use super::super::super::io;
use super::super::ppu;
use super::{mirror_nametable_addr, NametableMirror, RESET_TARGET_ADDR};
use crate::emu::memory::MemoryMapper;

use std::cell::RefCell;
use std::rc::Rc;

const PRG_BANK_SIZE: usize = 0x2000; // 8KB
const CHR_BANK_SIZE: usize = 0x0400; // 1KB

pub struct MMC3Mapper {
    ppu: Rc<RefCell<ppu::PPU>>,
    apu: Rc<RefCell<apu::APU>>,
    controllers: [io::controller::Controller; 2],

    prg_rom: Vec<[u8; PRG_BANK_SIZE]>,
    chr_mem: Vec<[u8; CHR_BANK_SIZE]>,
    chr_is_ram: bool,
    prg_ram: Box<[u8; 0x2000]>,

    prg_bank_mode: u8,
    chr_bank_mode: u8,
    bank_select: u8,
    bank_regs: [u8; 8],

    // IRQ counter state
    irq_counter: u8,
    irq_latch: u8,
    irq_enable: bool,
    irq_reload: bool,
    irq_pending: bool,

    // Mirroring
    mirroring: NametableMirror,

    // VRAM for nametables
    vram: Box<[u8; 0x800]>,

    // CPU RAM (0x0000-0x07FF, mirrored to 0x1FFF)
    cpu_ram: Box<[u8; 0x800]>,
}

impl MMC3Mapper {
    pub fn new(flags: u8, prg_banks: Vec<[u8; 16384]>, chr_banks: Vec<[u8; 8192]>) -> MMC3Mapper {
        // Flatten PRG/CHR banks into 8K/1K chunks
        let mut prg_rom = vec![];
        for (_i, bank) in prg_banks.iter().enumerate() {
            prg_rom.push(<[u8; PRG_BANK_SIZE]>::try_from(&bank[0..PRG_BANK_SIZE]).unwrap());
            prg_rom.push(
                <[u8; PRG_BANK_SIZE]>::try_from(&bank[PRG_BANK_SIZE..2 * PRG_BANK_SIZE]).unwrap(),
            );
            // println!("MMC3: Split 16KB PRG bank {} into two 8KB banks ({} and {})",
            //          i, prg_rom.len() - 2, prg_rom.len() - 1);
        }

        if prg_rom.is_empty() {
            panic!("MMC3: No PRG banks loaded!");
        }

        let (chr_mem, chr_is_ram) = if chr_banks.is_empty() {
            (vec![[0; CHR_BANK_SIZE]; 8], true)
        } else {
            let mut chr_mem = vec![];
            for bank in chr_banks {
                for i in 0..8 {
                    chr_mem.push(
                        <[u8; CHR_BANK_SIZE]>::try_from(
                            &bank[i * CHR_BANK_SIZE..(i + 1) * CHR_BANK_SIZE],
                        )
                        .unwrap(),
                    );
                }
            }
            (chr_mem, false)
        };
        MMC3Mapper {
            ppu: Rc::new(RefCell::new(ppu::PPU::new())),
            apu: Rc::new(RefCell::new(apu::APU::new())),
            controllers: [
                io::controller::Controller::new(),
                io::controller::Controller::new(),
            ],
            prg_rom,
            chr_mem,
            chr_is_ram,
            prg_ram: Box::new([0; 0x2000]),
            prg_bank_mode: 0,
            chr_bank_mode: 0,
            bank_select: 0,
            // Initialize bank registers to valid values for compatibility
            bank_regs: [0, 2, 4, 5, 6, 7, 0, 1],
            irq_counter: 0,
            irq_latch: 0,
            irq_enable: false,
            irq_reload: false,
            irq_pending: false,
            // Initialize mirroring from iNES header flags
            mirroring: if flags & 1 == 0 {
                NametableMirror::Horizontal
            } else {
                NametableMirror::Vertical
            },
            vram: Box::new([0; 0x800]),
            cpu_ram: Box::new([0; 0x800]),
        }
    }

    fn get_prg_bank(&self, register: usize) -> usize {
        let banks = self.prg_rom.len();
        (self.bank_regs[register] as usize) % banks
    }

    fn get_chr_bank(&self, register: usize) -> usize {
        if self.chr_mem.is_empty() {
            return 0;
        }
        let banks = self.chr_mem.len();
        (self.bank_regs[register] as usize) % banks
    }

    fn map_prg(&self, addr: u16) -> Option<&[u8; PRG_BANK_SIZE]> {
        let bank = match addr {
            0x8000..=0x9FFF => {
                if self.prg_bank_mode == 0 {
                    // Mode 0: R6 at $8000-$9FFF
                    self.get_prg_bank(6)
                } else {
                    // Mode 1: Fixed second-to-last bank at $8000-$9FFF
                    self.prg_rom.len().saturating_sub(2)
                }
            }
            0xA000..=0xBFFF => {
                // R7 always controls $A000-$BFFF
                self.get_prg_bank(7)
            }
            0xC000..=0xDFFF => {
                if self.prg_bank_mode == 0 {
                    // Mode 0: Fixed second-to-last bank at $C000-$DFFF
                    if self.prg_rom.len() >= 2 {
                        self.prg_rom.len() - 2
                    } else {
                        0
                    }
                } else {
                    // Mode 1: R6 at $C000-$DFFF
                    self.get_prg_bank(6)
                }
            }
            0xE000..=0xFFFF => {
                // Always fixed to last bank
                if self.prg_rom.len() >= 1 {
                    self.prg_rom.len() - 1
                } else {
                    0
                }
            }
            _ => return None,
        };

        self.prg_rom.get(bank)
    }

    fn map_chr(&self, addr: u16) -> (usize, usize) {
        let bank_idx = match addr {
            0x0000..=0x03FF => {
                if self.chr_bank_mode == 0 {
                    // Mode 0: 2KB banks at $0000 and $0800
                    self.get_chr_bank(0) & 0xFE
                } else {
                    // Mode 1: 1KB banks
                    self.get_chr_bank(2)
                }
            }
            0x0400..=0x07FF => {
                if self.chr_bank_mode == 0 {
                    // Mode 0: Second half of 2KB bank
                    (self.get_chr_bank(0) & 0xFE) + 1
                } else {
                    // Mode 1: 1KB banks
                    self.get_chr_bank(3)
                }
            }
            0x0800..=0x0BFF => {
                if self.chr_bank_mode == 0 {
                    // Mode 0: 2KB banks
                    self.get_chr_bank(1) & 0xFE
                } else {
                    // Mode 1: 1KB banks
                    self.get_chr_bank(4)
                }
            }
            0x0C00..=0x0FFF => {
                if self.chr_bank_mode == 0 {
                    // Mode 0: Second half of 2KB bank
                    (self.get_chr_bank(1) & 0xFE) + 1
                } else {
                    // Mode 1: 1KB banks
                    self.get_chr_bank(5)
                }
            }
            0x1000..=0x13FF => {
                if self.chr_bank_mode == 0 {
                    // Mode 0: 1KB banks
                    self.get_chr_bank(2)
                } else {
                    // Mode 1: 2KB banks
                    self.get_chr_bank(0) & 0xFE
                }
            }
            0x1400..=0x17FF => {
                if self.chr_bank_mode == 0 {
                    // Mode 0: 1KB banks
                    self.get_chr_bank(3)
                } else {
                    // Mode 1: Second half of 2KB bank
                    (self.get_chr_bank(0) & 0xFE) + 1
                }
            }
            0x1800..=0x1BFF => {
                if self.chr_bank_mode == 0 {
                    // Mode 0: 1KB banks
                    self.get_chr_bank(4)
                } else {
                    // Mode 1: 2KB banks
                    self.get_chr_bank(1) & 0xFE
                }
            }
            0x1C00..=0x1FFF => {
                if self.chr_bank_mode == 0 {
                    // Mode 0: 1KB banks
                    self.get_chr_bank(5)
                } else {
                    // Mode 1: Second half of 2KB bank
                    (self.get_chr_bank(1) & 0xFE) + 1
                }
            }
            _ => 0,
        };

        let offset = addr as usize % CHR_BANK_SIZE;
        (bank_idx, offset)
    }
}

impl MemoryMapper for MMC3Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = super::mirror_addr(addr);
        match addr {
            0x0000..=0x1FFF => {
                // CPU RAM (mirrored every 0x800 bytes)
                let ram_addr = addr & 0x07FF;
                unsafe { *self.cpu_ram.as_ptr().offset(ram_addr as isize) }
            }
            0x2000..=0x2007 => self.ppu.borrow_mut().read(addr, self as _),
            0x4000..=0x4013 | 0x4015 => self.apu.borrow_mut().read(addr),
            0x4014 => self.ppu.borrow_mut().read(addr, self as _),
            0x4016 => self.controllers[0].poll(),
            0x4017 => self.controllers[1].poll(),
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize],
            0x8000..=0xFFFF => {
                if let Some(bank) = self.map_prg(addr) {
                    let value = bank[(addr as usize) % PRG_BANK_SIZE];
                    value
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let addr = super::mirror_addr(addr);
        match addr {
            0x0000..=0x1FFF => {
                // CPU RAM (mirrored every 0x800 bytes)
                let ram_addr = addr & 0x07FF;
                unsafe {
                    *self.cpu_ram.as_mut_ptr().offset(ram_addr as isize) = value;
                }
            }
            0x2000..=0x2007 => {
                let should_write =
                    self.ppu
                        .borrow_mut()
                        .write(addr, value, self.prg_ram.as_mut_ptr());
                if let Some((addr, value)) = should_write {
                    self.ppu_write(addr, value);
                }
            }
            0x4000..=0x4013 | 0x4015 => self.apu.borrow_mut().write(addr, value),
            0x4014 => {
                self.ppu
                    .borrow_mut()
                    .write(addr, value, self.prg_ram.as_mut_ptr());
            }
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize] = value,
            0x8000..=0x9FFF => {
                if addr & 1 == 0 {
                    // Bank select
                    self.bank_select = value & 0x07;
                    self.prg_bank_mode = (value >> 6) & 1;
                    self.chr_bank_mode = (value >> 7) & 1;
                } else {
                    // Bank data - enforce 2KB bank constraints for R0 and R1
                    let reg = self.bank_select as usize;
                    if reg <= 1 {
                        // R0 and R1 are 2KB banks - force even numbers
                        self.bank_regs[reg] = value & 0xFE;
                    } else {
                        // R2-R7 are 1KB banks
                        self.bank_regs[reg] = value;
                    }
                }
            }
            0xA000..=0xBFFF => {
                if addr & 1 == 0 {
                    // Mirroring control
                    self.mirroring = if value & 1 == 0 {
                        NametableMirror::Vertical
                    } else {
                        NametableMirror::Horizontal
                    };
                } else {
                    // PRG RAM protect (ignored for now)
                }
            }
            0xC000..=0xDFFF => {
                if addr & 1 == 0 {
                    // IRQ latch - set the value to reload the counter with
                    self.irq_latch = value;
                } else {
                    // IRQ reload - reload the counter on next tick
                    self.irq_reload = true;
                    self.irq_counter = 0; // Clear counter immediately
                }
            }
            0xE000..=0xFFFF => {
                if addr & 1 == 0 {
                    // IRQ disable
                    self.irq_enable = false;
                    self.irq_pending = false;
                } else {
                    // IRQ enable
                    self.irq_enable = true;
                }
            }
            0x4016 => {}
            0x4017 => {}
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                // CHR ROM/RAM access
                let (bank_idx, offset) = self.map_chr(addr);
                if let Some(bank) = self.chr_mem.get(bank_idx) {
                    bank[offset]
                } else {
                    0
                }
            }
            0x2000..=0x3EFF => {
                // Nametable access with mirroring
                let mirrored_addr = mirror_nametable_addr(addr, self.mirroring);
                let vram_addr = (mirrored_addr & 0x7FF) as usize;
                self.vram[vram_addr]
            }
            0x3F00..=0x3FFF => {
                // Palette RAM - should be handled by PPU
                0
            }
            _ => 0,
        }
    }

    fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        match addr {
            0x0000..=0x1FFF => {
                let (bank_idx, offset) = self.map_chr(addr);
                if let Some(bank) = self.chr_mem.get(bank_idx) {
                    let copy_size = std::cmp::min(size, CHR_BANK_SIZE - offset);
                    unsafe {
                        std::ptr::copy(bank.as_ptr().add(offset), dest, copy_size);
                    }
                }
            }
            0x2000..=0x3EFF => {
                let mirrored_addr = mirror_nametable_addr(addr, self.mirroring);
                let vram_addr = (mirrored_addr & 0x7FF) as usize;
                let copy_size = std::cmp::min(size, 0x800 - vram_addr);
                unsafe {
                    std::ptr::copy(self.vram.as_ptr().add(vram_addr), dest, copy_size);
                }
            }
            _ => {}
        }
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x1FFF => {
                if self.chr_is_ram {
                    let (bank_idx, offset) = self.map_chr(addr);
                    if let Some(bank) = self.chr_mem.get_mut(bank_idx) {
                        bank[offset] = value;
                    }
                }
            }
            0x2000..=0x3EFF => {
                // Nametable write with mirroring
                let mirrored_addr = mirror_nametable_addr(addr, self.mirroring);
                let vram_addr = (mirrored_addr & 0x7FF) as usize;
                self.vram[vram_addr] = value;
            }
            _ => {}
        }
    }

    fn code_start(&mut self) -> u16 {
        // Read reset vector through proper mapper CPU read (like other mappers do)
        let lo = self.cpu_read(RESET_TARGET_ADDR);
        let hi = self.cpu_read(RESET_TARGET_ADDR + 1);
        let start_addr = ((hi as u16) << 8) | (lo as u16);

        start_addr
    }

    fn ppu(&self) -> Rc<RefCell<ppu::PPU>> {
        Rc::clone(&self.ppu)
    }

    fn apu(&self) -> Rc<RefCell<apu::APU>> {
        Rc::clone(&self.apu)
    }

    fn controllers(&mut self) -> &mut [io::controller::Controller; 2] {
        &mut self.controllers
    }

    fn poll_irq(&mut self) -> bool {
        if self.irq_pending {
            self.irq_pending = false;
            true
        } else {
            false
        }
    }

    fn ppu_cycle_260(&mut self, scanline: u16) {
        // MMC3 IRQ counter ticks on cycle 260 of visible and pre-render scanlines
        if scanline < 240 || scanline == 261 {
            if self.irq_reload || self.irq_counter == 0 {
                self.irq_counter = self.irq_latch;
                self.irq_reload = false;
            } else {
                self.irq_counter = self.irq_counter.saturating_sub(1);
            }

            if self.irq_counter == 0 && self.irq_enable {
                self.irq_pending = true;
            }
        }
    }
}
