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

    pub fn write(&mut self, value: u8) {
        self.mode = (value >> 7) & 1;
        self.irq_inhibit = (value >> 6) & 1 != 0;

        if self.irq_inhibit {
            // Clear IRQ immediately
            // This would be handled by the main APU
        }

        // Reset step counter
        self.step = 0;
        self.cycles = 0;
    }

    pub fn cycle(&mut self) -> bool {
        self.cycles += 1;

        let irq = match self.mode {
            0 => self.cycle_mode_0(),
            1 => self.cycle_mode_1(),
            _ => false,
        };

        irq
    }

    fn cycle_mode_0(&mut self) -> bool {
        // Mode 0: 4-step sequence
        let frame_length = 7456; // CPU cycles per frame step (14915 / 2)

        if self.cycles >= frame_length {
            self.cycles = 0;
            let old_step = self.step;
            self.step = (self.step + 1) % 4;

            // Generate IRQ when advancing from step 3 to step 0 (if not inhibited)
            if old_step == 3 && !self.irq_inhibit {
                return true;
            }
        }

        false
    }

    fn cycle_mode_1(&mut self) -> bool {
        // Mode 1: 5-step sequence
        let frame_length = if self.step == 3 { 14914 } else { 7456 };

        if self.cycles >= frame_length {
            self.cycles = 0;
            self.step = (self.step + 1) % 5;

            // No IRQ in mode 1
        }

        false
    }

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
        fc.write(0x00);
        assert_eq!(fc.mode, 0);
        assert!(!fc.irq_inhibit);
        assert_eq!(fc.step, 0);
        assert_eq!(fc.cycles, 0);

        // Test mode 1
        fc.write(0x80);
        assert_eq!(fc.mode, 1);
        assert!(!fc.irq_inhibit);

        // Test IRQ inhibit
        fc.write(0x40);
        assert_eq!(fc.mode, 0);
        assert!(fc.irq_inhibit);

        // Test both mode 1 and IRQ inhibit
        fc.write(0xC0);
        assert_eq!(fc.mode, 1);
        assert!(fc.irq_inhibit);
    }

    #[test]
    fn test_frame_counter_mode_0_cycle() {
        let mut fc = FrameCounter::new();
        fc.write(0x00); // Mode 0

        // Test first few cycles
        for _ in 0..7455 {
            assert!(!fc.cycle());
        }

        // 7456th cycle should advance step
        assert!(!fc.cycle());
        assert_eq!(fc.step, 1);
        assert_eq!(fc.cycles, 0);

        // Test step progression
        for _ in 0..7456 {
            assert!(!fc.cycle());
        }
        assert_eq!(fc.step, 2);

        for _ in 0..7456 {
            assert!(!fc.cycle());
        }
        assert_eq!(fc.step, 3);

        // Step 3 should generate IRQ (if not inhibited)
        for _ in 0..7455 {
            assert!(!fc.cycle());
        }
        assert!(fc.cycle()); // Should return true for IRQ
        assert_eq!(fc.step, 0); // Should wrap around
    }

    #[test]
    fn test_frame_counter_mode_0_irq() {
        let mut fc = FrameCounter::new();
        fc.write(0x00); // Mode 0, no IRQ inhibit

        // Advance to step 3
        for _ in 0..7456 * 3 {
            fc.cycle();
        }
        assert_eq!(fc.step, 3);

        // Step 3 should generate IRQ
        for _ in 0..7455 {
            assert!(!fc.cycle());
        }
        assert!(fc.cycle()); // Should return true for IRQ
        assert_eq!(fc.step, 0);
    }

    #[test]
    fn test_frame_counter_mode_0_irq_inhibit() {
        let mut fc = FrameCounter::new();
        fc.write(0x40); // Mode 0, IRQ inhibit

        // Advance to step 3
        for _ in 0..7456 * 3 {
            fc.cycle();
        }
        assert_eq!(fc.step, 3);

        // Step 3 should not generate IRQ when inhibited
        for _ in 0..7455 {
            assert!(!fc.cycle());
        }
        assert!(!fc.cycle()); // Should return false for IRQ
        assert_eq!(fc.step, 0);
    }

    #[test]
    fn test_frame_counter_mode_1_cycle() {
        let mut fc = FrameCounter::new();
        fc.write(0x80); // Mode 1

        // Test first step (7456 cycles)
        for _ in 0..7455 {
            assert!(!fc.cycle());
        }
        assert!(!fc.cycle());
        assert_eq!(fc.step, 1);
        assert_eq!(fc.cycles, 0);

        // Test second step (7456 cycles)
        for _ in 0..7456 {
            assert!(!fc.cycle());
        }
        assert_eq!(fc.step, 2);

        // Test third step (7456 cycles)
        for _ in 0..7456 {
            assert!(!fc.cycle());
        }
        assert_eq!(fc.step, 3);

        // Test fourth step (14914 cycles - longer)
        for _ in 0..14913 {
            assert!(!fc.cycle());
        }
        assert!(!fc.cycle());
        assert_eq!(fc.step, 4);

        // Test fifth step (7456 cycles)
        for _ in 0..7456 {
            assert!(!fc.cycle());
        }
        assert_eq!(fc.step, 0); // Should wrap around
    }

    #[test]
    fn test_frame_counter_mode_1_no_irq() {
        let mut fc = FrameCounter::new();
        fc.write(0x80); // Mode 1

        // Cycle through all steps - should never generate IRQ
        for _ in 0..7456 * 4 + 14914 {
            assert!(!fc.cycle());
        }
        assert_eq!(fc.step, 0);
    }

    #[test]
    fn test_frame_counter_get_step() {
        let mut fc = FrameCounter::new();
        assert_eq!(fc.get_step(), 0);

        fc.write(0x00); // Mode 0
        for _ in 0..7456 {
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

        // Test that exactly 7456 cycles advance one step
        for _ in 0..7456 {
            fc.cycle();
        }
        assert_eq!(fc.cycles, 0); // Should reset to 0
        assert_eq!(fc.step, 1);

        // Test that 7455 cycles don't advance
        fc.write(0x00); // Reset
        for _ in 0..7455 {
            fc.cycle();
        }
        assert_eq!(fc.step, 0); // Should still be 0
    }

    #[test]
    fn test_frame_counter_mode_1_timing_accuracy() {
        let mut fc = FrameCounter::new();
        fc.write(0x80); // Mode 1

        // Advance to step 3
        for _ in 0..7456 * 3 {
            fc.cycle();
        }
        assert_eq!(fc.step, 3);

        // Step 3 should take 14914 cycles
        for _ in 0..14913 {
            assert!(!fc.cycle());
        }
        assert!(!fc.cycle());
        assert_eq!(fc.step, 4);
        assert_eq!(fc.cycles, 0);
    }
}
