# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Krankulator is a NES (Nintendo Entertainment System) emulator written in Rust. It emulates the MOS Technology 6502 CPU, PPU (Picture Processing Unit), APU (Audio Processing Unit), and various memory mappers to run NES games and test ROMs.

## Common Commands

### Building and Running
```bash
# Build the project
cargo build

# Run with a NES ROM file
cargo run -- input/nes/nestest.nes

# Run in headless mode (no graphics)
cargo run -- --headless input/nes/nestest.nes

# Capture headless audio to a WAV file
cargo run -- --wav-out /tmp/krankulator.wav input/nes/apu/square.nes

# Run with debugging enabled
cargo run -- --debug --verbose input/nes/nestest.nes

# Add breakpoints
cargo run -- --breakpoint 0xC000 input/nes/nestest.nes

# Specify loader type (nes, ascii, bin)
cargo run -- --loader ascii input/ascii/instructions

# Run with custom code starting address
cargo run -- --codeaddr 0x400 --loader bin input/bin/test.bin
```

### Testing
```bash
# Run all tests
cargo test

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
cargo check

# Format code
cargo fmt

# Run clippy for linting
cargo clippy
```

## Architecture Overview

### Core Components

**Emulator (src/emu/mod.rs)**
- Main emulator struct that orchestrates CPU, PPU, APU, and memory
- Handles cycle-accurate timing between components
- Manages emulation state (running, debugging, breakpoints)

**CPU (src/emu/cpu/mod.rs)**
- MOS 6502 CPU implementation with all official opcodes
- Handles instruction decoding, execution, and flag management
- Supports debugging features like breakpoints and register inspection

**PPU (src/emu/ppu/mod.rs)**
- Picture Processing Unit for graphics rendering
- Implements proper VRAM addressing with internal registers (v, t, x, w)
- Handles NMI (Non-Maskable Interrupt) generation for VBlank
- Per-dot cycle-accurate rendering

**Memory System (src/emu/memory/)**
- Memory mappers for different cartridge types (NROM, MMC1, MMC3, UxROM, AxROM, CNROM, BNROM, GxROM)
- Handles bank switching and memory mirroring
- Separates CPU and PPU memory spaces
- Mapper trait includes `ppu_cycle_260()` hook for scanline-counting mappers (MMC3)
- `PpuBus` shared struct handles CHR read/write, nametable mirroring, palette RAM, and VRAM for simple mappers
- AND-type bus conflict emulation for discrete logic mappers (BNROM, GxROM)

**APU (src/emu/apu/)**
- Audio Processing Unit with pulse, triangle, noise, and DMC channels
- Frame counter for audio timing
- Sound generation for authentic NES audio

**Graphics (src/emu/gfx/)**
- Frame buffer (`buf.rs`) and palette lookup table (`palette.rs`)
- `mod.rs` contains the full-frame renderer

**Audio (src/emu/audio/)**
- Audio output handling using rodio crate
- WAV writer (`wav.rs`) for capturing test output
- `CapturingAudioOutput` backend for headless audio capture in tests

### Key Design Patterns

**CPU-PPU Synchronization**
- The emulator runs in discrete cycles, with proper timing between CPU, PPU, and APU
- PPU runs at 3x CPU speed (3 PPU cycles per CPU cycle)
- Interleaved per-cycle (CPU instruction, then 3 PPU dots, repeat)

**Memory Mapping**
- Uses trait objects for different mapper implementations
- Mappers handle bank switching and memory mirroring specific to cartridge types

**Test-Driven Development**
- Extensive test suite using actual NES test ROMs
- Tests cover CPU instructions, PPU behavior, APU functionality, and timing
- APU mixer integration tests compare captured audio against hardware reference recordings
  - Requires Python venv: `cd scripts && uv venv && uv pip install -r requirements.txt`
  - Tests skip gracefully if Python venv or reference files are missing
  - Reference recordings are in `input/nes/apu/mixer_reference/`
  - CI runs the 4 ignored mixer tests separately in release mode

## File Structure

- `src/main.rs` - Entry point with command-line argument parsing
- `src/emu/` - Core emulation components
- `src/util/` - Utility functions for hex parsing, file I/O
- `input/` - Test ROMs and data files
  - `input/nes/` - NES ROM files for testing
  - `input/ascii/` - ASCII assembly test files
  - `input/bin/` - Binary test files
- `opcodes/` - CPU opcode generation scripts

## Testing Strategy

The project uses both unit tests and integration tests:

**Unit Tests**
- Test individual CPU instructions and flag behavior
- Verify PPU register operations and timing
- Test memory mapper functionality

**Integration Tests**
- Run complete NES test ROMs (nestest, blargg test suite)
- Verify cycle-accurate behavior against known-good logs
- Test various cartridge mappers with real games
- Compare APU mixer output against square, triangle, noise, and DMC hardware recordings

## Important Implementation Details

**6502 CPU Emulation**
- All official opcodes implemented with proper cycle counts
- Accurate flag handling for arithmetic and logical operations
- Support for all addressing modes

**PPU Implementation**
- Proper VRAM address handling with internal registers (v, t, x, w)
- VBlank timing and NMI generation
- Scroll register updates at correct cycle points during rendering
- Per-dot cycle-accurate rendering
- Sprite 0 hit is approximate (position-based, not pixel-overlap)

**Memory Mappers**
- NROM, MMC1, MMC3, UxROM, AxROM, CNROM, BNROM, GxROM
- Proper mirroring for nametables and palettes
- BNROM/GxROM use AND-type bus conflicts (written value ANDed with ROM byte at write address)
- BNROM uses full 8-bit bank register (not masked to 2 bits), wrapping via modulo
- Simple mappers (BNROM, GxROM, CNROM, UxROM, AxROM) share PPU logic via `PpuBus`

**Audio System**
- Length counters for all channels
- Proper frame counter timing
- DMC channel with sample playback
- APU soft reset preserves channel registers and replays last $4017 write
- Pulse and noise timers tick at half CPU rate; triangle ticks every CPU cycle
- Per-cycle mixer accumulation for proper anti-aliasing of high-frequency noise
- Mixer tests compare emulator WAV output against hardware reference MP3 recordings

**CPU Bus**
- Open bus emulation: write-only registers return last value on data bus
- Indexed addressing performs dummy reads at uncorrected (pre-page-fix) address
- RMW instructions always perform the dummy read regardless of page crossing

The emulator passes all standard NES test ROM suites (Klaus2m5, nestest, blargg CPU/PPU/APU/APU 2005/timing, APU reset, cpu_exec_space, CPU interrupts, PPU OAM, VRAM access).

