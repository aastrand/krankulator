extern crate sdl2;

use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{BlendMode, Canvas, Texture, TextureCreator};
use sdl2::video::Window;

use super::memory;

/*
Common Name	Address	Bits	Notes
PPUCTRL	$2000	VPHB SINN	NMI enable (V), PPU master/slave (P), sprite height (H), background tile select (B), sprite tile select (S), increment mode (I), nametable select (NN)
PPUMASK	$2001	BGRs bMmG	color emphasis (BGR), sprite enable (s), background enable (b), sprite left column enable (M), background left column enable (m), greyscale (G)
PPUSTATUS	$2002	VSO- ----	vblank (V), sprite 0 hit (S), sprite overflow (O); read resets write pair for $2005/$2006
OAMADDR	$2003	aaaa aaaa	OAM read/write address
OAMDATA	$2004	dddd dddd	OAM data read/write
PPUSCROLL	$2005	xxxx xxxx	fine scroll position (two writes: X scroll, Y scroll)
PPUADDR	$2006	aaaa aaaa	PPU read/write address (two writes: most significant byte, least significant byte)
PPUDATA	$2007	dddd dddd	PPU data read/write
OAMDMA	$4014	aaaa aaaa	OAM DMA high address
*/
pub const CTRL_REG_ADDR: u16 = 0x2000;
pub const CTRL_NAMETABLE_ADDR: u8 = 0b0000_0011;
pub const CTRL_VRAM_ADDR_INC: u8 = 0b0000_0100;
pub const CTRL_SPRITE_PATTERN_TABLE_OFFSET: u8 = 0b0000_1000;
pub const CTRL_BG_PATTERN_TABLE_OFFSET: u8 = 0b0001_0000;
pub const CTRL_SPRITE_SIZE: u8 = 0b0010_0000;
pub const CTRL_NMI_ENABLE: u8 = 0b1000_0000;

pub const MASK_BACKGROUND_ENABLE: u8 = 0b0000_1000;
pub const MASK_SPRITES_ENABLE: u8 = 0b0001_0000;
#[allow(dead_code)]
pub const MASK_RENDERING_ENABLE: u8 = 0b0001_1000;

pub const STATUS_VERTICAL_BLANK_BIT: u8 = 0b1000_0000;
pub const STATUS_SPRITE_ZERO_HIT: u8 = 0b0100_0000;

pub const MASK_REG_ADDR: u16 = 0x2001;
pub const STATUS_REG_ADDR: u16 = 0x2002;

pub const OAM_ADDR: u16 = 0x2003;
pub const OAM_DATA_ADDR: u16 = 0x2004;
pub const SCROLL_ADDR: u16 = 0x2005;
pub const ADDR_ADDR: u16 = 0x2006;
pub const DATA_ADDR: u16 = 0x2007;
pub const OAM_DMA: u16 = 0x4014;

pub const OAM_DATA_SIZE: usize = 256;

pub const PRE_RENDER_SCANLINE: u16 = 261;
pub const VBLANK_SCANLINE: u16 = 241;
pub const CYCLES_PER_SCANLINE: u16 = 340;
pub const NUM_SCANLINES: u16 = PRE_RENDER_SCANLINE + 1;

pub const NAMETABLE_BASE_ADDR: usize = 0x2000;
pub const ATTRIBUTE_TABLE_ADDR: usize = 0x23C0;
pub const UNIVERSAL_BG_COLOR_ADDR: usize = 0x3F00;

