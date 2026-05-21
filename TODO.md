# Krankulator TODO

## Mappers

Currently implemented: **NROM (0), MMC1 (1), UxROM (2), CNROM (3), MMC3 (4), AxROM (7), MMC2 (9), BNROM (34), GxROM (66)**
Coverage: ~679/695 licensed NTSC US games (97.7%)

### Completed: Priority 1 quick wins

**Mapper 66 — GxROM** [done]
- Games (~5): Super Mario Bros./Duck Hunt combo cart, Dragon Power, Gumshoe, Thunder & Lightning
- Single register at $8000-FFFF selects both PRG and CHR bank. Two bits each. AND-type bus conflicts.

**Mapper 34 — BNROM** [done]
- Games (~2): Deadly Towers, Impossible Mission II
- Full 8-bit PRG bank register (wraps via modulo), 32KB granularity. CHR RAM. AND-type bus conflicts.

### Priority 2: High-impact licensed game coverage

**Mapper 9 — MMC2 (PxROM)** [done]
- Games (2): Mike Tyson's Punch-Out!!, Punch-Out!!
- CHR latch-switching via `ppu_fetch()` hook: reading $0FD8/$0FE8 (left) and $1FD8-$1FDF/$1FE8-$1FEF (right) triggers deferred CHR bank switch. PRG: 8KB switchable + 24KB fixed.

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
| Done | 0,1,2,3,4,7,9,34,66 | 679 | 679/695 (97.7%) |
| Priority 2 | 5 | ~8 | 687 (98.8%) |
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
- [x] File open dialog (native via rfd crate) [S]
  - Filter for .nes files
- [x] Remember last opened directory [XS]
- [x] Fullscreen toggle (F11), integer/fill scaling toggle (I) [S]
- [x] Window title shows loaded ROM name [XS]
- [ ] Drag-and-drop ROM file onto window to load [S]
- [ ] Recent ROMs list (persist across sessions) [M]

---

## Emulation Features

- [ ] Rewind (ring buffer of save states, hold a key to scrub back) [L]
- [x] Fast-forward (hold Space for uncapped speed) [S]
- [ ] Slow-motion (0.5x speed toggle) [XS]
- [ ] Screenshot (save framebuffer as PNG) [S]
- [ ] Video recording (save to GIF or MP4) [L]
- [ ] Game Genie code support [M]
- [ ] NSF player mode (play NES Sound Format music files) [L]
- [ ] PAL timing mode (50 Hz, different PPU/CPU ratios) [M]

---

## Video / Rendering

Currently: pixels crate, integer scaling (default) or fill scaling (I key), fullscreen (F11).

