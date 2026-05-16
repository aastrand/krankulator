use crate::emu::savestate::{SavestateReader, SavestateWriter};

#[derive(Debug, PartialEq)]
pub enum FrameStep {
    None,
    QuarterFrame,
    HalfFrame,
    Irq,
    IrqHalfFrame,
    Deferred4017Apply { immediate_clock: bool },
}

pub struct FrameCounter {
    mode: u8,
    step: u8,
    cycles: u32,
    irq_inhibit: bool,
    reset_delay: u8,
    pending_write: u8,
    block_tick: u8,
}

const MODE0_CYCLES: [u32; 6] = [7457, 14913, 22371, 29828, 29829, 29830];
const MODE1_CYCLES: [u32; 6] = [7457, 14913, 22371, 29829, 37281, 37282];

impl FrameCounter {
    pub fn new() -> Self {
        Self {
            mode: 0,
            step: 0,
            cycles: 10, // hardware powers on as if $4017=$00 written ~10 clocks before first instruction
            irq_inhibit: false,
            reset_delay: 0,
            pending_write: 0,
            block_tick: 0,
        }
    }

    pub fn reset_with_value(&mut self, value: u8) {
        self.mode = (value >> 7) & 1;
        self.irq_inhibit = (value >> 6) & 1 != 0;
        self.step = 0;
        self.cycles = 10;
        self.reset_delay = 0;
        self.pending_write = 0;
        self.block_tick = 0;
    }

    pub fn write(&mut self, value: u8, emu_cycle: u64) {
        self.irq_inhibit = (value >> 6) & 1 != 0;
        self.pending_write = value;
        self.reset_delay = if (emu_cycle & 1) == 0 { 3 } else { 4 };
    }

    pub fn cycle(&mut self) -> FrameStep {
        if self.reset_delay > 0 {
            self.reset_delay -= 1;
            if self.reset_delay == 0 {
                self.mode = (self.pending_write >> 7) & 1;
                self.step = 0;
                self.cycles = 0;
                if self.mode == 1 {
                    self.block_tick = 2;
                    return FrameStep::Deferred4017Apply {
                        immediate_clock: true,
                    };
                }
                return FrameStep::None;
            }
            return FrameStep::None;
        }

        if self.block_tick > 0 {
            self.block_tick -= 1;
        }

        self.cycles += 1;

        match self.mode {
            0 => self.cycle_mode_0(),
            1 => self.cycle_mode_1(),
            _ => FrameStep::None,
        }
    }

    fn cycle_mode_0(&mut self) -> FrameStep {
        if self.step >= 6 {
            return FrameStep::None;
        }
        let target = MODE0_CYCLES[self.step as usize];
        if self.cycles < target {
            return FrameStep::None;
        }
        let current_step = self.step;
        self.step += 1;
        if self.step >= 6 {
            self.step = 0;
            self.cycles = 0;
        }
        match current_step {
            0 | 2 => FrameStep::QuarterFrame,
            1 => FrameStep::HalfFrame,
            3 => FrameStep::Irq,
            4 => FrameStep::IrqHalfFrame,
            5 => FrameStep::Irq,
            _ => FrameStep::None,
        }
    }

    fn cycle_mode_1(&mut self) -> FrameStep {
        if self.step >= 6 {
            return FrameStep::None;
        }
        let target = MODE1_CYCLES[self.step as usize];
        if self.cycles < target {
            return FrameStep::None;
        }
        let current_step = self.step;
        self.step += 1;
        if self.step >= 6 {
            self.step = 0;
            self.cycles = 0;
        }

        if self.block_tick > 0 {
            return FrameStep::None;
        }

        match current_step {
            0 | 2 => FrameStep::QuarterFrame,
            1 => FrameStep::HalfFrame,
            4 => FrameStep::HalfFrame,
            _ => FrameStep::None,
        }
    }

    #[cfg(test)]
    pub fn get_step(&self) -> u8 {
        self.step
    }

    #[cfg(test)]
    pub fn get_mode(&self) -> u8 {
        self.mode
    }

    pub fn irq_inhibit(&self) -> bool {
        self.irq_inhibit
    }

    pub fn save_state(&self, w: &mut SavestateWriter) {
        w.write_u8(self.mode);
        w.write_u8(self.step);
        w.write_u32(self.cycles);
        w.write_bool(self.irq_inhibit);
        w.write_u8(self.reset_delay);
        w.write_u8(self.pending_write);
        w.write_u8(self.block_tick);
    }

    pub fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        self.mode = r.read_u8()?;
        self.step = r.read_u8()?;
        self.cycles = r.read_u32()?;
        self.irq_inhibit = r.read_bool()?;
        self.reset_delay = r.read_u8()?;
        self.pending_write = r.read_u8()?;
        self.block_tick = r.read_u8()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_counter_new() {
        let fc = FrameCounter::new();
        assert_eq!(fc.mode, 0);
        assert_eq!(fc.step, 0);
        assert_eq!(fc.cycles, 10);
        assert!(!fc.irq_inhibit);
    }

    #[test]
    fn test_frame_counter_write() {
        let mut fc = FrameCounter::new();

        fc.write(0x00, 0);
        assert_eq!(fc.mode, 0);
        assert!(!fc.irq_inhibit);
        assert_eq!(fc.reset_delay, 3);

        fc.write(0x40, 0);
        assert_eq!(fc.mode, 0);
        assert!(fc.irq_inhibit);

        fc.write(0xC0, 1);
        assert_eq!(fc.reset_delay, 4);
        assert!(fc.irq_inhibit);
    }

