use super::*;
use crate::emu::io::controller;
use crate::emu::memory::{MemoryMapper, MAX_RAM_SIZE};
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_16K: usize = 16 * 1024;
const PRG_32K: usize = 32 * 1024;
const CHR_8K: usize = 8 * 1024;
const PRG_ROM_ADDR: usize = 0x8000;

pub struct Vrc3Mapper {
    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    prg_banks_16k: Vec<[u8; PRG_16K]>,

    irq_latch: u16,
    irq_counter: u16,
    irq_enabled: bool,
    irq_enable_after_ack: bool,
    irq_mode_8bit: bool,
    irq_pending: bool,

    ppu: PpuBus,
    pub controllers: [controller::Controller; 2],
}

impl Vrc3Mapper {
    pub fn new(flags: u8, prg_banks: Vec<[u8; PRG_16K]>, chr_banks: Vec<[u8; CHR_8K]>) -> Self {
        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);

        if !prg_banks.is_empty() {
            mem[PRG_ROM_ADDR..PRG_ROM_ADDR + PRG_16K].copy_from_slice(&prg_banks[0]);
            let last = prg_banks.len() - 1;
            mem[PRG_ROM_ADDR + PRG_16K..PRG_ROM_ADDR + PRG_32K].copy_from_slice(&prg_banks[last]);
        }

        let addr_space_ptr = mem.as_mut_ptr();
        let mirroring = mirroring_from_flags(flags);

        let ppu = if chr_banks.is_empty() {
            PpuBus::new_ram(CHR_8K, mirroring)
        } else {
            PpuBus::new_rom(&chr_banks[0], mirroring)
        };

        Vrc3Mapper {
            _addr_space: mem,
            addr_space_ptr,
            prg_banks_16k: prg_banks,
            irq_latch: 0,
            irq_counter: 0,
            irq_enabled: false,
            irq_enable_after_ack: false,
            irq_mode_8bit: false,
            irq_pending: false,
            ppu,
            controllers: [controller::Controller::new(), controller::Controller::new()],
        }
    }

    fn switch_prg_16k(&mut self, bank: usize) {
        let bank_index = bank % self.prg_banks_16k.len().max(1);
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.prg_banks_16k[bank_index].as_ptr(),
                self.addr_space_ptr.add(PRG_ROM_ADDR),
                PRG_16K,
            );
        }
    }
}

