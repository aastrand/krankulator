# krankulator

A cycle-stepped NES emulator written in Rust.

Started as a learning-Rust project — a bare 6502 emulator iterating against the [Klaus2m5](https://github.com/Klaus2m5/6502_65C02_functional_tests) functional test suite. NES support grew from there: naive VBlank rendering and input, validated against Kevin Horton's [nestest](http://www.qmtpro.com/~nes/misc/) log; then mapper support for real cartridges; then APU audio and cycle-accurate PPU rendering in the AI-assisted era.

![emulator screenshot 1](screenshots/emulator.png)
![emulator screenshot 2](screenshots/emulator2.png)
![emulator screenshot 3](screenshots/emulator3.png)
![emulator screenshot 4](screenshots/web.png)
![emulator screenshot 5](screenshots/mobileweb.png)


## Features

- **MOS 6502 CPU** — all official opcodes plus common unofficial ones (LAX, SAX, DCP, ISB, SLO, SRE, RLA, RRA)
- **PPU** — per-dot cycle-accurate rendering, sprite evaluation, sprite 0 hit, even/odd frame timing
- **APU** — pulse, triangle, noise, and DMC channels with nonlinear NES mixing, per-cycle accumulation, and IIR high-pass/low-pass filtering at 44.1 kHz
- **Mappers** — NROM (0), MMC1 (1), UxROM (2), CNROM (3), MMC3 (4), AxROM (7), BNROM (34), GxROM (66)
- **Battery-backed SRAM** — persistent `.sav` files for MMC1/MMC3 cartridges
- **Savestates** — 4 slots per game, custom binary format with full state serialization (CPU, PPU, APU including audio filter state, memory, mappers, controllers)
- **Audio output** via [rodio](https://github.com/RustAudio/rodio), plus headless capture and WAV export for analysis
- **Windowed rendering** via [winit](https://github.com/rust-windowing/winit) + [pixels](https://github.com/parasyte/pixels)
- **Gamepad support** — GCController on macOS, gilrs on Linux/Windows; two-player with Joy-Con pair auto-split
- **WebAssembly frontend** — runs in the browser with Canvas 2D rendering, AudioWorklet audio, and touch controls for mobile
- **On-screen overlay** — 8x8 bitmap font with outlined text for frame time display (Tab) and toast notifications (save/load/slot); double-tap on mobile
- **Headless mode** for testing and CI

## Architecture

```mermaid
graph TD
    Main["desktop/main.rs<br/>CLI + ROM loader"] --> Emu["Emulator<br/>cycle loop & timing"]
    Web["web/lib.rs<br/>wasm entry + rAF loop"] --> Emu

    Emu --> CPU["CPU<br/>6502 + unofficial ops"]
    Emu --> PPU["PPU<br/>dot renderer"]
    Emu --> APU["APU<br/>audio synthesis"]
    Emu --> Mem["MemoryMapper<br/>trait object"]

    Mem --> NROM
    Mem --> MMC1
    Mem --> MMC3
    Mem --> Simple["Simple mappers<br/>UxROM, CNROM, AxROM,<br/>BNROM, GxROM"]
    Simple --> PpuBus["PpuBus<br/>shared CHR/VRAM/palette"]

    Emu --> IO["IOHandler<br/>trait object"]
    IO --> Winit["WinitPixels<br/>window + input"]
    IO --> WebIO["WebIO<br/>Canvas 2D + keyboard + touch"]
    IO --> Headless["Headless<br/>testing"]

    APU --> Audio["AudioBackend<br/>trait object"]
    Audio --> Rodio["rodio + ringbuf"]
    Audio --> Worklet["AudioWorklet"]
    Audio --> Silent["silent / capture"]

    Emu --> SS["Savestate<br/>binary serialize"]
```

The project is a Cargo workspace with three crates: `core/` (platform-independent emulation
library), `desktop/` (native frontend), and `web/` (WebAssembly frontend). The core compiles
to both native and wasm32 targets with zero cfg gates.

The emulator runs a tight cycle loop: each iteration executes one CPU cycle, then steps
the PPU three dots (3:1 PPU-to-CPU ratio), then cycles the APU. Memory mappers are trait
objects — each cartridge type implements its own bank switching, mirroring, and IRQ logic
(e.g. MMC3 scanline counter). Simple discrete-logic mappers (UxROM, CNROM, AxROM, BNROM,
GxROM) share PPU bus logic via `PpuBus`; BNROM and GxROM emulate AND-type bus conflicts.
The IO and audio layers are traits, allowing desktop, web, or headless operation with the
same emulation core.

## Building and running

### Desktop

```bash
cargo build --release
cargo run --release -- path/to/game.nes
```

### Web

```bash
cargo install trunk
cd web && trunk serve --port 8080
```

### CLI options

```
cargo run -- [OPTIONS] <INPUT>

OPTIONS:
    --headless           Run without graphics
    --wav-out <PATH>     Capture headless audio to a WAV file
    --debug              Enable debugger
    --verbose / --quiet  Control log output
    -b, --breakpoint     Add CPU breakpoint (e.g. 0xC000)
    -l, --loader         Loader type: nes (default), ascii, bin
    --codeaddr           Code start address for bin/ascii loaders
```

### Controls

#### Keyboard

| Key | Action |
|-----|--------|
| Arrow keys | D-pad |
| Z | A button |
| X | B button |
| C | Start |
| V | Select |
| S | Save state |
| A | Load state |
| Q | Cycle save slot (0-3) |
| R | Reset |
| M | Mute/unmute log |
| 1-5 | Toggle individual APU channels |
| 0 | Master mute |
| Tab | Toggle frame time overlay |
| Esc | Quit |

#### Gamepad (auto-detected)

Standard controllers (Pro Controller, Xbox, PS, 8BitDo) use conventional mapping. Joy-Con pair auto-splits into P1 (right) and P2 (left):

| Button (P1 right Joy-Con) | Action |
|---------------------------|--------|
| Stick | D-pad |
| Switch X | NES A |
| Switch B | NES B |
| + | Start |
| R / ZR | Select |
| Switch A | Load state |
| Switch Y | Save state |

| Button (P2 left Joy-Con) | Action |
|--------------------------|--------|
| Stick | D-pad |
| D-pad down | NES A |
| D-pad left | NES B |
| - | Start |
| L / ZL | Select |

## Testing

Tests cover CPU instructions, PPU behavior, APU channels, memory mappers, and savestate
round-trips. Integration tests run actual NES test ROMs to validate accuracy:

```bash
cargo test              # run all tests
cargo test -- --ignored # run slow tests too
```

### APU mixer reference tests

The mixer tests compare captured emulator WAV output against hardware reference MP3
recordings for square, triangle, noise, and DMC channel ROMs. They are ignored for
normal local runs, but CI runs them in a separate release-mode job.

```bash
cd scripts
uv venv
uv pip install -r requirements.txt
cd ..
cargo test --release test_apu_mixer -- --ignored --nocapture --test-threads=4
```

The comparison script emits JSON diagnostics and PNG reports for spectrogram,
waveform, spectrum, and envelope comparisons.

### Test ROM suites

| Suite | Tests | Status |
|-------|-------|--------|
| [Klaus2m5 6502 functional](https://github.com/Klaus2m5/6502_65C02_functional_tests) | Full instruction + addressing mode coverage | ✅ |
| [nestest](http://www.qmtpro.com/~nes/misc/) | CPU instruction correctness (official + unofficial) | ✅ |
| [Blargg CPU](https://github.com/christopherpow/nes-test-roms) | `official_only` — all official opcodes | ✅ |
| Blargg PPU | VBlank basics/set/clear time, NMI control/timing/on/off, VBL suppression, even/odd frames/timing | ✅ |
| Blargg APU | Length counters, length table, IRQ flag, jitter, len timing, IRQ flag timing, DMC basics, DMC rates | ✅ |
| Blargg APU 2005 | Length counter, length table, IRQ flag/timing, clock jitter, len timing mode 0/1, reset timing, len halt timing, len reload timing | ✅ |
| APU mixer references | Square, triangle, noise, and DMC output compared against hardware recordings | ✅ |
| APU reset | $4015 cleared, $4017 timing/written, IRQ flag cleared, len ctrs enabled, works immediately | ✅ |
| cpu_exec_space | APU register space execution | ✅ |
| Blargg instruction timing | Cycle-accurate instruction timing | ✅ |
| CPU interrupts | NMI and BRK interaction | ✅ |
| PPU OAM | OAM read, OAM stress | ✅ |
| CPU registers/RAM | Registers after reset, RAM after reset | ✅ |
| VRAM access | VRAM read/write validation | ✅ |

## Platform support

**Desktop:** Built on cross-platform crates (winit, pixels, rodio) — runs on macOS, Linux, and
Windows. Tested primarily on macOS.

**Web:** Runs in any modern browser (Firefox, Chrome, Safari) via WebAssembly. Requires
AudioWorklet support for sound. Mobile devices get a dedicated landscape touch layout with
virtual d-pad and action buttons.

## License

This project is licensed under the [PolyForm Noncommercial License 1.0.0](LICENSE). You may use, modify, and distribute the software for any noncommercial purpose. See the LICENSE file for full terms.

## Resources

- [nesdev wiki](https://www.nesdev.org/wiki/) — the authoritative NES hardware reference
- [6502 instruction set](https://www.masswerk.at/6502/6502_instruction_set.html)
- [6502 addressing modes](https://slark.me/c64-downloads/6502-addressing-modes.pdf)
- [Klaus2m5 functional tests](https://github.com/Klaus2m5/6502_65C02_functional_tests)
- [nestest](http://www.qmtpro.com/~nes/misc/) — Kevin Horton's CPU test ROM
- [NES rendering overview](https://austinmorlan.com/posts/nes_rendering_overview/)
- [nes-test-roms](https://github.com/christopherpow/nes-test-roms) — collection of NES test ROMs (Blargg et al.)
