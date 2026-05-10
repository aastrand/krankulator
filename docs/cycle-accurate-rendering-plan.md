# Cycle-Accurate PPU Rendering Plan

## Problem Statement

**Historical:** Krankulator used to composite the full frame at VBlank via `gfx::render()`,
so mid-scanline register effects did not appear until the next frame.

**Today:** Dot-stepped PPU output + CPU catch-up (see **Implementation status** below). The
paragraphs that follow in this document are mostly the **original migration checklist**, kept
for design context.

## Implementation status (2026)

| Phase | Status |
|-------|--------|
| **1** — Master clock, owned PPU/APU, bus timing, catch-up | **Done** |
| **2** — Scanline buffer, background in PPU tick | **Done** |
| **3** — BG fetch pipeline, shift registers, MMC3 A12 from `ppu_fetch` | **Done** (live BG from shifters; see caveat below) |
| **4** — Sprite eval, fetches, compositing, sprite 0 hit, overflow quirk | **Done** (functional model; see caveat below) |
| **5** — Open bus, warm-up, MMC3 edge cases, etc. | **Partial** (OAMDATA glitch, vblank/NMI suppression, PPUSTATUS clear semantics done) |

**Phase 3 caveat:** Visible pixels normally use the background shift registers and fine-X mux.
After a **second `$2006` write** mid-visible-line, `render_line_v` is realigned but the
shifters still hold older tile data until the pipeline reloads; the core sets
`bg_tile_lookup_direct_this_scanline` until dot 256 so output matches raster tests and common
mid-line VRAM address tricks.

**Phase 4 caveat:** Sprites use **latched pattern bytes** in `sprite_line` (equivalent output to
shift-register units for 8-pixel-wide patterns). There is no separate per-sprite shift-register
array. Optional follow-up: add targeted tests (priority, flip, 8×16, overflow false-positive ROMs).

## Current Architecture

### What we have

```
CPU executes instruction
  → master_clock += 3; APU steps
  → On PPU register / mapper access: sync PPU to bus timestamp (catch-up loop of step_dot)
  → PPU step_dot_with_rendering (visible lines):
      dots 1–256: BG from shift registers + sprite_line mux → scanline_buf → gfx::Buffer
      BG fetch pipeline + sprite eval (odd 65–255) + sprite fetches 257–320 + prefetch 321–336
  → VBlank NMI; display via iohandler.render(&buf) — no gfx::render() frame composite
```

### Key files

| File | Role |
|------|------|
| `emu/mod.rs` | `master_clock`, catch-up, `cpu_read`/`cpu_write`, owned `ppu` + `apu` |
| `emu/ppu/mod.rs` | Dot FSM, shift registers, sprites, `$2002` / OAM / scroll behavior |
| `emu/gfx/buf.rs` | 256×240 RGB framebuffer; `set_pixel` / test-only `get_pixel` |
| `emu/gfx/palette.rs` | NES palette |
| `emu/memory/mod.rs` | `MemoryMapper`, `cpu_maps_ppu_registers`, `IdentityMapper::new_flat_cpu_bus` (.bin tests) |
| `emu/memory/mapper/mmc3.rs` | IRQ from filtered A12 edges on `ppu_fetch` |

### Still open (accuracy / polish)

- Phase 5 items: PPU open bus, full warm-up, MMC3 vs 8×16 smoke on real carts
- MMC3-class games: IRQ + timing may still need game-driven verification
- Optional: sprite pipeline tests and formal per-slot shift registers if desired

## Target Architecture

### Synchronization model: catch-up

The **catch-up** model is the best fit for an interpreted emulator:

- CPU is the master — runs instruction by instruction, incrementing a master cycle counter
- PPU runs lazily — only catches up to the CPU's current position when needed
- Catch-up is triggered by: CPU reads/writes to PPU registers ($2000-$2007), mapper
  register writes, and predicted events (VBlank NMI, sprite 0 hit scanline, mapper IRQs)

