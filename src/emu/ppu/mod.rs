use super::gfx::{buf::Buffer, palette};
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
pub const MASK_BACKGROUND_LEFT_ENABLE: u8 = 0b0000_0010;
pub const MASK_SPRITES_LEFT_ENABLE: u8 = 0b0000_0100;
#[cfg(test)]
pub const MASK_RENDERING_ENABLE: u8 = 0b0001_1000;

pub const STATUS_VERTICAL_BLANK_BIT: u8 = 0b1000_0000;
pub const STATUS_SPRITE_ZERO_HIT: u8 = 0b0100_0000;
pub const STATUS_SPRITE_OVERFLOW: u8 = 0b0010_0000;

pub const MASK_REG_ADDR: u16 = 0x2001;
pub const STATUS_REG_ADDR: u16 = 0x2002;

pub const OAM_ADDR: u16 = 0x2003;
pub const OAM_DATA_ADDR: u16 = 0x2004;
pub const SCROLL_ADDR: u16 = 0x2005;
pub const ADDR_ADDR: u16 = 0x2006;
pub const DATA_ADDR: u16 = 0x2007;
pub const OAM_DMA: u16 = 0x4014;

pub const OAM_DATA_SIZE: usize = 256;
pub const SCREEN_WIDTH: usize = 256;
pub const SCREEN_HEIGHT: usize = 240;

pub const PRE_RENDER_SCANLINE: u16 = 261;
pub const VBLANK_SCANLINE: u16 = 241;
pub const CYCLES_PER_SCANLINE: u16 = 340;
pub const NUM_SCANLINES: u16 = PRE_RENDER_SCANLINE + 1;

pub const UNIVERSAL_BG_COLOR_ADDR: usize = 0x3F00;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StepResult {
    pub fire_vblank_nmi: bool,
    pub ppu_cycle_260_scanline: Option<u16>,
}

#[derive(Clone, Copy)]
struct SpritePixel {
    value: u8,
    palette_id: u8,
    behind_background: bool,
}

#[derive(Clone, Copy, Default)]
struct SpriteLineEntry {
    attr: u8,
    x: u8,
    pattern_lo: u8,
    pattern_hi: u8,
}

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

    pub ppu_ctrl: u8,
    pub ppu_mask: u8,
    ppu_status: u8,
    oam_addr: u8,

    ppu_addr: [u8; 2],
    ppu_addr_idx: usize,
    ppu_data_valid: bool,

    ppu_data_buf: u8,

    pub cycle: u16,
    pub scanline: u16,
    pub frames: u64,
    pub last_synced_dot: u64,
    scanline_buf: [(u8, u8, u8); SCREEN_WIDTH],
    scanline_pixels_written: usize,
    render_line_v: u16,
    next_render_line_v: u16,
    bg_pattern_shift_low: u16,
    bg_pattern_shift_high: u16,
    bg_attr_shift_low: u16,
    bg_attr_shift_high: u16,
    bg_next_tile_id: u8,
    bg_next_attr: u8,
    bg_next_pattern_low: u8,
    bg_next_pattern_high: u8,

    /// Up to eight sprites for the scanline after `sprite_eval_target_scanline()`, 4 bytes each.
    secondary_oam: [u8; 32],
    secondary_oam_count: u8,
    /// Pattern/attr/X for the scanline currently being drawn (filled on the previous scanline).
    sprite_line: [SpriteLineEntry; 8],
    /// Staging during dots 257–320; committed at dot 1 of the next scanline.
    sprite_fetch_line: [SpriteLineEntry; 8],
    sprite_line_count: u8,
    /// Primary OAM sprite 0 will appear on the scanline we're drawing (latched from evaluation).
    sprite_zero_on_current_line: bool,
    sprite_zero_pending_next_line: bool,

    /// Primary OAM index for sprite evaluation (0–63), advanced on odd cycles 65–255.
    sprite_eval_n: u8,
    /// Byte offset within sprite during overflow / diagonal evaluation (0–3).
    sprite_eval_m: u8,
    /// `true` after 8 in-range sprites were copied: remaining odd cycles run step 3 (diagonal scan).
    sprite_eval_overflow_phase: bool,

    /// Set after the second `$2006` write advances `v` mid-scanline; consumed on the next visible
    /// dot to rebuild BG shift registers from `render_line_v` and VRAM.
    bg_shifter_resync_pending: bool,

    /// Last byte driven onto the CPU data bus by a PPU register read (open-bus model).
    ppu_open_bus: u8,

    /// $2002 read on scanline 241 dot 0 suppresses the NMI for that vblank.
    nmi_suppress_next_vblank: bool,
}

impl PPU {
    pub fn new() -> PPU {
        let oam_ram = Box::new([0; OAM_DATA_SIZE]);

        PPU {
            // Initialize internal scroll registers
            v: 0,
            t: 0,
            x: 0,
            w: false,

            oam_ram: oam_ram,

            ppu_ctrl: 0,
            ppu_mask: 0,
            ppu_status: 0x0,
            oam_addr: 0,

            ppu_addr: [0; 2],
            ppu_addr_idx: 0,
            ppu_data_valid: false,

            ppu_data_buf: 0,

            cycle: 0,
            scanline: 0,
            frames: 0,
            last_synced_dot: 0,
            scanline_buf: [(0, 0, 0); SCREEN_WIDTH],
            scanline_pixels_written: 0,
            render_line_v: 0,
            next_render_line_v: 0,
            bg_pattern_shift_low: 0,
            bg_pattern_shift_high: 0,
            bg_attr_shift_low: 0,
            bg_attr_shift_high: 0,
            bg_next_tile_id: 0,
            bg_next_attr: 0,
            bg_next_pattern_low: 0,
            bg_next_pattern_high: 0,

            secondary_oam: [0xFF; 32],
            secondary_oam_count: 0,
            sprite_line: [SpriteLineEntry::default(); 8],
            sprite_fetch_line: [SpriteLineEntry::default(); 8],
            sprite_line_count: 0,
            sprite_zero_on_current_line: false,
            sprite_zero_pending_next_line: false,

            sprite_eval_n: 0,
            sprite_eval_m: 0,
            sprite_eval_overflow_phase: false,

            bg_shifter_resync_pending: false,

            ppu_open_bus: 0,

            nmi_suppress_next_vblank: false,
        }
    }

    pub fn get_status_reg(&mut self) -> u8 {
        // Reading on scanline 241 dot 0 occurs one PPU dot before the vblank flag is set; if games poll
        // $2002 here, suppress the NMI that would otherwise trigger at dot 1.
        if self.scanline == VBLANK_SCANLINE && self.cycle == 0 {
            self.nmi_suppress_next_vblank = true;
        }

        self.ppu_status
    }

    pub fn read(&mut self, addr: u16, mem: &dyn memory::MemoryMapper) -> u8 {
        let v = match addr {
            // Write-only registers and mirrors: CPU reads see the last PPU bus value (simplified open bus).
            CTRL_REG_ADDR | MASK_REG_ADDR | OAM_ADDR | SCROLL_ADDR | ADDR_ADDR => self.ppu_open_bus,

            STATUS_REG_ADDR => {
                let status = self.get_status_reg();

                // Only vblank (bit 7) clears on read; sprite 0 hit and overflow clear at prerender dot 1
                // (https://www.nesdev.org/wiki/PPU_programmer_reference#PPUSTATUS_-_Rendering_events_.28.242002_read.29).
                self.ppu_status &= !STATUS_VERTICAL_BLANK_BIT;
                // reset write toggle - this is crucial for proper scrolling
                self.w = false;
                // Legacy support
                self.ppu_addr_idx = 0;
                self.ppu_addr[0] = 0;
                self.ppu_addr[1] = 0;
                self.ppu_data_valid = false;

                (status & 0xE0) | (self.ppu_open_bus & 0x1F)
            }

            OAM_DATA_ADDR => {
                // reads during vertical or forced blanking return the value from OAM at that address but do not increment.
                self.oam_ram[self.oam_addr as usize]
            }
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
            _ => self.ppu_open_bus,
        };

        self.ppu_open_bus = v;
        v
    }

    fn inc_vram_addr_v(&mut self) {
        if (self.ppu_ctrl & CTRL_VRAM_ADDR_INC) == CTRL_VRAM_ADDR_INC {
            self.v = self.v.wrapping_add(32);
        } else {
            self.v = self.v.wrapping_add(1);
        }
        self.v &= 0x3FFF; // Keep within valid range
    }

