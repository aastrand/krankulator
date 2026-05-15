use super::super::*;
use super::*;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

/*
AxROM (Mapper 7)

CPU $8000-$FFFF: 32 KB switchable PRG ROM bank

PPU $0000-$1FFF: 8 KB CHR-RAM (non-bankswitched)

Bank switching:
- Single register at $8000-$FFFF: "xxxM xPPP"
- PPP selects 32 KB PRG ROM bank (up to 8 banks = 256KB)
- M selects 1 KB VRAM page for all 4 nametables (single-screen mirroring)

Mirroring:
- Single-screen, mapper-selectable mirroring
- Bit 4 controls which 1KB VRAM page is used for all nametables

Special characteristics:
- No PRG RAM
- CHR-RAM instead of CHR-ROM
- Bus conflicts on some variants (AMROM/AOROM)
*/

const AXROM_PRG_BANK_SIZE: usize = 32 * 1024;
const AXROM_CHR_SIZE: usize = 8 * 1024;
const VRAM_SIZE: u16 = 2 * 1024;

const PRG_ROM_ADDR: usize = 0x8000;

pub struct AxROMMapper {
    _flags: u8,

    _addr_space: Box<[u8; MAX_RAM_SIZE]>,
    addr_space_ptr: *mut u8,

    // PRG ROM banks (32KB each)
    _prg_banks: Vec<[u8; AXROM_PRG_BANK_SIZE]>,
    selected_prg_bank: usize,

    // CHR RAM (8KB, non-bankswitched)
    _chr_ram: Box<[u8; AXROM_CHR_SIZE]>,
    chr_ptr: *mut u8,

    // VRAM (2KB for single-screen mirroring)
    _vram: Box<[u8; VRAM_SIZE as usize]>,
    vrm_ptr: *mut u8,

    // Single-screen mirroring page (0 or 1)
    single_screen_page: u8,

    pub controllers: [controller::Controller; 2],
    palette_ram: [u8; 32],
}

impl AxROMMapper {
    pub fn new(flags: u8, prg_banks: Vec<[u8; AXROM_PRG_BANK_SIZE]>) -> AxROMMapper {
        if prg_banks.is_empty() {
            panic!("AxROM requires at least one PRG bank");
        }

        let mut mem: Box<[u8; MAX_RAM_SIZE]> = Box::new([0; MAX_RAM_SIZE]);

        // Load first PRG bank initially
        mem[PRG_ROM_ADDR..PRG_ROM_ADDR + AXROM_PRG_BANK_SIZE].clone_from_slice(&prg_banks[0]);

        let addr_space_ptr = mem.as_mut_ptr();

        // CHR RAM (writable)
        let mut chr_ram = Box::new([0; AXROM_CHR_SIZE]);
        let chr_ptr = chr_ram.as_mut_ptr();

        // VRAM for single-screen mirroring
        let mut vram = Box::new([0; VRAM_SIZE as usize]);
        let vrm_ptr = vram.as_mut_ptr();

        AxROMMapper {
            _flags: flags,

            _addr_space: mem,
            addr_space_ptr,

            _prg_banks: prg_banks,
            selected_prg_bank: 0,

            _chr_ram: chr_ram,
            chr_ptr,

            _vram: vram,
            vrm_ptr,

            single_screen_page: 0,

            controllers: [controller::Controller::new(), controller::Controller::new()],
            palette_ram: [0x0F; 32],
        }
    }

    fn switch_prg_bank(&mut self, bank: u8) {
        let bank_index = (bank as usize) % self._prg_banks.len();
        if bank_index != self.selected_prg_bank {
            self.selected_prg_bank = bank_index;

            // Update PRG ROM in memory
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self._prg_banks[bank_index].as_ptr(),
                    self.addr_space_ptr.offset(PRG_ROM_ADDR as isize),
                    AXROM_PRG_BANK_SIZE,
                );
            }
        }
    }

    fn switch_single_screen(&mut self, page: u8) {
        self.single_screen_page = page & 1;
    }

    fn get_single_screen_addr(&self, addr: u16) -> u16 {
        // For single-screen mirroring, all nametable addresses map to the same 1KB page
        let page_offset = (self.single_screen_page as u16) * 0x400;
        page_offset + (addr & 0x03FF)
    }
}

impl MemoryMapper for AxROMMapper {
    fn cpu_read(&mut self, addr: u16) -> u8 {
        let addr = mirror_addr(addr);
        unsafe { *self.addr_space_ptr.offset(addr as isize) }
    }

