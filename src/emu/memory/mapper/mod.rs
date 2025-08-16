pub mod mmc1;
pub mod mmc3;
pub mod nrom;
pub mod uxrom;

pub const RESET_TARGET_ADDR: u16 = 0xfffc;
pub const NAMETABLE_ALIGNMENT_BIT: u8 = 0b0000_0001;

pub const MAX_VRAM_ADDR: u16 = 0x4000;

fn mirror_addr(addr: u16) -> u16 {
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
            // $2000-$23FF, $2800-$2BFF -> 0x000-0x3FF
            // $2400-$27FF, $2C00-$2FFF -> 0x400-0x7FF
            (addr & 0x0400) | (addr & 0x03FF)
        }
        NametableMirror::Lower => 0x2000 + (addr % 0x400),
        NametableMirror::Higher => 0x2800 + (addr % 0x400),
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
        // Horizontal mirroring (NESDev-correct)
        // $2000-$23FF, $2800-$2BFF -> 0x000-0x3FF
        // $2400-$27FF, $2C00-$2FFF -> 0x400-0x7FF
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
            0x0400
        );
        assert_eq!(
            mirror_nametable_addr(0x27FF, NametableMirror::Horizontal),
            0x07FF
        );
        assert_eq!(
            mirror_nametable_addr(0x2800, NametableMirror::Horizontal),
            0x0000
        );
        assert_eq!(
            mirror_nametable_addr(0x2BFF, NametableMirror::Horizontal),
            0x03FF
        );
        assert_eq!(
            mirror_nametable_addr(0x2C00, NametableMirror::Horizontal),
            0x0400
        );
        assert_eq!(
            mirror_nametable_addr(0x2FFF, NametableMirror::Horizontal),
            0x07FF
        );

        // Vertical mirroring (NESDev-correct)
        // $2000-$27FF -> 0x000-0x7FF, $2800-$2FFF -> 0x000-0x7FF
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
            0x0800 & 0x07FF
        ); // 0x0000
        assert_eq!(
            mirror_nametable_addr(0x2BFF, NametableMirror::Vertical),
            0x0BFF & 0x07FF
        ); // 0x03FF
        assert_eq!(
            mirror_nametable_addr(0x2C00, NametableMirror::Vertical),
            0x0C00 & 0x07FF
        ); // 0x0400
        assert_eq!(
            mirror_nametable_addr(0x2FFF, NametableMirror::Vertical),
            0x0FFF & 0x07FF
        ); // 0x07FF

        // Lower and Higher mirroring (unchanged)
        assert_eq!(
            mirror_nametable_addr(0x2123, NametableMirror::Lower),
            0x2123
        );
        assert_eq!(
            mirror_nametable_addr(0x2523, NametableMirror::Lower),
            0x2123
        );
        assert_eq!(
            mirror_nametable_addr(0x2923, NametableMirror::Lower),
            0x2123
        );
        assert_eq!(
            mirror_nametable_addr(0x2d23, NametableMirror::Lower),
            0x2123
        );

        assert_eq!(
            mirror_nametable_addr(0x2123, NametableMirror::Higher),
            0x2923
        );
        assert_eq!(
            mirror_nametable_addr(0x2523, NametableMirror::Higher),
            0x2923
        );
        assert_eq!(
            mirror_nametable_addr(0x2923, NametableMirror::Higher),
            0x2923
        );
        assert_eq!(
            mirror_nametable_addr(0x2d23, NametableMirror::Higher),
            0x2923
        );
    }
}
