const DEFAULT_CAPACITY: usize = 9000;
pub const CAPTURES_PER_SECOND: f64 = 30.0;

pub struct RewindBuffer {
    deltas: Vec<Vec<u8>>,
    head: usize,
    len: usize,
    capacity: usize,
    has_wrapped: bool,
    prev_savestate: Vec<u8>,
    rewind_state: Vec<u8>,
    rewind_cursor: usize,
    rewind_remaining: usize,
    rewind_first_step: bool,
    entries_consumed: usize,
}

impl Default for RewindBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl RewindBuffer {
    pub fn new() -> Self {
        Self {
            deltas: vec![Vec::new(); DEFAULT_CAPACITY],
            head: 0,
            len: 0,
            capacity: DEFAULT_CAPACITY,
            has_wrapped: false,
            prev_savestate: Vec::new(),
            rewind_state: Vec::new(),
            rewind_cursor: 0,
            rewind_remaining: 0,
            rewind_first_step: false,
            entries_consumed: 0,
        }
    }

    pub fn push(&mut self, savestate: &[u8]) {
        let delta = compress_delta(&self.prev_savestate, savestate);
        copy_into(&mut self.deltas[self.head], &delta);

        self.head = (self.head + 1) % self.capacity;
        if self.len < self.capacity {
            self.len += 1;
        } else {
            self.has_wrapped = true;
        }

        copy_into(&mut self.prev_savestate, savestate);
    }

    pub fn begin_rewind(&mut self) {
        self.rewind_state.clear();
        self.rewind_state.extend_from_slice(&self.prev_savestate);
        self.rewind_cursor = if self.head == 0 {
            self.capacity - 1
        } else {
            self.head - 1
        };
        self.rewind_remaining = if self.has_wrapped {
            self.len + 1
        } else {
            self.len
        };
        self.rewind_first_step = true;
        self.entries_consumed = 0;
    }

    fn advance_rewind(&mut self) -> bool {
        if self.rewind_remaining == 0 {
            return false;
        }
        self.rewind_remaining -= 1;

        if self.rewind_first_step {
            self.rewind_first_step = false;
            return true;
        }

        apply_compressed_delta(&mut self.rewind_state, &self.deltas[self.rewind_cursor]);
        self.rewind_cursor = if self.rewind_cursor == 0 {
            self.capacity - 1
        } else {
            self.rewind_cursor - 1
        };
        self.entries_consumed += 1;
        true
    }

    pub fn step_back(&mut self) -> Option<&[u8]> {
        if self.advance_rewind() {
            Some(&self.rewind_state)
        } else {
            None
        }
    }

    pub fn step_back_into(&mut self, dest: &mut Vec<u8>) -> bool {
        if self.advance_rewind() {
            copy_into(dest, &self.rewind_state);
            true
        } else {
            false
        }
    }

    pub fn finish_rewind(&mut self) {
        self.len -= self.entries_consumed;
        if self.entries_consumed > 0 {
            self.head = (self.rewind_cursor + 1) % self.capacity;
        }
        copy_into(&mut self.prev_savestate, &self.rewind_state);
        if self.len == 0 {
            self.has_wrapped = false;
        }
    }

    pub fn rewind_remaining(&self) -> usize {
        self.rewind_remaining
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
        self.has_wrapped = false;
        self.prev_savestate.clear();
        self.rewind_state.clear();
        self.rewind_remaining = 0;
    }
}

fn copy_into(dst: &mut Vec<u8>, src: &[u8]) {
    dst.clear();
    dst.extend_from_slice(src);
}

/// Compress XOR delta between `old` and `new` into sparse block format:
/// `[original_len: u32][block_count: u32][blocks: (offset: u32, length: u32, data...)]`
fn compress_delta(old: &[u8], new: &[u8]) -> Vec<u8> {
    let len = old.len().max(new.len());
    let mut result = Vec::with_capacity(64);
    result.extend_from_slice(&(len as u32).to_le_bytes());

    let block_count_pos = result.len();
    result.extend_from_slice(&0u32.to_le_bytes());

    let mut block_count: u32 = 0;
    let mut i = 0;

    while i < len {
        let old_byte = old.get(i).copied().unwrap_or(0);
        let new_byte = new.get(i).copied().unwrap_or(0);
        let xor = old_byte ^ new_byte;

        if xor != 0 {
            let block_start = i;
            let mut block_data = vec![xor];
            i += 1;

            let mut zeros_in_a_row = 0;
            while i < len && zeros_in_a_row < 3 {
                let ob = old.get(i).copied().unwrap_or(0);
                let nb = new.get(i).copied().unwrap_or(0);
                let x = ob ^ nb;
                if x == 0 {
                    zeros_in_a_row += 1;
                } else {
                    zeros_in_a_row = 0;
                }
                block_data.push(x);
                i += 1;
            }

            while block_data.last() == Some(&0) {
                block_data.pop();
            }

            result.extend_from_slice(&(block_start as u32).to_le_bytes());
            result.extend_from_slice(&(block_data.len() as u32).to_le_bytes());
            result.extend_from_slice(&block_data);
            block_count += 1;
        } else {
            i += 1;
        }
    }

    result[block_count_pos..block_count_pos + 4].copy_from_slice(&block_count.to_le_bytes());

    result
}

