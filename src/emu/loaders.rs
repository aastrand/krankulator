use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;

extern crate hex;

pub trait Loader {
    fn load(&self, path: &str) -> Vec<u8>;
    fn offset(&self) -> u16;
    fn code_start(&self) -> u16;
}

pub struct AsciiLoader {}

impl Loader for AsciiLoader {
    fn load(&self, path: &str) -> Vec<u8> {
        if !Path::new(path).exists() {
            panic!("File does not exist: {}", path);
        }

        let mut code: Vec<u8> = vec![];

        if let Ok(lines) = read_lines(path) {
            for line in lines {
                if let Ok(content) = line {
                    let content = content.trim();
                    let content = content.split(';').nth(0).unwrap();
                    if content.len() > 0 {
                        for byte in content.split(' ') {
                            let mut decoded = hex::decode(byte).expect("Decoding failed");
                            code.append(&mut decoded);
                        }
                    }
                }
            }
        }

        code
    }

    fn offset(&self) -> u16 {
        0x600
    }

    fn code_start(&self) -> u16 {
        0x600
    }
}

pub struct BinLoader {}

impl Loader for BinLoader {
    fn load(&self, path: &str) -> Vec<u8> {
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

    fn offset(&self) -> u16 {
        0
    }

    fn code_start(&self) -> u16 {
        0x400
    }
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

pub struct InesLoader {}

const INES_HEADER_SIZE: u32 = 16;

impl Loader for InesLoader {
    fn load(&self, path: &str)  -> Vec<u8> {
        let bytes = BinLoader{}.load(path);
        let mut code: Vec<u8> = vec![];
        for i in 0 .. INES_HEADER_SIZE {
            println!("header byte {}: 0x{:x}", i, bytes.get(i as usize).unwrap());
        }

        let num_prg_blocks = bytes.get(4).unwrap();
        println!("num_prg_blocks: 0x{:x}", num_prg_blocks);

        let flags = bytes.get(6).unwrap();
        println!("flags: {:#010b}", flags);

        let mapper = flags >> 4;
        println!("mapper: 0x{:x}", mapper);

        let prg_offset: u32 = INES_HEADER_SIZE + (*flags & 0b0000_01000) as u32 * 64;
        println!("prg_offset: 0x{:x}", prg_offset);

        for i in prg_offset .. prg_offset as u32 + (*num_prg_blocks as u32) * 16384 {
            code.push(*bytes.get(i as usize).unwrap());
        }

        code
    }

    fn offset(&self) -> u16 {
        0
    }

    fn code_start(&self) -> u16 {
        0 // TODO: solve
    }
}

#[allow(dead_code)] // only used in tests
pub fn load_ascii(path: &str) -> Vec<u8> {
    let l: & dyn Loader = &AsciiLoader{};
    l.load(path)
}

#[allow(dead_code)] // only used in tests
pub fn load_bin(path: &str) -> Vec<u8> {
    let l: & dyn Loader = &BinLoader{};
    l.load(path)
}

#[allow(dead_code)] // only used in tests
pub fn load_nes(path: &str) -> Vec<u8> {
    let l: & dyn Loader = &InesLoader{};
    l.load(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_ines() {
        let code = load_nes("input/official_only.nes");
        assert_eq!(code.len(), 16 * 16 * 1024);
    }

}