pub const PALETTE_SIZE: usize = 64;
pub const PALETTE: [Color; PALETTE_SIZE] = [
    Color::RGB(84, 84, 84),
    Color::RGB(0, 30, 116),
    Color::RGB(8, 16, 144),
    Color::RGB(48, 0, 136),
    Color::RGB(68, 0, 100),
    Color::RGB(92, 0, 48),
    Color::RGB(84, 4, 0),
    Color::RGB(60, 24, 0),
    Color::RGB(32, 42, 0),
    Color::RGB(8, 58, 0),
    Color::RGB(0, 64, 0),
    Color::RGB(0, 60, 0),
    Color::RGB(0, 50, 60),
    Color::RGB(0, 0, 0),
    Color::RGB(0, 0, 0),
    Color::RGB(0, 0, 0),
    Color::RGB(152, 150, 152),
    Color::RGB(8, 76, 196),
    Color::RGB(48, 50, 236),
    Color::RGB(92, 30, 228),
    Color::RGB(136, 20, 176),
    Color::RGB(160, 20, 100),
    Color::RGB(152, 34, 32),
    Color::RGB(120, 60, 0),
    Color::RGB(84, 90, 0),
    Color::RGB(40, 114, 0),
    Color::RGB(8, 124, 0),
    Color::RGB(0, 118, 40),
    Color::RGB(0, 102, 120),
    Color::RGB(0, 0, 0),
    Color::RGB(0, 0, 0),
    Color::RGB(0, 0, 0),
    Color::RGB(236, 238, 236),
    Color::RGB(76, 154, 236),
    Color::RGB(120, 124, 236),
    Color::RGB(176, 98, 236),
    Color::RGB(228, 84, 236),
    Color::RGB(236, 88, 180),
    Color::RGB(236, 106, 100),
    Color::RGB(212, 136, 32),
    Color::RGB(160, 170, 0),
    Color::RGB(116, 196, 0),
    Color::RGB(76, 208, 32),
    Color::RGB(56, 204, 108),
    Color::RGB(56, 180, 204),
    Color::RGB(60, 60, 60),
    Color::RGB(0, 0, 0),
    Color::RGB(0, 0, 0),
    Color::RGB(236, 238, 236),
    Color::RGB(168, 204, 236),
    Color::RGB(188, 188, 236),
    Color::RGB(212, 178, 236),
    Color::RGB(236, 174, 236),
    Color::RGB(236, 174, 212),
    Color::RGB(236, 180, 176),
    Color::RGB(228, 196, 144),
    Color::RGB(204, 210, 120),
    Color::RGB(180, 222, 120),
    Color::RGB(168, 226, 144),
    Color::RGB(152, 226, 180),
    Color::RGB(160, 214, 228),
    Color::RGB(160, 162, 160),
    Color::RGB(0, 0, 0),
    Color::RGB(0, 0, 0),
];

/*
        RAM Memory Map
      +---------+-------+----------------+
      | Address | Size  | Description    |
      +---------+-------+----------------+
      | $0000   | $2000 | Pattern Tables |
      | $2000   | $800  | Name Tables    |
      | $3F00   | $20   | Palettes       |
      +---------+-------+----------------+

        Programmer Memory Map
      +---------+-------+-------+--------------------+
      | Address | Size  | Flags | Description        |
      +---------+-------+-------+--------------------+
      | $0000   | $1000 | C     | Pattern Table #0   |
      | $1000   | $1000 | C     | Pattern Table #1   |
      | $2000   | $3C0  |       | Name Table #0      |
      | $23C0   | $40   |  N    | Attribute Table #0 |
      | $2400   | $3C0  |  N    | Name Table #1      |
      | $27C0   | $40   |  N    | Attribute Table #1 |
      | $2800   | $3C0  |  N    | Name Table #2      |
      | $2BC0   | $40   |  N    | Attribute Table #2 |
      | $2C00   | $3C0  |  N    | Name Table #3      |
      | $2FC0   | $40   |  N    | Attribute Table #3 |
      | $3000   | $F00  |   R   |                    |
      | $3F00   | $10   |       | Image Palette #1   |
      | $3F10   | $10   |       | Sprite Palette #1  |
      | $3F20   | $E0   |    P  |                    |
      | $4000   | $C000 |     F |                    |
      +---------+-------+-------+--------------------+
                          C = Either CHR-ROM or CHR-RAM
                          N = Mirrored (see Subsection G)
                          P = Mirrored (see Subsection H)
                          R = Mirror of $2000-2EFF (VRAM)
                          F = Mirror of $0000-3FFF (VRAM)
*/

pub struct PPU {
    vram_addr: u16,
    _tmp_vram_addr: u16,

    /*
      The PPU internally contains 256 bytes of memory known as Object Attribute Memory which determines how sprites are rendered.
      The CPU can manipulate this memory through memory mapped registers at OAMADDR ($2003), OAMDATA ($2004), and OAMDMA ($4014).

      OAM can be viewed as an array with 64 entries.
      Each entry has 4 bytes: the sprite Y coordinate, the sprite tile number, the sprite attribute, and the sprite X coordinate.
    */
    oam_ram: Box<[u8; OAM_DATA_SIZE]>,
    oam_ram_ptr: *mut u8,

