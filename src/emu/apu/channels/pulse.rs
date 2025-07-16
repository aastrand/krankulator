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

    // Envelope
    envelope_start: bool,
    envelope_divider: u8,
    envelope_decay_level: u8,

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

            envelope_start: false,
            envelope_divider: 0,
            envelope_decay_level: 0,

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

        // Start envelope when control is written
        self.envelope_start = true;
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

        let length_index = ((value >> 3) & 0x1F) as usize;
        let length_value = LENGTH_COUNTER_TABLE[length_index];

        println!(
            "  Pulse1 set_timer_high: value={:02X}, length_index={}, length_value={}, enabled={}",
            value, length_index, length_value, self.enabled
        );

        if self.enabled {
            self.length_counter = length_value;
            println!("  Pulse1 length counter set to: {}", self.length_counter);
        } else {
            println!("  Pulse1 not enabled, length counter not set");
        }

        self.duty_step = 0;
        // Start envelope when timer high is written
        self.envelope_start = true;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.length_counter = 0;
        } else {
            // When enabling, set length counter from last timer high write
            if self.last_timer_high != 0 {
                let length_index = ((self.last_timer_high >> 3) & 0x1F) as usize;
                self.length_counter = LENGTH_COUNTER_TABLE[length_index];
            }
            // Restart envelope when enabling
            self.envelope_start = true;
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
                // Clamp timer to valid range
                if self.timer < 8 {
                    self.timer = 8;
                }
            }
        } else {
            self.sweep_divider -= 1;
        }

        // Generate output
        self.generate_output();
    }

    fn generate_output(&mut self) {
        if !self.enabled || self.length_counter == 0 || self.timer < 8 {
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
                self.envelope_decay_level
            };
            self.output = vol as f32 / 15.0;
        }
    }

    pub fn clock_envelope(&mut self) {
        if self.envelope_start {
            self.envelope_start = false;
            self.envelope_decay_level = 15;
            self.envelope_divider = self.volume + 1; // NES APU: divider = volume + 1
        } else if self.envelope_divider == 0 {
            self.envelope_divider = self.volume + 1; // NES APU: divider = volume + 1
            if self.envelope_decay_level > 0 {
                self.envelope_decay_level -= 1;
            } else if self.length_counter_halt {
                self.envelope_decay_level = 15;
            }
        } else {
            self.envelope_divider -= 1;
        }
    }

    pub fn get_sample(&self) -> f32 {
        self.output
    }

    pub fn clock_length_counter(&mut self) {
        if !self.length_counter_halt && self.length_counter > 0 {
            println!(
                "  Pulse1 clock_length_counter: before={}, after={}",
                self.length_counter,
                self.length_counter - 1
            );
            self.length_counter -= 1;
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn get_length_counter(&self) -> u8 {
        self.length_counter
    }

    pub fn get_timer(&self) -> u16 {
        self.timer
    }

    pub fn output(&self) -> u8 {
        let output = if !self.enabled || self.length_counter == 0 || self.timer < 8 {
            0
        } else {
            // Simplified output logic for debugging
            let env = if self.constant_volume {
                self.volume
            } else {
                self.envelope_decay_level
            };
            let duty_val = (DUTY_CYCLES[self.duty_cycle as usize] >> (7 - self.duty_step)) & 1;
            if duty_val == 0 {
                0
            } else {
                env
            }
        };
        println!(
            "  Pulse1 output: enabled={}, length_counter={}, timer={}, duty_cycle={:02b}, duty_step={}, constant_volume={}, volume={}, output={}",
            self.enabled,
            self.length_counter,
            self.timer,
            self.duty_cycle,
            self.duty_step,
            self.constant_volume,
            self.volume,
            output
        );
        output
    }
}

// Duty cycle patterns (8-bit patterns) - Fixed patterns
const DUTY_CYCLES: [u8; 4] = [
    0b01000000, // 12.5% (0,0,0,0,0,0,1,0)
    0b01100000, // 25%   (0,0,0,0,0,1,1,0)
    0b01111000, // 50%   (0,0,0,1,1,1,1,0)
    0b10011111, // 75%   (1,1,1,1,0,0,1,0)
];

// Length counter lookup table
const LENGTH_COUNTER_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pulse_channel_new() {
        let pulse = PulseChannel::new();
        assert_eq!(pulse.duty_cycle, 0);
        assert_eq!(pulse.timer, 0);
        assert_eq!(pulse.length_counter, 0);
        assert_eq!(pulse.volume, 0);
        assert!(!pulse.enabled);
        assert_eq!(pulse.output, 0.0);
    }

    #[test]
    fn test_pulse_channel_set_control() {
        let mut pulse = PulseChannel::new();

        // Test duty cycle setting
        pulse.set_control(0b11000000); // Duty cycle 3 (75%)
        assert_eq!(pulse.duty_cycle, 3);

        // Test length counter halt
        pulse.set_control(0b00100000); // Length counter halt
        assert!(pulse.length_counter_halt);

        // Test constant volume
        pulse.set_control(0b00010000); // Constant volume
        assert!(pulse.constant_volume);

        // Test volume setting
        pulse.set_control(0b00001111); // Volume 15
        assert_eq!(pulse.volume, 15);

        // Test envelope start
        assert!(pulse.envelope_start);
    }

    #[test]
    fn test_pulse_channel_set_sweep() {
        let mut pulse = PulseChannel::new();

        // Test sweep enable
        pulse.set_sweep(0b10000000); // Sweep enabled
        assert!(pulse.sweep_enabled);

        // Test sweep period
        pulse.set_sweep(0b01110000); // Period 7
        assert_eq!(pulse.sweep_period, 7);

        // Test sweep negate
        pulse.set_sweep(0b00001000); // Negate enabled
        assert!(pulse.sweep_negate);

        // Test sweep shift
        pulse.set_sweep(0b00000111); // Shift 7
        assert_eq!(pulse.sweep_shift, 7);

        // Test sweep reload
        assert!(pulse.sweep_reload);
    }

    #[test]
    fn test_pulse_channel_set_timer() {
        let mut pulse = PulseChannel::new();

        // Test timer low
        pulse.set_timer_low(0x34);
        assert_eq!(pulse.timer & 0xFF, 0x34);

        // Test timer high
        pulse.set_timer_high(0x12); // Timer bits 0-2, length counter bits 3-7
        assert_eq!(pulse.timer >> 8, 0x02); // Only bits 0-2
        assert_eq!(pulse.last_timer_high, 0x12);
    }

    #[test]
    fn test_pulse_channel_set_enabled() {
        let mut pulse = PulseChannel::new();

        // Test enabling
        pulse.set_enabled(true);
        assert!(pulse.enabled);

        // Test disabling
        pulse.set_enabled(false);
        assert!(!pulse.enabled);
        assert_eq!(pulse.length_counter, 0);
    }

    #[test]
    fn test_pulse_channel_cycle() {
        let mut pulse = PulseChannel::new();

        // Set up a basic timer
        pulse.set_timer_low(0x10);
        pulse.set_timer_high(0x00);
        pulse.enabled = true;
        pulse.length_counter = 10;

        // Reset timer_value to 0 to test immediate advancement
        pulse.timer_value = 0;

        // Cycle should advance duty step when timer reaches 0
        let initial_step = pulse.duty_step;
        // First cycle should advance immediately since timer_value starts at 0
        pulse.cycle();
        assert_eq!(pulse.duty_step, (initial_step + 1) % 8);
    }

    #[test]
    fn test_pulse_channel_generate_output() {
        let mut pulse = PulseChannel::new();

        // Test disabled channel
        pulse.generate_output();
        assert_eq!(pulse.output, 0.0);

        // Test enabled channel with duty cycle
        pulse.enabled = true;
        pulse.length_counter = 10;
        pulse.timer = 100; // Valid timer
        pulse.duty_cycle = 2; // 50% duty cycle
        pulse.duty_step = 0;
        pulse.constant_volume = true;
        pulse.volume = 15;

        pulse.generate_output();
        // Duty cycle 2, step 0 should be 0 (first bit is 0)
        assert_eq!(pulse.output, 0.0);

        // Test step 4 which should be 1
        pulse.duty_step = 4;
        pulse.generate_output();
        assert_eq!(pulse.output, 1.0); // 15/15 = 1.0
    }

    #[test]
    fn test_pulse_channel_clock_envelope() {
        let mut pulse = PulseChannel::new();

        // Test envelope start
        pulse.envelope_start = true;
        pulse.volume = 5;
        pulse.clock_envelope();
        assert!(!pulse.envelope_start);
        assert_eq!(pulse.envelope_decay_level, 15);
        assert_eq!(pulse.envelope_divider, 6); // Should be volume + 1

        // Test envelope decay
        pulse.envelope_divider = 0;
        pulse.clock_envelope();
        assert_eq!(pulse.envelope_decay_level, 14);
        assert_eq!(pulse.envelope_divider, 6); // Should be volume + 1

        // Test envelope reaching 0
        pulse.envelope_decay_level = 0;
        pulse.clock_envelope();
        assert_eq!(pulse.envelope_decay_level, 0);
    }

    #[test]
    fn test_pulse_channel_clock_length_counter() {
        let mut pulse = PulseChannel::new();

        // Test normal decrement
        pulse.length_counter = 10;
        pulse.length_counter_halt = false;
        pulse.clock_length_counter();
        assert_eq!(pulse.length_counter, 9);

        // Test halt behavior
        pulse.length_counter_halt = true;
        pulse.clock_length_counter();
        assert_eq!(pulse.length_counter, 9); // Should not decrement

        // Test reaching 0
        pulse.length_counter = 0;
        pulse.clock_length_counter();
        assert_eq!(pulse.length_counter, 0); // Should not go below 0
    }

    #[test]
    fn test_pulse_channel_sweep() {
        let mut pulse = PulseChannel::new();

        // Set up sweep
        pulse.sweep_enabled = true;
        pulse.sweep_period = 2;
        pulse.sweep_shift = 3;
        pulse.sweep_negate = false;
        pulse.timer = 1000;
        pulse.sweep_reload = true;

        // Test sweep reload
        pulse.cycle();
        assert!(!pulse.sweep_reload);
        assert_eq!(pulse.sweep_divider, 2);

        // Test sweep calculation - need to cycle until sweep divider reaches 0
        // Sweep divider starts at 2, so we need 3 cycles to reach 0
        for _ in 0..3 {
            pulse.cycle(); // Advance sweep divider
        }
        // Timer should change: 1000 + (1000 >> 3) = 1000 + 125 = 1125
        assert_eq!(pulse.timer, 1125);
    }

    #[test]
    fn test_duty_cycles() {
        // Test duty cycle patterns
        assert_eq!(DUTY_CYCLES[0], 0b01000000); // 12.5%
        assert_eq!(DUTY_CYCLES[1], 0b01100000); // 25%
        assert_eq!(DUTY_CYCLES[2], 0b01111000); // 50%
        assert_eq!(DUTY_CYCLES[3], 0b10011111); // 75%
    }

    #[test]
    fn test_length_counter_table() {
        // Test some known values from the table
        assert_eq!(LENGTH_COUNTER_TABLE[0], 10);
        assert_eq!(LENGTH_COUNTER_TABLE[1], 254);
        assert_eq!(LENGTH_COUNTER_TABLE[2], 20);
        assert_eq!(LENGTH_COUNTER_TABLE[31], 30);
    }
}