This keeps hot CPU data in L1 cache during instruction bursts, while still producing
cycle-accurate pixel output. Used by Mesen and most high-quality emulators.

### Timing invariants

The migration must keep one unambiguous timing model:

- `master_clock` is measured in PPU dots, not CPU cycles
- `PPU::step_dot()` advances exactly one PPU dot
- The catch-up loop calls `step_dot()` in a tight single-dot loop
- The old 3-dots-per-CPU-cycle stepping path is retired; compatibility helpers may exist
  only outside catch-up and must be thin wrappers around `step_dot()`
- CPU reads/writes to PPU or mapper registers must catch the PPU up to the exact bus-cycle
  timestamp of the access, not merely to the end of the CPU instruction
- NMI and IRQ edges must be scheduled so the CPU cannot execute past an interrupt becoming
  observable while the PPU is still lazily behind
- Mapper IRQ logic must observe the same PPU memory fetches as rendering; scanline-only
  shortcuts are temporary compatibility scaffolding
- APU timing remains CPU-driven. The PPU catch-up refactor must not move audio stepping
  onto the lazy PPU clock.

### Per-dot rendering pipeline

Each visible dot (0-255 on scanlines 0-239) outputs one pixel. The PPU runs a state
machine keyed on `(scanline_type, dot)`:

```
Visible scanlines (0-239):
  Dots 1-256:   Fetch BG tiles (4 fetches per 8 dots), shift registers output pixels
                 Sprite evaluation runs in parallel (secondary OAM fill)
  Dot 257:       Horizontal scroll bits t → v
  Dots 257-320:  Fetch sprite tile data for next scanline (8 sprites × 8 dots)
  Dots 321-336:  Pre-fetch first 2 tiles of next scanline
  Dots 337-340:  Two dummy nametable fetches

Post-render (240):   Idle
VBlank (241-260):    Set VBlank at dot 1 of 241; fire NMI if enabled
Pre-render (261):    Like visible but no pixel output
                     Dots 280-304: vertical scroll bits t → v
```

### Shift register model

New PPU fields for the tile pipeline:

```rust
bg_shift_low: u16,     // BG pattern low bitplane (shifts left each dot)
bg_shift_high: u16,    // BG pattern high bitplane
at_shift_low: u8,      // Attribute low bit
at_shift_high: u8,     // Attribute high bit
at_latch_low: bool,    // Attribute latch (feeds shift reg every 8 dots)
at_latch_high: bool,

nt_byte: u8,           // Fetched nametable byte (tile ID)
at_byte: u8,           // Fetched attribute byte
pt_low: u8,            // Fetched pattern table low byte
pt_high: u8,           // Fetched pattern table high byte
```

Every 8 dots: load fetched tile data into the upper 8 bits of the shift registers.
Every dot: shift registers left by 1; fine X selects the output bit position.

### Sprite rendering

```rust
secondary_oam: [u8; 32],         // Up to 8 sprites for next scanline
sprite_shift_low: [u8; 8],       // Sprite pattern low bitplane
sprite_shift_high: [u8; 8],      // Sprite pattern high bitplane
sprite_attr: [u8; 8],            // Sprite attributes (flip, priority, palette)
sprite_x_counter: [u8; 8],       // X position countdown
sprite_count: u8,                // Sprites found for this scanline
sprite_zero_on_line: bool,       // Whether sprite 0 is in secondary OAM
```

Sprite evaluation (dots 65-256): scan primary OAM, copy in-range sprites to secondary
OAM (max 8). Must emulate the diagonal-scan overflow bug.

Sprite rendering (dots 1-256, using data fetched on previous scanline): decrement X
counters; when counter reaches 0, start shifting out pixels. Multiplex with BG output
using priority bits.

Sprite 0 hit: set when sprite and background rendering are enabled and sprite 0's opaque
pixel overlaps an opaque background pixel. It must not fire at x=255, and it must respect
the separate PPUMASK left-column clipping bits for background and sprites.

