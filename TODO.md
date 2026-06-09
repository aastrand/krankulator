# Krankulator TODO

## Mappers

Currently implemented: **NROM (0), MMC1 (1), UxROM (2), CNROM (3), MMC3 (4), MMC5 (5), AxROM (7), MMC2 (9), MMC4 (10), Color Dreams (11), Bandai FCG (16/159), Jaleco SS88006 (18), Namco 163 (19), Action 53 (28), UNROM 512 (30), Mapper 31, VRC2/VRC4 (21/22/23/25), Taito TC0190 (33), BNROM (34), Taito TC0690 (48), GxROM (66), Sunsoft 4 (68), Sunsoft FME-7 (69), Camerica (71), VRC3 (73), VRC1 (75), Irem 74161/32 (78), Simple (87/140/152/180/184/185), Namco 108 (88/206), NES-EVENT (105), TxSROM (118), TQROM (119), Namco 175/340 (210)**
Coverage: ~93-95% of all licensed NES/Famicom games. 100% of licensed NTSC-NA and PAL titles. Remaining gaps are Japan-only Famicom games.

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

**Mapper 10 — MMC4 (FxROM)** [done]
- Games (3): Fire Emblem, Fire Emblem Gaiden, Famicom Wars
- MMC2 variant with 16KB switchable PRG ($8000-$BFFF) + 16KB fixed (last bank at $C000-$FFFF), 8KB battery-backed PRG RAM at $6000, range-based left-half latch triggers ($0FD8-$0FDF / $0FE8-$0FEF).

**Mapper 5 — MMC5 (ExROM)** [done]
- Games (~8): Castlevania III: Dracula's Curse, Laser Invasion, Uncharted Waters, Romance of the Three Kingdoms II, Nobunaga's Ambition II, Gemfire, L'Empereur
- The most complex NES mapper. Multiple PRG/CHR banking modes (4 PRG modes, 4 CHR modes), ExRAM with nametable mapping, hardware 8×8 multiplier, scanline IRQ via PPU fetch detection, fill-mode nametable, two expansion pulse audio channels mixed into APU.

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

### Completed: Priority 5 — Famicom Konami VRC

**Mappers 21/22/23/25 — VRC2+VRC4 (Konami)** [done]
- One unified implementation with 9 address-line remapping variants (VRC2a/b/c, VRC4a/b/c/d/e/f)
- Games (~20): Crisis Force, Gradius II, Parodius Da!, Contra (JP), Bio Miracle Bokutte Upa, Ganbare Goemon 2/Gaiden/Gaiden 2, Wai Wai World 1&2, Boku Dracula-kun, TwinBee 3, Tiny Toon (JP)
- VRC2: PRG/CHR banking, VRC2a half-resolution CHR. VRC4 adds scanline/cycle IRQ + PRG swap mode.

### Completed: Priority 5b — Famicom Konami VRC (quick wins)

**Mapper 75 — VRC1 (Konami)** [done]
- Games: Ganbare Goemon! Karakuri Douchuu (first Goemon), King Kong 2, Tetsuwan Atom (Astro Boy)
- 8KB PRG + 4KB CHR banking with high bits from $9000, mirroring control, no IRQ.

**Mapper 73 — VRC3 (Konami)** [done]
- Games: Salamander (Life Force JP)
- 16KB PRG banking + 16-bit/8-bit IRQ counter, CHR RAM only.

### Priority 5 (remaining): Famicom — Konami VRC expansion audio

**Mappers 24/26 — VRC6 (Konami)** [M-L] — 3 games
- Games: Akumajou Densetsu (Castlevania III JP — the definitive version), Madara, Esper Dream 2
- VRC6 expansion audio: 2 pulse + 1 sawtooth channels. Address swap between 24 and 26.

**Mapper 85 — VRC7 (Konami)** [L] — 2 games
- Games: Lagrange Point (acclaimed sci-fi RPG with OPLL FM synthesis), Tiny Toon 2 JP
- VRC4-style banking + IRQ + 6-channel FM synthesis (YM2413 subset). FM synth is the hard part.

### Completed: Priority 6a — Famicom trivial mappers

**Mappers 87, 140, 152, 180, 184, 185 — Simple discrete mappers** [done]
- Unified SimpleMapper with PpuBus. 6 mapper types covering ~27 games.
- Games: TwinBee, The Goonies (JP), City Connection, Arkanoid II, Atlantis no Nazo, Crazy Climber, Ninja-kun, Bird Week

