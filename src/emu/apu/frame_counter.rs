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
        let frame_length = 7457; // CPU cycles per frame step

        if self.cycles >= frame_length {
            self.cycles = 0;
            self.step = (self.step + 1) % 4;

            // Generate IRQ on step 3 (if not inhibited)
            if self.step == 3 && !self.irq_inhibit {
                return true;
            }
        }

        false
    }

    fn cycle_mode_1(&mut self) -> bool {
        // Mode 1: 5-step sequence
        let frame_length = if self.step == 3 { 14915 } else { 7457 };

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
