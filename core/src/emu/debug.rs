use super::apu::ApuDebugState;
use super::cpu::disasm::DisasmLine;
use super::gfx::palette;
use super::memory::MemoryMapper;
use super::ppu;

pub const DISASM_CONTEXT: usize = 6;

pub struct CpuSnapshot {
    pub pc: u16,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub status: u8,
    pub cycle: u64,
}

pub struct PpuSnapshot {
    pub ctrl: u8,
    pub mask: u8,
    pub status: u8,
    pub v: u16,
    pub t: u16,
    pub fine_x: u8,
    pub scanline: u16,
    pub dot: u16,
    pub frame: u64,
    pub scroll_x: u16,
    pub scroll_y: u16,
    pub nametable_select: u8,
}

pub struct SpriteInfo {
    pub index: u8,
    pub x: u8,
    pub y: u8,
    pub tile: u8,
    pub attr: u8,
    pub pixels: Vec<u8>,
    pub width: u8,
    pub height: u8,
}

pub struct NametableImage {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct DebugSnapshot {
    pub cpu: CpuSnapshot,
    pub ppu: PpuSnapshot,
    pub apu: ApuDebugState,
    pub disasm: Vec<DisasmLine>,
    pub disasm_pc_index: usize,
    pub palette: [u8; 32],
    pub stack: Vec<u8>,
    pub oam: [u8; ppu::OAM_DATA_SIZE],
    pub sprites: Vec<SpriteInfo>,
    pub nametables: Vec<NametableImage>,
    pub pattern_tables: [NametableImage; 2],
}

pub fn render_sprites(
    oam: &[u8; ppu::OAM_DATA_SIZE],
    ppu: &ppu::PPU,
    mem: &dyn MemoryMapper,
) -> Vec<SpriteInfo> {
    let sprite_height = ppu.ctrl_sprite_size();
    let is_8x16 = sprite_height == 16;
    let pattern_base = ppu.ctrl_sprite_pattern_table_addr();

    let mut sprites = Vec::new();
    for i in 0..64 {
        let base = i * 4;
        let y = oam[base];
        if y >= 0xF0 {
            continue;
        }
        let tile = oam[base + 1];
        let attr = oam[base + 2];
        let x = oam[base + 3];
        let palette_id = attr & 0x03;
        let flip_h = attr & 0x40 != 0;
        let flip_v = attr & 0x80 != 0;

        let h = sprite_height as usize;
        let mut pixels = vec![0u8; 8 * h * 3];

        for row in 0..h {
            let actual_row = if flip_v { h - 1 - row } else { row };
            let (pat_addr_lo, pat_addr_hi) = if is_8x16 {
                let bank = if tile & 1 != 0 { 0x1000u16 } else { 0 };
                let tile_base = u16::from(tile & 0xFE);
                let tile_offset = if actual_row >= 8 { 1 } else { 0 };
                let fine_row = (actual_row % 8) as u16;
                let addr = bank + (tile_base + tile_offset) * 16 + fine_row;
                (addr, addr + 8)
            } else {
                let fine_row = actual_row as u16;
                let addr = pattern_base + u16::from(tile) * 16 + fine_row;
                (addr, addr + 8)
            };

            let lo = mem.ppu_read(pat_addr_lo);
            let hi = mem.ppu_read(pat_addr_hi);

            for col in 0..8 {
                let actual_col = if flip_h { col } else { 7 - col };
                let bit_lo = (lo >> actual_col) & 1;
                let bit_hi = (hi >> actual_col) & 1;
                let color_idx = bit_lo | (bit_hi << 1);

                let pal_addr = 0x3F10 + u16::from(palette_id) * 4 + u16::from(color_idx);
                let nes_color = if color_idx == 0 {
                    mem.ppu_read(0x3F00) as usize
                } else {
                    mem.ppu_read(pal_addr) as usize
                } % palette::PALETTE_SIZE;

                let (r, g, b) = palette::PALETTE[nes_color];
                let px_col = if flip_h { 7 - col } else { col } as usize;
                let px = (row * 8 + px_col) * 3;
                pixels[px] = r;
                pixels[px + 1] = g;
                pixels[px + 2] = b;
            }
        }

        sprites.push(SpriteInfo {
            index: i as u8,
            x,
            y,
            tile,
            attr,
            pixels,
            width: 8,
            height: sprite_height,
        });
    }
    sprites
}

const NT_TILE_COLS: u32 = 32;
const NT_TILE_ROWS: u32 = 30;
const NT_PX_W: u32 = NT_TILE_COLS * 8;
const NT_PX_H: u32 = NT_TILE_ROWS * 8;

fn read_chr(addr: u16, chr_snapshot: Option<&[u8]>, mem: &dyn MemoryMapper) -> u8 {
    if let Some(chr) = chr_snapshot {
        if (addr as usize) < chr.len() {
            return chr[addr as usize];
        }
    }
    mem.ppu_read(addr)
}

pub fn render_nametable(
    nt_base: u16,
    ppu: &ppu::PPU,
    mem: &dyn MemoryMapper,
    chr_snapshot: Option<&[u8]>,
) -> NametableImage {
    let pattern_base = ppu.ctrl_background_pattern_addr();
    let mut pixels = vec![0u8; (NT_PX_W * NT_PX_H * 3) as usize];

    for tile_row in 0..NT_TILE_ROWS {
        for tile_col in 0..NT_TILE_COLS {
            let nt_addr = nt_base + (tile_row * NT_TILE_COLS + tile_col) as u16;
            let tile_id = mem.ppu_read(0x2000 + (nt_addr & 0x0FFF)) as u16;

            let attr_addr = nt_base + 0x03C0 + (tile_row / 4 * 8 + tile_col / 4) as u16;
            let attr_byte = mem.ppu_read(0x2000 + (attr_addr & 0x0FFF));
            let shift = ((tile_row / 2) % 2 * 2 + (tile_col / 2) % 2) * 2;
            let palette_id = (attr_byte >> shift) & 0x03;

            for row in 0..8u16 {
                let addr = pattern_base + tile_id * 16 + row;
                let lo = read_chr(addr, chr_snapshot, mem);
                let hi = read_chr(addr + 8, chr_snapshot, mem);

                for col in 0..8u16 {
                    let bit = 7 - col;
                    let color_idx = ((lo >> bit) & 1) | (((hi >> bit) & 1) << 1);

                    let pal_addr = 0x3F00 + u16::from(palette_id) * 4 + u16::from(color_idx);
                    let nes_color = if color_idx == 0 {
                        mem.ppu_read(0x3F00) as usize
                    } else {
                        mem.ppu_read(pal_addr) as usize
                    } % palette::PALETTE_SIZE;

                    let (r, g, b) = palette::PALETTE[nes_color];
                    let px_x = tile_col * 8 + col as u32;
                    let px_y = tile_row * 8 + row as u32;
                    let px = ((px_y * NT_PX_W + px_x) * 3) as usize;
                    pixels[px] = r;
                    pixels[px + 1] = g;
                    pixels[px + 2] = b;
                }
            }
        }
    }

    NametableImage {
        pixels,
        width: NT_PX_W,
        height: NT_PX_H,
    }
}

pub fn render_all_nametables(
    ppu: &ppu::PPU,
    mem: &dyn MemoryMapper,
    chr_snapshot: Option<&[u8]>,
) -> Vec<NametableImage> {
    vec![
        render_nametable(0x000, ppu, mem, chr_snapshot),
        render_nametable(0x400, ppu, mem, chr_snapshot),
        render_nametable(0x800, ppu, mem, chr_snapshot),
        render_nametable(0xC00, ppu, mem, chr_snapshot),
    ]
}

const PT_TILES: u32 = 16;
const PT_PX: u32 = PT_TILES * 8;

pub fn render_pattern_table(
    base: u16,
    palette_ram: &[u8; 32],
    mem: &dyn MemoryMapper,
    chr_snapshot: Option<&[u8]>,
) -> NametableImage {
    let mut pixels = vec![0u8; (PT_PX * PT_PX * 3) as usize];

    for tile_row in 0..PT_TILES {
        for tile_col in 0..PT_TILES {
            let tile_idx = tile_row * PT_TILES + tile_col;
            for row in 0..8u16 {
                let addr = base + tile_idx as u16 * 16 + row;
                let lo = read_chr(addr, chr_snapshot, mem);
                let hi = read_chr(addr + 8, chr_snapshot, mem);

                for col in 0..8u16 {
                    let bit = 7 - col;
                    let color_idx = ((lo >> bit) & 1) | (((hi >> bit) & 1) << 1);
                    let nes_color = if color_idx == 0 {
                        palette_ram[0] as usize
                    } else {
                        palette_ram[color_idx as usize] as usize
                    } % palette::PALETTE_SIZE;
                    let (r, g, b) = palette::PALETTE[nes_color];
                    let px_x = tile_col * 8 + col as u32;
                    let px_y = tile_row * 8 + row as u32;
                    let px = ((px_y * PT_PX + px_x) * 3) as usize;
                    pixels[px] = r;
                    pixels[px + 1] = g;
                    pixels[px + 2] = b;
                }
            }
        }
    }

    NametableImage {
        pixels,
        width: PT_PX,
        height: PT_PX,
    }
}