**Mappers 88, 206 — Namco 108 / DxROM** [done]
- Direct CHR management with 1KB granularity. Mapper 88 is a subset with CHR address offset.
- Games (~48): Mappy-Land, Karnov, Sky Kid, Dragon Spirit, Dragon Slayer IV, Quest of Ki, Wagyan Land, Dragon Buster II, Valkyrie no Bouken, R.B.I. Baseball 1-3, Gauntlet, Fantasy Zone, Quinty

**Mapper 33 — Taito TC0190** [done]
- 8KB PRG + 2KB/1KB CHR banking, mirroring via PRG register bit 6, no IRQ.
- Games (9): Don Doko Don, Akira, Power Blazer (JP Power Blade), Insector X, Bakushou Jinsei Gekijou

### Completed: Priority 6a2 — Famicom Irem

**Mapper 78 — Irem 74161/32** [done]
- Games: Holy Diver (cult classic Castlevania-like)
- SimpleMapper: PRG/CHR select + submapper-aware mirroring (submapper 1 = single-screen, submapper 3 = H/V). Bus conflicts.

### Completed: Priority 6a3 — Famicom Namco/Taito

**Mapper 210 — Namco 175/340** [done]
- Games (12): Splatterhouse: Wanpaku Graffiti, Dream Master, Wagyan Land 2&3, Famista '91-'94
- Banking only (no IRQ, no expansion audio). Two sub-variants: 175 (hardwired mirroring, PRG RAM), 340 (mapper-controlled mirroring).

**Mapper 48 — Taito TC0690** [done]
- Games (2): Don Doko Don 2, Bakushou Jinsei Gekijou 3
- Mapper 33 + A12-based scanline IRQ (latch XOR $FF), mirroring via $E000.

### Completed: Priority 6a4 — Famicom Namco 163

**Mapper 19 — Namco 163** [done]
- 8-channel wavetable expansion audio, 15-bit CPU-cycle IRQ counter, CHR-ROM as nametables, 128-byte internal sound RAM, WRAM write protection.

### Completed: Priority 6b — Famicom Bandai/Jaleco

**Mapper 16/159 — Bandai FCG (LZ93D50)** [done]
- Games (~19): Dragon Ball Z: Kyoushuu! Saiyajin, Dragon Ball: Dai Maou Fukkatsu, Famicom Jump I&II, SD Gundam Gaiden
- Two submappers: FCG (submapper 4, registers at $6000) and LZ93D50 (submapper 5, registers at $8000). 16KB switchable PRG + 16KB fixed, 8x1KB CHR, 4-way mirroring. FCG: direct IRQ counter. LZ93D50: latched IRQ + I2C EEPROM (24C02, 256 bytes) with full START/STOP/ACK state machine.

**Mapper 18 — Jaleco SS88006** [done]
- Games (15): Pizza Pop!, Saiyuuki World 2, Ninja Jajamaru: Ginga Daisakusen, Holy Diver (JP)
- 3 switchable 8KB PRG + 1 fixed, 8 independent 1KB CHR banks via nibble-split register writes (D0-D3 per write, two writes per register). CPU-cycle IRQ with configurable counter width (4/8/12/16-bit). PRG RAM with chip-enable and write-protect.

### Priority 6 (remaining): Famicom — Taito/Irem/Sunsoft

**Mapper 80 — Taito X1-005** [S-M] — 7 games
- Games: Minelvaton Saga, Fudou Myouou Den, Mirai Shinwa Jarvas, Kyonshiizu 2, Taito Grand Prix
- 8KB PRG / 1KB CHR banking, 128-byte on-die RAM with security byte ($A0→$A0 check).
- Mapper 207 is a variant with different nametable banking — implement together.

**Mapper 82 — Taito X1-017** [S] — 4 games
- Games: SD Keiji: Blader, Kyuukyoku Harikiri Stadium (3 versions)
- Similar to X1-005 but with banked PRG RAM and CHR banking control. Japan only.

**Mapper 65 — Irem H-3001** [S-M] — 3 games
- Games: Spartan X 2 (Kung-Fu Master sequel), Kaiketsu Yanchamaru 3, Daiku no Gen-san 2
- 8KB PRG banking + 16-bit IRQ counter.

