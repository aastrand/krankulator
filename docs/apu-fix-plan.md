# APU Bug Fix & Improvement Plan

## Context

Pulse sounds wrong, noise channel is too loud and stays on too long. Cross-referenced our
implementation against Mesen2 (C++, gold standard), TetaNES (Rust), and NESdev wiki docs.
Found 9 confirmed bugs — several directly explain the reported symptoms.

## Bug Summary

### Critical — directly cause the reported symptoms

| # | Bug | File:Line | Impact |
|---|-----|-----------|--------|
| 1 | **Noise LFSR output bit polarity inverted** | `noise.rs:147` | Noise outputs volume when bit 0 is SET (should output 0). Starts loud instead of muted. **Primary cause of "noise too loud".** |
| 2 | **Noise LFSR mode 1 feedback bits wrong** | `noise.rs:122-128` | Mode 1 XORs bit 6 with bit 5; should XOR bit 0 with bit 6. Wrong pseudo-random sequence for metallic noise. |
| 3 | **DMC reads bits MSB-first instead of LSB-first** | `dmc.rs:147-148` | Bits within each sample byte are reversed. All DMC audio plays wrong. |
| 4 | **Noise envelope divider reload value wrong** | `noise.rs:164` | Uses `volume + 1` (period V+2); should use `volume` (period V+1, matching pulse and Mesen2/TetaNES). Noise envelope decays slower than it should. |
| 5 | **Linear mixing instead of nonlinear NES DAC formula** | `mod.rs:431-439` | Uses linear coefficients with pre-normalized floats. Should use NESdev nonlinear lookup tables with raw DAC integer outputs. Wrong volume balance between all channels. |

### High — affects correctness

| # | Bug | File:Line | Impact |
|---|-----|-----------|--------|
| 6 | **Envelope start flag set on wrong registers** | `pulse.rs:99`, `noise.rs:76` | Both pulse and noise set `envelope_start` on control register writes ($4000/$4004/$400C). Should only set it on 4th register writes ($4003/$4007/$400F). Additionally, noise `set_length_counter()` is *missing* the start flag set. |
| 7 | **Frame counter mode 1 clocks length/sweep too often** | `mod.rs:348` | Clocks on steps 1,2,3 (`step <= 3`); should clock only on steps 1 and 3. Length counters decrement 3x/frame instead of 2x in 5-step mode. |

### Medium — sound quality

| # | Bug | File:Line | Impact |
|---|-----|-----------|--------|
| 8 | **No audio filtering** | `mod.rs` | NES hardware has HP@37Hz, HP@440Hz, LP@14kHz. Missing filters cause DC offset and harsh aliasing. |
| 9 | **Triangle timer_low reload bug** | `triangle.rs:70` | `set_timer_low()` reloads `timer_value` from `timer`. On real hardware, writing timer low only updates the reload value, doesn't restart the timer. |

---

## Implementation Plan

### Phase 1: Fix channel output bugs (noise, DMC)

**File: `src/emu/apu/channels/noise.rs`**

1. **Fix LFSR output polarity** (line 147): Invert the condition.
   - Before: `if (self.shift_register & 1) == 0 { output = 0 } else { output = vol }`
   - After: `if (self.shift_register & 1) == 1 { output = 0 } else { output = vol }`
   - Confirmed against Mesen2 `NoiseChannel.h:26-29` and TetaNES `noise.rs:67-68`

2. **Fix LFSR mode 1 feedback** (line 122-128): Change mode 1 to XOR bit 0 with bit 6.
   - Before: `((self.shift_register >> 6) & 1) ^ ((self.shift_register >> 5) & 1)`
   - After: `(self.shift_register & 1) ^ ((self.shift_register >> 6) & 1)`
   - Mode 0 is already correct (bit 0 XOR bit 1)
   - Confirmed against Mesen2 `NoiseChannel.h:44` and TetaNES `noise.rs:162`

3. **Fix envelope divider reload** (lines 164, 166): Change `self.volume + 1` to `self.volume`.
   - Matches pulse channel, Mesen2 `ApuEnvelope.h:67-82`, TetaNES `envelope.rs:63`

4. **Fix envelope start flag** (line 76): Remove `self.envelope_start = true` from `set_control()`.
   Add `self.envelope_start = true` to `set_length_counter()`.

5. **Change output to raw DAC value** (line 156): Return `vol as f32` instead of `vol as f32 / 15.0`.

**File: `src/emu/apu/channels/dmc.rs`**

6. **Fix bit order** (lines 147-148): Read LSB-first.
   - Before: `let bit = (self.sample_buffer >> 7) & 1; self.sample_buffer <<= 1;`
   - After: `let bit = self.sample_buffer & 1; self.sample_buffer >>= 1;`
   - Standard NESdev behavior, confirmed by both references

7. **Change output to raw DAC value** (line 210): Return `self.output_level as f32` instead of centered float.

### Phase 2: Fix pulse channel bugs

**File: `src/emu/apu/channels/pulse.rs`**

