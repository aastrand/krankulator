pub const A: u8 = 0b0000_0001;
pub const B: u8 = 0b0000_0010;
pub const SELECT: u8 = 0b0000_0100;
pub const START: u8 = 0b0000_1000;
pub const UP: u8 = 0b0001_0000;
pub const DOWN: u8 = 0b0010_0000;
pub const LEFT: u8 = 0b0100_0000;
pub const RIGHT: u8 = 0b1000_0000;

pub struct Controller {
    status: u8,
    polls: u64,
}

impl Controller {
    pub fn new() -> Controller {
        Controller {
            status: 0,
            polls: 0,
        }
    }

    pub fn poll(&mut self) -> u8 {
        let mask = 1 << (self.polls % 8);
        let value = (self.status & mask) >> (self.polls % 8);
        self.polls = self.polls.wrapping_add(1);
        value
    }

    pub fn set_pressed(&mut self, button: u8) {
        self.status |= button;
    }

    pub fn set_not_pressed(&mut self, button: u8) {
        self.status &= !button;
    }

    #[allow(dead_code)] // only used in tests
    pub fn is_pressed(&self, button: u8) -> u8 {
        if self.status & button == button {
            1
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_pressed() {
        let mut c = Controller::new();
        c.set_pressed(A);

        assert_eq!(c.status, A);
    }

    #[test]
    fn test_set_not_pressed() {
        let mut c = Controller::new();
        c.set_pressed(A);
        assert_eq!(c.status, A);

        c.set_not_pressed(A);
        assert_eq!(c.status, 0);
    }

    #[test]
    fn test_is_pressed() {
        let mut c = Controller::new();
        c.set_pressed(A);
        c.set_pressed(SELECT);

        assert_eq!(c.is_pressed(A), 1);
        assert_eq!(c.is_pressed(B), 0);
        assert_eq!(c.is_pressed(SELECT), 1);
        assert_eq!(c.is_pressed(START), 0);
        assert_eq!(c.is_pressed(UP), 0);
        assert_eq!(c.is_pressed(DOWN), 0);
        assert_eq!(c.is_pressed(LEFT), 0);
        assert_eq!(c.is_pressed(RIGHT), 0);
    }

    #[test]
    fn test_poll() {
        let mut c = Controller::new();
        c.set_pressed(A);
        c.set_pressed(SELECT);
        assert_eq!(c.poll(), 1);
        assert_eq!(c.poll(), 0);
        assert_eq!(c.poll(), 1);
        assert_eq!(c.poll(), 0);
        assert_eq!(c.poll(), 0);
        assert_eq!(c.poll(), 0);
        assert_eq!(c.poll(), 0);
        assert_eq!(c.poll(), 0);

        assert_eq!(c.poll(), 1);
        assert_eq!(c.poll(), 0);
        assert_eq!(c.poll(), 1);
        assert_eq!(c.poll(), 0);
        assert_eq!(c.poll(), 0);
        assert_eq!(c.poll(), 0);
        assert_eq!(c.poll(), 0);
        assert_eq!(c.poll(), 0);
    }
}