**Mapper 32 — Irem G-101** [S] — 4 games
- Games: Image Fight, Major League, Kaiketsu Yanchamaru 2, Ai Senshi Nicol
- 8KB PRG banking with one-screen mirroring option. Japan only.

**Mapper 67 — Sunsoft 3** [S] — 3 games
- Games: Fantasy Zone II, Maharaja, Nantettatte!! Baseball
- 2KB CHR banking + IRQ counter (4-step latch write sequence).

### Priority 7: Famicom — Trivial discrete mappers

**Mapper 86 — Jaleco JF-13** [XS] — 3-4 games
- Games: Moero!! Pro Yakyuu (Red/Blue), Urusei Yatsura
- 32KB PRG + 8KB CHR via registers at $6000-$7FFF. Trivial.

**Mapper 72 — Jaleco JF-17** [XS] — 3 games
- Games: Pinball Quest, Moero!! Pro Tennis, Moero!! Pro Soccer
- 16KB PRG + 8KB CHR with acknowledge bit (write twice: once with bit 7, once without).

**Mapper 76 — Namco 109 (NAMCOT-3446)** [XS] — 3 games
- Games: Megami Tensei (Digital Devil Story), Famista series
- Subset of Namco 108 using only first 6 registers with 2KB CHR granularity. May already work via mapper 206.

**Mapper 70 — Bandai 74161/32** [XS] — 2-3 games
- Games: Kamen Rider Club, Space Shadow, Family Trainer: Manhattan Police
- 16KB PRG + 8KB CHR via single register. Very similar to mapper 152.

**Mapper 89 — Sunsoft-2 (early)** [XS] — 2-3 games
- Games: Mito Koumon, Tenka no Goikenban
- 16KB PRG + 8KB CHR + single-screen mirroring via single register.

**Mapper 95 — Namco 3425 (NAMCOT-3425)** [XS] — 2 games
- Games: Dragon Buster, Star Wars (Namco)
- Namco 108 variant with nametable control via CHR bank bit.

**Mapper 154 — Namco 3453 (NAMCOT-3453)** [XS] — 2 games
- Games: Devil Man, Youma Ninpou Chou
- Namco 108 variant with single-screen mirroring via bit 6 of bank register.

**Mapper 92 — Jaleco JF-19** [XS] — 2 games
- Games: Moero!! Pro Yakyuu '88 Kettei Ban, Moero!! Pro Soccer
- 16KB PRG (fixed low, switchable high) + 8KB CHR. Upper bank switched by writing with acknowledge bits.

**Mapper 93 — Sunsoft-2 (74161)** [XS] — 2 games
- Games: Fantasy Zone, Shanghai
- 16KB switchable PRG only, no CHR banking. Board-level enable for CHR RAM.

**Mapper 94 — Senjou no Ookami** [XS] — 1 game
- Game: Senjou no Ookami (Commando)
- 16KB PRG banking, no CHR banking. Single register.

**Mapper 97 — Irem TAM-S1** [XS] — 1 game
- Game: Kaiketsu Yanchamaru
- 16KB PRG (fixed low, switchable high — reversed from typical), mirroring control.

### Completed: Priority 8 — Unlicensed & Homebrew

**Mapper 11 — Color Dreams** [done]
- Games: Bible Adventures, Spiritual Warfare, Captain Comic, Crystal Mines, Menace Beach
- 32KB PRG + 8KB CHR via single register. AND-type bus conflicts.

**Mapper 71 — Camerica/Codemasters** [done]
- Games: Micro Machines, Fantastic Adventures of Dizzy, The Ultimate Stuntman, Big Nose the Caveman
- UxROM-like 16KB PRG banking. Fire Hawk variant adds single-screen mirroring.

**Mapper 28 — Action 53** [done]
- Homebrew multicart mapper. Two-step register, 4 PRG banking modes, 32KB CHR RAM.

**Mapper 30 — UNROM 512** [done]
- NESmaker homebrew standard. 16KB PRG + 32KB CHR RAM, submapper bus conflicts.

**Mapper 31 — NSF/Homebrew** [done]
- 4KB PRG bank granularity, 8 slots. Used by NSF players and homebrew.

### Priority 8 (remaining): Unlicensed

**Mapper 79 — NINA-03/NINA-06 (AVE)** [XS] — 16 games
- Games: Tiles of Fate, Krazy Kreatures, Deathbots, F-15 City War
- 32KB PRG + 8KB CHR banking. Trivial.

