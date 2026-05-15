use std::io;

const MAGIC: &[u8; 4] = b"KRNK";
const VERSION: u8 = 4;

pub struct SavestateWriter {
    buf: Vec<u8>,
}

impl SavestateWriter {
    pub fn new() -> Self {
        let mut w = Self {
            buf: Vec::with_capacity(64 * 1024),
        };
        w.buf.extend_from_slice(MAGIC);
        w.buf.push(VERSION);
        w
    }

    pub fn write_u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    pub fn write_u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn write_u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn write_u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn write_i8(&mut self, v: i8) {
        self.buf.push(v as u8);
    }
    pub fn write_f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn write_f64(&mut self, v: f64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn write_bool(&mut self, v: bool) {
        self.buf.push(v as u8);
    }

    pub fn write_bytes(&mut self, data: &[u8]) {
        self.write_u32(data.len() as u32);
        self.buf.extend_from_slice(data);
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

pub struct SavestateReader<'a> {
    data: &'a [u8],
    pos: usize,
    version: u8,
}

impl<'a> SavestateReader<'a> {
    pub fn new(data: &'a [u8]) -> io::Result<Self> {
        if data.len() < 5 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "savestate too short",
            ));
        }
        if &data[0..4] != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bad savestate magic",
            ));
        }
        let version = data[4];
        if version < 3 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "savestate version {} is too old (minimum supported: 3)",
                    version
                ),
            ));
        }
        if version > VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "savestate version {} is newer than supported {}",
                    version, VERSION
                ),
            ));
        }
        Ok(Self {
            data,
            pos: 5,
            version,
        })
    }

    pub fn version(&self) -> u8 {
        self.version
    }

    fn need(&self, n: usize) -> io::Result<()> {
        if self.pos + n > self.data.len() {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "savestate truncated",
            ))
        } else {
            Ok(())
        }
    }

    pub fn read_u8(&mut self) -> io::Result<u8> {
        self.need(1)?;
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    pub fn read_u16(&mut self) -> io::Result<u16> {
        self.need(2)?;
        let v = u16::from_le_bytes(self.data[self.pos..self.pos + 2].try_into().unwrap());
        self.pos += 2;
        Ok(v)
    }

    pub fn read_u32(&mut self) -> io::Result<u32> {
        self.need(4)?;
        let v = u32::from_le_bytes(self.data[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        Ok(v)
    }

    pub fn read_u64(&mut self) -> io::Result<u64> {
        self.need(8)?;
        let v = u64::from_le_bytes(self.data[self.pos..self.pos + 8].try_into().unwrap());
        self.pos += 8;
        Ok(v)
    }

    pub fn read_i8(&mut self) -> io::Result<i8> {
        Ok(self.read_u8()? as i8)
    }

    pub fn read_f32(&mut self) -> io::Result<f32> {
        self.need(4)?;
        let v = f32::from_le_bytes(self.data[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        Ok(v)
    }

    pub fn read_f64(&mut self) -> io::Result<f64> {
        self.need(8)?;
        let v = f64::from_le_bytes(self.data[self.pos..self.pos + 8].try_into().unwrap());
        self.pos += 8;
        Ok(v)
    }

    pub fn read_bool(&mut self) -> io::Result<bool> {
        Ok(self.read_u8()? != 0)
    }

    #[allow(dead_code)]
    pub fn read_bytes(&mut self) -> io::Result<Vec<u8>> {
        let len = self.read_u32()? as usize;
        self.need(len)?;
        let v = self.data[self.pos..self.pos + len].to_vec();
        self.pos += len;
        Ok(v)
    }

    pub fn read_bytes_into(&mut self, dest: &mut [u8]) -> io::Result<()> {
        let len = self.read_u32()? as usize;
        if len != dest.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("expected {} bytes, got {}", dest.len(), len),
            ));
        }
        self.need(len)?;
        dest.copy_from_slice(&self.data[self.pos..self.pos + len]);
        self.pos += len;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_primitives() {
        let mut w = SavestateWriter::new();
        w.write_u8(0x42);
        w.write_u16(0x1234);
        w.write_u32(0xDEADBEEF);
        w.write_u64(0x0102030405060708);
        w.write_i8(-5);
        w.write_f32(3.14);
        w.write_f64(2.718281828);
        w.write_bool(true);
        w.write_bool(false);
        w.write_bytes(&[1, 2, 3, 4]);

        let data = w.finish();
        let mut r = SavestateReader::new(&data).unwrap();

        assert_eq!(r.read_u8().unwrap(), 0x42);
        assert_eq!(r.read_u16().unwrap(), 0x1234);
        assert_eq!(r.read_u32().unwrap(), 0xDEADBEEF);
        assert_eq!(r.read_u64().unwrap(), 0x0102030405060708);
        assert_eq!(r.read_i8().unwrap(), -5);
        assert!((r.read_f32().unwrap() - 3.14).abs() < 1e-6);
        assert!((r.read_f64().unwrap() - 2.718281828).abs() < 1e-9);
        assert_eq!(r.read_bool().unwrap(), true);
        assert_eq!(r.read_bool().unwrap(), false);
        let mut buf = [0u8; 4];
        r.read_bytes_into(&mut buf).unwrap();
        assert_eq!(buf, [1, 2, 3, 4]);
    }

    #[test]
    fn test_bad_magic() {
        let data = b"BAD!\x01rest";
        assert!(SavestateReader::new(data).is_err());
    }

    #[test]
    fn test_future_version_rejected() {
        let mut data = Vec::new();
        data.extend_from_slice(MAGIC);
        data.push(VERSION + 1);
        assert!(SavestateReader::new(&data).is_err());
    }

    #[test]
    fn test_older_version_accepted() {
        let mut data = Vec::new();
        data.extend_from_slice(MAGIC);
        data.push(3);
        let r = SavestateReader::new(&data).unwrap();
        assert_eq!(r.version(), 3);
    }

    #[test]
    fn test_too_old_version_rejected() {
        let mut data = Vec::new();
        data.extend_from_slice(MAGIC);
        data.push(2);
        assert!(SavestateReader::new(&data).is_err());
    }

    #[test]
    fn test_truncated() {
        let mut w = SavestateWriter::new();
        w.write_u64(42);
        let data = w.finish();
        let truncated = &data[..data.len() - 2];
        let mut r = SavestateReader::new(truncated).unwrap();
        assert!(r.read_u64().is_err());
    }

    #[test]
    fn test_bytes_length_mismatch() {
        let mut w = SavestateWriter::new();
        w.write_bytes(&[1, 2, 3]);
        let data = w.finish();
        let mut r = SavestateReader::new(&data).unwrap();
        let mut buf = [0u8; 5];
        assert!(r.read_bytes_into(&mut buf).is_err());
    }
}