    pub ppu_ctrl: u8,
    pub ppu_mask: u8,
    ppu_status: u8,
    oam_addr: u8,

    ppu_scroll_positions: [u8; 2],
    ppu_scroll_idx: usize,

    ppu_addr: [u8; 2],
    ppu_addr_idx: usize,
    ppu_data_valid: bool,

    pub cycle: u16,
    pub scanline: u16,
    pub frames: u64,
    vblank_bit_race_condition: bool,
}

impl PPU {
    pub fn new() -> PPU {
        let mut oam_ram = Box::new([0; OAM_DATA_SIZE]);
        let oam_ram_ptr = oam_ram.as_mut_ptr();

        PPU {
            vram_addr: 0,
            _tmp_vram_addr: 0,

            oam_ram: oam_ram,
            oam_ram_ptr: oam_ram_ptr,

            ppu_ctrl: 0,
            ppu_mask: 0,
            ppu_status: 0x0,
            oam_addr: 0,

            ppu_scroll_positions: [0; 2],
            ppu_scroll_idx: 0,

            ppu_addr: [0; 2],
            ppu_addr_idx: 0,
            ppu_data_valid: false,

            cycle: 0,
            scanline: 0,
            frames: 0,
            vblank_bit_race_condition: false,
        }
    }

    pub fn get_status_reg(&mut self) -> u8 {
        let status = self.ppu_status;

        // Reading PPUSTATUS within two cycles of the start of vertical blank will return 0 in bit 7 but clear the latch anyway,
        // causing NMI to not occur that frame.
        // 240, 0 => (239*340) == 81260 + next tick

        // Reading one PPU clock before reads it as clear and never sets the flag or generates NMI for that frame.
        /*if self.scanline == 240 && self.cycle == 0 {
            //self.tick == 81260 {
            status &= !VERTICAL_BLANK_BIT;
            self.vblank_bit_race_condition = true;
        }
        // Reading on the same PPU clock or one later reads it as set, clears it, and suppresses the NMI for that frame.
        else if self.scanline == 240 && (self.cycle == 1 || self.cycle == 2) {
            //self.tick == 81261 || self.tick == 81262 {
            status |= VERTICAL_BLANK_BIT;
            self.vblank_bit_race_condition = true;
        }*/

        status
    }

    pub fn read(&mut self, addr: u16, mem: &dyn memory::MemoryMapper) -> u8 {
        match addr {
            CTRL_REG_ADDR => self.ppu_ctrl,
            MASK_REG_ADDR => self.ppu_mask,
            STATUS_REG_ADDR => {
                let status = self.get_status_reg();

                // reading clears bit 7
                self.ppu_status &= !STATUS_VERTICAL_BLANK_BIT;
                // reset address latches
                self.ppu_addr_idx = 0;
                self.ppu_addr[0] = 0;
                self.ppu_addr[1] = 0;
                self.ppu_scroll_idx = 0;

                status
            }
            OAM_ADDR => self.oam_addr,

            OAM_DATA_ADDR => {
                // reads during vertical or forced blanking return the value from OAM at that address but do not increment.
                let value = self.oam_ram[self.oam_addr as usize];
                println!("Read {:X} from oam_ram[{:X}]", value, self.oam_addr);
                value
            }
            SCROLL_ADDR => 0,
            ADDR_ADDR => 0, // TODO: decay
            DATA_ADDR => {
                let page = memory::addr_to_page(self.vram_addr);

                let value = match page {
                    0x0 | 0x10 => {
                        // patterntble
                        //unsafe { *chr_ptr.offset(self.vram_addr as _) }
                        mem.ppu_read(addr as _)
                    }
                    0x20 | 0x30 => {
                        // nametable
                        // Addresses $3F10/$3F14/$3F18/$3F1C are mirrors of $3F00/$3F04/$3F08/$3F0C.
                        let addr: usize = match self.vram_addr {
                            0x3f10 | 0x3f14 | 0x3f18 | 0x3f1c => (self.vram_addr - 0x10) as _,
                            _ => self.vram_addr as _,
                        };
                        //self.vram[addr % VRAM_SIZE]
                        mem.ppu_read(addr as _)
                    }
                    _ => 0,
                };

                if !self.ppu_data_valid {
                    self.ppu_data_valid = true;
                } else {
                    self.inc_vram_addr();
                }

                value
            }
            _ => {
                //println!("addr {:X} not mapped for read!", addr);
                0
            }
        }
    }

