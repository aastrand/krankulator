extern crate sdl2;

use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

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

  pub fn read(&mut self, addr: u16) -> u8 {
    match addr {
      CTRL_REG_ADDR => self.ppu_ctrl,
      MASK_REG_ADDR => self.ppu_mask,
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
      DATA_ADDR => {
        let value = self.vram[(self.vram_addr as usize) % VRAM_SIZE];
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
    if self.ppu_ctrl & CTRL_VRAM_ADDR_INC == CTRL_VRAM_ADDR_INC {
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
          // TODO: seems we are missing something here? vram mapping?
          self.vram_addr = (((self.ppu_addr[0] as u16) << 8) + self.ppu_addr[1] as u16) % 0x4000;
          self.ppu_data_valid = false;
          //println!("Set vram_addr to {:X}", self.vram_addr);
        }
        self.ppu_addr_idx = (self.ppu_addr_idx + 1) % 2
      }
      DATA_ADDR => {
        self.vram[(self.vram_addr as usize) % VRAM_SIZE] = value;
        //println!("Wrote {:X} to vram[{:X}]", value, (self.vram_addr as usize) % VRAM_SIZE);
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

  pub fn render(&mut self, canvas: &mut Canvas<Window>) {
    for y in 0..0x1f as usize {
      for x in 0..0x20 as usize {
        let val = self.vram[(y * 0x20) + x];
        canvas.set_draw_color(Color::RGB(val, val, val));
        canvas.fill_rect(Rect::new((x as i32) * 8, (y as i32) * 8, 8, 8));
      }
    }
  }
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

    ppu.read(STATUS_REG_ADDR);
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

    ppu.read(STATUS_REG_ADDR);
    ppu.write(ADDR_ADDR, 0x37, mem);
    ppu.write(ADDR_ADDR, 0x11, mem);

    for b in 0..10 {
      ppu.write(DATA_ADDR, b, mem);
    }

    assert_eq!(ppu.vram_addr, 0x371b);
    assert_eq!(ppu.vram[0x716], 5);
    assert_eq!(ppu.vram[0x71a], 9);
  }
}
