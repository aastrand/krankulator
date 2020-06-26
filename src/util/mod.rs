pub fn strip_hex_str(s: &str) -> &str {
    if s.len() > 1 {
       match &s[..2] {
           "0x" => &s[2..],
           _ => s,
       }
   } else {
       s
   }
}

pub fn hex_str_to_u16(s: &str) -> Result<u16, std::num::ParseIntError> {
   let s = strip_hex_str(s);
   u16::from_str_radix(s, 16)
}

pub fn hex_str_to_u8(s: &str) -> Result<u8, std::num::ParseIntError> {
   let s = strip_hex_str(s);
   u8::from_str_radix(s, 16)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_hex_input() {
        assert_eq!(strip_hex_str("1"), "1");
        assert_eq!(strip_hex_str("12"), "12");
        assert_eq!(strip_hex_str("0x"), "");
        assert_eq!(strip_hex_str("0x43"), "43");
    }

    #[test]
    fn test_hex_str_to_u16() {
        assert_eq!(hex_str_to_u16("1").unwrap(), 1);
        assert_eq!(hex_str_to_u16("12").unwrap(), 0x12);
        assert_eq!(hex_str_to_u16("0x4711").unwrap(), 0x4711);
        assert_eq!(hex_str_to_u16("0x").is_ok(), false);
    }

    #[test]
    fn test_hex_str_to_u8() {
        assert_eq!(hex_str_to_u8("1").unwrap(), 1);
        assert_eq!(hex_str_to_u8("12").unwrap(), 0x12);
        assert_eq!(hex_str_to_u8("0x43").unwrap(), 0x43);
        assert_eq!(hex_str_to_u8("0x4711").is_ok(), false);
        assert_eq!(hex_str_to_u8("0x").is_ok(), false);
    }
}
