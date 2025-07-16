pub struct NoiseChannel {
    control: u8,
    volume: u8,
    constant_volume: bool,

    // Envelope
    envelope_start: bool,
    envelope_divider: u8,
    envelope_decay_level: u8,

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

    // Store last length counter value for proper initialization
    last_length_counter: u8,
}

impl NoiseChannel {
    pub fn new() -> Self {
        Self {
            control: 0,
            volume: 0,
            constant_volume: false,

            envelope_start: false,
            envelope_divider: 0,
            envelope_decay_level: 0,

            period: 0,
            timer: 0,
            timer_value: 0,

            length_counter: 0,
            length_counter_halt: false,

            enabled: false,

            shift_register: 1, // Initialize to 1
            output: 0.0,
            last_length_counter: 0,
        }
    }

    pub fn set_control(&mut self, value: u8) {
        self.control = value;
        self.length_counter_halt = (value >> 5) & 1 != 0;
        self.constant_volume = (value >> 4) & 1 != 0;
        self.volume = value & 0x0F;

        // Start envelope when control is written
        self.envelope_start = true;
    }

    pub fn set_period(&mut self, value: u8) {
        self.period = value & 0x0F;
        self.timer = NOISE_PERIODS[self.period as usize];
        self.timer_value = self.timer;
    }

