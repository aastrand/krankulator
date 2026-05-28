# Krankulator TODO

## Mappers

Currently implemented: **NROM (0), MMC1 (1), UxROM (2), CNROM (3), MMC3 (4), MMC5 (5), AxROM (7), MMC2 (9), BNROM (34), Sunsoft 4 (68), Sunsoft FME-7 (69), GxROM (66), NES-EVENT (105), TxSROM (118), TQROM (119)**
Coverage: 695/695 licensed NTSC US games (100%)

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

**Mapper 5 — MMC5 (ExROM)** [done, known issues]
- Games (~8): Castlevania III: Dracula's Curse, Laser Invasion, Uncharted Waters, Romance of the Three Kingdoms II, Nobunaga's Ambition II, Gemfire, L'Empereur
- The most complex NES mapper. Multiple PRG/CHR banking modes (4 PRG modes, 4 CHR modes), ExRAM with nametable mapping, hardware 8×8 multiplier, scanline IRQ via PPU fetch detection, fill-mode nametable, two expansion pulse audio channels mixed into APU.
- **Known issue:** Scanline IRQ timing is not fully accurate — games boot and are playable but scanline-timed effects (e.g. Castlevania III status bar) may glitch. Likely blocked on core IRQ/NMI timing accuracy (see ignored cpu_interrupts_v2 and ppu_vbl_nmi tests).

### Completed: Priority 3 — MMC3 variants

**Mapper 118 — TxSROM** [done]
- Games (3): NES Play Action Football, Pro Sport Hockey, Goal! Two
- MMC3 variant where mirroring is controlled by bit 7 of CHR bank registers instead of the dedicated mirroring register.

**Mapper 119 — TQROM** [done]
- Games (2): High Speed, Pin-Bot
- MMC3 variant with mixed CHR ROM/RAM. Bit 6 of CHR bank register selects ROM vs RAM.

### Completed: Priority 4 — Remaining licensed games

**Mapper 68 — Sunsoft 4** [done]
- Games (1): After Burner
- CHR/PRG banking plus nametable mapping from CHR ROM.

**Mapper 69 — Sunsoft FME-7 / 5B** [done]
- Games (1): Batman: Return of the Joker
- Command/parameter register pair. 8 CHR + 4 PRG banks, CPU cycle-based IRQ counter. 5B expansion audio not yet implemented.

**Mapper 105 — NES-EVENT (MMC1 variant)** [done]
- Games (1): Nintendo World Championships 1990 (extremely rare)
- MMC1 variant with repurposed CHR registers, 30-bit CPU-cycle IRQ timer, init state machine.

### Priority 5: Unlicensed (optional, ~95 more games)

**Mapper 11 — Color Dreams** [XS] — ~31 games
**Mapper 71 — Camerica/Codemasters** [XS] — ~20 games
**Mapper 79 — NINA-03/NINA-06 (AVE)** [XS] — ~15 games
**Mapper 64 — RAMBO-1 (Tengen)** [M] — ~5 games

### Mapper coverage summary

| Step | Mappers | New games | Cumulative |
|------|---------|-----------|------------|
| Done | 0,1,2,3,4,5,7,9,34,66,68,69,105,118,119 | 695 | 695/695 (100%) |
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
- [ ] Inhibit screensaver/suspend while running (D-Bus `org.freedesktop.ScreenSaver.Inhibit`, needs zbus or similar) [S]
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

- [x] App icon (macOS dock icon via NSApplication, web favicon) [XS]
- [x] Native menu bar via muda crate (cross-platform: macOS/Windows/Linux) [M]
  - File: Open ROM (Cmd+O), Quit
  - Emulation: Reset (Cmd+R), Save State (Cmd+S), Load State (Cmd+L), Cycle Save Slot (Cmd+Q)
  - Display: Fullscreen (Cmd+F), Integer Scaling — checkmarks synced with keyboard shortcuts
  - Help: About (with app icon, version, website)
  - Open ROM triggers rfd file dialog and hot-swaps the mapper mid-emulation
  - Linux requires GTK3 (`libgtk-3-dev`) — available on virtually all desktop distros
- [x] File open dialog (native via rfd crate) [S]
  - Filter for .nes files
- [x] Remember last opened directory [XS]
- [x] Fullscreen toggle (F11), integer/fill scaling toggle (I) [S]
- [x] Window title shows loaded ROM name [XS]
- [x] Recent ROMs submenu (File > Recent, last 10, persisted in ~/.config/krankulator/recent_roms.txt) [M]
- [x] No-ROM launch shows black screen with "Open a ROM to play" banner [XS]
- [x] Unsupported mapper errors toast on-screen [XS]
- [ ] Drag-and-drop ROM file onto window to load [S]

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

Currently: pixels crate on macOS/Windows, Cairo software rendering on Linux (GTK3). Integer scaling (default) or fill scaling (I key), fullscreen (F11).

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

