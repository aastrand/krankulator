#[derive(Debug, PartialEq)]
pub enum FrameStep {
    None,
    Step(u8),
}

pub struct FrameCounter {
    mode: u8,
    step: u8,
    cycles: u32,
    irq_inhibit: bool,
}

impl FrameCounter {
    pub fn new() -> Self {
        Self {
            mode: 0,
            step: 0,
            cycles: 0,
            irq_inhibit: false,
        }
    }

    pub fn write(&mut self, value: u8) -> bool {
        self.mode = (value >> 7) & 1;
        self.irq_inhibit = (value >> 6) & 1 != 0;

        if self.irq_inhibit {
            // Clear IRQ immediately
            // This would be handled by the main APU
        }

        // Reset step counter
        self.step = 0;
        self.cycles = 0;

        // Return true if immediate clocking should occur
        // This happens when switching to mode 1 (bit 7 set)
        self.mode == 1
    }

    pub fn cycle(&mut self) -> FrameStep {
        match self.mode {
            0 => self.cycle_mode_0(),
            1 => self.cycle_mode_1(),
            _ => FrameStep::None,
        }
    }

    fn cycle_mode_0(&mut self) -> FrameStep {
        let frame_length = 7457;
        self.cycles += 1;

        if self.cycles == frame_length {
            self.cycles = 0;
            self.step = (self.step + 1) % 4;
            if self.step == 0 {
                if !self.irq_inhibit {
                    FrameStep::Step(0)
                } else {
                    FrameStep::None
                }
            } else {
                FrameStep::Step(self.step)
            }
        } else {
            FrameStep::None
        }
    }

    fn cycle_mode_1(&mut self) -> FrameStep {
        let frame_length = if self.step == 3 { 7456 } else { 7457 };
        self.cycles += 1;

        if self.cycles == frame_length {
            self.cycles = 0;
            self.step = (self.step + 1) % 5;
            if self.step == 0 {
                FrameStep::None
            } else {
                FrameStep::Step(self.step)
            }
        } else {
            FrameStep::None
        }
    }

    // only used for testing
    #[cfg(test)]
    pub fn get_step(&self) -> u8 {
        self.step
    }

    pub fn get_mode(&self) -> u8 {
        self.mode
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
        assert_eq!(fc.cycles, 0);
        assert!(!fc.irq_inhibit);
    }

    #[test]
    fn test_frame_counter_write() {
        let mut fc = FrameCounter::new();

        // Test mode 0
        assert!(!fc.write(0x00));
        assert_eq!(fc.mode, 0);
        assert!(!fc.irq_inhibit);
        assert_eq!(fc.step, 0);
        assert_eq!(fc.cycles, 0);

        // Test mode 1
        assert!(fc.write(0x80));
        assert_eq!(fc.mode, 1);
        assert!(!fc.irq_inhibit);

        // Test IRQ inhibit
        assert!(!fc.write(0x40));
        assert_eq!(fc.mode, 0);
        assert!(fc.irq_inhibit);

        // Test both mode 1 and IRQ inhibit
        assert!(fc.write(0xC0));
        assert_eq!(fc.mode, 1);
        assert!(fc.irq_inhibit);
    }

    #[test]
    fn test_frame_counter_mode_0_cycle() {
        let mut fc = FrameCounter::new();
        fc.write(0x00); // Mode 0

        // Test first few cycles
        for _ in 0..7456 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }

        // 7457th cycle should advance step
        assert_eq!(fc.cycle(), FrameStep::Step(1));
        assert_eq!(fc.step, 1);
        assert_eq!(fc.cycles, 0);

