use std::fs::File;
use std::io::{self, Write};

pub fn write_wav(path: &str, samples: &[f32], sample_rate: u32) -> io::Result<()> {
    let mut f = File::create(path)?;
    let data_len = (samples.len() * 4) as u32;
    // RIFF header
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVE")?;
    // fmt chunk: IEEE float
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&3u16.to_le_bytes())?; // IEEE float
    f.write_all(&1u16.to_le_bytes())?; // mono
    f.write_all(&sample_rate.to_le_bytes())?;
    f.write_all(&(sample_rate * 4).to_le_bytes())?; // byte rate
    f.write_all(&4u16.to_le_bytes())?; // block align
    f.write_all(&32u16.to_le_bytes())?; // bits per sample
                                        // data chunk
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for &s in samples {
        f.write_all(&s.to_le_bytes())?;
    }
    Ok(())
}