impl MemoryMapper for Vrc3Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = mirror_addr(addr);
        unsafe { *self.addr_space_ptr.offset(addr as isize) }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let addr = mirror_addr(addr);
        match (addr >> 12) & 0x0F {
            0x0 | 0x1 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
            0x6 | 0x7 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
            0x8 => {
                self.irq_latch = (self.irq_latch & 0xFFF0) | (value as u16 & 0x0F);
            }
            0x9 => {
                self.irq_latch = (self.irq_latch & 0xFF0F) | ((value as u16 & 0x0F) << 4);
            }
            0xA => {
                self.irq_latch = (self.irq_latch & 0xF0FF) | ((value as u16 & 0x0F) << 8);
            }
            0xB => {
                self.irq_latch = (self.irq_latch & 0x0FFF) | ((value as u16 & 0x0F) << 12);
            }
            0xC => {
                self.irq_pending = false;
                self.irq_enable_after_ack = value & 0x01 != 0;
                self.irq_enabled = value & 0x02 != 0;
                self.irq_mode_8bit = value & 0x04 != 0;
                if self.irq_enabled {
                    self.irq_counter = self.irq_latch;
                }
            }
            0xD => {
                self.irq_pending = false;
                self.irq_enabled = self.irq_enable_after_ack;
            }
            0xF => {
                self.switch_prg_16k((value & 0x07) as usize);
            }
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        self.ppu.read(addr)
    }

    unsafe fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        self.ppu.copy(addr, dest, size);
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        self.ppu.write(addr, value);
    }

    fn code_start(&mut self) -> u16 {
        ((self.cpu_read(RESET_TARGET_ADDR + 1) as u16) << 8)
            + self.cpu_read(RESET_TARGET_ADDR) as u16
    }

    fn controllers(&mut self) -> &mut [controller::Controller; 2] {
        &mut self.controllers
    }

    fn poll_irq(&mut self) -> bool {
        if self.irq_pending {
            self.irq_pending = false;
            return true;
        }
        false
    }

    fn cpu_cycle(&mut self, _ppu_dot: u64) {
        if !self.irq_enabled {
            return;
        }
        if self.irq_mode_8bit {
            let lo = (self.irq_counter & 0xFF).wrapping_add(1);
            if lo > 0xFF {
                self.irq_counter = (self.irq_counter & 0xFF00) | (self.irq_latch & 0x00FF);
                self.irq_pending = true;
            } else {
                self.irq_counter = (self.irq_counter & 0xFF00) | lo;
            }
        } else {
            self.irq_counter = self.irq_counter.wrapping_add(1);
            if self.irq_counter == 0 {
                self.irq_counter = self.irq_latch;
                self.irq_pending = true;
            }
        }
    }

    fn mapper_id(&self) -> u8 {
        73
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let ram = unsafe { std::slice::from_raw_parts(self.addr_space_ptr, MAX_RAM_SIZE) };
        w.write_bytes(ram);
        self.ppu.save_state(w);
        w.write_u16(self.irq_latch);
        w.write_u16(self.irq_counter);
        w.write_bool(self.irq_enabled);
        w.write_bool(self.irq_enable_after_ack);
        w.write_bool(self.irq_mode_8bit);
        w.write_bool(self.irq_pending);
        save_mirroring(w, self.ppu.mirroring);
        save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let ram = unsafe { std::slice::from_raw_parts_mut(self.addr_space_ptr, MAX_RAM_SIZE) };
        r.read_bytes_into(ram)?;
        self.ppu.load_state(r)?;
        self.irq_latch = r.read_u16()?;
        self.irq_counter = r.read_u16()?;
        self.irq_enabled = r.read_bool()?;
        self.irq_enable_after_ack = r.read_bool()?;
        self.irq_mode_8bit = r.read_bool()?;
        self.irq_pending = r.read_bool()?;
        self.ppu.mirroring = load_mirroring(r)?;
        load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper(num_prg_16k: usize) -> Box<dyn MemoryMapper> {
        let mut prg = Vec::new();
        for i in 0..num_prg_16k {
            let mut bank = [0xFFu8; PRG_16K];
            bank[0] = i as u8;
            prg.push(bank);
        }
        Box::new(Vrc3Mapper::new(0, prg, vec![]))
    }

    #[test]
    fn test_prg_banking() {
        let mut m = make_mapper(4);

        assert_eq!(m.cpu_read(0x8000), 0); // bank 0
        assert_eq!(m.cpu_read(0xC000), 3); // last bank fixed

        m.cpu_write(0xF000, 2);
        assert_eq!(m.cpu_read(0x8000), 2);
        assert_eq!(m.cpu_read(0xC000), 3); // still fixed
    }

    #[test]
    fn test_chr_ram() {
        let mut m = make_mapper(2);

        m.ppu_write(0x0000, 0x42);
        assert_eq!(m.ppu_read(0x0000), 0x42);
    }

    #[test]
    fn test_irq_16bit() {
        let mut m = make_mapper(2);

        // Set latch to 0xFFFC (will overflow after 4 cycles)
        m.cpu_write(0x8000, 0x0C); // latch bits 0-3
        m.cpu_write(0x9000, 0x0F); // latch bits 4-7
        m.cpu_write(0xA000, 0x0F); // latch bits 8-11
        m.cpu_write(0xB000, 0x0F); // latch bits 12-15

        // Enable IRQ, 16-bit mode
        m.cpu_write(0xC000, 0x02);

        assert!(!m.poll_irq());
        m.cpu_cycle(0);
        m.cpu_cycle(0);
        m.cpu_cycle(0);
        assert!(!m.poll_irq());
        m.cpu_cycle(0); // overflow: 0xFFFF → 0x0000
        assert!(m.poll_irq());
    }

    #[test]
    fn test_irq_ack() {
        let mut m = make_mapper(2);

        m.cpu_write(0x8000, 0x0E); // latch = 0xFFFE
        m.cpu_write(0x9000, 0x0F);
        m.cpu_write(0xA000, 0x0F);
        m.cpu_write(0xB000, 0x0F);
        m.cpu_write(0xC000, 0x03); // enable + enable-after-ack

        m.cpu_cycle(0);
        m.cpu_cycle(0); // overflow
        assert!(m.poll_irq());

        // Ack — should re-enable since bit 0 was set
        m.cpu_write(0xD000, 0x00);
        assert!(!m.poll_irq());
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut m = make_mapper(4);
        m.cpu_write(0xF000, 2);
        m.cpu_write(0x8000, 0x05);

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);
        let data = w.finish();

        let mut m2 = make_mapper(4);
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.cpu_read(0x8000), m.cpu_read(0x8000));
    }
}
