const CODE_START_ADDR: u16 = 0x400;

pub const NEGATIVE_BIT: u8 = 0b10000000;
pub const OVERFLOW_BIT: u8 = 0b01000000;
pub const ZERO_BIT: u8 = 0b00000010;
pub const CARRY_BIT: u8 = 0b00000001;

pub struct Cpu {
    pub pc: u16,

    pub a: u8,
    pub x: u8,
    pub y: u8,

    pub sp: u8,
    status: u8
}

impl Cpu {

    pub fn new() -> Cpu {
        Cpu{pc: CODE_START_ADDR, a: 0, x: 0, y: 0, sp: 0, status: 0}
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

    pub fn set_status_flag(&mut self, flag: u8) {
        self.status |= flag;
    }

    pub fn clear_status_flag(&mut self, flag: u8) {
        self.status &= !flag;
    }

    pub fn add_to_a_with_carry(&mut self, operand: u8) {
        let cin = if self.carry_flag() { 1 } else { 0 };
        let value: u32 = self.a as u32 + operand as u32 + cin as u32;
        let value = if value > u8::max_value() as u32 {
            self.set_status_flag(CARRY_BIT);
            value % u8::max_value() as u32 - 1
        } else {
            self.clear_status_flag(CARRY_BIT);
            value
        };

        let value: u8 = value as u8;

        // http://www.righto.com/2012/12/the-6502-overflow-flag-explained.html
        if (operand^value)&(self.a^value)&0x80 != 0 {
            self.set_status_flag(OVERFLOW_BIT);
        } else {
            self.clear_status_flag(OVERFLOW_BIT);
        }

        self.a = value as u8;

        self.check_negative(self.a);
        self.check_zero(self.a)
    }

    pub fn sub_from_a_with_carry(&mut self, operand: u8) {
        let cin = if self.carry_flag() { 1 } else { 0 };
        let value: u8 =
        if operand > self.a {
            // Set borrow, which is !carry
            self.clear_status_flag(CARRY_BIT);
            u8::max_value() - (operand - self.a) + 1
        } else {
            self.set_status_flag(CARRY_BIT);
            self.a - operand
        } - cin;

        // http://www.righto.com/2012/12/the-6502-overflow-flag-explained.html
        if ((255-operand)^value)&((self.a)^value)&0x80 != 0 {
            self.set_status_flag(OVERFLOW_BIT);
        } else {
            self.clear_status_flag(OVERFLOW_BIT);
        }

        self.a = value;

        self.check_negative(self.a);
        self.check_zero(self.a)
    }

    pub fn check_negative(&mut self, value: u8) {
        // After most instructions that have a value result, this flag will contain bit 7 of that result.
        if (value >> 7) == 1 {
            self.status |= NEGATIVE_BIT;
        } else {
            self.status &= !NEGATIVE_BIT;
        }
    }

    pub fn check_zero(&mut self, value: u8) {
        if value == 0 {
            self.status |= ZERO_BIT;
        } else {
            self.status &= !ZERO_BIT;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_to_a_with_carry() {
        // http://www.righto.com/2012/12/the-6502-overflow-flag-explained.html
        let mut cpu: Cpu = Cpu::new();

        cpu.a = 80;
        cpu.add_to_a_with_carry(16);
        assert_eq!(96, cpu.a);
        assert_eq!(false, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.carry_flag());

        cpu.a = 80;
        cpu.add_to_a_with_carry(80);
        assert_eq!(160, cpu.a);
        assert_eq!(true, cpu.negative_flag());
        assert_eq!(true, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.carry_flag());

        cpu.a = 80;
        cpu.add_to_a_with_carry(144);
        assert_eq!(224, cpu.a);
        assert_eq!(true, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.carry_flag());

        cpu.a = 80;
        cpu.add_to_a_with_carry(208);
        assert_eq!(32, cpu.a);
        assert_eq!(false, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.carry_flag());

        cpu.a = 208;
        cpu.clear_status_flag(CARRY_BIT);
        cpu.add_to_a_with_carry(16);
        assert_eq!(224, cpu.a);
        assert_eq!(true, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.carry_flag());

        cpu.a = 208;
        cpu.add_to_a_with_carry(80);
        assert_eq!(32, cpu.a);
        assert_eq!(false, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.carry_flag());

        cpu.a = 208;
        cpu.clear_status_flag(CARRY_BIT);
        cpu.add_to_a_with_carry(144);
        assert_eq!(96, cpu.a);
        assert_eq!(false, cpu.negative_flag());
        assert_eq!(true, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.carry_flag());

        cpu.a = 208;
        cpu.clear_status_flag(CARRY_BIT);
        cpu.add_to_a_with_carry(208);
        assert_eq!(160, cpu.a);
        assert_eq!(true, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.carry_flag());
    }

    #[test]
    fn test_sub_from_a_with_carry() {
        // http://www.righto.com/2012/12/the-6502-overflow-flag-explained.html
        let mut cpu: Cpu = Cpu::new();

        cpu.a = 80;
        cpu.sub_from_a_with_carry(240);
        assert_eq!(96, cpu.a);
        assert_eq!(false, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.carry_flag());

        cpu.a = 80;
        cpu.sub_from_a_with_carry(176);
        assert_eq!(160, cpu.a);
        assert_eq!(true, cpu.negative_flag());
        assert_eq!(true, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.carry_flag());

        cpu.a = 80;
        cpu.sub_from_a_with_carry(112);
        assert_eq!(224, cpu.a);
        assert_eq!(true, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.carry_flag());

        cpu.a = 80;
        cpu.sub_from_a_with_carry(48);
        assert_eq!(32, cpu.a);
        assert_eq!(false, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.carry_flag());

        cpu.a = 208;
        cpu.clear_status_flag(CARRY_BIT);
        cpu.sub_from_a_with_carry(240);
        assert_eq!(224, cpu.a);
        assert_eq!(true, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.carry_flag());

        cpu.a = 208;
        cpu.sub_from_a_with_carry(176);
        assert_eq!(32, cpu.a);
        assert_eq!(false, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.carry_flag());

        cpu.a = 208;
        cpu.clear_status_flag(CARRY_BIT);
        cpu.sub_from_a_with_carry(112);
        assert_eq!(96, cpu.a);
        assert_eq!(false, cpu.negative_flag());
        assert_eq!(true, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.carry_flag());

        cpu.a = 208;
        cpu.clear_status_flag(CARRY_BIT);
        cpu.sub_from_a_with_carry(48);
        assert_eq!(160, cpu.a);
        assert_eq!(true, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.carry_flag());
    }

    #[test]
    fn test_check_negative() {
        let mut cpu: Cpu = Cpu::new();
        assert_eq!(false, cpu.negative_flag());
        cpu.check_negative(8);
        assert_eq!(false, cpu.negative_flag());
        cpu.check_negative(255);
        assert_eq!(true, cpu.negative_flag());
        cpu.check_negative(8);
        assert_eq!(false, cpu.negative_flag());
    }

    #[test]
    fn test_check_zero() {
        let mut cpu: Cpu = Cpu::new();
        assert_eq!(false, cpu.zero_flag());
        cpu.check_zero(8);
        assert_eq!(false, cpu.zero_flag());
        cpu.check_zero(0);
        assert_eq!(true, cpu.zero_flag());
        cpu.check_zero(8);
        assert_eq!(false, cpu.zero_flag());
    }

    #[test]
    fn test_status_flag() {
        let mut cpu: Cpu = Cpu::new();

        assert_eq!(false, cpu.negative_flag());
        cpu.set_status_flag(NEGATIVE_BIT);
        assert_eq!(true, cpu.negative_flag());
        cpu.clear_status_flag(NEGATIVE_BIT);
        assert_eq!(false, cpu.negative_flag());

        assert_eq!(false, cpu.overflow_flag());
        cpu.set_status_flag(OVERFLOW_BIT);
        assert_eq!(true, cpu.overflow_flag());
        cpu.clear_status_flag(OVERFLOW_BIT);
        assert_eq!(false, cpu.overflow_flag());

        assert_eq!(false, cpu.zero_flag());
        cpu.set_status_flag(ZERO_BIT);
        assert_eq!(true, cpu.zero_flag());
        cpu.clear_status_flag(ZERO_BIT);
        assert_eq!(false, cpu.zero_flag());

        assert_eq!(false, cpu.carry_flag());
        cpu.set_status_flag(CARRY_BIT);
        assert_eq!(true, cpu.carry_flag());
        cpu.clear_status_flag(CARRY_BIT);
        assert_eq!(false, cpu.carry_flag());
    }
}