    pub fn write(&mut self, addr: u16, value: u8) -> Option<(u16, u8)> {
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
                // During active rendering, OAMDATA writes do not store to OAM; OAMADDR glitches forward
                // (high 6 bits increment) per nesdev Wiki/OAM.
                let rendering = (self.scanline == PRE_RENDER_SCANLINE
                    || self.scanline < SCREEN_HEIGHT as u16)
                    && (self.ppu_mask & (MASK_BACKGROUND_ENABLE | MASK_SPRITES_ENABLE)) != 0;
                if rendering {
                    // Glitch: high 6 bits (+4 per entry, i.e. coarse Sprite index) advance; low 2 bits clear.
                    self.oam_addr = ((self.oam_addr >> 2).wrapping_add(1)) << 2;
                } else {
                    self.oam_ram[self.oam_addr as usize] = value;
                    self.oam_addr = self.oam_addr.wrapping_add(1);
                }
            }
            SCROLL_ADDR => {
                // Proper scroll register handling
                if !self.w {
                    // First write (X scroll)
                    self.t = (self.t & 0xFFE0) | ((value >> 3) as u16);
                    self.x = value & 0x07;
                } else {
                    // Second write (Y scroll)
                    self.t = (self.t & 0x8C1F)
                        | (((value & 0x07) as u16) << 12)
                        | (((value & 0xF8) as u16) << 2);
                }

                self.w = !self.w;
            }
            ADDR_ADDR => {
                if !self.w {
                    // First write (high byte)
                    self.t = (self.t & 0x80FF) | (((value & 0x3F) as u16) << 8);
                } else {
                    // Second write (low byte)
                    self.t = (self.t & 0xFF00) | (value as u16);
                    self.v = self.t;
                    self.sync_render_origin_after_v_write();
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
            OAM_DMA => {}  // Handled by Emulator::cpu_write which has mapper access
            _ => {} //println!("addr {:X} not mapped for write!", addr),
        }

        self.ppu_open_bus = value;
        ret
    }

    pub fn get_current_vram_addr(&self) -> u16 {
        self.v
    }

    pub fn get_temp_vram_addr(&self) -> u16 {
        self.t
    }

    #[cfg(test)]
    pub fn step_dot(&mut self) -> StepResult {
        self.step_dot_inner(None)
    }

    pub fn oam_dma_write(&mut self, offset: u8, value: u8) {
        self.oam_ram[offset as usize] = value;
    }

    pub fn step_dot_with_rendering(
        &mut self,
        mem: &mut dyn memory::MemoryMapper,
        framebuffer: &mut Buffer,
    ) -> StepResult {
        self.step_dot_inner(Some((mem, framebuffer)))
    }

    fn step_dot_inner(
        &mut self,
        mut render_ctx: Option<(&mut dyn memory::MemoryMapper, &mut Buffer)>,
    ) -> StepResult {
        // With rendering enabled, each odd PPU frame is one PPU clock shorter than normal.
        // This is done by skipping the first idle tick on the first visible scanline (by jumping directly from (339,261)
        // on the pre-render scanline to (0,0) on the first visible scanline and doing the last cycle of the last dummy nametable fetch there instead;
        if self.ppu_mask & MASK_BACKGROUND_ENABLE == MASK_BACKGROUND_ENABLE
            && self.frames % 2 == 1
            && self.scanline == PRE_RENDER_SCANLINE
            && self.cycle == CYCLES_PER_SCANLINE - 1
        {
            self.cycle = 0;
            self.scanline = 0;
            self.frames += 1;
            self.last_synced_dot = self.last_synced_dot.wrapping_add(1);
            return StepResult::default();
        }

        self.cycle = self.cycle.wrapping_add(1);
        if self.cycle > CYCLES_PER_SCANLINE {
            self.cycle = 0;
            self.scanline = self.scanline.wrapping_add(1) % NUM_SCANLINES;
            if self.scanline == 0 {
                self.frames += 1;
            }
        }

        self.last_synced_dot = self.last_synced_dot.wrapping_add(1);

        if self.bg_shifter_resync_pending {
            if let Some((mem, _)) = render_ctx.as_ref() {
                if self.scanline < SCREEN_HEIGHT as u16
                    && self.cycle >= 1
                    && self.cycle <= 256
                    && (self.ppu_mask & MASK_BACKGROUND_ENABLE) != 0
                {
                    self.resync_background_shifters_for_dot(&**mem, self.cycle);
                }
            }
            self.bg_shifter_resync_pending = false;
        }

        let rendering_enabled = self.mask_background_enabled() || self.mask_sprites_enabled();
        let rendering_scanline =
            self.scanline < VBLANK_SCANLINE || self.scanline == PRE_RENDER_SCANLINE;

        let mut result = StepResult::default();

        if self.scanline < SCREEN_HEIGHT as u16 && self.cycle >= 1 && self.cycle <= 256 {
            if self.cycle == 1 {
                self.scanline_pixels_written = 0;
                self.render_line_v = self.next_render_line_v;
                if rendering_scanline {
                    if rendering_enabled {
                        self.sprite_line = self.sprite_fetch_line;
                        self.sprite_line_count = self.secondary_oam_count;
                        self.sprite_zero_on_current_line = self.sprite_zero_pending_next_line;
                        self.sprite_zero_pending_next_line = false;
                        self.start_sprite_evaluation();
                    } else {
                        self.sprite_line_count = 0;
                        self.sprite_zero_on_current_line = false;
                    }
                }
            }

            if let Some((mem, framebuffer)) = render_ctx.as_mut() {
                let x = (self.cycle - 1) as usize;
                self.scanline_buf[x] = self.render_pixel(&**mem);
                self.scanline_pixels_written += 1;

                if self.cycle == 256 {
                    self.copy_scanline_to_framebuffer(*framebuffer);
                }
            }
        }

        let sprite_zero_hit = if let Some((mem, _)) = render_ctx.as_ref() {
            self.sprite_zero_hit_with_rendering(&**mem, self.cycle)
        } else {
            self.sprite_zero_hit(self.cycle)
        };
        if sprite_zero_hit {
            self.ppu_status |= STATUS_SPRITE_ZERO_HIT;
        }

        if rendering_scanline && rendering_enabled && self.cycle == 257 {
            self.sprite_fetch_line = [SpriteLineEntry::default(); 8];
        }

        if rendering_scanline && rendering_enabled {
            if self.cycle >= 65 && self.cycle <= 255 && (self.cycle - 65) % 2 == 0 {
                self.sprite_evaluation_tick();
            }
        }

        if rendering_scanline && rendering_enabled {
            if let Some((mem, _)) = render_ctx.as_mut() {
                self.render_fetch_step(*mem);
            }
        }

        if self.scanline == VBLANK_SCANLINE && self.cycle == 1 {
            self.ppu_status |= STATUS_VERTICAL_BLANK_BIT;
            let mut fire = self.vblank_nmi_is_enabled();
            if self.nmi_suppress_next_vblank {
                fire = false;
                self.nmi_suppress_next_vblank = false;
            }
            result.fire_vblank_nmi = fire;
        }

        // OAMADDR is set to 0 during each of ticks 257-320 (the sprite tile loading interval) of the pre-render and visible scanlines.
        if rendering_scanline && rendering_enabled && self.cycle >= 257 && self.cycle <= 320 {
            self.oam_addr = 0;
        }

        // PPUSTATUS bits 5–7 (O, S, V) clear at dot 1 of prerender; games poll sprite 0 hit across frames.
        if self.scanline == PRE_RENDER_SCANLINE && self.cycle == 1 {
            self.ppu_status &= !STATUS_VERTICAL_BLANK_BIT;
            self.ppu_status &= !STATUS_SPRITE_ZERO_HIT;
            self.ppu_status &= !STATUS_SPRITE_OVERFLOW;
            self.clear_background_shift_registers();
            self.nmi_suppress_next_vblank = false;
        }

        // Handle scroll register updates during rendering
        if rendering_scanline && rendering_enabled {
            // Increment horizontal scroll every 8 dots across the scanline.
            // This happens at dots 328, 336, 8, 16, 24... 240, 248, 256.
            if matches!(self.cycle, 328 | 336)
                || (self.cycle >= 8 && self.cycle <= 256 && self.cycle % 8 == 0)
            {
                self.inc_coarse_x();
            }

            // Increment Y scroll at dot 256.
            if self.cycle == 256 {
                self.inc_y();
            }

            // Copy horizontal scroll from t to v at dot 257.
            if self.cycle == 257 {
                self.copy_horizontal_scroll();
                self.next_render_line_v = self.v;
            }

            // During dots 280-304 of the pre-render scanline, copy vertical bits from t to v.
            if self.scanline == PRE_RENDER_SCANLINE && self.cycle >= 280 && self.cycle <= 304 {
                self.copy_vertical_scroll();
                self.next_render_line_v = self.v;
            }
        }

        if rendering_scanline && self.cycle == 260 {
            result.ppu_cycle_260_scanline = Some(self.scanline);
        }

        result
    }

    fn copy_scanline_to_framebuffer(&self, framebuffer: &mut Buffer) {
        let y = self.scanline as usize;
        if y >= SCREEN_HEIGHT {
            return;
        }

        for (x, color) in self.scanline_buf.iter().enumerate() {
            framebuffer.set_pixel(x, y, *color);
        }
    }

    fn render_pixel(&self, mem: &dyn memory::MemoryMapper) -> (u8, u8, u8) {
        let backdrop = self.backdrop_color(mem);
        let (bg_pixel, bg_palette_id) = self.visible_background_pixel(mem, self.cycle);
        let bg_color = if bg_pixel == 0 {
            backdrop
        } else {
            self.background_palette_color(mem, bg_pixel, bg_palette_id)
        };

        let screen_x = self.cycle - 1;
        if let Some(sprite) = self.sprite_pixel(screen_x) {
            if bg_pixel == 0 || !sprite.behind_background {
                return self.sprite_palette_color(mem, sprite);
            }
        }

        bg_color
    }

