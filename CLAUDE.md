# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Krankulator is a NES (Nintendo Entertainment System) emulator written in Rust. It emulates the MOS Technology 6502 CPU, PPU (Picture Processing Unit), APU (Audio Processing Unit), and various memory mappers to run NES games and test ROMs.

## Versioning

The project uses SemVer with auto-incremented patch numbers. The version is `MAJOR.MINOR.PATCH` where PATCH = total commit count on master (computed by CI via `git rev-list --count HEAD`). Major and minor are set manually in the Cargo.toml files.

**When to bump major/minor:** If a change breaks save state compatibility, removes a supported mapper, changes the libretro API surface, or otherwise breaks backwards compatibility with existing users, suggest bumping the major version. If a change adds significant new features (new mapper, new frontend, new emulation capability), suggest bumping the minor version. Update the `version` field in all four `Cargo.toml` files (core, desktop, web, libretro), the `LIBRARY_VERSION` in `libretro/src/lib.rs`, and `display_version` in `libretro/krankulator_libretro.info`.

## Workspace Structure

The project is a Cargo workspace with four crates:

- **`core/`** (`krankulator-core`) — Platform-independent emulation library. Compiles to native and wasm32 with zero cfg gates. Only dependency: `hex`.
- **`desktop/`** (`krankulator-desktop`) — Native frontend. Platform-split IO: winit+pixels on macOS/Windows, GTK3+GLArea+glow on Linux. Produces the `krankulator` binary.
- **`web/`** (`krankulator-web`) — WebAssembly frontend using web-sys, Canvas 2D, AudioWorklet, touch controls. Built with `trunk`.
- **`libretro/`** (`krankulator-libretro`) — RetroArch/libretro core. Raw C FFI, no extra dependencies. Produces `krankulator_libretro.so/.dll/.dylib`.

Key traits defined in core that frontends implement:
- `IOHandler` (`core/src/emu/io/mod.rs`) — `init()`, `log()`, `poll()`, `render()`, `exit()`, `frame_time_ms()`, `set_frame_duration_nanos()`, `set_overscan_available()`
- `AudioBackend` (`core/src/emu/audio/mod.rs`) — `push_samples()`, `flush()`, `clear()`

## Common Commands

### Building and Running
```bash
# Build everything (native + wasm)
cargo build --workspace

# Run desktop with a NES ROM file
cargo run -- test-roms/other/nestest.nes

# Run in headless mode (no graphics)
cargo run -- --headless test-roms/other/nestest.nes

# Capture headless audio to a WAV file
cargo run -- --wav-out /tmp/krankulator.wav test-roms/apu_mixer/square.nes

# Run with debugging enabled
cargo run -- --debug --verbose test-roms/other/nestest.nes

# Add breakpoints
cargo run -- --breakpoint 0xC000 test-roms/other/nestest.nes

# Specify loader type (nes, ascii, bin)
cargo run -- --loader ascii input/ascii/instructions

# Run with custom code starting address
cargo run -- --codeaddr 0x400 --loader bin input/bin/test.bin

# Build libretro core for RetroArch
cargo build --release -p krankulator-libretro

# Test with RetroArch
retroarch -L target/release/libkrankulator_libretro.so path/to/game.nes

# Build core for wasm32 (verify no platform deps leak in)
cargo build -p krankulator-core --target wasm32-unknown-unknown

# Build and serve web version (requires trunk: cargo install trunk)
# HTTPS required for Gamepad API / SharedArrayBuffer — see docs/web-dev-setup.md
cd web && trunk serve
```

### Testing
```bash
# Run all tests
cargo test --workspace

# Run only core tests
cargo test -p krankulator-core

# Run specific test
cargo test test_nestest

# Run tests with output
cargo test -- --nocapture

# Run APU mixer reference tests (requires scripts/.venv)
cd scripts && uv venv && uv pip install -r requirements.txt
cd .. && cargo test --release test_apu_mixer -- --ignored --nocapture --test-threads=4
```

