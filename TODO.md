# Krankulator TODO

## Mappers

Currently implemented: **NROM (0), MMC1 (1), UxROM (2), CNROM (3), MMC3 (4), AxROM (7), BNROM (34), GxROM (66)**
Coverage: ~677/695 licensed NTSC US games (97.4%)

### Completed: Priority 1 quick wins

**Mapper 66 — GxROM** [done]
- Games (~5): Super Mario Bros./Duck Hunt combo cart, Dragon Power, Gumshoe, Thunder & Lightning
- Single register at $8000-FFFF selects both PRG and CHR bank. Two bits each. AND-type bus conflicts.

**Mapper 34 — BNROM** [done]
- Games (~2): Deadly Towers, Impossible Mission II
- Full 8-bit PRG bank register (wraps via modulo), 32KB granularity. CHR RAM. AND-type bus conflicts.

### Priority 2: High-impact licensed game coverage

**Mapper 206 — DxROM (Namco 108 / MIMIC-1)** [S]
- Games (~10): Gauntlet, R.B.I. Baseball 1-3, Karnov, Indiana Jones and the Temple of Doom, Fantasy Zone, Pac-Mania, Ring King, Super Sprint
- Simplified predecessor to MMC3. Uses similar bank select registers ($8000/$8001) but without IRQ counter, without mirroring control, and with smaller bank counts. Can reuse much of existing MMC3 logic.

**Mapper 9 — MMC2 (PxROM)** [M]
- Games (2): Mike Tyson's Punch-Out!!, Punch-Out!!
- Unique CHR latch-switching: reading specific tiles ($FD/$FE) from pattern tables automatically switches CHR banks. Requires hooking into PPU tile fetch logic. PRG banking is simple (8KB switchable + 24KB fixed).

**Mapper 5 — MMC5 (ExROM)** [XL]
- Games (~8): Castlevania III: Dracula's Curse, Laser Invasion, Uncharted Waters, Romance of the Three Kingdoms II, Nobunaga's Ambition II, Gemfire, L'Empereur
- The most complex NES mapper. Multiple PRG/CHR banking modes, ExRAM with extended attributes, hardware multiplier, scanline IRQ, fill-mode nametable, split-screen support. Castlevania III is the main test target.

### Priority 3: MMC3 variants (leverage existing code)

**Mapper 118 — TxSROM** [S]
- Games (3): NES Play Action Football, Pro Sport Hockey, Goal! Two
- MMC3 variant where mirroring is controlled by bit 7 of CHR bank registers instead of the dedicated mirroring register. Flag/mode on existing MMC3 code.

**Mapper 119 — TQROM** [S]
- Games (2): High Speed, Pin-Bot
- MMC3 variant with mixed CHR ROM/RAM. Bit 6 of CHR bank register selects ROM vs RAM. Minor modification to existing MMC3.

### Priority 4: Remaining licensed games

**Mapper 68 — Sunsoft 4** [M]
- Games (1): After Burner
- CHR/PRG banking plus nametable mapping from CHR ROM.

**Mapper 69 — Sunsoft FME-7 / 5B** [M]
- Games (1): Batman: Return of the Joker
- Command/parameter register pair. 8 CHR + 4 PRG banks, CPU cycle-based IRQ counter.

**Mapper 105 — NES-EVENT (MMC1 variant)** [M]
- Games (1): Nintendo World Championships 1990 (extremely rare)
- MMC1 variant with timer/IRQ. Skip unless completionist.

### Priority 5: Unlicensed (optional, ~95 more games)

**Mapper 11 — Color Dreams** [XS] — ~31 games
**Mapper 71 — Camerica/Codemasters** [XS] — ~20 games
**Mapper 79 — NINA-03/NINA-06 (AVE)** [XS] — ~15 games
**Mapper 64 — RAMBO-1 (Tengen)** [M] — ~5 games

### Mapper coverage summary

| Step | Mappers | New games | Cumulative |
|------|---------|-----------|------------|
| Done | 0,1,2,3,4,7,34,66 | 677 | 677/695 (97.4%) |
| Priority 2 | 206, 9, 5 | ~20 | 697 (99.3%) |
| Priority 3 | 118, 119 | 5 | 697 (99.6%) |
| Priority 4 | 68, 69, 105 | 3 | 695/695 (100%) |
| Priority 5 | 11, 71, 79, 64 | ~71 unlicensed | — |

