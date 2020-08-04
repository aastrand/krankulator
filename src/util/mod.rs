use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;

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

pub fn read_bytes(path: &str) -> Vec<u8> {
    if !Path::new(path).exists() {
        panic!("File does not exist: {}", path);
    }

    let result = std::fs::read(path);
    match result {
        Ok(code) => {
            return code;
        }
        _ => {
            panic!("Error while parsing binary file {}", path);
        }
    }
}

pub fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

pub fn filename(s: &str) -> &str {
    let i = if let Some(i) = s.rfind("/") {
        i + 1
    } else {
        if let Some(i) = s.rfind("\\") {
            i + 1
        } else {
            0
        }
    };

    &s[i..]
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

    #[test]
    fn test_filename() {
        assert_eq!(filename(&format!("")), &format!(""));
        assert_eq!(
            filename(&format!("input/nes/test.rom")),
            &format!("test.rom")
        );
        assert_eq!(
            filename(&format!("D:\\Documents\\roms\\nes\\donkey kong.nes")),
            &format!("donkey kong.nes")
        );
    }
}