### Development
```bash
# Check for compilation errors
cargo check --workspace

# Format code
cargo fmt

# Run clippy for linting
cargo clippy --workspace
```

**Always run `cargo fmt` before committing.** Formatting changes must not be mixed into functional commits.

## Architecture Overview

### Core Library (`core/src/`)

**Emulator (`emu/mod.rs`)**
- Main emulator struct that orchestrates CPU, PPU, APU, and memory
- Handles cycle-accurate timing between components
- `run()` — blocking loop for desktop; `run_one_frame()` — single-frame step for web/rAF
- `new_with(io, mapper, audio)` — constructor taking trait objects for any frontend (defaults to NTSC)
- `new_with_region(io, mapper, audio, region)` — constructor with explicit region (NTSC or PAL)
- `load_rom(mapper, path)` — hot-swap mapper for loading a new ROM mid-emulation (resets CPU/PPU/APU)
- `load_rom_with_region(mapper, path, region)` — hot-swap with region change
- `take_pending_open_rom()` — returns path from menu Open ROM action; desktop main loop re-enters `run()` after loading

**Region (`emu/region.rs`)**
- `Region` enum (Ntsc, Pal) with `config()` returning `RegionConfig`
- `RegionConfig` struct: master clock dividers (CPU÷12/16, PPU÷4/5), scanline counts (262/312), CPU clock rate, frame duration, odd-frame skip flag, input poll interval
- Master clock sub-dot accumulator: `master_clock_sub` tracks fractional PPU dots for PAL's non-integer 3.2:1 ratio. Pattern: 3,3,3,3,4 dots over 5 CPU cycles
- Region auto-detected from iNES header (byte 9 for iNES 1.0, byte 12 for NES 2.0) with filename fallback (`(E)`, `(Europe)`, `(PAL)`)
- Desktop `--region auto|ntsc|pal` CLI override (default: auto)

**CPU (`emu/cpu/mod.rs`)**
- MOS 6502 CPU implementation with all official opcodes
- Handles instruction decoding, execution, and flag management

**PPU (`emu/ppu/mod.rs`)**
- Picture Processing Unit for graphics rendering
- Implements proper VRAM addressing with internal registers (v, t, x, w)
- Level-based NMI output signal (`nmi_output`), edge-detected by the emulator each CPU cycle
- `update_nmi_output()` recalculates the NMI line whenever VBL flag or NMI enable changes
- `nmi_rising_edge_dot` records rising edges with same-dot cancellation for $2002 race suppression
- Per-dot cycle-accurate rendering

**Memory System (`emu/memory/`)**
- Memory mappers for different cartridge types (NROM, MMC1, MMC2, MMC3, MMC5, TxSROM, TQROM, UxROM, AxROM, CNROM, BNROM, GxROM, Sunsoft 4, Sunsoft FME-7, NES-EVENT, VRC2/VRC4, Namco 108/DxROM, Taito TC0190, Simple discrete: 87/140/152/180/184/185)
- Handles bank switching and memory mirroring
- Separates CPU and PPU memory spaces
- Mapper trait includes `ppu_cycle_260()` hook for scanline-counting mappers (MMC3), `cpu_cycle(ppu_dot)` for per-cycle mapper logic (MMC5 audio/IRQ, receives current PPU dot for timing), `notify_ppu_ctrl()` for sprite size tracking, and `audio_expansion_output()` for expansion audio mixing
- `PpuBus` shared struct handles CHR read/write, nametable mirroring, palette RAM, and VRAM for simple mappers
- AND-type bus conflict emulation for discrete logic mappers (BNROM, GxROM)

**APU (`emu/apu/`)**
- Audio Processing Unit with pulse, triangle, noise, and DMC channels
- Frame counter for audio timing with region-specific cycle tables (NTSC/PAL)
- DMC and noise period tables selected by region
- Per-cycle mixer accumulation for proper anti-aliasing