    fn visible_background_pixel(&self, _mem: &dyn memory::MemoryMapper, dot: u16) -> (u8, u8) {
        let screen_x = dot.saturating_sub(1);
        if !self.mask_background_enabled()
            || (screen_x < 8 && (self.ppu_mask & MASK_BACKGROUND_LEFT_ENABLE) == 0)
        {
            return (0, 0);
        }

        (
            self.background_shift_pixel_value(),
            self.background_shift_palette_id(),
        )
    }

    fn background_palette_color(
        &self,
        mem: &dyn memory::MemoryMapper,
        pixel: u8,
        palette_id: u8,
    ) -> (u8, u8, u8) {
        let palette_addr =
            UNIVERSAL_BG_COLOR_ADDR as u16 + u16::from(palette_id) * 4 + u16::from(pixel);
        let color_idx = mem.ppu_read(palette_addr) as usize % palette::PALETTE_SIZE;
        palette::PALETTE[color_idx]
    }

    fn sprite_palette_color(
        &self,
        mem: &dyn memory::MemoryMapper,
        sprite: SpritePixel,
    ) -> (u8, u8, u8) {
        let palette_addr = UNIVERSAL_BG_COLOR_ADDR as u16
            + 0x10
            + u16::from(sprite.palette_id) * 4
            + u16::from(sprite.value);
        let color_idx = mem.ppu_read(palette_addr) as usize % palette::PALETTE_SIZE;
        palette::PALETTE[color_idx]
    }

    fn sprite_pixel(&self, screen_x: u16) -> Option<SpritePixel> {
        if !self.mask_sprites_enabled()
            || (screen_x < 8 && (self.ppu_mask & MASK_SPRITES_LEFT_ENABLE) == 0)
        {
            return None;
        }

        for i in 0..self.sprite_line_count {
            let e = self.sprite_line[i as usize];
            let sx = u16::from(e.x);
            if screen_x < sx || screen_x >= sx.wrapping_add(8) {
                continue;
            }

            let mut col = (screen_x - sx) as u8;
            if e.attr & 0x40 != 0 {
                col = 7 - col;
            }

            let bit = 7 - col;
            let value =
                ((e.pattern_lo >> bit) & 0x01) | (((e.pattern_hi >> bit) & 0x01) << 1);
            if value == 0 {
                continue;
            }

            return Some(SpritePixel {
                value,
                palette_id: e.attr & 0x03,
                behind_background: e.attr & 0x20 != 0,
            });
        }

        None
    }

    /// After `$2006` realigns `render_line_v`, rebuild BG shift registers so the next visible pixel
    /// matches hardware for the current fine-X scroll and `draw_cycle` (1–256).
    fn resync_background_shifters_for_dot(&mut self, mem: &dyn memory::MemoryMapper, draw_cycle: u16) {
        if draw_cycle == 0 || draw_cycle > 256 {
            return;
        }
        let screen_col = draw_cycle - 1;
        let pixel_offset = u16::from(self.x) + screen_col;
        let tile_index = (pixel_offset / 8) as i16;
        let fine_in_tile = (pixel_offset % 8) as u8;

        let pat_base = self.ctrl_background_pattern_addr();

        let bg_v_curr = Self::coarse_x_offset(self.render_line_v, tile_index);
        let fine_y = ((bg_v_curr >> 12) & 0x07) as u16;
        let tile_id_curr = mem.ppu_read(0x2000 | (bg_v_curr & 0x0FFF)) as u16;
        let pat_lo_curr = mem.ppu_read(pat_base + tile_id_curr * 16 + fine_y);
        let pat_hi_curr = mem.ppu_read(pat_base + tile_id_curr * 16 + fine_y + 8);
        let attr_curr = Self::palette_index_at_coarse_v(mem, bg_v_curr);
        let (al_c, ah_c) = Self::expanded_attr_plane_bytes(attr_curr);

        let bg_v_next = Self::coarse_x_incremented(bg_v_curr);
        let tile_id_next = mem.ppu_read(0x2000 | (bg_v_next & 0x0FFF)) as u16;
        let pat_lo_next = mem.ppu_read(pat_base + tile_id_next * 16 + fine_y);
        let pat_hi_next = mem.ppu_read(pat_base + tile_id_next * 16 + fine_y + 8);
        let attr_next = Self::palette_index_at_coarse_v(mem, bg_v_next);
        let (al_n, ah_n) = Self::expanded_attr_plane_bytes(attr_next);

        self.bg_pattern_shift_low = (u16::from(pat_lo_curr) << 8) | u16::from(pat_lo_next);
        self.bg_pattern_shift_high = (u16::from(pat_hi_curr) << 8) | u16::from(pat_hi_next);
        self.bg_attr_shift_low = (al_c << 8) | al_n;
        self.bg_attr_shift_high = (ah_c << 8) | ah_n;

        for _ in 0..fine_in_tile {
            self.shift_background_registers();
        }
    }

    pub(crate) fn palette_index_at_coarse_v(mem: &dyn memory::MemoryMapper, bg_v: u16) -> u8 {
        let attr_addr = 0x23C0 | (bg_v & 0x0C00) | ((bg_v >> 4) & 0x38) | ((bg_v >> 2) & 0x07);
        let attr = mem.ppu_read(attr_addr);
        let coarse_x = (bg_v & 0x001F) as u8;
        let coarse_y = ((bg_v >> 5) & 0x001F) as u8;
        let shift = ((coarse_y & 0x02) << 1) | (coarse_x & 0x02);
        (attr >> shift) & 0x03
    }

    fn expanded_attr_plane_bytes(attr: u8) -> (u16, u16) {
        let al = if attr & 0x01 != 0 { 0xFFu16 } else { 0 };
        let ah = if attr & 0x02 != 0 { 0xFFu16 } else { 0 };
        (al, ah)
    }

    fn background_shift_pixel_value(&self) -> u8 {
        let bit_mux = 0x8000 >> self.x;
        let low = if self.bg_pattern_shift_low & bit_mux != 0 {
            1
        } else {
            0
        };
        let high = if self.bg_pattern_shift_high & bit_mux != 0 {
            1
        } else {
            0
        };

        low | (high << 1)
    }

    fn background_shift_palette_id(&self) -> u8 {
        let bit_mux = 0x8000 >> self.x;
        let low = if self.bg_attr_shift_low & bit_mux != 0 {
            1
        } else {
            0
        };
        let high = if self.bg_attr_shift_high & bit_mux != 0 {
            1
        } else {
            0
        };

        low | (high << 1)
    }

    #[cfg(test)]
    fn background_v_for_dot(&self, dot: u16) -> u16 {
        let pixel_offset = u16::from(self.x) + dot - 1;
        Self::coarse_x_offset(self.render_line_v, (pixel_offset / 8) as i16)
    }

    #[cfg(test)]
    fn background_fine_x_for_dot(&self, dot: u16) -> u8 {
        let pixel_offset = u16::from(self.x) + dot - 1;
        (pixel_offset & 0x07) as u8
    }

    fn sync_render_origin_after_v_write(&mut self) {
        self.render_line_v = if self.scanline < SCREEN_HEIGHT as u16 && self.cycle < 256 {
            self.render_origin_for_dot(self.v, self.cycle + 1)
        } else {
            self.v
        };
        self.next_render_line_v = self.v;

        let rendering_scanline = self.scanline < SCREEN_HEIGHT as u16
            || self.scanline == PRE_RENDER_SCANLINE;
        let rendering_enabled = self.mask_background_enabled() || self.mask_sprites_enabled();
        if rendering_scanline && rendering_enabled && self.scanline < SCREEN_HEIGHT as u16 && self.cycle < 256 {
            self.bg_shifter_resync_pending = true;
        }
    }

    fn render_origin_for_dot(&self, v: u16, dot: u16) -> u16 {
        let pixel_offset = u16::from(self.x) + dot - 1;
        Self::coarse_x_offset(v, -((pixel_offset / 8) as i16))
    }

    fn render_fetch_step(&mut self, mem: &mut dyn memory::MemoryMapper) {
        if self.background_dummy_fetch_cycle() {
            self.fetch_background_nametable_byte(mem);
        } else if self.background_fetch_cycle() {
            self.background_fetch_step(mem);
        } else if self.sprite_fetch_cycle() {
            self.sprite_fetch_step(mem);
        }
    }

    fn background_fetch_step(&mut self, mem: &mut dyn memory::MemoryMapper) {
        match (self.cycle - 1) % 8 {
            0 => self.fetch_background_nametable_byte(mem),
            2 => self.fetch_background_attribute_byte(mem),
            4 => self.fetch_background_pattern_low(mem),
            6 => self.fetch_background_pattern_high(mem),
            7 => {
                self.shift_background_registers();
                self.load_background_shift_registers();
                return;
            }
            _ => {}
        }

        self.shift_background_registers();
    }

    fn background_fetch_cycle(&self) -> bool {
        matches!(self.cycle, 1..=256 | 321..=336)
    }

    fn background_dummy_fetch_cycle(&self) -> bool {
        matches!(self.cycle, 337 | 339)
    }

