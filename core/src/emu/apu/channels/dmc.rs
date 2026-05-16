use crate::emu::memory::MemoryMapper;
use crate::emu::savestate::{SavestateReader, SavestateWriter};

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

    // Output unit
    shift_register: u8,
    bits_remaining: u8,
    silence: bool,

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

            timer: DMC_PERIODS[0],
            timer_value: DMC_PERIODS[0],

            enabled: false,

            sample_buffer: 0,
            sample_buffer_empty: true,

            shift_register: 0,
            bits_remaining: 0,
            silence: true,

            current_address: 0,
            bytes_remaining: 0,

            output_level: 0,
            output: 0.0,

            irq_enabled: false,
            irq_pending: false,
        }
    }

    pub fn hard_reset(&mut self) {
        self.control = 0;
        self.direct_load = 0;
        self.sample_address = 0;
        self.sample_length = 0;
        self.timer = DMC_PERIODS[0];
        self.timer_value = DMC_PERIODS[0];
        self.enabled = false;
        self.sample_buffer = 0;
        self.sample_buffer_empty = true;
        self.shift_register = 0;
        self.bits_remaining = 0;
        self.silence = true;
        self.current_address = 0;
        self.bytes_remaining = 0;
        self.output_level = 0;
        self.output = 0.0;
        self.irq_enabled = false;
        self.irq_pending = false;
    }

    pub fn save_state(&self, w: &mut SavestateWriter) {
        w.write_u8(self.control);
        w.write_u8(self.direct_load);
        w.write_u8(self.sample_address);
        w.write_u8(self.sample_length);
        w.write_u16(self.timer);
        w.write_u16(self.timer_value);
        w.write_bool(self.enabled);
        w.write_u8(self.sample_buffer);
        w.write_bool(self.sample_buffer_empty);
        w.write_u8(self.shift_register);
        w.write_u8(self.bits_remaining);
        w.write_bool(self.silence);
        w.write_u16(self.current_address);
        w.write_u16(self.bytes_remaining);
        w.write_u8(self.output_level);
        w.write_f32(self.output);
        w.write_bool(self.irq_enabled);
        w.write_bool(self.irq_pending);
    }

    pub fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        self.control = r.read_u8()?;
        self.direct_load = r.read_u8()?;
        self.sample_address = r.read_u8()?;
        self.sample_length = r.read_u8()?;
        self.timer = r.read_u16()?;
        self.timer_value = r.read_u16()?;
        self.enabled = r.read_bool()?;
        self.sample_buffer = r.read_u8()?;
        self.sample_buffer_empty = r.read_bool()?;
        self.shift_register = r.read_u8()?;
        self.bits_remaining = r.read_u8()?;
        self.silence = r.read_bool()?;
        self.current_address = r.read_u16()?;
        self.bytes_remaining = r.read_u16()?;
        self.output_level = r.read_u8()?;
        self.output = r.read_f32()?;
        self.irq_enabled = r.read_bool()?;
        self.irq_pending = r.read_bool()?;
        Ok(())
    }

    pub fn set_control(&mut self, value: u8) {
        self.control = value;
        let irq_enable = (value >> 7) & 1 != 0;
        self.irq_enabled = irq_enable;
        self.timer = DMC_PERIODS[(value & 0x0F) as usize];
        if !irq_enable {
            self.irq_pending = false;
        }
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
        // NES-accurate: do not update bytes_remaining here
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
        // Immediately fill sample buffer if empty and bytes remain
        if self.sample_buffer_empty && self.bytes_remaining > 0 {
            self.load_sample(memory);
        }
        if self.timer_value == 0 {
            self.timer_value = self.timer.wrapping_sub(1);
            self.clock_output();
        } else {
            self.timer_value -= 1;
        }

        // Generate output
        self.generate_output();
    }

    fn clock_output(&mut self) {
        if self.bits_remaining == 0 {
            self.bits_remaining = 8;
            if self.sample_buffer_empty {
                self.silence = true;
            } else {
                self.silence = false;
                self.shift_register = self.sample_buffer;
                self.sample_buffer_empty = true;
            }
        }

        if !self.silence {
            if self.shift_register & 1 == 1 {
                if self.output_level <= 125 {
                    self.output_level += 2;
                }
            } else {
                if self.output_level >= 2 {
                    self.output_level -= 2;
                }
            }
            self.shift_register >>= 1;
        }

        self.bits_remaining -= 1;
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
        if self.current_address == 0x0000 {
            self.current_address = 0x8000;
        }

        if self.bytes_remaining == 0 {
            // Only set IRQ if IRQ is enabled and loop is NOT enabled
            let loop_enabled = (self.control & 0x40) != 0;
            if self.irq_enabled && !loop_enabled {
                self.irq_pending = true;
            }
            // Only restart if loop bit (bit 6) is set
            if self.enabled && loop_enabled {
                self.restart_sample();
            }
        }
    }

    fn restart_sample(&mut self) {
        self.current_address = 0xC000 | ((self.sample_address as u16) << 6);
        self.bytes_remaining = ((self.sample_length as u16) << 4) | 1;
    }

    fn generate_output(&mut self) {
        self.output = self.output_level as f32;
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
        fn controllers(&mut self) -> &mut [crate::emu::io::controller::Controller; 2] {
            panic!()
        }
        fn poll_irq(&mut self) -> bool {
            false
        }
    }

    #[test]
    fn test_dmc_sample_bits_lsb_first_order() {
        let mut dmc = DmcChannel::new();
        dmc.enabled = true;
        dmc.bytes_remaining = 10;
        dmc.bits_remaining = 1;
        dmc.silence = false;
        dmc.shift_register = 0x01;
        dmc.output_level = 64;
        // sample buffer loaded for next cycle
        dmc.sample_buffer = 0x01;
        dmc.sample_buffer_empty = false;
        dmc.clock_output();
        // bit 0 of 0x01 is 1 → +2, then bits_remaining goes to 0, reloads from buffer
        assert_eq!(dmc.output_level, 66);

        dmc.sample_buffer = 0x80;
        dmc.sample_buffer_empty = false;
        dmc.bits_remaining = 1;
        dmc.silence = false;
        dmc.shift_register = 0x80;
        dmc.clock_output();
        // bit 0 of 0x80 is 0 → -2
        assert_eq!(dmc.output_level, 64);
    }

    #[test]
    fn test_dmc_channel_new() {
        let dmc = DmcChannel::new();
        assert_eq!(dmc.control, 0);
        assert_eq!(dmc.direct_load, 0);
        assert_eq!(dmc.sample_address, 0);
        assert_eq!(dmc.sample_length, 0);
        assert_eq!(dmc.timer, DMC_PERIODS[0]);
        assert_eq!(dmc.timer_value, DMC_PERIODS[0]);
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
        // timer_value is NOT reset by set_control (hardware behavior)
        assert_eq!(dmc.timer_value, DMC_PERIODS[0]);
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
        assert_eq!(dmc.bytes_remaining, 0); // Should not change when enabled (NES-accurate)
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

        dmc.set_control(0x01); // Period = DMC_PERIODS[1] = 380
        dmc.enabled = true;
        dmc.bytes_remaining = 10;
        dmc.timer_value = 0; // Force timer to expire on next cycle

        // First cycle: timer_value == 0 → reloads to timer-1=379, calls clock_output
        dmc.cycle(&mut mem);
        assert_eq!(dmc.timer_value, dmc.timer - 1);

        // Full period = timer cycles: 379 decrements + 1 reload
        for _ in 0..dmc.timer {
            dmc.cycle(&mut mem);
        }
        assert_eq!(dmc.timer_value, dmc.timer - 1);
    }

    #[test]
    fn test_dmc_channel_clock_output() {
        let mut dmc = DmcChannel::new();

        // When bits_remaining is 0 and buffer empty → silence, reload to 8, then decrement to 7
        dmc.clock_output();
        assert!(dmc.silence);
        assert_eq!(dmc.bits_remaining, 7);

        // Load a sample into the buffer, then clock when bits_remaining hits 0
        dmc.bytes_remaining = 10;
        dmc.current_address = 0xC001;
        dmc.sample_buffer_empty = true;
        // Drain remaining bits in silence
        for _ in 0..7 {
            dmc.clock_output();
        }
        // Now the cycle() would have filled the buffer; simulate that
        dmc.sample_buffer = 0xAA;
        dmc.sample_buffer_empty = false;
        // bits_remaining is 0 → new output cycle loads buffer into shift register
        dmc.clock_output();
        assert!(!dmc.silence);
        assert_eq!(dmc.shift_register, 0xAA >> 1); // shifted once

        // Test bit processing with known shift register
        dmc.output_level = 64;
        dmc.shift_register = 0x01; // bit 0 = 1
        dmc.silence = false;
        dmc.bits_remaining = 2;
        dmc.clock_output();
        assert_eq!(dmc.output_level, 66); // +2

        dmc.shift_register = 0x02; // bit 0 = 0
        dmc.bits_remaining = 2;
        dmc.clock_output();
        assert_eq!(dmc.output_level, 64); // -2
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

        // Test sample restart when enabled and looping
        dmc.enabled = true;
        dmc.control = 0x40; // Set loop bit
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

        // Output always reflects output_level regardless of enabled state
        dmc.generate_output();
        assert_eq!(dmc.output, 0.0);

        // Raw 7-bit DAC output to mixer (0–127)
        dmc.output_level = 64;
        dmc.generate_output();
        assert_eq!(dmc.output, 64.0);

        // Test maximum level
        dmc.output_level = 126;
        dmc.generate_output();
        assert_eq!(dmc.output, 126.0);

        // Test minimum level
        dmc.output_level = 2;
        dmc.generate_output();
        assert_eq!(dmc.output, 2.0);
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

        // Bit 1: should not increase past 125 → 127
        dmc.output_level = 126;
        dmc.shift_register = 0x01;
        dmc.silence = false;
        dmc.bits_remaining = 2;
        dmc.clock_output();
        assert_eq!(dmc.output_level, 126); // 126 > 125, no change

        dmc.output_level = 125;
        dmc.shift_register = 0x01;
        dmc.silence = false;
        dmc.bits_remaining = 2;
        dmc.clock_output();
        assert_eq!(dmc.output_level, 127); // 125 + 2 = 127

        // Bit 0: should not decrease below 2 → 0
        dmc.output_level = 1;
        dmc.shift_register = 0x00;
        dmc.silence = false;
        dmc.bits_remaining = 2;
        dmc.clock_output();
        assert_eq!(dmc.output_level, 1); // 1 < 2, no change

        dmc.output_level = 2;
        dmc.shift_register = 0x00;
        dmc.silence = false;
        dmc.bits_remaining = 2;
        dmc.clock_output();
        assert_eq!(dmc.output_level, 0); // 2 - 2 = 0
    }

    #[test]
    fn test_dmc_channel_address_wrapping() {
        let mut dmc = DmcChannel::new();

        // Test address wrapping
        dmc.current_address = 0xFFFF;
        dmc.bytes_remaining = 1;
        dmc.load_sample(&mut DummyMemory);
        assert_eq!(dmc.current_address, 0x8000); // Should wrap around to 0x8000 (NES-accurate)
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
            fn controllers(&mut self) -> &mut [crate::emu::io::controller::Controller; 2] {
                panic!()
            }
            fn poll_irq(&mut self) -> bool {
                false
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