Test ROMs sourced from `test-roms/` git submodule ([christopherpow/nes-test-roms](https://github.com/christopherpow/nes-test-roms)). Two macros: `test_input!` for `input/` (ascii, bin), `test_rom!` for `test-roms/` (NES ROMs).

- [x] Add nes-test-roms as a git submodule (replace copied files in `input/nes/`) [M]
  - Update all test paths in source code to point at submodule location
  - Remove duplicated ROM files from repo
  - CI: init submodule in GitHub Actions workflow
- [ ] Aim for 100% pass rate across all applicable suites. Track status per-suite.

### Already tested (wired up in our test suite)

| Suite | ROMs | Status | Location |
|-------|------|--------|----------|
| instr_test-v5 | official_only, all_instrs | ✅ | integration_tests.rs |
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
| dmc_tests | status, status_irq, buffer_retained, latency | ✅ | integration_tests.rs |
| oam_read | 1 | ✅ | integration_tests.rs |
| ppu_vbl_nmi | 01, 03, 04, 05, 09 | ✅ | integration_tests.rs |
| ppu_vbl_nmi | 02, 06, 07, 08, 10 | ❌ ignored | integration_tests.rs |
| blargg_ppu_tests_2005 | palette_ram, sprite_ram, vram_access, vbl_clear_time, power_up_palette | ✅ | integration_tests.rs |
| ppu_open_bus | 1 | ❌ ignored | integration_tests.rs |
| oam_stress | 1 | ❌ ignored | integration_tests.rs |
| mmc3_test | all 6 | ✅ | memory/mapper/mmc3.rs |
| mmc3_test_2 | 5 of 6 (6-MMC3_alt fails) | ✅ (1 ignored) | memory/mapper/mmc3.rs |
| apu_mixer | 4 | ⏸ ignored (release-only) | apu/mod.rs |
| vbl_nmi_timing | all 7 | ✅ | integration_tests.rs |
| sprite_hit_tests_2005 | all 11 | ✅ | integration_tests.rs |
| sprite_overflow_tests | all 5 | ✅ | integration_tests.rs |
| ppu_read_buffer | 1 | ❌ ignored | integration_tests.rs |
| cpu_dummy_reads | 1 | ❌ ignored (hangs) | integration_tests.rs |
| cpu_dummy_writes | all 2 | ❌ ignored | integration_tests.rs |
| dmc_dma_during_read4 | all 5 | ❌ ignored (hangs) | integration_tests.rs |
| sprdma_and_dmc_dma | all 2 | ❌ ignored | integration_tests.rs |

### Ignored tests — failure analysis and fix plan

14 tests are wired up but `#[ignore]`d. Grouped by root cause, ordered by recommended attack priority.

| Test | What it tests | Game impact | Root cause | Size |
|------|--------------|-------------|------------|------|
| **Priority 1 — PPU VBL/NMI timing** | | | | |
| `02-vbl_set_time` | Exact PPU dot when VBL flag is set | High — affects all games with frame-sensitive NMI handlers | VBL suppression: reading $2002 at exact VBL dot should suppress flag (line 04 outputs `-V`, expected `--`) | M |
| ~~`05-nmi_timing`~~ | ~~Exact CPU cycle when NMI fires after VBL~~ | ~~High~~ | **FIXED** — dot-aware NMI countdown compares VBL dot against penultimate cycle | ✅ |
| `06-suppression` | VBL flag suppression when $2002 read at exact dot | Medium — rare but Battletoads-class edge cases | Same as 02: need to suppress VBL flag + NMI when read hits the set dot | M |
| `07-nmi_on_timing` | Enabling NMI ($2000 write) near VBL clear | Medium — games that toggle NMI enable near VBL | Off by 1 PPU dot: CPU/PPU phase alignment gives 3-dot resolution but test needs 1-dot | S |
| `08-nmi_off_timing` | Disabling NMI ($2000 write) near VBL set | Medium — same class of games | Off by 2 PPU dots: same sub-CPU-cycle sync precision issue as 07 | S |
| **Priority 2 — NMI hijacking + even/odd timing** | | | | |
| `2-nmi_and_brk` | NMI during BRK redirects to NMI vector | Medium — any game hitting BRK near VBL | Detect NMI edge before vector fetch in BRK | M |
| `3-nmi_and_irq` | NMI during IRQ redirects to NMI vector | Medium — IRQ-heavy games (MMC3) near VBL | Same mechanism in trigger_irq() | M |
| `10-even_odd_timing` | Odd-frame clock skip timing vs BG enable | Low — only cycle-exact raster effects | Odd-frame skip happens too late relative to $2001 BG enable | S |
| **Priority 3 — Small targeted fixes** | | | | |
| `1-instr_timing` | Cycle counts for unofficial NOP/SBC opcodes | Very low — only unofficial opcodes 82/89/C2/E2/0B/2B fail | Add cycle counts for ~6 unofficial opcodes | XS |
| `04-dummy_reads_apu` | Dummy reads on indexed ops trigger APU side effects | Low — only if game does indexed write to $40xx | APU registers respond to dummy read at uncorrected address | S |
| `5-branch_delays_irq` | Branch instruction delays IRQ by 1 cycle | Low — extremely narrow timing window | IRQ sampling during taken branch needs page-cross check | S |
| **Priority 4 — Deeper plumbing** | | | | |
| `4-irq_and_dma` | OAM DMA delays IRQ servicing | Low — IRQ-during-DMA is rare in practice | DMA doesn't model per-cycle IRQ polling | L |
| `ppu_open_bus` | PPU bus bits decay to 0 after ~600ms | Low — very few games rely on decay | Need per-bit decay timer on PPU data bus | M |
| **Priority 5 — Deprioritize** | | | | |
| `oam_stress` | OAM address/read/write under stress | Low — test only passes 1/4 on real HW | PPU-CPU alignment jitter, may be unfixable deterministically | S |
| `cpu_exec_space_ppuio` | Code execution from PPU I/O space | Very low — no real game does this | PPU open bus during instruction fetch | M |

**Attack order rationale:**

1. **PPU VBL/NMI timing (02, 05, 06, 07, 08)** — biggest game compatibility payoff. These are all facets of the same subsystem: exact-dot VBL flag set, NMI propagation delay, suppression, and $2000-triggered NMI edge detection. Many games with flickering or missing frames trace back to NMI timing off by 1-2 PPU dots. Fix them together.

2. **NMI hijacking (nmi_and_brk, nmi_and_irq) + even/odd timing** — second pass on code we attempted and reverted. With correct VBL timing from step 1, the hijacking logic should be straightforward: check for pending NMI edge before the vector fetch on cycle 6 of BRK/IRQ.

3. **Small wins (instr_timing_1, dummy_reads_apu, branch_delays_irq)** — quick targeted fixes. Add cycle counts for 6 unofficial opcodes, wire APU dummy reads, adjust branch IRQ sampling.

4. **PPU open bus + IRQ/DMA** — lower game impact, more plumbing work.

5. **oam_stress + cpu_exec_space_ppuio** — oam_stress is flaky on real hardware; ppuio tests a scenario no game uses.

### Not automatable

Visual demos, interactive tests, or unsupported hardware — cannot use $6000 protocol.

| Suite | Why |
|-------|-----|
| 240pee | Interactive menu-driven visual test suite |
| blargg_litewall / scanline / nmi_sync / stomper / window5 | Visual rendering demos |
| full_palette | Visual (displays palette colors) |
| scrolltest | Visual + interactive scroll test |
| read_joy3 | Requires precise controller read timing |
| tvpassfail | Interactive TV display test |
| vaus-test / PaddleTest3 | Requires Vaus/paddle controller hardware |
| dpcmletterbox / soundtest / volume_tests | Audio demos |
| nes15-1.0.0 / ny2011 / spritecans-2011 / stars_se / tutor | Games and demos |
| stress | Mixed visual + interactive test suite |
| nrom368 | Needs NROM-368 mapper variant |
| exram / mmc5test / mmc5test_v2 | Visual/manual MMC5 tests (no $6000 protocol) |
| m22chrbankingtest | Needs mapper 22 (VRC2a) |
| MMC1_A12 | Visual/manual MMC1 test |
| fdsirqtests | Needs FDS mapper |
| pal_apu_tests | Wired up but ignored (needs PAL mode) |

---

## Quality / Accuracy

- [x] APU mixer capture/reference workflow for square, triangle, noise, and DMC channels
  - Headless `CapturingAudioOutput`, WAV export, hardware reference MP3 fixtures, JSON/PNG analysis reports
  - CI runs `cargo test --release test_apu_mixer -- --ignored --nocapture --test-threads=4`
- [ ] PPU VBL/NMI dot-accurate timing (see ignored test plan above) [M]
- [ ] NMI hijacking during BRK/IRQ vector fetch [M]
- [ ] Sprite 0 hit: upgrade from position-based to pixel-overlap accuracy [M]
- [ ] PPU open bus decay behavior [M]
- [ ] CPU unofficial/illegal opcodes (for some unlicensed games and demos) [L]
- [ ] APU DMC DMA cycle stealing accuracy [L]

---

## Misc / Maybe

- [ ] Config file (~/.config/krankulator/config.toml) for persistent settings [M]
- [ ] Netplay (rollback-based) [XXL]
- [ ] Input recording/playback (TAS support) [L]
- [ ] ROM database (hash-based game identification, auto-select mapper) [M]
- [ ] Cheat search (RAM watch/search for modifying values) [L]