    fn cpu_write(&mut self, addr: u16, value: u8) {
        let addr = super::mirror_addr(addr);
        let page = addr_to_page(addr);

        match page {
            0x0 | 0x10 | 0x60 => unsafe { *self.addr_space_ptr.offset(addr as isize) = value },
            // Bank switching register at $8000-$FFFF
            // Format: xxxM xPPP where PPP = PRG bank, M = single-screen page
            0x80 | 0x90 | 0xa0 | 0xb0 | 0xc0 | 0xd0 | 0xe0 | 0xf0 => {
                // Switch PRG bank (bits 2-0)
                self.switch_prg_bank(value & 0x07);
                // Switch single-screen page (bit 4)
                self.switch_single_screen((value >> 4) & 1);
            }
            _ => { /* Ignore writes to unmapped areas */ }
        }
    }

    fn ppu_read(&self, addr: u16) -> u8 {
        let mut addr = addr;
        let page = addr_to_page(addr);
        if addr >= 0x3F00 && addr < 0x4000 {
            let mut palette_addr = (addr as usize - 0x3F00) % 32;
            if palette_addr & 0x13 == 0x10 {
                palette_addr &= !0x10;
            }
            return self.palette_ram[palette_addr];
        }
        match page {
            // CHR RAM (writable)
            0x0 | 0x10 => unsafe { *self.chr_ptr.offset(addr as _) },
            0x20 => {
                // Single-screen mirroring
                addr = self.get_single_screen_addr(addr);
                unsafe { *self.vrm_ptr.offset(addr as _) }
            }
            0x30 => {
                // Mirror of nametables
                addr = self.get_single_screen_addr(addr) % VRAM_SIZE;
                unsafe { *self.vrm_ptr.offset(addr as _) }
            }
            _ => panic!("Addr {:X} not mapped for ppu_read!", addr),
        }
    }

    fn ppu_copy(&self, addr: u16, dest: *mut u8, size: usize) {
        let mut addr = addr % MAX_VRAM_ADDR;
        let page = addr_to_page(addr);
        match page {
            0x0 | 0x10 => unsafe { std::ptr::copy(self.chr_ptr.offset(addr as _), dest, size) },
            0x20 => {
                addr = self.get_single_screen_addr(addr);
                unsafe { std::ptr::copy(self.vrm_ptr.offset(addr as _), dest, size) }
            }
            0x30 => {
                addr = self.get_single_screen_addr(addr) % VRAM_SIZE;
                unsafe { std::ptr::copy(self.vrm_ptr.offset(addr as _), dest, size) }
            }
            _ => panic!("Addr not mapped for ppu_copy: {:X}", addr),
        }
    }