    #[test]
    fn test_mode_0_quarter_frame_at_7457() {
        let mut fc = FrameCounter::new();
        fc.write(0x00, 0);
        // Consume the 3-cycle reset delay
        for _ in 0..3 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        // 7456 cycles of nothing
        for _ in 0..7456 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        // Cycle 7457: quarter frame
        assert_eq!(fc.cycle(), FrameStep::QuarterFrame);
    }

    #[test]
    fn test_mode_0_half_frame_at_14913() {
        let mut fc = FrameCounter::new();
        fc.write(0x00, 0);
        for _ in 0..3 {
            fc.cycle();
        }
        for _ in 0..14912 {
            fc.cycle();
        }
        assert_eq!(fc.cycle(), FrameStep::HalfFrame);
    }

    #[test]
    fn test_mode_0_irq_window() {
        let mut fc = FrameCounter::new();
        fc.write(0x00, 0);
        for _ in 0..3 {
            fc.cycle();
        }
        // Advance to just before the IRQ window
        for _ in 0..29827 {
            fc.cycle();
        }
        // Cycle 29828: first IRQ
        assert_eq!(fc.cycle(), FrameStep::Irq);
        // Cycle 29829: IRQ + half frame
        assert_eq!(fc.cycle(), FrameStep::IrqHalfFrame);
        // Cycle 29830: final IRQ, then reset
        assert_eq!(fc.cycle(), FrameStep::Irq);
        // Should now be back at start
        assert_eq!(fc.step, 0);
        assert_eq!(fc.cycles, 0);
    }

    #[test]
    fn test_mode_0_irq_inhibited() {
        let mut fc = FrameCounter::new();
        fc.write(0x40, 0); // IRQ inhibit set
        for _ in 0..3 {
            fc.cycle();
        }
        for _ in 0..29827 {
            fc.cycle();
        }
        // IRQ events still fire (APU decides whether to act on them)
        assert_eq!(fc.cycle(), FrameStep::Irq);
        assert_eq!(fc.cycle(), FrameStep::IrqHalfFrame);
        assert_eq!(fc.cycle(), FrameStep::Irq);
    }

    #[test]
    fn test_mode_0_full_cycle() {
        let mut fc = FrameCounter::new();
        fc.write(0x00, 0);
        for _ in 0..3 {
            fc.cycle();
        }
        // Full sequence: 29830 cycles
        let mut events = Vec::new();
        for _ in 0..29830 {
            let step = fc.cycle();
            if step != FrameStep::None {
                events.push(step);
            }
        }
        assert_eq!(events.len(), 6);
        assert_eq!(events[0], FrameStep::QuarterFrame); // 7457
        assert_eq!(events[1], FrameStep::HalfFrame); // 14913
        assert_eq!(events[2], FrameStep::QuarterFrame); // 22371
        assert_eq!(events[3], FrameStep::Irq); // 29828
        assert_eq!(events[4], FrameStep::IrqHalfFrame); // 29829
        assert_eq!(events[5], FrameStep::Irq); // 29830
    }

    #[test]
    fn test_mode_1_immediate_clock() {
        let mut fc = FrameCounter::new();
        fc.write(0x80, 0);
        for _ in 0..2 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(
            fc.cycle(),
            FrameStep::Deferred4017Apply {
                immediate_clock: true
            }
        );
    }

    #[test]
    fn test_mode_1_full_cycle() {
        let mut fc = FrameCounter::new();
        fc.write(0x80, 0);
        for _ in 0..3 {
            fc.cycle();
        }
        let mut events = Vec::new();
        for _ in 0..37282 {
            let step = fc.cycle();
            if step != FrameStep::None {
                events.push(step);
            }
        }
        assert_eq!(events.len(), 4);
        assert_eq!(events[0], FrameStep::QuarterFrame); // 7457
        assert_eq!(events[1], FrameStep::HalfFrame); // 14913
        assert_eq!(events[2], FrameStep::QuarterFrame); // 22371
        assert_eq!(events[3], FrameStep::HalfFrame); // 37281
        assert_eq!(fc.step, 0);
    }

    #[test]
    fn test_mode_0_wraps_correctly() {
        let mut fc = FrameCounter::new();
        fc.write(0x00, 0);
        for _ in 0..3 {
            fc.cycle();
        }
        // Run through 2 full sequences
        for _ in 0..29830 * 2 {
            fc.cycle();
        }
        assert_eq!(fc.step, 0);
        assert_eq!(fc.cycles, 0);
    }

    #[test]
    fn test_odd_cycle_write_delay() {
        let mut fc = FrameCounter::new();
        fc.write(0x00, 1); // Odd cycle → 4-cycle delay
        for _ in 0..3 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        // 4th cycle: delay expires
        assert_eq!(fc.cycle(), FrameStep::None);
        // Now counting from 0
        assert_eq!(fc.mode, 0);
        assert_eq!(fc.step, 0);
    }

    #[test]
    fn test_get_mode() {
        let mut fc = FrameCounter::new();
        assert_eq!(fc.get_mode(), 0);

        fc.write(0x80, 0);
        assert_eq!(fc.get_mode(), 0);
        for _ in 0..3 {
            fc.cycle();
        }
        assert_eq!(fc.get_mode(), 1);

        fc.write(0x00, 0);
        for _ in 0..3 {
            fc.cycle();
        }
        assert_eq!(fc.get_mode(), 0);
    }
}
