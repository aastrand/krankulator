pub struct NoiseChannel {
    control: u8,
    volume: u8,
    constant_volume: bool,

    period: u8,
    timer: u16,
    timer_value: u16,

    length_counter: u8,
    length_counter_halt: bool,

    enabled: bool,

    // LFSR (Linear Feedback Shift Register)
    shift_register: u16,

    // Output
    output: f32,
}

impl NoiseChannel {
    pub fn new() -> Self {
        Self {
            control: 0,
            volume: 0,
            constant_volume: false,

            period: 0,
            timer: 0,
            timer_value: 0,

            length_counter: 0,
            length_counter_halt: false,

            enabled: false,

            shift_register: 1, // Initialize to 1
            output: 0.0,
        }
    }

    pub fn set_control(&mut self, value: u8) {
        self.control = value;
        self.length_counter_halt = (value >> 5) & 1 != 0;
        self.constant_volume = (value >> 4) & 1 != 0;
        self.volume = value & 0x0F;
    }

    pub fn set_period(&mut self, value: u8) {
        self.period = value & 0x0F;
        self.timer = NOISE_PERIODS[self.period as usize];
        self.timer_value = self.timer;
    }

    pub fn set_length_counter(&mut self, value: u8) {
        if self.enabled {
            self.length_counter = LENGTH_COUNTER_TABLE[((value >> 3) & 0x1F) as usize] as u8;
        }
        // Removed auto-enable logic
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        let was_disabled = !self.enabled;
        self.enabled = enabled;
        if !enabled {
            self.length_counter = 0;
        } else if was_disabled {
            // If enabling and we have a valid timer, initialize length counter
            if self.timer > 0 {
                // Use a default length counter value if none was set
                if self.length_counter == 0 {
                    self.length_counter = 254; // Default to a long duration
                }
            }
        }
    }

    pub fn cycle(&mut self) {
        if self.timer_value == 0 {
            self.timer_value = self.timer;
            self.clock_shift_register();
        } else {
            self.timer_value -= 1;
        }

        // Generate output
        self.generate_output();
    }

    fn clock_shift_register(&mut self) {
        let feedback = if (self.control & 0x80) != 0 {
            // Mode 1: 6-bit feedback
            ((self.shift_register >> 6) & 1) ^ ((self.shift_register >> 5) & 1)
        } else {
            // Mode 0: 1-bit feedback
            ((self.shift_register >> 1) & 1) ^ (self.shift_register & 1)
        };

        self.shift_register >>= 1;
        self.shift_register |= feedback << 14;
    }

    fn generate_output(&mut self) {
        if !self.enabled || self.length_counter == 0 {
            self.output = 0.0;
            return;
        }

        // Check if the least significant bit is set
        if (self.shift_register & 1) == 0 {
            self.output = 0.0;
        } else {
            let vol = if self.constant_volume {
                self.volume
            } else {
                // Envelope would go here
                self.volume
            };
            self.output = vol as f32 / 15.0;
        }
    }

    pub fn get_sample(&self) -> f32 {
        self.output
    }

    pub fn clock_length_counter(&mut self) {
        if !self.length_counter_halt && self.length_counter > 0 {
            self.length_counter -= 1;
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn get_length_counter(&self) -> u8 {
        self.length_counter
    }
}

// Noise period lookup table
const NOISE_PERIODS: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];

// Length counter lookup table
const LENGTH_COUNTER_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];