**Mapper 113 — HES/Sachen multicart** [XS] — 3-6 games
- Games: Various Sachen/HES titles (Asia unlicensed)
- 32KB PRG + 8KB CHR + mirroring via single register.

**Mapper 64 — RAMBO-1 (Tengen)** [M] — 5-6 games
- Games: Shinobi, Rolling Thunder, Klax, Skull & Crossbones, Road Runner
- MMC3 clone with extra PRG mode + CPU-cycle-counting IRQ variant. Most complex missing mapper.

**Mapper 228 — Action 52** [S] — 1 cart (52 mini-games)
- Games: Action 52, Cheetahmen II. Historically notable novelty.

### Mapper coverage summary

Current coverage: **~93-95% of all licensed NES/Famicom games**. Nearly all missing games are **Japan-only Famicom** titles. NA/EU coverage is essentially 100%.

| Priority | Mappers | Category | New games | Effort | Highlight titles |
|----------|---------|----------|-----------|--------|------------------|
| Done | 0-5,7,9-11,16,18,19,21-25,28,30,31,33,34,48,66,68,69,71,73,75,78,87,88,105,118,119,140,152,159,180,184,185,206,210 | Licensed NES + Famicom + homebrew | ~2300 | — | 100% licensed (NTSC+PAL) + VRC + Namco + Taito + Bandai + Jaleco + homebrew |
| 5 | 24,26,85 | Famicom Konami expansion audio | ~5 | M-L | Castlevania III JP, Lagrange Point |
| 6 | 32,65,67,80,82,207 | Famicom Taito/Irem/Sunsoft | ~20 | S-M | Minelvaton Saga, Spartan X 2, Image Fight, Fantasy Zone II |
| 7 | 70,72,76,86,89,92,93,94,95,97,154 | Famicom trivial discrete | ~25 | XS each | Megami Tensei, Pinball Quest, Dragon Buster |
| 8 | 64,79,113,228 | Unlicensed | ~30 | XS-M | Shinobi, Rolling Thunder, Tiles of Fate |

Implementing priorities 6+7 (trivial/moderate mappers) would bring coverage to **~98%**. VRC6/VRC7 (priority 5) are low game count but high prestige — they're the expansion audio showcases.

PAL-exclusive licensed games need **zero** new mappers — all use mappers 0/1/2/4/7 already supported.

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
- [x] Inhibit screensaver/suspend while running (D-Bus `org.freedesktop.ScreenSaver.Inhibit` via gdbus on Linux) [S]
- [x] Configurable key/button bindings [M]
  - `Action` enum (27 variants): P1/P2 NES buttons + system actions (save/load/rewind/mute/etc.)
  - `InputBindings` with separate keyboard (`KeyId`) and gamepad (`GamepadButtonId`) binding vectors
  - Press-to-bind overlay UI: F10 or Emulation → Input Settings menu item
  - In-framebuffer overlay using 8x8 font, state machine (SelectAction → ActionMenu → WaitingForInput)
  - Persisted in settings.txt as `bind_kb_*` / `bind_gp_*` key=value pairs; no bind keys = defaults (backward compatible)
  - Two-player keyboard support (P2 bindings assignable, no defaults)
  - Desktop only (web/libretro excluded — libretro has its own remapping, web can follow later)
- [ ] Per-controller gamepad profiles [S]
- [ ] Turbo A/B buttons (optional toggle) [XS]

---

## On-Screen Display

8x8 bitmap font overlay rendered directly into the framebuffer (core/src/emu/gfx/font.rs, overlay.rs).
1px outlined text for readability on any background. Toggle with Tab (desktop/web) or double-tap canvas (mobile).

- [x] On-screen message logger (semi-transparent overlay) [M]
  - Toast notifications for save/load state, slot cycling, errors
  - Auto-expire after 120 frames (~2 seconds), capped at 4 simultaneous
- [x] Optional FPS counter overlay [S]
  - Shows emulation time in ms and percentage of frame budget (region-aware: 16.64ms NTSC, 20.00ms PAL)
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
- [x] No-ROM launch shows CRT static noise with "Open a ROM to play" banner [XS]
- [x] Unsupported mapper errors toast on-screen [XS]
- [ ] Drag-and-drop ROM file onto window to load [S]

---

## Emulation Features

