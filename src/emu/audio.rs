use ringbuf::{
    traits::{Consumer, Producer, Split},
    HeapCons, HeapProd, HeapRb,
};
use rodio::{OutputStream, Sink, Source};

pub trait AudioBackend {
    fn push_samples(&mut self, samples: &[f32]);
    fn clear(&mut self);
}

pub struct AudioOutput {
    producer: HeapProd<f32>,
    _stream: OutputStream,
    #[allow(dead_code)]
    sink: Sink,
}

impl AudioOutput {
    pub fn new(sample_rate: u32) -> Self {
        let rb = HeapRb::<f32>::new(8192);
        let (producer, consumer) = rb.split();

        let (stream, stream_handle) = OutputStream::try_default().unwrap();
        let sink = Sink::try_new(&stream_handle).unwrap();

        let source = AudioBufferSource::new(consumer, sample_rate);
        sink.append(source);
        sink.play();

        Self {
            producer,
            _stream: stream,
            sink,
        }
    }
}

impl AudioBackend for AudioOutput {
    fn push_samples(&mut self, samples: &[f32]) {
        self.producer.push_slice(samples);
    }

    fn clear(&mut self) {
        // Consumer drains residual samples naturally (~186ms worst case).
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

struct AudioBufferSource {
    consumer: HeapCons<f32>,
    sample_rate: u32,
}

impl AudioBufferSource {
    fn new(consumer: HeapCons<f32>, sample_rate: u32) -> Self {
        Self {
            consumer,
            sample_rate,
        }
    }
}

impl Iterator for AudioBufferSource {
    type Item = f32;
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.consumer.try_pop().unwrap_or(0.0))
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
