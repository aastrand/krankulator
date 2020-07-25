use super::memory::mapper;

use std::cell::RefCell;
use std::rc::Rc;

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
pub const CTRL_REG_ADDR: usize = 0x2000;
pub const CTRL_NMI_ENABLE: u8 = 0b1000_0000;

pub const MASK_REG_ADDR: usize = 0x2001;
pub const STATUS_REG_ADDR: usize = 0x2002;
pub const VERTICAL_BLANK_BIT: u8 = 0b1000_0000;

pub const OAM_ADDR: usize = 0x2003;
pub const OAM_DATA_ADDR: usize = 0x2004;
pub const SCROLL_ADDR: usize = 0x2005;
pub const ADDR_ADDR: usize = 0x2006;
pub const DATA_ADDR: usize = 0x2007;
pub const _OAM_DMA: usize = 0x4014;

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
  mapper: Rc<RefCell<dyn mapper::MemoryMapper>>,

  vram: Box<[u8; VRAM_SIZE]>,

  _vram_addr: u16,
  _tmp_vram_addr: u16,

  /*
    The PPU internally contains 256 bytes of memory known as Object Attribute Memory which determines how sprites are rendered.
    The CPU can manipulate this memory through memory mapped registers at OAMADDR ($2003), OAMDATA ($2004), and OAMDMA ($4014).

    OAM can be viewed as an array with 64 entries.
    Each entry has 4 bytes: the sprite Y coordinate, the sprite tile number, the sprite attribute, and the sprite X coordinate.
  */
  oam_ram: Box<[u8; OAM_DATA_SIZE]>,

  ppu_ctrl: u8,
  ppu_mask: u8,
  ppu_status: u8,
  oam_addr: u8,
  oam_data: u8,
  ppu_scroll: u8,
  ppu_addr: u8,
  ppu_data: u8,
  oam_dma: u8,

  pub cycle: u16,
  pub scanline: u16,
}

impl PPU {
  pub fn new() -> PPU {
    PPU {
      mapper: Rc::new(RefCell::new(mapper::IdentityMapper::new(0))),

      vram: Box::new([0; VRAM_SIZE]),

      _vram_addr: 0,
      _tmp_vram_addr: 0,

      oam_ram: Box::new([0; OAM_DATA_SIZE]),

      ppu_ctrl: 0,
      ppu_mask: 0,
      ppu_status: 0x80,
      oam_addr: 0,
      oam_data: 0,
      ppu_scroll: 0,
      ppu_addr: 0,
      ppu_data: 0,
      oam_dma: 0,

      cycle: 0,
      scanline: 0,
    }
  }

  pub fn read(&mut self, addr: usize) -> u8 {
    match addr {
      CTRL_REG_ADDR => self.ppu_ctrl,
      MASK_REG_ADDR => self.ppu_mask,
      STATUS_REG_ADDR => {
        let status = self.ppu_status;
        // reading clears bit 7
        self.ppu_status &= !0b1000_0000;
        // TODO: clear "address latch" ?
        status
      }
      //OAM_ADDR => self.oam_addr,
      OAM_DATA_ADDR => {
        let value = self.oam_ram[self.oam_addr as usize];
        // Writes will increment OAMADDR after the write;
        // reads during vertical or forced blanking return the value from OAM at that address but do not increment.
        if self.ppu_status & VERTICAL_BLANK_BIT != VERTICAL_BLANK_BIT {
          self.oam_addr = self.oam_addr.wrapping_add(1)
        }

        value
      }
      SCROLL_ADDR => self.ppu_scroll,
      ADDR_ADDR => self.ppu_addr,
      DATA_ADDR => self.ppu_data,
      _ => panic!("addr {:X} not mapped for read!", addr),
    }
  }

  pub fn write(&mut self, addr: usize, value: u8) {
    match addr {
      CTRL_REG_ADDR => self.ppu_ctrl = value,
      MASK_REG_ADDR => self.ppu_mask = value,
      OAM_ADDR => self.oam_addr = value,
      OAM_DATA_ADDR => self.oam_data = value,
      SCROLL_ADDR => self.ppu_scroll = value,
      ADDR_ADDR => self.ppu_addr = value,
      DATA_ADDR => self.ppu_data = value,
      _ => panic!("addr {:X} not mapped for write!", addr),
    }
  }

  pub fn install_mapper(&mut self, mapper: Rc<RefCell<dyn mapper::MemoryMapper>>) {
    self.mapper = mapper;
  }

  pub fn cycle(&mut self) {
    for _ in 0..3 {
      self.cycle = self.cycle.wrapping_add(1);

      if self.cycle == 341 {
        self.cycle = 0;
        self.scanline = self.scanline.wrapping_add(1);

        if self.scanline == 262 {
          self.scanline = 0;
        }
      }
    }
  }
}