    fn sprite_fetch_cycle(&self) -> bool {
        (257..=320).contains(&self.cycle)
    }

    fn ppu_fetch(&mut self, mem: &mut dyn memory::MemoryMapper, addr: u16) -> u8 {
        mem.ppu_fetch(addr, self.last_synced_dot)
    }

    fn fetch_background_nametable_byte(&mut self, mem: &mut dyn memory::MemoryMapper) {
        let addr = 0x2000 | (self.v & 0x0FFF);
        self.bg_next_tile_id = self.ppu_fetch(mem, addr);
    }

    fn fetch_background_attribute_byte(&mut self, mem: &mut dyn memory::MemoryMapper) {
        let addr = 0x23C0 | (self.v & 0x0C00) | ((self.v >> 4) & 0x38) | ((self.v >> 2) & 0x07);
        let attr = self.ppu_fetch(mem, addr);
        let coarse_x = (self.v & 0x001F) as u8;
        let coarse_y = ((self.v >> 5) & 0x001F) as u8;
        let shift = ((coarse_y & 0x02) << 1) | (coarse_x & 0x02);
        self.bg_next_attr = (attr >> shift) & 0x03;
    }

    fn fetch_background_pattern_low(&mut self, mem: &mut dyn memory::MemoryMapper) {
        let addr = self.background_pattern_fetch_addr();
        self.bg_next_pattern_low = self.ppu_fetch(mem, addr);
    }

    fn fetch_background_pattern_high(&mut self, mem: &mut dyn memory::MemoryMapper) {
        let addr = self.background_pattern_fetch_addr() + 8;
        self.bg_next_pattern_high = self.ppu_fetch(mem, addr);
    }

    fn sprite_fetch_step(&mut self, mem: &mut dyn memory::MemoryMapper) {
        let slot = ((self.cycle - 257) / 8).min(7) as usize;
        match (self.cycle - 1) % 8 {
            0 | 2 => {
                self.ppu_fetch(mem, 0x2000);
                if (self.cycle - 257) % 8 == 0 {
                    if slot < self.secondary_oam_count as usize {
                        let b = slot * 4;
                        self.sprite_fetch_line[slot].attr = self.secondary_oam[b + 2];
                        self.sprite_fetch_line[slot].x = self.secondary_oam[b + 3];
                    } else {
                        self.sprite_fetch_line[slot] = SpriteLineEntry::default();
                    }
                }
            }
            4 => {
                let addr = self.secondary_sprite_pattern_addr(slot);
                let lo = self.ppu_fetch(mem, addr);
                self.sprite_fetch_line[slot].pattern_lo = lo;
            }
            6 => {
                let addr = self.secondary_sprite_pattern_addr(slot) + 8;
                let hi = self.ppu_fetch(mem, addr);
                self.sprite_fetch_line[slot].pattern_hi = hi;
            }
            _ => {}
        }
    }

    fn sprite_eval_target_scanline(&self) -> u16 {
        if self.scanline == PRE_RENDER_SCANLINE {
            0
        } else {
            self.scanline.wrapping_add(1)
        }
    }

    fn start_sprite_evaluation(&mut self) {
        self.secondary_oam.fill(0xFF);
        self.secondary_oam_count = 0;
        self.sprite_eval_n = 0;
        self.sprite_eval_m = 0;
        self.sprite_eval_overflow_phase = false;
    }

    /// One evaluation step on an odd PPU cycle during 65–255; see
    /// <https://www.nesdev.org/wiki/PPU_sprite_evaluation> (including overflow step 3).
    fn sprite_evaluation_tick(&mut self) {
        let target = self.sprite_eval_target_scanline();
        let h = u16::from(self.ctrl_sprite_size());

        let in_sprite_y_range = |y: u8| -> bool {
            let sprite_top = u16::from(y).wrapping_add(1);
            target >= sprite_top && target < sprite_top.wrapping_add(h)
        };

        if !self.sprite_eval_overflow_phase {
            if self.sprite_eval_n >= 64 {
                return;
            }
            let n = self.sprite_eval_n as usize;
            let base = n * 4;
            let y = self.oam_ram[base];
            if in_sprite_y_range(y) && self.secondary_oam_count < 8 {
                let dst = self.secondary_oam_count as usize * 4;
                self.secondary_oam[dst..dst + 4]
                    .copy_from_slice(&self.oam_ram[base..base + 4]);
                if n == 0 {
                    self.sprite_zero_pending_next_line = true;
                }
                self.secondary_oam_count += 1;
                if self.secondary_oam_count == 8 {
                    self.sprite_eval_overflow_phase = true;
                    self.sprite_eval_m = 0;
                }
            }
            self.sprite_eval_n += 1;
        } else {
            let n = (self.sprite_eval_n & 63) as usize;
            let m = (self.sprite_eval_m & 3) as usize;
            let idx = n * 4 + m;
            let y_byte = self.oam_ram[idx];

            if in_sprite_y_range(y_byte) {
                self.ppu_status |= STATUS_SPRITE_OVERFLOW;
                for _ in 0..3 {
                    self.sprite_eval_m += 1;
                    if self.sprite_eval_m == 4 {
                        self.sprite_eval_m = 0;
                        self.sprite_eval_n = (self.sprite_eval_n + 1) & 63;
                    }
                }
            } else {
                self.sprite_eval_n = (self.sprite_eval_n + 1) & 63;
                self.sprite_eval_m = (self.sprite_eval_m + 1) & 3;
            }
        }
    }

    fn secondary_sprite_pattern_addr(&self, slot: usize) -> u16 {
        let b = slot * 4;
        if slot >= self.secondary_oam_count as usize {
            return 0;
        }

        let tile_id = u16::from(self.secondary_oam[b + 1]);
        let y = self.secondary_oam[b];
        let sprite_top = u16::from(y).wrapping_add(1);
        let line = self.sprite_eval_target_scanline();
        let h = u16::from(self.ctrl_sprite_size());

        let mut row = line.wrapping_sub(sprite_top);
        if row >= h {
            row = 0;
        }

        let attr = self.secondary_oam[b + 2];
        if attr & 0x80 != 0 {
            row = h - 1 - row;
        }

        self.sprite_pattern_addr_for(tile_id, row, h)
    }

    fn sprite_pattern_addr_for(&self, tile_id: u16, row: u16, sprite_height: u16) -> u16 {
        if sprite_height == 16 {
            let pattern_table = if tile_id & 0x01 == 1 { 0x1000 } else { 0x0000 };
            let tile_base = tile_id & 0xFE;
            let tile_offset = if row >= 8 { 1 } else { 0 };
            pattern_table + (tile_base + tile_offset) * 16 + (row % 8)
        } else {
            self.ctrl_sprite_pattern_table_addr() + tile_id * 16 + row
        }
    }

    fn background_pattern_fetch_addr(&self) -> u16 {
        let fine_y = (self.v >> 12) & 0x07;
        self.ctrl_background_pattern_addr() + u16::from(self.bg_next_tile_id) * 16 + fine_y
    }

    fn load_background_shift_registers(&mut self) {
        self.bg_pattern_shift_low =
            (self.bg_pattern_shift_low & 0xFF00) | u16::from(self.bg_next_pattern_low);
        self.bg_pattern_shift_high =
            (self.bg_pattern_shift_high & 0xFF00) | u16::from(self.bg_next_pattern_high);

        let attr_low: u16 = if self.bg_next_attr & 0x01 != 0 {
            0xFF
        } else {
            0x00
        };
        let attr_high: u16 = if self.bg_next_attr & 0x02 != 0 {
            0xFF
        } else {
            0x00
        };
        self.bg_attr_shift_low = (self.bg_attr_shift_low & 0xFF00) | attr_low;
        self.bg_attr_shift_high = (self.bg_attr_shift_high & 0xFF00) | attr_high;
    }

    fn shift_background_registers(&mut self) {
        self.bg_pattern_shift_low <<= 1;
        self.bg_pattern_shift_high <<= 1;
        self.bg_attr_shift_low <<= 1;
        self.bg_attr_shift_high <<= 1;
    }

    fn clear_background_shift_registers(&mut self) {
        self.bg_pattern_shift_low = 0;
        self.bg_pattern_shift_high = 0;
        self.bg_attr_shift_low = 0;
        self.bg_attr_shift_high = 0;
    }

    fn coarse_x_offset(mut v: u16, offset: i16) -> u16 {
        if offset > 0 {
            for _ in 0..offset {
                v = Self::coarse_x_incremented(v);
            }
        } else {
            for _ in 0..offset.abs() {
                v = Self::coarse_x_decremented(v);
            }
        }
        v
    }

    fn coarse_x_incremented(mut v: u16) -> u16 {
        if (v & 0x001F) == 0x001F {
            v &= 0x7FE0;
            v ^= 0x0400;
        } else {
            v += 1;
        }
        v
    }

    fn coarse_x_decremented(mut v: u16) -> u16 {
        if (v & 0x001F) == 0 {
            v = (v & 0x7FE0) | 0x001F;
            v ^= 0x0400;
        } else {
            v -= 1;
        }
        v
    }

