const CODE_START_ADDR: u16 = 0x400;

const NEGATIVE_BIT: u8 = 0b10000000;
const OVERFLOW_BIT: u8 = 0b01000000;
const ZERO_BIT: u8 = 0b00000010;
const CARRY_BIT: u8 = 0b00000001;

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

    pub fn carry_flag(&self) -> bool {
        (self.status & CARRY_BIT) == 1
    }

    pub fn negative_flag(&self) -> bool {
        (self.status & NEGATIVE_BIT) == 128
    }

    pub fn overflow_flag(&self) -> bool {
        (self.status & OVERFLOW_BIT) == 64
    }

    pub fn zero_flag(&self) -> bool {
        (self.status & ZERO_BIT) == 2
    }

    pub fn add_to_a_with_carry(&mut self, operand: u8, from_mem: bool) {
        let val: u32 = self.a as u32 + operand as u32;

        if val > u8::max_value() as u32 - 1 {
            if from_mem {
                self.a = 0;
                self.status = self.status | ZERO_BIT;
            } else {
                self.a = (val % u8::max_value() as u32) as u8 - 1;
            }
            self.status = self.status | CARRY_BIT;
        } else {
            self.a = val as u8;
        }

        if (self.a >> 7) == 1 {
            self.status = self.status | NEGATIVE_BIT;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_to_a_with_carry() {
        let mut cpu: Cpu = Cpu::new();

        cpu.add_to_a_with_carry(1, false);
        assert_eq!(1, cpu.a);
        assert_eq!(false, cpu.carry_flag());
        assert_eq!(false, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();
        cpu.a = 100;
        cpu.add_to_a_with_carry(100, false);
        assert_eq!(200, cpu.a);
        assert_eq!(false, cpu.carry_flag());
        assert_eq!(true, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();
        cpu.a = 0xc0;
        cpu.add_to_a_with_carry(0xc4, false);
        assert_eq!(0x84, cpu.a);
        assert_eq!(true, cpu.carry_flag());
        assert_eq!(true, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();
        cpu.a = 0xc0;
        cpu.add_to_a_with_carry(0xc4, true);
        assert_eq!(0x0, cpu.a);
        assert_eq!(true, cpu.carry_flag());
        assert_eq!(true, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());
    }
}
