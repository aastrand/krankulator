extern crate sdl2;

use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
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
pub const CTRL_VRAM_ADDR_INC: u8 = 0b0000_0100;
pub const CTRL_PATTERN_TABLE_OFFSET: u8 = 0b0001_0000;
pub const CTRL_NMI_ENABLE: u8 = 0b1000_0000;

pub const MASK_REG_ADDR: u16 = 0x2001;
pub const STATUS_REG_ADDR: u16 = 0x2002;
pub const VERTICAL_BLANK_BIT: u8 = 0b1000_0000;

pub const OAM_ADDR: u16 = 0x2003;
pub const OAM_DATA_ADDR: u16 = 0x2004;
pub const SCROLL_ADDR: u16 = 0x2005;
pub const ADDR_ADDR: u16 = 0x2006;
pub const DATA_ADDR: u16 = 0x2007;
pub const OAM_DMA: u16 = 0x4014;

pub const VRAM_SIZE: usize = 2048;
pub const OAM_DATA_SIZE: usize = 256;

pub const UNIVERSAL_BG_COLOR_ADDR: usize = 0x3F00;
pub const ATTRIBUTE_TABLE_ADDR: usize = 0x23C0;

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
    pub vram: Box<[u8; VRAM_SIZE]>,

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
}

impl PPU {
    pub fn new() -> PPU {
        let mut oam_ram = Box::new([0; OAM_DATA_SIZE]);
        let oam_ram_ptr = oam_ram.as_mut_ptr();

        PPU {
            vram: Box::new([0; VRAM_SIZE]),

            vram_addr: 0,
            _tmp_vram_addr: 0,

            oam_ram: oam_ram,
            oam_ram_ptr: oam_ram_ptr,

            ppu_ctrl: 0,
            ppu_mask: 0,
            ppu_status: 0x80,
            oam_addr: 0,

            ppu_scroll_positions: [0; 2],
            ppu_scroll_idx: 0,

            ppu_addr: [0; 2],
            ppu_addr_idx: 0,
            ppu_data_valid: false,

            cycle: 0,
            scanline: 0,
        }
    }