---

## Input / Controllers

Desktop: keyboard via winit (arrows, Z/X/A/B, C/V start/select). Gamepad via GCController (macOS) / gilrs (Linux/Windows).
Web: keyboard + touch controls (virtual d-pad with deadzone, A/B/Start/Select buttons) + Gamepad API (standard mapping).

- [x] Gamepad support [M]
  - macOS: GCController framework (objc2-game-controller) — required because macOS intercepts Bluetooth controller input
  - Linux/Windows: gilrs crate with event-based input, SdlMappings filter to avoid misdetected HID devices
  - Web: Gamepad API (navigator.getGamepads()), standard mapping, OR-merged with keyboard/touch
  - Auto-detect connected controllers (up to 2)
  - D-pad and analog stick mapping (with deadzone)
  - Two-player support (Joy-Con pair auto-splits into P1/P2)
  - Edge-detected save/load state and slot cycling on P1
  - All platforms: input sources OR-merged so keyboard and gamepad work simultaneously
- [ ] Configurable key/button bindings [S]
  - Save bindings to a config file
  - Per-controller profiles
- [ ] Turbo A/B buttons (optional toggle) [XS]

---

## On-Screen Display

8x8 bitmap font overlay rendered directly into the framebuffer (core/src/emu/gfx/font.rs, overlay.rs).
1px outlined text for readability on any background. Toggle with Tab (desktop/web) or double-tap canvas (mobile).

- [x] On-screen message logger (semi-transparent overlay) [M]
  - Toast notifications for save/load state, slot cycling, errors
  - Auto-expire after 120 frames (~2 seconds), capped at 4 simultaneous
- [x] Optional FPS counter overlay [S]
  - Shows emulation time in ms and percentage of 16.64ms NTSC frame budget
- [ ] Channel mute status indicator (when toggling audio channels 1-5) [XS]

---

## UI / Desktop App Polish

Currently: bare winit window, no menu, CLI-only file selection.

- [x] App icon (macOS dock icon via NSApplication, web favicon) [XS]
- [ ] Native menu bar (File, Emulation, Audio, Video, Help) [L]
  - File: Open ROM, Recent ROMs, Close
  - Emulation: Pause/Resume, Reset, Save State, Load State
  - Audio: Mute, Channel toggles
  - Video: Fullscreen, scaling options
  - Help: About, keyboard shortcuts
- [ ] File open dialog (native via rfd crate) [S]
  - Filter for .nes files
  - Remember last opened directory
- [ ] Fullscreen toggle (F11 or Cmd+F) [S]
- [x] Window title shows loaded ROM name [XS]
- [ ] Drag-and-drop ROM file onto window to load [S]
- [ ] Recent ROMs list (persist across sessions) [M]

---

## Emulation Features

- [ ] Rewind (ring buffer of save states, hold a key to scrub back) [L]
- [ ] Fast-forward (uncapped speed while held, or 2x/4x toggle) [S]
- [ ] Slow-motion (0.5x speed toggle) [XS]
- [ ] Screenshot (save framebuffer as PNG) [S]
- [ ] Video recording (save to GIF or MP4) [L]
- [ ] Game Genie code support [M]
- [ ] NSF player mode (play NES Sound Format music files) [L]
- [ ] PAL timing mode (50 Hz, different PPU/CPU ratios) [M]

---

## Video / Rendering

Currently: pixels crate, 4x integer scale, no filters.

