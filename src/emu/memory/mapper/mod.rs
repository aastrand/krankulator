pub mod axrom;
pub mod bnrom;
pub mod cnrom;
pub mod gxrom;
pub mod mmc1;
pub mod mmc3;
pub mod nrom;
pub mod uxrom;

pub const RESET_TARGET_ADDR: u16 = 0xfffc;
pub const NAMETABLE_ALIGNMENT_BIT: u8 = 0b0000_0001;

use crate::emu::io::controller::Controller;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

fn save_controllers(w: &mut SavestateWriter, controllers: &[Controller; 2]) {
    w.write_u8(controllers[0].save_status());
    w.write_u64(controllers[0].save_polls());
    w.write_u8(controllers[1].save_status());
    w.write_u64(controllers[1].save_polls());
}

fn load_controllers(
    r: &mut SavestateReader,
    controllers: &mut [Controller; 2],
) -> std::io::Result<()> {
    controllers[0].load_status(r.read_u8()?);
    controllers[0].load_polls(r.read_u64()?);
    controllers[1].load_status(r.read_u8()?);
    controllers[1].load_polls(r.read_u64()?);
    Ok(())
}

fn save_mirroring(w: &mut SavestateWriter, m: NametableMirror) {
    w.write_u8(match m {
        NametableMirror::Lower => 0,
        NametableMirror::Higher => 1,
        NametableMirror::Vertical => 2,
        NametableMirror::Horizontal => 3,
    });
}

fn load_mirroring(r: &mut SavestateReader) -> std::io::Result<NametableMirror> {
    Ok(match r.read_u8()? {
        0 => NametableMirror::Lower,
        1 => NametableMirror::Higher,
        2 => NametableMirror::Vertical,
        3 => NametableMirror::Horizontal,
        v => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("bad mirroring value: {}", v),
            ))
        }
    })
}

pub const MAX_VRAM_ADDR: u16 = 0x4000;

