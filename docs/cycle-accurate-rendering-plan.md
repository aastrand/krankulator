# Cycle-Accurate PPU Rendering Plan

## Problem Statement

Krankulator currently renders the entire frame once per VBlank. The PPU state machine
(`ppu/mod.rs`) accurately tracks cycles, scanlines, and scroll register updates, but
the actual pixel output is produced by `gfx::render()` — a separate function that reads
scroll state once and composites the full 256x240 framebuffer at the end of the frame.

This means mid-frame PPU register writes (scroll changes, palette swaps, bank switches)
have no visual effect until the next frame. Games that rely on raster effects — split-screen
scrolling, scanline-timed IRQs, sprite 0 hit polling for status bars — cannot render correctly.

## Current Architecture

### What we have

```
CPU executes instruction
  → Emulator::cycle() calls PPU::cycle()
    → PPU advances 3 dots per call (correct 3:1 ratio)
    → PPU updates v/t/x/w registers at correct cycle points
    → PPU checks sprite 0 hit (approximate: Y/X position only)
    → PPU detects VBlank at scanline 241
  → On VBlank NMI:
    → gfx::render() reads current scroll state ONCE
    → Renders all 32x30 background tiles to framebuffer
    → Renders all 64 sprites on top
    → Submits framebuffer to display
```

### Key files

| File | Role |
|------|------|
| `ppu/mod.rs` | PPU state machine: registers, cycle counting, scroll updates |
| `gfx/mod.rs` | Frame rendering: background tiles, sprites, composition |
| `gfx/buf.rs` | 256x240 RGB framebuffer |
| `gfx/palette.rs` | 64-entry NES color palette |
| `mod.rs` | Main emulation loop, VBlank trigger, render dispatch |
| `memory/mod.rs` | Memory mapper trait with `ppu_cycle_260()` hook |

### What works well (keep)

- PPU internal register model (v, t, x, w) is correct
- Scroll increment rules are present, but the current bulk-advance path does not execute
  every rule at its exact dot yet
- 3:1 PPU-to-CPU clock ratio
- Cycle/scanline/frame counters
- `ppu_cycle_260()` mapper callback for MMC3-style IRQs
- OAM DMA handling
- Palette addressing and mirroring

### What's broken or missing

1. **No per-dot pixel output** — rendering is deferred to end-of-frame
2. **No shift registers** — tile data isn't fetched/shifted per-dot
3. **No secondary OAM** — sprite evaluation isn't cycle-timed
4. **Sprite 0 hit is approximate** — checks Y/X position, not actual pixel overlap
5. **No catch-up mechanism** — PPU register writes don't trigger rendering sync
6. **VBlank race condition** — `vblank_bit_race_condition` field referenced but never defined (dead code)
7. **OAMDATA glitch** — documented in comments but not implemented
8. **Sprite overflow** — flag set but evaluation bug not emulated
9. **Bulk PPU stepping hides dot timing bugs** — scroll updates such as coarse Y at dot
   256 currently run from scanline-boundary logic instead of true per-dot execution

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

**Goal**: Establish the timing foundation and ownership model before changing rendering
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

**Goal**: Move background rendering from `gfx::render()` into the PPU tick, one scanline
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
6. Keep `gfx::render_sprites()` temporarily (render sprites on top at VBlank as before)
7. Keep the current approximate sprite 0 hit check until Phase 4 replaces it with
   pixel-level overlap; this preserves sprite-0-polling games as well as the current code
   can during Phases 2-3
8. Verify: CPU-timed mid-frame scroll changes should now render correctly for the
   background layer

This phase is still a transitional renderer. It samples nametable, attribute, pattern, and
palette data at pixel-output time rather than the hardware fetch dots, so CHR banking,
palette writes, and nametable changes during the same scanline can still be inaccurate
until Phase 3.

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

**Goal**: Replace the direct tile lookup with the hardware-accurate fetch/shift pipeline.

**Status** (2026): Fetch path (NT→AT→PT, prefetch, dummy fetches) and **`ppu_fetch` / MMC3 A12** run from the shift pipeline. **Live framebuffer background** still uses **`render_line_v` + direct nametable/pattern read** per pixel so mid-scanline `$2006` splits stay aligned with tests and common raster tricks; shifter output is validated in unit tests (`assert_shifter_matches_direct`, etc.). Compositing purely from shift registers would need full parity with **fine-X mux + tile-merge** on partial reload (see Phase 5 refinements).

**Remaining**
- Optional: drive visible pixels from shifters after a Visual2C02-accurate mid-line reload model.
- MMC3 title behaviour: re-smoke after Phase 4 sprite path (ongoing).

**(Prior checkpoint text below retained for history.)**

**Status**: Checkpoint (half-finished). In tree: shift-register state, 4-step fetch
cadence (NT → AT → PT low → PT high), tile-boundary loads, prefetch and dummy fetches,
and **unit-test QA** comparing shifter output against the Phase 2 direct background
lookup (so the pipeline is exercised and locked without risking the live framebuffer
path yet). Live visible background pixels still use the **direct lookup** path because
enabling shifter-composited pixels regressed several titles; toggling back is a Phase 3
follow-up once the discrepancy between the two paths is understood.

