pub struct DmcChannel {
    control: u8,
    direct_load: u8,
    sample_address: u8,
    sample_length: u8,

    timer: u16,
    timer_value: u16,

    enabled: bool,

    // Sample buffer
    sample_buffer: u8,
    sample_buffer_empty: bool,
    bits_remaining: u8,

    // Address and length
    current_address: u16,
    bytes_remaining: u16,

    // Output
    output_level: u8,
    output: f32,

    // IRQ
    irq_enabled: bool,
    irq_pending: bool,
}

impl DmcChannel {
    pub fn new() -> Self {
        Self {
            control: 0,
            direct_load: 0,
            sample_address: 0,
            sample_length: 0,

            timer: 0,
            timer_value: 0,

            enabled: false,

            sample_buffer: 0,
            sample_buffer_empty: true,
            bits_remaining: 0,

            current_address: 0,
            bytes_remaining: 0,

            output_level: 0,
            output: 0.0,

            irq_enabled: false,
            irq_pending: false,
        }
    }

    pub fn set_control(&mut self, value: u8) {
        self.control = value;
        self.irq_enabled = (value >> 7) & 1 != 0;
        self.timer = DMC_PERIODS[(value & 0x0F) as usize];
        self.timer_value = self.timer;
    }

    pub fn set_direct_load(&mut self, value: u8) {
        self.direct_load = value & 0x7F;
        self.output_level = self.direct_load;
    }

    pub fn set_sample_address(&mut self, value: u8) {
        self.sample_address = value;
        self.current_address = 0xC000 | ((value as u16) << 6);
    }

    pub fn set_sample_length(&mut self, value: u8) {
        self.sample_length = value;
        if self.enabled {
            self.bytes_remaining = ((value as u16) << 4) | 1;
        }
        // Removed auto-enable logic
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.bytes_remaining = 0;
            self.irq_pending = false;
        } else if self.bytes_remaining == 0 {
            self.restart_sample();
        }
    }

    pub fn cycle(&mut self) {
        if self.timer_value == 0 {
            self.timer_value = self.timer;
            self.clock_output();
        } else {
            self.timer_value -= 1;
        }

        // Generate output
        self.generate_output();
    }

    fn clock_output(&mut self) {
        if !self.enabled || self.bytes_remaining == 0 {
            return;
        }

        if self.bits_remaining == 0 {
            if self.sample_buffer_empty {
                self.load_sample();
            }

            if !self.sample_buffer_empty {
                self.bits_remaining = 8;
            }
        }

        if self.bits_remaining > 0 {
            let bit = (self.sample_buffer >> 7) & 1;
            self.sample_buffer <<= 1;
            self.bits_remaining -= 1;

            if bit == 1 && self.output_level < 126 {
                self.output_level += 2;
            } else if bit == 0 && self.output_level > 1 {
                self.output_level -= 2;
            }
        }
    }

    fn load_sample(&mut self) {
        // In a real implementation, this would read from memory
        // For now, we'll just simulate it
        if self.bytes_remaining > 0 {
            self.sample_buffer = 0x80; // Default sample value
            self.sample_buffer_empty = false;
            self.bytes_remaining -= 1;
            self.current_address = self.current_address.wrapping_add(1);

            if self.bytes_remaining == 0 {
                if self.irq_enabled {
                    self.irq_pending = true;
                }
                if self.enabled {
                    self.restart_sample();
                }
            }
        }
    }

    fn restart_sample(&mut self) {
        self.current_address = 0xC000 | ((self.sample_address as u16) << 6);
        self.bytes_remaining = ((self.sample_length as u16) << 4) | 1;
    }

    fn generate_output(&mut self) {
        if !self.enabled {
            self.output = 0.0;
            return;
        }

        // Convert 7-bit output level to float
        self.output = (self.output_level as f32 - 64.0) / 64.0;
    }

    pub fn get_sample(&self) -> f32 {
        self.output
    }

    #[allow(dead_code)]
    pub fn get_irq_pending(&self) -> bool {
        self.irq_pending
    }

    #[allow(dead_code)]
    pub fn clear_irq(&mut self) {
        self.irq_pending = false;
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

// DMC period lookup table
const DMC_PERIODS: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
];
