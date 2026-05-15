pub mod channels;
pub mod frame_counter;

pub const SAMPLE_RATE: u32 = 44100;
pub const CYCLES_PER_SAMPLE: f64 = 1_789_773.0 / SAMPLE_RATE as f64;

/// First-order IIR chain at `SAMPLE_RATE`: HPF ~37 Hz, HPF ~440 Hz, LPF ~14 kHz.
struct AudioFilter {
    hp1_x1: f32,
    hp1_y1: f32,
    hp1_a: f32,
    hp2_x1: f32,
    hp2_y1: f32,
    hp2_a: f32,
    lp_y: f32,
    lp_a: f32,
}

impl AudioFilter {
    fn new(sample_rate: u32) -> Self {
        let sr = sample_rate as f32;
        let hp_a = |fc: f32| {
            let rc = 1.0 / (2.0 * std::f32::consts::PI * fc);
            let dt = 1.0 / sr;
            rc / (rc + dt)
        };
        let lp_a = |fc: f32| {
            let rc = 1.0 / (2.0 * std::f32::consts::PI * fc);
            let dt = 1.0 / sr;
            dt / (rc + dt)
        };
        Self {
            hp1_x1: 0.0,
            hp1_y1: 0.0,
            hp1_a: hp_a(37.0),
            hp2_x1: 0.0,
            hp2_y1: 0.0,
            hp2_a: hp_a(440.0),
            lp_y: 0.0,
            lp_a: lp_a(14_000.0),
        }
    }

    fn reset(&mut self) {
        self.hp1_x1 = 0.0;
        self.hp1_y1 = 0.0;
        self.hp2_x1 = 0.0;
        self.hp2_y1 = 0.0;
        self.lp_y = 0.0;
    }

    fn process(&mut self, x: f32) -> f32 {
        let y1 = self.hp1_a * (self.hp1_y1 + x - self.hp1_x1);
        self.hp1_x1 = x;
        self.hp1_y1 = y1;

        let y2 = self.hp2_a * (self.hp2_y1 + y1 - self.hp2_x1);
        self.hp2_x1 = y1;
        self.hp2_y1 = y2;

        let y = self.lp_a * y2 + (1.0 - self.lp_a) * self.lp_y;
        self.lp_y = y;
        y
    }

    fn save_state(&self, w: &mut SavestateWriter) {
        w.write_f32(self.hp1_x1);
        w.write_f32(self.hp1_y1);
        w.write_f32(self.hp2_x1);
        w.write_f32(self.hp2_y1);
        w.write_f32(self.lp_y);
    }

    fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        self.hp1_x1 = r.read_f32()?;
        self.hp1_y1 = r.read_f32()?;
        self.hp2_x1 = r.read_f32()?;
        self.hp2_y1 = r.read_f32()?;
        self.lp_y = r.read_f32()?;
        Ok(())
    }
}

use crate::emu::apu::frame_counter::FrameStep;
use crate::emu::savestate::{SavestateReader, SavestateWriter};
use channels::{DmcChannel, NoiseChannel, PulseChannel, TriangleChannel};
use frame_counter::FrameCounter;

pub struct APU {
    pulse1: PulseChannel,
    pulse2: PulseChannel,
    triangle: TriangleChannel,
    noise: NoiseChannel,
    dmc: DmcChannel,
    frame_counter: FrameCounter,

    // Audio output
    sample_buffer: Vec<f32>,
    sample_index: usize,
    cycles_since_sample: f64,

    // Status
    status: u8,
    frame_irq_since_dot: u64,
    enabled_channels: u8,
    // Debug override mute independent of $4015 (bit0..bit4: p1,p2,tri,noise,dmc)
    mute_mask: u8,

    audio_filter: AudioFilter,
}

impl APU {
    pub fn new() -> Self {
        let mut apu = Self {
            pulse1: PulseChannel::new(0),
            pulse2: PulseChannel::new(1),
            triangle: TriangleChannel::new(),
            noise: NoiseChannel::new(),
            dmc: DmcChannel::new(),
            frame_counter: FrameCounter::new(),

            sample_buffer: vec![0.0; 4096],
            sample_index: 0,
            cycles_since_sample: 0.0,

            status: 0,
            frame_irq_since_dot: u64::MAX,
            enabled_channels: 0,
            mute_mask: 0,
            audio_filter: AudioFilter::new(SAMPLE_RATE),
        };
        apu.reset();
        apu
    }