    fn inc_vram_addr(&mut self) {
        if (self.ppu_ctrl & CTRL_VRAM_ADDR_INC) == CTRL_VRAM_ADDR_INC {
            self.vram_addr = self.vram_addr.wrapping_add(32);
        } else {
            self.vram_addr = self.vram_addr.wrapping_add(1);
        }
    }

    pub fn write(&mut self, addr: u16, value: u8, cpu_ram: *mut u8) -> Option<(u16, u8)> {
        //self.ppu_status |= value & 0b1110_0000;
        let mut ret = None;

        match addr {
            CTRL_REG_ADDR => self.ppu_ctrl = value,
            MASK_REG_ADDR => self.ppu_mask = value,
            OAM_ADDR => {
                self.oam_addr = value;
                //println!("Set oam_addr to {:X}", value);
            }
            OAM_DATA_ADDR => {
                // Writes to OAMDATA during rendering (on the pre-render line and the visible lines 0-239,
                // provided either sprite or background rendering is enabled) do not modify values in OAM,
                // but do perform a glitchy increment of OAMADDR, bumping only the high 6 bits
                /*if (self.scanline == 261 || self.scanline < 240)
                    && self.ppu_mask & MASK_RENDERING_ENABLE != 0
                {
                    self.oam_addr = (self.oam_addr.wrapping_add(1) & 0b1111_1100)
                        + (self.oam_addr & 0b0000_0011);
                    println!("Glitch-increased oam_addr to {:X}]", self.oam_addr);
                } else {*/
                self.oam_ram[self.oam_addr as usize] = value;
                //println!("Wrote to {:X} oam_ram[{:X}]", value, self.oam_addr);
                self.oam_addr = self.oam_addr.wrapping_add(1);
                //}
            }
            SCROLL_ADDR => {
                self.ppu_scroll_positions[self.ppu_scroll_idx] = value;
                self.ppu_scroll_idx = (self.ppu_scroll_idx + 1) % 2;
            }
            ADDR_ADDR => {
                self.ppu_addr[self.ppu_addr_idx] = value;
                if self.ppu_addr_idx == 1 {
                    // Valid addresses are $0000-$3FFF; higher addresses will be mirrored down.
                    self.vram_addr =
                        (((self.ppu_addr[0] as u16) << 8) + self.ppu_addr[1] as u16) % 0x4000;
                    self.ppu_data_valid = false;
                    //println!("Set vram_addr to {:X}", self.vram_addr);
                }
                self.ppu_addr_idx = (self.ppu_addr_idx + 1) % 2;
                /*if self.ppu_addr_idx == 0 {
                    self.ppu_addr[0] = 0;
                    self.ppu_addr[1] = 0;
                }*/
            }
            DATA_ADDR => {
                let page = memory::addr_to_page(self.vram_addr);

                match page {
                    0x20 | 0x30 => {
                        // Addresses $3F10/$3F14/$3F18/$3F1C are mirrors of $3F00/$3F04/$3F08/$3F0C.
                        let addr: u16 = match self.vram_addr {
                            0x3f10 | 0x3f14 | 0x3f18 | 0x3f1c => (self.vram_addr - 0x10),
                            _ => self.vram_addr,
                        };
                        // This should be written by the mapper, bounce it back
                        ret = Some((addr, value))
                    }
                    _ => {}
                }

                /*println!(
                    "Wrote {:X} to vram[{:X}]",
                    value,
                    (self.vram_addr as usize)
                );*/
                self.inc_vram_addr();
            }
            OAM_DMA => {
                // Writing $XX will upload 256 bytes of data from CPU page $XX00-$XXFF to the internal PPU OAM.
                // This page is typically located in internal RAM, commonly $0200-$02FF,
                let page: u16 = (value as u16) << 8;
                // TODO: move to mapper
                if page < 0x2000 {
                    unsafe {
                        std::ptr::copy(cpu_ram.offset(page as _), self.oam_ram_ptr, 256);
                    }
                } else {
                    panic!("Tried to OAM_DMA copy page {:X}", page);
                }
                //println!("OAM DMA copied page {:X}", page);
            }
            _ => {} //println!("addr {:X} not mapped for write!", addr),
        }

        ret
    }