**Graphics (`emu/gfx/`)**
- Frame buffer (`buf.rs`): 256x240 RGB pixels
- Palette lookup table (`palette.rs`)
- Bitmap font (`font.rs`): 8x8 pixel font with 1px outlined rendering for overlay text
- Overlay (`overlay.rs`): frame time display with region-aware budget percentage (Tab to toggle), toast notifications for save/load/slot changes, persistent banner for no-ROM state, rewind status indicator
- CRT shaders (`shaders/`): CRT-Lottes-Fast (Timothy Lottes, public domain) in WGSL (`crt_lottes.wgsl`) and GLSL ES 3.0 (`crt_lottes_web.vert`/`.frag`). 8-tap gaussian filter, windowed cosine scanlines, aperture grille mask, barrel distortion, auto-exposure tonemapper. Toggled via F9 key or menu. GLSL version also used on Linux GTK backend (adapted to GL 3.30 at runtime).

**Audio (`emu/audio/`)**
- `AudioBackend` trait with `push_samples()`, `flush()`, `clear()`
- `SilentAudioOutput`, `CapturingAudioOutput` for headless/test use
- WAV writer (`wav.rs`) for capturing test output

**Rewind (`emu/rewind.rs`)**
- `RewindBuffer`: ring buffer of savestate + framebuffer pairs (300 slots = 10s at 30fps capture rate)
- Captures every other frame to give 2x rewind speed. Rewind activated via `PollResult.rewind` (W key or right trigger)
- During rewind, emulation is paused; frames are popped and rendered with overlay showing remaining time

**IO (`emu/io/`)**
- `IOHandler` trait for input/rendering
- `loader.rs` — ROM loading (iNES format), includes `load_nes_from_bytes()` and `load_nes_from_bytes_with_sram()` for web
- `controller.rs` — NES controller state
- `HeadlessIOHandler` for tests

**Loader (`emu/io/loader.rs`)**
- `load_nes_from_bytes(&[u8])` — parse iNES ROM from byte slice
- `load_nes_from_bytes_with_sram(&[u8], Option<Vec<u8>>)` — same but with pre-loaded SRAM (used by web)
- `rom_has_battery(&[u8])` — check iNES header for battery flag
- `detect_region(&[u8])` — detect region from iNES header only
- `detect_region_with_filename(&[u8], Option<&str>)` — detect region from header + filename heuristic (`(E)`, `(Europe)`, `(PAL)`)
- `InesLoader::load(path)` — load from filesystem (used by desktop)

### Desktop Frontend (`desktop/src/`)

- `main.rs` — CLI (clap), wires IOHandler + AudioBackend to core; `--region auto|ntsc|pal` flag; region auto-detection from header + filename; no-ROM launch shows banner screen; outer loop handles Open ROM by reloading mapper and re-entering `run()`; unsupported mapper errors toast on-screen
- `settings.rs` — Persistent settings (`~/.config/krankulator/settings.txt`): `integer_scaling`, `scanlines`. Simple key=value format, no serde.
- `io/mod.rs` — Shared menu construction (`build_menu_contents()`), `MenuIds`/`MenuItems` structs, recent ROMs persistence (`~/.config/krankulator/recent_roms.txt`, last 10), platform re-export (`PlatformIOHandler`)
- `io/winit_backend.rs` — macOS/Windows: `WinitPixelsIOHandler` using winit 0.30 + pixels (wgpu), muda menu via `init_for_nsapp()`/`init_for_hwnd()`, CRT shader via `pixels.render_with()` + wgpu render pipeline, debug shell (shrust)
- `io/gtk_backend.rs` — Linux: `GtkPixelsIOHandler` using GTK3 + GLArea (OpenGL 3.3 via glow + eglGetProcAddress), muda menu via `init_for_gtk_window()`, native Wayland support. CRT-Lottes-Fast shader (GLSL 3.30, adapted from web ES 3.0 sources). Menu bar hidden in fullscreen. Screensaver/suspend inhibited via D-Bus `org.freedesktop.ScreenSaver.Inhibit`.
- `audio.rs` — `AudioOutput`: rodio + ringbuf for audio playback
- `gamepad.rs` — Platform-abstracted gamepad input (GCController on macOS, gilrs on Linux/Windows); Joy-Con pair auto-split into two players; edge detection for save/load/cycle triggers; filters by SdlMappings to avoid misdetected HID devices

