use super::memory;

pub mod opcodes;

pub const NEGATIVE_BIT: u8 = 0b10000000;
pub const OVERFLOW_BIT: u8 = 0b01000000;
pub const IGNORE_BIT: u8 = 0b00100000;
pub const BREAK_BIT: u8 = 0b00010000;
pub const DECIMAL_BIT: u8 = 0b00001000;
pub const INTERRUPT_BIT: u8 = 0b00000100;
pub const ZERO_BIT: u8 = 0b00000010;
pub const CARRY_BIT: u8 = 0b00000001;

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

    pub fn and(&mut self, operand: u8) {
        self.a &= operand;
        self.check_negative(self.a);
        self.check_zero(self.a);
    }

    pub fn bit(&mut self, operand: u8) {
        // Bits 7 and 6 of operand are transfered to bit 7 and 6 of SR (N,V);
        let mask: u8 = 0b1100_0000;
        self.status = (self.status & !mask) | (operand & mask);
        // The zeroflag is set to the result of operand AND accumulator.
        self.check_zero(self.a & operand);
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
            (value & 0xff) as u8
        } else {
            self.clear_status_flag(CARRY_BIT);
            (value & 0xff) as u8
        };

        // http://www.righto.com/2012/12/the-6502-overflow-flag-explained.html
        if (operand ^ value) & (self.a ^ value) & 0x80 != 0 {
            self.set_status_flag(OVERFLOW_BIT);
        } else {
            self.clear_status_flag(OVERFLOW_BIT);
        }

        self.a = value;

        self.check_negative(self.a);
        self.check_zero(self.a)
    }

    pub fn sub_from_a_with_carry(&mut self, operand: u8) {
        self.add_to_a_with_carry(operand ^ 255)
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

    pub fn eor(&mut self, operand: u8) {
        self.a = self.a ^ operand;
        self.check_negative(self.a);
        self.check_zero(self.a);
    }

    pub fn asl(&mut self, value: u8) -> u8 {
        // Arithmetic Shift Left
        // ASL shifts all bits left one position.
        // 0 is shifted into bit 0 and the original bit 7 is shifted into the Carry.
        let b7: u8 = value & 0b1000_0000;
        let value = value << 1;

        if b7 == 128 {
            self.set_status_flag(CARRY_BIT);
        } else {
            self.clear_status_flag(CARRY_BIT);
        }

        self.check_negative(value);
        self.check_zero(value);

        value
    }

    pub fn lsr(&mut self, value: u8) -> u8 {
        // Logical Shift Right
        // LSR shifts all bits right one position.
        // 0 is shifted into bit 7 and the original bit 0 is shifted into the Carry.
        let b0: u8 = value & 0b0000_0001;
        let value = value >> 1;

        if b0 == 1 {
            self.set_status_flag(CARRY_BIT);
        } else {
            self.clear_status_flag(CARRY_BIT);
        }

        self.check_negative(value);
        self.check_zero(value);

        value
    }

    pub fn ora(&mut self, value: u8) {
        self.a |= value;
        self.check_negative(self.a);
        self.check_zero(self.a);
    }

    pub fn rol(&mut self, value: u8) -> u8 {
        // ROtate Left
        // ROL shifts all bits left one position.
        // The Carry is shifted into bit 0 and the original bit 7 is shifted into the Carry.
        let b7: u8 = value & 0b1000_0000;
        let value = value << 1;

        let b0 = self.status & CARRY_BIT;

        if b7 == 128 {
            self.set_status_flag(CARRY_BIT);
        } else {
            self.clear_status_flag(CARRY_BIT);
        }
        let value = (value & !0b0000_0001) | (b0 & 0b0000_0001);

        self.check_negative(value);
        self.check_zero(value);

        value
    }

    pub fn ror(&mut self, value: u8) -> u8 {
        // ROtate Right
        // ROR shifts all bits right one position.
        // The Carry is shifted into bit 7 and the original bit 0 is shifted into the Carry.
        let b0: u8 = value & 0b0000_0001;
        let value = value >> 1;

        let b7 = (self.status & CARRY_BIT) << 7;

        if b0 == 1 {
            self.set_status_flag(CARRY_BIT);
        } else {
            self.clear_status_flag(CARRY_BIT);
        }
        let value = (value & !0b1000_0000) | (b7 & 0b1000_0000);

        self.check_negative(value);
        self.check_zero(value);

        value
    }

    pub fn register_str(&self) -> String {
        // A:00 X:00 Y:00 P:26 SP:FB
        format!(
            "A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X}",
            self.a, self.x, self.y, self.status, self.sp
        )
    }

    pub fn status_str(&self) -> String {
        format!(
            "\tN:{} V:{} Z:{} C:{} P:{:#010b}",
            self.negative_flag() as i32,
            self.overflow_flag() as i32,
            self.zero_flag() as i32,
            self.carry_flag() as i32,
            self.status
        )
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
    fn test_and() {
        let mut cpu: Cpu = Cpu::new();
        cpu.a = 0b1000_0001;
        cpu.and(0b1100_0000);
        assert_eq!(cpu.a, 0b1000_0000);
        assert_eq!(cpu.negative_flag(), true);
        assert_eq!(cpu.zero_flag(), false);

        cpu.a = 0b1000_0001;
        cpu.and(0b0100_0000);
        assert_eq!(cpu.a, 0b0000_0000);
        assert_eq!(cpu.negative_flag(), false);
        assert_eq!(cpu.zero_flag(), true);

        cpu.a = 0b0100_0001;
        cpu.and(0b0100_0000);
        assert_eq!(cpu.a, 0b0100_0000);
        assert_eq!(cpu.negative_flag(), false);
        assert_eq!(cpu.zero_flag(), false);
    }

    #[test]
    fn test_bit() {
        let mut cpu: Cpu = Cpu::new();
        cpu.a = 0b1000_0001;
        cpu.bit(0b1100_0000);
        assert_eq!(cpu.negative_flag(), true);
        assert_eq!(cpu.overflow_flag(), true);
        assert_eq!(cpu.zero_flag(), false);

        cpu.a = 0b1000_0001;
        cpu.bit(0b0100_0000);
        assert_eq!(cpu.negative_flag(), false);
        assert_eq!(cpu.overflow_flag(), true);
        assert_eq!(cpu.zero_flag(), true);
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
    fn test_eor() {
        let mut cpu: Cpu = Cpu::new();
        cpu.a = 0b1000_0001;
        cpu.eor(0b1100_0000);
        assert_eq!(cpu.a, 0b0100_0001);
        assert_eq!(cpu.negative_flag(), false);
        assert_eq!(cpu.zero_flag(), false);

        cpu.a = 0b1000_0001;
        cpu.eor(0b0100_0000);
        assert_eq!(cpu.a, 0b1100_0001);
        assert_eq!(cpu.negative_flag(), true);
        assert_eq!(cpu.zero_flag(), false);

        cpu.a = 0b1000_0000;
        cpu.eor(0b1000_0000);
        assert_eq!(cpu.a, 0b0000_0000);
        assert_eq!(cpu.negative_flag(), false);
        assert_eq!(cpu.zero_flag(), true);
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

    #[test]
    fn test_asl() {
        let mut cpu: Cpu = Cpu::new();

        let value = cpu.asl(0);
        assert_eq!(value, 0);
        assert_eq!(false, cpu.carry_flag());
        assert_eq!(true, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();

        let value = cpu.asl(127);
        assert_eq!(value, 254);
        assert_eq!(false, cpu.carry_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();

        let value = cpu.asl(255);
        assert_eq!(value, 254);
        assert_eq!(true, cpu.carry_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();

        let value = cpu.asl(128);
        assert_eq!(value, 0);
        assert_eq!(true, cpu.carry_flag());
        assert_eq!(true, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());
    }

    #[test]
    fn test_lsr() {
        let mut cpu: Cpu = Cpu::new();

        let value = cpu.lsr(0);
        assert_eq!(value, 0);
        assert_eq!(false, cpu.carry_flag());
        assert_eq!(true, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();
        let value = cpu.lsr(1);
        assert_eq!(value, 0);
        assert_eq!(true, cpu.carry_flag());
        assert_eq!(true, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();
        let value = cpu.lsr(255);
        assert_eq!(value, 127);
        assert_eq!(true, cpu.carry_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();
        let value = cpu.lsr(8);
        assert_eq!(value, 4);
        assert_eq!(false, cpu.carry_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());
    }

    #[test]
    fn test_ora() {
        let mut cpu: Cpu = Cpu::new();
        cpu.a = 0b1000_0001;
        cpu.ora(0b1100_0000);
        assert_eq!(cpu.a, 0b1100_0001);
        assert_eq!(cpu.negative_flag(), true);
        assert_eq!(cpu.zero_flag(), false);

        cpu.a = 0;
        cpu.ora(0);
        assert_eq!(cpu.a, 0);
        assert_eq!(cpu.negative_flag(), false);
        assert_eq!(cpu.zero_flag(), true);

        cpu.a = 0b0000_1010;
        cpu.ora(0b0000_0101);
        assert_eq!(cpu.a, 0b0000_1111);
        assert_eq!(cpu.negative_flag(), false);
        assert_eq!(cpu.zero_flag(), false);
    }

    #[test]
    fn test_rol() {
        let mut cpu: Cpu = Cpu::new();

        let value = cpu.rol(0);
        assert_eq!(value, 0);
        assert_eq!(false, cpu.carry_flag());
        assert_eq!(true, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();
        cpu.set_status_flag(CARRY_BIT);
        let value = cpu.rol(0);
        assert_eq!(value, 1);
        assert_eq!(false, cpu.carry_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();
        let value = cpu.rol(1);
        assert_eq!(value, 2);
        assert_eq!(false, cpu.carry_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();
        cpu.set_status_flag(CARRY_BIT);
        let value = cpu.rol(128);
        assert_eq!(value, 1);
        assert_eq!(true, cpu.carry_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();
        let value = cpu.rol(127);
        assert_eq!(value, 254);
        assert_eq!(false, cpu.carry_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();
        cpu.set_status_flag(CARRY_BIT);
        let value = cpu.rol(255);
        assert_eq!(value, 255);
        assert_eq!(true, cpu.carry_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.negative_flag());
    }

    #[test]
    fn test_ror() {
        let mut cpu: Cpu = Cpu::new();

        let value = cpu.ror(0);
        assert_eq!(value, 0);
        assert_eq!(false, cpu.carry_flag());
        assert_eq!(true, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();
        cpu.set_status_flag(CARRY_BIT);
        let value = cpu.ror(0);
        assert_eq!(value, 128);
        assert_eq!(false, cpu.carry_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();
        let value = cpu.ror(1);
        assert_eq!(value, 0);
        assert_eq!(true, cpu.carry_flag());
        assert_eq!(true, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();
        cpu.set_status_flag(CARRY_BIT);
        let value = cpu.ror(1);
        assert_eq!(value, 128);
        assert_eq!(true, cpu.carry_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(true, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();
        let value = cpu.ror(255);
        assert_eq!(value, 127);
        assert_eq!(true, cpu.carry_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());

        let mut cpu: Cpu = Cpu::new();
        let value = cpu.ror(8);
        assert_eq!(value, 4);
        assert_eq!(false, cpu.carry_flag());
        assert_eq!(false, cpu.zero_flag());
        assert_eq!(false, cpu.negative_flag());
    }

    #[test]
    fn test_status_str() {
        let mut cpu = Cpu::new();
        cpu.set_status_flag(255);
        let s = cpu.status_str();

        assert_eq!(s, "\tN:1 V:1 Z:1 C:1 P:0b11111111");
    }

    #[test]
    fn test_register_str() {
        let mut cpu = Cpu::new();
        cpu.a = 0x42;
        cpu.x = 0x47;
        cpu.y = 0x11;
        cpu.sp = 0xab;
        let s = cpu.register_str();

        assert_eq!(s, "A:42 X:47 Y:11 P:20 SP:AB");
    }
}