    fn backdrop_color(&self, mem: &dyn memory::MemoryMapper) -> (u8, u8, u8) {
        let color_idx =
            mem.ppu_read(UNIVERSAL_BG_COLOR_ADDR as u16) as usize % palette::PALETTE_SIZE;
        palette::PALETTE[color_idx]
    }

    #[cfg(test)]
    pub fn catch_up_to<F>(&mut self, target_dot: u64, mut on_step: F) -> bool
    where
        F: FnMut(StepResult),
    {
        let mut fire_vblank_nmi = false;
        while self.last_synced_dot < target_dot {
            let result = self.step_dot();
            fire_vblank_nmi |= result.fire_vblank_nmi;
            on_step(result);
        }
        fire_vblank_nmi
    }

    #[cfg(test)]
    pub fn cycle(&mut self) -> bool {
        let mut fire_vblank_nmi = false;
        for _ in 0..3 {
            fire_vblank_nmi |= self.step_dot().fire_vblank_nmi;
        }
        fire_vblank_nmi
    }

    fn inc_coarse_x(&mut self) {
        if (self.v & 0x1F) == 0x1F {
            // Coarse X overflow, switch nametable
            self.v &= 0x7FE0;
            self.v ^= 0x0400;
        } else {
            self.v += 1;
        }
    }

    fn inc_y(&mut self) {
        if (self.v & 0x7000) == 0x7000 {
            // Fine Y overflow, increment coarse Y
            self.v &= 0x8FFF;
            let coarse_y = (self.v & 0x03E0) >> 5;
            if coarse_y == 29 {
                // Coarse Y overflow, switch nametable
                self.v &= 0x7C1F;
                self.v ^= 0x0800;
            } else if coarse_y == 31 {
                self.v &= 0x7C1F;
            } else {
                self.v += 0x20;
            }
        } else {
            self.v += 0x1000;
        }
    }

    fn copy_horizontal_scroll(&mut self) {
        self.v = (self.v & 0x7BE0) | (self.t & 0x041F);
    }

    fn copy_vertical_scroll(&mut self) {
        self.v = (self.v & 0x041F) | (self.t & 0x7BE0);
    }

    pub fn sprite_zero_hit(&self, dot: u16) -> bool {
        if !self.mask_background_enabled()
            || !self.mask_sprites_enabled()
            || self.scanline >= SCREEN_HEIGHT as u16
        {
            return false;
        }

        let sprite_top = u16::from(self.oam_ram[0]).wrapping_add(1);
        let sprite_height = u16::from(self.ctrl_sprite_size());
        let sprite_x = u16::from(self.oam_ram[3]);

        self.scanline >= sprite_top
            && self.scanline < sprite_top.wrapping_add(sprite_height)
            && sprite_x < 255
            && dot >= sprite_x.wrapping_add(1)
    }

    fn sprite_zero_hit_with_rendering(&self, mem: &dyn memory::MemoryMapper, dot: u16) -> bool {
        if !self.mask_background_enabled()
            || !self.mask_sprites_enabled()
            || self.scanline >= SCREEN_HEIGHT as u16
        {
            return false;
        }

        if !self.sprite_zero_on_current_line || dot == 0 || dot > SCREEN_WIDTH as u16 {
            return false;
        }

        let screen_x = dot - 1;
        if screen_x == 255 {
            return false;
        }
        // PPUMASK: no sprite 0 hit in the left column if either BG or sprites are clipped there.
        if screen_x < 8
            && ((self.ppu_mask & MASK_BACKGROUND_LEFT_ENABLE) == 0
                || (self.ppu_mask & MASK_SPRITES_LEFT_ENABLE) == 0)
        {
            return false;
        }

        let e = self.sprite_line[0];
        let sx = u16::from(e.x);
        if screen_x < sx || screen_x >= sx.wrapping_add(8) {
            return false;
        }

        let mut col = (screen_x - sx) as u8;
        if e.attr & 0x40 != 0 {
            col = 7 - col;
        }
        let bit = 7 - col;
        let sp = ((e.pattern_lo >> bit) & 0x01) | (((e.pattern_hi >> bit) & 0x01) << 1);
        if sp == 0 {
            return false;
        }

        let (bg_pixel, _) = self.visible_background_pixel(mem, dot);
        if bg_pixel == 0 {
            return false;
        }

        true
    }

    pub fn vblank_nmi_is_enabled(&self) -> bool {
        (self.ppu_ctrl & CTRL_NMI_ENABLE) == CTRL_NMI_ENABLE
    }

    pub fn is_in_vblank(&self) -> bool {
        (self.ppu_status & STATUS_VERTICAL_BLANK_BIT) == STATUS_VERTICAL_BLANK_BIT
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

    pub fn mask_sprites_enabled(&self) -> bool {
        self.ppu_mask & MASK_SPRITES_ENABLE == MASK_SPRITES_ENABLE
    }
}

#[cfg(test)]
impl PPU {
    fn direct_background_pixel_value(&self, mem: &dyn memory::MemoryMapper, dot: u16) -> u8 {
        if dot == 0 || dot > SCREEN_WIDTH as u16 {
            return 0;
        }

        let bg_v = self.background_v_for_dot(dot);
        let fine_x = self.background_fine_x_for_dot(dot);
        let fine_y = ((bg_v >> 12) & 0x07) as u16;
        let tile_id = mem.ppu_read(0x2000 | (bg_v & 0x0FFF)) as u16;
        let pattern_addr = self.ctrl_background_pattern_addr() + tile_id * 16 + fine_y;
        let low = mem.ppu_read(pattern_addr);
        let high = mem.ppu_read(pattern_addr + 8);
        let bit = 7 - fine_x;
        ((low >> bit) & 0x01) | (((high >> bit) & 0x01) << 1)
    }

