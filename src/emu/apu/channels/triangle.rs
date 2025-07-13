pub struct TriangleChannel {
    control: u8,
    linear_counter: u8,
    linear_counter_reload: u8,
    linear_counter_reload_flag: bool,

    timer: u16,
    timer_value: u16,

    length_counter: u8,
    length_counter_halt: bool,

    enabled: bool,

    // Output
    step: u8,
    output: f32,
}

impl TriangleChannel {
    pub fn new() -> Self {
        Self {
            control: 0,
            linear_counter: 0,
            linear_counter_reload: 0,
            linear_counter_reload_flag: false,

            timer: 0,
            timer_value: 0,

            length_counter: 0,
            length_counter_halt: false,

            enabled: false,

            step: 0,
            output: 0.0,
        }
    }

    pub fn set_control(&mut self, value: u8) {
        self.control = value;
        self.length_counter_halt = (value >> 7) & 1 != 0;
        self.linear_counter_reload = value & 0x7F;
    }

    pub fn set_timer_low(&mut self, value: u8) {
        self.timer = (self.timer & 0xFF00) | value as u16;
    }

    pub fn set_timer_high(&mut self, value: u8) {
        self.timer = (self.timer & 0x00FF) | ((value & 7) as u16) << 8;
        self.timer_value = self.timer;

        if self.enabled {
            self.length_counter = LENGTH_COUNTER_TABLE[((value >> 3) & 0x1F) as usize] as u8;
        }

        self.linear_counter_reload_flag = true;
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
            if self.linear_counter > 0 && self.length_counter > 0 {
                self.step = (self.step + 1) % 32;
            }
        } else {
            self.timer_value -= 1;
        }

        // Generate output
        self.generate_output();
    }

    fn generate_output(&mut self) {
        if !self.enabled || self.length_counter == 0 || self.linear_counter == 0 {
            self.output = 0.0;
            return;
        }

        // Triangle wave pattern
        let triangle_value = TRIANGLE_WAVE[self.step as usize];
        self.output = (triangle_value as f32 - 7.5) / 7.5; // Normalize to [-1.0, 1.0]
    }

    pub fn get_sample(&self) -> f32 {
        self.output
    }

    pub fn clock_linear_counter(&mut self) {
        if self.linear_counter_reload_flag {
            self.linear_counter = self.linear_counter_reload;
        } else if self.linear_counter > 0 {
            self.linear_counter -= 1;
        }

        if !self.length_counter_halt {
            self.linear_counter_reload_flag = false;
        }
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

// Triangle wave pattern (32 steps)
const TRIANGLE_WAVE: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
    13, 14, 15,
];

// Length counter lookup table
const LENGTH_COUNTER_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];
