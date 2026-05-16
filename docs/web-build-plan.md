# Krankulator Web Build Plan

## Context

Krankulator is a cycle-accurate NES emulator with a clean architecture: `IOHandler` trait for input/rendering, `AudioBackend` trait for audio, and a pure emulation core with no platform dependencies. The goal is to compile to WebAssembly and run in the browser with video, audio, input, and ROM loading working. This is tagged as [XL] in TODO.md.

## Decisions

- **Project structure:** Single crate with `#[cfg(target_arch = "wasm32")]` conditionals (no workspace split)
- **Audio:** AudioWorklet from the start (not deprecated ScriptProcessorNode)
- **Demo ROM:** Fetch on demand from URL when user clicks "Try Demo" (not bundled in wasm binary)
- **Rendering:** Canvas 2D via web-sys (not pixels/wgpu — smaller binary, simpler)
- **Main loop:** requestAnimationFrame (not winit on wasm — avoids refactoring ownership model)
- **Build tool:** trunk

## Current Architecture (what we're working with)

**Already platform-abstract (no changes needed):**
- `Emulator` core (`src/emu/mod.rs`) — CPU, PPU, APU, memory mappers
- `IOHandler` trait (`src/emu/io/mod.rs:34`) — `init()`, `poll()`, `render()`, `exit()`
- `AudioBackend` trait (`src/emu/audio/mod.rs:12`) — `push_samples()`, `clear()`, `drain_captured()`
- `HeadlessIOHandler` — proves core runs without windowing
- `Buffer` (`src/emu/gfx/buf.rs`) — simple `Vec<u8>` of 256x240 RGB pixels

**Platform-specific (needs wasm alternatives):**
- `WinitPixelsIOHandler` — winit 0.30 + pixels 0.17 (wgpu-backed)
- `AudioOutput` — rodio + ringbuf (rodio has no wasm support)
- `main.rs` — clap CLI parsing, `Emulator::run()` blocking loop
- `std::time::Instant` — used in IO handler for frame pacing, and in `Emulator` for stats
- `shrust` — debug REPL shell

**The main loop problem:** `Emulator::run()` is a blocking `loop { cycle() }`. The browser needs control returned every frame via `requestAnimationFrame`. We need a `run_one_frame()` method.

**Why not winit on wasm?** winit 0.30 supports wasm32, but:
1. `pump_app_events()` (our current approach) is not supported on wasm
2. `spawn_app()` requires winit to own the ApplicationHandler — means restructuring emulator ownership
3. TetaNES designed around this from the start; retrofitting it is a bigger refactor
4. Raw requestAnimationFrame + web-sys is ~20 lines and keeps our existing ownership model

---

## Known Gotchas (bake into implementation)