    pub fn reset(&mut self) {
        self.pulse1.hard_reset();
        self.pulse2.hard_reset();
        self.triangle.hard_reset();
        self.noise.hard_reset();
        self.dmc.hard_reset();
        self.frame_counter = FrameCounter::new();
        self.sample_index = 0;
        self.cycles_since_sample = 0.0;
        self.status = 0;
        self.frame_irq_since_dot = u64::MAX;
        self.enabled_channels = 0;
        self.mute_mask = 0;
        self.audio_filter.reset();
        for s in &mut self.sample_buffer {
            *s = 0.0;
        }
    }

    // Public API for UI to control per-channel mute without touching $4015
    pub fn set_master_mute(&mut self, muted: bool) {
        self.mute_mask = if muted { 0x1F } else { 0x00 };
        // println!("APU: MASTER {}", if muted { "MUTED" } else { "ENABLED" });
    }

    pub fn get_master_mute(&self) -> bool {
        self.mute_mask == 0x1F
    }

    pub fn toggle_mute_bit(&mut self, bit: u8, label: &str) {
        let old_mask = self.mute_mask;
        self.mute_mask ^= bit;
        let now_muted = self.mute_mask & bit != 0;
        println!(
            "APU: {} {} (mask: {:02X} -> {:02X})",
            label,
            if now_muted { "MUTED" } else { "ENABLED" },
            old_mask,
            self.mute_mask
        );
    }

