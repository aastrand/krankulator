# Per-Cycle CPU State Machine

## Goal

Replace the atomic instruction execution model with a per-cycle state machine. This fixes CPU interrupt timing tests (nmi_and_brk, nmi_and_irq, irq_and_dma, branch_delays_irq) by construction.

## Current State (WIP on this branch)

The state machine infrastructure is in place and compiles:
- `CpuPhase` enum: Fetch, Execute, Interrupt, Dma
- `InterruptKind` enum: Nmi, Irq, Brk
- `InstrType` classification for all 256 opcodes
- Pipeline state fields on Emulator (cpu_step, cpu_opcode, cpu_addr, cpu_data, etc.)
- All addressing mode step functions implemented (read, write, RMW patterns)
- All operation dispatch functions (apply_read_op, apply_rmw_op, apply_implied_op)
- Interrupt entry as 7-cycle state machine with NMI hijacking at cycle 5
- DMA as per-cycle state machine (alignment + 256 read/write pairs)
- Savestate v7 with backwards-compatible v6 loading
- Old execute_instruction() and helpers removed

87/87 unit tests pass. Integration tests have two known issues to fix:

### Issue 1: PC advancement timing

`step_fetch()` advances PC past the opcode byte (real 6502 behavior), but integration tests (nestest log comparison) capture `emu.cpu.pc` and expect it to point AT the opcode, not past it.

**Fix options:**
- A) Don't advance PC in step_fetch, advance it in each addressing mode's first step instead. All step functions that read operands need to read from `cpu.pc + 1` and advance PC explicitly. This is the cleanest approach but requires updating ~20 step functions.
- B) Store instruction-start PC in a public field and have the nestest test use that instead of `cpu.pc`. Minimal code change but leaky abstraction.

Option A is recommended. The pattern for each addressing mode:
```rust
fn step_read_imm(&mut self, step: u8) {
    // step 0: read operand at PC+1, then advance PC past both opcode and operand
    self.cpu.pc = self.cpu.pc.wrapping_add(1); // past opcode
    let value = self.cpu_read_cycle(self.cpu.pc);
    self.cpu.pc = self.cpu.pc.wrapping_add(1); // past operand
    ...
}
```

### Issue 2: CycleState::CpuExecuted semantics

The nestest integration test uses `CpuExecuted` vs `CpuAhead` to detect instruction boundaries. It captures `cpu.cycle` on CpuAhead returns and expects the cycle count at the START of the instruction.

In the old model, CpuExecuted fired at the boundary, the instruction executed atomically, then CpuAhead returns happened during catch-up. `cpu.cycle` was already advanced to instruction_end.

In the new model, CpuExecuted fires at the Fetch phase. The test loop's first iteration immediately gets CpuExecuted and breaks, capturing the cycle count correctly. But on subsequent iterations, CpuAhead returns during Execute phases update the cycle count to mid-instruction values.

**Fix:** The current approach (CpuExecuted from Fetch) is actually correct if PC isn't advanced during fetch (Issue 1 fix). The test flow becomes:
1. `pc = cpu.pc` (at opcode) ✓
2. `cycles = cpu.cycle` (at instruction start) ✓
3. Loop: cycle() → CpuExecuted (Fetch) → break immediately
4. Assertions pass

### Issue 3: APU IRQ timing tests

4 APU tests fail (irq_timing, reset_timing, 4017_timing, 4017_written). These likely need the APU frame IRQ polling to use the new `frame_irq_pending()` method correctly, or need timing adjustments for the per-cycle model.

### Issue 4: MMC3 scanline timing

2 MMC3 tests fail. Likely a PPU sync timing difference — the old model used `cpu_bus_cycle_offset` for mid-instruction PPU sync, the new model syncs PPU to `master_clock` on each bus access. The sync point may be off by 1-2 dots.

## Architecture

### State Machine

Each `cycle()` call advances the CPU by one cycle:
- **Fetch**: Read opcode, classify instruction, enter Execute/Interrupt
- **Execute**: Step through addressing mode + operation (1-6 cycles after fetch)
- **Interrupt**: 7-cycle BRK/IRQ/NMI entry with NMI hijacking at vector fetch
- **Dma**: 513-514 cycle OAM transfer with per-cycle PPU/APU stepping

### Interrupt Polling

- NMI: edge-detected from PPU, sets `pending_nmi`, fires at next Fetch
- IRQ: polled at `finish_instruction()` using I flag saved at instruction start (`irq_i_flag_sampled`). RTI overrides the saved flag after restoring P register.
- Branch IRQ suppression: taken non-page-crossing branches call `finish_instruction_no_irq_poll()`

### Bus Access

`cpu_read_cycle()` and `cpu_write_cycle()` replace the old `cpu_read()`/`cpu_write()`. They sync PPU to `master_clock` before PPU register accesses (instead of the old `instruction_start_dot + cpu_bus_cycle_offset * 3` calculation). This is correct because each cycle does exactly one bus access.

### Files Changed

- `core/src/emu/mod.rs` — state machine, new cycle(), removed execute_instruction
- `core/src/emu/apu/mod.rs` — added `frame_irq_pending()` method
- `core/src/emu/savestate.rs` — version bump to 7

## Remaining Work

1. Fix PC advancement (Issue 1) — update all step functions
2. Verify nestest passes (8991 instructions)
3. Fix APU timing tests
4. Fix MMC3 scanline timing
5. Run full test suite, fix any remaining failures
6. Test with real games (Battletoads, Mega Man 2, etc.)
7. Un-ignore the 4 cpu_interrupts_v2 tests and verify they pass
8. Clean up dead code, run cargo fmt
