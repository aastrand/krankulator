pub const NEGATIVE_BIT: u8 = 0b10000000;
pub const OVERFLOW_BIT: u8 = 0b01000000;
pub const IGNORE_BIT: u8 = 0b00100000;
pub const BREAK_BIT: u8 = 0b00010000;
pub const DECIMAL_BIT: u8 = 0b00001000;
pub const INTERRUPT_BIT: u8 = 0b00000100;
pub const ZERO_BIT: u8 = 0b00000010;
pub const CARRY_BIT: u8 = 0b00000001;

use super::memory;

pub struct Cpu {
    pub pc: u16,

    pub a: u8,
    pub x: u8,
    pub y: u8,

    pub sp: u8,

    pub status: u8,
}

impl Cpu {
    // TODO: not sure Cpu should use super::memory
    // maybe it's better to add these as parameters
    pub fn new() -> Cpu {
        Cpu {
            pc: memory::CODE_START_ADDR,
            a: 0,
            x: 0,
            y: 0,
            sp: memory::STACK_START_ADDR as u8,
            status: 0b0010_0000, // ignored bits set
        }
    }

    pub fn carry_flag(&self) -> bool {
        (self.status & CARRY_BIT) == 1
    }

    #[allow(dead_code)] // only used in tests
    pub fn decimal_flag(&self) -> bool {
        (self.status & DECIMAL_BIT) == 8
    }

    #[allow(dead_code)] // only used in tests
    pub fn interrupt_flag(&self) -> bool {
        (self.status & INTERRUPT_BIT) == 4
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
        let value: i32 = self.a as i32 + operand as i32 + cin as i32;
        let value = if value > u8::max_value() as i32 {
            self.set_status_flag(CARRY_BIT);
            value % u8::max_value() as i32 - 1
        } else {
            self.clear_status_flag(CARRY_BIT);
            value
        };

        let value: u8 = value as u8;

        // http://www.righto.com/2012/12/the-6502-overflow-flag-explained.html
        if (operand ^ value) & (self.a ^ value) & 0x80 != 0 {
            self.set_status_flag(OVERFLOW_BIT);
        } else {
            self.clear_status_flag(OVERFLOW_BIT);
        }

        self.a = value as u8;

        self.check_negative(self.a);
        self.check_zero(self.a)
    }

    pub fn sub_from_a_with_carry(&mut self, operand: u8) {
        let cin = if self.carry_flag() { 0 } else { 1 };
        let value: i32 = (self.a as i32 - operand as i32) - cin;

        if value < 0 {
            // Set borrow, which is !carry
            self.clear_status_flag(CARRY_BIT);
        } else {
            self.set_status_flag(CARRY_BIT);
        }

        // Handle overflow
        let value = if value < -256 as i32 {
            value + u8::max_value() as i32
        } else {
            value
        };

        // http://www.righto.com/2012/12/the-6502-overflow-flag-explained.html
        if ((255 - operand) ^ value as u8) & ((self.a) ^ value as u8) & 0x80 != 0 {
            self.set_status_flag(OVERFLOW_BIT);
        } else {
            self.clear_status_flag(OVERFLOW_BIT);
        }

        self.a = value as u8;

        self.check_negative(self.a);
        self.check_zero(self.a);

        // TODO: this should be enough, but some testcases fail :|
        //self.add_to_a_with_carry(u8::max_value() - operand)
    }

    pub fn check_negative(&mut self, value: u8) {
        // After most instructions that have a value result, this flag will contain bit 7 of that result.
        if (value >> 7) == 1 {
            self.set_status_flag(NEGATIVE_BIT);
        } else {
            self.clear_status_flag(NEGATIVE_BIT);
        }
    }

    pub fn check_zero(&mut self, value: u8) {
        if value == 0 {
            self.set_status_flag(ZERO_BIT);
        } else {
            self.clear_status_flag(ZERO_BIT);
        }
    }

    pub fn compare(&mut self, register_value: u8, operand: u8) {
        // Compare sets flags as if a subtraction had been carried out.
        // If the value in the accumulator is equal or greater than the compared value, the Carry will be set.
        // The equal (Z) and negative (N) flags will be set based on equality or lack thereof and the sign (i.e. A>=$80) of the accumulator.
        if register_value >= operand {
            self.set_status_flag(CARRY_BIT);
            if register_value == operand {
                self.set_status_flag(ZERO_BIT);
            } else {
                self.clear_status_flag(ZERO_BIT);
            }
            self.clear_status_flag(NEGATIVE_BIT);
        } else {
            self.clear_status_flag(CARRY_BIT);
            self.clear_status_flag(ZERO_BIT);
            self.set_status_flag(NEGATIVE_BIT);
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
        cpu.set_status_flag(CARRY_BIT);
        cpu.sub_from_a_with_carry(240);
        assert_eq!(96, cpu.a);
        assert_eq!(false, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.carry_flag());

        cpu.a = 80;
        cpu.set_status_flag(CARRY_BIT);
        cpu.sub_from_a_with_carry(176);
        assert_eq!(160, cpu.a);
        assert_eq!(true, cpu.negative_flag());
        assert_eq!(true, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.carry_flag());

        cpu.a = 80;
        cpu.set_status_flag(CARRY_BIT);
        cpu.sub_from_a_with_carry(112);
        assert_eq!(224, cpu.a);
        assert_eq!(true, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.carry_flag());

        cpu.a = 80;
        cpu.set_status_flag(CARRY_BIT);
        cpu.sub_from_a_with_carry(48);
        assert_eq!(32, cpu.a);
        assert_eq!(false, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.carry_flag());

        cpu.a = 208;
        cpu.set_status_flag(CARRY_BIT);
        cpu.sub_from_a_with_carry(240);
        assert_eq!(224, cpu.a);
        assert_eq!(true, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.carry_flag());

        cpu.a = 208;
        cpu.set_status_flag(CARRY_BIT);
        cpu.sub_from_a_with_carry(176);
        assert_eq!(32, cpu.a);
        assert_eq!(false, cpu.negative_flag());
        assert_eq!(false, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.carry_flag());

        cpu.a = 208;
        cpu.set_status_flag(CARRY_BIT);
        cpu.sub_from_a_with_carry(112);
        assert_eq!(96, cpu.a);
        assert_eq!(false, cpu.negative_flag());
        assert_eq!(true, cpu.overflow_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.carry_flag());

        cpu.a = 208;
        cpu.set_status_flag(CARRY_BIT);
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

    #[test]
    fn test_compare() {
        let mut cpu: Cpu = Cpu::new();

        cpu.compare(0, 1);
        assert_eq!(false, cpu.carry_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.negative_flag());

        cpu.compare(1, 1);
        assert_eq!(true, cpu.carry_flag());
        assert_eq!(true, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());

        cpu.compare(2, 1);
        assert_eq!(true, cpu.carry_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());
    }
}