    pub fn cycle(&mut self) -> bool {
        // With rendering enabled, each odd PPU frame is one PPU clock shorter than normal.
        // This is done by skipping the first idle tick on the first visible scanline (by jumping directly from (339,261)
        // on the pre-render scanline to (0,0) on the first visible scanline and doing the last cycle of the last dummy nametable fetch there instead;
        let mut num_cycles = 3;
        if self.ppu_mask & MASK_BACKGROUND_ENABLE == MASK_BACKGROUND_ENABLE
            && self.frames % 2 == 1
            && self.scanline == 261
            && self.cycle > 336
        {
            num_cycles = 4;
        }

        // TODO: check sprite zero hit
        // STATUS_SPRITE_ZERO_HIT cleared at dot 1 of the pre-render line.  Used for raster timing.
        /*if self.hit_pixel_1(num_cycles) {
            self.ppu_status &= !STATUS_SPRITE_ZERO_HIT;
        }*/

        self.cycle = self.cycle.wrapping_add(num_cycles);
        if self.cycle > CYCLES_PER_SCANLINE {
            self.cycle = self.cycle % (CYCLES_PER_SCANLINE + 1);
            self.scanline = self.scanline.wrapping_add(1) % NUM_SCANLINES;
            if self.scanline == 0 {
                self.frames += 1;
            }
        }

        let hit_pixel_1 = self.hit_pixel_1(num_cycles);

        // OAMADDR is set to 0 during each of ticks 257-320 (the sprite tile loading interval) of the pre-render and visible scanlines.
        /*if (self.scanline < VBLANK_SCANLINE || self.scanline == PRE_RENDER_SCANLINE)
            && self.cycle > 256
        {
            self.oam_addr = 0;
        }*/

        let vblank =
            self.scanline == VBLANK_SCANLINE && hit_pixel_1 && !self.vblank_bit_race_condition;

        if self.scanline == PRE_RENDER_SCANLINE && hit_pixel_1 {
            self.ppu_status &= !STATUS_VERTICAL_BLANK_BIT;
        } else if vblank {
            self.ppu_status |= STATUS_VERTICAL_BLANK_BIT;
        }

        // return vblank = true for scanline 241 and pixel *1*
        return vblank;
    }

    pub fn hit_pixel_1(&self, num_cycles: u16) -> bool {
        self.cycle > 0 && self.cycle < (1 + num_cycles)
    }

    pub fn vblank_nmi_is_enabled(&self) -> bool {
        (self.ppu_ctrl & CTRL_NMI_ENABLE) == CTRL_NMI_ENABLE
    }

    #[allow(dead_code)]
    pub fn is_in_vblank(&mut self) -> bool {
        (self.get_status_reg() & STATUS_VERTICAL_BLANK_BIT) == STATUS_VERTICAL_BLANK_BIT
    }