## Migration Plan

### Phase 1: Master clock, bus timing, and ownership

**Status — complete.** Includes `IdentityMapper::new_flat_cpu_bus` / `cpu_maps_ppu_registers` for
binary CPU test ROMs (e.g. Klaus) where `$2000–$2007` must be RAM.

**Goal** (historical): Establish the timing foundation and ownership model before changing rendering
output.

1. Add a `master_clock: u64` counter to the emulator (counts in PPU dots)
2. Add `ppu.last_synced_dot: u64` to track how far the PPU has been advanced
3. Split the PPU clock API:
   - `PPU::step_dot()` advances exactly one PPU dot
   - the old bulk `PPU::cycle(num_cycles)` path is retired or reduced to a test-only
     helper around repeated `step_dot()` calls
4. Implement `ppu.catch_up_to(target_dot: u64)` — runs `PPU::step_dot()` from
   `last_synced_dot` to `target_dot`
5. Refactor CPU memory access calls to carry an explicit bus-cycle offset:
   - keep the CPU instruction interpreter instruction-oriented
   - annotate each CPU read/write with the CPU-cycle offset where the bus access occurs
   - compute the access timestamp as `instruction_start_dot + cpu_cycle_offset * 3`
   - avoid a full CPU micro-step interpreter unless the offset approach proves too brittle
6. Treat this as a significant CPU-memory interface change: every `cpu_read`/`cpu_write`
   path that can reach PPU registers or mapper registers must receive or derive the access
   timestamp
7. Replace `Rc<RefCell<PPU>>` in the mapper/memory hot path:
   - `Emulator` owns `PPU` and memory/mapper state separately
   - CPU memory methods receive `&mut PPU` plus the bus timestamp when they need to touch
     PPU registers
   - PPU rendering methods receive a mutable PPU bus/mapper reference for nametable,
     pattern-table, palette, and mapper IRQ side effects
   - no per-dot PPU path should borrow through `Rc<RefCell<>>`
8. Insert catch-up calls before every CPU read/write to $2000-$2007 and before mapper
   register writes
9. Add event prediction for VBlank NMI and mapper IRQ edges so the CPU execution loop stops
   and catches up before an interrupt can be sampled
10. Keep APU stepping CPU-driven in the main loop; this phase should not change audio
    timing

Phase 1 is not guaranteed to be perfectly behavior-preserving. Splitting the bulk PPU
advance into `step_dot()` will make existing scroll updates run at their intended dots
instead of scanline boundaries. That can change games that accidentally worked with the
old approximation, but it is a required accuracy fix.

**Tests**:
- `test_master_clock_advances_3x_cpu` — after one CPU cycle, master clock is +3
- `test_catch_up_idempotent` — calling catch_up_to with current position is a no-op
- `test_catch_up_advances_ppu_state` — PPU scanline/dot match expected values after
  catch-up to a known dot count
- `test_register_read_triggers_catch_up` — reading $2002 advances PPU to current clock
- `test_register_write_uses_bus_cycle_timestamp` — a mid-instruction $2005/$2006 write
  synchronizes to the bus access dot, not the instruction end
- `test_cpu_access_offsets_match_instruction_timing` — representative addressing modes
  timestamp their reads/writes at the expected CPU-cycle offsets
- `test_predicted_nmi_forces_catch_up` — CPU execution stops before sampling an NMI whose
  edge occurs while the PPU is lazily behind
- `test_apu_timing_unchanged_by_ppu_catch_up` — APU frame/audio counters still advance on
  the CPU schedule
- All existing PPU tests must still pass (regression gate)

### Phase 2: Scanline buffer and background rendering in PPU

**Status — complete.** Background is drawn dot-by-dot into `scanline_buf` / framebuffer during
`step_dot_with_rendering`; `gfx::render()` / `render_background` frame composite removed.

**Goal** (historical): Move background rendering from `gfx::render()` into the PPU tick, one scanline
at a time. This is the critical step that unlocks raster effects.

