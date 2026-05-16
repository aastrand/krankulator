pub mod wav;

pub trait AudioBackend {
    fn push_samples(&mut self, samples: &[f32]);
    fn clear(&mut self);
    fn flush(&mut self) {}
    fn drain_captured(&mut self) -> Vec<f32> {
        Vec::new()
    }
}

pub struct SilentAudioOutput;

impl SilentAudioOutput {
    pub fn new() -> Self {
        Self
    }
}

impl AudioBackend for SilentAudioOutput {
    fn push_samples(&mut self, _samples: &[f32]) {}
    fn clear(&mut self) {}
}

pub struct CapturingAudioOutput {
    buf: Vec<f32>,
}

impl CapturingAudioOutput {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(44100 * 30),
        }
    }
}

impl AudioBackend for CapturingAudioOutput {
    fn push_samples(&mut self, samples: &[f32]) {
        self.buf.extend_from_slice(samples);
    }

    fn clear(&mut self) {
        self.buf.clear();
    }

    fn drain_captured(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.buf)
    }
}
