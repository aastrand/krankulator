use crate::emu::savestate::{SavestateWriter, SavestateReader};

#[derive(Debug, PartialEq)]
pub enum FrameStep {
    None,
    Step(u8),
    /// $4017 reset finished; if `immediate_clock`, clock length/half/quarter once (5-step entry).
    Deferred4017Apply { immediate_clock: bool },
}

pub struct FrameCounter {
    mode: u8,
    step: u8,
    cycles: u32,
    irq_inhibit: bool,
    /// CPU cycles remaining until `pending_write` is applied (see nesdev $4017 delay).
    reset_delay: u8,
    pending_write: u8,
}

/// NTSC 4-step sequence: cycles between quarter/half-frame clocks (sum 29829).
/// Source: nesdev wiki "APU Frame Counter", confirmed against Mesen2 NesApu.cpp.
const NTSC_4: [u32; 4] = [7457, 7456, 7458, 7458];
/// NTSC 5-step sequence (sum 37281).
const NTSC_5: [u32; 5] = [7457, 7456, 7458, 7458, 7452];

impl FrameCounter {
    pub fn new() -> Self {
        Self {
            mode: 0,
            step: 0,
            cycles: 0,
            irq_inhibit: false,
            reset_delay: 0,
            pending_write: 0,
        }
    }

    /// Call for $4017 writes. `emu_cycle` should match the global 1:1 CPU cycle counter when the
    /// write is sampled (even CPU cycle → 3 A cycles delay, odd → 4).
    pub fn write(&mut self, value: u8, emu_cycle: u64) {
        self.irq_inhibit = (value >> 6) & 1 != 0;
        self.pending_write = value;
        self.reset_delay = if (emu_cycle & 1) == 0 { 3 } else { 4 };
    }

    pub fn cycle(&mut self) -> FrameStep {
        if self.reset_delay > 0 {
            self.reset_delay -= 1;
            if self.reset_delay == 0 {
                self.mode = (self.pending_write >> 7) & 1;
                self.step = 0;
                self.cycles = 0;
                if self.mode == 1 {
                    return FrameStep::Deferred4017Apply {
                        immediate_clock: true,
                    };
                }
                return FrameStep::None;
            }
            return FrameStep::None;
        }

        match self.mode {
            0 => self.cycle_mode_0(),
            1 => self.cycle_mode_1(),
            _ => FrameStep::None,
        }
    }

    fn cycle_mode_0(&mut self) -> FrameStep {
        let period = NTSC_4[self.step as usize];
        self.cycles += 1;
        if self.cycles < period {
            return FrameStep::None;
        }
        self.cycles = 0;
        self.step = (self.step + 1) % 4;
        FrameStep::Step(self.step)
    }

    fn cycle_mode_1(&mut self) -> FrameStep {
        let period = NTSC_5[self.step as usize];
        self.cycles += 1;
        if self.cycles < period {
            return FrameStep::None;
        }
        self.cycles = 0;
        self.step = (self.step + 1) % 5;
        if self.step == 0 {
            FrameStep::None
        } else {
            FrameStep::Step(self.step)
        }
    }

    #[cfg(test)]
    pub fn get_step(&self) -> u8 {
        self.step
    }

    pub fn get_mode(&self) -> u8 {
        self.mode
    }

    pub fn irq_inhibit(&self) -> bool {
        self.irq_inhibit
    }

    pub fn save_state(&self, w: &mut SavestateWriter) {
        w.write_u8(self.mode);
        w.write_u8(self.step);
        w.write_u32(self.cycles);
        w.write_bool(self.irq_inhibit);
        w.write_u8(self.reset_delay);
        w.write_u8(self.pending_write);
    }

    pub fn load_state(&mut self, r: &mut SavestateReader) -> std::io::Result<()> {
        self.mode = r.read_u8()?;
        self.step = r.read_u8()?;
        self.cycles = r.read_u32()?;
        self.irq_inhibit = r.read_bool()?;
        self.reset_delay = r.read_u8()?;
        self.pending_write = r.read_u8()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_counter_new() {
        let fc = FrameCounter::new();
        assert_eq!(fc.mode, 0);
        assert_eq!(fc.step, 0);
        assert_eq!(fc.cycles, 0);
        assert!(!fc.irq_inhibit);
    }

    #[test]
    fn test_frame_counter_write() {
        let mut fc = FrameCounter::new();

        fc.write(0x00, 0);
        assert_eq!(fc.mode, 0);
        assert!(!fc.irq_inhibit);
        assert_eq!(fc.reset_delay, 3);

        fc.write(0x40, 0);
        assert_eq!(fc.mode, 0);
        assert!(fc.irq_inhibit);

        fc.write(0xC0, 1);
        assert_eq!(fc.reset_delay, 4);
        assert!(fc.irq_inhibit);
    }

    #[test]
    fn test_frame_counter_mode_0_cycle() {
        let mut fc = FrameCounter::new();
        fc.write(0x00, 0);
        for _ in 0..2 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::None);
        assert_eq!(fc.mode, 0);

        for _ in 0..7456 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(1));
        assert_eq!(fc.step, 1);