**Known follow-ups (deferred)**:
- MMC3 games (e.g. SMB3, Mega Man 3, Kirby, Battletoads) still show wrong scrolling /
  garbling relative to SMB1-level mappers. Further mapper IRQ / A12 work kept colliding
  with incomplete Phase 4 sprite fetch timing; **accept as broken for now** and revisit
  after sprite evaluation and fetches are cycle-accurate.
- Prefetch coarse-X increment bug fixed: advance coarse X only at dots **328** and
  **336** during prefetch, not on every dot from 328–340.

1. Add shift register fields to PPU (listed above)
2. Implement the 4-step fetch cycle (NT → AT → PT low → PT high) every 8 dots
3. Load fetched data into shift registers at tile boundaries
4. Output pixels by selecting bits from shift registers using fine X *(live path: still
   direct lookup; shifter output validated in tests)*
5. Implement pre-fetch (dots 321-336) and dummy fetches (337-340)
6. Drive mapper A12/IRQ observation from actual PPU pattern-table fetches *(MMC3: IRQ
   clocking edge-based from `ppu_fetch`; behavior still not game-good — see deferrals)*
7. Verify: fine-X scrolling should be pixel-perfect; no visual regression on games
   that worked before *(gate not fully met for MMC3-class titles yet)*

**Tests**:
- `test_shift_register_loads_at_tile_boundary` — after 8 dots, new tile data is loaded
  into the upper bits of the shift registers
- `test_fine_x_selects_shift_register_pixel` — with fine X = 0..7, the correct bit is
  selected from the shift register
- `test_tile_fetch_sequence` — verify the 4 memory accesses happen at dots N+0, N+2,
  N+4, N+6 within each 8-dot window
- `test_prefetch_dots_321_336_seed_visible_shifters` — first two tiles of next scanline
  are fetched during dots 321-336
- Shifter-vs-direct background consistency tests under `cfg(test)` *(e.g.
  `assert_shifter_matches_direct_background` and related cases)*
- `test_mmc3_a12_edges_from_filtered_ppu_fetches` / related MMC3 unit coverage — mapper
  tests pass; visual MMC3 smoke still failing (see deferrals)

**Not removed yet (Phase 3 completion)**:
- Direct tile lookup for **live** background pixels (remains until shifter path matches
  hardware under full-game smoke)
- Any `ppu_cycle_260()` hook on the mapper trait may remain for compatibility; MMC3 IRQ
  **clocking** in this tree is driven from pattern fetches, not the empty scanline stub

### Phase 4: Sprite evaluation and rendering in PPU

**Goal**: Cycle-accurate sprite handling, proper sprite 0 hit, and correct priority.

**Status** (2026): Secondary OAM, evaluation dots **65–191** (every 2 cycles), sprite fetches from **secondary** on dots **257–320**, per-line **`sprite_line`** compositing, pixel-level **sprite 0 hit** when `sprite_zero_on_current_line`, **`STATUS_SPRITE_OVERFLOW`** on ninth in-range sprite. **`gfx::render_sprites`** removed.

**Not done yet**
- Hardware **sprite overflow diagonal / false-positive** behaviour
- Formal **sprite shift-register** model (functionally equivalent latched patterns + X)
- Extra plan tests (priority, flip, 8×16, overflow quirk) — add as needed

**Original task checklist** (partially out of date):

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

**Old code removed**:
- `gfx::render_sprites()` (gfx/mod.rs)
- `gfx::render()` entirely — no longer called from emulation loop
- `gfx::tile_to_attribute_byte()`, `gfx::tile_to_attribute_pos()`,
  `gfx::get_nametable_base_addr()`, `gfx::get_attribute_table_addr()` — move to PPU
  or delete if duplicated by shift register logic
- Old `PPU::sprite_zero_hit()` approximate check (ppu/mod.rs)
- Dead code: `vblank_bit_race_condition` references, commented-out OAMDATA glitch code
- Old gfx tests that test removed helper functions (replace with new PPU tests)
- `gfx/mod.rs` itself if only `buf.rs` and `palette.rs` remain (move those up or into
  ppu/)

### Phase 5: Edge cases and accuracy refinements

**Goal**: Handle hardware quirks that affect specific games.

**Done in tree**
- **OAMDATA glitch** during rendering (visible + pre-render when either BG or sprites enabled): no OAM write; `OAMADDR` advances coarse (+4 / high 6 bits).
- **VBlank / NMI**: reading `$2002` on **scanline 241, dot 0** sets a latch that **suppresses** the NMI edge when vblank is latched at dot 1 (simplified; open-bus and exact 1-cycle edges not modeled).

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

- **Phase 1 is the highest-risk phase** because it changes the timing model, CPU-memory
  interface, interrupt scheduling, and PPU ownership boundaries
- **Phase 2 is the highest-value phase** because it is the first phase that can visibly
  improve raster effects while leaving sprite rendering mostly unchanged
- **Phases 3-4 are hardware-complex but architecturally incremental** once Phase 1 has
  established dot stepping and direct PPU bus access
- **Phase 5 is lower architectural risk** because it focuses on edge-case quirks after the
  main timing pipeline exists

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
- **Avoid `Rc<RefCell<>>` in the hot path**: Phase 1 should remove this from per-dot PPU
  execution by passing direct mutable references or narrow bus adapters

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
