use super::*;
use crate::emu::io::controller;
use crate::emu::memory::{addr_to_page, MemoryMapper, MAX_RAM_SIZE};
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_8K: usize = 8 * 1024;
const CHR_4K: usize = 4 * 1024;

pub struct Vrc1Mapper {
    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    prg_banks: Vec<[u8; PRG_8K]>,
    chr_banks_4k: Vec<[u8; CHR_4K]>,

    prg_select: [u8; 3],
    chr_lo: u8,
    chr_hi: u8,
    chr_lo_high_bit: u8,
    chr_hi_high_bit: u8,

    ppu: PpuBus,
    pub controllers: [controller::Controller; 2],
}

impl Vrc1Mapper {
    pub fn new(flags: u8, prg_banks_16k: Vec<[u8; 16384]>, chr_banks_8k: Vec<[u8; 8192]>) -> Self {
        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);

        let mut prg_banks = Vec::new();
        for bank in &prg_banks_16k {
            prg_banks.push(<[u8; PRG_8K]>::try_from(&bank[..PRG_8K]).unwrap());
            prg_banks.push(<[u8; PRG_8K]>::try_from(&bank[PRG_8K..]).unwrap());
        }

        let mut chr_banks_4k = Vec::new();
        for bank in &chr_banks_8k {
            chr_banks_4k.push(<[u8; CHR_4K]>::try_from(&bank[..CHR_4K]).unwrap());
            chr_banks_4k.push(<[u8; CHR_4K]>::try_from(&bank[CHR_4K..]).unwrap());
        }

        if !prg_banks.is_empty() {
            let last = prg_banks.len() - 1;
            mem[0x8000..0xA000].copy_from_slice(&prg_banks[0]);
            mem[0xA000..0xC000].copy_from_slice(&prg_banks[1.min(last)]);
            mem[0xC000..0xE000].copy_from_slice(&prg_banks[2.min(last)]);
            mem[0xE000..0x10000].copy_from_slice(&prg_banks[last]);
        }

        let addr_space_ptr = mem.as_mut_ptr();
        let mirroring = mirroring_from_flags(flags);

        let ppu = if chr_banks_8k.is_empty() {
            PpuBus::new_ram(8192, mirroring)
        } else {
            PpuBus::new_rom(&chr_banks_8k[0], mirroring)
        };

        Vrc1Mapper {
            _addr_space: mem,
            addr_space_ptr,
            prg_banks,
            chr_banks_4k,
            prg_select: [0, 1, 2],
            chr_lo: 0,
            chr_hi: 0,
            chr_lo_high_bit: 0,
            chr_hi_high_bit: 0,
            ppu,
            controllers: [controller::Controller::new(), controller::Controller::new()],
        }
    }

    fn switch_prg_8k(&mut self, slot: usize, bank: u8) {
        let bank_index = bank as usize % self.prg_banks.len().max(1);
        self.prg_select[slot] = bank;
        let base = 0x8000 + slot * PRG_8K;
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.prg_banks[bank_index].as_ptr(),
                self.addr_space_ptr.add(base),
                PRG_8K,
            );
        }
    }

    fn rebuild_chr(&mut self) {
        if self.chr_banks_4k.is_empty() {
            return;
        }
        let lo_idx = ((self.chr_lo_high_bit << 4) | self.chr_lo) as usize % self.chr_banks_4k.len();
        let hi_idx = ((self.chr_hi_high_bit << 4) | self.chr_hi) as usize % self.chr_banks_4k.len();
        let mut combined = [0u8; 8192];
        combined[..CHR_4K].copy_from_slice(&self.chr_banks_4k[lo_idx]);
        combined[CHR_4K..].copy_from_slice(&self.chr_banks_4k[hi_idx]);
        self.ppu.switch_chr_bank(&[combined], 0);
    }
}