- [x] Rewind (hold W or right trigger to scrub back through 10s of gameplay at 2x speed) [L]
- [x] Fast-forward (hold Space for uncapped speed) [S]
- [ ] Slow-motion (0.5x speed toggle) [XS]
- [ ] Screenshot (save framebuffer as PNG) [S]
- [ ] Video recording (save to GIF or MP4) [L]
- [ ] Game Genie code support [M]
- [ ] NSF player mode (play NES Sound Format music files) [L]
- [x] PAL timing mode (50 Hz, 3.2:1 PPU/CPU ratio via master clock sub-dot accumulator, region auto-detect from iNES header + filename heuristic, `--region` CLI override, all 10 blargg PAL APU tests passing) [M]

---

## Video / Rendering

Currently: pixels crate (wgpu) on macOS/Windows, GTK3 GLArea (OpenGL 3.3 via glow) on Linux. Integer scaling (default) or fill scaling (I key), fullscreen (F11).

- [x] CRT scanline filter (CRT-Lottes-Fast shader, F9 toggle, persisted in settings) [L]
  - WGSL for wgpu (macOS/Windows), GLSL ES 3.0 for WebGL2 (web), GLSL 3.30 for Linux GLArea
- [ ] NTSC composite video simulation (blargg's nes_ntsc or similar) [L]
- [ ] Configurable window scale (1x-6x) [S]
- [ ] Aspect ratio correction (8:7 pixel aspect ratio for accurate NES output) [S]
- [x] Overscan cropping option (hide top/bottom 8 scanlines, toggled via menu, persisted) [S]

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
| cpu_exec_space | PPU I/O test | ✅ | integration_tests.rs |
| cpu_interrupts_v2 | 1-cli_latency | ✅ | integration_tests.rs |
| cpu_interrupts_v2 | 2-nmi_and_brk, 3-nmi_and_irq, 4-irq_and_dma, 5-branch_delays_irq | ❌ ignored | integration_tests.rs |
| instr_timing | 2-branch_timing | ✅ | integration_tests.rs |
| instr_timing | 1-instr_timing | ✅ | integration_tests.rs |
| cpu_timing_test6 | 1 ROM | ✅ | integration_tests.rs |
| instr_misc | abs_x_wrap, branch_wrap, dummy_reads | ✅ | integration_tests.rs |
| instr_misc | dummy_reads_apu | ❌ ignored | integration_tests.rs |
| branch_timing_tests | all 3 | ✅ | integration_tests.rs |
| apu_test | all 8 singles | ✅ | apu/mod.rs |
| blargg_apu_2005 | all 11 | ✅ | apu/mod.rs |
| apu_reset | all 6 | ✅ | apu/mod.rs |
| pal_apu_tests | all 10 | ✅ | apu/mod.rs |
| dmc_tests | status, status_irq, buffer_retained, latency | ✅ | integration_tests.rs |
| oam_read | 1 | ✅ | integration_tests.rs |
| ppu_vbl_nmi | 01, 03, 04, 05, 09, 10 | ✅ | integration_tests.rs |
| ppu_vbl_nmi | 02, 06, 07, 08 | ❌ ignored | integration_tests.rs |
| blargg_ppu_tests_2005 | palette_ram, sprite_ram, vram_access, vbl_clear_time, power_up_palette | ✅ | integration_tests.rs |
| ppu_open_bus | 1 | ✅ | integration_tests.rs |
| oam_stress | 1 | ❌ ignored | integration_tests.rs |
| mmc3_test | all 6 | ✅ | memory/mapper/mmc3.rs |
| mmc3_test_2 | all 6 | ✅ | memory/mapper/mmc3.rs |
| apu_mixer | 4 | ⏸ ignored (release-only) | apu/mod.rs |
| vbl_nmi_timing | all 7 | ✅ | integration_tests.rs |
| sprite_hit_tests_2005 | all 11 | ✅ | integration_tests.rs |
| sprite_overflow_tests | all 5 | ✅ | integration_tests.rs |
| ppu_read_buffer | 1 | ❌ ignored (DMA+PPU bus side-effects) | integration_tests.rs |
| cpu_dummy_reads | 1 | ✅ | integration_tests.rs |
| cpu_dummy_writes | all 2 | ✅ | integration_tests.rs |
| dmc_dma_during_read4 | all 5 | ❌ ignored | integration_tests.rs |
| sprdma_and_dmc_dma | all 2 | ❌ ignored | integration_tests.rs |

### Ignored tests — failure analysis and fix plan

13 tests are wired up but `#[ignore]`d. Grouped by root cause, ordered by recommended attack priority.

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
| ~~`10-even_odd_timing`~~ | ~~Odd-frame clock skip timing vs BG enable~~ | ~~Low~~ | **FIXED** — latch BG enable at pre-render dot 337 for skip decision | ✅ |
| **Priority 3 — Small targeted fixes** | | | | |
| ~~`1-instr_timing`~~ | ~~Cycle counts for unofficial opcodes~~ | ~~Very low~~ | **FIXED** — added 22 missing unofficial opcode definitions (NOPs, ANC, ALR, ARR, XAA, LAX#, SBX, SHA, SHX, SHY, TAS, LAS) | ✅ |
| `04-dummy_reads_apu` | Dummy reads on indexed ops trigger APU side effects | Low — only if game does indexed write to $40xx | APU registers respond to dummy read at uncorrected address | S |
| `5-branch_delays_irq` | Branch instruction delays IRQ by 1 cycle | Low — extremely narrow timing window | IRQ sampling during taken branch needs page-cross check | S |
| **Priority 4 — Deeper plumbing** | | | | |
| `4-irq_and_dma` | OAM DMA delays IRQ servicing | Low — IRQ-during-DMA is rare in practice | DMA doesn't model per-cycle IRQ polling | L |
| ~~`ppu_open_bus`~~ | ~~PPU bus bits decay to 0 after ~600ms~~ | ~~Low~~ | **FIXED** — per-bit decay timer + OAM attribute bit masking + palette partial refresh | ✅ |
| **Priority 5 — Deprioritize** | | | | |
| `oam_stress` | OAM address/read/write under stress | Low — test only passes 1/4 on real HW | PPU-CPU alignment jitter, may be unfixable deterministically | S |
| ~~`cpu_exec_space_ppuio`~~ | ~~Code execution from PPU I/O space~~ | ~~Very low~~ | **FIXED** — added cycle-2 dummy read of PC+1 for RTS, RTI, and BRK | ✅ |

**Attack order rationale:**

1. **PPU VBL/NMI timing (02, 05, 06, 07, 08)** — biggest game compatibility payoff. These are all facets of the same subsystem: exact-dot VBL flag set, NMI propagation delay, suppression, and $2000-triggered NMI edge detection. Many games with flickering or missing frames trace back to NMI timing off by 1-2 PPU dots. Fix them together.

2. **NMI hijacking (nmi_and_brk, nmi_and_irq) + even/odd timing** — second pass on code we attempted and reverted. With correct VBL timing from step 1, the hijacking logic should be straightforward: check for pending NMI edge before the vector fetch on cycle 6 of BRK/IRQ.

3. **Small wins (dummy_reads_apu, branch_delays_irq)** — quick targeted fixes. Wire APU dummy reads, adjust branch IRQ sampling. (instr_timing_1 already fixed.)

4. **PPU open bus + IRQ/DMA** — lower game impact, more plumbing work.

5. **oam_stress** — flaky on real hardware.

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


---

## Quality / Accuracy

- [x] APU mixer capture/reference workflow for square, triangle, noise, and DMC channels
  - Headless `CapturingAudioOutput`, WAV export, hardware reference MP3 fixtures, JSON/PNG analysis reports
  - CI runs `cargo test --release test_apu_mixer -- --ignored --nocapture --test-threads=4`
- [ ] PPU VBL/NMI dot-accurate timing (see ignored test plan above) [M]
- [ ] NMI hijacking during BRK/IRQ vector fetch [M]
- [ ] Sprite 0 hit: upgrade from position-based to pixel-overlap accuracy [M]
- [ ] PPU open bus decay behavior [M]
- [x] CPU unofficial/illegal opcodes (LAX, SAX, DCP, ISB, SLO, SRE, RLA, RRA, ANC, ALR, ARR, SBX, SHA, SHX, SHY, TAS, LAS, XAA + NOP variants) [L]
- [ ] APU DMC DMA cycle stealing accuracy [L]

---

## Misc / Maybe

- [x] Persistent settings (~/.config/krankulator/settings.txt) for integer scaling and CRT scanlines [M]
- [ ] Netplay (rollback-based) [XXL]
- [ ] Input recording/playback (TAS support) [L]
- [ ] ROM database (hash-based game identification, auto-select mapper) [M]
- [ ] Cheat search (RAM watch/search for modifying values) [L]