- [ ] Shader/filter support [L]
  - CRT scanline filter
  - NTSC composite video simulation (blargg's nes_ntsc or similar)
- [ ] Configurable window scale (1x-6x) [S]
- [ ] Aspect ratio correction (8:7 pixel aspect ratio for accurate NES output) [S]
- [ ] Overscan cropping option (hide top/bottom 8 scanlines like real TVs did) [S]

---

## Build Targets

Currently: native desktop (macOS, Windows, Linux) and web (WASM). CI builds and releases all platforms.

### RetroArch / libretro core [XL] — Complete
- [x] Create a libretro core wrapper (`krankulator_libretro`)
  - Implement libretro API (retro_run, retro_load_game, etc.)
  - Audio/video callbacks instead of direct output
  - Input abstracted through libretro API
  - Core info file (.info) for RetroArch
  - Separate Cargo workspace member
- Gives access to RetroArch's ecosystem: shaders, netplay, achievements, controller support, recording

### Web (WASM + Canvas 2D) [XL] — Complete
- [x] Compile core emulation to wasm32-unknown-unknown (workspace split, zero cfg gates)
- [x] Canvas 2D rendering, AudioWorklet audio, keyboard/touch/gamepad input
- [x] Mobile-friendly responsive layout with touch controls
- [x] Responsive canvas scaling (up to 4x) with fullscreen mode (F key / double-click)
- [x] Audio pause on tab visibility change
- [x] Local storage for save RAM and save states
- [x] Hosted on GitHub Pages (krankulator.teknodromen.se)

### Cross-platform desktop builds [M] — Complete
- [x] macOS: .app bundle with icon (arm64)
- [x] Windows: .exe with embedded icon (x86_64)
- [x] Linux: AppImage (x86_64)

---

## CI/CD & Releases

Currently: GitHub Actions runs `cargo build`, `cargo test`, APU mixer reference tests, web deployment to GitHub Pages, and automated releases on push to master.

- [x] Automated releases on master push [L]
  - Rolling `latest` release with macOS arm64, Windows x86_64, Linux x86_64 artifacts
- [x] WASM build + deploy to GitHub Pages [M]
- [x] RetroArch core artifacts (build the libretro .dylib/.dll/.so for Linux x86_64/aarch64, Windows, macOS) [M]
- [x] Version numbering scheme (SemVer, patch auto-incremented by commit count) [XS]
- [ ] Release notes generation (from conventional commits or PR titles) [S]

---

## Test ROMs

Currently: copies of test ROMs checked into `input/nes/`. Source repo at `../nes-test-roms/` (66 test suites).

- [ ] Add nes-test-roms as a git submodule (replace copied files in `input/nes/`) [M]
  - Update all test paths in source code to point at submodule location
  - Remove duplicated ROM files from repo
  - CI: init submodule in GitHub Actions workflow
- [ ] Aim for 100% pass rate across all applicable suites. Track status per-suite.

### Already tested (wired up in our test suite)

| Suite | ROMs | Status | Location |
|-------|------|--------|----------|
| instr_test-v3 | official_only, all_instrs | ✅ | integration_tests.rs |
| instr_test-v5 | official_only | ✅ | integration_tests.rs |
| blargg_nes_cpu_test5 | official | ✅ | (same ROM as instr_test) |
| cpu_reset | ram_after_reset, registers | ✅ | integration_tests.rs |
| cpu_exec_space | APU test | ✅ | apu/mod.rs |
| cpu_exec_space | PPU I/O test | ❌ ignored | integration_tests.rs |
| cpu_interrupts_v2 | 1-cli_latency | ✅ | integration_tests.rs |
| cpu_interrupts_v2 | 2-nmi_and_brk, 3-nmi_and_irq, 4-irq_and_dma, 5-branch_delays_irq | ❌ ignored | integration_tests.rs |
| instr_timing | 2-branch_timing | ✅ | integration_tests.rs |
| instr_timing | 1-instr_timing | ❌ ignored | integration_tests.rs |
| cpu_timing_test6 | 1 ROM | ✅ | integration_tests.rs |
| instr_misc | abs_x_wrap, branch_wrap, dummy_reads | ✅ | integration_tests.rs |
| instr_misc | dummy_reads_apu | ❌ ignored | integration_tests.rs |
| branch_timing_tests | all 3 | ✅ | integration_tests.rs |
| apu_test | all 8 singles | ✅ | apu/mod.rs |
| blargg_apu_2005 | all 11 | ✅ | apu/mod.rs |
| apu_reset | all 6 | ✅ | apu/mod.rs |
| pal_apu_tests | all 10 | ⏸ ignored (needs PAL) | apu/mod.rs |
| dmc_tests | status, status_irq | ✅ | integration_tests.rs |
| dmc_tests | buffer_retained, latency | ❌ ignored | integration_tests.rs |
| oam_read | 1 | ✅ | integration_tests.rs |
| ppu_vbl_nmi | 01, 03, 04, 09 | ✅ | integration_tests.rs |
| ppu_vbl_nmi | 02, 05, 06, 07, 08, 10 | ❌ ignored | integration_tests.rs |
| blargg_ppu_tests_2005 | palette_ram, sprite_ram, vram_access, vbl_clear_time, power_up_palette | ✅ | integration_tests.rs |
| ppu_open_bus | 1 | ❌ ignored | integration_tests.rs |
| oam_stress | 1 | ❌ ignored | integration_tests.rs |
| mmc3_test | all 6 | ✅ | memory/mapper/mmc3.rs |
| apu_mixer | 4 | ⏸ ignored (release-only) | apu/mod.rs |

### Not yet wired up

All use the blargg $6000 status protocol (0=pass) and run to infinite loop.
ROMs not yet copied into `input/nes/`.

**PPU timing**

1. [ ] `vbl_nmi_timing` — 7 ROMs testing VBL/NMI to single-PPU-clock accuracy. Separate from blargg's ppu_vbl_nmi suite, even more timing-precise.
2. [ ] `ppu_read_buffer` — thorough PPU $2007 read buffer tests (1 ROM). ~20 second test, mammoth coverage of read buffer edge cases.

**Sprite tests (need sprite 0 hit upgrade)**

3. [ ] `sprite_hit_tests_2005.10.05` — 11 ROMs testing sprite 0 hit. basics, alignment, corners, flip, left_clip, right_edge, screen_bottom, double_height, timing_basics, timing_order, edge_timing. Currently our sprite 0 hit is position-based, not pixel-overlap — expect failures on 02+ until upgraded.
4. [ ] `sprite_overflow_tests` — 5 ROMs (Basics, Details, Timing, Obscure, Emulator). Tests the buggy sprite overflow flag evaluation. Basics may pass, later ones test the diagonal evaluation bug.

**CPU dummy reads/writes (may need bus accuracy work)**

5. [ ] `cpu_dummy_reads` — 1 ROM. Tests that indexed addressing does dummy read at uncorrected address. We document this behavior as implemented.
6. [ ] `cpu_dummy_writes` — 2 ROMs (OAM, PPU mem). Tests RMW instructions write-back-original-then-modified. Requires accurate PPU/OAM side effects from dummy writes.

**DMC/DMA cycle stealing (needs DMA accuracy work)**

7. [ ] `dmc_dma_during_read4` — 5 ROMs testing DMC DMA interleaving with $2007/$4016 reads. Very precise cycle-stealing behavior.
8. [ ] `sprdma_and_dmc_dma` — 2 ROMs testing OAM DMA + DMC DMA interaction. Cycle-accurate DMA interleaving.

**MMC3 revision tests**

9. [ ] `mmc3_test_2` — 6 ROMs, updated version of mmc3_test. Tests 5 and 6 distinguish MMC3 rev A vs rev B (we pass mmc3_test already, these may differ on edge cases).

### Not automatable (visual, interactive, or needs unsupported hardware)

These cannot be tested with the $6000 protocol — they're visual demos, interactive tests, or require unsupported mappers/peripherals.

| Suite | Why not automatable |
|-------|-------------------|
| 240pee | Interactive menu-driven visual test suite |
| blargg_litewall | Visual rendering demo |
| full_palette | Visual (displays palette colors) |
| scanline / scanline-a1 | Visual scanline rendering demo |
| scrolltest | Visual + interactive scroll test |
| nmi_sync | Visual demo (timed-write line drawing) |
| read_joy3 | Requires precise controller read timing / input |
| tvpassfail | Interactive TV display test |
| vaus-test / PaddleTest3 | Requires Vaus/paddle controller hardware |
| dpcmletterbox | Visual DPCM demo |
| soundtest | Audio playback demo |
| volume_tests | Audio volume level demo |
| stomper | Visual demo |
| nes15-1.0.0 | Puzzle game |
| ny2011 / spritecans-2011 / stars_se | Demos |
| tutor | Tutorial demo |
| window5 | Visual demo |
| stress | Mixed visual + interactive test suite |
| nrom368 | Needs NROM-368 mapper variant |
| exram / mmc5test / mmc5test_v2 | Needs mapper 5 (MMC5) — add when MMC5 is implemented |
| m22chrbankingtest | Needs mapper 22 (VRC2a) |
| MMC1_A12 | Visual/manual MMC1 test |
| fdsirqtests | Needs FDS mapper |
| pal_apu_tests | Already wired up but ignored (needs PAL mode) |
| other/ | Collection of misc demos and homebrew games |

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
