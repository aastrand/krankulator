const DEFAULT_CAPACITY: usize = 300;
pub const CAPTURES_PER_SECOND: f64 = 30.0;

pub struct RewindBuffer {
    savestates: Vec<Vec<u8>>,
    framebuffers: Vec<Vec<u8>>,
    head: usize,
    len: usize,
    capacity: usize,
}

impl Default for RewindBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl RewindBuffer {
    pub fn new() -> Self {
        Self {
            savestates: vec![Vec::new(); DEFAULT_CAPACITY],
            framebuffers: vec![Vec::new(); DEFAULT_CAPACITY],
            head: 0,
            len: 0,
            capacity: DEFAULT_CAPACITY,
        }
    }

    pub fn push(&mut self, savestate: &[u8], framebuffer: &[u8]) {
        let slot = self.head;
        copy_into(&mut self.savestates[slot], savestate);
        copy_into(&mut self.framebuffers[slot], framebuffer);
        self.head = (self.head + 1) % self.capacity;
        if self.len < self.capacity {
            self.len += 1;
        }
    }

    pub fn pop_into(&mut self, framebuffer_dst: &mut [u8]) -> Option<Vec<u8>> {
        if self.len == 0 {
            return None;
        }
        self.head = if self.head == 0 {
            self.capacity - 1
        } else {
            self.head - 1
        };
        self.len -= 1;
        framebuffer_dst.copy_from_slice(&self.framebuffers[self.head]);
        Some(std::mem::take(&mut self.savestates[self.head]))
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }
}

fn copy_into(dst: &mut Vec<u8>, src: &[u8]) {
    dst.clear();
    dst.extend_from_slice(src);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop() {
        let mut buf = RewindBuffer::new();
        assert!(buf.is_empty());

        buf.push(&[1, 2, 3], &[10, 20, 30]);
        buf.push(&[4, 5, 6], &[40, 50, 60]);
        assert!(!buf.is_empty());

        let mut fb = vec![0u8; 3];
        let ss = buf.pop_into(&mut fb).unwrap();
        assert_eq!(ss, vec![4, 5, 6]);
        assert_eq!(fb, vec![40, 50, 60]);

        let ss = buf.pop_into(&mut fb).unwrap();
        assert_eq!(ss, vec![1, 2, 3]);
        assert_eq!(fb, vec![10, 20, 30]);

        assert!(buf.is_empty());
    }

    #[test]
    fn test_wrap_around() {
        let mut buf = RewindBuffer {
            savestates: vec![Vec::new(); 3],
            framebuffers: vec![Vec::new(); 3],
            head: 0,
            len: 0,
            capacity: 3,
        };

        buf.push(&[1], &[10]);
        buf.push(&[2], &[20]);
        buf.push(&[3], &[30]);
        buf.push(&[4], &[40]);

        let mut fb = vec![0u8; 1];
        assert_eq!(buf.pop_into(&mut fb).unwrap(), vec![4]);
        assert_eq!(buf.pop_into(&mut fb).unwrap(), vec![3]);
        assert_eq!(buf.pop_into(&mut fb).unwrap(), vec![2]);
        assert!(buf.pop_into(&mut fb).is_none());
    }

    #[test]
    fn test_reuses_framebuffer_allocation() {
        let mut buf = RewindBuffer {
            savestates: vec![Vec::new(); 3],
            framebuffers: vec![Vec::new(); 3],
            head: 0,
            len: 0,
            capacity: 3,
        };

        buf.push(&[1], &[10, 20, 30, 40, 50]);
        let mut fb = vec![0u8; 5];
        buf.pop_into(&mut fb);
        assert!(buf.framebuffers[0].capacity() >= 5);
    }

    #[test]
    fn test_clear() {
        let mut buf = RewindBuffer::new();
        buf.push(&[1], &[10]);
        buf.push(&[2], &[20]);
        buf.clear();
        assert!(buf.is_empty());
        let mut fb = vec![0u8; 1];
        assert!(buf.pop_into(&mut fb).is_none());
    }
}
