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
    shift: u8,
    strobe: bool,
    polls: u64,
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}

impl Controller {
    pub fn new() -> Controller {
        Controller {
            status: 0,
            shift: 0,
            strobe: false,
            polls: 0,
        }
    }

    pub fn set_strobe(&mut self, on: bool) {
        if self.strobe && !on {
            self.shift = self.status;
            self.polls = 0;
        }
        self.strobe = on;
    }

    pub fn poll(&mut self) -> u8 {
        if self.strobe {
            return self.status & 1;
        }
        let bit = if self.polls < 8 {
            (self.shift >> self.polls) & 1
        } else {
            1
        };
        self.polls = self.polls.wrapping_add(1);
        bit
    }

    pub fn set_pressed(&mut self, button: u8) {
        self.status |= button;
    }

    pub fn set_not_pressed(&mut self, button: u8) {
        self.status &= !button;
    }

    pub fn save_status(&self) -> u8 {
        self.status
    }
    pub fn save_polls(&self) -> u64 {
        self.polls
    }
    pub fn save_shift(&self) -> u8 {
        self.shift
    }
    pub fn save_strobe(&self) -> bool {
        self.strobe
    }
    pub fn load_status(&mut self, s: u8) {
        self.status = s;
    }
    pub fn load_polls(&mut self, p: u64) {
        self.polls = p;
    }
    pub fn load_shift(&mut self, s: u8) {
        self.shift = s;
    }
    pub fn load_strobe(&mut self, s: bool) {
        self.strobe = s;
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
        c.set_strobe(true);
        c.set_strobe(false);
        assert_eq!(c.poll(), 1); // A
        assert_eq!(c.poll(), 0); // B
        assert_eq!(c.poll(), 1); // SELECT
        assert_eq!(c.poll(), 0); // START
        assert_eq!(c.poll(), 0);
        assert_eq!(c.poll(), 0);
        assert_eq!(c.poll(), 0);
        assert_eq!(c.poll(), 0);

        // Re-strobe resets the shift register
        c.set_strobe(true);
        c.set_strobe(false);
        assert_eq!(c.poll(), 1); // A again
        assert_eq!(c.poll(), 0);
        assert_eq!(c.poll(), 1); // SELECT again
    }

    #[test]
    fn test_poll_during_strobe() {
        let mut c = Controller::new();
        c.set_pressed(A);
        c.set_pressed(START);
        c.set_strobe(true);
        // While strobe is high, always returns bit 0 (A button)
        assert_eq!(c.poll(), 1);
        assert_eq!(c.poll(), 1);
        assert_eq!(c.poll(), 1);
    }
}
