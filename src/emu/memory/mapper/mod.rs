pub mod mmc1;
pub mod nrom;

pub const RESET_TARGET_ADDR: u16 = 0xfffc;

fn addr_to_page(addr: u16) -> u16 {
    (addr >> 8) & 0xf0
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_addr_mirroring() {
        assert_eq!(mirror_addr(0x973), 0x173);
        assert_eq!(mirror_addr(0x3002), 0x2002);
        assert_eq!(mirror_addr(0x8000), 0x8000);
    }

    #[test]
    fn test_addr_to_page() {
        assert_eq!(addr_to_page(0x80), 0x0);
        assert_eq!(addr_to_page(0x8000), 0x80);
        assert_eq!(addr_to_page(0x1234), 0x10);
        assert_eq!(addr_to_page(0xffff), 0xf0);
    }
}
