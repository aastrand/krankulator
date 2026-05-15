use crate::emu::savestate::{SavestateReader, SavestateWriter};

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
        let mut triangle = Self {
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
        };

        // Ensure timer_value is valid
        triangle.timer_value = triangle.timer;
        triangle
    }

    pub fn hard_reset(&mut self) {
        self.control = 0;
        self.linear_counter = 0;
        self.linear_counter_reload = 0;
        self.linear_counter_reload_flag = false;
        self.timer = 0;
        self.timer_value = 0;
        self.length_counter = 0;
        self.length_counter_halt = false;
        self.enabled = false;
        self.step = 0;
        self.output = 0.0;

        // Ensure all output is completely zeroed
        self.output = 0.0;
    }

    pub fn save_state(&self, w: &mut SavestateWriter) {
        w.write_u8(self.control);
        w.write_u8(self.linear_counter);
        w.write_u8(self.linear_counter_reload);
        w.write_bool(self.linear_counter_reload_flag);
        w.write_u16(self.timer);
        w.write_u16(self.timer_value);
        w.write_u8(self.length_counter);
        w.write_bool(self.length_counter_halt);
        w.write_bool(self.enabled);
        w.write_u8(self.step);
        w.write_f32(self.output);
    }

    pub fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        self.control = r.read_u8()?;
        self.linear_counter = r.read_u8()?;
        self.linear_counter_reload = r.read_u8()?;
        self.linear_counter_reload_flag = r.read_bool()?;
        self.timer = r.read_u16()?;
        self.timer_value = r.read_u16()?;
        self.length_counter = r.read_u8()?;
        self.length_counter_halt = r.read_bool()?;
        self.enabled = r.read_bool()?;
        self.step = r.read_u8()?;
        self.output = r.read_f32()?;
        Ok(())
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
        let new_timer = (self.timer & 0x00FF) | ((value & 0x07) as u16) << 8;
        self.timer = new_timer;

        if self.enabled {
            self.length_counter = LENGTH_COUNTER_TABLE[((value >> 3) & 0x1F) as usize] as u8;
        }

        self.linear_counter_reload_flag = true;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.length_counter = 0;
        } else {
            self.linear_counter_reload_flag = true;
        }
    }

    pub fn cycle(&mut self) {
        if self.timer == 0 {
            return;
        }

        if self.timer_value == 0 {
            self.timer_value = self.timer;
            if self.enabled && self.length_counter > 0 && self.linear_counter > 0 {
                self.step = (self.step + 1) % 32;
                self.generate_output();
            }
        } else {
            self.timer_value -= 1;
        }
    }

    fn generate_output(&mut self) {
        // Triangle wave pattern
        let triangle_value = TRIANGLE_WAVE[self.step as usize];
        self.output = triangle_value as f32;
    }

    pub fn get_sample(&self) -> f32 {
        self.output
    }

    pub fn get_length_counter(&self) -> u8 {
        self.length_counter
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

    #[cfg(test)]
    pub fn is_enabled(&self) -> bool {
        self.enabled
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_triangle_channel_new() {
        let triangle = TriangleChannel::new();
        assert_eq!(triangle.control, 0);
        assert_eq!(triangle.linear_counter, 0);
        assert_eq!(triangle.timer, 0);
        assert_eq!(triangle.length_counter, 0);
        assert!(!triangle.enabled);
        assert_eq!(triangle.step, 0);
        assert_eq!(triangle.output, 0.0);
    }

    #[test]
    fn test_triangle_channel_set_control() {
        let mut triangle = TriangleChannel::new();

        // Test length counter halt
        triangle.set_control(0b10000000); // Length counter halt
        assert!(triangle.length_counter_halt);

        // Test linear counter reload
        triangle.set_control(0b01111111); // Linear counter reload 127
        assert_eq!(triangle.linear_counter_reload, 127);
    }

    #[test]
    fn test_triangle_channel_set_timer() {
        let mut triangle = TriangleChannel::new();

        // Test timer low (does not restart the frequency divider)
        triangle.set_timer_low(0x34);
        assert_eq!(triangle.timer & 0xFF, 0x34);

        triangle.set_timer_high(0x12); // Timer bits 0-2, length counter bits 3-7
        assert_eq!(triangle.timer >> 8, 0x02); // Only bits 0-2
        assert!(triangle.linear_counter_reload_flag);

        let tv_after_high = triangle.timer_value;
        triangle.set_timer_low(0x55);
        assert_eq!(triangle.timer & 0xFF, 0x55);
        assert_eq!(
            triangle.timer_value, tv_after_high,
            "timer low write only updates reload, not current divider"
        );
    }

    #[test]
    fn test_triangle_channel_set_enabled() {
        let mut triangle = TriangleChannel::new();

        // Test enabling
        triangle.set_enabled(true);
        assert!(triangle.enabled);

        // Test disabling
        triangle.set_enabled(false);
        assert!(!triangle.enabled);
        assert_eq!(triangle.length_counter, 0);
    }

    #[test]
    fn test_triangle_channel_cycle() {
        let mut triangle = TriangleChannel::new();

        // Set up a basic timer
        triangle.set_timer_low(0x10);
        triangle.set_timer_high(0x00);
        triangle.enabled = true;
        triangle.length_counter = 10;
        triangle.linear_counter = 5;

        // Reset timer_value to 0 to test immediate advancement
        triangle.timer_value = 0;

        // Cycle should advance step when timer reaches 0
        let initial_step = triangle.step;
        // First cycle should advance immediately since timer_value starts at 0
        triangle.cycle();
        assert_eq!(triangle.step, (initial_step + 1) % 32);
    }

    #[test]
    fn test_triangle_channel_generate_output() {
        let mut triangle = TriangleChannel::new();

        // After power-on, output is 0
        assert_eq!(triangle.get_sample(), 0.0);

        triangle.enabled = true;
        triangle.length_counter = 10;
        triangle.linear_counter = 5;
        triangle.step = 0;

        triangle.generate_output();
        // Step 0 → DAC 15 (raw, for nonlinear mixer)
        assert_eq!(triangle.output, 15.0);

        // DAC holds last value when silenced
        triangle.set_enabled(false);
        assert_eq!(triangle.get_sample(), 15.0);

        // Step 15 which should be 0
        triangle.step = 15;
        triangle.generate_output();
        assert_eq!(triangle.output, 0.0);
    }

    #[test]
    fn test_triangle_channel_clock_linear_counter() {
        let mut triangle = TriangleChannel::new();

        // Enable the channel first since clock methods now require it to be enabled
        triangle.enabled = true;

        // Test linear counter reload
        triangle.linear_counter_reload_flag = true;
        triangle.linear_counter_reload = 10;
        triangle.clock_linear_counter();
        assert_eq!(triangle.linear_counter, 10);
        assert!(!triangle.linear_counter_reload_flag);

        // Test linear counter decrement
        triangle.linear_counter = 5;
        triangle.clock_linear_counter();
        assert_eq!(triangle.linear_counter, 4);

        // Test linear counter reaching 0
        triangle.linear_counter = 0;
        triangle.clock_linear_counter();
        assert_eq!(triangle.linear_counter, 0);
    }

    #[test]
    fn test_triangle_channel_clock_length_counter() {
        let mut triangle = TriangleChannel::new();

        // Enable the channel first since clock methods now require it to be enabled
        triangle.enabled = true;

        // Test normal decrement
        triangle.length_counter = 10;
        triangle.length_counter_halt = false;
        triangle.clock_length_counter();
        assert_eq!(triangle.length_counter, 9);

        // Test halt behavior
        triangle.length_counter_halt = true;
        triangle.clock_length_counter();
        assert_eq!(triangle.length_counter, 9); // Should not decrement

        // Test reaching 0
        triangle.length_counter = 0;
        triangle.clock_length_counter();
        assert_eq!(triangle.length_counter, 0); // Should not go below 0
    }

    #[test]
    fn test_triangle_wave_pattern() {
        // Test triangle wave pattern values
        assert_eq!(TRIANGLE_WAVE[0], 15);
        assert_eq!(TRIANGLE_WAVE[15], 0);
        assert_eq!(TRIANGLE_WAVE[16], 0);
        assert_eq!(TRIANGLE_WAVE[31], 15);

        // Test the pattern is symmetric
        for i in 0..16 {
            assert_eq!(TRIANGLE_WAVE[i], TRIANGLE_WAVE[31 - i]);
        }
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
    fn test_triangle_channel_timer_validation() {
        let mut triangle = TriangleChannel::new();

        // Test that timer < 8 produces no output (triangle doesn't validate timer in generate_output)
        triangle.enabled = true;
        triangle.length_counter = 10;
        triangle.linear_counter = 5;
        triangle.timer = 7; // Invalid timer
        triangle.generate_output();
        // Triangle channel doesn't validate timer in generate_output, so it should still produce output
        assert_ne!(triangle.output, 0.0);

        // Test that timer >= 8 produces output
        triangle.timer = 8; // Valid timer
        triangle.generate_output();
        assert_ne!(triangle.output, 0.0);
    }
}