        // Test step progression
        for _ in 0..7456 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(2));
        assert_eq!(fc.step, 2);

        for _ in 0..7456 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(3));
        assert_eq!(fc.step, 3);

        // Step 3 should generate IRQ (if not inhibited)
        for _ in 0..7456 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(0)); // Should return true for IRQ
        assert_eq!(fc.step, 0); // Should wrap around
    }

    #[test]
    fn test_frame_counter_mode_0_irq() {
        let mut fc = FrameCounter::new();
        fc.write(0x00); // Mode 0, no IRQ inhibit

        // Advance to step 3
        for _ in 0..7457 * 3 {
            fc.cycle();
        }
        assert_eq!(fc.step, 3);

        // Step 3 should generate IRQ
        for _ in 0..7456 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(0)); // Should return true for IRQ
        assert_eq!(fc.step, 0);
    }

    #[test]
    fn test_frame_counter_mode_0_irq_inhibit() {
        let mut fc = FrameCounter::new();
        fc.write(0x40); // Mode 0, IRQ inhibit

        // Advance to step 3
        for _ in 0..7457 * 3 {
            fc.cycle();
        }
        assert_eq!(fc.step, 3);

        // Step 3 should not generate IRQ when inhibited
        for _ in 0..7456 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::None); // Should return false for IRQ
        assert_eq!(fc.step, 0);
    }

    #[test]
    fn test_frame_counter_mode_1_cycle() {
        let mut fc = FrameCounter::new();
        fc.write(0x80); // Mode 1

        // Test first step (7457 cycles)
        for _ in 0..7456 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(1));
        assert_eq!(fc.step, 1);
        assert_eq!(fc.cycles, 0);

        // Test second step (7457 cycles)
        for _ in 0..7456 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(2));
        assert_eq!(fc.step, 2);

        // Test third step (7457 cycles)
        for _ in 0..7456 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(3));
        assert_eq!(fc.step, 3);

        // Test fourth step (7456 cycles - shorter)
        for _ in 0..7455 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(4));
        assert_eq!(fc.step, 4);

        // Test fifth step (7457 cycles)
        for _ in 0..7456 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::None); // Step 0 returns None
        assert_eq!(fc.step, 0); // Should wrap around
    }

    #[test]
    fn test_frame_counter_mode_1_no_irq() {
        let mut fc = FrameCounter::new();
        fc.write(0x80); // Mode 1

        // Cycle through all steps - should never generate IRQ
        let mut step_count = 0;
        for _ in 0..7457 * 4 + 7456 {
            let result = fc.cycle();
            if result != FrameStep::None {
                step_count += 1;
            }
        }
        assert_eq!(fc.step, 0);
        assert_eq!(step_count, 4); // Should have seen 4 steps (1,2,3,4) but no step 0
    }

    #[test]
    fn test_frame_counter_get_step() {
        let mut fc = FrameCounter::new();
        assert_eq!(fc.get_step(), 0);

        fc.write(0x00); // Mode 0
        for _ in 0..7457 {
            fc.cycle();
        }
        assert_eq!(fc.get_step(), 1);
    }

    #[test]
    fn test_frame_counter_get_mode() {
        let mut fc = FrameCounter::new();
        assert_eq!(fc.get_mode(), 0);

        fc.write(0x80); // Mode 1
        assert_eq!(fc.get_mode(), 1);

        fc.write(0x00); // Mode 0
        assert_eq!(fc.get_mode(), 0);
    }

    #[test]
    fn test_frame_counter_timing_accuracy() {
        let mut fc = FrameCounter::new();
        fc.write(0x00); // Mode 0

        // Test that exactly 7457 cycles advance one step
        for _ in 0..7457 {
            fc.cycle();
        }
        assert_eq!(fc.cycles, 0); // Should reset to 0
        assert_eq!(fc.step, 1);

        // Test that 7456 cycles don't advance
        fc.write(0x00); // Reset
        for _ in 0..7456 {
            fc.cycle();
        }
        assert_eq!(fc.step, 0); // Should still be 0
    }

    #[test]
    fn test_frame_counter_mode_1_timing_accuracy() {
        let mut fc = FrameCounter::new();
        fc.write(0x80); // Mode 1

        // Advance to step 3
        for _ in 0..7457 * 3 {
            fc.cycle();
        }
        assert_eq!(fc.step, 3);

        // Step 3 should take 7456 cycles
        for _ in 0..7455 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(4));
        assert_eq!(fc.cycles, 0);
    }
}
