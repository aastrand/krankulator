use crate::emu::memory::MemoryMapper;

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

    pub fn cycle(&mut self, memory: &mut dyn MemoryMapper) {
        if self.timer_value == 0 {
            self.timer_value = self.timer;
            self.clock_output(memory);
        } else {
            self.timer_value -= 1;
        }

        // Generate output
        self.generate_output();
    }

    fn clock_output(&mut self, memory: &mut dyn MemoryMapper) {
        if !self.enabled || self.bytes_remaining == 0 {
            return;
        }

        if self.bits_remaining == 0 {
            if self.sample_buffer_empty {
                self.load_sample(memory);
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

    fn load_sample(&mut self, memory: &mut dyn MemoryMapper) {
        if self.bytes_remaining == 0 {
            return;
        }
        // Real memory read
        let data = memory.cpu_read(self.current_address);
        self.sample_buffer = data;
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

    #[cfg(test)]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_active(&self) -> bool {
        self.bytes_remaining > 0
    }
}

// DMC period lookup table
const DMC_PERIODS: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emu::memory::MemoryMapper;

    struct DummyMemory;
    impl MemoryMapper for DummyMemory {
        fn cpu_read(&mut self, _addr: u16) -> u8 {
            0xAA
        }
        fn cpu_write(&mut self, _addr: u16, _value: u8) {}
        fn ppu_read(&self, _addr: u16) -> u8 {
            0
        }
        fn ppu_copy(&self, _addr: u16, _dest: *mut u8, _size: usize) {}
        fn ppu_write(&mut self, _addr: u16, _value: u8) {}
        fn code_start(&mut self) -> u16 {
            0
        }
        fn ppu(&self) -> std::rc::Rc<std::cell::RefCell<crate::emu::ppu::PPU>> {
            panic!()
        }
        fn apu(&self) -> std::rc::Rc<std::cell::RefCell<crate::emu::apu::APU>> {
            panic!()
        }
        fn controllers(&mut self) -> &mut [crate::emu::io::controller::Controller; 2] {
            panic!()
        }
    }

    #[test]
    fn test_dmc_channel_new() {
        let dmc = DmcChannel::new();
        assert_eq!(dmc.control, 0);
        assert_eq!(dmc.direct_load, 0);
        assert_eq!(dmc.sample_address, 0);
        assert_eq!(dmc.sample_length, 0);
        assert_eq!(dmc.timer, 0);
        assert_eq!(dmc.timer_value, 0);
        assert!(!dmc.enabled);
        assert_eq!(dmc.sample_buffer, 0);
        assert!(dmc.sample_buffer_empty);
        assert_eq!(dmc.bits_remaining, 0);
        assert_eq!(dmc.current_address, 0);
        assert_eq!(dmc.bytes_remaining, 0);
        assert_eq!(dmc.output_level, 0);
        assert_eq!(dmc.output, 0.0);
        assert!(!dmc.irq_enabled);
        assert!(!dmc.irq_pending);
    }

    #[test]
    fn test_dmc_channel_set_control() {
        let mut dmc = DmcChannel::new();

        // Test IRQ enable
        dmc.set_control(0b10000000); // IRQ enable
        assert!(dmc.irq_enabled);

        // Test period setting
        dmc.set_control(0b00001111); // Period 15
        assert_eq!(dmc.timer, DMC_PERIODS[15]);

        // Test that timer value is also set
        assert_eq!(dmc.timer_value, DMC_PERIODS[15]);
    }

    #[test]
    fn test_dmc_channel_set_direct_load() {
        let mut dmc = DmcChannel::new();

        // Test direct load setting
        dmc.set_direct_load(0x7F);
        assert_eq!(dmc.direct_load, 0x7F);
        assert_eq!(dmc.output_level, 0x7F);

        // Test that higher bits are ignored
        dmc.set_direct_load(0xFF);
        assert_eq!(dmc.direct_load, 0x7F);
    }

    #[test]
    fn test_dmc_channel_set_sample_address() {
        let mut dmc = DmcChannel::new();

        // Test sample address setting
        dmc.set_sample_address(0x40);
        assert_eq!(dmc.sample_address, 0x40);
        assert_eq!(dmc.current_address, 0xC000 | (0x40 << 6)); // 0xC000 + (0x40 * 64)

        // Test another address
        dmc.set_sample_address(0x80);
        assert_eq!(dmc.current_address, 0xC000 | (0x80 << 6)); // 0xC000 + (0x80 * 64)
    }

    #[test]
    fn test_dmc_channel_set_sample_length() {
        let mut dmc = DmcChannel::new();

        // Test when disabled
        dmc.set_sample_length(0x10);
        assert_eq!(dmc.sample_length, 0x10);
        assert_eq!(dmc.bytes_remaining, 0); // Should not change when disabled

        // Test when enabled
        dmc.enabled = true;
        dmc.set_sample_length(0x10);
        assert_eq!(dmc.bytes_remaining, (0x10 << 4) | 1); // (0x10 * 16) + 1
    }

    #[test]
    fn test_dmc_channel_set_enabled() {
        let mut dmc = DmcChannel::new();

        // Test enabling
        dmc.set_enabled(true);
        assert!(dmc.enabled);

        // Test disabling
        dmc.set_enabled(false);
        assert!(!dmc.enabled);
        assert_eq!(dmc.bytes_remaining, 0);
        assert!(!dmc.irq_pending);

        // Test enabling with no bytes remaining (should restart)
        dmc.sample_address = 0x40;
        dmc.sample_length = 0x10;
        dmc.set_enabled(true);
        assert_eq!(dmc.bytes_remaining, (0x10 << 4) | 1);
    }

    #[test]
    fn test_dmc_channel_cycle() {
        let mut dmc = DmcChannel::new();
        let mut mem = DummyMemory;

        // Set up a basic timer
        dmc.set_control(0x01); // Period 1
        dmc.enabled = true;
        dmc.bytes_remaining = 10;

        // Cycle should advance timer
        let initial_timer = dmc.timer_value;
        dmc.cycle(&mut mem);
        assert_eq!(dmc.timer_value, initial_timer - 1);

        // Cycle until timer reaches 0
        for _ in 0..initial_timer {
            dmc.cycle(&mut mem);
        }
        assert_eq!(dmc.timer_value, dmc.timer); // Should reset to timer
    }

    #[test]
    fn test_dmc_channel_clock_output() {
        let mut dmc = DmcChannel::new();
        let mut mem = DummyMemory;

        // Test when disabled
        dmc.clock_output(&mut mem);
        assert_eq!(dmc.bits_remaining, 0); // Should not change

        // Test when no bytes remaining
        dmc.enabled = true;
        dmc.bytes_remaining = 0;
        dmc.clock_output(&mut mem);
        assert_eq!(dmc.bits_remaining, 0); // Should not change

        // Test when bits remaining is 0 but sample buffer is empty
        dmc.bytes_remaining = 10;
        dmc.bits_remaining = 0;
        dmc.sample_buffer_empty = true;
        dmc.current_address = 0xC001; // Set to odd address to get 0xFF sample
        dmc.clock_output(&mut mem);
        // Should try to load sample and set bits_remaining to 8, then process a bit (so 7 left)
        assert_eq!(dmc.bits_remaining, 7);
        assert!(!dmc.sample_buffer_empty);

        // Test bit processing
        dmc.sample_buffer = 0x80; // MSB = 1
        dmc.output_level = 64; // Middle level
        dmc.clock_output(&mut mem);
        assert_eq!(dmc.output_level, 66); // Should increase by 2
        assert_eq!(dmc.bits_remaining, 6); // Should be 6 after processing one bit

        // Test bit processing with 0 bit
        dmc.sample_buffer = 0x00; // MSB = 0
        dmc.clock_output(&mut mem);
        assert_eq!(dmc.output_level, 64); // Should decrease by 2
        assert_eq!(dmc.bits_remaining, 5); // Should be 5 after processing another bit
    }

    #[test]
    fn test_dmc_channel_load_sample() {
        let mut dmc = DmcChannel::new();
        let mut mem = DummyMemory;

        // Set up sample parameters
        dmc.sample_address = 0x40;
        dmc.sample_length = 0x10;
        dmc.bytes_remaining = 10;
        dmc.current_address = 0xC000 | (0x40 << 6);

        // Test sample loading
        dmc.load_sample(&mut mem);
        assert_eq!(dmc.bytes_remaining, 9);
        assert!(!dmc.sample_buffer_empty);
        assert_eq!(dmc.current_address, (0xC000 | (0x40 << 6)) + 1);

        // Test reaching end of sample
        dmc.bytes_remaining = 1;
        dmc.irq_enabled = true;
        dmc.load_sample(&mut mem);
        assert_eq!(dmc.bytes_remaining, 0);
        assert!(dmc.irq_pending);

        // Test sample restart when enabled
        dmc.enabled = true;
        dmc.bytes_remaining = 1; // Set to 1 so load_sample will trigger restart
        dmc.load_sample(&mut mem);
        assert_eq!(dmc.bytes_remaining, ((0x10 as u16) << 4) | 1); // Should restart
    }

    #[test]
    fn test_dmc_channel_restart_sample() {
        let mut dmc = DmcChannel::new();

        // Set up sample parameters
        dmc.sample_address = 0x40;
        dmc.sample_length = 0x10;

        // Test restart
        dmc.restart_sample();
        assert_eq!(dmc.current_address, 0xC000 | (0x40 << 6));
        assert_eq!(dmc.bytes_remaining, (0x10 << 4) | 1);
    }

    #[test]
    fn test_dmc_channel_generate_output() {
        let mut dmc = DmcChannel::new();

        // Test disabled channel
        dmc.generate_output();
        assert_eq!(dmc.output, 0.0);

        // Test enabled channel
        dmc.enabled = true;
        dmc.output_level = 64; // Middle level
        dmc.generate_output();
        assert_eq!(dmc.output, 0.0); // (64 - 64) / 64 = 0.0

        // Test maximum level
        dmc.output_level = 126;
        dmc.generate_output();
        assert_eq!(dmc.output, (126.0 - 64.0) / 64.0);

        // Test minimum level
        dmc.output_level = 2;
        dmc.generate_output();
        assert_eq!(dmc.output, (2.0 - 64.0) / 64.0);
    }

    #[test]
    fn test_dmc_channel_irq_handling() {
        let mut dmc = DmcChannel::new();

        // Test IRQ pending
        dmc.irq_pending = true;
        assert!(dmc.get_irq_pending());

        // Test clear IRQ
        dmc.clear_irq();
        assert!(!dmc.get_irq_pending());
        assert!(!dmc.irq_pending);
    }

    #[test]
    fn test_dmc_periods_table() {
        // Test some known values from the table
        assert_eq!(DMC_PERIODS[0], 428);
        assert_eq!(DMC_PERIODS[1], 380);
        assert_eq!(DMC_PERIODS[15], 54);

        // Test that periods decrease with index (higher frequency)
        for i in 1..16 {
            assert!(DMC_PERIODS[i] < DMC_PERIODS[i - 1]);
        }
    }

    #[test]
    fn test_dmc_channel_output_level_bounds() {
        let mut dmc = DmcChannel::new();
        dmc.enabled = true;
        dmc.bytes_remaining = 10;
        dmc.bits_remaining = 8;

        // Test output level upper bound
        dmc.output_level = 126;
        dmc.sample_buffer = 0x80; // MSB = 1
        dmc.clock_output(&mut DummyMemory);
        assert_eq!(dmc.output_level, 126); // Should not exceed 126

        // Test output level lower bound
        dmc.output_level = 1;
        dmc.sample_buffer = 0x00; // MSB = 0
        dmc.clock_output(&mut DummyMemory);
        assert_eq!(dmc.output_level, 1); // Should not go below 1
    }

    #[test]
    fn test_dmc_channel_address_wrapping() {
        let mut dmc = DmcChannel::new();

        // Test address wrapping
        dmc.current_address = 0xFFFF;
        dmc.bytes_remaining = 1;
        dmc.load_sample(&mut DummyMemory);
        assert_eq!(dmc.current_address, 0x0000); // Should wrap around
    }

    #[test]
    fn test_dmc_channel_sample_buffer_pattern() {
        let mut dmc = DmcChannel::new();
        // Custom dummy memory to simulate the old pattern
        struct DummyPatternMemory;
        impl MemoryMapper for DummyPatternMemory {
            fn cpu_read(&mut self, addr: u16) -> u8 {
                if addr & 0x01 != 0 {
                    0xFF
                } else {
                    0x00
                }
            }
            fn cpu_write(&mut self, _addr: u16, _value: u8) {}
            fn ppu_read(&self, _addr: u16) -> u8 {
                0
            }
            fn ppu_copy(&self, _addr: u16, _dest: *mut u8, _size: usize) {}
            fn ppu_write(&mut self, _addr: u16, _value: u8) {}
            fn code_start(&mut self) -> u16 {
                0
            }
            fn ppu(&self) -> std::rc::Rc<std::cell::RefCell<crate::emu::ppu::PPU>> {
                panic!()
            }
            fn apu(&self) -> std::rc::Rc<std::cell::RefCell<crate::emu::apu::APU>> {
                panic!()
            }
            fn controllers(&mut self) -> &mut [crate::emu::io::controller::Controller; 2] {
                panic!()
            }
        }
        let mut mem = DummyPatternMemory;
        dmc.current_address = 0xC001; // Odd address
        dmc.bytes_remaining = 1;
        dmc.load_sample(&mut mem);
        assert_eq!(dmc.sample_buffer, 0xFF); // Should be 0xFF for odd addresses

        dmc.current_address = 0xC000; // Even address
        dmc.bytes_remaining = 1;
        dmc.load_sample(&mut mem);
        assert_eq!(dmc.sample_buffer, 0x00); // Should be 0x00 for even addresses
    }
}