    pub fn read(&mut self, addr: u16, chr_ptr: *mut u8) -> u8 {
        match addr {
            CTRL_REG_ADDR => self.ppu_ctrl,
            MASK_REG_ADDR => self.ppu_mask,
            OAM_ADDR => self.oam_addr,
            STATUS_REG_ADDR => {
                let status = self.ppu_status;
                // reading clears bit 7
                self.ppu_status &= !VERTICAL_BLANK_BIT;
                // reset address latches
                self.ppu_addr_idx = 0;
                self.ppu_scroll_idx = 0;

                status
            }
            OAM_DATA_ADDR => {
                // TODO: reads during vertical or forced blanking return the value from OAM at that address but do not increment.
                let value = self.oam_ram[self.oam_addr as usize];
                value
            }
            SCROLL_ADDR => 0,
            ADDR_ADDR => 0, // TODO: decay
            DATA_ADDR => {
                let page = memory::addr_to_page(self.vram_addr);

                let value = match page {
                    0x0 | 0x10 => {
                        // patterntble
                        unsafe { *chr_ptr.offset(self.vram_addr as _) }
                    }
                    0x20 | 0x30 => {
                        // nametable
                        // Addresses $3F10/$3F14/$3F18/$3F1C are mirrors of $3F00/$3F04/$3F08/$3F0C. 
                        let addr: usize = match self.vram_addr {
                            0x3f10 | 0x3f14 | 0x3f18 | 0x3f1c => (self.vram_addr - 0x10) as _,
                            _ => self.vram_addr as _
                        };
                        self.vram[addr % VRAM_SIZE]
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
                println!("addr {:X} not mapped for read!", addr);
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

    pub fn write(&mut self, addr: u16, value: u8, cpu_ram: *mut u8) {
        self.ppu_status |= value & 0b1110_0000;

        match addr {
            CTRL_REG_ADDR => self.ppu_ctrl = value,
            MASK_REG_ADDR => self.ppu_mask = value,
            OAM_ADDR => self.oam_addr = value,
            OAM_DATA_ADDR => {
                self.oam_ram[self.oam_addr as usize] = value;
                if self.ppu_status & VERTICAL_BLANK_BIT != VERTICAL_BLANK_BIT {
                    self.oam_addr = self.oam_addr.wrapping_add(1)
                }
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
                self.ppu_addr_idx = (self.ppu_addr_idx + 1) % 2
            }
            DATA_ADDR => {
                let page = memory::addr_to_page(self.vram_addr);

                match page {
                    0x20 | 0x30 => {
                        // TODO: more than one nametable? this should be handled by the mapper most likely
                        // nametable
                        // TODO: some kind of nametable mirroring needs to be checked

                        // Addresses $3F10/$3F14/$3F18/$3F1C are mirrors of $3F00/$3F04/$3F08/$3F0C. 
                        let addr: usize = match self.vram_addr {
                            0x3f10 | 0x3f14 | 0x3f18 | 0x3f1c => (self.vram_addr - 0x10) as _,
                            _ => self.vram_addr as _
                        };
                        self.vram[addr % VRAM_SIZE] = value;
                    }
                    _ => {}
                }

                /*println!(
                    "Wrote {:X} to vram[{:X}] ({:X})",
                    value,
                    (self.vram_addr as usize) % VRAM_SIZE,
                    self.vram_addr
                );*/
                self.inc_vram_addr();
            }
            OAM_DMA => {
                // Writing $XX will upload 256 bytes of data from CPU page $XX00-$XXFF to the internal PPU OAM.
                // This page is typically located in internal RAM, commonly $0200-$02FF,
                let page: u16 = (value as u16) << 8;
                if page < 0x20 {
                    unsafe {
                        std::ptr::copy(cpu_ram, self.oam_ram_ptr, 256);
                    }
                } else {
                    panic!("Tried to OAM_DMA copy page {:X}", page);
                }
            }
            _ => println!("addr {:X} not mapped for write!", addr),
        }
    }

    pub fn cycle(&mut self) {
        // OAMADDR is set to 0 during each of ticks 257-320 (the sprite tile loading interval) of the pre-render and visible scanlines.
        if self.cycle > 254 {
            self.oam_addr = 0;
        }

        for _ in 0..3 {
            // TODO: pre-render scanline? -1
            if self.scanline == 0 {
                self.ppu_status &= !VERTICAL_BLANK_BIT;
            } else if self.scanline == 241 {
                self.ppu_status |= VERTICAL_BLANK_BIT;
            }

            self.cycle = self.cycle.wrapping_add(1);

            if self.cycle == 341 {
                self.cycle = 0;
                self.scanline = self.scanline.wrapping_add(1) % 262;
            }
        }
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

        let addr_offset: u16 =
            if self.ppu_ctrl & CTRL_PATTERN_TABLE_OFFSET == CTRL_PATTERN_TABLE_OFFSET {
                0x1000
            } else {
                0
            };

        let bg_color = PALETTE[self.vram[UNIVERSAL_BG_COLOR_ADDR % VRAM_SIZE] as usize % PALETTE_SIZE];
        canvas.set_draw_color(bg_color);
        //self.print_palette();

        canvas.clear();
        for y in 0..0x1f as usize {
            for x in 0..0x20 as usize {
                // what sprite # is written at this tile?
                let pattern_table_index = self.vram[(y * 0x20) + x] as u16;
                // where is the pixels for that tile?
                let pattern_table_addr = (pattern_table_index * 16) + addr_offset;
                // copy pixels
                mem.ppu_copy(pattern_table_addr, tile_ptr, 16);

                // where are the palette attributes for that tile?
                let attribute_table_addr_offset =
                    self.tile_to_attribute_byte(x as u8, y as u8) as usize;
                // fetch palette attributes for that grid
                let attribute_byte =
                    self.vram[(ATTRIBUTE_TABLE_ADDR + attribute_table_addr_offset) % VRAM_SIZE];
                // find our position within grid and what palette to use
                let palette_offset = self.tile_to_attribute_pos(x as u8, y as u8, attribute_byte);

                let _ = texture.with_lock(None, |buffer: &mut [u8], pitch: usize| {
                    for yp in 0..8 {
                        let lb = tile[yp];
                        let hb = tile[yp + 8];
                        for xp in 0..8 {
                            let mask = 1 << xp;
                            let left = (lb & mask) >> xp;
                            let right = ((hb & mask) >> xp) << 1;
                            let pixel_value: usize = (left | right) as usize;

                            let transparency = if pixel_value == 0 { 0 } else { 0xff };

                            let palette = self.vram[(UNIVERSAL_BG_COLOR_ADDR
                                + ((palette_offset as usize) * 4)
                                + pixel_value)
                                % VRAM_SIZE] as usize;

                            if palette > 63 {
                                self._print_palette();
                                // 0 28 0 1 239 3 2
                                println!(
                                    "x={} y={} xp={} yp={} palette={} pixel_value={} palette_offset={}  ",
                                    x,
                                    y,
                                    xp,
                                    yp,
                                    palette,
                                    pixel_value,
                                    palette_offset
                                );
                            }

                            let color = {
                                PALETTE[palette % PALETTE_SIZE]
                            };

                            let offset = yp * pitch + (28 - (xp * 4));
                            buffer[offset] = color.r;
                            buffer[offset + 1] = color.g;
                            buffer[offset + 2] = color.b;
                            buffer[offset + 3] = transparency;
                        }
                    }
                });
                let _ = canvas.copy(
                    &texture,
                    None,
                    Some(Rect::new((x as i32) * 8, (y as i32) * 8, 8, 8)),
                );
                // }
            }
        }
    }

    fn _print_palette(&self) {
        for i in 0..0x20 as usize {
            let addr = UNIVERSAL_BG_COLOR_ADDR + i;
            println!("vram[{:X}] = {:X}", addr, self.vram[addr % VRAM_SIZE])
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
        let mem = Box::new([0; 2 * 1024]).as_mut_ptr();

        ppu.write(ADDR_ADDR, 0x32, mem);
        assert_eq!(ppu.vram_addr, 0);
        ppu.write(ADDR_ADDR, 0x11, mem);

        assert_eq!(ppu.vram_addr, 0x3211);

        ppu.write(ADDR_ADDR, 0x40, mem);
        ppu.write(ADDR_ADDR, 0x1, mem);

        assert_eq!(ppu.vram_addr, 0x1);
    }

    #[test]
    fn test_write_ppu_addr_reset() {
        let mut ppu = PPU::new();
        let mem = Box::new([0; 2 * 1024]).as_mut_ptr();
        ppu.write(ADDR_ADDR, 0x82, mem);

        ppu.read(STATUS_REG_ADDR, mem);
        assert_eq!(ppu.vram_addr, 0);
        ppu.write(ADDR_ADDR, 0x32, mem);
        assert_eq!(ppu.vram_addr, 0);
        ppu.write(ADDR_ADDR, 0x11, mem);

        assert_eq!(ppu.vram_addr, 0x3211);
    }

    #[test]
    fn test_write_ppu_data() {
        let mut ppu = PPU::new();
        let mem = Box::new([0; 2 * 1024]).as_mut_ptr();

        ppu.read(STATUS_REG_ADDR, mem);
        ppu.write(ADDR_ADDR, 0x37, mem);
        ppu.write(ADDR_ADDR, 0x11, mem);

        for b in 0..10 {
            ppu.write(DATA_ADDR, b, mem);
        }

        assert_eq!(ppu.vram_addr, 0x371b);
        assert_eq!(ppu.vram[0x716], 5);
        assert_eq!(ppu.vram[0x71a], 9);
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
        let mem = Box::new([0; 2 * 1024]).as_mut_ptr();
        ppu.write((ATTRIBUTE_TABLE_ADDR + 49) as u16, 0b0110_0011, mem);
        ppu.write((UNIVERSAL_BG_COLOR_ADDR + 3) as u16, 0x11, mem);
        ppu.write((UNIVERSAL_BG_COLOR_ADDR + 4) as u16, 0x11, mem);
        ppu.write((UNIVERSAL_BG_COLOR_ADDR + 5) as u16, 0x11, mem);

        // bottomright = 1
        // bottomleft  = 2
        // topright    = 0
        // topleft     = 3
        let attribute_byte = 0b0110_0011;
        assert_eq!(ppu.tile_to_attribute_pos(0x04, 0x19, attribute_byte), 3)
    }
}