fn apply_compressed_delta(state: &mut Vec<u8>, delta: &[u8]) {
    if delta.len() < 8 {
        return;
    }
    let original_len = u32::from_le_bytes(delta[0..4].try_into().unwrap()) as usize;
    let block_count = u32::from_le_bytes(delta[4..8].try_into().unwrap()) as usize;

    state.resize(original_len, 0);

    let mut pos = 8;
    for _ in 0..block_count {
        if pos + 8 > delta.len() {
            break;
        }
        let offset = u32::from_le_bytes(delta[pos..pos + 4].try_into().unwrap()) as usize;
        let length = u32::from_le_bytes(delta[pos + 4..pos + 8].try_into().unwrap()) as usize;
        pos += 8;

        if pos + length > delta.len() {
            break;
        }
        for j in 0..length {
            if offset + j < state.len() {
                state[offset + j] ^= delta[pos + j];
            }
        }
        pos += length;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_small_buffer(capacity: usize) -> RewindBuffer {
        RewindBuffer {
            deltas: vec![Vec::new(); capacity],
            head: 0,
            len: 0,
            capacity,
            has_wrapped: false,
            prev_savestate: Vec::new(),
            rewind_state: Vec::new(),
            rewind_cursor: 0,
            rewind_remaining: 0,
            rewind_first_step: false,
            entries_consumed: 0,
        }
    }

    #[test]
    fn test_compress_decompress_roundtrip() {
        let old = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let new = vec![1, 2, 99, 4, 5, 6, 77, 8];

        let delta = compress_delta(&old, &new);
        let mut state = old.clone();
        apply_compressed_delta(&mut state, &delta);
        assert_eq!(state, new);

        apply_compressed_delta(&mut state, &delta);
        assert_eq!(state, old);
    }

    #[test]
    fn test_compress_identical() {
        let data = vec![10, 20, 30, 40];
        let delta = compress_delta(&data, &data);
        let block_count = u32::from_le_bytes(delta[4..8].try_into().unwrap());
        assert_eq!(block_count, 0);
    }

    #[test]
    fn test_compress_completely_different() {
        let old = vec![0u8; 100];
        let new = vec![0xFF; 100];
        let delta = compress_delta(&old, &new);

        let mut state = old.clone();
        apply_compressed_delta(&mut state, &delta);
        assert_eq!(state, new);
    }

    #[test]
    fn test_compress_large_offsets() {
        let mut old = vec![0u8; 100_000];
        let mut new = vec![0u8; 100_000];
        // Change bytes past the 64K boundary
        new[70_000] = 0xAA;
        new[90_000] = 0xBB;

        let delta = compress_delta(&old, &new);
        apply_compressed_delta(&mut old, &delta);
        assert_eq!(old[70_000], 0xAA);
        assert_eq!(old[90_000], 0xBB);
        assert_eq!(old[0], 0);
    }

    #[test]
    fn test_push_step_back() {
        let mut buf = RewindBuffer::new();

        let s1 = vec![1, 2, 3, 4, 5];
        let s2 = vec![1, 2, 99, 4, 5];
        let s3 = vec![1, 2, 99, 4, 77];

        buf.push(&s1);
        buf.push(&s2);
        buf.push(&s3);

        buf.begin_rewind();

        assert_eq!(buf.step_back().unwrap(), &s3[..]);
        assert_eq!(buf.step_back().unwrap(), &s2[..]);
        assert_eq!(buf.step_back().unwrap(), &s1[..]);
        assert!(buf.step_back().is_none());
    }

    #[test]
    fn test_push_many_step_back() {
        let mut buf = RewindBuffer::new();

        let states: Vec<Vec<u8>> = (0..100u8)
            .map(|i| {
                let mut s = vec![0u8; 50];
                s[0] = i;
                s[25] = i.wrapping_mul(3);
                s[49] = i.wrapping_add(100);
                s
            })
            .collect();

        for s in &states {
            buf.push(s);
        }

        buf.begin_rewind();

        for expected in states.iter().rev() {
            let got = buf.step_back().unwrap().to_vec();
            assert_eq!(got, *expected);
        }

        assert!(buf.step_back().is_none());
    }

    #[test]
    fn test_finish_rewind_truncates() {
        let mut buf = RewindBuffer::new();

        for i in 0..5u8 {
            buf.push(&[i, i + 10, i + 20]);
        }

        buf.begin_rewind();
        buf.step_back(); // s4 (first step, current position)
        buf.step_back(); // s3
        buf.finish_rewind();

        // Buffer preserves states reachable from the resume point (s3).
        // Non-wrapped buffer: s3 (current), s2, s1, s0.
        buf.begin_rewind();
        let mut results = Vec::new();
        while let Some(state) = buf.step_back() {
            results.push(state.to_vec());
        }
        assert_eq!(results.len(), 4);
        assert_eq!(results[0], vec![3, 13, 23]);
        assert_eq!(results[1], vec![2, 12, 22]);
        assert_eq!(results[2], vec![1, 11, 21]);
        assert_eq!(results[3], vec![0, 10, 20]);
    }

    #[test]
    fn test_finish_rewind_wrapped() {
        let mut buf = new_small_buffer(3);

        // Push 5 states into cap-3 buffer → wraps, has_wrapped = true
        for i in 0..5u8 {
            buf.push(&[i, i + 10]);
        }
        // Buffer: deltas for s2→s3, s3→s4 at valid slots + prev=s4
        // Rewindable: s4, s3, s2 + s1 (via wrap) = 4 states

        buf.begin_rewind();
        buf.step_back(); // s4
        buf.step_back(); // s3
        buf.finish_rewind();

        // Now at s3, has_wrapped still true, len=2
        // Rewindable: s3, s2, s1 = 3 states
        buf.begin_rewind();
        let mut results = Vec::new();
        while let Some(state) = buf.step_back() {
            results.push(state.to_vec());
        }
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], vec![3, 13]);
        assert_eq!(results[1], vec![2, 12]);
        assert_eq!(results[2], vec![1, 11]);
    }

    #[test]
    fn test_finish_rewind_then_push() {
        let mut buf = new_small_buffer(10);

        buf.push(&[1, 1]);
        buf.push(&[2, 2]);
        buf.push(&[3, 3]);
        buf.push(&[4, 4]);

        buf.begin_rewind();
        assert_eq!(buf.step_back().unwrap(), &[4, 4]);
        assert_eq!(buf.step_back().unwrap(), &[3, 3]);
        buf.finish_rewind();

        buf.push(&[10, 10]);
        buf.push(&[11, 11]);

        buf.begin_rewind();
        let mut results = Vec::new();
        while let Some(state) = buf.step_back() {
            results.push(state.to_vec());
        }

        assert_eq!(results[0], vec![11, 11]);
        assert_eq!(results[1], vec![10, 10]);
        assert_eq!(results[2], vec![3, 3]);
        assert_eq!(results[3], vec![2, 2]);
        assert_eq!(results[4], vec![1, 1]);
    }

    #[test]
    fn test_wrap_around() {
        let mut buf = new_small_buffer(5);

        for i in 0..7u8 {
            buf.push(&[i, i + 100]);
        }

        buf.begin_rewind();
        let mut results = Vec::new();
        while let Some(state) = buf.step_back() {
            results.push(state.to_vec());
        }

        // 7 pushes in 5-slot buffer, has_wrapped: 5 deltas + prev = 6 states
        assert_eq!(results.len(), 6);
        assert_eq!(results[0], vec![6, 106]);
        assert_eq!(results[1], vec![5, 105]);
        assert_eq!(results[2], vec![4, 104]);
        assert_eq!(results[3], vec![3, 103]);
        assert_eq!(results[4], vec![2, 102]);
        assert_eq!(results[5], vec![1, 101]);
    }

    #[test]
    fn test_step_back_into() {
        let mut buf = RewindBuffer::new();
        buf.push(&[1, 2, 3]);
        buf.push(&[4, 5, 6]);

        buf.begin_rewind();
        let mut dest = Vec::new();
        assert!(buf.step_back_into(&mut dest));
        assert_eq!(dest, vec![4, 5, 6]);
        assert!(buf.step_back_into(&mut dest));
        assert_eq!(dest, vec![1, 2, 3]);
        assert!(!buf.step_back_into(&mut dest));
    }

    #[test]
    fn test_clear() {
        let mut buf = RewindBuffer::new();
        buf.push(&[1, 2, 3]);
        buf.push(&[4, 5, 6]);
        buf.clear();
        assert!(buf.is_empty());
        buf.begin_rewind();
        assert!(buf.step_back().is_none());
    }

    #[test]
    fn test_empty_rewind() {
        let mut buf = RewindBuffer::new();
        buf.begin_rewind();
        assert!(buf.step_back().is_none());
    }

    #[test]
    fn test_single_push() {
        let mut buf = RewindBuffer::new();
        buf.push(&[42, 99]);
        buf.begin_rewind();
        let got = buf.step_back().unwrap().to_vec();
        assert_eq!(got, vec![42, 99]);
        assert!(buf.step_back().is_none());
    }
}