    pub fn render(&mut self, canvas: &mut Canvas<Window>, mem: &dyn memory::MemoryMapper) {
        let texture_creator: TextureCreator<_> = canvas.texture_creator();
        let mut tile = [0; 16];
        let tile_ptr = tile.as_mut_ptr();

        let mut texture = texture_creator
            .create_texture_streaming(PixelFormatEnum::RGBA32, 8, 8)
            .map_err(|e| e.to_string())
            .ok()
            .unwrap();

        texture.set_blend_mode(BlendMode::Blend);

        let bg_color = PALETTE[mem.ppu_read(UNIVERSAL_BG_COLOR_ADDR as _) as usize % PALETTE_SIZE];
        canvas.set_draw_color(bg_color);
        //self.print_palette();

        let nametable_addr =
            NAMETABLE_BASE_ADDR + (0x400 * (self.ppu_ctrl & CTRL_NAMETABLE_ADDR) as usize);

        canvas.clear();

        if self.ppu_mask & MASK_BACKGROUND_ENABLE == MASK_BACKGROUND_ENABLE {
            let pattern_table: u16 =
                if self.ppu_ctrl & CTRL_BG_PATTERN_TABLE_OFFSET == CTRL_BG_PATTERN_TABLE_OFFSET {
                    0x1000
                } else {
                    0
                };
            for y in 0..0x1f as usize {
                for x in 0..0x20 as usize {
                    // what sprite # is written at this tile?
                    let pattern_table_index =
                        mem.ppu_read((nametable_addr + (y * 0x20) + x) as _) as u16;
                    // where are the pixels for that tile?
                    let pattern_table_addr = (pattern_table_index * 16) + pattern_table;
                    // copy pixels
                    mem.ppu_copy(pattern_table_addr, tile_ptr, 16);

                    // where are the palette attributes for that tile?
                    let attribute_table_addr_offset =
                        self.tile_to_attribute_byte(x as u8, y as u8) as usize;
                    // fetch palette attributes for that grid
                    let attribute_byte =
                        mem.ppu_read((ATTRIBUTE_TABLE_ADDR + attribute_table_addr_offset) as _);
                    // find our position within grid and what palette to use
                    let palette = self.tile_to_attribute_pos(x as u8, y as u8, attribute_byte);

                    self.render_tile_to_texture(mem, tile_ptr, palette, &mut texture);
                    let _ = canvas.copy(
                        &texture,
                        None,
                        Some(Rect::new((x as i32) * 8, (y as i32) * 8, 8, 8)),
                    );
                    // }
                }
            }
        }

        if self.ppu_mask & MASK_SPRITES_ENABLE == MASK_SPRITES_ENABLE {
            let pattern_table: u16 = if self.ppu_ctrl & CTRL_SPRITE_PATTERN_TABLE_OFFSET
                == CTRL_SPRITE_PATTERN_TABLE_OFFSET
            {
                0x1000
            } else {
                0
            };

            for s in 0..64 {
                let s = s * 4;
                let y = unsafe { *self.oam_ram_ptr.offset(s) };
                if y > 0xed {
                    continue;
                }

                let tile = unsafe { *self.oam_ram_ptr.offset(s + 1) };
                // TODO: 8x16
                if self.ppu_ctrl & CTRL_SPRITE_SIZE == CTRL_SPRITE_SIZE {
                } else {
                }
                let pattern_table_addr = (tile as u16 * 16) + pattern_table;
                mem.ppu_copy(pattern_table_addr, tile_ptr, 16);

                let attributes = unsafe { *self.oam_ram_ptr.offset(s + 2) };
                // Palette (4 to 7) of sprite
                let palette = (attributes & 0b0000_0011) + 4;
                let flip_horizontally = attributes & 0b0100_0000 == 0b0100_0000;
                let flip_vertically = attributes & 0b1000_0000 == 0b1000_0000;

                let x = unsafe { *self.oam_ram_ptr.offset(s + 3) };

                self.render_tile_to_texture(mem, tile_ptr, palette, &mut texture);

                /*println!(
                    "rendering sprite {} to x:{}, y:{} with palette {}",
                    s/4, x, y, palette
                );*/
                let _ = canvas.copy_ex(
                    &texture,
                    None,
                    Some(Rect::new(x as i32, y as i32, 8, 8)),
                    0f64,
                    None,
                    flip_horizontally,
                    flip_vertically,
                );
            }
        }
    }

    fn render_tile_to_texture(
        &self,
        mem: &dyn memory::MemoryMapper,
        tile_ptr: *mut u8,
        palette: u8,
        texture: &mut Texture,
    ) {
        let _ = texture.with_lock(None, |buffer: &mut [u8], pitch: usize| {
            for yp in 0..8 as usize {
                let lb = unsafe { *tile_ptr.offset(yp as _) };
                let hb = unsafe { *tile_ptr.offset((yp + 8) as _) };
                for xp in 0..8 as usize {
                    let mask = 1 << xp;
                    let left = (lb & mask) >> xp;
                    let right = ((hb & mask) >> xp) << 1;
                    let pixel_value: usize = (left | right) as usize;

                    let transparency = if pixel_value == 0 { 0 } else { 0xff };

                    let color = PALETTE[mem.ppu_read(
                        (UNIVERSAL_BG_COLOR_ADDR + ((palette as usize) * 4) + pixel_value) as _,
                    ) as usize
                        % PALETTE_SIZE];

                    let offset = yp * pitch + (28 - (xp * 4));
                    buffer[offset] = color.r;
                    buffer[offset + 1] = color.g;
                    buffer[offset + 2] = color.b;
                    buffer[offset + 3] = transparency;
                }
            }
        });
    }