### Web Frontend (`web/`)

- `src/lib.rs` — wasm-bindgen entry, ROM loading, emulator setup, rAF game loop
- `src/io.rs` — `WebIOHandler`: WebGL2 CRT rendering (with Canvas 2D fallback), controller polling
- `src/crt_renderer.rs` — `CrtRenderer`: WebGL2 CRT-Lottes-Fast shader pipeline, canvas resize handling, F9 toggle
- `src/audio.rs` — `WebAudioBackend`: AudioWorklet ring buffer, context setup, resume-on-interaction, visibility pause
- `src/input.rs` — keyboard handling, touch controls (dpad, action buttons), double-tap overlay toggle, Gamepad API polling, fullscreen toggle (F key / double-click)
- `src/persistence.rs` — localStorage save states/SRAM, base64 encoding, beforeunload handler
- `index.html` — HTML shell with desktop canvas, touch layout (landscape), rotate prompt (portrait)
- `assets/audio_processor.js` — AudioWorklet ring buffer processor
- `assets/mario-walking.png` — Sprite sheet for rotate-prompt animation
- `assets/PressStart2P.woff2` — Pixel font (OFL licensed)
- `Trunk.toml` — trunk build config (release mode, COOP/COEP headers, TLS cert paths)

### Libretro Core (`libretro/`)

- `src/lib.rs` — All 25 `#[no_mangle] extern "C"` libretro API exports, `LibretroIOHandler`, `LibretroAudioBackend`
- `src/libretro_sys.rs` — Minimal C type definitions mirroring `libretro.h` (structs, constants, callback types)
- `krankulator_libretro.info` — Core metadata for RetroArch
- Raw FFI with zero extra dependencies beyond `krankulator-core`

### Test paths

Two macros in `core/src/lib.rs`:
- `test_input!("ascii/foo")` — expands to `$CARGO_MANIFEST_DIR/../input/` (for ascii and bin test inputs)
- `test_rom!("suite/foo.nes")` — expands to `$CARGO_MANIFEST_DIR/../test-roms/` (NES test ROM submodule)

### Key Design Patterns

**CPU-PPU Synchronization**
- NTSC: PPU runs at 3x CPU speed (master÷12=CPU, ÷4=PPU, clean 3:1 ratio)
- PAL: PPU runs at 3.2x CPU speed (master÷16=CPU, ÷5=PPU, 16 PPU dots per 5 CPU cycles)
- `master_clock` tracks PPU dots; `master_clock_sub` accumulates fractional dots for PAL (always 0 for NTSC)
- Per-CPU-cycle: add `master_clocks_per_cpu` to sub, divide by `master_clocks_per_ppu` for PPU advance (3 or 3/3/3/3/4 pattern)

**Memory Mapping**
- Uses trait objects for different mapper implementations
- Mappers handle bank switching and memory mirroring specific to cartridge types

**Audio Pipeline**
- APU accumulates samples per cycle, outputs batches via `push_samples()`
- `flush()` called once per frame — desktop is no-op (ring buffer), web sends to AudioWorklet via postMessage
- Web AudioWorklet uses fixed-size ring buffer (8192 samples) to absorb timing jitter
- Mobile Safari workaround: AudioContext resumed on user gesture, routed through MediaStreamDestination → HTMLAudioElement

**Frame pacing (web)**
- `requestAnimationFrame` loop with `performance.now()` time accumulator
- Targets 60.0988 FPS (NTSC) or 50.007 FPS (PAL), caps at 2 frames per rAF to prevent spiral-of-death