1. Add a `scanline_buf: [u8; 256]` (or `[(u8,u8,u8); 256]`) to PPU for the current
   scanline's pixel output
2. Make `PPU::step_dot()` accept the PPU bus/mapper access it needs for rendering, rather
   than storing an `Rc<RefCell<>>` internally
3. In `PPU::step_dot()`, on visible scanlines (0-239), dots 1-256:
   - Compute background pixel using current v register and fine X
   - Read nametable, attribute, pattern, and palette data through the PPU bus/mapper
     reference passed into the dot step
   - Write pixel to `scanline_buf[dot - 1]`
4. At dot 256 (or end of visible dots), copy `scanline_buf` into the framebuffer
5. Remove `gfx::render_background()` — background is now rendered inline
6. ~~Keep `gfx::render_sprites()` temporarily~~ — superseded by Phase 4 in-PPU sprites
7. ~~Approximate sprite 0 hit~~ — superseded by Phase 4 pixel overlap
8. Verify: CPU-timed mid-frame scroll changes should now render correctly for the
   background layer

Phase 2 was **transitional** until Phase 3 wired fetches/shifters; the note below applied during
that window only:

_It sampled VRAM at pixel time rather than at fetch dots; Phase 3 fixed that for BG._

**Tests**:
- `test_scanline_produces_256_pixels` — a visible scanline fills the buffer
- `test_bg_disabled_outputs_backdrop` — with PPUMASK background off, every pixel is the
  universal background color ($3F00)
- `test_scroll_change_mid_frame` — write to $2005 at scanline 100, verify pixels above
  and below the split use different scroll offsets
- `test_framebuffer_matches_old_renderer` — render a known nametable + pattern table
  setup with both old and new paths, compare pixel output (transitional test, removed
  after phase 4)

**Old code removed**:
- `gfx::render_background()` (gfx/mod.rs)
- `gfx::render()` call path that invokes background rendering
- Helper functions only used by old background renderer (if any become orphaned)

### Phase 3: Shift registers and proper tile fetch pipeline

**Status — complete** for the goals below. **Live** background pixels use **`background_shift_pixel_value` /
`background_shift_palette_id`** with fine-X. **Exception:** after a second **`$2006`** write during a
visible scanline (while BG/sprites enabled), **`bg_tile_lookup_direct_this_scanline`** selects the
direct nametable/pattern path until dot 256 so VRAM-address splits align with hardware-ish behavior
and unit tests. Mapper **MMC3** IRQ clocking uses **A12 edges** from **`ppu_fetch`** (filtered by dot).

**Goal** (historical): Replace ad hoc per-pixel VRAM reads with the fetch/shift pipeline.

1. Add shift register fields to PPU (listed above)
2. Implement the 4-step fetch cycle (NT → AT → PT low → PT high) every 8 dots
3. Load fetched data into shift registers at tile boundaries
4. Output pixels by selecting bits from shift registers using fine-X (plus `$2006` direct
   fallback for the rest of the scanline when noted above)
5. Implement pre-fetch (dots 321-336) and dummy fetches (337-340)
6. Drive mapper A12/IRQ observation from actual PPU pattern-table fetches (`ppu_fetch` on MMC3)
7. Verify: fine-X scrolling should be pixel-perfect; no visual regression on games
   that worked before *(MMC3-class titles may still need game-by-game smoke — Phase 5 / QA)*

**Tests**:
- `test_shift_register_loads_at_tile_boundary` — after 8 dots, new tile data is loaded
  into the upper bits of the shift registers
- `test_fine_x_selects_shift_register_pixel` — with fine X = 0..7, the correct bit is
  selected from the shift register
- `test_tile_fetch_sequence` — verify the 4 memory accesses happen at dots N+0, N+2,
  N+4, N+6 within each 8-dot window
- `test_prefetch_dots_321_336_seed_visible_shifters` — first two tiles of next scanline
  are fetched during dots 321-336