    fn ppu_write(&mut self, addr: u16, value: u8) {
        let mut addr = addr % MAX_VRAM_ADDR;
        if addr >= 0x3F00 && addr < 0x4000 {
            let mut palette_addr = (addr as usize - 0x3F00) % 32;
            if palette_addr & 0x13 == 0x10 {
                palette_addr &= !0x10;
            }
            self.palette_ram[palette_addr] = value;
            return;
        }
        let page = addr_to_page(addr);
        match page {
            // CHR RAM is writable
            0x0 | 0x10 => unsafe { *self.chr_ptr.offset(addr as _) = value },
            0x20 => {
                // Single-screen mirroring
                addr = self.get_single_screen_addr(addr);
                unsafe { *self.vrm_ptr.offset(addr as _) = value }
            }
            0x30 => {
                // Mirror of nametables
                addr = self.get_single_screen_addr(addr) % VRAM_SIZE;
                unsafe { *self.vrm_ptr.offset(addr as _) = value }
            }
            _ => panic!("Addr not mapped for ppu_write: {:X}", addr),
        }
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
        7
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        let ram = unsafe { std::slice::from_raw_parts(self.addr_space_ptr, MAX_RAM_SIZE) };
        w.write_bytes(ram);
        let chr = unsafe { std::slice::from_raw_parts(self.chr_ptr, AXROM_CHR_SIZE) };
        w.write_bytes(chr);
        let vram = unsafe { std::slice::from_raw_parts(self.vrm_ptr, VRAM_SIZE as usize) };
        w.write_bytes(vram);
        w.write_bytes(&self.palette_ram);
        w.write_u8(self.selected_prg_bank as u8);
        w.write_u8(self.single_screen_page);
        super::save_controllers(w, &self.controllers);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        let ram = unsafe { std::slice::from_raw_parts_mut(self.addr_space_ptr, MAX_RAM_SIZE) };
        r.read_bytes_into(ram)?;
        let chr = unsafe { std::slice::from_raw_parts_mut(self.chr_ptr, AXROM_CHR_SIZE) };
        r.read_bytes_into(chr)?;
        let vram = unsafe { std::slice::from_raw_parts_mut(self.vrm_ptr, VRAM_SIZE as usize) };
        r.read_bytes_into(vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        self.selected_prg_bank = r.read_u8()? as usize;
        self.single_screen_page = r.read_u8()?;
        super::load_controllers(r, &mut self.controllers)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_axrom_prg_bank_switching() {
        let bank1 = [1; AXROM_PRG_BANK_SIZE];
        let bank2 = [2; AXROM_PRG_BANK_SIZE];
        let bank3 = [3; AXROM_PRG_BANK_SIZE];

        let mut mapper: Box<dyn MemoryMapper> =
            Box::new(AxROMMapper::new(0, vec![bank1, bank2, bank3]));

        // Initially bank 0 should be selected
        assert_eq!(mapper.cpu_read(0x8000), 1);
        assert_eq!(mapper.cpu_read(0xFFFF), 1);

        // Switch to bank 1
        mapper.cpu_write(0x8000, 1);
        assert_eq!(mapper.cpu_read(0x8000), 2);
        assert_eq!(mapper.cpu_read(0xFFFF), 2);

        // Switch to bank 2
        mapper.cpu_write(0x8000, 2);
        assert_eq!(mapper.cpu_read(0x8000), 3);
        assert_eq!(mapper.cpu_read(0xFFFF), 3);

        // Test wrapping - bank 3 should wrap to bank 0
        mapper.cpu_write(0x8000, 3);
        assert_eq!(mapper.cpu_read(0x8000), 1);
        assert_eq!(mapper.cpu_read(0xFFFF), 1);
    }

    #[test]
    fn test_axrom_single_screen_mirroring() {
        let bank1 = [0; AXROM_PRG_BANK_SIZE];
        let mut mapper: Box<dyn MemoryMapper> = Box::new(AxROMMapper::new(0, vec![bank1]));

        // Write to nametable $2000
        mapper.ppu_write(0x2000, 0x42);

        // With single-screen mirroring, all nametable addresses should map to the same location
        assert_eq!(mapper.ppu_read(0x2000), 0x42);
        assert_eq!(mapper.ppu_read(0x2400), 0x42); // Different nametable
        assert_eq!(mapper.ppu_read(0x2800), 0x42); // Different nametable
        assert_eq!(mapper.ppu_read(0x2C00), 0x42); // Different nametable

        // Switch to the other single-screen page
        mapper.cpu_write(0x8000, 0x10); // Set bit 4 to switch page

        // Now the same addresses should read different values (new page)
        assert_ne!(mapper.ppu_read(0x2000), 0x42);

        // Write to the new page
        mapper.ppu_write(0x2000, 0x84);
        assert_eq!(mapper.ppu_read(0x2000), 0x84);
        assert_eq!(mapper.ppu_read(0x2400), 0x84);
        assert_eq!(mapper.ppu_read(0x2800), 0x84);
        assert_eq!(mapper.ppu_read(0x2C00), 0x84);

        // Switch back to page 0
        mapper.cpu_write(0x8000, 0x00);
        assert_eq!(mapper.ppu_read(0x2000), 0x42);
    }

    #[test]
    fn test_axrom_chr_ram_write() {
        let bank1 = [0; AXROM_PRG_BANK_SIZE];
        let mut mapper: Box<dyn MemoryMapper> = Box::new(AxROMMapper::new(0, vec![bank1]));

        // CHR RAM should be writable
        mapper.ppu_write(0x0100, 0x55);
        assert_eq!(mapper.ppu_read(0x0100), 0x55);

        // Test another address
        mapper.ppu_write(0x1FFF, 0xAA);
        assert_eq!(mapper.ppu_read(0x1FFF), 0xAA);
    }

    #[test]
    fn test_axrom_combined_bank_and_mirroring() {
        let bank1 = [1; AXROM_PRG_BANK_SIZE];
        let bank2 = [2; AXROM_PRG_BANK_SIZE];

        let mut mapper: Box<dyn MemoryMapper> = Box::new(AxROMMapper::new(0, vec![bank1, bank2]));

        // Test combined PRG bank switch and mirroring control
        // Value 0x11 = PRG bank 1, single-screen page 1
        mapper.cpu_write(0x8000, 0x11);

        // Should be in PRG bank 1
        assert_eq!(mapper.cpu_read(0x8000), 2);

        // Write to nametable and verify single-screen behavior on page 1
        mapper.ppu_write(0x2000, 0x33);
        assert_eq!(mapper.ppu_read(0x2000), 0x33);
        assert_eq!(mapper.ppu_read(0x2400), 0x33);

        // Switch to PRG bank 0, single-screen page 0
        mapper.cpu_write(0x8000, 0x00);

        // Should be in PRG bank 0
        assert_eq!(mapper.cpu_read(0x8000), 1);

        // Single-screen page should have changed
        assert_ne!(mapper.ppu_read(0x2000), 0x33);
    }
}
