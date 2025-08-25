pub struct PulseChannel {
    channel_index: u8, // 0 for pulse 1, 1 for pulse 2
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
}

impl PulseChannel {
    pub fn new(channel_index: u8) -> Self {
        Self {
            channel_index,
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
        }
    }

    pub fn hard_reset(&mut self) {
        self.duty_cycle = 0;
        self.duty_value = 0;
        self.duty_step = 0;
        self.timer = 0;
        self.timer_value = 0;
        self.length_counter = 0;
        self.length_counter_halt = false;
        self.volume = 0;
        self.constant_volume = false;
        self.envelope_start = false;
        self.envelope_divider = 0;
        self.envelope_decay_level = 0;
        self.sweep_enabled = false;
        self.sweep_period = 0;
        self.sweep_shift = 0;
        self.sweep_negate = false;
        self.sweep_reload = false;
        self.sweep_divider = 0;
        self.enabled = false;
        self.output = 0.0;
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

        let length_index = ((value >> 3) & 0x1F) as usize;
        let length_value = LENGTH_COUNTER_TABLE[length_index];

        if self.enabled {
            self.length_counter = length_value;
        }

        self.duty_step = 0;
        // Start envelope when timer high is written
        self.envelope_start = true;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.length_counter = 0;
        }
        // Do NOT reload length counter here!
        // self.envelope_start = true; // (optional, only if envelope should restart on enable)
    }

    pub fn cycle(&mut self) {
        // Timer countdown and duty step advancement
        if self.timer_value == 0 {
            self.timer_value = self.timer;
            self.duty_step = (self.duty_step + 1) % 8;
        } else {
            self.timer_value -= 1;
        }

        // Generate output (do this every cycle)
        self.generate_output();
    }

    pub fn clock_sweep(&mut self) {
        // Handle divider first: reload if zero or reload flag is set, otherwise decrement
        if self.sweep_divider == 0 || self.sweep_reload {
            // Check if we should update the timer before reloading (when divider WAS zero)
            if self.sweep_divider == 0
                && self.sweep_enabled
                && self.sweep_shift > 0
                && !self.is_sweep_muting()
            {
                let target_period = self.calculate_target_period();
                self.timer = target_period;
            }

            self.sweep_divider = self.sweep_period;
            self.sweep_reload = false;
        } else {
            self.sweep_divider -= 1;
        }
    }

    fn calculate_target_period(&self) -> u16 {
        let change = self.timer >> self.sweep_shift;
        if self.sweep_negate {
            // Pulse 1 uses ones' complement, Pulse 2 uses two's complement
            if self.channel_index == 0 {
                // Ones' complement: subtract (change + 1)
                self.timer.wrapping_sub(change + 1)
            } else {
                // Two's complement: subtract change
                self.timer.wrapping_sub(change)
            }
        } else {
            self.timer.wrapping_add(change)
        }
    }

    fn is_sweep_muting(&self) -> bool {
        // Channel is muted if current period < 8 OR target period > 0x7FF
        self.timer < 8 || self.calculate_target_period() > 0x7FF
    }

    fn generate_output(&mut self) {
        // Channel is silenced if: not enabled, length counter = 0, timer < 8, or sweep muting
        if !self.enabled || self.length_counter == 0 || self.timer < 8 || self.is_sweep_muting() {
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
            self.envelope_divider = self.volume; // Divider reloads with volume parameter
        } else if self.envelope_divider == 0 {
            self.envelope_divider = self.volume; // Divider reloads with volume parameter
            if self.envelope_decay_level > 0 {
                self.envelope_decay_level -= 1;
            } else if self.length_counter_halt {
                self.envelope_decay_level = 15; // Loop flag causes reload to 15
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
            /*println!(
                "  Pulse1 clock_length_counter: before={}, after={}",
                self.length_counter,
                self.length_counter - 1
            );*/
            self.length_counter -= 1;
        }
    }

    pub fn get_length_counter(&self) -> u8 {
        self.length_counter
    }

    #[cfg(test)]
    pub fn is_enabled(&self) -> bool {
        self.enabled
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
        let pulse = PulseChannel::new(0);
        assert_eq!(pulse.channel_index, 0);
        assert_eq!(pulse.duty_cycle, 0);
        assert_eq!(pulse.timer, 0);
        assert_eq!(pulse.length_counter, 0);
        assert_eq!(pulse.volume, 0);
        assert!(!pulse.enabled);
        assert_eq!(pulse.output, 0.0);
    }

    #[test]
    fn test_pulse_channel_set_control() {
        let mut pulse = PulseChannel::new(0);

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
        let mut pulse = PulseChannel::new(0);

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
        let mut pulse = PulseChannel::new(0);

        // Test timer low
        pulse.set_timer_low(0x34);
        assert_eq!(pulse.timer & 0xFF, 0x34);

        // Test timer high
        pulse.set_timer_high(0x12); // Timer bits 0-2, length counter bits 3-7
        assert_eq!(pulse.timer >> 8, 0x02); // Only bits 0-2
    }

    #[test]
    fn test_pulse_channel_set_enabled() {
        let mut pulse = PulseChannel::new(0);

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
        let mut pulse = PulseChannel::new(0);

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
        let mut pulse = PulseChannel::new(0);

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
        let mut pulse = PulseChannel::new(0);

        // Test envelope start
        pulse.envelope_start = true;
        pulse.volume = 5;
        pulse.clock_envelope();
        assert!(!pulse.envelope_start);
        assert_eq!(pulse.envelope_decay_level, 15);
        assert_eq!(pulse.envelope_divider, 5); // Should be volume

        // Test envelope decay
        pulse.envelope_divider = 0;
        pulse.clock_envelope();
        assert_eq!(pulse.envelope_decay_level, 14);
        assert_eq!(pulse.envelope_divider, 5); // Should be volume

        // Test envelope reaching 0
        pulse.envelope_decay_level = 0;
        pulse.clock_envelope();
        assert_eq!(pulse.envelope_decay_level, 0);
    }

    #[test]
    fn test_pulse_channel_clock_length_counter() {
        let mut pulse = PulseChannel::new(0);

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
        let mut pulse = PulseChannel::new(0);

        // Set up sweep with a smaller timer to avoid muting
        pulse.sweep_enabled = true;
        pulse.sweep_period = 2;
        pulse.sweep_shift = 3;
        pulse.sweep_negate = false;
        pulse.timer = 100; // Use smaller timer to avoid target period > 0x7FF
        pulse.sweep_reload = true;
        pulse.sweep_divider = 1; // Start with non-zero divider

        // Verify we're not in a muting condition
        assert!(!pulse.is_sweep_muting(), "Should not be muting initially");

        // Test sweep reload - since reload flag is set, should reload divider
        pulse.clock_sweep();
        assert!(!pulse.sweep_reload);
        assert_eq!(pulse.sweep_divider, 2);
        assert_eq!(pulse.timer, 100); // Should not change due to reload

        // Test sweep calculation - need to clock until sweep divider reaches 0
        pulse.clock_sweep(); // divider = 1
        assert_eq!(pulse.sweep_divider, 1);
        assert_eq!(pulse.timer, 100); // Should not change yet

        pulse.clock_sweep(); // divider = 0, should decrement to 0
        assert_eq!(pulse.sweep_divider, 0);
        assert_eq!(pulse.timer, 100); // Still should not change yet

        pulse.clock_sweep(); // divider WAS 0, should update timer and reload
                             // Timer should change: 100 + (100 >> 3) = 100 + 12 = 112
        assert_eq!(pulse.timer, 112);
        assert_eq!(pulse.sweep_divider, 2); // Should reload
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

    #[test]
    fn test_sweep_muting_behavior() {
        let mut pulse = PulseChannel::new(0);
        pulse.enabled = true;
        pulse.length_counter = 10;
        pulse.constant_volume = true;
        pulse.volume = 15;
        pulse.duty_cycle = 2; // 50% duty
        pulse.duty_step = 4; // High bit

        // Test timer < 8 mutes channel
        pulse.timer = 7;
        pulse.generate_output();
        assert_eq!(pulse.output, 0.0, "Timer < 8 should mute channel");

        // Test valid timer produces output
        pulse.timer = 100;
        pulse.generate_output();
        assert_eq!(pulse.output, 1.0, "Valid timer should produce output");

        // Test sweep muting (target period > 0x7FF)
        pulse.timer = 0x7FF;
        pulse.sweep_shift = 1; // Change = 0x7FF >> 1 = 0x3FF
        pulse.sweep_negate = false; // Add change
                                    // Target period = 0x7FF + 0x3FF = 0xBFE > 0x7FF, so should mute
        assert!(
            pulse.is_sweep_muting(),
            "Target period > 0x7FF should cause muting"
        );
        pulse.generate_output();
        assert_eq!(pulse.output, 0.0, "Sweep muting should silence output");
    }

    #[test]
    fn test_sweep_calculation_pulse1_vs_pulse2() {
        // Test ones' complement (pulse 1) vs two's complement (pulse 2)
        let mut pulse1 = PulseChannel::new(0);
        let mut pulse2 = PulseChannel::new(1);

        pulse1.timer = 100;
        pulse2.timer = 100;
        pulse1.sweep_shift = 2; // Change = 100 >> 2 = 25
        pulse2.sweep_shift = 2;
        pulse1.sweep_negate = true;
        pulse2.sweep_negate = true;

        // Pulse 1: ones' complement = 100 - (25 + 1) = 74
        assert_eq!(pulse1.calculate_target_period(), 74);

        // Pulse 2: two's complement = 100 - 25 = 75
        assert_eq!(pulse2.calculate_target_period(), 75);
    }

    #[test]
    fn test_envelope_divider_timing() {
        let mut pulse = PulseChannel::new(0);

        // Set up envelope with volume 3
        pulse.volume = 3;
        pulse.constant_volume = false;
        pulse.envelope_start = true;

        // First clock should initialize envelope
        pulse.clock_envelope();
        assert!(!pulse.envelope_start);
        assert_eq!(pulse.envelope_decay_level, 15);
        assert_eq!(pulse.envelope_divider, 3); // Should be volume, not volume + 1

        // Clock 3 more times to reach divider = 0
        for _ in 0..3 {
            pulse.clock_envelope();
        }
        assert_eq!(pulse.envelope_divider, 0);
        assert_eq!(pulse.envelope_decay_level, 15); // Should not have changed yet

        // Next clock should reload divider and decrement decay level
        pulse.clock_envelope();
        assert_eq!(pulse.envelope_divider, 3);
        assert_eq!(pulse.envelope_decay_level, 14);
    }

    #[test]
    fn test_envelope_loop_behavior() {
        let mut pulse = PulseChannel::new(0);

        pulse.volume = 1; // Fast envelope
        pulse.constant_volume = false;
        pulse.length_counter_halt = true; // Enable loop
        pulse.envelope_decay_level = 1; // Almost at bottom
        pulse.envelope_divider = 0; // Ready to decrement

        // Should decrement to 0
        pulse.clock_envelope();
        assert_eq!(pulse.envelope_decay_level, 0);

        // Next clock should loop back to 15
        pulse.envelope_divider = 0;
        pulse.clock_envelope();
        assert_eq!(pulse.envelope_decay_level, 15);
    }

    #[test]
    fn test_timer_frequency_calculation() {
        let mut pulse = PulseChannel::new(0);

        // Set timer to specific value
        pulse.timer = 100; // Should give frequency of 1789773 / (16 * 101) ≈ 1108 Hz
        pulse.timer_value = 0; // Force immediate reload

        let initial_step = pulse.duty_step;

        // Should advance duty step when timer reaches 0
        pulse.cycle();
        assert_eq!(pulse.duty_step, (initial_step + 1) % 8);
        assert_eq!(pulse.timer_value, 100); // Should reload with timer value
    }

    #[test]
    fn test_duty_cycle_waveforms() {
        let mut pulse = PulseChannel::new(0);
        pulse.enabled = true;
        pulse.length_counter = 10;
        pulse.timer = 100;
        pulse.constant_volume = true;
        pulse.volume = 15;

        // Test 12.5% duty cycle (should be high only on step 1)
        pulse.duty_cycle = 0;
        for step in 0..8 {
            pulse.duty_step = step;
            pulse.generate_output();
            if step == 1 {
                assert_eq!(pulse.output, 1.0, "12.5% duty should be high on step 1");
            } else {
                assert_eq!(
                    pulse.output, 0.0,
                    "12.5% duty should be low on step {}",
                    step
                );
            }
        }

        // Test 50% duty cycle (should be high on steps 1-4)
        pulse.duty_cycle = 2;
        for step in 0..8 {
            pulse.duty_step = step;
            pulse.generate_output();
            if (1..=4).contains(&step) {
                assert_eq!(
                    pulse.output, 1.0,
                    "50% duty should be high on step {}",
                    step
                );
            } else {
                assert_eq!(pulse.output, 0.0, "50% duty should be low on step {}", step);
            }
        }
    }

    #[test]
    fn test_constant_volume_vs_envelope() {
        let mut pulse = PulseChannel::new(0);
        pulse.enabled = true;
        pulse.length_counter = 10;
        pulse.timer = 100;
        pulse.duty_cycle = 2;
        pulse.duty_step = 4; // High bit for 50% duty

        // Test constant volume mode
        pulse.constant_volume = true;
        pulse.volume = 10;
        pulse.envelope_decay_level = 5; // Different from volume
        pulse.generate_output();
        assert_eq!(
            pulse.output,
            10.0 / 15.0,
            "Should use volume in constant volume mode"
        );

        // Test envelope mode
        pulse.constant_volume = false;
        pulse.generate_output();
        assert_eq!(
            pulse.output,
            5.0 / 15.0,
            "Should use envelope decay level in envelope mode"
        );
    }

    #[test]
    fn test_sweep_unit_clock_timing() {
        let mut pulse = PulseChannel::new(0);
        pulse.sweep_enabled = true;
        pulse.sweep_period = 2;
        pulse.sweep_shift = 1;
        pulse.sweep_negate = false;
        pulse.timer = 100;
        pulse.sweep_reload = true;
        pulse.sweep_divider = 1; // Start with non-zero divider

        // First clock should reload divider due to reload flag
        pulse.clock_sweep();
        assert!(!pulse.sweep_reload);
        assert_eq!(pulse.sweep_divider, 2);
        assert_eq!(pulse.timer, 100); // Should not change yet

        // Clock until divider reaches 0
        pulse.clock_sweep(); // divider = 1
        assert_eq!(pulse.sweep_divider, 1);
        pulse.clock_sweep(); // divider = 0
        assert_eq!(pulse.sweep_divider, 0);
        pulse.clock_sweep(); // divider WAS 0, should update timer and reload
        assert_eq!(pulse.timer, 150); // 100 + (100 >> 1) = 150
        assert_eq!(pulse.sweep_divider, 2); // Should reload
    }
}
