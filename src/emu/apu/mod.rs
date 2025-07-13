pub mod channels;
pub mod frame_counter;

use channels::{DmcChannel, NoiseChannel, PulseChannel, TriangleChannel};
use frame_counter::FrameCounter;

pub const SAMPLE_RATE: u32 = 44100;
pub const CYCLES_PER_SAMPLE: u32 = 1789773 / SAMPLE_RATE; // NES CPU clock / sample rate

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
    cycles_since_sample: u32,

    // Status
    status: u8,
    enabled_channels: u8,
}

impl APU {
    pub fn new() -> Self {
        Self {
            pulse1: PulseChannel::new(),
            pulse2: PulseChannel::new(),
            triangle: TriangleChannel::new(),
            noise: NoiseChannel::new(),
            dmc: DmcChannel::new(),
            frame_counter: FrameCounter::new(),

            sample_buffer: vec![0.0; 4096], // Buffer for audio samples
            sample_index: 0,
            cycles_since_sample: 0,

            status: 0,
            enabled_channels: 0,
        }
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x4015 => self.status,
            _ => 0,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            // Pulse 1
            0x4000 => self.pulse1.set_control(value),
            0x4001 => self.pulse1.set_sweep(value),
            0x4002 => self.pulse1.set_timer_low(value),
            0x4003 => self.pulse1.set_timer_high(value),

            // Pulse 2
            0x4004 => self.pulse2.set_control(value),
            0x4005 => self.pulse2.set_sweep(value),
            0x4006 => self.pulse2.set_timer_low(value),
            0x4007 => self.pulse2.set_timer_high(value),

            // Triangle
            0x4008 => self.triangle.set_control(value),
            0x4009 => {} // Unused
            0x400A => self.triangle.set_timer_low(value),
            0x400B => self.triangle.set_timer_high(value),

            // Noise
            0x400C => self.noise.set_control(value),
            0x400D => {} // Unused
            0x400E => self.noise.set_period(value),
            0x400F => self.noise.set_length_counter(value),

            // DMC
            0x4010 => self.dmc.set_control(value),
            0x4011 => self.dmc.set_direct_load(value),
            0x4012 => self.dmc.set_sample_address(value),
            0x4013 => self.dmc.set_sample_length(value),

            // Status
            0x4015 => {
                println!("APU $4015 write: value = {:02X}", value);
                self.enabled_channels = value;
                println!("  pulse1 enable: {}", value & 0x01 != 0);
                self.pulse1.set_enabled(value & 0x01 != 0);
                println!("  pulse2 enable: {}", value & 0x02 != 0);
                self.pulse2.set_enabled(value & 0x02 != 0);
                println!("  triangle enable: {}", value & 0x04 != 0);
                self.triangle.set_enabled(value & 0x04 != 0);
                println!("  noise enable: {}", value & 0x08 != 0);
                self.noise.set_enabled(value & 0x08 != 0);
                println!("  dmc enable: {}", value & 0x10 != 0);
                self.dmc.set_enabled(value & 0x10 != 0);
            }

            // Frame Counter
            0x4017 => self.frame_counter.write(value),

            _ => {}
        }
    }

    pub fn cycle(&mut self) {
        // Run frame counter
        let frame_irq = self.frame_counter.cycle();

        // Clock length counters and linear counter on frame counter steps
        let step = self.frame_counter.get_step();
        let mode = self.frame_counter.get_mode();

        // Clock length counters on steps 0 and 2 (mode 0) or steps 0, 1, 2, 3 (mode 1)
        if (mode == 0 && (step == 0 || step == 2)) || (mode == 1 && step <= 3) {
            self.pulse1.clock_length_counter();
            self.pulse2.clock_length_counter();
            self.triangle.clock_length_counter();
            self.noise.clock_length_counter();
        }

        // Clock linear counter on every cycle (except when reload flag is set)
        self.triangle.clock_linear_counter();

        // Update status register
        self.status &= 0x40; // Keep DMC IRQ bit
        if frame_irq {
            self.status |= 0x40;
        }

        // Cycle channels
        self.pulse1.cycle();
        self.pulse2.cycle();
        self.triangle.cycle();
        self.noise.cycle();
        self.dmc.cycle();

        // Generate audio samples
        self.cycles_since_sample += 1;
        if self.cycles_since_sample >= CYCLES_PER_SAMPLE {
            self.cycles_since_sample = 0;
            self.generate_sample();
        }
    }

    fn generate_sample(&mut self) {
        let pulse1_sample = self.pulse1.get_sample();
        let pulse2_sample = self.pulse2.get_sample();
        let triangle_sample = self.triangle.get_sample();
        let noise_sample = self.noise.get_sample();
        let dmc_sample = self.dmc.get_sample();

        // Debug: print individual channel samples and states
        if self.sample_index % 1000 == 0 {
            // Only print every 1000th sample to avoid spam
            println!(
                "APU Debug - Pulse1: {} (enabled: {}, len: {}), Pulse2: {} (enabled: {}, len: {}), Triangle: {} (enabled: {}, len: {}), Noise: {} (enabled: {}, len: {}), DMC: {} (enabled: {})",
                pulse1_sample, self.pulse1.is_enabled(), self.pulse1.get_length_counter(),
                pulse2_sample, self.pulse2.is_enabled(), self.pulse2.get_length_counter(),
                triangle_sample, self.triangle.is_enabled(), self.triangle.get_length_counter(),
                noise_sample, self.noise.is_enabled(), self.noise.get_length_counter(),
                dmc_sample, self.dmc.is_enabled()
            );
        }

        // Mix all channels
        let mixed_sample = self.mix_channels(
            pulse1_sample,
            pulse2_sample,
            triangle_sample,
            noise_sample,
            dmc_sample,
        );

        // Debug: check if we're generating any samples
        if mixed_sample != 0.0 {
            println!("APU generating sample: {}", mixed_sample);
        }

        // Store in buffer
        if self.sample_index < self.sample_buffer.len() {
            self.sample_buffer[self.sample_index] = mixed_sample;
            self.sample_index += 1;
        }
    }

    fn mix_channels(&self, pulse1: f32, pulse2: f32, triangle: f32, noise: f32, dmc: f32) -> f32 {
        // NES audio mixing algorithm
        // Based on Blargg's NES APU reference implementation

        let pulse_out = (pulse1 + pulse2) * 0.00752;
        let tnd_out = triangle * 0.00851 + noise * 0.00494 + dmc * 0.00335;

        let mixed = pulse_out + tnd_out;

        // Clamp to [-1.0, 1.0]
        mixed.max(-1.0).min(1.0)
    }

    pub fn get_audio_samples(&mut self) -> &[f32] {
        let samples = &self.sample_buffer[..self.sample_index];
        self.sample_index = 0;
        samples
    }

    pub fn clear_irq(&mut self) {
        self.status &= 0xBF; // Clear frame IRQ bit
    }
}
