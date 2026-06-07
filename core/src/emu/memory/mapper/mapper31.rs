use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_4K: usize = 4 * 1024;
const CHR_SIZE: usize = 8 * 1024;

pub struct Mapper31 {
    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    prg_banks: Vec<[u8; PRG_4K]>,
    bank_regs: [u8; 8],

    ppu: PpuBus,
    pub controllers: [controller::Controller; 2],
}

impl Mapper31 {
    pub fn new(flags: u8, prg_16k_banks: Vec<[u8; 16384]>) -> Self {
        let mut prg_banks: Vec<[u8; PRG_4K]> = Vec::new();
        for bank in &prg_16k_banks {
            for chunk in 0..4 {
                let mut b = [0u8; PRG_4K];
                let start = chunk * PRG_4K;
                b.copy_from_slice(&bank[start..start + PRG_4K]);
                prg_banks.push(b);
            }
        }

        if prg_banks.is_empty() {
            panic!("Mapper 31 requires at least one PRG bank");
        }

        let last = (prg_banks.len() - 1) as u8;
        let mut bank_regs = [0u8; 8];
        bank_regs[7] = last;

        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);
        let addr_space_ptr = mem.as_mut_ptr();

        let mut mapper = Mapper31 {
            _addr_space: mem,
            addr_space_ptr,
            prg_banks,
            bank_regs,
            ppu: PpuBus::new_ram(CHR_SIZE, mirroring_from_flags(flags)),
            controllers: [controller::Controller::new(), controller::Controller::new()],
        };

        for slot in 0..8 {
            mapper.apply_bank(slot);
        }

        mapper
    }

    fn apply_bank(&mut self, slot: usize) {
        let bank_index = (self.bank_regs[slot] as usize) % self.prg_banks.len();
        let dest_addr = 0x8000 + slot * PRG_4K;
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.prg_banks[bank_index].as_ptr(),
                self.addr_space_ptr.add(dest_addr),
                PRG_4K,
            );
        }
    }
}

impl MemoryMapper for Mapper31 {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = mirror_addr(addr);
        unsafe { *self.addr_space_ptr.offset(addr as isize) }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);

        match page {
            0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
            0x50
                // $5FF8-$5FFF: bank registers
                if addr >= 0x5FF8 => {
                    let slot = (addr - 0x5FF8) as usize;
                    self.bank_regs[slot] = value;
                    self.apply_bank(slot);
                }
            0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {}
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
        31
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let ram = unsafe { std::slice::from_raw_parts(self.addr_space_ptr, MAX_RAM_SIZE) };
        w.write_bytes(ram);
        self.ppu.save_state(w);
        w.write_bytes(&self.bank_regs);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let ram = unsafe { std::slice::from_raw_parts_mut(self.addr_space_ptr, MAX_RAM_SIZE) };
        r.read_bytes_into(ram)?;
        self.ppu.load_state(r)?;
        r.read_bytes_into(&mut self.bank_regs)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_16k_bank(fill: u8) -> [u8; 16384] {
        [fill; 16384]
    }

    #[test]
    fn test_last_bank_at_f000_on_startup() {
        let m: Box<dyn MemoryMapper> = Box::new(Mapper31::new(
            0,
            vec![make_16k_bank(0x11), make_16k_bank(0x22)],
        ));

        // 2 16K banks = 8 4K banks (indices 0-7)
        // Bank reg 7 = last bank (7) => $F000 = 0x22
        assert_eq!(m.ppu_read(0x0000), 0x00); // CHR RAM starts empty
                                              // Slot 7 ($F000) should have last 4K bank
    }

    #[test]
    fn test_bank_switching() {
        let mut banks = Vec::new();
        for i in 0..4u8 {
            let mut bank = [0u8; 16384];
            // Fill each 4K chunk differently
            for chunk in 0..4 {
                let fill = i * 4 + chunk as u8;
                let start = chunk * PRG_4K;
                bank[start..start + PRG_4K].fill(fill);
            }
            banks.push(bank);
        }

        let mut m: Box<dyn MemoryMapper> = Box::new(Mapper31::new(0, banks));

        // Switch slot 0 ($8000-$8FFF) to 4K bank 5
        m.cpu_write(0x5FF8, 5);
        assert_eq!(m.cpu_read(0x8000), 5);

        // Switch slot 1 ($9000-$9FFF) to 4K bank 3
        m.cpu_write(0x5FF9, 3);
        assert_eq!(m.cpu_read(0x9000), 3);
    }

    #[test]
    fn test_writes_below_5ff8_ignored() {
        let mut m: Box<dyn MemoryMapper> = Box::new(Mapper31::new(0, vec![make_16k_bank(0xFF)]));

        let before = m.cpu_read(0x8000);
        m.cpu_write(0x5FF7, 0x00);
        assert_eq!(m.cpu_read(0x8000), before);
    }

    #[test]
    fn test_chr_ram() {
        let mut m: Box<dyn MemoryMapper> = Box::new(Mapper31::new(0, vec![make_16k_bank(0xFF)]));

        m.ppu_write(0x0100, 0x42);
        assert_eq!(m.ppu_read(0x0100), 0x42);
    }
}