8. **Fix envelope start flag** (line 99): Remove `self.envelope_start = true` from `set_control()`.
   Keep the existing `self.envelope_start = true` in `set_timer_high()` (line 128) — that one is correct.

9. **Change output to raw DAC value** (line 213): Return `vol as f32` instead of `vol as f32 / 15.0`.

### Phase 3: Fix triangle channel

**File: `src/emu/apu/channels/triangle.rs`**

10. **Fix timer_low reload** (line 70): Remove `self.timer_value = self.timer;` from `set_timer_low()`.

11. **Change output to raw DAC value** (line 137): Return `triangle_value as f32` instead of centered `(triangle_value as f32 - 7.5) / 7.5`.

### Phase 4: Fix mixing and frame counter

**File: `src/emu/apu/mod.rs`**

12. **Replace linear mixing with nonlinear NESdev formula** (lines 431-439):
    ```rust
    fn mix_channels(&self, pulse1: f32, pulse2: f32, triangle: f32, noise: f32, dmc: f32) -> f32 {
        let pulse_sum = pulse1 + pulse2;
        let pulse_out = if pulse_sum > 0.0 {
            95.88 / (8128.0 / pulse_sum + 100.0)
        } else {
            0.0
        };
        let tnd_sum = triangle / 8227.0 + noise / 12241.0 + dmc / 22638.0;
        let tnd_out = if tnd_sum > 0.0 {
            159.79 / (1.0 / tnd_sum + 100.0)
        } else {
            0.0
        };
        // Output range ~[0.0, 1.0], center for audio
        (pulse_out + tnd_out) * 2.0 - 1.0
    }
    ```
    Channel inputs are now raw DAC values (pulse/noise: 0-15, triangle: 0-15, dmc: 0-127).

13. **Fix frame counter mode 1 length/sweep clocking** (line 348):
    - Before: `(mode == 1 && step <= 3)`
    - After: `(mode == 1 && (step == 1 || step == 3))`

14. **Remove redundant triangle gate** (line 378-379): Always call `self.triangle.cycle()`.
    The triangle's `cycle()` method already gates on enable/length/linear internally.

### Phase 5: Add audio filtering

**File: `src/emu/apu/mod.rs`**

15. Add a simple `AudioFilter` struct with 3 first-order IIR filters:
    - High-pass at 37 Hz (DC removal)
    - High-pass at 440 Hz (bass shaping)
    - Low-pass at 14 kHz (anti-aliasing)
    - Coefficients precomputed for 44100 Hz sample rate
    - Apply in `generate_sample()` after mixing

### Phase 6: Update tests

- Update unit tests in each channel file to match new raw DAC output ranges
- Update `test_apu_mix_channels` for the nonlinear formula
- Add tests for corrected noise LFSR polarity and mode 1 feedback
- Add tests for corrected DMC bit order
- Run existing blargg APU test ROMs (`cargo test`)

### Phase 7: Additional test ROMs

We have blargg's `apu_test` suite (len_ctr, len_table, irq_flag, jitter, len_timing,
irq_flag_timing, dmc_basics, dmc_rates). We're missing:

- **blargg's `apu_mixer`** tests (square.nes, tnd.nes, dmc.nes) — would verify our new
  nonlinear mixing formula
- **blargg's `apu_reset`** tests — verify APU state after reset

These can be downloaded from the nes-test-roms collections on GitHub and added to
`input/nes/apu/`.

---

## Verification

1. `cargo test` — all existing tests pass (after updating expected values)
2. Run blargg APU test ROMs in headless mode — verify pass status
3. Manual listening test with games that exercise noise (e.g. Mario, Zelda, Mega Man)
   and DMC (e.g. games with sampled drums/speech)
4. Compare pulse/noise balance against Mesen2 output on the same ROM

---

## Current branch status (WIP)

Implemented in tree: channel fixes above (noise LFSR, DMC LSB-first, envelopes, raw DAC into
nonlinear mixer), first-order IIR chain after mix, triangle timer low write behavior, frame
counter NTSC step intervals, delayed `$4017` with `Deferred4017Apply`, and `$4017` odd/even delay
via `cpu.cycle + cpu_bus_cycle_offset`. Blargg ROMs **1–3** (`len_ctr`, `len_table`, `irq_flag`)
pass under `cargo test`. ROMs **4–8** are **`#[ignore]`** in `src/main.rs` until frame IRQ edge
timing, length half-frame alignment, and DMC period/DMA match hardware; run them with
`cargo test test_nes_apu_ -- --ignored`. An experimental `+1` on the `$4017` cycle tag was tried
and reverted (made timing worse).

---

## References

- **Mesen2** (`/tmp/mesen2/Core/NES/APU/`): ApuEnvelope.h, NoiseChannel.h, NesSoundMixer.cpp
- **TetaNES** (`/tmp/tetanes/tetanes-core/src/apu/`): envelope.rs, noise.rs, filter.rs, apu.rs
- **NESdev wiki**: APU Mixer, APU Envelope, APU Noise, APU Frame Counter
