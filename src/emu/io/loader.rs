use super::super::{super::util, memory::mapper};

extern crate hex;

pub trait Loader {
    fn load(&self, path: &str) -> Box<dyn mapper::MemoryMapper>;
    fn code_start(&self) -> u16;
}

pub struct AsciiLoader {}

impl Loader for AsciiLoader {
    fn load(&self, path: &str) -> Box<dyn mapper::MemoryMapper> {
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

        let mut mapper: Box<dyn mapper::MemoryMapper> = Box::new(mapper::IdentityMapper::new());
        let mut i: u32 = 0;
        for b in code.iter() {
            mapper.write_bus(0x600 + i as u16, *b);
            i += 1;
        }

        mapper
    }

    fn code_start(&self) -> u16 {
        0x600
    }
}

pub struct BinLoader {}

impl Loader for BinLoader {
    fn load(&self, path: &str) -> Box<dyn mapper::MemoryMapper> {
        let bytes = util::read_bytes(path);

        let mut mapper: Box<dyn mapper::MemoryMapper> = Box::new(mapper::IdentityMapper::new());
        let mut i: u32 = 0;
        for b in bytes.iter() {
            mapper.write_bus(i as u16, *b);
            i += 1;
        }

        mapper
    }

    fn code_start(&self) -> u16 {
        0x400
    }
}

pub struct InesLoader {}

const INES_HEADER_SIZE: u32 = 16;
const PRG_BANK_SIZE: usize = 16384;

impl Loader for InesLoader {
    fn load(&self, path: &str) -> Box<dyn mapper::MemoryMapper> {
        let bytes = util::read_bytes(path);

        // TODO: create header struct
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

        let mut prg_banks: Vec<Box<[u8; PRG_BANK_SIZE]>> = vec![];

        for b in 0..*num_prg_blocks {
            let mut code: Box<[u8; PRG_BANK_SIZE]> = Box::new([0; PRG_BANK_SIZE]);

            let block_offset: usize = prg_offset as usize + (b as u32 * PRG_BANK_SIZE as u32) as usize;
            code[0..PRG_BANK_SIZE].clone_from_slice(&bytes[block_offset..(block_offset+PRG_BANK_SIZE)]);

            prg_banks.push(code);
        }

        // TODO: read mapper byte and get correct one
        let mapper = mapper::NROMMapper::new(**prg_banks.get(0).unwrap(), None);

        Box::new(mapper)
    }

    fn code_start(&self) -> u16 {
        0xc000 // TODO: fix interrupt vectors
    }
}

#[allow(dead_code)] // only used in tests
pub fn load_ascii(path: &str) -> Box<dyn mapper::MemoryMapper> {
    let l: & dyn Loader = &AsciiLoader{};
    l.load(path)
}

#[allow(dead_code)] // only used in tests
pub fn load_bin(path: &str) -> Box<dyn mapper::MemoryMapper> {
    let l: & dyn Loader = &BinLoader{};
    l.load(path)
}

#[allow(dead_code)] // only used in tests
pub fn load_nes(path: &str) -> Box<dyn mapper::MemoryMapper> {
    let l: & dyn Loader = &InesLoader{};
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