    fn _print_palette(&self, mem: &dyn memory::MemoryMapper) {
        for i in 0..0x20 as usize {
            let addr = UNIVERSAL_BG_COLOR_ADDR + i;
            println!("vram[{:X}] = {:X}", addr, mem.ppu_read(addr as _))
        }
    }

    fn tile_to_attribute_byte(&self, x: u8, y: u8) -> u8 {
        ((y / 4) * 8) + (x / 4)
    }

    fn tile_to_attribute_pos(&self, x: u8, y: u8, attribute_byte: u8) -> u8 {
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

    /*fn _render_tile(
        &self,
        idx: u8,
        mem: &dyn memory::MemoryMapper,
        tc: TextureCreator<WindowContext>, // TODO ???
    ) -> Result<Texture, String> {
        let mut tile = [0; 16];
        let tile_ptr = tile.as_mut_ptr();
        mem.ppu_read((idx as u16) * 16, tile_ptr, 16);

        let mut texture = tc
            .create_texture_streaming(PixelFormatEnum::RGB24, 8, 8)
            .map_err(|e| e.to_string())?;

        texture.with_lock(None, |buffer: &mut [u8], pitch: usize| {
            for y in 0..8 {
                let lb = tile[y];
                let hb = tile[y + 8];
                for x in 1..9 {
                    let mask = 1 << x;
                    let left = ((lb << x) & mask) >> x;
                    let right = ((hb << x) & mask) >> x - 1;
                    let val = ((left + right) * 63) + 63;
                    // 00 = 0 => 63
                    // 01 = 1 => 126
                    // 10 = 2 => 189
                    // 11 = 3 => 255

                    let offset = y * pitch + x * 3;
                    buffer[offset] = val;
                    buffer[offset + 1] = val;
                    buffer[offset + 2] = val;
                }
            }
        })?;

        //Ok(texture)
        Err(format!(""))
    }*/
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_ppu_addr() {
        let mut ppu = PPU::new();
        let cpu_ram = Box::new([0; 2 * 1024]).as_mut_ptr();

        ppu.write(ADDR_ADDR, 0x32, cpu_ram);
        assert_eq!(ppu.vram_addr, 0);
        ppu.write(ADDR_ADDR, 0x11, cpu_ram);

        assert_eq!(ppu.vram_addr, 0x3211);

        ppu.write(ADDR_ADDR, 0x40, cpu_ram);
        ppu.write(ADDR_ADDR, 0x1, cpu_ram);

        assert_eq!(ppu.vram_addr, 0x1);
    }

    #[test]
    fn test_write_ppu_addr_reset() {
        let mut ppu = PPU::new();
        let cpu_ram = Box::new([0; 2 * 1024]).as_mut_ptr();
        let mem: &dyn memory::MemoryMapper = &memory::IdentityMapper::new(0);

        ppu.write(ADDR_ADDR, 0x82, cpu_ram);

        ppu.read(STATUS_REG_ADDR, mem);
        assert_eq!(ppu.vram_addr, 0);
        ppu.write(ADDR_ADDR, 0x32, cpu_ram);
        assert_eq!(ppu.vram_addr, 0);
        ppu.write(ADDR_ADDR, 0x11, cpu_ram);

        assert_eq!(ppu.vram_addr, 0x3211);
    }

    #[test]
    fn test_write_ppu_data() {
        let mut ppu = PPU::new();
        let cpu_ram = Box::new([0; 2 * 1024]).as_mut_ptr();
        let mem: &dyn memory::MemoryMapper = &memory::IdentityMapper::new(0);

        ppu.read(STATUS_REG_ADDR, mem);
        ppu.write(ADDR_ADDR, 0x37, cpu_ram);
        ppu.write(ADDR_ADDR, 0x11, cpu_ram);

        for b in 0..10 {
            let should_write = ppu.write(DATA_ADDR, b, cpu_ram);
            assert_eq!(should_write.unwrap().0, (0x3711 + b as u16));
            assert_eq!(should_write.unwrap().1, b);
        }

        assert_eq!(ppu.vram_addr, 0x371b);
    }

    #[test]
    fn test_tile_to_attribute_bytes() {
        let ppu = PPU::new();
        assert_eq!(ppu.tile_to_attribute_byte(0x04, 0x19), 49)
    }

    #[test]
    fn test_tile_to_attribute_pos() {
        let ppu = PPU::new();
        // bottomright = 1
        // bottomleft  = 2
        // topright    = 0
        // topleft     = 3
        let attribute_byte = 0b0110_0011;
        assert_eq!(ppu.tile_to_attribute_pos(0x0, 0x0, attribute_byte), 3);
        assert_eq!(ppu.tile_to_attribute_pos(0x0, 0x1, attribute_byte), 3);
        assert_eq!(ppu.tile_to_attribute_pos(0x1, 0x0, attribute_byte), 3);
        assert_eq!(ppu.tile_to_attribute_pos(0x1, 0x1, attribute_byte), 3);

        assert_eq!(ppu.tile_to_attribute_pos(0x2, 0x0, attribute_byte), 0);
        assert_eq!(ppu.tile_to_attribute_pos(0x2, 0x1, attribute_byte), 0);
        assert_eq!(ppu.tile_to_attribute_pos(0x3, 0x0, attribute_byte), 0);
        assert_eq!(ppu.tile_to_attribute_pos(0x3, 0x1, attribute_byte), 0);

        assert_eq!(ppu.tile_to_attribute_pos(0x2, 0x2, attribute_byte), 1);
        assert_eq!(ppu.tile_to_attribute_pos(0x2, 0x3, attribute_byte), 1);
        assert_eq!(ppu.tile_to_attribute_pos(0x3, 0x2, attribute_byte), 1);
        assert_eq!(ppu.tile_to_attribute_pos(0x3, 0x3, attribute_byte), 1);

        assert_eq!(ppu.tile_to_attribute_pos(0x0, 0x2, attribute_byte), 2);
        assert_eq!(ppu.tile_to_attribute_pos(0x1, 0x3, attribute_byte), 2);
        assert_eq!(ppu.tile_to_attribute_pos(0x0, 0x2, attribute_byte), 2);
        assert_eq!(ppu.tile_to_attribute_pos(0x1, 0x3, attribute_byte), 2);

        assert_eq!(ppu.tile_to_attribute_pos(0x04, 0x19, attribute_byte), 3);
    }

    #[test]
    fn test_tile_palette() {
        let mut ppu = PPU::new();
        let cpu_ram = Box::new([0; 2 * 1024]).as_mut_ptr();

        ppu.write((ATTRIBUTE_TABLE_ADDR + 49) as u16, 0b0110_0011, cpu_ram);
        ppu.write((UNIVERSAL_BG_COLOR_ADDR + 3) as u16, 0x11, cpu_ram);
        ppu.write((UNIVERSAL_BG_COLOR_ADDR + 4) as u16, 0x11, cpu_ram);
        ppu.write((UNIVERSAL_BG_COLOR_ADDR + 5) as u16, 0x11, cpu_ram);

        // bottomright = 1
        // bottomleft  = 2
        // topright    = 0
        // topleft     = 3
        let attribute_byte = 0b0110_0011;
        assert_eq!(ppu.tile_to_attribute_pos(0x04, 0x19, attribute_byte), 3);
    }

    #[test]
    fn test_cycle() {
        let mut ppu = PPU::new();

        let vblank = ppu.cycle();
        assert_eq!(vblank, false);
        assert_eq!(ppu.scanline, 0);
        assert_eq!(ppu.cycle, 3);
        assert_eq!(ppu.ppu_status & STATUS_VERTICAL_BLANK_BIT, 0);

        while ppu.cycle() == false {
            assert_eq!(ppu.ppu_status & STATUS_VERTICAL_BLANK_BIT, 0);
        }

        assert_eq!(ppu.scanline, 241);
        match ppu.cycle {
            1 | 2 | 3 => {}
            _ => panic!(
                "expected pixel 1 to have been hit in 3-pixel cycle, was {}",
                ppu.cycle
            ),
        }

        while ppu.scanline != 0 {
            let vblank = ppu.cycle();
            assert_eq!(vblank, false);
        }
        assert_eq!(ppu.ppu_status & STATUS_VERTICAL_BLANK_BIT, 0);
    }

    #[test]
    pub fn vblank_is_enabled() {
        let mut ppu = PPU::new();
        assert_eq!(ppu.vblank_nmi_is_enabled(), false);
        ppu.ppu_ctrl |= STATUS_VERTICAL_BLANK_BIT;
        assert_eq!(ppu.vblank_nmi_is_enabled(), true);
    }
}
