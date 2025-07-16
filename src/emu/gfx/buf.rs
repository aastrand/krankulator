const WIDTH: usize = 256;
const HEIGHT: usize = 240;

pub struct Buffer {
    pub data: Vec<u8>,
    pub width: usize,
    #[allow(dead_code)]
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

    pub fn clear(&mut self, background_color: (u8, u8, u8)) {
        for i in (0..self.data.len()).step_by(3) {
            self.data[i] = background_color.0;
            self.data[i + 1] = background_color.1;
            self.data[i + 2] = background_color.2;
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

    pub fn get_pixel(&self, x: usize, y: usize) -> (u8, u8, u8) {
        let base = y * 3 * self.width + x * 3;
        if base + 2 < self.data.len() {
            (self.data[base], self.data[base + 1], self.data[base + 2])
        } else {
            (0, 0, 0) // Return black if out of bounds
        }
    }
}