    fn direct_background_palette_id(&self, mem: &dyn memory::MemoryMapper, dot: u16) -> u8 {
        let bg_v = self.background_v_for_dot(dot);
        PPU::palette_index_at_coarse_v(mem, bg_v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emu::memory::MemoryMapper;

    fn prime_visible_scanline(ppu: &mut PPU, mem: &mut dyn memory::MemoryMapper) {
        ppu.scanline = PRE_RENDER_SCANLINE;
        ppu.cycle = 320;
        ppu.v = ppu.next_render_line_v;
        let mut framebuffer = Buffer::new();

        for _ in 0..21 {
            ppu.step_dot_with_rendering(mem, &mut framebuffer);
        }

        assert_eq!(ppu.scanline, 0);
        assert_eq!(ppu.cycle, 0);
    }

    #[test]
    fn test_write_ppu_addr() {
        let mut ppu = PPU::new();

        ppu.write(ADDR_ADDR, 0x32);
        // v is not updated yet, only t is
        ppu.write(ADDR_ADDR, 0x11);
        assert_eq!(ppu.v, 0x3211);
        ppu.write(ADDR_ADDR, 0x40);
        // v is not updated yet, only t is
        ppu.write(ADDR_ADDR, 0x1);
        assert_eq!(ppu.v, 0x0001);
    }

    #[test]
    fn test_write_ppu_addr_reset() {
        let mut ppu = PPU::new();

        let mem = memory::IdentityMapper::new(0);

        ppu.write(ADDR_ADDR, 0x82);

        ppu.read(STATUS_REG_ADDR, &mem);
        assert_eq!(ppu.v, 0);
        ppu.write(ADDR_ADDR, 0x32);
        assert_eq!(ppu.v, 0);
        ppu.write(ADDR_ADDR, 0x11);

        assert_eq!(ppu.v, 0x3211);
    }

    #[test]
    fn test_write_ppu_data() {
        let mut ppu = PPU::new();

        let mem = memory::IdentityMapper::new(0);

        ppu.read(STATUS_REG_ADDR, &mem);
        ppu.write(ADDR_ADDR, 0x37);
        ppu.write(ADDR_ADDR, 0x11);

        for b in 0..10 {
            let should_write = ppu.write(DATA_ADDR, b);
            assert_eq!(should_write.unwrap().0, (0x3711 + b as u16));
            assert_eq!(should_write.unwrap().1, b);
        }

        assert_eq!(ppu.v, 0x371b);
    }

    #[test]
    fn test_read_ppu_data() {
        let mut ppu = PPU::new();

        let mem: &mut dyn memory::MemoryMapper = &mut memory::IdentityMapper::new(0x4000);

        mem.ppu_write(0x3000, 0x47);

        // Set address using proper PPU interface
        ppu.write(ADDR_ADDR, 0x30);
        ppu.write(ADDR_ADDR, 0x00);

        let first = ppu.read(DATA_ADDR, mem);

        // Reset address for second read
        ppu.write(ADDR_ADDR, 0x30);
        ppu.write(ADDR_ADDR, 0x00);
        let second = ppu.read(DATA_ADDR, mem);

        mem.ppu_write(0x3000, 0x14);

        // Reset address for third read
        ppu.write(ADDR_ADDR, 0x30);
        ppu.write(ADDR_ADDR, 0x00);
        let third = ppu.read(DATA_ADDR, mem);

        // Reset address for fourth read
        ppu.write(ADDR_ADDR, 0x30);
        ppu.write(ADDR_ADDR, 0x00);
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

    #[test]
    fn test_catch_up_idempotent() {
        let mut ppu = PPU::new();

        let fired_nmi = ppu.catch_up_to(0, |_| {});

        assert_eq!(fired_nmi, false);
        assert_eq!(ppu.last_synced_dot, 0);
        assert_eq!(ppu.scanline, 0);
        assert_eq!(ppu.cycle, 0);
    }

    #[test]
    fn test_catch_up_advances_ppu_state() {
        let mut ppu = PPU::new();

        let fired_nmi = ppu.catch_up_to(341, |_| {});

        assert_eq!(fired_nmi, false);
        assert_eq!(ppu.last_synced_dot, 341);
        assert_eq!(ppu.scanline, 1);
        assert_eq!(ppu.cycle, 0);
    }

    #[test]
    fn test_step_dot_reports_cycle_260() {
        let mut ppu = PPU::new();
        let mut cycle_260_scanline = None;

        ppu.catch_up_to(260, |step| {
            if let Some(scanline) = step.ppu_cycle_260_scanline {
                cycle_260_scanline = Some(scanline);
            }
        });

        assert_eq!(cycle_260_scanline, Some(0));
    }

    #[test]
    fn test_scanline_produces_256_pixels() {
        let mut ppu = PPU::new();
        let mut mem = memory::IdentityMapper::new(0x4000);
        let mut framebuffer = Buffer::new();
        mem.ppu_write(UNIVERSAL_BG_COLOR_ADDR as u16, 1);

        for _ in 0..256 {
            ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        }

        assert_eq!(ppu.scanline_pixels_written, 256);
        assert_eq!(framebuffer.get_pixel(0, 0), palette::PALETTE[1]);
        assert_eq!(framebuffer.get_pixel(255, 0), palette::PALETTE[1]);
    }

    #[test]
    fn test_bg_disabled_outputs_backdrop() {
        let mut ppu = PPU::new();
        let mut mem = memory::IdentityMapper::new(0x4000);
        let mut framebuffer = Buffer::new();
        mem.ppu_write(UNIVERSAL_BG_COLOR_ADDR as u16, 2);

        for _ in 0..256 {
            ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        }

        for x in 0..SCREEN_WIDTH {
            assert_eq!(framebuffer.get_pixel(x, 0), palette::PALETTE[2]);
        }
    }

    #[test]
    fn test_bg_left_mask_outputs_backdrop_for_first_8_pixels() {
        let mut ppu = PPU::new();
        let mut mem = memory::IdentityMapper::new(0x4000);
        let mut framebuffer = Buffer::new();

        ppu.ppu_mask = MASK_BACKGROUND_ENABLE;
        ppu.next_render_line_v = 0;
        mem.ppu_write(0x2000, 1);
        mem.ppu_write(0x2001, 1);
        mem.ppu_write(0x0010, 0x80);
        mem.ppu_write(UNIVERSAL_BG_COLOR_ADDR as u16, 0);
        mem.ppu_write(UNIVERSAL_BG_COLOR_ADDR as u16 + 1, 3);

        prime_visible_scanline(&mut ppu, &mut mem);
        for _ in 0..256 {
            ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        }

        assert_eq!(framebuffer.get_pixel(0, 0), palette::PALETTE[0]);
        assert_eq!(framebuffer.get_pixel(8, 0), palette::PALETTE[3]);
    }

    #[test]
    fn test_sprite_left_mask_outputs_backdrop_for_first_8_pixels() {
        let mut ppu = PPU::new();
        let mut mem = memory::IdentityMapper::new(0x4000);
        let mut framebuffer = Buffer::new();

        ppu.scanline = 1;
        ppu.ppu_mask = MASK_SPRITES_ENABLE;
        ppu.oam_ram[0] = 0;
        ppu.oam_ram[1] = 1;
        ppu.oam_ram[3] = 0;
        mem.ppu_write(0x0010, 0x80);
        mem.ppu_write(UNIVERSAL_BG_COLOR_ADDR as u16, 0);
        mem.ppu_write(UNIVERSAL_BG_COLOR_ADDR as u16 + 0x11, 3);

        for _ in 0..256 {
            ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        }

        assert_eq!(framebuffer.get_pixel(0, 1), palette::PALETTE[0]);
    }

    #[test]
    fn test_sprite_rendering_uses_dot_renderer() {
        let mut ppu = PPU::new();
        let mut mem = memory::IdentityMapper::new(0x4000);
        let mut framebuffer = Buffer::new();

        ppu.scanline = 0;
        ppu.cycle = 0;
        ppu.ppu_mask = MASK_SPRITES_ENABLE | MASK_SPRITES_LEFT_ENABLE;
        ppu.oam_ram[0] = 0;
        ppu.oam_ram[1] = 1;
        ppu.oam_ram[3] = 0;
        mem.ppu_write(0x0010, 0x80);
        mem.ppu_write(UNIVERSAL_BG_COLOR_ADDR as u16, 0);
        mem.ppu_write(UNIVERSAL_BG_COLOR_ADDR as u16 + 0x11, 3);

        // One full scanline primes evaluation (target line 1) and sprite fetches (dots 257–320).
        for _ in 0..341 {
            ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        }
        for _ in 0..256 {
            ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        }

        assert_eq!(framebuffer.get_pixel(0, 1), palette::PALETTE[3]);
    }

    #[test]
    fn test_prefetch_v_does_not_shift_visible_pixels() {
        let mut ppu = PPU::new();
        let mut mem = memory::IdentityMapper::new(0x4000);
        let mut framebuffer = Buffer::new();

        ppu.ppu_mask = MASK_BACKGROUND_ENABLE | MASK_BACKGROUND_LEFT_ENABLE;
        ppu.next_render_line_v = 0;
        mem.ppu_write(0x2000, 1);
        mem.ppu_write(0x2002, 2);
        mem.ppu_write(0x0010, 0x80); // tile 1, row 0, leftmost pixel = color 1
        mem.ppu_write(0x0020, 0x00); // tile 2 would render backdrop if used
        mem.ppu_write(UNIVERSAL_BG_COLOR_ADDR as u16, 0);
        mem.ppu_write(UNIVERSAL_BG_COLOR_ADDR as u16 + 1, 3);

        prime_visible_scanline(&mut ppu, &mut mem);
        for _ in 0..256 {
            ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        }
        assert_eq!(framebuffer.get_pixel(0, 0), palette::PALETTE[3]);
    }

    #[test]
    fn test_scanline_advances_across_tiles() {
        let mut ppu = PPU::new();
        let mut mem = memory::IdentityMapper::new(0x4000);
        let mut framebuffer = Buffer::new();

        ppu.ppu_mask = MASK_BACKGROUND_ENABLE | MASK_BACKGROUND_LEFT_ENABLE;
        ppu.next_render_line_v = 0;
        mem.ppu_write(0x2000, 1);
        mem.ppu_write(0x2001, 2);
        mem.ppu_write(0x0010, 0x80); // tile 1 only lights x=0
        mem.ppu_write(0x0020, 0x40); // tile 2 only lights x=9
        mem.ppu_write(UNIVERSAL_BG_COLOR_ADDR as u16, 0);
        mem.ppu_write(UNIVERSAL_BG_COLOR_ADDR as u16 + 1, 3);

        prime_visible_scanline(&mut ppu, &mut mem);
        for _ in 0..256 {
            ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        }

        assert_eq!(framebuffer.get_pixel(0, 0), palette::PALETTE[3]);
        assert_eq!(framebuffer.get_pixel(8, 0), palette::PALETTE[0]);
        assert_eq!(framebuffer.get_pixel(9, 0), palette::PALETTE[3]);
    }

    #[test]
    fn test_mid_scanline_ppuaddr_write_anchors_next_pixel() {
        let mut ppu = PPU::new();
        let mut mem = memory::IdentityMapper::new(0x4000);
        let mut framebuffer = Buffer::new();

        ppu.ppu_mask = MASK_BACKGROUND_ENABLE | MASK_BACKGROUND_LEFT_ENABLE;
        ppu.next_render_line_v = 0;
        mem.ppu_write(0x2000, 1);
        mem.ppu_write(0x2001, 1);
        mem.ppu_write(0x2002, 2);
        mem.ppu_write(0x0010, 0x00); // tile 1 is transparent.
        mem.ppu_write(0x0020, 0x80); // tile 2 lights its leftmost pixel.
        mem.ppu_write(UNIVERSAL_BG_COLOR_ADDR as u16, 0);
        mem.ppu_write(UNIVERSAL_BG_COLOR_ADDR as u16 + 1, 3);

        for _ in 0..8 {
            ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        }

        ppu.write(ADDR_ADDR, 0x00);
        ppu.write(ADDR_ADDR, 0x02);

        for _ in 0..248 {
            ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        }

        assert_eq!(framebuffer.get_pixel(8, 0), palette::PALETTE[3]);
    }

    #[test]
    fn test_shift_register_loads_at_tile_boundary() {
        let mut ppu = PPU::new();
        let mut mem = memory::IdentityMapper::new(0x4000);
        let mut framebuffer = Buffer::new();

        ppu.ppu_mask = MASK_BACKGROUND_ENABLE;
        mem.ppu_write(0x2000, 1);
        mem.ppu_write(0x23C0, 0b0000_0011);
        mem.ppu_write(0x0010, 0x80);
        mem.ppu_write(0x0018, 0x40);

        for _ in 0..8 {
            ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        }

        assert_eq!(ppu.bg_next_tile_id, 1);
        assert_eq!(ppu.bg_next_attr, 3);
        assert_eq!(ppu.bg_pattern_shift_low & 0x00FF, 0x80);
        assert_eq!(ppu.bg_pattern_shift_high & 0x00FF, 0x40);
        assert_eq!(ppu.bg_attr_shift_low & 0x00FF, 0xFF);
        assert_eq!(ppu.bg_attr_shift_high & 0x00FF, 0xFF);
    }

    #[test]
    fn test_tile_fetch_sequence() {
        let mut ppu = PPU::new();
        let mut mem = memory::IdentityMapper::new(0x4000);
        let mut framebuffer = Buffer::new();

        ppu.ppu_mask = MASK_BACKGROUND_ENABLE;
        mem.ppu_write(0x2000, 1);
        mem.ppu_write(0x23C0, 0b0000_0010);
        mem.ppu_write(0x0010, 0x80);
        mem.ppu_write(0x0018, 0x40);

        ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        assert_eq!(ppu.bg_next_tile_id, 1);

        ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        assert_eq!(ppu.bg_next_attr, 2);

        ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        assert_eq!(ppu.bg_next_pattern_low, 0x80);

        ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        assert_eq!(ppu.bg_next_pattern_high, 0x40);
    }

    #[test]
    fn test_prefetch_dots_321_336_seed_visible_shifters() {
        let mut ppu = PPU::new();
        let mut mem = memory::IdentityMapper::new(0x4000);

        ppu.ppu_mask = MASK_BACKGROUND_ENABLE;
        ppu.next_render_line_v = 0;
        mem.ppu_write(0x2000, 1);
        mem.ppu_write(0x2001, 2);
        mem.ppu_write(0x0010, 0x80);
        mem.ppu_write(0x0020, 0x40);

        prime_visible_scanline(&mut ppu, &mut mem);

        assert_eq!(ppu.bg_pattern_shift_low, 0x8040);
    }

    fn seed_background_comparison_pattern(mem: &mut dyn memory::MemoryMapper) {
        for tile in 0..32u16 {
            mem.ppu_write(0x2000 + tile, (tile + 1) as u8);
            mem.ppu_write(0x23C0 + (tile / 4), (tile % 4) as u8 * 0x55);

            let pattern_base = (tile + 1) * 16;
            for row in 0..8u16 {
                let rotate = ((tile + row) % 8) as u32;
                mem.ppu_write(pattern_base + row, 0b1001_0110u8.rotate_left(rotate));
                mem.ppu_write(pattern_base + row + 8, 0b0110_1001u8.rotate_right(rotate));
            }
        }
    }

    fn assert_shifter_matches_direct_background(
        ppu: &PPU,
        mem: &dyn memory::MemoryMapper,
        dot: u16,
    ) {
        let direct_pixel = ppu.direct_background_pixel_value(mem, dot);
        let shifter_pixel = ppu.background_shift_pixel_value();
        assert_eq!(
            shifter_pixel, direct_pixel,
            "pixel mismatch at scanline {}, dot {}; v={:04X}, render_line_v={:04X}, low={:04X}, high={:04X}, attr_low={:04X}, attr_high={:04X}",
            ppu.scanline,
            dot,
            ppu.v,
            ppu.render_line_v,
            ppu.bg_pattern_shift_low,
            ppu.bg_pattern_shift_high,
            ppu.bg_attr_shift_low,
            ppu.bg_attr_shift_high
        );

        if direct_pixel != 0 {
            let direct_palette = ppu.direct_background_palette_id(mem, dot);
            let shifter_palette = ppu.background_shift_palette_id();
            assert_eq!(
                shifter_palette, direct_palette,
                "palette mismatch at scanline {}, dot {}",
                ppu.scanline, dot
            );
        }
    }

    #[test]
    fn test_shifter_matches_direct_background_after_prefetch() {
        let mut ppu = PPU::new();
        let mut mem = memory::IdentityMapper::new(0x4000);
        let mut framebuffer = Buffer::new();

        ppu.ppu_mask = MASK_BACKGROUND_ENABLE | MASK_BACKGROUND_LEFT_ENABLE;
        ppu.next_render_line_v = 0;
        seed_background_comparison_pattern(&mut mem);
        prime_visible_scanline(&mut ppu, &mut mem);
        ppu.render_line_v = ppu.next_render_line_v;

        for dot in 1..=256 {
            assert_shifter_matches_direct_background(&ppu, &mem, dot);
            ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        }
    }

    #[test]
    fn test_shifter_matches_direct_background_after_cold_start_scanline() {
        let mut ppu = PPU::new();
        let mut mem = memory::IdentityMapper::new(0x4000);
        let mut framebuffer = Buffer::new();

        ppu.ppu_mask = MASK_BACKGROUND_ENABLE | MASK_BACKGROUND_LEFT_ENABLE;
        seed_background_comparison_pattern(&mut mem);

        for _ in 0..341 {
            ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        }
        ppu.render_line_v = ppu.next_render_line_v;

        for dot in 1..=256 {
            assert_shifter_matches_direct_background(&ppu, &mem, dot);
            ppu.step_dot_with_rendering(&mut mem, &mut framebuffer);
        }
    }

    #[test]
    fn test_fine_x_selects_shift_register_pixel() {
        let mut ppu = PPU::new();
        ppu.bg_pattern_shift_low = 0b1010_0000_0000_0000;
        ppu.bg_pattern_shift_high = 0b0110_0000_0000_0000;
        ppu.bg_attr_shift_low = 0b1100_0000_0000_0000;
        ppu.bg_attr_shift_high = 0b0101_0000_0000_0000;

        let expected_pixels = [1, 2, 3, 0];
        let expected_palettes = [1, 3, 0, 2];
        for fine_x in 0..4 {
            ppu.x = fine_x;
            assert_eq!(
                ppu.background_shift_pixel_value(),
                expected_pixels[fine_x as usize]
            );
            assert_eq!(
                ppu.background_shift_palette_id(),
                expected_palettes[fine_x as usize]
            );
        }
    }

    #[test]
    fn test_sprite_zero_hit_uses_visible_sprite_y() {
        let mut ppu = PPU::new();
        ppu.ppu_mask = MASK_BACKGROUND_ENABLE | MASK_SPRITES_ENABLE;
        ppu.oam_ram[0] = 20;
        ppu.oam_ram[3] = 10;

        ppu.scanline = 20;
        assert_eq!(ppu.sprite_zero_hit(11), false);

        ppu.scanline = 21;
        assert_eq!(ppu.sprite_zero_hit(10), false);
        assert_eq!(ppu.sprite_zero_hit(11), true);
    }

    #[test]
    fn test_sprite_zero_hit_requires_bg_and_sprites() {
        let mut ppu = PPU::new();
        ppu.oam_ram[0] = 20;
        ppu.oam_ram[3] = 10;
        ppu.scanline = 21;

        ppu.ppu_mask = MASK_SPRITES_ENABLE;
        assert_eq!(ppu.sprite_zero_hit(11), false);

        ppu.ppu_mask = MASK_BACKGROUND_ENABLE;
        assert_eq!(ppu.sprite_zero_hit(11), false);

        ppu.ppu_mask = MASK_BACKGROUND_ENABLE | MASK_SPRITES_ENABLE;
        assert_eq!(ppu.sprite_zero_hit(11), true);
    }

    #[test]
    fn test_sprite_eval_copies_in_range_sprite_to_secondary_oam() {
        let mut ppu = PPU::new();
        ppu.ppu_mask = MASK_RENDERING_ENABLE;
        ppu.scanline = 13;
        ppu.oam_ram[0] = 10;
        ppu.oam_ram[1] = 5;
        ppu.oam_ram[2] = 0x21;
        ppu.oam_ram[3] = 20;

        ppu.start_sprite_evaluation();
        ppu.sprite_evaluation_tick();

        assert_eq!(ppu.secondary_oam_count, 1);
        assert_eq!(ppu.secondary_oam[0..4], [10, 5, 0x21, 20]);
        assert!(ppu.sprite_zero_pending_next_line);
    }

    #[test]
    fn test_sprite_eval_overflow_flag_past_eight_sprites() {
        let mut ppu = PPU::new();
        ppu.ppu_mask = MASK_RENDERING_ENABLE;
        ppu.scanline = 20;
        ppu.ppu_status &= !STATUS_SPRITE_OVERFLOW;

        for i in 0..9 {
            let b = i * 4;
            ppu.oam_ram[b] = 19;
            ppu.oam_ram[b + 1] = i as u8;
            ppu.oam_ram[b + 2] = 0;
            ppu.oam_ram[b + 3] = (i * 8) as u8;
        }

        ppu.start_sprite_evaluation();
        for _ in 0..9 {
            ppu.sprite_evaluation_tick();
        }

        assert_eq!(ppu.secondary_oam_count, 8);
        assert!(ppu.ppu_status & STATUS_SPRITE_OVERFLOW != 0);
    }

    #[test]
    fn test_sprite_zero_hit_requires_opaque_sprite_and_background_pixels() {
        let mut ppu = PPU::new();
        let mut mem = memory::IdentityMapper::new(0x4000);

        ppu.ppu_mask = MASK_BACKGROUND_ENABLE | MASK_SPRITES_ENABLE;
        ppu.oam_ram[0] = 20;
        ppu.oam_ram[1] = 2;
        ppu.oam_ram[3] = 10;
        ppu.render_line_v = 0;

        mem.ppu_write(0x2001, 1);
        mem.ppu_write(0x0010, 0x20); // BG tile 1, row 0, x=10 is opaque.
        mem.ppu_write(0x0020, 0x00); // Sprite tile 2, row 0 is transparent.
        mem.ppu_write(0x0021, 0x80); // Sprite tile 2, row 1, x=10 is opaque.

        // `sprite_zero_hit_with_rendering` uses the same BG path as live pixels; prime shifters
        // so the background is opaque without stepping the full fetch pipeline.
        ppu.bg_pattern_shift_low = 0x8000;
        ppu.bg_pattern_shift_high = 0;

        ppu.scanline = 21;
        ppu.sprite_zero_on_current_line = true;
        ppu.sprite_line_count = 1;
        // Scanline 21: first row of sprite — transparent pixel at screen x=10.
        ppu.sprite_line[0] = SpriteLineEntry {
            attr: 0,
            x: 10,
            pattern_lo: 0x00,
            pattern_hi: 0x00,
        };
        assert_eq!(ppu.sprite_zero_hit_with_rendering(&mem, 11), false);

        ppu.scanline = 22;
        ppu.sprite_line[0] = SpriteLineEntry {
            attr: 0,
            x: 10,
            pattern_lo: 0x80,
            pattern_hi: 0x00,
        };
        assert_eq!(ppu.sprite_zero_hit_with_rendering(&mem, 11), true);

        mem.ppu_write(0x0010, 0x00);
        ppu.bg_pattern_shift_low = 0;
        assert_eq!(ppu.sprite_zero_hit_with_rendering(&mem, 11), false);
    }

    #[test]
    fn test_oamdata_glitch_during_rendering_skips_write_and_increment_high_bits() {
        let mut ppu = PPU::new();

        ppu.scanline = 10;
        ppu.oam_addr = 0x08;
        ppu.oam_ram[8] = 0x55;
        ppu.ppu_mask = MASK_BACKGROUND_ENABLE;

        ppu.write(OAM_DATA_ADDR, 0xAB);
        assert_eq!(ppu.oam_ram[8], 0x55);
        assert_eq!(ppu.oam_addr, 0x0C);
    }

    #[test]
    fn test_oamdata_write_normal_when_rendering_off() {
        let mut ppu = PPU::new();

        ppu.scanline = 10;
        ppu.oam_addr = 0x08;
        ppu.ppu_mask = 0;

        ppu.write(OAM_DATA_ADDR, 0xAB);
        assert_eq!(ppu.oam_ram[8], 0xAB);
        assert_eq!(ppu.oam_addr, 0x09);
    }

    #[test]
    fn test_ppu_open_bus_status_merges_low_bits() {
        let mut ppu = PPU::new();
        let mem = memory::IdentityMapper::new(0x4000);
        ppu.ppu_open_bus = 0x1A;
        ppu.ppu_status = STATUS_VERTICAL_BLANK_BIT | STATUS_SPRITE_ZERO_HIT;
        let v = ppu.read(STATUS_REG_ADDR, &mem);
        assert_eq!(v & 0xE0, (STATUS_VERTICAL_BLANK_BIT | STATUS_SPRITE_ZERO_HIT) & 0xE0);
        assert_eq!(v & 0x1F, 0x1A);
        assert_eq!(ppu.ppu_open_bus, v);
    }

    #[test]
    fn test_sprite_priority_behind_background_keeps_bg_color() {
        let mut ppu = PPU::new();
        let mut mem = memory::IdentityMapper::new(0x4000);
        ppu.ppu_mask = MASK_BACKGROUND_ENABLE
            | MASK_SPRITES_ENABLE
            | MASK_BACKGROUND_LEFT_ENABLE
            | MASK_SPRITES_LEFT_ENABLE;
        ppu.x = 0;
        ppu.bg_pattern_shift_low = 0x8000;
        ppu.bg_pattern_shift_high = 0;
        ppu.bg_attr_shift_low = 0;
        ppu.bg_attr_shift_high = 0;
        mem.ppu_write(0x3f00, 0x0F);
        mem.ppu_write(0x3f01, 0x20);
        mem.ppu_write(0x3f11, 0x10);

        ppu.sprite_line_count = 1;
        ppu.sprite_line[0] = SpriteLineEntry {
            attr: 0x21, // palette 1 + behind background
            x: 8,
            pattern_lo: 0xFF,
            pattern_hi: 0xFF,
        };
        ppu.cycle = 9;
        let bg_only = ppu.render_pixel(&mem);

        ppu.sprite_line[0].attr = 0x01; // in front
        let sprite_over = ppu.render_pixel(&mem);
        assert_ne!(bg_only, sprite_over);
        assert_eq!(bg_only, palette::PALETTE[0x20 as usize % palette::PALETTE_SIZE]);
    }

    #[test]
    fn test_sprite_horizontal_flip_samples_mirrored_column() {
        let mut ppu = PPU::new();
        let mut mem = memory::IdentityMapper::new(0x4000);
        ppu.ppu_mask = MASK_SPRITES_ENABLE | MASK_SPRITES_LEFT_ENABLE;
        mem.ppu_write(0x3f00, 0x0E);
        mem.ppu_write(0x3f01, 0x20);
        mem.ppu_write(0x3f11, 0x10);

        // 0x0F: opaque on the low bits (right side of unflipped tile); column 0 reads bit 7 = 0.
        ppu.sprite_line_count = 1;
        ppu.sprite_line[0] = SpriteLineEntry {
            attr: 0,
            x: 0,
            pattern_lo: 0x0F,
            pattern_hi: 0,
        };
        ppu.cycle = 1;
        let no_flip = ppu.render_pixel(&mem);

        ppu.sprite_line[0].attr = 0x40;
        let flipped = ppu.render_pixel(&mem);
        assert_eq!(no_flip, palette::PALETTE[0x0E as usize % palette::PALETTE_SIZE]);
        assert_eq!(flipped, palette::PALETTE[0x10 as usize % palette::PALETTE_SIZE]);
    }

    #[test]
    fn test_8x16_sprite_pattern_addr_selects_bank_by_tile_lsb() {
        let mut ppu = PPU::new();
        ppu.ppu_ctrl = CTRL_SPRITE_SIZE;
        let tile_base: u16 = 0x2A;
        let row_lo = 4u16;
        let addr_top_half = ppu.sprite_pattern_addr_for(0x2B, row_lo, 16);
        assert_eq!(addr_top_half, 0x1000 + tile_base * 16 + row_lo);
        let row_hi = 11u16;
        let addr_bottom_half = ppu.sprite_pattern_addr_for(0x2B, row_hi, 16);
        assert_eq!(
            addr_bottom_half,
            0x1000 + (tile_base + 1) * 16 + (row_hi % 8)
        );
    }

    #[test]
    fn test_sprite_overflow_false_positive_off_scanline_y_diagonal_phase() {
        // Nine entries at Y=19, but index 8 uses a bogus byte at +m so step-3 sees an in-range Y
        // where a naïve model would not (hardware "diagonal" OAM walk).
        let mut ppu = PPU::new();
        ppu.ppu_mask = MASK_RENDERING_ENABLE;
        ppu.scanline = 20;
        ppu.ppu_status &= !STATUS_SPRITE_OVERFLOW;

        for i in 0..8 {
            let b = i * 4;
            ppu.oam_ram[b] = 19;
            ppu.oam_ram[b + 1] = i as u8;
            ppu.oam_ram[b + 2] = 0;
            ppu.oam_ram[b + 3] = (i * 8) as u8;
        }
        ppu.oam_ram[8 * 4] = 19;
        ppu.oam_ram[8 * 4 + 1] = 9;
        ppu.oam_ram[8 * 4 + 2] = 0;
        ppu.oam_ram[8 * 4 + 3] = 64;
        // Corrupt OAM byte that diagonal fetch reads as a Y coordinate for an earlysprite index.
        ppu.oam_ram[9] = 19;

        ppu.start_sprite_evaluation();
        for _ in 0..128 {
            ppu.sprite_evaluation_tick();
        }
        assert!(ppu.secondary_oam_count <= 8);
        assert!(ppu.ppu_status & STATUS_SPRITE_OVERFLOW != 0);
    }

    #[test]
    fn test_status_read_at_vblank_dot_0_requests_nmi_suppression() {
        let mut ppu = PPU::new();
        let mem = memory::IdentityMapper::new(0);
        ppu.scanline = VBLANK_SCANLINE;
        ppu.cycle = 0;

        ppu.read(STATUS_REG_ADDR, &mem);
        assert!(ppu.nmi_suppress_next_vblank);
    }
}