### Audio batching
`push_samples()` gets called from `cycle()` with tiny batches (a few samples at a time, ~735 total per frame). Calling `postMessage()` per batch would be catastrophically slow. The web `AudioBackend` must accumulate samples internally and flush once per frame. Options:
- Buffer in the web AudioBackend impl, send in one `postMessage()` when buffer reaches ~735 samples (one frame's worth)
- Or add a `flush()` call after `run_one_frame()` in the rAF loop

### 60.0988 Hz vs 60.000 Hz audio drift
NES runs at 60.0988 fps but requestAnimationFrame fires at 60.000 Hz (display vsync). That 0.1% mismatch means the emulator produces ~0.7 extra audio samples per frame. Over minutes, the audio buffer in the worklet grows unbounded. Fix: audio-driven timing feedback loop:
- After each frame, check audio buffer fill level (worklet reports back via postMessage)
- If buffer is too full (>2 frames of samples): skip emulating one frame
- If buffer is too empty (<0.5 frames): run an extra frame
- This naturally syncs emulation speed to audio playback rate

### `rand` 0.7.3 needs upgrading
`rand` 0.7 depends on `getrandom` 0.1.x which has different wasm feature flags than 0.2.x. Bump to `rand` 0.8+ to get clean `getrandom = { version = "0.2", features = ["js"] }` support. Otherwise it's dependency hell. Check what `rand` is actually used for — if it's just initial RAM state, we might not even need it on wasm (could seed from `Math.random()` via js-sys).

### `println!` on wasm is silent
`println!` compiles but goes nowhere on wasm32. For debugging, use `web_sys::console::log_1()`. Options:
- Use the `log` crate + `console_log` backend (adds deps)
- Simple `cfg`-gated macro: `println!` on native, `console::log` on wasm
- Or just accept silence for non-error paths and use `console_error_panic_hook` for panics

### Browser autoplay policy
`AudioContext` cannot be created/resumed until a user gesture (click/tap/keypress). The UX flow must be:
1. User loads ROM (via file picker, drag-drop, or "Try Demo")
2. Show a "Click to Play" overlay on the canvas
3. On click: create AudioContext, start AudioWorklet, begin emulation loop
4. Cannot auto-start audio on page load or ROM load alone

### `shrust` / `debug()` calls
The interactive debugger (`Emulator::debug()`) uses stdin via shrust. Must gate:
- The `extern crate shrust` and all `use shrust::*` behind `#[cfg(not(target_arch = "wasm32"))]`
- Every call to `self.debug()` in `mod.rs` — either gate the calls or make `debug()` a no-op on wasm
- The stepping/breakpoint logic can stay (harmless), but the REPL interaction cannot

### Focus and tab visibility
- `requestAnimationFrame` naturally stops firing when the tab is hidden — emulation auto-pauses
- Consider pausing audio (suspend AudioContext) when document loses focus to prevent buffer buildup
- Resume on focus/visibility change

---

## Library/Approach Decisions

### Rendering: Canvas 2D via `web-sys`

**Why not keep pixels/wgpu on wasm?** Pixels uses wgpu which pulls in a large WebGL2/WebGPU backend (~2-5MB wasm binary vs ~500KB). For blitting a 256x240 framebuffer, canvas 2D via `ImageData` is simpler, faster to build, and produces a tiny binary. nes-rust, rustynes, and rusticnes-wasm all use this approach.

**How it works:** Get a `CanvasRenderingContext2d`, construct an `ImageData` from the RGB framebuffer (converting to RGBA), call `putImageData()` each frame. Sub-1ms per frame at NES resolution.

### Audio: Web Audio API AudioWorklet via `web-sys`

**Why AudioWorklet?** ScriptProcessorNode is deprecated, runs on the main thread (jank risk), and has high latency. AudioWorklet runs on a dedicated audio thread with 128-sample processing blocks (~2.9ms at 44.1kHz).

**Implementation approach:**
1. A small JS AudioWorkletProcessor file (bundled by trunk as an asset)
2. Rust creates `AudioContext`, calls `audioWorklet.addModule()` to load the processor
3. Creates an `AudioWorkletNode` connected to destination
4. Samples flow from emulator → postMessage → worklet ring buffer
5. Phase 2 upgrade: SharedArrayBuffer ring buffer (lower latency, needs COOP/COEP headers)

**Fallback if SharedArrayBuffer unavailable:** Use `MessagePort.postMessage()` to send sample batches from main thread to worklet. Higher latency but works without special headers. This is the MVP approach.

### Build tool: `trunk`

Industry standard for Rust wasm apps. Handles: wasm-bindgen, wasm-opt, asset bundling, HTML templating, dev server with live reload. TetaNES uses trunk. Configured via `Trunk.toml`.

### Input: `web-sys` keyboard events

Listen for `keydown`/`keyup` on the document. Map `KeyboardEvent.code` to NES buttons. Store state in `Rc<RefCell<[bool; 8]>>` shared with the IOHandler.

### ROM loading: File input + drag-and-drop + fetch-on-demand demo

- `<input type="file" accept=".nes">` element for ROM selection
- Drag-and-drop onto the canvas
- "Try Demo" button fetches a free homebrew ROM from a URL (e.g., hosted alongside the app)
- Candidate demo ROMs: LJ65 (Tetris clone, public domain, NROM/mapper 0), Alter Ego, Blade Buster

---

## Reference: Existing Rust NES Emulators with Web Builds

| | TetaNES | nes-rust | Rustico |
|---|---|---|---|
| **Rendering** | wgpu (WebGL2) | Canvas 2D putImageData | Canvas 2D (in Web Worker) |
| **Audio** | cpal → AudioBufferSourceNode (~100-330ms latency) | ScriptProcessorNode (deprecated) | **AudioWorklet** (low latency) |
| **Main loop** | winit spawn_app (wraps rAF) | JS rAF calling step_frame() | Web Worker + rAF |
| **Build** | trunk | wasm-pack | wasm-bindgen + custom script |
| **Threading** | Single-threaded on web | Single-threaded | Off main thread |

Rustico's AudioWorklet approach is closest to what we want. Their pattern:
- `audio_processor.js` registers an `AudioWorkletProcessor` subclass
- Main thread sends samples via `port.postMessage()`
- Worklet accumulates in buffer, reads in `process()` callback
- `AudioContext` created with `{latencyHint: 'interactive', sampleRate: 44100}`

---

## MVP Scope (Phase 1)

**Goal:** Krankulator runs in Firefox/Chrome with video, audio, keyboard input, and ROM loading.

### Step 1: Make core compile on wasm32

- `rustup target add wasm32-unknown-unknown`
- Gate platform-specific deps in `Cargo.toml`:
  ```toml
  [target.'cfg(not(target_arch = "wasm32"))'.dependencies]
  rodio = "0.17"
  winit = "0.30"
  pixels = "0.17"
  clap = { version = "3.1.1", features = ["derive"] }
  shrust = "0.0.7"

  [target.'cfg(target_arch = "wasm32")'.dependencies]
  wasm-bindgen = "0.2"
  wasm-bindgen-futures = "0.4"
  web-sys = { version = "0.3", features = [...] }
  js-sys = "0.3"
  console_error_panic_hook = "0.1"
  getrandom = { version = "0.2", features = ["js"] }
  ```
- Gate `WinitPixelsIOHandler` and `AudioOutput` behind `#[cfg(not(target_arch = "wasm32"))]`
- `std::time::Instant` in `Emulator`: only used for stats/FPS display — on wasm, stub it or use `web_sys::Performance::now()` behind a thin wrapper
- `rand` 0.7.3: upgrade to `rand` 0.8+ (0.7's getrandom 0.1.x has incompatible wasm feature flags). Add `getrandom = { version = "0.2", features = ["js"] }` for wasm entropy. Check what rand is used for — if only initial RAM fill, could replace with js-sys `Math.random()`.
- `shrust` / `debug()`: gate `extern crate shrust`, all shrust imports, and all `self.debug()` call sites behind `#[cfg(not(target_arch = "wasm32"))]`. Make debugger a no-op on wasm.
- `println!`: add `console_error_panic_hook` for panics. For general logging, add a cfg-gated log macro or just accept silence on non-error paths.
- Verify: `cargo build --target wasm32-unknown-unknown --lib`

### Step 2: Frame-at-a-time execution

Add `Emulator::run_one_frame(&mut self) -> bool` — runs cycles until `ppu.frames` increments, then returns:
```rust
pub fn run_one_frame(&mut self) -> bool {
    let target_frame = self.ppu.frames + 1;
    while self.ppu.frames < target_frame {
        if self.cycle() == CycleState::Exiting {
            return false;
        }
    }
    true
}
```

### Step 3: Web audio backend (AudioWorklet)

Create `src/emu/audio/web.rs` + `web/audio_processor.js`:

**audio_processor.js** (loaded by trunk as asset):
```js
class NesAudioProcessor extends AudioWorkletProcessor {
    constructor() {
        super();
        this.buffer = new Float32Array(0);
        this.port.onmessage = (e) => {
            const old = this.buffer;
            this.buffer = new Float32Array(old.length + e.data.length);
            this.buffer.set(old);
            this.buffer.set(e.data, old.length);
        };
    }
    process(inputs, outputs) {
        const output = outputs[0][0];
        if (this.buffer.length >= output.length) {
            output.set(this.buffer.subarray(0, output.length));
            this.buffer = this.buffer.subarray(output.length);
        }
        return true;
    }
}
registerProcessor('nes-audio-processor', NesAudioProcessor);
```

**web.rs AudioBackend impl:**
- `push_samples()`: accumulate into internal `Vec<f32>` buffer (do NOT postMessage per call — called hundreds of times per frame with tiny batches)
- `flush()` (new method, called once after `run_one_frame()`): convert accumulated buffer to `Float32Array`, send via single `AudioWorkletNode.port.postMessage()`, clear buffer
- On init: `AudioContext` with `{sampleRate: 44100, latencyHint: 'interactive'}`, `audioWorklet.addModule('audio_processor.js')`, create `AudioWorkletNode`, connect to destination
- AudioContext creation deferred until user gesture (click/tap) — browser autoplay policy
- Audio drift compensation: worklet reports buffer fill level back via `port.postMessage()`. If buffer > 2 frames: skip one frame of emulation. If buffer < 0.5 frames: run extra frame. This keeps emulation synced to audio clock despite 60.0988 vs 60.000 Hz mismatch.

### Step 4: Web IO handler

Create `src/emu/io/web.rs`:

**Render** — `IOHandler::render()`:
- Get canvas `CanvasRenderingContext2d`
- Convert RGB `Buffer.data` (256*240*3 bytes) → RGBA (256*240*4 bytes, alpha=255)
- Wrap as `ImageData`, call `putImageData()`
- No frame pacing needed (rAF provides 60fps timing)

**Input** — `IOHandler::poll()`:
- On init, register `keydown`/`keyup` listeners on `document`
- Store pressed keys in `Rc<RefCell<HashSet<String>>>` (key codes)
- In `poll()`, read the set and map to controller state:
  - ArrowUp/Down/Left/Right → D-pad
  - KeyZ → A, KeyX → B, KeyC → Start, KeyV → Select

### Step 5: Web entry point & HTML shell

**`src/lib.rs`** (gated with `#[cfg(target_arch = "wasm32")]`):
- `#[wasm_bindgen(start)] pub fn main()` — sets panic hook (`console_error_panic_hook`), shows UI
- ROM loading: attach `change` listener to file input, read with `FileReader`, pass bytes to `loader::InesLoader`
- "Try Demo" button: `fetch()` a homebrew ROM from relative URL
- After ROM loaded: show "Click to Play" overlay on canvas
- On click: create AudioContext + AudioWorklet, create `Emulator::new_web(mapper)`, start rAF loop
- rAF loop:
  1. Check audio buffer fill (drift compensation)
  2. `run_one_frame()` (possibly skip or double based on audio fill)
  3. `audio.flush()` — send accumulated samples to worklet
  4. IOHandler renders frame to canvas
- Tab visibility: suspend AudioContext on `visibilitychange` hidden, resume on visible

**`index.html`**:
```html
<canvas id="nes-canvas" width="256" height="240"></canvas>
<input type="file" id="rom-input" accept=".nes">
<button id="demo-btn">Try Demo</button>
```
- CSS: scale canvas with `image-rendering: pixelated`, center, dark background
- Drag-and-drop listeners on canvas

### Step 6: Build & test locally

- `Trunk.toml`:
  ```toml
  [build]
  target = "index.html"
  
  [serve]
  headers = { "Cross-Origin-Opener-Policy" = "same-origin", "Cross-Origin-Embedder-Policy" = "require-corp" }
  ```
- `trunk serve --open` → opens in browser
- Test in Firefox and Chrome: video, audio, input, ROM loading
- Check DevTools console for errors

### Step 7: CI

- New GitHub Actions job:
  ```yaml
  wasm:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown
      - run: cargo build --target wasm32-unknown-unknown --lib
  ```
- Compile-only check — validates all cfg gates work and wasm compiles
- Keep existing native test suite unchanged

---

## Phase 2: Polish

- SharedArrayBuffer ring buffer for audio (replace postMessage, lower latency, needs COOP/COEP)
- Save states to `localStorage`
- Battery-backed RAM (SRAM) persistence to `localStorage`
- Responsive CSS layout (scale canvas to viewport, max 4x)
- GitHub Pages deployment via CI (`trunk build --release` → deploy to gh-pages branch)
- Gamepad API support (`gamepads` crate or raw web-sys `navigator.getGamepads()`)
- On-screen touch controls (D-pad + A/B/Start/Select overlays for mobile)
- "Click to start audio" overlay (browser autoplay policy)

## Phase 3: Nice-to-have

- Web Worker for emulation (off main thread, like Rustico — prevents dropped frames)
- wgpu rendering (enables CRT shaders, NTSC filter)
- PWA manifest + service worker (offline play, also solves COOP/COEP for GitHub Pages)
- Mobile-optimized layout with proper touch UX
- Netplay via WebRTC
- Performance profiling (measure frame time budget)

---

## Key Files to Create/Modify

| File | Action | Purpose |
|------|--------|---------|
| `Cargo.toml` | Modify | Platform-conditional deps, add wasm deps |
| `src/lib.rs` | Create | Wasm entry point, rAF loop, ROM loading |
| `src/emu/io/web.rs` | Create | Canvas 2D rendering, keyboard input |
| `src/emu/io/mod.rs` | Modify | Gate winit behind cfg, add web module |
| `src/emu/audio/web.rs` | Create | AudioWorklet backend |
| `src/emu/audio/mod.rs` | Modify | Gate rodio behind cfg, add web module |
| `src/emu/mod.rs` | Modify | Add `run_one_frame()`, gate `Instant` usage |
| `src/main.rs` | Modify | (no changes needed if lib.rs is the wasm entry) |
| `index.html` | Create | HTML shell with canvas + file input |
| `web/audio_processor.js` | Create | AudioWorklet processor script |
| `Trunk.toml` | Create | Build config + COOP/COEP headers |
| `.github/workflows/*.yml` | Modify | Add wasm compile check job |

---

## Verification Plan

1. `cargo build` — native still compiles and works
2. `cargo test` — all existing tests pass (nothing regressed)
3. `cargo build --target wasm32-unknown-unknown --lib` — wasm compiles clean
4. `trunk serve --open` — dev server starts, page loads
5. Open in Firefox:
   - Click "Try Demo" or load ROM via file picker → game renders on canvas
   - Audio plays after user interaction (click/key)
   - Arrow keys + Z/X/C/V control the game
   - No console errors, no wasm panics
6. Same in Chrome
7. Drag-and-drop a .nes file onto canvas → loads and runs
8. CI: wasm build job passes on push
