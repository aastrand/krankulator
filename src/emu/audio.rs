use rodio::{OutputStream, Sink, Source};
use std::sync::{Arc, Mutex};

pub trait AudioBackend {
    fn push_samples(&self, samples: &[f32]);
}

pub struct AudioOutput {
    pub buffer: Arc<Mutex<Vec<f32>>>,
    _stream: OutputStream,
    #[allow(dead_code)]
    sink: Sink,
}

impl AudioOutput {
    pub fn new(sample_rate: u32) -> Self {
        let (stream, stream_handle) = OutputStream::try_default().unwrap();
        let sink = Sink::try_new(&stream_handle).unwrap();
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let buffer_clone = buffer.clone();

        // Custom source that pulls from the buffer
        let source = AudioBufferSource::new(buffer_clone, sample_rate);
        // Up volume
        sink.set_volume(10.0);
        sink.append(source);
        sink.play();

        Self {
            buffer,
            _stream: stream,
            sink,
        }
    }
}

impl AudioBackend for AudioOutput {
    fn push_samples(&self, samples: &[f32]) {
        let mut buf = self.buffer.lock().unwrap();
        buf.extend_from_slice(samples);
    }
}

pub struct SilentAudioOutput;

impl SilentAudioOutput {
    pub fn new() -> Self {
        Self
    }
}

impl AudioBackend for SilentAudioOutput {
    fn push_samples(&self, _samples: &[f32]) {
        // Do nothing
    }
}

struct AudioBufferSource {
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
}

impl AudioBufferSource {
    fn new(buffer: Arc<Mutex<Vec<f32>>>, sample_rate: u32) -> Self {
        Self {
            buffer,
            sample_rate,
        }
    }
}

impl Iterator for AudioBufferSource {
    type Item = f32;
    fn next(&mut self) -> Option<Self::Item> {
        let mut buf = self.buffer.lock().unwrap();
        if !buf.is_empty() {
            Some(buf.remove(0))
        } else {
            // Output silence if buffer is empty
            Some(0.0)
        }
    }
}

impl Source for AudioBufferSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        1
    }
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}