**Persistence (web)**
- Save states and SRAM stored in `localStorage` as base64-encoded binary
- Keys: `krankulator:{fnv1a_hash}:ss{0-3}` for save states, `krankulator:{fnv1a_hash}:sram` for battery RAM
- SRAM auto-saves every ~5s, on page unload, and when switching ROMs
- Save state keys: S (save), A (load), Q (cycle slot 0-3)

**Input merging**
- Multiple input sources (keyboard, touch, gamepad) are OR-merged into a single controller state bitmask each frame
- Desktop: keyboard state tracked in `kb_state: u8`, OR'd with gilrs gamepad state, written via `load_status()`
- Web: keyboard/touch keys set OR'd with Gamepad API poll result
- Gamepad meta-actions (save/load/cycle) use edge detection (trigger on press, not hold)
- Rewind: W key or right trigger (held, not edge-detected)

## File Structure

```
Cargo.toml          — Virtual workspace manifest
core/               — Platform-independent emulation library
  src/lib.rs        — Crate root, exports test_input! and test_rom! macros
  src/emu/          — Emulator core (cpu, ppu, apu, memory, io, gfx, audio, rewind, region)
  src/emu/gfx/shaders/ — CRT-Lottes-Fast shader sources (crt_lottes.wgsl, crt_lottes_web.vert/.frag)
  src/util/         — Hex parsing, file I/O utilities
desktop/            — Native frontend binary
  src/main.rs       — CLI entry point
  src/settings.rs   — Persistent settings (integer_scaling, scanlines)
  src/io/mod.rs     — Shared menu, recent ROMs, platform re-export
  src/io/winit_backend.rs — macOS/Windows IOHandler (winit + pixels + CRT shader)
  src/io/gtk_backend.rs   — Linux IOHandler (GTK3 + GLArea + glow)
  src/audio.rs      — rodio AudioBackend
  build.rs          — Windows icon embedding (winresource)
  assets/           — icon.png, icon.ico, Info.plist, krankulator.desktop
web/                — WebAssembly frontend
  src/lib.rs        — wasm-bindgen entry, ROM loading, emulator setup, rAF game loop
  src/io.rs         — WebIOHandler (WebGL2 CRT rendering with Canvas 2D fallback)
  src/crt_renderer.rs — WebGL2 CRT shader pipeline
  src/audio.rs      — WebAudioBackend (AudioWorklet, context setup)
  src/input.rs      — Keyboard, touch controls, double-tap overlay toggle, Gamepad API
  src/persistence.rs — localStorage save states/SRAM, base64, beforeunload
  index.html        — HTML shell (desktop + touch layout + rotate prompt)
  assets/           — Static assets (audio_processor.js, background.jpg, mario-walking.png, PressStart2P.woff2)
  Trunk.toml        — Build config
libretro/           — RetroArch / libretro core
  src/lib.rs        — FFI exports, LibretroIOHandler, LibretroAudioBackend
  src/libretro_sys.rs — C type definitions (structs, constants, callbacks)
  krankulator_libretro.info — Core metadata for RetroArch
test-roms/          — NES test ROM submodule (christopherpow/nes-test-roms)
input/              — Non-NES test data files
  ascii/            — ASCII assembly test files
  bin/              — Binary test files
scripts/            — APU mixer test scripts (Python)
docs/               — Design documents and dev setup guides
```

## Testing Strategy

All emulation tests live in `core/` (572 tests, 21 ignored). Desktop has 6 tests including audio backend wiring.

**Unit Tests**
- Test individual CPU instructions and flag behavior
- Verify PPU register operations and timing
- Test memory mapper functionality

**Integration Tests (`core/src/emu/integration_tests.rs`)**
- Run complete NES test ROMs (nestest, blargg CPU/PPU/APU suites, interrupt tests, timing tests, DMC tests)
- Verify cycle-accurate behavior against known-good logs
- Test various cartridge mappers
- Savestate roundtrip tests
- Tests that fail are `#[ignore]`d — run with `cargo test -- --ignored` to check progress

