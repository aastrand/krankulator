use super::super::{super::util, memory::mapper};

extern crate hex;

pub trait Loader {
    fn load(&mut self, path: &str) -> Box<dyn mapper::MemoryMapper>;
}

pub struct AsciiLoader {}

impl Loader for AsciiLoader {
    fn load(&mut self, path: &str) -> Box<dyn mapper::MemoryMapper> {
        let mut code: Vec<u8> = vec![];

        if let Ok(lines) = util::read_lines(path) {
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

        let mut mapper: Box<dyn mapper::MemoryMapper> =
            Box::new(mapper::IdentityMapper::new(0x600));
        let mut i: u32 = 0;
        for b in code.iter() {
            mapper.write_bus(0x600 + i as usize, *b);
            i += 1;
        }

        mapper
    }
}

pub struct BinLoader {}

impl Loader for BinLoader {
    fn load(&mut self, path: &str) -> Box<dyn mapper::MemoryMapper> {
        let bytes = util::read_bytes(path);

        let mut mapper: Box<dyn mapper::MemoryMapper> =
            Box::new(mapper::IdentityMapper::new(0x400));
        let mut i: u32 = 0;
        for b in bytes.iter() {
            mapper.write_bus(i as usize, *b);
            i += 1;
        }

        mapper
    }
}

pub struct InesLoader {}

impl InesLoader {
    pub fn new() -> Box<InesLoader> {
        Box::new(InesLoader {})
    }
}

const INES_HEADER_SIZE: u32 = 16;
const PRG_BANK_SIZE: usize = 16384;

impl Loader for InesLoader {
    fn load(&mut self, path: &str) -> Box<dyn mapper::MemoryMapper> {
        let bytes = util::read_bytes(path);

        // TODO: create header struct
        let num_prg_blocks = bytes.get(4).unwrap();
        let flags = bytes.get(6).unwrap();
        let mapper = flags >> 4;
        let prg_offset: u32 = INES_HEADER_SIZE + (*flags & 0b0000_01000) as u32 * 64;

        // TODO: read chr blocks

        let mut prg_banks: Vec<Box<[u8; PRG_BANK_SIZE]>> = vec![];

        for b in 0..*num_prg_blocks {
            let mut code: Box<[u8; PRG_BANK_SIZE]> = Box::new([0; PRG_BANK_SIZE]);

            let block_offset: usize =
                prg_offset as usize + (b as u32 * PRG_BANK_SIZE as u32) as usize;
            code[0..PRG_BANK_SIZE]
                .clone_from_slice(&bytes[block_offset..(block_offset + PRG_BANK_SIZE)]);

            prg_banks.push(code);
        }

        let result: Box<dyn mapper::MemoryMapper> = match mapper {
            0 => Box::new(mapper::nrom::NROMMapper::new(
                **prg_banks.get(0).unwrap(),
                Some(*prg_banks.pop().unwrap()),
            )),
            1 => Box::new(mapper::mmc1::MMC1Mapper::new(prg_banks)),
            _ => panic!("Mapper {:X} not implemented!", mapper),
        };

        result
    }
}

#[allow(dead_code)] // only used in tests
pub fn load_ascii(path: &str) -> Box<dyn mapper::MemoryMapper> {
    let mut l: Box<dyn Loader> = Box::new(AsciiLoader {});
    l.load(path)
}

#[allow(dead_code)] // only used in tests
pub fn load_bin(path: &str) -> Box<dyn mapper::MemoryMapper> {
    let mut l: Box<dyn Loader> = Box::new(BinLoader {});
    l.load(path)
}

#[allow(dead_code)] // only used in tests
pub fn load_nes(path: &str) -> Box<dyn mapper::MemoryMapper> {
    let mut l: Box<dyn Loader> = InesLoader::new();
    l.load(path)
}

/*#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_ines() {
        let code = load_nes("input/official_only.nes");
        // TODO
    }

}*/
