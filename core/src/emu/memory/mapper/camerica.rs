use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_SIZE: usize = 8 * 1024;
const BANK_SWITCHABLE_ADDR: usize = 0x8000;
const BANK_FIXED_ADDR: usize = 0xC000;

pub struct CamericaMapper {
    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    _prg_banks: Vec<[u8; PRG_BANK_SIZE]>,
    selected_bank: usize,

    ppu: PpuBus,
    pub controllers: [controller::Controller; 2],
}

impl CamericaMapper {
    pub fn new(flags: u8, prg_banks: Vec<[u8; PRG_BANK_SIZE]>) -> Self {
        if prg_banks.is_empty() {
            panic!("Camerica mapper requires at least one PRG bank");
        }

        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);

        let last_bank = prg_banks.len() - 1;
        mem[BANK_SWITCHABLE_ADDR..BANK_SWITCHABLE_ADDR + PRG_BANK_SIZE]
            .clone_from_slice(&prg_banks[0]);
        mem[BANK_FIXED_ADDR..BANK_FIXED_ADDR + PRG_BANK_SIZE]
            .clone_from_slice(&prg_banks[last_bank]);

        let addr_space_ptr = mem.as_mut_ptr();

        CamericaMapper {
            _addr_space: mem,
            addr_space_ptr,
            _prg_banks: prg_banks,
            selected_bank: 0,
            ppu: PpuBus::new_ram(CHR_SIZE, mirroring_from_flags(flags)),
            controllers: [controller::Controller::new(), controller::Controller::new()],
        }
    }

    fn switch_prg_bank(&mut self, bank: u8) {
        let bank_index = (bank as usize) % self._prg_banks.len();
        if bank_index != self.selected_bank {
            self.selected_bank = bank_index;
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self._prg_banks[bank_index].as_ptr(),
                    self.addr_space_ptr.add(BANK_SWITCHABLE_ADDR),
                    PRG_BANK_SIZE,
                );
            }
        }
    }
}

impl MemoryMapper for CamericaMapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = mirror_addr(addr);
        unsafe { *self.addr_space_ptr.offset(addr as isize) }
    }

    fn cpu_peek(&self, addr: u16) -> u8 {
        let addr = mirror_addr(addr);
        unsafe { *self.addr_space_ptr.offset(addr as isize) }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);

        match page {
            0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
            // $8000-$9FFF: Fire Hawk mirroring register
            0x80 | 0x90 => {
                self.ppu.mirroring = if value & 0x10 != 0 {
                    NametableMirror::Higher
                } else {
                    NametableMirror::Lower
                };
            }
            // $C000-$FFFF: PRG bank select
            0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                self.switch_prg_bank(value & 0x0F);
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
        71
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let ram = unsafe { std::slice::from_raw_parts(self.addr_space_ptr, MAX_RAM_SIZE) };
        w.write_bytes(ram);
        self.ppu.save_state(w);
        w.write_u8(self.selected_bank as u8);
        save_mirroring(w, self.ppu.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let ram = unsafe { std::slice::from_raw_parts_mut(self.addr_space_ptr, MAX_RAM_SIZE) };
        r.read_bytes_into(ram)?;
        self.ppu.load_state(r)?;
        self.selected_bank = r.read_u8()? as usize;
        self.ppu.mirroring = load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bank(fill: u8) -> [u8; PRG_BANK_SIZE] {
        [fill; PRG_BANK_SIZE]
    }

    #[test]
    fn test_prg_bank_switching() {
        let mut m: Box<dyn MemoryMapper> = Box::new(CamericaMapper::new(
            0,
            vec![
                make_bank(0x11),
                make_bank(0x22),
                make_bank(0x33),
                make_bank(0x44),
            ],
        ));

        assert_eq!(m.cpu_read(0x8000), 0x11);
        assert_eq!(m.cpu_read(0xC000), 0x44);

        m.cpu_write(0xC000, 1);
        assert_eq!(m.cpu_read(0x8000), 0x22);
        assert_eq!(m.cpu_read(0xC000), 0x44);

        m.cpu_write(0xC000, 2);
        assert_eq!(m.cpu_read(0x8000), 0x33);
    }

    #[test]
    fn test_bank_register_at_c000_not_8000() {
        let mut m: Box<dyn MemoryMapper> = Box::new(CamericaMapper::new(
            0,
            vec![make_bank(0x11), make_bank(0x22)],
        ));

        // Write to $8000 should NOT change PRG bank (it's the mirroring register)
        m.cpu_write(0x8000, 1);
        assert_eq!(m.cpu_read(0x8000), 0x11);

        // Write to $C000 changes PRG bank
        m.cpu_write(0xC000, 1);
        assert_eq!(m.cpu_read(0x8000), 0x22);
    }

    #[test]
    fn test_fire_hawk_mirroring() {
        let mut m: Box<dyn MemoryMapper> = Box::new(CamericaMapper::new(0, vec![make_bank(0xFF)]));

        // Write to $8000 with bit 4 set => Higher nametable
        m.cpu_write(0x8000, 0x10);
        m.ppu_write(0x2000, 0xAA);
        assert_eq!(m.ppu_read(0x2400), 0xAA);
        assert_eq!(m.ppu_read(0x2800), 0xAA);

        // Write to $8000 with bit 4 clear => Lower nametable
        m.cpu_write(0x8000, 0x00);
        m.ppu_write(0x2000, 0xBB);
        assert_eq!(m.ppu_read(0x2400), 0xBB);
        assert_eq!(m.ppu_read(0x2800), 0xBB);
    }

    #[test]
    fn test_chr_ram() {
        let mut m: Box<dyn MemoryMapper> = Box::new(CamericaMapper::new(0, vec![make_bank(0xFF)]));

        m.ppu_write(0x0100, 0x42);
        assert_eq!(m.ppu_read(0x0100), 0x42);
    }

    #[test]
    fn test_bank_wraps() {
        let mut m: Box<dyn MemoryMapper> = Box::new(CamericaMapper::new(
            0,
            vec![make_bank(0xAA), make_bank(0xBB)],
        ));

        m.cpu_write(0xC000, 2);
        assert_eq!(m.cpu_read(0x8000), 0xAA);

        m.cpu_write(0xC000, 3);
        assert_eq!(m.cpu_read(0x8000), 0xBB);
    }
}
