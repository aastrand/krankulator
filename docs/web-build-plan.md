# Krankulator Web Build Plan

## Status

**Phase 1 (MVP) — Complete.** The emulator runs in Firefox/Chrome with video, audio, keyboard input, ROM loading, and CI.

**Implementation diverged from original plan in one key way:** instead of cfg gates in a single crate, we restructured into a Cargo workspace (`core/`, `desktop/`, `web/`). The core compiles to wasm32 with zero cfg gates.

## What was built (Phase 1)

- [x] **Step 1: Core compiles on wasm32** — workspace split, no cfg gates needed
- [x] **Step 2: Frame-at-a-time execution** — `run_one_frame()` + `audio.flush()` per frame
- [x] **Step 3: AudioWorklet audio** — ring buffer worklet, per-frame postMessage, drift compensation via buffer level feedback
- [x] **Step 4: Web IO handler** — Canvas 2D putImageData, keyboard input (only prevent_default on mapped keys)
- [x] **Step 5: Web entry point** — file picker, "I'm feeling lucky" button, rAF loop with NTSC frame pacing
- [x] **Step 6: Build & test** — trunk with release mode, COOP/COEP headers
- [x] **Step 7: CI** — wasm32 compile check job, GitHub Pages auto-deploy on master push
- [x] **Custom domain** — krankulator.teknodromen.se via CNAME

## Phase 2: Polish

- [ ] SharedArrayBuffer ring buffer for audio (replace postMessage, lower latency)
- [ ] Save states to `localStorage`
- [ ] Battery-backed RAM (SRAM) persistence to `localStorage`
- [ ] Responsive CSS layout (scale canvas to viewport, max 4x)
- [ ] Gamepad API support
- [ ] On-screen touch controls for mobile
- [ ] Drag-and-drop ROM loading onto canvas
- [ ] Pause audio on tab visibility change
- [ ] Interactive debug REPL (shrust) in desktop crate

## Phase 3: Nice-to-have

- [ ] Web Worker for emulation (off main thread — prevents dropped frames)
- [ ] wgpu rendering (enables CRT shaders, NTSC filter)
- [ ] PWA manifest + service worker (offline play)
- [ ] Mobile-optimized layout with proper touch UX
- [ ] Netplay via WebRTC
- [ ] Performance profiling (measure frame time budget)

## Architecture (as built)

```
Cargo.toml          — Virtual workspace manifest
core/               — Platform-independent emulation library (only dep: hex)
  src/emu/          — CPU, PPU, APU, memory mappers, IO traits, audio traits
desktop/            — Native frontend (winit + pixels + rodio + shrust)
web/                — WebAssembly frontend (web-sys + Canvas 2D + AudioWorklet)
  src/lib.rs        — wasm-bindgen entry, IOHandler/AudioBackend impls, rAF loop
  assets/           — audio_processor.js, background.jpg, CNAME
  Trunk.toml        — Build config (release, COOP/COEP headers)
```

Key traits in core that frontends implement:
- `IOHandler` — `init()`, `log()`, `poll()`, `render()`, `exit()`
- `AudioBackend` — `push_samples()`, `flush()`, `clear()`

## Audio architecture

```
APU (per cycle) → push_samples() → Vec<f32> buffer (Rust)
                                         ↓ flush() once per frame
                                    postMessage(Float32Array)
                                         ↓
                              AudioWorklet ring buffer (8192 samples)
                                         ↓ process() every 128 samples
                                    speaker output

Drift compensation: worklet reports buffer level every 8 process calls.
Rust drops samples when level > 4096 (high water mark) to prevent
unbounded growth from 60.0988 vs 60.0 Hz mismatch.
```

## Reference

| | TetaNES | nes-rust | Rustico | **Krankulator** |
|---|---|---|---|---|
| **Rendering** | wgpu (WebGL2) | Canvas 2D | Canvas 2D (Worker) | **Canvas 2D** |
| **Audio** | AudioBufferSourceNode | ScriptProcessorNode | AudioWorklet | **AudioWorklet** |
| **Main loop** | winit spawn_app | JS rAF | Worker + rAF | **rAF + time accumulator** |
| **Build** | trunk | wasm-pack | custom | **trunk** |
