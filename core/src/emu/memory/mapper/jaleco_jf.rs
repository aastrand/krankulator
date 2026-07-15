use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_16K: usize = 16 * 1024;
const PRG_32K: usize = 32 * 1024;
const CHR_8K: usize = 8 * 1024;
const PRG_ROM_ADDR: usize = 0x8000;

pub struct JalecoJfMapper {
    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    is_jf19: bool,
    prg_banks_16k: Vec<[u8; PRG_16K]>,
    chr_banks_8k: Vec<[u8; CHR_8K]>,
    selected_prg: usize,
    selected_chr: usize,
    prev_write: u8,

    ppu: PpuBus,
    pub controllers: [controller::Controller; 2],
}

impl JalecoJfMapper {
    pub fn new(
        flags: u8,
        prg_banks_16k: Vec<[u8; PRG_16K]>,
        chr_banks_8k: Vec<[u8; CHR_8K]>,
        is_jf19: bool,
    ) -> Self {
        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);

        if !prg_banks_16k.is_empty() {
            mem[PRG_ROM_ADDR..PRG_ROM_ADDR + PRG_16K].copy_from_slice(&prg_banks_16k[0]);
            // JF-17: last bank fixed at $C000. JF-19: switchable at $C000 with
            // the latch powering up to 0 (bank 0 mapped at both halves).
            let high = if is_jf19 { 0 } else { prg_banks_16k.len() - 1 };
            mem[PRG_ROM_ADDR + PRG_16K..PRG_ROM_ADDR + PRG_32K]
                .copy_from_slice(&prg_banks_16k[high]);
        }

        let addr_space_ptr = mem.as_mut_ptr();

        let ppu = if chr_banks_8k.is_empty() {
            PpuBus::new_ram(CHR_8K, mirroring_from_flags(flags))
        } else {
            PpuBus::new_rom(&chr_banks_8k[0], mirroring_from_flags(flags))
        };

        JalecoJfMapper {
            _addr_space: mem,
            addr_space_ptr,
            is_jf19,
            prg_banks_16k,
            chr_banks_8k,
            selected_prg: 0,
            selected_chr: 0,
            prev_write: 0,
            ppu,
            controllers: [controller::Controller::new(), controller::Controller::new()],
        }
    }

    fn switch_prg(&mut self, bank: usize) {
        let bank_index = bank % self.prg_banks_16k.len().max(1);
        self.selected_prg = bank_index;
        let dest = if self.is_jf19 {
            PRG_ROM_ADDR + PRG_16K
        } else {
            PRG_ROM_ADDR
        };
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.prg_banks_16k[bank_index].as_ptr(),
                self.addr_space_ptr.add(dest),
                PRG_16K,
            );
        }
    }

    fn switch_chr(&mut self, bank: usize) {
        if self.chr_banks_8k.is_empty() {
            return;
        }
        let bank_index = bank % self.chr_banks_8k.len();
        if bank_index != self.selected_chr {
            self.selected_chr = bank_index;
            self.ppu.switch_chr_bank(&self.chr_banks_8k, bank_index);
        }
    }
}