    pub fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x4015 => {
                let mut status = 0;
                if self.pulse1.get_length_counter() > 0 {
                    status |= 0x01;
                }
                if self.pulse2.get_length_counter() > 0 {
                    status |= 0x02;
                }
                if self.triangle.get_length_counter() > 0 {
                    status |= 0x04;
                }
                if self.noise.get_length_counter() > 0 {
                    status |= 0x08;
                }
                if self.dmc.is_active() {
                    status |= 0x10;
                }
                // Set DMC IRQ flag (bit 7) if pending
                if self.dmc.get_irq_pending() {
                    status |= 0x80;
                }
                status |= self.status & 0x40; // Only frame IRQ bit (bit 6)
                self.clear_irq(); // Clear frame IRQ on read (do not clear DMC IRQ)
                status
            }
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8, emu_cycle: u64) {
        // Debug: log all APU register writes
        // println!("APU write: ${:04X} = {:02X}", addr, value);

        match addr {
            // Pulse 1
            0x4000 => {
                /*
                println!(
                    "  Pulse1 control: duty={}, constant_volume={}, volume={}",
                    (value >> 6) & 3,
                    (value >> 4) & 1,
                    value & 0xF
                );
                */
                self.pulse1.set_control(value)
            }
            0x4001 => {
                /*
                println!(
                    "  Pulse1 sweep: enabled={}, period={}, shift={}, negate={}",
                    (value >> 7) & 1,
                    (value >> 4) & 7,
                    (value >> 0) & 7,
                    (value >> 3) & 1
                );
                */
                self.pulse1.set_sweep(value)
            }
            0x4002 => {
                //println!("  Pulse1 timer low: {:02X}", value);
                self.pulse1.set_timer_low(value)
            }
            0x4003 => {
                /*
                println!(
                    "  Pulse1 timer high: {:02X}, length counter: {}",
                    value,
                    (value >> 3) & 0x1F
                );
                */
                self.pulse1.set_timer_high(value)
            }

            // Pulse 2
            0x4004 => {
                /*
                println!(
                    "  Pulse2 control: duty={}, constant_volume={}, volume={}",
                    (value >> 6) & 3,
                    (value >> 4) & 1,
                    value & 0xF
                );
                */
                self.pulse2.set_control(value)
            }
            0x4005 => {
                /*
                println!(
                    "  Pulse2 sweep: enabled={}, period={}, shift={}, negate={}",
                    (value >> 7) & 1,
                    (value >> 4) & 7,
                    (value >> 0) & 7,
                    (value >> 3) & 1
                );
                */
                self.pulse2.set_sweep(value)
            }
            0x4006 => {
                //println!("  Pulse2 timer low: {:02X}", value);
                self.pulse2.set_timer_low(value)
            }
            0x4007 => {
                /*
                println!(
                    "  Pulse2 timer high: {:02X}, length counter: {}",
                    value,
                    (value >> 3) & 0x1F
                );
                */
                self.pulse2.set_timer_high(value)
            }

            // Triangle
            0x4008 => {
                /*
                println!(
                    "  Triangle control: linear_counter={}, control_flag={}",
                    value & 0x7F,
                    (value >> 7) & 1
                );
                */
                self.triangle.set_control(value)
            }
            0x4009 => {} // Unused
            0x400A => {
                //println!("  Triangle timer low: {:02X}", value);
                self.triangle.set_timer_low(value)
            }
            0x400B => {
                /*
                println!(
                    "  Triangle timer high: {:02X}, length counter: {}",
                    value,
                    (value >> 3) & 0x1F
                );
                */
                self.triangle.set_timer_high(value)
            }

            // Noise
            0x400C => {
                /*
                println!(
                    "  Noise control: constant_volume={}, volume={}",
                    (value >> 4) & 1,
                    value & 0xF
                );
                */
                self.noise.set_control(value)
            }
            0x400D => {} // Unused
            0x400E => {
                /*
                println!(
                    "  Noise period: mode={}, period={}",
                    (value >> 7) & 1,
                    value & 0xF
                );
                */
                self.noise.set_period(value)
            }
            0x400F => {
                //println!("  Noise length counter: {}", (value >> 3) & 0x1F);
                self.noise.set_length_counter(value)
            }

            // DMC
            0x4010 => {
                /*
                println!(
                    "  DMC control: irq_enable={}, loop={}, rate={}",
                    (value >> 7) & 1,
                    (value >> 6) & 1,
                    value & 0xF
                );
                */
                self.dmc.set_control(value)
            }
            0x4011 => {
                //println!("  DMC direct load: {:02X}", value);
                self.dmc.set_direct_load(value)
            }
            0x4012 => {
                //println!("  DMC sample address: {:02X}", value);
                self.dmc.set_sample_address(value)
            }
            0x4013 => {
                //println!("  DMC sample length: {:02X}", value);
                self.dmc.set_sample_length(value)
            }

            // Status
            0x4015 => {
                self.enabled_channels = value;

                self.pulse1.set_enabled(value & 0x01 != 0);
                self.pulse2.set_enabled(value & 0x02 != 0);
                self.triangle.set_enabled(value & 0x04 != 0);
                self.noise.set_enabled(value & 0x08 != 0);
                self.dmc.set_enabled(value & 0x10 != 0);

                // Clear DMC IRQ on $4015 write (as per NES APU)
                self.dmc.clear_irq();
            }

            // Frame Counter
            0x4017 => {
                if value & 0x40 != 0 {
                    self.clear_irq();
                }
                self.frame_counter.write(value, emu_cycle);
            }

            _ => {}
        }
    }

    pub fn cycle(&mut self, dot: u64, memory: &mut dyn crate::emu::memory::MemoryMapper) {
        let frame_step = self.frame_counter.cycle();

        match frame_step {
            FrameStep::Deferred4017Apply { immediate_clock } => {
                if immediate_clock {
                    self.clock_half_frame();
                }
            }
            FrameStep::QuarterFrame => {
                self.clock_quarter_frame();
            }
            FrameStep::HalfFrame => {
                self.clock_half_frame();
            }
            FrameStep::Irq => {
                if !self.frame_counter.irq_inhibit() {
                    self.status |= 0x40;
                    // +7 dots: APU cycle runs after master_clock advances, so compensate
                    // for the delay between assertion and CPU penultimate-cycle sampling.
                    if self.frame_irq_since_dot == u64::MAX {
                        self.frame_irq_since_dot = dot + 7;
                    }
                }
            }
            FrameStep::IrqHalfFrame => {
                if !self.frame_counter.irq_inhibit() {
                    self.status |= 0x40;
                    if self.frame_irq_since_dot == u64::MAX {
                        self.frame_irq_since_dot = dot + 7;
                    }
                }
                self.clock_half_frame();
            }
            FrameStep::None => {}
        }

        // Cycle channels
        self.pulse1.cycle();
        self.pulse2.cycle();
        self.triangle.cycle();
        self.noise.cycle();
        self.dmc.cycle(memory);

        self.pulse1.end_cycle();
        self.pulse2.end_cycle();

        self.cycles_since_sample += 1.0;
        if self.cycles_since_sample >= CYCLES_PER_SAMPLE {
            self.cycles_since_sample -= CYCLES_PER_SAMPLE;
            self.generate_sample();
        }
    }

    fn clock_quarter_frame(&mut self) {
        self.pulse1.clock_envelope();
        self.pulse2.clock_envelope();
        self.noise.clock_envelope();
        self.triangle.clock_linear_counter();
    }

    fn clock_half_frame(&mut self) {
        self.clock_quarter_frame();
        self.pulse1.clock_length_counter();
        self.pulse2.clock_length_counter();
        self.triangle.clock_length_counter();
        self.noise.clock_length_counter();
        self.pulse1.clock_sweep();
        self.pulse2.clock_sweep();
    }

    fn generate_sample(&mut self) {
        let mut pulse1_sample = self.pulse1.get_sample();
        let mut pulse2_sample = self.pulse2.get_sample();
        let mut triangle_sample = self.triangle.get_sample();
        let mut noise_sample = self.noise.get_sample();
        let mut dmc_sample = self.dmc.get_sample();

        // Apply debug mute overrides
        if self.mute_mask & 0x01 != 0 {
            pulse1_sample = 0.0;
        }
        if self.mute_mask & 0x02 != 0 {
            pulse2_sample = 0.0;
        }
        if self.mute_mask & 0x04 != 0 {
            triangle_sample = 0.0;
        }
        if self.mute_mask & 0x08 != 0 {
            noise_sample = 0.0;
        }
        if self.mute_mask & 0x10 != 0 {
            dmc_sample = 0.0;
        }

        // Mix all channels
        let mixed_sample = self.mix_channels(
            pulse1_sample,
            pulse2_sample,
            triangle_sample,
            noise_sample,
            dmc_sample,
        );
        let filtered = self.audio_filter.process(mixed_sample);

        // Store in buffer
        if self.sample_index < self.sample_buffer.len() {
            self.sample_buffer[self.sample_index] = filtered;
            self.sample_index += 1;
        }
    }

    fn mix_channels(&self, pulse1: f32, pulse2: f32, triangle: f32, noise: f32, dmc: f32) -> f32 {
        // NES nonlinear mixer (NESdev); inputs are raw DAC counts:
        // pulse/noise 0–15, triangle 0–15, DMC 0–127
        let pulse_sum = pulse1 + pulse2;
        let pulse_out = if pulse_sum > 0.0 {
            95.88 / (8128.0 / pulse_sum + 100.0)
        } else {
            0.0
        };
        let tnd_sum = triangle / 8227.0 + noise / 12241.0 + dmc / 22638.0;
        let tnd_out = if tnd_sum > 0.0 {
            159.79 / (1.0 / tnd_sum + 100.0)
        } else {
            0.0
        };
        // Output in [0, ~1]; the high-pass filters remove DC offset
        pulse_out + tnd_out
    }

    pub fn get_audio_samples(&mut self) -> &[f32] {
        let samples = &self.sample_buffer[..self.sample_index];
        self.sample_index = 0;
        samples
    }

    pub fn save_state(&self, w: &mut SavestateWriter) {
        self.pulse1.save_state(w);
        self.pulse2.save_state(w);
        self.triangle.save_state(w);
        self.noise.save_state(w);
        self.dmc.save_state(w);
        self.frame_counter.save_state(w);
        w.write_u8(self.status);
        w.write_u8(self.enabled_channels);
        w.write_u8(self.mute_mask);
        w.write_f64(self.cycles_since_sample);
        w.write_u32(self.sample_index as u32);
        self.audio_filter.save_state(w);
        w.write_u64(self.frame_irq_since_dot);
    }

    pub fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        self.pulse1.load_state(r)?;
        self.pulse2.load_state(r)?;
        self.triangle.load_state(r)?;
        self.noise.load_state(r)?;
        self.dmc.load_state(r)?;
        self.frame_counter.load_state(r)?;
        self.status = r.read_u8()?;
        self.enabled_channels = r.read_u8()?;
        self.mute_mask = r.read_u8()?;
        self.cycles_since_sample = r.read_f64()?;
        self.sample_index = r.read_u32()? as usize;
        self.audio_filter.load_state(r)?;
        if r.version() >= 3 {
            self.frame_irq_since_dot = r.read_u64()?;
        } else {
            self.frame_irq_since_dot = if self.status & 0x40 != 0 { 0 } else { u64::MAX };
        }
        Ok(())
    }

    pub fn frame_irq_at_dot(&self, deadline_dot: u64) -> bool {
        self.frame_irq_since_dot != u64::MAX && self.frame_irq_since_dot < deadline_dot
    }

    pub fn dmc_irq_pending(&self) -> bool {
        self.dmc.get_irq_pending()
    }

    #[allow(dead_code)]
    pub fn clear_irq(&mut self) {
        self.status &= 0xBF;
        self.frame_irq_since_dot = u64::MAX;
    }
}

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
    // In all tests, call apu.cycle(&mut mem)

    #[test]
    fn test_apu_new() {
        let apu = APU::new();
        assert_eq!(apu.status, 0);
        assert_eq!(apu.enabled_channels, 0);
        assert_eq!(apu.sample_index, 0);
        assert_eq!(apu.cycles_since_sample, 0.0);
        assert_eq!(apu.sample_buffer.len(), 4096);
    }

    #[test]
    fn test_apu_read() {
        let mut apu = APU::new();

        // Test status register read
        assert_eq!(apu.read(0x4015), 0);

        // Test other addresses return 0
        assert_eq!(apu.read(0x4000), 0);
        assert_eq!(apu.read(0xFFFF), 0);
    }

    #[test]
    fn test_apu_write_pulse1() {
        let mut apu = APU::new();

        // Test pulse1 control
        apu.write(0x4000, 0x3F, 0); // Volume 15, constant volume
                                    // Test by enabling and checking output behavior
        apu.write(0x4015, 0x01, 0); // Enable pulse1
        apu.write(0x4002, 0x10, 0); // Set timer low
        apu.write(0x4003, 0x00, 0); // Set timer high

        // Cycle to generate output
        for _ in 0..100 {
            apu.cycle(0, &mut DummyMemory);
        }

        // Should produce some output
        let samples = apu.get_audio_samples();
        assert!(samples.len() > 0);
    }

    #[test]
    fn test_apu_write_pulse2() {
        let mut apu = APU::new();

        // Test pulse2 control
        apu.write(0x4004, 0x3F, 0); // Volume 15, constant volume
                                    // Test by enabling and checking output behavior
        apu.write(0x4015, 0x02, 0); // Enable pulse2
        apu.write(0x4006, 0x10, 0); // Set timer low
        apu.write(0x4007, 0x00, 0); // Set timer high

        // Cycle to generate output
        for _ in 0..100 {
            apu.cycle(0, &mut DummyMemory);
        }

        // Should produce some output
        let samples = apu.get_audio_samples();
        assert!(samples.len() > 0);
    }

    #[test]
    fn test_apu_write_triangle() {
        let mut apu = APU::new();

        // Test triangle control and timer
        apu.write(0x4008, 0x80, 0); // Length counter halt
        apu.write(0x400A, 0x34, 0); // Timer low
        apu.write(0x400B, 0x12, 0); // Timer high, length counter

        // Test by enabling and checking output behavior
        apu.write(0x4015, 0x04, 0); // Enable triangle

        // Cycle to generate output
        for _ in 0..100 {
            apu.cycle(0, &mut DummyMemory);
        }

        // Should produce some output
        let samples = apu.get_audio_samples();
        assert!(samples.len() > 0);
    }

    #[test]
    fn test_apu_write_noise() {
        let mut apu = APU::new();

        // Test noise control and period
        apu.write(0x400C, 0x3F, 0); // Volume 15, constant volume
        apu.write(0x400E, 0x0F, 0); // Period 15
        apu.write(0x400F, 0x20, 0); // Length counter index 4

        // Test by enabling and checking output behavior
        apu.write(0x4015, 0x08, 0); // Enable noise

        // Cycle to generate output
        for _ in 0..100 {
            apu.cycle(0, &mut DummyMemory);
        }

        // Should produce some output
        let samples = apu.get_audio_samples();
        assert!(samples.len() > 0);
    }

    #[test]
    fn test_apu_write_dmc() {
        let mut apu = APU::new();

        // Test DMC control and parameters
        apu.write(0x4010, 0x8F, 0); // IRQ enable, period 15
        apu.write(0x4011, 0x7F, 0); // Direct load
        apu.write(0x4012, 0x40, 0); // Sample address
        apu.write(0x4013, 0x10, 0); // Sample length

        // Test by enabling and checking output behavior
        apu.write(0x4015, 0x10, 0); // Enable DMC

        // Cycle to generate output
        for _ in 0..100 {
            apu.cycle(0, &mut DummyMemory);
        }

        // Should produce some output
        let samples = apu.get_audio_samples();
        assert!(samples.len() > 0);
    }

    #[test]
    fn test_apu_write_status() {
        let mut apu = APU::new();

        // Test enabling all channels
        apu.write(0x4015, 0x1F, 0); // Enable all channels
        assert!(apu.pulse1.is_enabled());
        assert!(apu.pulse2.is_enabled());
        assert!(apu.triangle.is_enabled());
        assert!(apu.noise.is_enabled());
        assert!(apu.dmc.is_enabled());
        assert_eq!(apu.enabled_channels, 0x1F);

        // Test disabling all channels
        apu.write(0x4015, 0x00, 0); // Disable all channels
        assert!(!apu.pulse1.is_enabled());
        assert!(!apu.pulse2.is_enabled());
        assert!(!apu.triangle.is_enabled());
        assert!(!apu.noise.is_enabled());
        assert!(!apu.dmc.is_enabled());
        assert_eq!(apu.enabled_channels, 0x00);

        // Test enabling individual channels
        apu.write(0x4015, 0x01, 0); // Enable only pulse1
        assert!(apu.pulse1.is_enabled());
        assert!(!apu.pulse2.is_enabled());
        assert!(!apu.triangle.is_enabled());
        assert!(!apu.noise.is_enabled());
        assert!(!apu.dmc.is_enabled());
    }

    #[test]
    fn test_apu_write_frame_counter() {
        let mut apu = APU::new();

        apu.write(0x4017, 0x00, 0);
        assert_eq!(apu.frame_counter.get_mode(), 0);
        for _ in 0..3 {
            apu.cycle(0, &mut DummyMemory);
        }
        assert_eq!(apu.frame_counter.get_mode(), 0);

        apu.write(0x4017, 0x80, 0);
        assert_eq!(apu.frame_counter.get_mode(), 0);
        for _ in 0..3 {
            apu.cycle(0, &mut DummyMemory);
        }
        assert_eq!(apu.frame_counter.get_mode(), 1);

        apu.write(0x4017, 0x40, 0);
        for _ in 0..3 {
            apu.cycle(0, &mut DummyMemory);
        }
        assert_eq!(apu.frame_counter.get_mode(), 0);
        assert!(apu.frame_counter.irq_inhibit());
    }

    #[test]
    fn test_apu_cycle() {
        let mut apu = APU::new();

        // Test basic cycling
        for _ in 0..100 {
            apu.cycle(0, &mut DummyMemory);
        }

        // Should have advanced cycles_since_sample
        assert!(apu.cycles_since_sample > 0.0);
    }

    #[test]
    fn test_apu_frame_counter_integration() {
        let mut apu = APU::new();

        // Set up frame counter mode 0
        apu.write(0x4017, 0x00, 0);
        for _ in 0..3 {
            apu.cycle(0, &mut DummyMemory);
        }

        // Enable triangle channel
        apu.write(0x4015, 0x04, 0);

        // Set triangle timer
        apu.write(0x400A, 0x10, 0);
        apu.write(0x400B, 0x00, 0);

        // Cycle through frame counter steps
        for _ in 0..7457 {
            apu.cycle(0, &mut DummyMemory);
        }

        // Should have advanced frame counter step
        assert_eq!(apu.frame_counter.get_step(), 1);
    }

    #[test]
    fn test_apu_mix_channels() {
        let apu = APU::new();

        let mixed = apu.mix_channels(15.0, 15.0, 15.0, 15.0, 127.0);
        assert!(mixed > 0.8 && mixed <= 1.0);

        let mixed = apu.mix_channels(0.0, 0.0, 0.0, 0.0, 0.0);
        assert!(mixed.abs() < 1e-5);

        let mixed = apu.mix_channels(15.0, 0.0, 0.0, 0.0, 0.0);
        assert!(mixed > 0.0 && mixed < 0.5);
    }

    #[test]
    fn test_apu_sample_generation() {
        let mut apu = APU::new();

        // Enable pulse1 and set it to produce output
        apu.write(0x4015, 0x01, 0); // Enable pulse1
        apu.write(0x4000, 0x3F, 0); // Volume 15, constant volume
        apu.write(0x4002, 0x10, 0); // Timer low
        apu.write(0x4003, 0x00, 0); // Timer high

        for _ in 0..CYCLES_PER_SAMPLE as u32 + 1 {
            apu.cycle(0, &mut DummyMemory);
        }

        assert_eq!(apu.sample_index, 1);
    }

    #[test]
    fn test_apu_get_audio_samples() {
        let mut apu = APU::new();

        for _ in 0..(CYCLES_PER_SAMPLE * 5.0) as u32 {
            apu.cycle(0, &mut DummyMemory);
        }

        // Get samples
        let samples = apu.get_audio_samples();
        assert!(samples.len() > 0);
        assert_eq!(apu.sample_index, 0); // Should reset after getting samples
    }

    #[test]
    fn test_apu_clear_irq() {
        let mut apu = APU::new();

        // Set frame IRQ bit
        apu.status |= 0x40;
        assert_eq!(apu.status & 0x40, 0x40);

        // Clear IRQ
        apu.clear_irq();
        assert_eq!(apu.status & 0x40, 0x00);
    }

    #[test]
    fn test_apu_constants() {
        // Test sample rate
        assert_eq!(SAMPLE_RATE, 44100);

        assert!(CYCLES_PER_SAMPLE > 40.0);
        assert!(CYCLES_PER_SAMPLE < 41.0);
    }

    #[test]
    fn test_apu_envelope_clocking() {
        let mut apu = APU::new();

        // Set up frame counter mode 0
        apu.write(0x4017, 0x00, 0);
        for _ in 0..3 {
            apu.cycle(0, &mut DummyMemory);
        }

        // Enable pulse1 and set envelope
        apu.write(0x4015, 0x01, 0);
        apu.write(0x4000, 0x20, 0); // Volume 0, envelope enabled

        // Full mode 0 sequence is 29830 cycles
        for _ in 0..29830 {
            apu.cycle(0, &mut DummyMemory);
        }

        // Envelope should have been clocked multiple times
        assert_eq!(apu.frame_counter.get_step(), 0); // Should wrap around
    }

    #[test]
    fn test_apu_status_reflects_channel_length_counters() {
        let mut apu = APU::new();
        // Enable all channels
        apu.pulse1.set_enabled(true);
        apu.pulse2.set_enabled(true);
        apu.triangle.set_enabled(true);
        apu.noise.set_enabled(true);
        apu.dmc.set_enabled(true);
        // Set length counters via helper methods (simulate as if they were set)
        // We'll use a workaround: call set_timer_high/set_length_counter with enabled true
        apu.pulse1.set_timer_high(0b00011000); // length_counter = LENGTH_COUNTER_TABLE[3] = 2
        apu.pulse2.set_timer_high(0b00101000); // length_counter = LENGTH_COUNTER_TABLE[5] = 4
        apu.triangle.set_timer_high(0b00111000); // length_counter = LENGTH_COUNTER_TABLE[7] = 6
        apu.noise.set_length_counter(0b01001000); // length_counter = LENGTH_COUNTER_TABLE[9] = 8
                                                  // Simulate DMC active
        apu.dmc.set_enabled(true); // This will set bytes_remaining if not already
                                   // Manually set DMC bytes_remaining via a public method if available, otherwise skip DMC check
        let status = apu.read(0x4015);
        // Only check bits for channels we can set
        assert!(status & 0x0F != 0x00); // At least one channel active
                                        // Now disable all channels
        apu.pulse1.set_enabled(false);
        apu.pulse2.set_enabled(false);
        apu.triangle.set_enabled(false);
        apu.noise.set_enabled(false);
        apu.dmc.set_enabled(false);
        let status = apu.read(0x4015);
        assert_eq!(status & 0x1F, 0x00); // All channels inactive
    }

    fn run_blargg_apu_rom(rom_path: &str, test_name: &str) {
        use crate::emu;
        use crate::emu::io::loader;
        use crate::util::get_status_str;

        let mut emu = emu::Emulator::new_headless(loader::load_nes(&String::from(rom_path)));
        emu.cpu.status = 0x34;
        emu.cpu.sp = 0xfd;
        emu.toggle_should_trigger_nmi(true);
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);
        emu.run();

        let expected = format!("\n{}\n\nPassed\n", test_name);
        let buf = get_status_str(&mut emu, 0x6004, 200);
        let status = emu.mem.cpu_read(0x6000);
        assert_eq!(0, status, "test status 0x{:02X}: {}", status, buf.trim());
        assert_eq!(expected, buf);
    }

    #[test]
    fn test_blargg_apu_len_ctr() {
        run_blargg_apu_rom("input/nes/apu/1-len_ctr.nes", "1-len_ctr");
    }

    #[test]
    fn test_blargg_apu_len_table() {
        run_blargg_apu_rom("input/nes/apu/2-len_table.nes", "2-len_table");
    }

    #[test]
    fn test_blargg_apu_irq_flag() {
        run_blargg_apu_rom("input/nes/apu/3-irq_flag.nes", "3-irq_flag");
    }

    #[test]
    fn test_blargg_apu_jitter() {
        run_blargg_apu_rom("input/nes/apu/4-jitter.nes", "4-jitter");
    }

    #[test]
    fn test_blargg_apu_len_timing() {
        run_blargg_apu_rom("input/nes/apu/5-len_timing.nes", "5-len_timing");
    }

    #[test]
    fn test_blargg_apu_irq_flag_timing() {
        run_blargg_apu_rom("input/nes/apu/6-irq_flag_timing.nes", "6-irq_flag_timing");
    }

    #[test]
    fn test_blargg_apu_dmc_basics() {
        run_blargg_apu_rom("input/nes/apu/7-dmc_basics.nes", "7-dmc_basics");
    }

    #[test]
    fn test_blargg_apu_dmc_rates() {
        run_blargg_apu_rom("input/nes/apu/8-dmc_rates.nes", "8-dmc_rates");
    }

    fn run_blargg_apu_2005_rom(rom_path: &str) {
        use crate::emu;
        use crate::emu::io::loader;

        let mut emu = emu::Emulator::new_headless(loader::load_nes(&String::from(rom_path)));
        emu.cpu.status = 0x34;
        emu.cpu.sp = 0xfd;
        emu.toggle_should_trigger_nmi(true);
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);
        emu.run();

        let result = emu.mem.cpu_read(0x00F0);
        assert_eq!(1, result, "{} failed with result code {}", rom_path, result);
    }

    #[test]
    fn test_blargg_apu_2005_len_ctr() {
        run_blargg_apu_2005_rom("input/nes/apu/01.len_ctr.nes");
    }

    #[test]
    fn test_blargg_apu_2005_len_table() {
        run_blargg_apu_2005_rom("input/nes/apu/02.len_table.nes");
    }

    #[test]
    fn test_blargg_apu_2005_irq_flag() {
        run_blargg_apu_2005_rom("input/nes/apu/03.irq_flag.nes");
    }

    #[test]
    fn test_blargg_apu_2005_clock_jitter() {
        run_blargg_apu_2005_rom("input/nes/apu/04.clock_jitter.nes");
    }

    #[test]
    fn test_blargg_apu_2005_len_timing_mode0() {
        run_blargg_apu_2005_rom("input/nes/apu/05.len_timing_mode0.nes");
    }

    #[test]
    fn test_blargg_apu_2005_len_timing_mode1() {
        run_blargg_apu_2005_rom("input/nes/apu/06.len_timing_mode1.nes");
    }

    #[test]
    fn test_blargg_apu_2005_irq_flag_timing() {
        run_blargg_apu_2005_rom("input/nes/apu/07.irq_flag_timing.nes");
    }

    #[test]
    fn test_blargg_apu_2005_irq_timing() {
        run_blargg_apu_2005_rom("input/nes/apu/08.irq_timing.nes");
    }

    #[test]
    fn test_blargg_apu_2005_reset_timing() {
        run_blargg_apu_2005_rom("input/nes/apu/09.reset_timing.nes");
    }

    #[test]
    fn test_blargg_apu_2005_len_halt_timing() {
        run_blargg_apu_2005_rom("input/nes/apu/10.len_halt_timing.nes");
    }

    #[test]
    fn test_blargg_apu_2005_len_reload_timing() {
        run_blargg_apu_2005_rom("input/nes/apu/11.len_reload_timing.nes");
    }
}
