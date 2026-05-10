pub mod buf;
pub mod palette;

#[cfg(test)]
fn tile_to_attribute_byte(x: u8, y: u8) -> u8 {
    ((y / 4) * 8) + (x / 4)
}

#[cfg(test)]
fn tile_to_attribute_pos(x: u8, y: u8, attribute_byte: u8) -> u8 {
    let x = x % 4;
    let y = y % 4;

    match y {
        0 | 1 => match x {
            // top
            0 | 1 => attribute_byte & 0b0000_0011, // left
            2 | 3 => (attribute_byte & 0b0000_1100) >> 2, // right
            _ => panic!("This can't happen"),
        },
        2 | 3 => match x {
            // bottom
            0 | 1 => (attribute_byte & 0b0011_0000) >> 4, // left
            2 | 3 => (attribute_byte & 0b1100_0000) >> 6, // right
            _ => panic!("This can't happen"),
        },
        _ => panic!("This can't happen"),
    }
}

#[cfg(test)]
fn get_nametable_base_addr(nametable_id: u8) -> u16 {
    match nametable_id {
        0 => 0x2000,
        1 => 0x2400,
        2 => 0x2800,
        3 => 0x2C00,
        _ => panic!("Invalid nametable ID: {}", nametable_id),
    }
}

#[cfg(test)]
fn get_attribute_table_addr(nametable_base: u16) -> u16 {
    nametable_base + 0x3C0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_to_attribute_bytes() {
        assert_eq!(tile_to_attribute_byte(0x04, 0x19), 49);
    }

    #[test]
    fn test_tile_to_attribute_pos() {
        // bottomright = 1
        // bottomleft  = 2
        // topright    = 0
        // topleft     = 3
        let attribute_byte = 0b0110_0011;
        assert_eq!(tile_to_attribute_pos(0x0, 0x0, attribute_byte), 3);
        assert_eq!(tile_to_attribute_pos(0x0, 0x1, attribute_byte), 3);
        assert_eq!(tile_to_attribute_pos(0x1, 0x0, attribute_byte), 3);
        assert_eq!(tile_to_attribute_pos(0x1, 0x1, attribute_byte), 3);

        assert_eq!(tile_to_attribute_pos(0x2, 0x0, attribute_byte), 0);
        assert_eq!(tile_to_attribute_pos(0x2, 0x1, attribute_byte), 0);
        assert_eq!(tile_to_attribute_pos(0x3, 0x0, attribute_byte), 0);
        assert_eq!(tile_to_attribute_pos(0x3, 0x1, attribute_byte), 0);

        assert_eq!(tile_to_attribute_pos(0x2, 0x2, attribute_byte), 1);
        assert_eq!(tile_to_attribute_pos(0x2, 0x3, attribute_byte), 1);
        assert_eq!(tile_to_attribute_pos(0x3, 0x2, attribute_byte), 1);
        assert_eq!(tile_to_attribute_pos(0x3, 0x3, attribute_byte), 1);

        assert_eq!(tile_to_attribute_pos(0x0, 0x2, attribute_byte), 2);
        assert_eq!(tile_to_attribute_pos(0x1, 0x3, attribute_byte), 2);
        assert_eq!(tile_to_attribute_pos(0x0, 0x2, attribute_byte), 2);
        assert_eq!(tile_to_attribute_pos(0x1, 0x3, attribute_byte), 2);

        assert_eq!(tile_to_attribute_pos(0x04, 0x19, attribute_byte), 3);
    }

    #[test]
    fn test_nametable_addresses() {
        assert_eq!(get_nametable_base_addr(0), 0x2000);
        assert_eq!(get_nametable_base_addr(1), 0x2400);
        assert_eq!(get_nametable_base_addr(2), 0x2800);
        assert_eq!(get_nametable_base_addr(3), 0x2C00);
    }

    #[test]
    fn test_attribute_table_addresses() {
        assert_eq!(get_attribute_table_addr(0x2000), 0x23C0);
        assert_eq!(get_attribute_table_addr(0x2400), 0x27C0);
        assert_eq!(get_attribute_table_addr(0x2800), 0x2BC0);
        assert_eq!(get_attribute_table_addr(0x2C00), 0x2FC0);
    }
}
