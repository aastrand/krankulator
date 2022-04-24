const WIDTH: usize = 256;
const HEIGHT: usize = 240;

pub struct Buffer {
    pub data: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

impl Buffer {
    pub fn new() -> Self {
        Buffer {
            data: vec![0; (WIDTH) * (HEIGHT) * 3],
            width: WIDTH,
            height: HEIGHT,
        }
    }

    pub fn set_pixel(&mut self, x: usize, y: usize, rgb: (u8, u8, u8)) {
        let base = y * 3 * self.width + x * 3;
        if base + 2 < self.data.len() {
            self.data[base] = rgb.0;
            self.data[base + 1] = rgb.1;
            self.data[base + 2] = rgb.2;
        }
    }
}
