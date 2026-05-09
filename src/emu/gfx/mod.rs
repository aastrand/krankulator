pub mod buf;
pub mod palette;

use super::memory;
use super::ppu;
use buf::Buffer;

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

pub fn render_sprites(mem: &dyn memory::MemoryMapper, buf: &mut Buffer) {
    let r = mem.ppu();
    let ppu = r.borrow();

    // Render sprites in reverse order (higher index sprites have lower priority)
    for i in (0..ppu::OAM_DATA_SIZE).step_by(4).rev() {
        let tile_idx = ppu.read_oam(i + 1) as u16;
        let tile_x = ppu.read_oam(i + 3) as usize;
        let tile_y = (ppu.read_oam(i) as usize).wrapping_add(1); // NES sprites have Y offset of +1

        // Skip sprites that are off-screen or invalid
        if tile_y >= 0xEF || tile_y >= 240 {
            continue;
        }

        let attributes = ppu.read_oam(i + 2);
        let flip_vertical = (attributes >> 7) & 1 == 1;
        let flip_horizontal = (attributes >> 6) & 1 == 1;
        let priority = (attributes >> 5) & 1 == 1; // Background priority bit

        let tile_size = ppu.ctrl_sprite_size();
        let palette_idx = attributes & 0b11;
        let sprite_palette = (palette_idx & 0b0000_0011) + 4;

        let (bank, actual_tile_idx) = match tile_size {
            8 => (ppu.ctrl_sprite_pattern_table_addr(), tile_idx),
            16 => {
                // For 8x16 sprites, the pattern table is determined by bit 0 of the tile index
                // and the tile index should have its LSB cleared for addressing
                let pattern_table = if tile_idx & 0x01 == 1 { 0x1000 } else { 0x0000 };
                let tile_index = tile_idx & 0xFE; // Clear LSB
                (pattern_table, tile_index)
            }
            _ => panic!("Invalid tile size"),
        };

        for y in 0..tile_size as usize {
            // For 8x16 sprites, we need to handle two 8x8 tiles stacked vertically
            let (current_tile_offset, current_y) = if tile_size == 16 {
                if y < 8 {
                    // Top 8x8 tile
                    ((bank + actual_tile_idx * 16) as usize, y)
                } else {
                    // Bottom 8x8 tile (next tile in pattern table)
                    ((bank + (actual_tile_idx + 1) * 16) as usize, y - 8)
                }
            } else {
                // 8x8 sprite
                ((bank + actual_tile_idx * 16) as usize, y)
            };

            let upper = mem.ppu_read((current_y + current_tile_offset) as u16);
            let lower = mem.ppu_read((current_y + 8 + current_tile_offset) as u16);

            for x in 0..8 {
                // Fix bit extraction to match background rendering
                let bit_pos = 7 - x;
                let mask = 1 << bit_pos;
                let upper_bit = (upper & mask) >> bit_pos;
                let lower_bit = (lower & mask) >> bit_pos;
                let value = (lower_bit << 1) | upper_bit;

                // Skip transparent pixels
                if value == 0 {
                    continue;
                }

                let rgb = palette::PALETTE[mem.ppu_read(
                    (ppu::UNIVERSAL_BG_COLOR_ADDR + (sprite_palette as usize * 4) + value as usize)
                        as u16,
                ) as usize
                    % palette::PALETTE_SIZE];

                let (pixel_x, pixel_y) = match (flip_horizontal, flip_vertical) {
                    (false, false) => (tile_x + x, tile_y + y),
                    (true, false) => (tile_x + 7 - x, tile_y + y),
                    (false, true) => (tile_x + x, tile_y + (tile_size as usize - 1) - y),
                    (true, true) => (tile_x + 7 - x, tile_y + (tile_size as usize - 1) - y),
                };

                // Check bounds and priority
                if pixel_x < 256 && pixel_y < 240 {
                    // For sprites with background priority, only draw if background pixel is transparent
                    if priority {
                        // Background priority - only draw if background pixel is transparent (color 0)
                        let background_pixel = buf.get_pixel(pixel_x, pixel_y);
                        let background_color_index =
                            mem.ppu_read(ppu::UNIVERSAL_BG_COLOR_ADDR as u16) as usize
                                % palette::PALETTE_SIZE;
                        let background_color = palette::PALETTE[background_color_index];

                        // If the background pixel is the universal background color, it's transparent
                        // Also check if it's the same as the background color (meaning no tile was drawn there)
                        if background_pixel == background_color {
                            buf.set_pixel(pixel_x, pixel_y, rgb);
                        }
                    } else {
                        // Sprite priority - always draw on top
                        buf.set_pixel(pixel_x, pixel_y, rgb);
                    }
                }
            }
        }
    }
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