impl MemoryMapper for Vrc1Mapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = mirror_addr(addr);
        unsafe { *self.addr_space_ptr.offset(addr as isize) }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let addr = mirror_addr(addr);
        let page = addr_to_page(addr);

        match page {
            0x0 | 0x10 | 0x60 | 0x70 => unsafe {
                *self.addr_space_ptr.offset(addr as isize) = value
            },
            0x80 => self.switch_prg_8k(0, value & 0x0F),
            0x90 => {
                self.ppu.mirroring = if value & 1 != 0 {
                    NametableMirror::Horizontal
                } else {
                    NametableMirror::Vertical
                };
                self.chr_lo_high_bit = (value >> 1) & 1;
                self.chr_hi_high_bit = (value >> 2) & 1;
                self.rebuild_chr();
            }
            0xa0 => self.switch_prg_8k(1, value & 0x0F),
            0xc0 => self.switch_prg_8k(2, value & 0x0F),
            0xe0 => {
                self.chr_lo = value & 0x0F;
                self.rebuild_chr();
            }
            0xf0 => {
                self.chr_hi = value & 0x0F;
                self.rebuild_chr();
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
        false
    }

    fn mapper_id(&self) -> u8 {
        75
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let ram = unsafe { std::slice::from_raw_parts(self.addr_space_ptr, MAX_RAM_SIZE) };
        w.write_bytes(ram);
        self.ppu.save_state(w);
        for &b in &self.prg_select {
            w.write_u8(b);
        }
        w.write_u8(self.chr_lo);
        w.write_u8(self.chr_hi);
        w.write_u8(self.chr_lo_high_bit);
        w.write_u8(self.chr_hi_high_bit);
        save_mirroring(w, self.ppu.mirroring);
        save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let ram = unsafe { std::slice::from_raw_parts_mut(self.addr_space_ptr, MAX_RAM_SIZE) };
        r.read_bytes_into(ram)?;
        self.ppu.load_state(r)?;
        for b in &mut self.prg_select {
            *b = r.read_u8()?;
        }
        self.chr_lo = r.read_u8()?;
        self.chr_hi = r.read_u8()?;
        self.chr_lo_high_bit = r.read_u8()?;
        self.chr_hi_high_bit = r.read_u8()?;
        self.ppu.mirroring = load_mirroring(r)?;
        load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mapper(num_prg_16k: usize, num_chr_8k: usize) -> Box<dyn MemoryMapper> {
        let mut prg = Vec::new();
        for i in 0..num_prg_16k {
            let mut bank = [0xFFu8; 16384];
            bank[0] = (i * 2) as u8;
            bank[PRG_8K] = (i * 2 + 1) as u8;
            prg.push(bank);
        }
        let mut chr = Vec::new();
        for i in 0..num_chr_8k {
            let mut bank = [0u8; 8192];
            bank[0] = (i * 2) as u8;
            bank[CHR_4K] = (i * 2 + 1) as u8;
            chr.push(bank);
        }
        Box::new(Vrc1Mapper::new(0, prg, chr))
    }

    #[test]
    fn test_prg_banking() {
        let mut m = make_mapper(4, 1); // 8 x 8KB PRG banks

        assert_eq!(m.cpu_read(0x8000), 0);
        assert_eq!(m.cpu_read(0xA000), 1);
        assert_eq!(m.cpu_read(0xC000), 2);
        assert_eq!(m.cpu_read(0xE000), 7); // last bank fixed

        m.cpu_write(0x8000, 5);
        assert_eq!(m.cpu_read(0x8000), 5);

        m.cpu_write(0xA000, 3);
        assert_eq!(m.cpu_read(0xA000), 3);

        m.cpu_write(0xC000, 6);
        assert_eq!(m.cpu_read(0xC000), 6);

        // $E000 stays fixed
        assert_eq!(m.cpu_read(0xE000), 7);
    }

    #[test]
    fn test_chr_banking() {
        let mut m = make_mapper(2, 8); // 16 x 4KB CHR banks

        // $9000 bit 1 = CHR0 high bit, bit 2 = CHR1 high bit
        // $E000 low 4 = CHR0 low bits, $F000 low 4 = CHR1 low bits
        m.cpu_write(0xE000, 3); // CHR0 = 4KB bank 3
        assert_eq!(m.ppu_read(0x0000), 3);

        m.cpu_write(0xF000, 5); // CHR1 = 5
        assert_eq!(m.ppu_read(0x1000), 5);

        // Set high bits via $9000
        m.cpu_write(0x9000, 0x02); // bit 1 set = CHR0 high bit
                                   // CHR0 = (1 << 4) | 3 = 19, 4KB bank 19 % 16 = 3 (wraps)
                                   // With 8 x 8KB = 16 x 4KB, bank 19 % 16 = 3
        assert_eq!(m.ppu_read(0x0000), 3);
    }

    #[test]
    fn test_mirroring() {
        let mut m = make_mapper(2, 1);

        // Default: vertical (flags=0, bit 0 = 0 → H, but VRC1 overrides)
        // Actually flags=0 → horizontal from mirroring_from_flags

        // Write $9000 bit 0 = 0 → vertical
        m.cpu_write(0x9000, 0x00);
        m.ppu_write(0x2000, 0xAA);
        assert_eq!(m.ppu_read(0x2800), 0xAA); // vertical mirror

        // Write $9000 bit 0 = 1 → horizontal
        m.cpu_write(0x9000, 0x01);
        m.ppu_write(0x2000, 0xBB);
        assert_eq!(m.ppu_read(0x2400), 0xBB); // horizontal mirror
    }

    #[test]
    fn test_savestate_roundtrip() {
        let mut m = make_mapper(4, 4);
        m.cpu_write(0x8000, 3);
        m.cpu_write(0xE000, 5);
        m.cpu_write(0x9000, 0x06); // CHR high bits + horizontal

        let mut w = SavestateWriter::new();
        m.save_state(&mut w);
        let data = w.finish();

        let mut m2 = make_mapper(4, 4);
        let mut r = SavestateReader::new(&data).unwrap();
        m2.load_state(&mut r).unwrap();

        assert_eq!(m2.cpu_read(0x8000), m.cpu_read(0x8000));
        assert_eq!(m2.ppu_read(0x0000), m.ppu_read(0x0000));
    }
}
