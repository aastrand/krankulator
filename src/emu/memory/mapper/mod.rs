pub mod axrom;
pub mod cnrom;
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
    // System memory at $0000-$07FF is mirrored at $0800-$0FFF, $1000-$17FF, and $1800-$1FFF
    // - attempting to access memory at, for example, $0173 is the same as accessing memory at $0973, $1173, or $1973.
    if addr < 0x2000 {
        addr % 0x800
    } else if addr < 0x4000 {
        0x2000 + (addr % 0x8)
    } else {
        addr
    }
}

#[derive(Copy, Clone)]
pub enum NametableMirror {
    Lower,
    Higher,
    Vertical,
    Horizontal,
}

fn mirror_nametable_addr(addr: u16, mirroring: NametableMirror) -> u16 {
    // NESDev-correct mirroring logic
    let vram_index = match mirroring {
        NametableMirror::Vertical => {
            // $2000-$27FF -> 0x000-0x7FF, $2800-$2FFF -> 0x000-0x7FF
            addr & 0x07FF
        }
        NametableMirror::Horizontal => {
            // $2000=$2400 -> 0x000-0x3FF, $2800=$2C00 -> 0x400-0x7FF
            ((addr >> 1) & 0x0400) | (addr & 0x03FF)
        }
        NametableMirror::Lower => addr & 0x03FF,
        NametableMirror::Higher => 0x0400 | (addr & 0x03FF),
    };
    vram_index
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
}