- Shifter-vs-direct consistency: `assert_shifter_matches_direct_background` and related cases
- `test_mmc3_a12_edges_from_filtered_ppu_fetches` / related MMC3 unit coverage

### Phase 4: Sprite evaluation and rendering in PPU

**Status — functionally complete.** Secondary OAM; evaluation on **odd cycles 65–255**; after eight
in-range sprites, **overflow detection** follows NESdev **step 3** (diagonal OAM / false positives).
Sprite fetches **257–320**; **`sprite_line`** compositing (flip, priority); pixel-level **sprite 0 hit**
using the same BG source as pixels. **`gfx::render_sprites`** / frame `gfx::render()` removed.

**Optional follow-ups**
- Dedicated **per-sprite shift register** state (currently **latched pattern bytes + X** per slot, output-equivalent for 8-wide sprites).
- Extra ROM/unit tests: priority, flip, 8×16, overflow edge cases from checklist below.

**Goal** (historical): Cycle-accurate sprite handling, proper sprite 0 hit, and correct priority.

**Original task checklist** (for reference):

1. Add secondary OAM and sprite shift register fields to PPU
2. Implement sprite evaluation on dots 65-256 (scan OAM, fill secondary OAM)
3. Implement sprite tile fetches on dots 257-320
4. On the next scanline, render sprite pixels alongside background:
   - Decrement X counters; shift out sprite pixels when counter reaches 0
   - Multiplex with BG pixel using priority bit
5. Implement pixel-level sprite 0 hit detection
6. Implement sprite overflow with the diagonal-scan bug
7. Remove `gfx::render_sprites()` entirely
8. Verify: sprite 0 hit games (SMB status bar polling), sprite priority, 8-sprite
   flicker should all work correctly

**Tests**:
- `test_sprite_eval_fills_secondary_oam` — place sprites at known Y positions, advance
  to a matching scanline, verify secondary OAM contains the right entries
- `test_sprite_eval_max_8` — with >8 sprites on a scanline, only 8 are copied and
  overflow flag is set
- `test_sprite_zero_hit_pixel_overlap` — set up sprite 0 and BG to overlap at a known
  dot, verify the hit flag is set at the correct cycle
- `test_sprite_zero_no_hit_transparent` — sprite 0 pixel is color 0 (transparent), no
  hit even if BG is opaque
- `test_sprite_zero_no_hit_at_x255` — sprite at x=255 never triggers hit
- `test_sprite_priority_behind_bg` — sprite with priority bit set only shows where BG
  is transparent
- `test_sprite_horizontal_flip` — verify flipped sprite pixels are mirrored
- `test_sprite_vertical_flip` — same for vertical
- `test_8x16_sprites` — correct tile selection and rendering for tall sprites
- `test_sprite_overflow_bug` — the diagonal-scan hardware bug produces the expected
  (incorrect) result

**Old code removed** (historical): `gfx::render()`, `gfx::render_sprites()`, and frame-wide
background helpers; composition lives in `PPU::step_dot_with_rendering`. Remaining `gfx/`
is framebuffer + shared palette helpers.

### Phase 5: Edge cases and accuracy refinements

**Goal**: Handle hardware quirks that affect specific games.

**Done in tree**
- **OAMDATA glitch** during rendering (visible + pre-render when either BG or sprites enabled): no OAM write; `OAMADDR` advances coarse (+4 / high 6 bits).
- **VBlank / NMI**: reading `$2002` on **scanline 241, dot 0** sets a latch that **suppresses** the NMI edge when vblank is latched at dot 1 (simplified; open-bus and exact 1-cycle edges not modeled).
- **PPUSTATUS (`$2002`)**: read clears **vblank (bit 7)** only; **sprite 0 hit** and **sprite overflow** are **not** cleared on read — they clear at **pre-render dot 1** together with vblank if still set (NESdev).