        for _ in 0..7455 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(2));

        for _ in 0..7457 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(3));

        for _ in 0..7457 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(0));
        assert_eq!(fc.step, 0);
    }

    #[test]
    fn test_frame_counter_mode_0_irq() {
        let mut fc = FrameCounter::new();
        fc.write(0x00, 0);
        for _ in 0..2 {
            fc.cycle();
        }
        fc.cycle();

        for _ in 0..22371 {
            fc.cycle();
        }
        assert_eq!(fc.step, 3);

        for _ in 0..7457 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(0));
        assert_eq!(fc.step, 0);
    }

    #[test]
    fn test_frame_counter_mode_0_irq_inhibit_still_clocks_step0() {
        let mut fc = FrameCounter::new();
        fc.write(0x40, 0);
        for _ in 0..2 {
            fc.cycle();
        }
        fc.cycle();

        for _ in 0..22371 {
            fc.cycle();
        }
        assert_eq!(fc.step, 3);

        for _ in 0..7457 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(0));
        assert_eq!(fc.step, 0);
    }

    #[test]
    fn test_frame_counter_mode_1_cycle() {
        let mut fc = FrameCounter::new();
        fc.write(0x80, 0);
        for _ in 0..2 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(
            fc.cycle(),
            FrameStep::Deferred4017Apply {
                immediate_clock: true
            }
        );

        for _ in 0..7456 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(1));

        for _ in 0..7455 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(2));

        for _ in 0..7457 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(3));

        for _ in 0..7457 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(4));

        for _ in 0..7451 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::None);
        assert_eq!(fc.step, 0);
    }

    #[test]
    fn test_frame_counter_mode_1_no_irq() {
        let mut fc = FrameCounter::new();
        fc.write(0x80, 0);
        for _ in 0..2 {
            fc.cycle();
        }
        assert_eq!(
            fc.cycle(),
            FrameStep::Deferred4017Apply {
                immediate_clock: true
            }
        );

        let mut step_count = 0;
        for _ in 0..37281 {
            let result = fc.cycle();
            if let FrameStep::Step(_) = result {
                step_count += 1;
            }
        }
        assert_eq!(fc.step, 0);
        assert_eq!(step_count, 4);
    }

    #[test]
    fn test_frame_counter_get_step() {
        let mut fc = FrameCounter::new();

        fc.write(0x00, 0);
        for _ in 0..2 {
            fc.cycle();
        }
        fc.cycle();
        for _ in 0..7457 {
            fc.cycle();
        }
        assert_eq!(fc.get_step(), 1);
    }

    #[test]
    fn test_frame_counter_get_mode() {
        let mut fc = FrameCounter::new();
        assert_eq!(fc.get_mode(), 0);

        fc.write(0x80, 0);
        assert_eq!(fc.get_mode(), 0);
        for _ in 0..3 {
            fc.cycle();
        }
        assert_eq!(fc.get_mode(), 1);

        fc.write(0x00, 0);
        for _ in 0..2 {
            fc.cycle();
        }
        fc.cycle();
        assert_eq!(fc.get_mode(), 0);
    }

    #[test]
    fn test_frame_counter_timing_accuracy() {
        let mut fc = FrameCounter::new();
        fc.write(0x00, 0);
        for _ in 0..2 {
            fc.cycle();
        }
        fc.cycle();

        for _ in 0..7457 {
            fc.cycle();
        }
        assert_eq!(fc.cycles, 0);
        assert_eq!(fc.step, 1);

        fc.write(0x00, 0);
        for _ in 0..2 {
            fc.cycle();
        }
        fc.cycle();
        for _ in 0..7456 {
            fc.cycle();
        }
        assert_eq!(fc.step, 0);
    }

    #[test]
    fn test_frame_counter_mode_1_timing_accuracy() {
        let mut fc = FrameCounter::new();
        fc.write(0x80, 0);
        for _ in 0..3 {
            fc.cycle();
        }

        for _ in 0..22371 {
            fc.cycle();
        }
        assert_eq!(fc.step, 3);

        for _ in 0..7457 {
            assert_eq!(fc.cycle(), FrameStep::None);
        }
        assert_eq!(fc.cycle(), FrameStep::Step(4));
        assert_eq!(fc.cycles, 0);
    }
}
