pub struct PulseChannel {
    duty_cycle: u8,
    #[allow(dead_code)]
    duty_value: u8,
    duty_step: u8,

    timer: u16,
    timer_value: u16,

    length_counter: u8,
    length_counter_halt: bool,

    volume: u8,
    constant_volume: bool,

    sweep_enabled: bool,
    sweep_period: u8,
    sweep_shift: u8,
    sweep_negate: bool,
    sweep_reload: bool,
    sweep_divider: u8,

    enabled: bool,

    // Output
    output: f32,
    // Add a field to store the last value written to timer high
    last_timer_high: u8,
}

impl PulseChannel {
    pub fn new() -> Self {
        Self {
            duty_cycle: 0,
            duty_value: 0,
            duty_step: 0,

            timer: 0,
            timer_value: 0,

            length_counter: 0,
            length_counter_halt: false,

            volume: 0,
            constant_volume: false,

            sweep_enabled: false,
            sweep_period: 0,
            sweep_shift: 0,
            sweep_negate: false,
            sweep_reload: false,
            sweep_divider: 0,

            enabled: false,

            output: 0.0,
            last_timer_high: 0,
        }
    }

    pub fn set_control(&mut self, value: u8) {
        self.duty_cycle = (value >> 6) & 3;
        self.length_counter_halt = (value >> 5) & 1 != 0;
        self.constant_volume = (value >> 4) & 1 != 0;
        self.volume = value & 0x0F;
    }

    pub fn set_sweep(&mut self, value: u8) {
        self.sweep_enabled = (value >> 7) & 1 != 0;
        self.sweep_period = (value >> 4) & 7;
        self.sweep_negate = (value >> 3) & 1 != 0;
        self.sweep_shift = value & 7;
        self.sweep_reload = true;
    }

    pub fn set_timer_low(&mut self, value: u8) {
        self.timer = (self.timer & 0xFF00) | value as u16;
    }

    pub fn set_timer_high(&mut self, value: u8) {
        // Timer uses bits 0-2, length counter uses bits 3-7
        self.timer = (self.timer & 0x00FF) | ((value & 0x07) as u16) << 8;
        self.timer_value = self.timer;
        self.last_timer_high = value;
        if self.enabled {
            self.length_counter = LENGTH_COUNTER_TABLE[((value >> 3) & 0x1F) as usize] as u8;
        }
        self.duty_step = 0;
        // Removed auto-enable logic
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        let was_disabled = !self.enabled;
        self.enabled = enabled;
        if !enabled {
            self.length_counter = 0;
        } else if was_disabled {
            // If enabling, reload length counter from last timer high value
            self.length_counter =
                LENGTH_COUNTER_TABLE[((self.last_timer_high >> 3) & 0x1F) as usize] as u8;
        }
    }

    pub fn cycle(&mut self) {
        if self.timer_value == 0 {
            self.timer_value = self.timer;
            self.duty_step = (self.duty_step + 1) % 8;
        } else {
            self.timer_value -= 1;
        }

        // Update sweep
        if self.sweep_reload {
            self.sweep_divider = self.sweep_period;
            self.sweep_reload = false;
        } else if self.sweep_divider == 0 {
            self.sweep_divider = self.sweep_period;
            if self.sweep_enabled && self.sweep_shift > 0 {
                let change = self.timer >> self.sweep_shift;
                if self.sweep_negate {
                    self.timer = self.timer.wrapping_sub(change);
                } else {
                    self.timer = self.timer.wrapping_add(change);
                }
            }
        } else {
            self.sweep_divider -= 1;
        }

        // Debug: print timer and length counter every 1000 cycles
        static mut CYCLE_COUNT: usize = 0;
        unsafe {
            CYCLE_COUNT += 1;
            if CYCLE_COUNT % 1000 == 0 {
                println!(
                    "Pulse Debug - enabled: {}, timer: {}, timer_value: {}, length_counter: {}",
                    self.enabled, self.timer, self.timer_value, self.length_counter
                );
            }
        }

        // Generate output
        self.generate_output();
    }

    fn generate_output(&mut self) {
        if !self.enabled || self.length_counter == 0 {
            self.output = 0.0;
            return;
        }

        // Get duty cycle value
        let duty_pattern = DUTY_CYCLES[self.duty_cycle as usize];
        let duty_bit = (duty_pattern >> (7 - self.duty_step)) & 1;

        if duty_bit == 0 {
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

// Duty cycle patterns (8-bit patterns)
const DUTY_CYCLES: [u8; 4] = [
    0b01000000, // 12.5%
    0b01100000, // 25%
    0b01111000, // 50%
    0b10011111, // 75%
];

// Length counter lookup table
const LENGTH_COUNTER_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];