**Still open (numbered items from original plan)**
1. PPU open bus behavior for register reads
2. Odd frame cycle skip (already partially implemented)
3. Confirm MMC3 A12 tracking in 8x16 sprite mode, now that fetch-edge tracking exists
4. PPU warm-up: first 29658 CPU cycles after reset, PPU ignores writes to some registers

**Tests**:
- `test_vblank_race_read_suppresses_nmi` — read $2002 on exact VBlank dot, verify NMI
  does not fire
- `test_oamdata_glitch_during_rendering` — write to $2003 during visible scanline,
  verify OAMADDR increments only high 6 bits
- `test_odd_frame_skip` — with rendering enabled, odd frames are 89341 dots, even
  frames are 89342
- `test_mmc3_a12_scanline_counter` — verify IRQ fires after the programmed number of
  scanlines

### Regression testing

Throughout all phases, the following must pass continuously:

- `cargo test` — all existing unit tests (CPU, APU, memory, mappers)
- `test_nestest` integration test — CPU accuracy baseline
- Any blargg PPU test ROMs that currently pass must continue to pass
- Visual smoke test: SMB1 title screen + first level must look correct after each phase

## Risk Assessment

- **Phases 1–4** are implemented; residual risk is **game-level verification** (especially MMC3
  IRQ + raster splits) and **optional** model refinements (sprite shift registers, open bus).
- **Phase 5** is lower architectural risk but still matters for failing test ROMs and edge-case
  titles that depend on open bus, reset warm-up, or exact MMC3 + 8×16 behavior.

## Performance Considerations

- **Catch-up batching**: the PPU inner loop should be a tight `while dot < target { ... }`
  with no CPU interaction checks — all the speed benefit comes from running the PPU in
  bursts
- **Pre-decode tiles**: convert planar NES tile format to packed pixels when CHR banks
  change, not on every access
- **Scanline buffer in L1**: the 256-byte scanline buffer and shift registers should fit
  comfortably in L1 cache
- **Predict sprite 0 hit**: since sprite 0's Y is known, only do per-pixel overlap checks
  on the relevant scanline
- **Bounds check elision**: structure array accesses so Rust can prove bounds at compile
  time, avoiding runtime checks in the hot loop
- **Avoid `Rc<RefCell<>>` in the hot path** — **done**; `Emulator` owns `PPU` / `APU`, mappers use `cpu_ram_ptr` for DMA.

## Reference Implementations to Study

| Emulator | Language | Architecture | Notes |
|----------|----------|-------------|-------|
| [Mesen2](https://github.com/SourMesen/Mesen2) | C++/C# | Dot-by-dot, catch-up | Gold standard for accuracy; study `PPU.cpp` |
| [TetaNES](https://github.com/lukeworks-tech/tetanes) | Rust | Catch-up w/ Clocked trait | Good Rust patterns for shared mapper state |
| [LaiNES](https://github.com/AndreaOrru/LaiNES) | C++ | Dot-by-dot | Compact, readable PPU (~283 lines) |
| [Nesium](https://github.com/mikai233/nesium) | Rust | Cycle-accurate | Structured around NESdev Wiki docs |
| [Kyle Wlacy's emu](https://kyle.space/posts/i-made-a-nes-emulator/) | Rust | Generator/coroutine per cycle | Novel architecture, very clear timing code |

## Test Games for Raster Effects

| Game | Effect | What to verify |
|------|--------|---------------|
| Super Mario Bros. | Status bar via sprite 0 hit | Score/coin area stays fixed while scrolling |
| Mega Man 2/3 | MMC3 IRQ split screen | Health bar area doesn't scroll with gameplay |
| Castlevania III | MMC3 IRQ + mid-frame scroll | Status bar + parallax scrolling |
| Super Mario Bros. 3 | MMC3 IRQ + palette changes | Status bar, world map |
| Battletoads | Fine scroll + mid-frame palette | Pause screen, level transitions |
| Rad Racer | Mid-scanline scroll (rare) | 3D road perspective effect |
