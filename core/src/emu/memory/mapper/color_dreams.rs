use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

const PRG_32K: usize = 32 * 1024;

pub struct ColorDreamsMapper {
    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    prg_banks: Vec<[u8; PRG_32K]>,
    chr_banks: Vec<[u8; 8192]>,

    prg_bank: usize,
    chr_bank: usize,

    ppu: PpuBus,
    pub controllers: [controller::Controller; 2],
}

impl ColorDreamsMapper {
    pub fn new(flags: u8, prg_banks: Vec<[u8; PRG_32K]>, chr_banks: Vec<[u8; 8192]>) -> Self {
        if prg_banks.is_empty() {
            panic!("Color Dreams requires at least one PRG bank");
        }

        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);
        let addr_space_ptr = mem.as_mut_ptr();

        let mirroring = mirroring_from_flags(flags);

        let mut mapper = ColorDreamsMapper {
            _addr_space: mem,
            addr_space_ptr,
            prg_banks,
            chr_banks,
            prg_bank: 0,
            chr_bank: 0,
            ppu: PpuBus::new_rom(&[0; 8192], mirroring),
            controllers: [controller::Controller::new(), controller::Controller::new()],
        };

        mapper.sync_prg();
        mapper.sync_chr();

        mapper
    }

    fn sync_prg(&mut self) {
        let bank = self.prg_bank % self.prg_banks.len();
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.prg_banks[bank].as_ptr(),
                self.addr_space_ptr.add(0x8000),
                PRG_32K,
            );
        }
    }

    fn sync_chr(&mut self) {
        if !self.chr_banks.is_empty() {
            let bank = self.chr_bank % self.chr_banks.len();
            self.ppu.switch_chr_bank(&self.chr_banks, bank);
        }
    }
}

impl MemoryMapper for ColorDreamsMapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = mirror_addr(addr);
        unsafe { *self.addr_space_ptr.offset(addr as isize) }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);

        match page {
            0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
            0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                let bus_conflict = unsafe { *self.addr_space_ptr.offset(addr as isize) };
                let value = value & bus_conflict;
                self.prg_bank = (value & 0x03) as usize;
                self.chr_bank = ((value >> 4) & 0x0F) as usize;
                self.sync_prg();
                self.sync_chr();
            }
            _ => {}
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        self.ppu.read(addr)
    }

    fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        self.ppu.copy(addr, dest, size);
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        self.ppu.write(addr, value);
    }

    fn code_start(&mut self) -> u16 {
        ((self.cpu_read(super::RESET_TARGET_ADDR + 1) as u16) << 8) as u16
            + self.cpu_read(super::RESET_TARGET_ADDR) as u16
    }

    fn controllers(&mut self) -> &mut [controller::Controller; 2] {
        &mut self.controllers
    }

    fn poll_irq(&mut self) -> bool {
        false
    }

    fn mapper_id(&self) -> u8 {
        11
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let ram = unsafe { std::slice::from_raw_parts(self.addr_space_ptr, MAX_RAM_SIZE) };
        w.write_bytes(ram);
        self.ppu.save_state(w);
        w.write_u8(self.prg_bank as u8);
        w.write_u8(self.chr_bank as u8);
        save_mirroring(w, self.ppu.mirroring);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let ram = unsafe { std::slice::from_raw_parts_mut(self.addr_space_ptr, MAX_RAM_SIZE) };
        r.read_bytes_into(ram)?;
        self.ppu.load_state(r)?;
        self.prg_bank = r.read_u8()? as usize;
        self.chr_bank = r.read_u8()? as usize;
        self.ppu.mirroring = load_mirroring(r)?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_prg(fill: u8) -> [u8; PRG_32K] {
        [fill; PRG_32K]
    }

    fn make_chr(fill: u8) -> [u8; 8192] {
        [fill; 8192]
    }

    #[test]
    fn test_prg_bank_switching() {
        let prg = vec![
            make_prg(0xFF),
            make_prg(0xBB),
            make_prg(0xCC),
            make_prg(0xDD),
        ];
        let chr = vec![make_chr(0)];
        let mut m: Box<dyn MemoryMapper> = Box::new(ColorDreamsMapper::new(0, prg, chr));

        assert_eq!(m.cpu_read(0x8000), 0xFF);
        // 0x01 & 0xFF = 0x01 → PRG bank 1
        m.cpu_write(0x8000, 0x01);
        assert_eq!(m.cpu_read(0x8000), 0xBB);
    }

    #[test]
    fn test_chr_bank_switching() {
        let prg = vec![make_prg(0xFF)];
        let chr = vec![make_chr(0x11), make_chr(0x22), make_chr(0x33)];
        let mut m: Box<dyn MemoryMapper> = Box::new(ColorDreamsMapper::new(0, prg, chr));

        assert_eq!(m.ppu_read(0x0000), 0x11);
        // 0x10 & 0xFF = 0x10 → CHR bank 1
        m.cpu_write(0x8000, 0x10);
        assert_eq!(m.ppu_read(0x0000), 0x22);
        // 0x20 & 0xFF = 0x20 → CHR bank 2
        m.cpu_write(0x8000, 0x20);
        assert_eq!(m.ppu_read(0x0000), 0x33);
    }

    #[test]
    fn test_bus_conflicts() {
        let mut prg = make_prg(0x00);
        prg[0] = 0x01; // only bit 0 set at $8000
        let chr = vec![make_chr(0x11), make_chr(0x22)];
        let mut m: Box<dyn MemoryMapper> = Box::new(ColorDreamsMapper::new(0, vec![prg], chr));

        // Write 0x13 but ROM has 0x01, AND = 0x01 → PRG bank 1, CHR bank 0
        m.cpu_write(0x8000, 0x13);
        assert_eq!(m.ppu_read(0x0000), 0x11);
    }

    #[test]
    fn test_mirroring_from_header() {
        let prg = vec![make_prg(0xFF)];
        let chr = vec![make_chr(0)];
        let mut m: Box<dyn MemoryMapper> = Box::new(ColorDreamsMapper::new(0x01, prg, chr));

        // Vertical mirroring: $2000 mirrors to $2800
        m.ppu_write(0x2000, 0x42);
        assert_eq!(m.ppu_read(0x2800), 0x42);
    }
}
