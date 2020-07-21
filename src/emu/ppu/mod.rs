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
pub const CTRL_NMI_ENABLE: u8 = 0b1000_0000;

pub const _MASK_REG_ADDR: u16 = 0x2001;
pub const _STATUS_REG_ADDR: u16 = 0x2002;
pub const _OAM_ADDR: u16 = 0x2003;
pub const _OAM_DATA: u16 = 0x2004;
pub const _SCROLL: u16 = 0x2006;
pub const _ADDR: u16 = 0x2007;
pub const _OAM_DMA: u16 = 0x4014;


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

pub struct _PPU {
    vram_addr: u16,
    tmp_vram_addr: u16,
    vram: [u8; 16 * 1024],
    spram: [u8; 256]
}