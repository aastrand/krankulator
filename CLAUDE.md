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
- Currently renders the full frame once per VBlank via `gfx::render()` — being migrated
  to cycle-accurate per-dot rendering

**Memory System (src/emu/memory/)**
- Memory mappers for different cartridge types (NROM, MMC1, MMC3, UxROM, AxROM, CNROM)
- Handles bank switching and memory mirroring
- Separates CPU and PPU memory spaces
- Mapper trait includes `ppu_cycle_260()` hook for scanline-counting mappers (MMC3)

**APU (src/emu/apu/)**
- Audio Processing Unit with pulse, triangle, noise, and DMC channels
- Frame counter for audio timing
- Sound generation for authentic NES audio

**Graphics (src/emu/gfx/)**
- Frame buffer (`buf.rs`) and palette lookup table (`palette.rs`)
- `mod.rs` contains the legacy full-frame renderer — will be removed as rendering moves
  into the PPU

**Audio (src/emu/audio.rs)**
- Audio output handling using rodio crate

### Key Design Patterns

**CPU-PPU Synchronization**
- The emulator runs in discrete cycles, with proper timing between CPU, PPU, and APU
- PPU runs at 3x CPU speed (3 PPU cycles per CPU cycle)
- Currently: interleaved per-cycle (CPU instruction, then 3 PPU dots, repeat)
- Target: catch-up model where PPU syncs lazily on register access

**Memory Mapping**
- Uses trait objects for different mapper implementations
- Mappers handle bank switching and memory mirroring specific to cartridge types

**Test-Driven Development**
- Extensive test suite using actual NES test ROMs
- Tests cover CPU instructions, PPU behavior, APU functionality, and timing

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

## Important Implementation Details

**6502 CPU Emulation**
- All official opcodes implemented with proper cycle counts
- Accurate flag handling for arithmetic and logical operations
- Support for all addressing modes

**PPU Implementation**
- Proper VRAM address handling with internal registers (v, t, x, w)
- VBlank timing and NMI generation
- Scroll register updates at correct cycle points during rendering
- Current limitation: full-frame rendering at VBlank (no mid-frame raster effects)
- Sprite 0 hit is approximate (position-based, not pixel-overlap)
- Active migration to per-dot cycle-accurate rendering

**Memory Mappers**
- NROM, MMC1, MMC3, UxROM, AxROM, CNROM
- Proper mirroring for nametables and palettes

**Audio System**
- Length counters for all channels
- Proper frame counter timing
- DMC channel with sample playback

The emulator is designed to be highly accurate and passes most standard NES test ROMs, making it suitable for both educational purposes and actual game compatibility testing.

## Active Work

Migrating PPU from full-frame-at-VBlank rendering to per-dot, catch-up-synchronized
pipeline. This will enable mid-frame raster effects (split-screen scrolling, scanline
IRQ-driven effects, sprite 0 hit polling). The migration is incremental across 5 phases
— each phase produces a working emulator. The old `gfx::render()` path will be removed
once rendering is fully integrated into the PPU.