**APU Mixer Tests**
- Compare emulator WAV output against hardware reference MP3 recordings
- Requires Python venv: `cd scripts && uv venv && uv pip install -r requirements.txt`
- Reference recordings in `test-roms/apu_mixer_recordings/`
- CI runs the 4 ignored mixer tests separately in release mode

## Important Implementation Details

**6502 CPU Emulation**
- All official opcodes implemented with proper cycle counts
- Accurate flag handling for arithmetic and logical operations
- Support for all addressing modes

**PPU Implementation**
- Proper VRAM address handling with internal registers (v, t, x, w)
- VBlank timing with level-based NMI output signal and CPU edge detection
- VBL flag suppression when $2002 read races with VBL set (dot 0 prevents flag, same-dot cancels NMI edge)
- Scroll register updates at correct cycle points during rendering
- Per-dot cycle-accurate rendering, scanline count from region (262 NTSC, 312 PAL)
- Odd-frame skip (NTSC only, disabled for PAL)
- Overscan masking skipped for PAL (PAL TVs show full 240 lines)
- Sprite 0 hit is approximate (position-based, not pixel-overlap)

**Memory Mappers**
- NROM, MMC1, MMC2, MMC3, MMC5, TxSROM, TQROM, UxROM, AxROM, CNROM, BNROM, GxROM, Sunsoft 4, Sunsoft FME-7, NES-EVENT, VRC2/VRC4, Namco 108/DxROM (88/206), Simple discrete (87/140/152/180/184/185)
- Proper mirroring for nametables and palettes
- BNROM/GxROM use AND-type bus conflicts (written value ANDed with ROM byte at write address)
- BNROM uses full 8-bit bank register (not masked to 2 bits), wrapping via modulo
- Simple mappers (BNROM, GxROM, CNROM, UxROM, AxROM) share PPU logic via `PpuBus`
- SimpleMapper (87/140/152/180/184/185): unified PpuBus-based implementation for trivial discrete-logic mappers with 8KB/4KB CHR and 16KB/32KB PRG banking
- Namco108Mapper (88/206): direct CHR/VRAM/palette management with 1KB CHR granularity; mapper 88 forces bit 6 on 1KB CHR bank writes
- Taito33Mapper (33): 8KB PRG + 2KB/1KB CHR banking, mirroring via PRG register bit 6, no IRQ
- VRC2/VRC4 (mappers 21/22/23/25): unified implementation with address-line remapping variants, 9 sub-variants, 8KB PRG + 1KB CHR banking, VRC4 adds scanline/cycle IRQ and PRG swap mode

**Audio System**
- Length counters for all channels
- Proper frame counter timing
- DMC channel with sample playback
- APU soft reset preserves channel registers and replays last $4017 write
- Pulse and noise timers tick at half CPU rate; triangle ticks every CPU cycle
- Per-cycle mixer accumulation for proper anti-aliasing of high-frequency noise

**CPU Bus**
- Open bus emulation: write-only registers return last value on data bus
- Indexed addressing performs dummy reads at uncorrected (pre-page-fix) address
- RMW instructions always perform the dummy read regardless of page crossing

The emulator passes standard NES test ROM suites including Klaus2m5, nestest, blargg CPU (v3+v5), blargg APU/APU 2005/APU reset, blargg PAL APU (all 10), blargg PPU 2005, DMC status, cpu_exec_space (APU), CPU timing, branch timing, instruction timing, instruction misc (including dummy_reads_apu), CPU interrupt CLI latency, PPU VBL basics/set-time/clear/NMI control/suppression/nmi-off-timing/even-odd frames, OAM read/stress, PPU open bus, sprite hit, sprite overflow, and MMC3 (both variants). Ignored tests track known gaps: NMI hijacking (nmi_and_brk etc.), nmi_on_timing (1-dot PPU-CPU alignment), DMA cycle stealing, cpu_dummy_writes, and ppu_read_buffer.