    pub fn set_length_counter(&mut self, value: u8) {
        self.last_length_counter = value;
        if self.enabled {
            self.length_counter = LENGTH_COUNTER_TABLE[((value >> 3) & 0x1F) as usize] as u8;
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        let was_disabled = !self.enabled;
        self.enabled = enabled;
        if !enabled {
            self.length_counter = 0;
        } else {
            // When enabling, set length counter from last length counter write
            if self.last_length_counter != 0 && self.timer > 0 {
                self.length_counter =
                    LENGTH_COUNTER_TABLE[((self.last_length_counter >> 3) & 0x1F) as usize] as u8;
            }
            // Restart envelope when enabling
            self.envelope_start = true;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noise_channel_new() {
        let noise = NoiseChannel::new();
        assert_eq!(noise.control, 0);
        assert_eq!(noise.volume, 0);
        assert!(!noise.constant_volume);
        assert!(!noise.envelope_start);
        assert_eq!(noise.envelope_divider, 0);
        assert_eq!(noise.envelope_decay_level, 0);
        assert_eq!(noise.period, 0);
        assert_eq!(noise.timer, 0);
        assert_eq!(noise.length_counter, 0);
        assert!(!noise.length_counter_halt);
        assert!(!noise.enabled);
        assert_eq!(noise.shift_register, 1);
        assert_eq!(noise.output, 0.0);
        assert_eq!(noise.last_length_counter, 0);
    }

    #[test]
    fn test_noise_channel_set_control() {
        let mut noise = NoiseChannel::new();

        // Test length counter halt
        noise.set_control(0b00100000); // Length counter halt
        assert!(noise.length_counter_halt);

        // Test constant volume
        noise.set_control(0b00010000); // Constant volume
        assert!(noise.constant_volume);

        // Test volume setting
        noise.set_control(0b00001111); // Volume 15
        assert_eq!(noise.volume, 15);

        // Test envelope start flag
        assert!(noise.envelope_start);
    }

    #[test]
    fn test_noise_channel_set_period() {
        let mut noise = NoiseChannel::new();

        // Test period 0
        noise.set_period(0x00);
        assert_eq!(noise.period, 0);
        assert_eq!(noise.timer, NOISE_PERIODS[0]);

        // Test period 15
        noise.set_period(0x0F);
        assert_eq!(noise.period, 15);
        assert_eq!(noise.timer, NOISE_PERIODS[15]);

        // Test that higher bits are ignored
        noise.set_period(0xF0);
        assert_eq!(noise.period, 0);
    }

    #[test]
    fn test_noise_channel_set_length_counter() {
        let mut noise = NoiseChannel::new();

        // Test when disabled
        noise.set_length_counter(0x20); // Length counter index 4
        assert_eq!(noise.length_counter, 0); // Should not change when disabled

        // Test when enabled
        noise.enabled = true;
        noise.set_length_counter(0x20); // Length counter index 4
        assert_eq!(noise.length_counter, LENGTH_COUNTER_TABLE[4]);
    }

    #[test]
    fn test_noise_channel_set_enabled() {
        let mut noise = NoiseChannel::new();

        // Test enabling
        noise.set_enabled(true);
        assert!(noise.enabled);

        // Test disabling
        noise.set_enabled(false);
        assert!(!noise.enabled);
        assert_eq!(noise.length_counter, 0);

        // Test enabling with valid timer
        noise.timer = 100;
        noise.last_length_counter = 0x20; // Length counter index 4
        noise.set_enabled(true);
        assert_eq!(noise.length_counter, LENGTH_COUNTER_TABLE[4]);
    }

    #[test]
    fn test_noise_channel_cycle() {
        let mut noise = NoiseChannel::new();

        // Set up a basic timer
        noise.set_period(0x01); // Period 1 = timer 8
        noise.enabled = true;
        noise.length_counter = 10;

        // Cycle should advance timer
        let initial_timer = noise.timer_value;
        noise.cycle();
        assert_eq!(noise.timer_value, initial_timer - 1);

        // Cycle until timer reaches 0
        for _ in 0..initial_timer {
            noise.cycle();
        }
        assert_eq!(noise.timer_value, noise.timer); // Should reset to timer
    }

    #[test]
    fn test_noise_channel_clock_shift_register() {
        let mut noise = NoiseChannel::new();

        // Test mode 0 (1-bit feedback)
        noise.shift_register = 0x0001;
        noise.control = 0x00; // Mode 0
        noise.clock_shift_register();
        // Feedback should be 1 ^ 0 = 1, so new value should be 0x4000
        assert_eq!(noise.shift_register, 0x4000);

        // Test mode 1 (6-bit feedback)
        noise.shift_register = 0x0040; // Bit 6 set
        noise.control = 0x80; // Mode 1
        noise.clock_shift_register();
        // Feedback should be 1 ^ 0 = 1, so new value should be 0x4020
        assert_eq!(noise.shift_register, 0x4020);
    }

    #[test]
    fn test_noise_channel_generate_output() {
        let mut noise = NoiseChannel::new();

        // Test disabled channel
        noise.generate_output();
        assert_eq!(noise.output, 0.0);

        // Test enabled channel with length counter 0
        noise.enabled = true;
        noise.length_counter = 0;
        noise.generate_output();
        assert_eq!(noise.output, 0.0);

        // Test enabled channel with valid conditions and LSB = 0
        noise.length_counter = 10;
        noise.shift_register = 0x0002; // LSB = 0
        noise.constant_volume = true;
        noise.volume = 8;
        noise.generate_output();
        assert_eq!(noise.output, 0.0); // LSB is 0, so output is 0

        // Test enabled channel with LSB = 1 and constant volume
        noise.shift_register = 0x0001; // LSB = 1
        noise.generate_output();
        assert_eq!(noise.output, 8.0 / 15.0); // Volume 8 normalized

        // Test enabled channel with envelope volume
        noise.constant_volume = false;
        noise.envelope_decay_level = 12;
        noise.generate_output();
        assert_eq!(noise.output, 12.0 / 15.0); // Envelope level 12 normalized
    }

    #[test]
    fn test_noise_channel_clock_envelope() {
        let mut noise = NoiseChannel::new();

        // Test envelope start
        noise.envelope_start = true;
        noise.volume = 5;
        noise.clock_envelope();
        assert_eq!(noise.envelope_decay_level, 15);
        assert_eq!(noise.envelope_divider, 6); // Should be volume + 1
        assert!(!noise.envelope_start);

        // Test normal divider countdown
        noise.envelope_divider = 0; // Set to 0 to trigger reset
        noise.clock_envelope();
        assert_eq!(noise.envelope_divider, 6); // Should reset to volume + 1
        assert_eq!(noise.envelope_decay_level, 14); // Should decrement

        // Test envelope reaching 0
        noise.envelope_decay_level = 0;
        noise.length_counter_halt = false;
        noise.clock_envelope();
        assert_eq!(noise.envelope_decay_level, 0); // Should stay at 0

        // Test envelope restart with halt
        noise.length_counter_halt = true;
        noise.envelope_divider = 0; // Set to 0 to trigger reset
        noise.clock_envelope();
        assert_eq!(noise.envelope_decay_level, 15); // Should restart
    }

    #[test]
    fn test_noise_channel_clock_length_counter() {
        let mut noise = NoiseChannel::new();

        // Test normal decrement
        noise.length_counter = 10;
        noise.length_counter_halt = false;
        noise.clock_length_counter();
        assert_eq!(noise.length_counter, 9);

        // Test halt behavior
        noise.length_counter_halt = true;
        noise.clock_length_counter();
        assert_eq!(noise.length_counter, 9); // Should not decrement

        // Test reaching 0
        noise.length_counter = 0;
        noise.clock_length_counter();
        assert_eq!(noise.length_counter, 0); // Should not go below 0
    }

    #[test]
    fn test_noise_periods_table() {
        // Test some known values from the table
        assert_eq!(NOISE_PERIODS[0], 4);
        assert_eq!(NOISE_PERIODS[1], 8);
        assert_eq!(NOISE_PERIODS[15], 4068);

        // Test that periods increase with index
        for i in 1..16 {
            assert!(NOISE_PERIODS[i] > NOISE_PERIODS[i - 1]);
        }
    }

    #[test]
    fn test_noise_shift_register_initialization() {
        let noise = NoiseChannel::new();
        // Shift register should be initialized to 1 (not 0)
        assert_eq!(noise.shift_register, 1);
    }

    #[test]
    fn test_noise_channel_output_range() {
        let mut noise = NoiseChannel::new();
        noise.enabled = true;
        noise.length_counter = 10;
        noise.shift_register = 0x0001; // LSB = 1

        // Test maximum volume
        noise.constant_volume = true;
        noise.volume = 15;
        noise.generate_output();
        assert_eq!(noise.output, 1.0); // 15/15 = 1.0

        // Test minimum volume
        noise.volume = 0;
        noise.generate_output();
        assert_eq!(noise.output, 0.0); // 0/15 = 0.0
    }
}
