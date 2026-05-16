use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ringbuf::{
    traits::{Consumer, Producer, Split},
    HeapCons, HeapProd, HeapRb,
};
use rodio::{OutputStream, Sink, Source};

use krankulator_core::emu::audio::AudioBackend;

pub struct AudioOutput {
    producer: HeapProd<f32>,
    mute: Arc<AtomicBool>,
    _stream: OutputStream,
    #[allow(dead_code)]
    sink: Sink,
}

impl AudioOutput {
    pub fn try_new(sample_rate: u32) -> Option<Self> {
        let rb = HeapRb::<f32>::new(8192);
        let (producer, consumer) = rb.split();
        let mute = Arc::new(AtomicBool::new(false));

        let (stream, stream_handle) = OutputStream::try_default().ok()?;
        let sink = Sink::try_new(&stream_handle).ok()?;

        let source = AudioBufferSource::new(consumer, sample_rate, Arc::clone(&mute));
        sink.append(source);
        sink.play();

        Some(Self {
            producer,
            mute,
            _stream: stream,
            sink,
        })
    }
}

impl AudioBackend for AudioOutput {
    fn push_samples(&mut self, samples: &[f32]) {
        self.producer.push_slice(samples);
    }

    fn clear(&mut self) {
        self.mute.store(true, Ordering::Relaxed);
    }
}

struct AudioBufferSource {
    consumer: HeapCons<f32>,
    mute: Arc<AtomicBool>,
    sample_rate: u32,
}

impl AudioBufferSource {
    fn new(consumer: HeapCons<f32>, sample_rate: u32, mute: Arc<AtomicBool>) -> Self {
        Self {
            consumer,
            mute,
            sample_rate,
        }
    }
}

impl Iterator for AudioBufferSource {
    type Item = f32;
    fn next(&mut self) -> Option<Self::Item> {
        if self.mute.load(Ordering::Relaxed) {
            while self.consumer.try_pop().is_some() {}
            self.mute.store(false, Ordering::Relaxed);
            return Some(0.0);
        }
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
