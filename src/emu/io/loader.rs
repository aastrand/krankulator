use super::super::memory;
use super::super::{super::util, memory::mapper};
use crate::emu::memory::mapper::mmc3::MMC3Mapper;

extern crate hex;

pub trait Loader {
    fn load(&self, path: &str) -> Result<Box<dyn memory::MemoryMapper>, String>;
}

pub struct AsciiLoader {}

impl Loader for AsciiLoader {
    fn load(&self, path: &str) -> Result<Box<dyn memory::MemoryMapper>, String> {
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

        let mut mapper: Box<dyn memory::MemoryMapper> =
            Box::new(memory::IdentityMapper::new(0x600));
        let mut i: u16 = 0;
        for b in code.iter() {
            mapper.cpu_write(0x600 + i as u16, *b);
            i += 1;
        }

        Ok(mapper)
    }
}

pub struct BinLoader {}

impl Loader for BinLoader {
    fn load(&self, path: &str) -> Result<Box<dyn memory::MemoryMapper>, String> {
        let bytes = util::read_bytes(path)?;

        let mut mapper: Box<dyn memory::MemoryMapper> =
            Box::new(memory::IdentityMapper::new(0x400));
        let mut i: u32 = 0;
        for b in bytes.iter() {
            mapper.cpu_write(i as u16, *b);
            i += 1;
        }

        Ok(mapper)
    }
}

pub struct InesLoader {}

impl InesLoader {
    pub fn new() -> Box<InesLoader> {
        Box::new(InesLoader {})
    }
}

const INES_HEADER_SIZE: usize = 16;
const PRG_BANK_SIZE: usize = 16384;
pub const CHR_BANK_SIZE: usize = 8192;