impl MemoryMapper for JalecoJfMapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = mirror_addr(addr);
        unsafe { *self.addr_space_ptr.offset(addr as isize) }
    }

    fn cpu_peek(&self, addr: u16) -> u8 {
        let addr = mirror_addr(addr);
        unsafe { *self.addr_space_ptr.offset(addr as isize) }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let addr = mirror_addr(addr);
        let page = addr_to_page(addr);
        match page {
            0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
            0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                let value = unsafe { bus_conflict(self.addr_space_ptr, addr, value) };
                // Banks latch on the 0-to-1 transition of bit 7 (PRG) / bit 6 (CHR)
                if value & 0x80 != 0 && self.prev_write & 0x80 == 0 {
                    let mask = if self.is_jf19 { 0x0F } else { 0x07 };
                    self.switch_prg((value & mask) as usize);
                }
                if value & 0x40 != 0 && self.prev_write & 0x40 == 0 {
                    self.switch_chr((value & 0x0F) as usize);
                }
                self.prev_write = value;
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
        ((self.cpu_read(super::RESET_TARGET_ADDR + 1) as u16) << 8)
            + self.cpu_read(super::RESET_TARGET_ADDR) as u16
    }

    fn controllers(&mut self) -> &mut [controller::Controller; 2] {
        &mut self.controllers
    }

    fn poll_irq(&mut self) -> bool {
        false
    }

    fn mapper_id(&self) -> u8 {
        if self.is_jf19 {
            92
        } else {
            72
        }
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let ram = unsafe { std::slice::from_raw_parts(self.addr_space_ptr, MAX_RAM_SIZE) };
        w.write_bytes(ram);
        self.ppu.save_state(w);
        w.write_u8(self.selected_prg as u8);
        w.write_u8(self.selected_chr as u8);
        w.write_u8(self.prev_write);
        save_mirroring(w, self.ppu.mirroring);
        save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let ram = unsafe { std::slice::from_raw_parts_mut(self.addr_space_ptr, MAX_RAM_SIZE) };
        r.read_bytes_into(ram)?;
        self.ppu.load_state(r)?;
        self.selected_prg = r.read_u8()? as usize;
        self.selected_chr = r.read_u8()? as usize;
        self.prev_write = r.read_u8()?;
        self.ppu.mirroring = load_mirroring(r)?;
        load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper(is_jf19: bool) -> JalecoJfMapper {
        let mut prg = Vec::new();
        for i in 0..4u8 {
            let mut bank = [0xFFu8; PRG_16K];
            bank[0] = i;
            prg.push(bank);
        }
        let mut chr = Vec::new();
        for i in 0..4u8 {
            chr.push([i * 0x11; CHR_8K]);
        }
        JalecoJfMapper::new(0, prg, chr, is_jf19)
    }

    #[test]
    fn test_jf17_prg_latch_edge() {
        let mut m = make_mapper(false);
        assert_eq!(m.cpu_read(0x8000), 0);
        assert_eq!(m.cpu_read(0xC000), 3);

        // Bit 7 rising edge latches PRG (write to $8001 where ROM=0xFF avoids conflicts)
        m.cpu_write(0x8001, 0x82);
        assert_eq!(m.cpu_read(0x8000), 2);

        // Bit 7 still high: no new latch
        m.cpu_write(0x8001, 0x81);
        assert_eq!(m.cpu_read(0x8000), 2);

        // Clear then set again: latches
        m.cpu_write(0x8001, 0x01);
        m.cpu_write(0x8001, 0x81);
        assert_eq!(m.cpu_read(0x8000), 1);

        // $C000 stays fixed to last bank
        assert_eq!(m.cpu_read(0xC000), 3);
    }

    #[test]
    fn test_jf17_chr_latch_edge() {
        let mut m = make_mapper(false);
        assert_eq!(m.ppu_read(0x0000), 0);

        m.cpu_write(0x8001, 0x43);
        assert_eq!(m.ppu_read(0x0000), 0x33);

        m.cpu_write(0x8001, 0x42);
        assert_eq!(m.ppu_read(0x0000), 0x33);

        m.cpu_write(0x8001, 0x02);
        m.cpu_write(0x8001, 0x42);
        assert_eq!(m.ppu_read(0x0000), 0x22);
    }

    #[test]
    fn test_jf19_prg_at_c000() {
        let mut m = make_mapper(true);
        assert_eq!(m.cpu_read(0x8000), 0);
        assert_eq!(m.cpu_read(0xC000), 0);

        m.cpu_write(0x8001, 0x82);
        assert_eq!(m.cpu_read(0xC000), 2);
        // $8000 stays fixed to first bank
        assert_eq!(m.cpu_read(0x8000), 0);
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut m = make_mapper(false);
        m.cpu_write(0x8001, 0x82);
        m.cpu_write(0x8001, 0x41);

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);
        let data = w.finish();

        let mut m2 = make_mapper(false);
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.cpu_read(0x8000), m.cpu_read(0x8000));
        assert_eq!(m2.ppu_read(0x0000), m.ppu_read(0x0000));
    }
}
