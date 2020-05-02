const CODE_START_ADDR: u16 = 0x400;

pub struct Cpu {
    pub pc: u16,

    pub a: u8,
    pub x: u8,
    pub y: u8,

    pub stack: u8,
    status: u8
}

impl Cpu {

    pub fn new() -> Cpu {
        Cpu{pc: CODE_START_ADDR, a: 0, x: 0, y: 0, stack: 0, status: 0}
    }

    pub fn overflow_flag(&self) -> bool {
        (self.status & 0b01000000) == 64
    }

    pub fn add_to_a_with_carry(&mut self, operand: u8) {
        let val: u32 = self.a as u32 + operand as u32;
        if val > u8::max_value() as u32 - 1 {
            self.a = (val % u8::max_value() as u32) as u8;
            self.status = self.status | 0b01000000;
        } else {
            self.a = val as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_to_a_with_carry() {
        let mut cpu: Cpu = Cpu::new();

        cpu.add_to_a_with_carry(1);
        assert_eq!(1, cpu.a);
        assert_eq!(false, cpu.overflow_flag());

        cpu.a = 100;
        cpu.add_to_a_with_carry(100);
        assert_eq!(200, cpu.a);
        assert_eq!(false, cpu.overflow_flag());

        cpu.a = 200;
        cpu.add_to_a_with_carry(200);
        assert_eq!(145, cpu.a);
        assert_eq!(true, cpu.overflow_flag());
    }
}