impl Loader for InesLoader {
    fn load(&self, path: &str) -> Result<Box<dyn memory::MemoryMapper>, String> {
        let bytes = util::read_bytes(path)?;

        //0-3: Constant $4E $45 $53 $1A ("NES" followed by MS-DOS end-of-file)
        if bytes[0] != 0x4E || bytes[1] != 0x45 || bytes[2] != 0x53 || bytes[3] != 0x1a {
            return Err(format!("ERROR: Missing iNES header magic numbers"));
        }

        // A file is a NES 2.0 ROM image file if it begins with "NES<EOF>" (same as iNES) and,
        // additionally, the byte at offset 7 has bit 2 clear and bit 3 set:
        let is_nes2_header = { bytes[7] & 0b0000_1100 == 0b0000_1000 };

        if is_nes2_header {
            println!("Header has NES 2.0 extension");
        }

        let num_prg_blocks = bytes[4];
        let num_chr_blocks = bytes[5];
        let flags = bytes[6];
        let mapper = flags >> 4;
        let prg_ram_units = bytes[8];
        let prg_offset: usize = INES_HEADER_SIZE + (flags & 0b0000_0100) as usize * 512;
        let chr_offset: usize = prg_offset + (num_prg_blocks as usize * PRG_BANK_SIZE);

        let mut prg_banks: Vec<[u8; PRG_BANK_SIZE]> = vec![];

        let chr_ram_size = {
            if is_nes2_header {
                /*
                ---------
                cccc CCCC
                |||| ++++- CHR-RAM size (volatile) shift count
                ++++------ CHR-NVRAM size (non-volatile) shift count
                If the shift count is zero, there is no CHR-(NV)RAM.
                If the shift count is non-zero, the actual size is
                "64 << shift count" bytes, i.e. 8192 bytes for a shift count of 7.
                */
                let ram_shift = bytes[11] & 0b0000_1111;
                let nvram_shift = (bytes[11] & 0b1111_0000) >> 4;
                let shift = std::cmp::max(ram_shift, nvram_shift);

                64 << shift
            } else {
                8192
            }
        };

        for b in 0..num_prg_blocks {
            let mut code = [0; PRG_BANK_SIZE];

            let block_offset: usize =
                prg_offset as usize + (b as u32 * PRG_BANK_SIZE as u32) as usize;
            code[0..PRG_BANK_SIZE]
                .clone_from_slice(&bytes[block_offset..(block_offset + PRG_BANK_SIZE)]);

            prg_banks.push(code);
        }

        println!(
            "Loading {} PRG banks, {} CHR banks, with {} PRG RAM units",
            num_prg_blocks, num_chr_blocks, prg_ram_units
        );

        let mut chr_banks: Vec<[u8; CHR_BANK_SIZE]> = vec![];

        if num_chr_blocks > 0 {
            for b in 0..num_chr_blocks {
                let mut gfx = [0; CHR_BANK_SIZE];

                let block_offset: usize =
                    chr_offset as usize + (b as u32 * CHR_BANK_SIZE as u32) as usize;
                gfx[0..CHR_BANK_SIZE]
                    .clone_from_slice(&bytes[block_offset..(block_offset + CHR_BANK_SIZE)]);

                chr_banks.push(gfx);
            }
        } else {
            // If the header CHR-ROM value is 0, we should assume that 8KB of CHR-RAM is available.
            println!("Got {} bytes of CHR RAM", chr_ram_size);
            for _ in 0..(chr_ram_size / CHR_BANK_SIZE) {
                let gfx: [u8; CHR_BANK_SIZE] = [0; CHR_BANK_SIZE];
                chr_banks.push(gfx);
            }
        }

        let result: Box<dyn memory::MemoryMapper> = match mapper {
            0 => Box::new(mapper::nrom::NROMMapper::new(
                flags,
                Box::new(*prg_banks.get(0).unwrap()),
                prg_banks.pop(),
                chr_banks.pop(),
            )),
            1 => Box::new(mapper::mmc1::MMC1Mapper::new(flags, prg_banks, chr_banks)),
            2 => Box::new(mapper::uxrom::UxROMMapper::new(flags, prg_banks)),
            4 => Box::new(MMC3Mapper::new(flags, prg_banks, chr_banks)),
            _ => panic!("Mapper {:X} not implemented!", mapper),
        };

        println!("Loaded {} with mapper {}", path, mapper);

        Ok(result)
    }
}

#[allow(dead_code)] // only used in tests
pub fn load_ascii(path: &str) -> Box<dyn memory::MemoryMapper> {
    let l: Box<dyn Loader> = Box::new(AsciiLoader {});
    l.load(path).ok().unwrap()
}

#[allow(dead_code)] // only used in tests
pub fn load_bin(path: &str) -> Box<dyn memory::MemoryMapper> {
    let l: Box<dyn Loader> = Box::new(BinLoader {});
    l.load(path).ok().unwrap()
}

#[allow(dead_code)] // only used in tests
pub fn load_nes(path: &str) -> Box<dyn memory::MemoryMapper> {
    let l: Box<dyn Loader> = InesLoader::new();
    l.load(path).ok().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_ines() {
        let l: Box<dyn Loader> = InesLoader::new();
        let result = l.load("input/nes/all_instrs.nes");
        assert_eq!(result.is_ok(), true);
        assert_eq!(result.ok().unwrap().code_start(), 0xea71);
    }

    #[test]
    fn test_load_ines_no_such_file() {
        let l: Box<dyn Loader> = InesLoader::new();
        let result = l.load("does_not_exist");
        assert_eq!(result.is_ok(), false);
        assert_eq!(
            result.err(),
            Some(format!("File does not exist: does_not_exist"))
        );
    }

    #[test]
    fn test_load_ines_header() {
        let l: Box<dyn Loader> = InesLoader::new();
        let result = l.load("input/nes/nestest.log");
        assert_eq!(result.is_ok(), false);
        assert_eq!(
            result.err(),
            Some(format!("ERROR: Missing iNES header magic numbers"))
        );
    }
}