pub fn mirror_addr(addr: u16) -> u16 {
    if addr < 0x2000 {
        addr % 0x800
    } else if addr < 0x4000 {
        0x2000 + (addr % 0x8)
    } else {
        addr
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum NametableMirror {
    Lower,
    Higher,
    Vertical,
    Horizontal,
}

fn mirror_nametable_addr(addr: u16, mirroring: NametableMirror) -> u16 {
    match mirroring {
        NametableMirror::Vertical => addr & 0x07FF,
        NametableMirror::Horizontal => ((addr >> 1) & 0x0400) | (addr & 0x03FF),
        NametableMirror::Lower => addr & 0x03FF,
        NametableMirror::Higher => 0x0400 | (addr & 0x03FF),
    }
}

const VRAM_SIZE: u16 = 2 * 1024;

pub struct PpuBus {
    chr: Vec<u8>,
    chr_writable: bool,
    vram: Box<[u8; VRAM_SIZE as usize]>,
    pub mirroring: NametableMirror,
    palette_ram: [u8; 32],
}

impl PpuBus {
    pub fn new_ram(chr_size: usize, mirroring: NametableMirror) -> Self {
        PpuBus {
            chr: vec![0; chr_size],
            chr_writable: true,
            vram: Box::new([0; VRAM_SIZE as usize]),
            mirroring,
            palette_ram: [0x0F; 32],
        }
    }

    pub fn new_rom(chr_data: &[u8], mirroring: NametableMirror) -> Self {
        PpuBus {
            chr: chr_data.to_vec(),
            chr_writable: false,
            vram: Box::new([0; VRAM_SIZE as usize]),
            mirroring,
            palette_ram: [0x0F; 32],
        }
    }

    pub fn switch_chr_bank(&mut self, banks: &[[u8; 8192]], bank_index: usize) {
        let src = &banks[bank_index];
        self.chr[..8192].copy_from_slice(src);
    }

    fn resolve_nametable_addr(&self, addr: u16) -> u16 {
        mirror_nametable_addr(addr, self.mirroring) % VRAM_SIZE
    }

    pub fn read(&self, addr: u16) -> u8 {
        let addr = addr % MAX_VRAM_ADDR;
        if addr >= 0x3F00 && addr < 0x4000 {
            let mut idx = (addr as usize - 0x3F00) % 32;
            if idx & 0x13 == 0x10 {
                idx &= !0x10;
            }
            return self.palette_ram[idx];
        }
        let page = super::addr_to_page(addr);
        match page {
            0x0 | 0x10 => self.chr[addr as usize],
            // $3000-$3EFF mirrors $2000-$2EFF
            0x20 | 0x30 => {
                let a = self.resolve_nametable_addr(addr);
                self.vram[a as usize]
            }
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        let addr = addr % MAX_VRAM_ADDR;
        if addr >= 0x3F00 && addr < 0x4000 {
            let mut idx = (addr as usize - 0x3F00) % 32;
            if idx & 0x13 == 0x10 {
                idx &= !0x10;
            }
            self.palette_ram[idx] = value;
            return;
        }
        let page = super::addr_to_page(addr);
        match page {
            0x0 | 0x10 => {
                if self.chr_writable {
                    self.chr[addr as usize] = value;
                }
            }
            // $3000-$3EFF mirrors $2000-$2EFF
            0x20 | 0x30 => {
                let a = self.resolve_nametable_addr(addr);
                self.vram[a as usize] = value;
            }
            _ => {}
        }
    }

    pub fn copy(&self, addr: u16, dest: *mut u8, size: usize) {
        let addr = addr % MAX_VRAM_ADDR;
        let page = super::addr_to_page(addr);
        match page {
            0x0 | 0x10 => unsafe {
                std::ptr::copy(self.chr.as_ptr().offset(addr as _), dest, size)
            },
            // $3000-$3EFF mirrors $2000-$2EFF
            0x20 | 0x30 => {
                let a = self.resolve_nametable_addr(addr);
                unsafe { std::ptr::copy(self.vram.as_ptr().offset(a as _), dest, size) }
            }
            _ => {}
        }
    }

    pub fn save_state(&self, w: &mut SavestateWriter) {
        w.write_bytes(&self.chr);
        w.write_bytes(&*self.vram);
        w.write_bytes(&self.palette_ram);
    }

    pub fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        r.read_bytes_into(&mut self.chr)?;
        r.read_bytes_into(&mut *self.vram)?;
        r.read_bytes_into(&mut self.palette_ram)?;
        Ok(())
    }
}

pub fn mirroring_from_flags(flags: u8) -> NametableMirror {
    if flags & NAMETABLE_ALIGNMENT_BIT == 1 {
        NametableMirror::Vertical
    } else {
        NametableMirror::Horizontal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_addr_mirroring() {
        assert_eq!(mirror_addr(0x973), 0x173);
        assert_eq!(mirror_addr(0x200C), 0x2004);
        assert_eq!(mirror_addr(0x3002), 0x2002);
        assert_eq!(mirror_addr(0x8000), 0x8000);
    }

    #[test]
    fn test_mirror_nametable_addr() {
        // Horizontal mirroring: $2000=$2400, $2800=$2C00
        assert_eq!(
            mirror_nametable_addr(0x2000, NametableMirror::Horizontal),
            0x0000
        );
        assert_eq!(
            mirror_nametable_addr(0x23FF, NametableMirror::Horizontal),
            0x03FF
        );
        assert_eq!(
            mirror_nametable_addr(0x2400, NametableMirror::Horizontal),
            0x0000
        );
        assert_eq!(
            mirror_nametable_addr(0x27FF, NametableMirror::Horizontal),
            0x03FF
        );
        assert_eq!(
            mirror_nametable_addr(0x2800, NametableMirror::Horizontal),
            0x0400
        );
        assert_eq!(
            mirror_nametable_addr(0x2BFF, NametableMirror::Horizontal),
            0x07FF
        );
        assert_eq!(
            mirror_nametable_addr(0x2C00, NametableMirror::Horizontal),
            0x0400
        );
        assert_eq!(
            mirror_nametable_addr(0x2FFF, NametableMirror::Horizontal),
            0x07FF
        );

        // Vertical mirroring: $2000=$2800, $2400=$2C00
        assert_eq!(
            mirror_nametable_addr(0x2000, NametableMirror::Vertical),
            0x0000
        );
        assert_eq!(
            mirror_nametable_addr(0x23FF, NametableMirror::Vertical),
            0x03FF
        );
        assert_eq!(
            mirror_nametable_addr(0x2400, NametableMirror::Vertical),
            0x0400
        );
        assert_eq!(
            mirror_nametable_addr(0x27FF, NametableMirror::Vertical),
            0x07FF
        );
        assert_eq!(
            mirror_nametable_addr(0x2800, NametableMirror::Vertical),
            0x0000
        );
        assert_eq!(
            mirror_nametable_addr(0x2BFF, NametableMirror::Vertical),
            0x03FF
        );
        assert_eq!(
            mirror_nametable_addr(0x2C00, NametableMirror::Vertical),
            0x0400
        );
        assert_eq!(
            mirror_nametable_addr(0x2FFF, NametableMirror::Vertical),
            0x07FF
        );

        // Lower: all nametables map to first physical page (0x000-0x3FF)
        assert_eq!(
            mirror_nametable_addr(0x2123, NametableMirror::Lower),
            0x0123
        );
        assert_eq!(
            mirror_nametable_addr(0x2523, NametableMirror::Lower),
            0x0123
        );
        assert_eq!(
            mirror_nametable_addr(0x2923, NametableMirror::Lower),
            0x0123
        );
        assert_eq!(
            mirror_nametable_addr(0x2D23, NametableMirror::Lower),
            0x0123
        );

        // Higher: all nametables map to second physical page (0x400-0x7FF)
        assert_eq!(
            mirror_nametable_addr(0x2123, NametableMirror::Higher),
            0x0523
        );
        assert_eq!(
            mirror_nametable_addr(0x2523, NametableMirror::Higher),
            0x0523
        );
        assert_eq!(
            mirror_nametable_addr(0x2923, NametableMirror::Higher),
            0x0523
        );
        assert_eq!(
            mirror_nametable_addr(0x2D23, NametableMirror::Higher),
            0x0523
        );
    }

    #[test]
    fn test_ppubus_3000_mirrors_2000() {
        let mut ppu = PpuBus::new_ram(8192, NametableMirror::Horizontal);

        // Write via $2000, read back via $3000 mirror
        ppu.write(0x2042, 0xAB);
        assert_eq!(ppu.read(0x3042), 0xAB);

        // Write via $3000 mirror, read back via $2000
        ppu.write(0x3100, 0xCD);
        assert_eq!(ppu.read(0x2100), 0xCD);
    }

    #[test]
    fn test_ppubus_read_wraps_above_4000() {
        let mut ppu = PpuBus::new_ram(8192, NametableMirror::Horizontal);

        ppu.write(0x0010, 0x77);
        // $4010 should wrap to $0010
        assert_eq!(ppu.read(0x4010), 0x77);
    }
}
