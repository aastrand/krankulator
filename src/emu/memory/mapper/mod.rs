pub mod mmc1;
pub mod nrom;

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
    Horizontal
}

fn mirror_nametable_addr(addr: u16, mirroring: NametableMirror) -> u16 {
    match mirroring {
        NametableMirror::Horizontal => {
            match addr & 0xff00 {
                0x2400 | 0x2500 | 0x2600 | 0x2700 | 0x2c00 | 0x2d00 | 0x200e | 0x2f00 => addr - 0x400,
                _ => addr,
            }
        }
        NametableMirror::Vertical => {
            match addr & 0xff00 {
                0x2800 | 0x2900 | 0x2a00 | 0x2b00 | 0x2c00 | 0x2d00 | 0x2e00 | 0x2f00 => addr - 0x800,
                _ => addr,
            }
        }
        NametableMirror::Lower => {
            0x2000 + (addr % 0x400)
        }
        NametableMirror::Higher => {
            0x2800 + (addr % 0x400)
        }
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
        assert_eq!(mirror_nametable_addr(0x2123, NametableMirror::Horizontal), 0x2123);
        assert_eq!(mirror_nametable_addr(0x2523, NametableMirror::Horizontal), 0x2123);
        assert_eq!(mirror_nametable_addr(0x2823, NametableMirror::Horizontal), 0x2823);
        assert_eq!(mirror_nametable_addr(0x2c23, NametableMirror::Horizontal), 0x2823);

        assert_eq!(mirror_nametable_addr(0x2123, NametableMirror::Vertical), 0x2123);
        assert_eq!(mirror_nametable_addr(0x2523, NametableMirror::Vertical), 0x2523);
        assert_eq!(mirror_nametable_addr(0x2923, NametableMirror::Vertical), 0x2123);
        assert_eq!(mirror_nametable_addr(0x2c23, NametableMirror::Vertical), 0x2423);

        assert_eq!(mirror_nametable_addr(0x2123, NametableMirror::Lower), 0x2123);
        assert_eq!(mirror_nametable_addr(0x2523, NametableMirror::Lower), 0x2123);
        assert_eq!(mirror_nametable_addr(0x2923, NametableMirror::Lower), 0x2123);
        assert_eq!(mirror_nametable_addr(0x2d23, NametableMirror::Lower), 0x2123);

        assert_eq!(mirror_nametable_addr(0x2123, NametableMirror::Higher), 0x2923);
        assert_eq!(mirror_nametable_addr(0x2523, NametableMirror::Higher), 0x2923);
        assert_eq!(mirror_nametable_addr(0x2923, NametableMirror::Higher), 0x2923);
        assert_eq!(mirror_nametable_addr(0x2d23, NametableMirror::Higher), 0x2923);
    }
}
