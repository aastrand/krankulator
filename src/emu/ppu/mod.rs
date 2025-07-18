use super::memory;

/*
Common Name	Address	Bits	Notes
PPUCTRL	$2000	VPHB SINN	NMI enable (V), PPU master/slave (P), sprite height (H), background tile select (B), sprite tile select (S), increment mode (I), nametable select (NN)
PPUMASK	$2001	BGRs bMmG	color emphasis (BGR), sprite enable (s), background enable (b), sprite left column enable (M), background left column enable (1), greyscale (G)
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

pub const UNIVERSAL_BG_COLOR_ADDR: usize = 0x3F00;

/*
        RAM Memory Map
      +---------+-------+----------------+
      | Address | Size  | Description    |
      | $0000   | $2000 | Pattern Tables |
      | $2000   | $800  | Name Tables    |
      | $3F00   | $20   | Palettes       |
      +---------+-------+----------------+

        Programmer Memory Map
      +---------+-------+-------+--------------------+
      | Address | Size  | Flags | Description        |
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
    // Internal scroll registers (the key to proper scrolling)
    v: u16,  // Current VRAM address during rendering
    t: u16,  // Temporary VRAM address (top-left onscreen)
    x: u8,   // Fine X scroll (0-7)
    w: bool, // Write toggle (shared between PPUSCROLL and PPUADDR)

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

    // Keep these for backward compatibility but they're now handled by internal registers
    pub ppu_scroll_positions: [u8; 2],
    ppu_scroll_idx: usize,

    ppu_addr: [u8; 2],
    ppu_addr_idx: usize,
    ppu_data_valid: bool,

    ppu_data_buf: u8,

    pub cycle: u16,
    pub scanline: u16,
    pub frames: u64,
    // Per-scanline scroll tracking
    pub scroll_x_per_scanline: [u8; 262],
    pub scroll_y_per_scanline: [u8; 262],
    pub fine_x_per_scanline: [u8; 262],
}

impl PPU {
    pub fn new() -> PPU {
        let mut oam_ram = Box::new([0; OAM_DATA_SIZE]);
        let oam_ram_ptr = oam_ram.as_mut_ptr();

        PPU {
            // Initialize internal scroll registers
            v: 0,
            t: 0,
            x: 0,
            w: false,

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

            ppu_data_buf: 0,

            cycle: 0,
            scanline: 0,
            frames: 0,
            scroll_x_per_scanline: [0; 262],
            scroll_y_per_scanline: [0; 262],
            fine_x_per_scanline: [0; 262],
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
                // reset write toggle - this is crucial for proper scrolling
                self.w = false;
                // Legacy support
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
                //println!("Read {:X} from oam_ram[{:X}]", value, self.oam_addr);
                value
            }
            SCROLL_ADDR => 0,
            ADDR_ADDR => 0, // TODO: decay
            DATA_ADDR => {
                let read_addr = self.v;
                let value = if read_addr >= 0x3f00 && read_addr <= 0x3fff {
                    // Palette read: return actual value, buffer is updated with mirrored nametable
                    let mirrored_addr = read_addr & 0x2fff;
                    let result = mem.ppu_read(read_addr as _);
                    self.ppu_data_buf = mem.ppu_read(mirrored_addr as _);
                    result
                } else {
                    // Pattern/nametable: return buffer, update buffer
                    let r = self.ppu_data_buf;
                    self.ppu_data_buf = mem.ppu_read(read_addr as _);
                    r
                };
                self.inc_vram_addr_v();
                value
            }
            _ => {
                //println!("addr {:X} not mapped for read!", addr);
                0
            }
        }
    }

    fn inc_vram_addr_v(&mut self) {
        if (self.ppu_ctrl & CTRL_VRAM_ADDR_INC) == CTRL_VRAM_ADDR_INC {
            self.v = self.v.wrapping_add(32);
        } else {
            self.v = self.v.wrapping_add(1);
        }
        self.v &= 0x3FFF; // Keep within valid range
    }

    pub fn write(&mut self, addr: u16, value: u8, cpu_ram: *mut u8) -> Option<(u16, u8)> {
        let mut ret = None;

        match addr {
            CTRL_REG_ADDR => {
                self.ppu_ctrl = value;
                // Update nametable bits in t register
                self.t = (self.t & 0xF3FF) | (((value & 0x03) as u16) << 10);
            }
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
                    //println!("Glitch-increased oam_addr to {:X}]", self.oam_addr);
                } else {*/
                self.oam_ram[self.oam_addr as usize] = value;
                //println!("Wrote to {:X} oam_ram[{:X}]", value, self.oam_addr);
                self.oam_addr = self.oam_addr.wrapping_add(1);
                //}
            }
            SCROLL_ADDR => {
                // Proper scroll register handling
                if !self.w {
                    // First write (X scroll)
                    self.t = (self.t & 0xFFE0) | ((value >> 3) as u16);
                    self.x = value & 0x07;

                    // Legacy support
                    self.ppu_scroll_positions[0] = value;
                } else {
                    // Second write (Y scroll)
                    self.t = (self.t & 0x8C1F)
                        | (((value & 0x07) as u16) << 12)
                        | (((value & 0xF8) as u16) << 2);

                    // Legacy support
                    self.ppu_scroll_positions[1] = value;
                }

                self.w = !self.w;

                // Legacy support
                self.ppu_scroll_idx = if self.w { 1 } else { 0 };
                // Per-scanline scroll tracking
                if self.scanline < 240 {
                    let idx = self.scanline as usize;
                    self.scroll_x_per_scanline[idx] = self.get_scroll_x();
                    self.scroll_y_per_scanline[idx] = self.get_scroll_y();
                    self.fine_x_per_scanline[idx] = self.get_fine_x();
                }
            }
            ADDR_ADDR => {
                if !self.w {
                    // First write (high byte)
                    self.t = (self.t & 0x80FF) | (((value & 0x3F) as u16) << 8);
                } else {
                    // Second write (low byte)
                    self.t = (self.t & 0xFF00) | (value as u16);
                    self.v = self.t;
                }

                self.w = !self.w;

                // Legacy support
                self.ppu_addr[if self.w { 0 } else { 1 }] = value;
                if !self.w {
                    // Valid addresses are $0000-$3FFF; higher addresses will be mirrored down.
                    self.ppu_data_valid = false;
                }
                self.ppu_addr_idx = if self.w { 1 } else { 0 };
            }
            DATA_ADDR => {
                let write_addr = self.v;
                let page = memory::addr_to_page(write_addr);
                match page {
                    0x20 | 0x30 => {
                        // Addresses $3F10/$3F14/$3F18/$3F1C are mirrors of $3F00/$3F04/$3F08/$3F0C.
                        let addr: u16 = match write_addr {
                            0x3f10 | 0x3f14 | 0x3f18 | 0x3f1c => write_addr - 0x10,
                            _ => write_addr,
                        };
                        // This should be written by the mapper, bounce it back
                        ret = Some((addr, value))
                    }
                    _ => {
                        // Write to VRAM/CHR
                        ret = Some((write_addr, value));
                    }
                }
                self.inc_vram_addr_v();
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
            && self.scanline == PRE_RENDER_SCANLINE
            && self.cycle > 336
        {
            num_cycles = 4;
        }

        self.cycle = self.cycle.wrapping_add(num_cycles);
        if self.cycle > CYCLES_PER_SCANLINE {
            let hit_pixel_0 = self.sprite_zero_hit(self.cycle);
            if hit_pixel_0 {
                self.ppu_status |= STATUS_SPRITE_ZERO_HIT;
            }

            self.cycle = self.cycle % (CYCLES_PER_SCANLINE + 1);
            self.scanline = self.scanline.wrapping_add(1) % NUM_SCANLINES;
            self.begin_scanline();
            if self.scanline == 0 {
                self.frames += 1;
                // At the start of each frame, copy t to v (scroll reset)
                if self.mask_background_enabled() || self.mask_sprites_enabled() {
                    self.v = (self.v & 0x041F) | (self.t & 0x7BE0);
                }
            }

            if self.scanline == VBLANK_SCANLINE {
                self.ppu_status |= STATUS_VERTICAL_BLANK_BIT;
                self.ppu_status &= !STATUS_SPRITE_ZERO_HIT;
            }

            // OAMADDR is set to 0 during each of ticks 257-320 (the sprite tile loading interval) of the pre-render and visible scanlines.
            if (self.scanline < VBLANK_SCANLINE || self.scanline == PRE_RENDER_SCANLINE)
                && self.cycle > 256
            {
                self.oam_addr = 0;
            }

            // STATUS_SPRITE_ZERO_HIT cleared at dot 1 of the pre-render line.  Used for raster timing.
            let vblank = self.scanline == VBLANK_SCANLINE && self.vblank_nmi_is_enabled();
            if self.scanline == PRE_RENDER_SCANLINE {
                self.ppu_status &= !STATUS_VERTICAL_BLANK_BIT;
                self.ppu_status &= !STATUS_SPRITE_ZERO_HIT;
            }

            // Handle scroll register updates during rendering
            if self.scanline < VBLANK_SCANLINE || self.scanline == PRE_RENDER_SCANLINE {
                if self.mask_background_enabled() || self.mask_sprites_enabled() {
                    // Copy horizontal scroll from t to v at cycle 257
                    if self.cycle == 257 {
                        self.v = (self.v & 0x7BE0) | (self.t & 0x041F);
                    }
                }
            }

            return vblank;
        }

        return false;
    }

    pub fn begin_scanline(&mut self) {
        // Call this at the start of each scanline (in your PPU cycle/scanline logic)
        if self.scanline > 0 && self.scanline < 240 {
            let idx = self.scanline as usize;
            self.scroll_x_per_scanline[idx] = self.scroll_x_per_scanline[idx - 1];
            self.scroll_y_per_scanline[idx] = self.scroll_y_per_scanline[idx - 1];
            self.fine_x_per_scanline[idx] = self.fine_x_per_scanline[idx - 1];
        }
    }

    pub fn get_scanline_scroll_x(&self, scanline: usize) -> u8 {
        self.scroll_x_per_scanline[scanline]
    }
    pub fn get_scanline_scroll_y(&self, scanline: usize) -> u8 {
        self.scroll_y_per_scanline[scanline]
    }
    pub fn get_scanline_fine_x(&self, scanline: usize) -> u8 {
        self.fine_x_per_scanline[scanline]
    }

    pub fn sprite_zero_hit(&self, num_cycles: u16) -> bool {
        let y = self.oam_ram[0] as usize;
        let x = self.oam_ram[3] as usize;
        (y == self.scanline as usize) && x <= num_cycles as usize && self.mask_sprites_enabled()
    }

    pub fn vblank_nmi_is_enabled(&self) -> bool {
        (self.ppu_ctrl & CTRL_NMI_ENABLE) == CTRL_NMI_ENABLE
    }

    #[allow(dead_code)]
    pub fn is_in_vblank(&mut self) -> bool {
        (self.get_status_reg() & STATUS_VERTICAL_BLANK_BIT) == STATUS_VERTICAL_BLANK_BIT
    }

    pub fn read_oam(&self, offset: usize) -> u8 {
        unsafe { *self.oam_ram_ptr.offset(offset as _) }
    }

    pub fn ctrl_sprite_pattern_table_addr(&self) -> u16 {
        if self.ppu_ctrl & CTRL_SPRITE_PATTERN_TABLE_OFFSET == CTRL_SPRITE_PATTERN_TABLE_OFFSET {
            0x1000
        } else {
            0
        }
    }

    pub fn ctrl_sprite_size(&self) -> u8 {
        if self.ppu_ctrl & CTRL_SPRITE_SIZE == CTRL_SPRITE_SIZE {
            16
        } else {
            8
        }
    }

    pub fn ctrl_background_pattern_addr(&self) -> u16 {
        if self.ppu_ctrl & CTRL_BG_PATTERN_TABLE_OFFSET == CTRL_BG_PATTERN_TABLE_OFFSET {
            0x1000
        } else {
            0
        }
    }

    pub fn mask_background_enabled(&self) -> bool {
        self.ppu_mask & MASK_BACKGROUND_ENABLE == MASK_BACKGROUND_ENABLE
    }

    #[allow(dead_code)]
    pub fn mask_rendering_enabled(&self) -> bool {
        (self.ppu_mask & MASK_SPRITES_ENABLE) == MASK_RENDERING_ENABLE
    }

    pub fn mask_sprites_enabled(&self) -> bool {
        self.ppu_mask & MASK_SPRITES_ENABLE == MASK_SPRITES_ENABLE
    }

    // New methods to access scroll information for rendering
    pub fn get_scroll_x(&self) -> u8 {
        self.ppu_scroll_positions[0]
    }

    pub fn get_scroll_y(&self) -> u8 {
        self.ppu_scroll_positions[1]
    }

    pub fn get_fine_x(&self) -> u8 {
        self.x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_ppu_addr() {
        let mut ppu = PPU::new();
        let mut cpu_ram = [0u8; 2 * 1024];
        let cpu_ram_ptr = cpu_ram.as_mut_ptr();

        ppu.write(ADDR_ADDR, 0x32, cpu_ram_ptr);
        // v is not updated yet, only t is
        ppu.write(ADDR_ADDR, 0x11, cpu_ram_ptr);
        assert_eq!(ppu.v, 0x3211);
        ppu.write(ADDR_ADDR, 0x40, cpu_ram_ptr);
        // v is not updated yet, only t is
        ppu.write(ADDR_ADDR, 0x1, cpu_ram_ptr);
        assert_eq!(ppu.v, 0x0001);
    }

    #[test]
    fn test_write_ppu_addr_reset() {
        let mut ppu = PPU::new();
        let mut cpu_ram = [0u8; 2 * 1024];
        let cpu_ram_ptr = cpu_ram.as_mut_ptr();
        let mem: &dyn memory::MemoryMapper = &memory::IdentityMapper::new(0);

        ppu.write(ADDR_ADDR, 0x82, cpu_ram_ptr);

        ppu.read(STATUS_REG_ADDR, mem);
        assert_eq!(ppu.v, 0);
        ppu.write(ADDR_ADDR, 0x32, cpu_ram_ptr);
        assert_eq!(ppu.v, 0);
        ppu.write(ADDR_ADDR, 0x11, cpu_ram_ptr);

        assert_eq!(ppu.v, 0x3211);
    }

    #[test]
    fn test_write_ppu_data() {
        let mut ppu = PPU::new();
        let mut cpu_ram = [0u8; 2 * 1024];
        let cpu_ram_ptr = cpu_ram.as_mut_ptr();
        let mem: &dyn memory::MemoryMapper = &memory::IdentityMapper::new(0);

        ppu.read(STATUS_REG_ADDR, mem);
        ppu.write(ADDR_ADDR, 0x37, cpu_ram_ptr);
        ppu.write(ADDR_ADDR, 0x11, cpu_ram_ptr);

        for b in 0..10 {
            let should_write = ppu.write(DATA_ADDR, b, cpu_ram_ptr);
            assert_eq!(should_write.unwrap().0, (0x3711 + b as u16));
            assert_eq!(should_write.unwrap().1, b);
        }

        assert_eq!(ppu.v, 0x371b);
    }

    #[test]
    fn test_read_ppu_data() {
        let mut ppu = PPU::new();
        let mut cpu_ram = [0u8; 2 * 1024];
        let cpu_ram_ptr = cpu_ram.as_mut_ptr();
        let mem: &mut dyn memory::MemoryMapper = &mut memory::IdentityMapper::new(0x4000);

        mem.ppu_write(0x3000, 0x47);

        // Set address using proper PPU interface
        ppu.write(ADDR_ADDR, 0x30, cpu_ram_ptr);
        ppu.write(ADDR_ADDR, 0x00, cpu_ram_ptr);

        let first = ppu.read(DATA_ADDR, mem);

        // Reset address for second read
        ppu.write(ADDR_ADDR, 0x30, cpu_ram_ptr);
        ppu.write(ADDR_ADDR, 0x00, cpu_ram_ptr);
        let second = ppu.read(DATA_ADDR, mem);

        mem.ppu_write(0x3000, 0x14);

        // Reset address for third read
        ppu.write(ADDR_ADDR, 0x30, cpu_ram_ptr);
        ppu.write(ADDR_ADDR, 0x00, cpu_ram_ptr);
        let third = ppu.read(DATA_ADDR, mem);

        // Reset address for fourth read
        ppu.write(ADDR_ADDR, 0x30, cpu_ram_ptr);
        ppu.write(ADDR_ADDR, 0x00, cpu_ram_ptr);
        let fourth = ppu.read(DATA_ADDR, mem);

        assert_eq!(first, 0);
        assert_eq!(second, 0x47);
        assert_eq!(third, 0x47);
        assert_eq!(fourth, 0x14);
    }

    #[test]
    fn test_cycle() {
        let mut ppu = PPU::new();
        ppu.ppu_ctrl |= CTRL_NMI_ENABLE;

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
