pub mod buf;
pub mod palette;

use super::memory;
use super::ppu;
use buf::Buffer;

fn tile_to_attribute_byte(x: u8, y: u8) -> u8 {
    ((y / 4) * 8) + (x / 4)
}

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

fn get_nametable_base_addr(nametable_id: u8) -> u16 {
    match nametable_id {
        0 => 0x2000,
        1 => 0x2400,
        2 => 0x2800,
        3 => 0x2C00,
        _ => panic!("Invalid nametable ID: {}", nametable_id),
    }
}

fn get_attribute_table_addr(nametable_base: u16) -> u16 {
    nametable_base + 0x3C0
}

fn render_tile(
    mem: &dyn memory::MemoryMapper,
    buf: &mut Buffer,
    tile_x: usize,
    tile_y: usize,
    nametable_addr: u16,
    pattern_table_base: u16,
    screen_x_offset: isize,
    screen_y_offset: isize,
) {
    // Get pattern table index for this tile
    let tile_addr = nametable_addr + (tile_y * 0x20) as u16 + tile_x as u16;
    let pattern_table_index = mem.ppu_read(tile_addr) as u16;
    let pattern_table_addr = (pattern_table_index * 16) + pattern_table_base;

    // Get attribute table address for this nametable
    let attribute_table_addr = get_attribute_table_addr(nametable_addr);
    let attribute_table_offset = tile_to_attribute_byte(tile_x as u8, tile_y as u8) as u16;
    let attribute_byte = mem.ppu_read(attribute_table_addr + attribute_table_offset);
    let palette = tile_to_attribute_pos(tile_x as u8, tile_y as u8, attribute_byte);

    // Render the 8x8 tile
    for y in 0..8u16 {
        let lb = mem.ppu_read(pattern_table_addr + y);
        let hb = mem.ppu_read(pattern_table_addr + y + 8);

        for x in 0..8usize {
            // Fix bit extraction - use consistent bit order
            let bit_pos = 7 - x; // MSB is leftmost pixel
            let mask = 1 << bit_pos;
            let left = (lb & mask) >> bit_pos;
            let right = ((hb & mask) >> bit_pos) << 1;
            let pixel_value = (left | right) as usize;

            // Skip transparent pixels (color 0)
            if pixel_value == 0 {
                continue;
            }

            let color = palette::PALETTE[mem.ppu_read(
                (ppu::UNIVERSAL_BG_COLOR_ADDR + ((palette as usize) * 4) + pixel_value) as u16,
            ) as usize
                % palette::PALETTE_SIZE];

            // Calculate screen coordinates - screen_x_offset and screen_y_offset already account for scroll
            let pixel_x = screen_x_offset + x as isize;
            let pixel_y = screen_y_offset + y as isize;

            // Check bounds
            if pixel_x >= 0 && pixel_x < 256 && pixel_y >= 0 && pixel_y < 240 {
                buf.set_pixel(pixel_x as usize, pixel_y as usize, color);
            }
        }
    }
}

fn render_background(mem: &dyn memory::MemoryMapper, buf: &mut Buffer) {
    let ppu = mem.ppu();
    let ppu_ref = ppu.borrow();

    let pattern_table_base = ppu_ref.ctrl_background_pattern_addr();

    // Use the scroll register values that the game writes
    let scroll_x = ppu_ref.get_scroll_x() as usize;
    let scroll_y = ppu_ref.get_scroll_y() as usize;
    let fine_x = ppu_ref.get_fine_x() as usize;

    // Get the current nametable from PPUCTRL
    let base_nametable = ppu_ref.ppu_ctrl & 0x03;

    drop(ppu_ref); // Release the borrow

    // Calculate starting tile positions from scroll values
    let start_tile_x = scroll_x / 8;
    let start_tile_y = scroll_y / 8;
    let pixel_offset_x = scroll_x % 8;
    let pixel_offset_y = scroll_y % 8;

    // We need to draw 33x31 tiles to cover the screen plus scrolling
    for screen_tile_y in 0..31 {
        for screen_tile_x in 0..33 {
            // Calculate the tile coordinates in the scrolled world
            let world_tile_x = start_tile_x + screen_tile_x;
            let world_tile_y = start_tile_y + screen_tile_y;

            // Determine which nametable to use based on mirroring
            // NES has 4 nametables but only 2 are physically present, mirrored
            let nt_x = (world_tile_x / 32) % 2;
            let nt_y = (world_tile_y / 30) % 2;

            // Calculate nametable ID - use the base nametable and add offsets
            // This handles the standard NES mirroring pattern
            let nametable_id = (base_nametable + (nt_x as u8) + ((nt_y * 2) as u8)) % 4;

            // Calculate tile position within the selected nametable
            let tile_x = world_tile_x % 32;
            let tile_y = world_tile_y % 30;

            let nametable_addr = get_nametable_base_addr(nametable_id as u8);

            // Calculate screen position with proper offset
            // Each tile is 8x8 pixels, and we need to account for scroll offsets
            // The screen coordinates should be positive and represent the actual pixel position on screen
            let screen_x = (screen_tile_x * 8) as isize - pixel_offset_x as isize - fine_x as isize;
            let screen_y = (screen_tile_y * 8) as isize - pixel_offset_y as isize;

            // Ensure we only render tiles that are at least partially visible
            if screen_x < -8 || screen_x >= 256 || screen_y < -8 || screen_y >= 240 {
                continue;
            }

            render_tile(
                mem,
                buf,
                tile_x,
                tile_y,
                nametable_addr,
                pattern_table_base,
                screen_x,
                screen_y,
            );
        }
    }
}

fn render_sprites(mem: &dyn memory::MemoryMapper, buf: &mut Buffer) {
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

pub fn render(mem: &dyn memory::MemoryMapper, buf: &mut Buffer) {
    // Get the universal background color from PPU memory
    let background_color_index =
        mem.ppu_read(ppu::UNIVERSAL_BG_COLOR_ADDR as u16) as usize % palette::PALETTE_SIZE;
    let background_color = palette::PALETTE[background_color_index];

    // Clear the buffer with the background color
    buf.clear(background_color);

    if mem.ppu().borrow().mask_background_enabled() {
        render_background(mem, buf);
    }

    if mem.ppu().borrow().mask_sprites_enabled() {
        render_sprites(mem, buf);
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
