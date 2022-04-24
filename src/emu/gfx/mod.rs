pub mod buf;
pub mod palette;

use super::memory;
use super::ppu;
use buf::Buffer;

struct Rect {
    x1: usize,
    y1: usize,
    x2: usize,
    y2: usize,
}

impl Rect {
    fn new(x1: usize, y1: usize, x2: usize, y2: usize) -> Self {
        Rect {
            x1: x1,
            y1: y1,
            x2: x2,
            y2: y2,
        }
    }
}

fn tile_to_attribute_byte(x: u8, y: u8) -> u8 {
    ((y / 4) * 8) + (x / 4)
}

fn tile_to_attribute_pos(x: u8, y: u8, attribute_byte: u8) -> u8 {
    // value = (bottomright << 6) | (bottomleft << 4) | (topright << 2) | (topleft << 0)
    // x:
    // 04, 05 => 0, 1 => left
    // 06, 07 => 2, 3 => right

    // y:
    // 18, 19 => 0, 1 => top
    // 1a, 1b => 2, 3 => bottom
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

fn render_name_table(
    mem: &dyn memory::MemoryMapper,
    buf: &mut Buffer,
    name_table_addr: u16,
    view_port: Rect,
    shift_x: isize,
    shift_y: isize,
) {
    let pattern_table = mem.ppu().borrow().ctrl_background_pattern_addr();

    for row in 0..0x1f as usize {
        for col in 0..0x20 as usize {
            // what sprite # is written at this tile?
            let pattern_table_index =
                mem.ppu_read((name_table_addr as usize + (row * 0x20) + col) as _) as u16;
            // where are the pixels for that tile?
            let pattern_table_addr = (pattern_table_index * 16) + pattern_table;

            // where are the palette attributes for that tile?
            let attribute_table_addr_offset = tile_to_attribute_byte(col as u8, row as u8) as usize;
            // fetch palette attributes for that grid
            let attribute_byte =
                mem.ppu_read((ppu::ATTRIBUTE_TABLE_ADDR + attribute_table_addr_offset) as _);
            // find our position within grid and what palette to use
            let palette = tile_to_attribute_pos(col as u8, row as u8, attribute_byte);

            for yp in 0..8 as usize {
                let lb = mem.ppu_read((yp as u16 + pattern_table_addr) as _);
                let hb = mem.ppu_read((yp as u16 + 8 + pattern_table_addr) as _);

                for xp in 0..8 {
                    let mask = 1 << xp;
                    let left = (lb & mask) >> xp;
                    let right = ((hb & mask) >> xp) << 1;
                    let pixel_value: usize = (left | right) as usize;

                    let color = palette::PALETTE[mem.ppu_read(
                        (ppu::UNIVERSAL_BG_COLOR_ADDR + ((palette as usize) * 4) + pixel_value)
                            as _,
                    ) as usize
                        % palette::PALETTE_SIZE];

                    let pixel_x = (col * 8) + (8 - (xp)) as usize;
                    let pixel_y = row * 8 + yp as usize;

                    if pixel_x >= view_port.x1
                        && pixel_x < view_port.x2
                        && pixel_y >= view_port.y1
                        && pixel_y < view_port.y2
                    {
                        buf.set_pixel(
                            (shift_x + pixel_x as isize) as usize,
                            (shift_y + pixel_y as isize) as usize,
                            color,
                        );
                    }
                }
            }
        }
    }
}

fn render_sprites(mem: &dyn memory::MemoryMapper, buf: &mut Buffer) {
    let r = mem.ppu();
    let ppu = r.borrow();

    for i in (0..ppu::OAM_DATA_SIZE).step_by(4).rev() {
        let tile_idx = ppu.read_oam(i + 1) as u16;
        let tile_x = ppu.read_oam(i + 3) as usize;
        let tile_y = ppu.read_oam(i) as usize;

        let flip_vertical = if ppu.read_oam(i + 2) >> 7 & 1 == 1 {
            true
        } else {
            false
        };
        let flip_horizontal = if ppu.read_oam(i + 2) >> 6 & 1 == 1 {
            true
        } else {
            false
        };

        let tile_size = ppu.ctrl_sprite_size();

        let pallette_idx = ppu.read_oam(i + 2) & 0b11;
        let sprite_palette = (pallette_idx & 0b0000_0011) + 4;
        let bank: u16 = match tile_size {
            8 => ppu.ctrl_sprite_pattern_table_addr(),
            16 => {
                if tile_idx & 0b0000_0001 == 1 {
                    0x1000
                } else {
                    0x0
                }
            }
            _ => {
                panic!("Invalid tile size");
            }
        };

        let tile_offset = (bank + tile_idx * 16) as usize;

        for y in 0..tile_size as usize {
            let mut upper = mem.ppu_read((y + tile_offset) as _);
            let mut lower = mem.ppu_read((y + 8 + tile_offset) as _);
            for x in (0..8).rev() {
                let value = (1 & lower) << 1 | (1 & upper);
                upper = upper >> 1;
                lower = lower >> 1;

                if value == 0 {
                    continue;
                }

                let rgb = palette::PALETTE[mem.ppu_read(
                    (ppu::UNIVERSAL_BG_COLOR_ADDR
                        + ((sprite_palette as usize) * 4)
                        + value as usize) as _,
                ) as usize
                    % palette::PALETTE_SIZE];

                match (flip_horizontal, flip_vertical) {
                    (false, false) => {
                        buf.set_pixel(tile_x + x, tile_y + y, rgb);
                    }
                    (true, false) => {
                        buf.set_pixel(tile_x + 7 - x, tile_y + y, rgb);
                    }
                    (false, true) => {
                        buf.set_pixel(tile_x + x, tile_y + 7 - y, rgb);
                    }
                    (true, true) => {
                        buf.set_pixel(tile_x + 7 - x, tile_y + 7 - y, rgb);
                    }
                }
            }
        }
    }
}

pub fn render(mem: &dyn memory::MemoryMapper, buf: &mut Buffer) {
    if mem.ppu().borrow().mask_background_enabled() {
        let scroll_x = (mem.ppu().borrow().ppu_scroll_positions[0]) as usize;
        let scroll_y = (mem.ppu().borrow().ppu_scroll_positions[1]) as usize;
        let main_nametable_addr = mem.ppu().borrow().name_table_addr();
        let second_nametable_addr = main_nametable_addr + 0x400;
        render_name_table(
            mem,
            buf,
            main_nametable_addr,
            Rect::new(scroll_x, scroll_y, 256, 240),
            -(scroll_x as isize),
            -(scroll_y as isize),
        );
        if scroll_x > 0 {
            render_name_table(
                mem,
                buf,
                second_nametable_addr,
                Rect::new(0, 0, scroll_x, 240),
                (256 - scroll_x) as isize,
                0,
            );
        } else if scroll_y > 0 {
            render_name_table(
                mem,
                buf,
                second_nametable_addr,
                Rect::new(0, 0, 256, scroll_y),
                0,
                (240 - scroll_y) as isize,
            );
        }
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
        assert_eq!(tile_to_attribute_byte(0x04, 0x19), 49)
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
    fn test_tile_palette() {
        // bottomright = 1
        // bottomleft  = 2
        // topright    = 0
        // topleft     = 3
        let attribute_byte = 0b0110_0011;
        assert_eq!(tile_to_attribute_pos(0x04, 0x19, attribute_byte), 3);
    }
}
