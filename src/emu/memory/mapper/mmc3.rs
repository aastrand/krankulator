use super::super::super::apu;
use super::super::super::io;
use super::super::ppu;
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

    irq_latch: u8,
    irq_enable: bool,
    irq_reload: bool,
    irq_pending: bool,
}

impl MMC3Mapper {
    pub fn new(_flags: u8, prg_banks: Vec<[u8; 16384]>, chr_banks: Vec<[u8; 8192]>) -> MMC3Mapper {
        // Flatten PRG/CHR banks into 8K/1K chunks
        let mut prg_rom = vec![];
        for bank in prg_banks.iter() {
            prg_rom.push(<[u8; PRG_BANK_SIZE]>::try_from(&bank[0..PRG_BANK_SIZE]).unwrap());
            prg_rom.push(
                <[u8; PRG_BANK_SIZE]>::try_from(&bank[PRG_BANK_SIZE..2 * PRG_BANK_SIZE]).unwrap(),
            );
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
            bank_regs: [0; 8],
            irq_latch: 0,
            irq_enable: false,
            irq_reload: false,
            irq_pending: false,
        }
    }

    fn prg_bank(&self, idx: usize) -> usize {
        let banks = self.prg_rom.len();
        (self.bank_regs[idx] as usize) % banks
    }

    fn map_prg(&self, addr: u16) -> Option<&[u8; PRG_BANK_SIZE]> {
        let bank = match addr {
            0x8000..=0x9FFF => {
                if self.prg_bank_mode == 0 {
                    self.prg_bank(6)
                } else {
                    self.prg_rom.len() - 2
                }
            }
            0xA000..=0xBFFF => self.prg_bank(7),
            0xC000..=0xDFFF => {
                if self.prg_bank_mode == 0 {
                    self.prg_rom.len() - 2
                } else {
                    self.prg_bank(6)
                }
            }
            0xE000..=0xFFFF => self.prg_rom.len() - 1,
            _ => return None,
        };
        self.prg_rom.get(bank)
    }
}

impl MemoryMapper for MMC3Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        match addr {
            0x2000..=0x2007 => self.ppu.borrow_mut().read(addr, self as _),
            0x4000..=0x4013 | 0x4015 => self.apu.borrow_mut().read(addr),
            0x4014 => self.ppu.borrow_mut().read(addr, self as _),
            0x4016 => self.controllers[0].poll(),
            0x4017 => self.controllers[1].poll(),
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize],
            0x8000..=0xFFFF => {
                if let Some(bank) = self.map_prg(addr) {
                    bank[(addr as usize) % PRG_BANK_SIZE]
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => self.prg_ram[(addr - 0x6000) as usize] = value,
            0x8000..=0x9FFF => {
                if addr & 1 == 0 {
                    // Bank select
                    self.bank_select = value & 0x07;
                    self.prg_bank_mode = (value >> 6) & 1;
                    self.chr_bank_mode = (value >> 7) & 1;
                } else {
                    // Bank data
                    self.bank_regs[self.bank_select as usize] = value;
                }
            }
            0xA000..=0xBFFF => {
                if addr & 1 == 0 {
                    // Mirroring (ignored for now)
                } else {
                    // PRG RAM protect (ignored for now)
                }
            }
            0xC000..=0xDFFF => {
                if addr & 1 == 0 {
                    // IRQ latch
                    self.irq_latch = value;
                } else {
                    // IRQ reload
                    self.irq_reload = true;
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
        let idx = (addr as usize) / CHR_BANK_SIZE;
        if let Some(bank) = self.chr_mem.get(idx) {
            bank[(addr as usize) % CHR_BANK_SIZE]
        } else {
            0
        }
    }

    fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        let idx = (addr as usize) / CHR_BANK_SIZE;
        if let Some(bank) = self.chr_mem.get(idx) {
            unsafe { std::ptr::copy(bank.as_ptr().add(addr as usize % CHR_BANK_SIZE), dest, size) }
        }
    }

    fn ppu_write(&mut self, _addr: u16, _value: u8) {
        if self.chr_is_ram {
            let idx = (_addr as usize) / CHR_BANK_SIZE;
            if let Some(bank) = self.chr_mem.get_mut(idx) {
                bank[(_addr as usize) % CHR_BANK_SIZE] = _value;
            }
        }
    }

    fn code_start(&mut self) -> u16 {
        let last = self.prg_rom.last().unwrap();
        let lo = last[0x3FFC - 0x2000];
        let hi = last[0x3FFD - 0x2000];
        (hi as u16) << 8 | (lo as u16)
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
}