- [ ] Shader/filter support [L]
  - CRT scanline filter
  - NTSC composite video simulation (blargg's nes_ntsc or similar)
  - Nearest-neighbor vs bilinear scaling option
- [ ] Configurable window scale (1x-6x) [S]
- [ ] Aspect ratio correction (8:7 pixel aspect ratio for accurate NES output) [S]
- [ ] Overscan cropping option (hide top/bottom 8 scanlines like real TVs did) [S]

---

## Build Targets

Currently: native desktop only (macOS), CI builds on Linux.

### RetroArch / libretro core [XL]
- [ ] Create a libretro core wrapper (`krankulator_libretro`)
  - Implement libretro API (retro_run, retro_load_game, etc.)
  - Audio/video callbacks instead of direct output
  - Input abstracted through libretro API
  - Core info file (.info) for RetroArch
  - Separate Cargo workspace member or feature flag
- Gives access to RetroArch's ecosystem: shaders, netplay, achievements, controller support, recording

### Web (WASM + Canvas 2D) [XL] — Complete
- [x] Compile core emulation to wasm32-unknown-unknown (workspace split, zero cfg gates)
- [x] Canvas 2D rendering, AudioWorklet audio, keyboard/touch/gamepad input
- [x] Mobile-friendly responsive layout with touch controls
- [x] Responsive canvas scaling (up to 4x) with fullscreen mode (F key / double-click)
- [x] Audio pause on tab visibility change
- [x] Local storage for save RAM and save states
- [x] Hosted on GitHub Pages (krankulator.teknodromen.se)

### Cross-platform desktop builds [M]
- [ ] macOS: .app bundle with icon, code signing
- [ ] Windows: .exe with icon, optional installer (WiX or NSIS)
- [ ] Linux: AppImage or .deb package

---

## CI/CD & Releases

Currently: GitHub Actions runs `cargo build`, `cargo test`, and a separate release-mode APU mixer reference job on push to master. No releases.

- [ ] Automated releases on master push (or on git tags) [L]
  - Build matrix: macOS (x86_64 + aarch64), Windows (x86_64), Linux (x86_64)
  - Produce downloadable artifacts (zip/tar.gz per platform)
  - GitHub Release with changelog from commits
- [ ] RetroArch core artifacts (build the libretro .dylib/.dll/.so) [M]
- [ ] WASM build step [M]
  - wasm-pack or cargo build --target wasm32-unknown-unknown
  - Deploy to GitHub Pages automatically
- [ ] Version numbering scheme (CalVer or SemVer) [XS]
- [ ] Release notes generation (from conventional commits or PR titles) [S]

---

## Test ROMs

Currently: copies of test ROMs checked into `input/nes/`. Source repo at `/Users/anders/Documents/code/nes-test-roms/` (66 test suites).

- [ ] Add nes-test-roms as a git submodule (replace copied files in `input/nes/`) [M]
  - Update all test paths in source code to point at submodule location
  - Remove duplicated ROM files from repo
  - CI: init submodule in GitHub Actions workflow
- [ ] Aim for 100% pass rate across all applicable suites. Track status per-suite:

### Not yet tested / known failing

These suites exist in nes-test-roms but aren't currently wired up as tests:

- [ ] branch_timing_tests
- [ ] cpu_dummy_reads
- [ ] cpu_dummy_writes
- [ ] cpu_exec_space (partial — only APU test referenced)
- [ ] cpu_reset
- [ ] cpu_timing_test6
- [ ] dmc_dma_during_read4
- [ ] dmc_tests
- [ ] instr_misc
- [ ] mmc3_test_2
- [ ] mmc5test / mmc5test_v2 (needs mapper 5)
- [ ] ppu_open_bus
- [ ] ppu_read_buffer
- [ ] sprite_overflow_tests
- [ ] sprdma_and_dmc_dma
- [ ] scanline / scanline-a1
- [ ] nmi_sync
- [ ] stress
- [ ] pal_apu_tests (needs PAL mode)
- [ ] read_joy3 (controller read timing)
- [ ] scrolltest
- [ ] full_palette
- [ ] vbl_nmi_timing (separate from blargg's ppu_vbl_nmi)

---

## Quality / Accuracy

- [x] APU mixer capture/reference workflow for square, triangle, noise, and DMC channels
  - Headless `CapturingAudioOutput`, WAV export, hardware reference MP3 fixtures, JSON/PNG analysis reports
  - CI runs `cargo test --release test_apu_mixer -- --ignored --nocapture --test-threads=4`
- [ ] Sprite 0 hit: upgrade from position-based to pixel-overlap accuracy [M]
- [ ] PPU open bus behavior [M]
- [ ] CPU unofficial/illegal opcodes (for some unlicensed games and demos) [L]
- [ ] APU DMC DMA cycle stealing accuracy [L]

---

## Misc / Maybe

- [ ] Config file (~/.config/krankulator/config.toml) for persistent settings [M]
- [ ] Netplay (rollback-based) [XXL]
- [ ] Input recording/playback (TAS support) [L]
- [ ] ROM database (hash-based game identification, auto-select mapper) [M]
- [ ] Cheat search (RAM watch/search for modifying values) [L]
