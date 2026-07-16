use std::path::PathBuf;

use super::super::memory;
use super::super::{super::util, memory::mapper};
use crate::emu::memory::mapper::mmc3::{MMC3Mapper, MMC3Variant};
use crate::emu::memory::mapper::mmc5::MMC5Mapper;

extern crate hex;

pub trait Loader {
    fn load(&self, path: &str) -> Result<Box<dyn memory::MemoryMapper>, String>;
}

pub struct AsciiLoader {}

impl Loader for AsciiLoader {
    fn load(&self, path: &str) -> Result<Box<dyn memory::MemoryMapper>, String> {
        let mut code: Vec<u8> = vec![];

        if let Ok(lines) = util::read_lines(path) {
            for line in lines.map_while(Result::ok) {
                let content = line.trim();
                let content = content.split(';').next().unwrap();
                if !content.is_empty() {
                    for byte in content.split(' ') {
                        let mut decoded = hex::decode(byte).expect("Decoding failed");
                        code.append(&mut decoded);
                    }
                }
            }
        }

        let mut mapper: Box<dyn memory::MemoryMapper> =
            Box::new(memory::IdentityMapper::new(0x600));
        for (i, b) in code.iter().enumerate() {
            mapper.cpu_write(0x600 + i as u16, *b);
        }

        Ok(mapper)
    }
}

pub struct BinLoader {}

impl Loader for BinLoader {
    fn load(&self, path: &str) -> Result<Box<dyn memory::MemoryMapper>, String> {
        let bytes = util::read_bytes(path)?;

        let mut mapper: Box<dyn memory::MemoryMapper> =
            Box::new(memory::IdentityMapper::new_flat_cpu_bus(0x400));
        for (i, b) in bytes.iter().enumerate() {
            mapper.cpu_write(i as u16, *b);
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
const INES_MAGIC: [u8; 4] = [0x4E, 0x45, 0x53, 0x1A];
const NES2_DETECT_MASK: u8 = 0b0000_1100;
const NES2_DETECT_VALUE: u8 = 0b0000_1000;
const FLAG_BATTERY: u8 = 0x02;
const FLAG_TRAINER: u8 = 0b0000_0100;
const TRAINER_SIZE: usize = 512;

const PRG_BANK_SIZE: usize = 16384;
pub const CHR_BANK_SIZE: usize = 8192;
const PRG_BANK_32K: usize = 32 * 1024;

fn combine_prg_banks_32k(prg_banks: &[[u8; PRG_BANK_SIZE]]) -> Vec<[u8; PRG_BANK_32K]> {
    let mut banks = Vec::new();

    for i in (0..prg_banks.len()).step_by(2) {
        let mut bank_32k = [0; PRG_BANK_32K];
        if i + 1 < prg_banks.len() {
            bank_32k[0..PRG_BANK_SIZE].copy_from_slice(&prg_banks[i]);
            bank_32k[PRG_BANK_SIZE..PRG_BANK_32K].copy_from_slice(&prg_banks[i + 1]);
        } else {
            bank_32k[0..PRG_BANK_SIZE].copy_from_slice(&prg_banks[i]);
            bank_32k[PRG_BANK_SIZE..PRG_BANK_32K].copy_from_slice(&prg_banks[i]);
        }
        banks.push(bank_32k);
    }

    banks
}

pub fn sav_path(rom_path: &str) -> PathBuf {
    let mut path = PathBuf::from(rom_path);
    path.set_extension("sav");
    path
}

impl Loader for InesLoader {
    fn load(&self, path: &str) -> Result<Box<dyn memory::MemoryMapper>, String> {
        let bytes = util::read_bytes(path)?;

        let sram_data = {
            let flags = bytes[6];
            let has_battery = (flags & FLAG_BATTERY) != 0;
            if has_battery {
                let sav = sav_path(path);
                match std::fs::read(&sav) {
                    Ok(data) => {
                        println!("Loaded save data from {}", sav.display());
                        Some(data)
                    }
                    Err(_) => None,
                }
            } else {
                None
            }
        };

        let result = load_nes_from_bytes_inner(&bytes, sram_data)?;
        Ok(result)
    }
}

pub fn load_nes_from_bytes(bytes: &[u8]) -> Result<Box<dyn memory::MemoryMapper>, String> {
    load_nes_from_bytes_inner(bytes, None)
}

pub fn load_nes_from_bytes_with_sram(
    bytes: &[u8],
    sram_data: Option<Vec<u8>>,
) -> Result<Box<dyn memory::MemoryMapper>, String> {
    load_nes_from_bytes_inner(bytes, sram_data)
}

pub fn rom_has_battery(bytes: &[u8]) -> bool {
    bytes.len() > 6 && (bytes[6] & FLAG_BATTERY) != 0
}

pub fn detect_region(bytes: &[u8]) -> crate::emu::region::Region {
    detect_region_with_filename(bytes, None)
}

pub fn detect_region_with_filename(
    bytes: &[u8],
    filename: Option<&str>,
) -> crate::emu::region::Region {
    use crate::emu::region::Region;
    if bytes.len() >= INES_HEADER_SIZE {
        let is_nes2 = bytes[7] & NES2_DETECT_MASK == NES2_DETECT_VALUE;
        if is_nes2 {
            return match bytes[12] & 0x03 {
                1 => Region::Pal,
                _ => Region::Ntsc,
            };
        }
        if bytes[9] & 0x01 != 0 {
            return Region::Pal;
        }
    }
    if let Some(name) = filename {
        if filename_suggests_pal(name) {
            return Region::Pal;
        }
    }
    Region::Ntsc
}

fn filename_suggests_pal(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("(e)") || lower.contains("(europe)") || lower.contains("(pal)")
}

fn load_nes_from_bytes_inner(
    bytes: &[u8],
    sram_data: Option<Vec<u8>>,
) -> Result<Box<dyn memory::MemoryMapper>, String> {
    if bytes.len() < INES_HEADER_SIZE {
        return Err("File too small for iNES header".to_string());
    }

    if bytes[0..4] != INES_MAGIC {
        return Err("Missing iNES header magic numbers".to_string());
    }

    let is_nes2_header = bytes[7] & NES2_DETECT_MASK == NES2_DETECT_VALUE;

    let num_prg_blocks = bytes[4];
    let num_chr_blocks = bytes[5];
    let flags = bytes[6];
    let mapper_id = {
        let ines_mapper = ((bytes[7] as u16) & 0xF0) | ((flags as u16) >> 4);
        if is_nes2_header {
            ines_mapper | (((bytes[8] as u16) & 0x0F) << 8)
        } else {
            ines_mapper
        }
    };
    let has_battery = (flags & FLAG_BATTERY) != 0;
    let submapper = if is_nes2_header {
        (bytes[8] >> 4) & 0x0F
    } else {
        0
    };
    let has_trainer = (flags & FLAG_TRAINER) != 0;
    let prg_offset: usize = INES_HEADER_SIZE + if has_trainer { TRAINER_SIZE } else { 0 };
    let chr_offset: usize = prg_offset + (num_prg_blocks as usize * PRG_BANK_SIZE);

    let chr_ram_size: usize = if is_nes2_header {
        let ram_shift = bytes[11] & 0b0000_1111;
        let nvram_shift = (bytes[11] & 0b1111_0000) >> 4;
        let shift = std::cmp::max(ram_shift, nvram_shift);
        if shift > 0 {
            64 << shift
        } else {
            0
        }
    } else {
        8192
    };

    let mut prg_banks: Vec<[u8; PRG_BANK_SIZE]> = vec![];
    for b in 0..num_prg_blocks {
        let mut code = [0; PRG_BANK_SIZE];
        let block_offset: usize = prg_offset + (b as usize * PRG_BANK_SIZE);
        code.clone_from_slice(&bytes[block_offset..(block_offset + PRG_BANK_SIZE)]);
        prg_banks.push(code);
    }

    let mut chr_banks: Vec<[u8; CHR_BANK_SIZE]> = vec![];
    if num_chr_blocks > 0 {
        for b in 0..num_chr_blocks {
            let mut gfx = [0; CHR_BANK_SIZE];
            let block_offset: usize = chr_offset + (b as usize * CHR_BANK_SIZE);
            gfx.clone_from_slice(&bytes[block_offset..(block_offset + CHR_BANK_SIZE)]);
            chr_banks.push(gfx);
        }
    } else {
        for _ in 0..(chr_ram_size / CHR_BANK_SIZE) {
            chr_banks.push([0; CHR_BANK_SIZE]);
        }
    }

    let result: Box<dyn memory::MemoryMapper> = match mapper_id {
        0 => Box::new(mapper::nrom::NROMMapper::new(
            flags,
            Box::new(*prg_banks.first().unwrap()),
            prg_banks.pop(),
            chr_banks.pop(),
        )),
        1 => Box::new(mapper::mmc1::MMC1Mapper::new(
            flags,
            prg_banks,
            chr_banks,
            has_battery,
            sram_data,
        )),
        2 => Box::new(mapper::uxrom::UxROMMapper::new(flags, prg_banks)),
        3 => {
            let mut prg_32k = [0; PRG_BANK_32K];
            if prg_banks.len() >= 2 {
                prg_32k[0..PRG_BANK_SIZE].copy_from_slice(&prg_banks[0]);
                prg_32k[PRG_BANK_SIZE..PRG_BANK_32K].copy_from_slice(&prg_banks[1]);
            } else if prg_banks.len() == 1 {
                prg_32k[0..PRG_BANK_SIZE].copy_from_slice(&prg_banks[0]);
                prg_32k[PRG_BANK_SIZE..PRG_BANK_32K].copy_from_slice(&prg_banks[0]);
            } else {
                return Err("CNROM requires at least one PRG bank".to_string());
            }
            Box::new(mapper::cnrom::CNROMMapper::new(flags, prg_32k, chr_banks))
        }
        4 => {
            let (mmc3_chr, mmc3_chr_ram_banks) = if num_chr_blocks > 0 {
                (chr_banks, 0)
            } else {
                (vec![], chr_ram_size / 1024)
            };
            Box::new(MMC3Mapper::new(
                flags,
                prg_banks,
                mmc3_chr,
                has_battery,
                sram_data,
                submapper,
                mmc3_chr_ram_banks,
            ))
        }
        5 => Box::new(MMC5Mapper::new(
            flags,
            prg_banks,
            chr_banks,
            has_battery,
            sram_data,
        )),
        6 => {
            let chr = if num_chr_blocks > 0 {
                chr_banks
            } else {
                vec![]
            };
            Box::new(mapper::front_fareast::FrontFareastMapper::new(
                flags, prg_banks, chr,
            ))
        }
        7 => Box::new(mapper::axrom::AxROMMapper::new(
            flags,
            combine_prg_banks_32k(&prg_banks),
            submapper,
        )),
        9 => Box::new(mapper::mmc2::MMC2Mapper::new(flags, prg_banks, chr_banks)),
        10 => Box::new(mapper::mmc4::MMC4Mapper::new(
            flags,
            prg_banks,
            chr_banks,
            has_battery,
            sram_data,
        )),
        11 => Box::new(mapper::color_dreams::ColorDreamsMapper::new(
            flags,
            combine_prg_banks_32k(&prg_banks),
            chr_banks,
        )),
        13 => Box::new(mapper::cprom::CpromMapper::new(flags, prg_banks)),
        15 => Box::new(mapper::k1029::K1029Mapper::new(prg_banks)),
        16 => Box::new(mapper::bandai_fcg::BandaiFcgMapper::new(
            flags, prg_banks, chr_banks, submapper, sram_data,
        )),
        32 => Box::new(mapper::irem_g101::IremG101Mapper::new(
            flags, prg_banks, chr_banks, submapper,
        )),
        18 => Box::new(mapper::jaleco_ss88006::JalecoSS88006Mapper::new(
            flags,
            prg_banks,
            chr_banks,
            has_battery,
            sram_data,
        )),
        19 => Box::new(mapper::namco163::Namco163Mapper::new(
            flags,
            prg_banks,
            chr_banks,
            has_battery,
            sram_data,
            submapper,
        )),
        28 => Box::new(mapper::action53::Action53Mapper::new(flags, prg_banks)),
        21 | 22 | 23 | 25 => Box::new(mapper::vrc2_4::Vrc2_4Mapper::new(
            flags,
            prg_banks,
            chr_banks,
            has_battery,
            sram_data,
            mapper_id as u8,
            submapper,
        )),
        24 | 26 => Box::new(mapper::vrc6::Vrc6Mapper::new(
            flags,
            prg_banks,
            chr_banks,
            has_battery,
            sram_data,
            mapper_id as u8,
        )),
        85 => Box::new(mapper::vrc7::Vrc7Mapper::new(
            flags,
            prg_banks,
            chr_banks,
            num_chr_blocks == 0,
            has_battery,
            sram_data,
        )),
        37 => Box::new(MMC3Mapper::new_variant(
            flags,
            prg_banks,
            chr_banks,
            has_battery,
            sram_data,
            submapper,
            MMC3Variant::Mapper37,
            0,
        )),
        30 => Box::new(mapper::unrom512::Unrom512Mapper::new(
            flags, prg_banks, submapper,
        )),
        31 => Box::new(mapper::mapper31::Mapper31::new(flags, prg_banks)),
        33 => Box::new(mapper::taito::Taito33Mapper::new(
            flags, prg_banks, chr_banks,
        )),
        48 => Box::new(mapper::taito::Taito33Mapper::new_mapper48(
            flags, prg_banks, chr_banks,
        )),
        47 => {
            let (mmc3_chr, mmc3_chr_ram_banks) = if num_chr_blocks > 0 {
                (chr_banks, 0)
            } else {
                (vec![], chr_ram_size / 1024)
            };
            Box::new(MMC3Mapper::new_variant(
                flags,
                prg_banks,
                mmc3_chr,
                has_battery,
                sram_data,
                submapper,
                MMC3Variant::Mapper47,
                mmc3_chr_ram_banks,
            ))
        }
        65 => Box::new(mapper::irem_h3001::IremH3001Mapper::new(
            flags, prg_banks, chr_banks,
        )),
        67 => Box::new(mapper::sunsoft3::Sunsoft3Mapper::new(
            flags, prg_banks, chr_banks,
        )),
        34 => {
            if is_nes2_header && submapper != 0 && submapper != 2 {
                return Err(format!(
                    "Mapper 34 submapper {submapper} not implemented; only BNROM supported"
                ));
            }
            Box::new(mapper::bnrom::BNROMMapper::new(
                flags,
                combine_prg_banks_32k(&prg_banks),
            ))
        }
        66 => Box::new(mapper::gxrom::GxROMMapper::new(
            flags,
            combine_prg_banks_32k(&prg_banks),
            chr_banks,
        )),
        68 => Box::new(mapper::sunsoft4::Sunsoft4Mapper::new(
            flags,
            prg_banks,
            chr_banks,
            has_battery,
            sram_data,
        )),
        69 => Box::new(mapper::sunsoft_fme7::SunsoftFme7Mapper::new(
            flags,
            prg_banks,
            chr_banks,
            has_battery,
            sram_data,
        )),
        70 => {
            let chr = if num_chr_blocks > 0 {
                chr_banks
            } else {
                vec![]
            };
            Box::new(mapper::simple::SimpleMapper::mapper70(
                flags, prg_banks, chr,
            ))
        }
        71 => Box::new(mapper::camerica::CamericaMapper::new(flags, prg_banks)),
        72 => {
            let chr = if num_chr_blocks > 0 {
                chr_banks
            } else {
                vec![]
            };
            Box::new(mapper::jaleco_jf::JalecoJfMapper::new(
                flags, prg_banks, chr, false,
            ))
        }
        76 => Box::new(mapper::namco108::Namco108Mapper::new(
            flags,
            prg_banks,
            chr_banks,
            mapper::namco108::Namco108Variant::M76,
        )),
        77 => Box::new(mapper::mapper77::Mapper77::new(flags, prg_banks, chr_banks)),
        73 => {
            let chr = if num_chr_blocks > 0 {
                chr_banks
            } else {
                vec![]
            };
            Box::new(mapper::vrc3::Vrc3Mapper::new(flags, prg_banks, chr))
        }
        75 => Box::new(mapper::vrc1::Vrc1Mapper::new(flags, prg_banks, chr_banks)),
        78 => Box::new(mapper::simple::SimpleMapper::mapper78(
            flags, prg_banks, chr_banks, submapper,
        )),
        87 => Box::new(mapper::simple::SimpleMapper::mapper87(
            flags, prg_banks, chr_banks,
        )),
        86 => {
            let chr = if num_chr_blocks > 0 {
                chr_banks
            } else {
                vec![]
            };
            Box::new(mapper::simple::SimpleMapper::mapper86(
                flags, prg_banks, chr,
            ))
        }
        80 => Box::new(mapper::taito_x1005::TaitoX1005Mapper::new(
            flags, prg_banks, chr_banks, false, sram_data,
        )),
        82 => Box::new(mapper::taito_x1017::TaitoX1017Mapper::new(
            flags, prg_banks, chr_banks, sram_data,
        )),
        88 => Box::new(mapper::namco108::Namco108Mapper::new(
            flags,
            prg_banks,
            chr_banks,
            mapper::namco108::Namco108Variant::M88,
        )),
        89 => {
            let chr = if num_chr_blocks > 0 {
                chr_banks
            } else {
                vec![]
            };
            Box::new(mapper::simple::SimpleMapper::mapper89(prg_banks, chr))
        }
        92 => {
            let chr = if num_chr_blocks > 0 {
                chr_banks
            } else {
                vec![]
            };
            Box::new(mapper::jaleco_jf::JalecoJfMapper::new(
                flags, prg_banks, chr, true,
            ))
        }
        93 => {
            let chr = if num_chr_blocks > 0 {
                chr_banks
            } else {
                vec![]
            };
            Box::new(mapper::simple::SimpleMapper::mapper93(
                flags, prg_banks, chr,
            ))
        }
        94 => {
            let chr = if num_chr_blocks > 0 {
                chr_banks
            } else {
                vec![]
            };
            Box::new(mapper::simple::SimpleMapper::mapper94(
                flags, prg_banks, chr,
            ))
        }
        96 => Box::new(mapper::oeka_kids::OekaKidsMapper::new(flags, prg_banks)),
        95 => Box::new(mapper::namco108::Namco108Mapper::new(
            flags,
            prg_banks,
            chr_banks,
            mapper::namco108::Namco108Variant::M95,
        )),
        97 => {
            let chr = if num_chr_blocks > 0 {
                chr_banks
            } else {
                vec![]
            };
            Box::new(mapper::simple::SimpleMapper::mapper97(
                flags, prg_banks, chr,
            ))
        }
        105 => Box::new(mapper::nes_event::NesEventMapper::new(flags, prg_banks)),
        118 => {
            let (mmc3_chr, mmc3_chr_ram_banks) = if num_chr_blocks > 0 {
                (chr_banks, 0)
            } else {
                (vec![], chr_ram_size / 1024)
            };
            Box::new(MMC3Mapper::new_variant(
                flags,
                prg_banks,
                mmc3_chr,
                has_battery,
                sram_data,
                submapper,
                MMC3Variant::TxSROM,
                mmc3_chr_ram_banks,
            ))
        }
        119 => {
            let (mmc3_chr, mmc3_chr_ram_banks) = if num_chr_blocks > 0 {
                (chr_banks, 0)
            } else {
                (vec![], chr_ram_size / 1024)
            };
            Box::new(MMC3Mapper::new_variant(
                flags,
                prg_banks,
                mmc3_chr,
                has_battery,
                sram_data,
                submapper,
                MMC3Variant::TQROM,
                mmc3_chr_ram_banks,
            ))
        }
        140 => Box::new(mapper::simple::SimpleMapper::mapper140(
            flags, prg_banks, chr_banks,
        )),
        152 => Box::new(mapper::simple::SimpleMapper::mapper152(
            prg_banks, chr_banks,
        )),
        154 => Box::new(mapper::namco108::Namco108Mapper::new(
            flags,
            prg_banks,
            chr_banks,
            mapper::namco108::Namco108Variant::M154,
        )),
        153 => {
            let chr = if num_chr_blocks > 0 {
                chr_banks
            } else {
                vec![]
            };
            Box::new(mapper::bandai_fcg::BandaiFcgMapper::new_mapper153(
                flags, prg_banks, chr, sram_data,
            ))
        }
        155 => Box::new(mapper::mmc1::MMC1Mapper::new_mmc1a(
            flags,
            prg_banks,
            chr_banks,
            has_battery,
            sram_data,
        )),
        156 => Box::new(mapper::daou::DaouMapper::new(prg_banks, chr_banks)),
        157 => {
            let chr = if num_chr_blocks > 0 {
                chr_banks
            } else {
                vec![]
            };
            Box::new(mapper::bandai_fcg::BandaiFcgMapper::new_mapper157(
                flags, prg_banks, chr, sram_data,
            ))
        }
        159 => Box::new(mapper::bandai_fcg::BandaiFcgMapper::new_mapper159(
            flags, prg_banks, chr_banks, sram_data,
        )),
        188 => Box::new(mapper::bandai_karaoke::BandaiKaraokeMapper::new(
            flags, prg_banks,
        )),
        207 => Box::new(mapper::taito_x1005::TaitoX1005Mapper::new(
            flags, prg_banks, chr_banks, true, sram_data,
        )),
        180 => {
            let chr = if num_chr_blocks > 0 {
                chr_banks
            } else {
                vec![]
            };
            Box::new(mapper::simple::SimpleMapper::mapper180(
                flags, prg_banks, chr,
            ))
        }
        184 => Box::new(mapper::simple::SimpleMapper::mapper184(
            flags, prg_banks, chr_banks,
        )),
        185 => Box::new(mapper::simple::SimpleMapper::mapper185(
            flags, prg_banks, chr_banks, submapper,
        )),
        206 => Box::new(mapper::namco108::Namco108Mapper::new(
            flags,
            prg_banks,
            chr_banks,
            mapper::namco108::Namco108Variant::M206,
        )),
        210 => Box::new(mapper::namco175_340::Namco175_340Mapper::new(
            flags, prg_banks, chr_banks, submapper,
        )),
        _ => return Err(format!("Mapper {mapper_id} not implemented")),
    };

    let mut result = result;
    if has_trainer && bytes.len() >= INES_HEADER_SIZE + TRAINER_SIZE {
        for (i, &b) in bytes[INES_HEADER_SIZE..INES_HEADER_SIZE + TRAINER_SIZE]
            .iter()
            .enumerate()
        {
            result.cpu_write(0x7000 + i as u16, b);
        }
    }

    Ok(result)
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
    use crate::test_rom;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn write_test_rom(mapper_id: u8, prg_blocks: u8, chr_blocks: u8) -> PathBuf {
        let mut bytes = vec![0; INES_HEADER_SIZE];
        bytes[0..4].copy_from_slice(b"NES\x1A");
        bytes[4] = prg_blocks;
        bytes[5] = chr_blocks;
        bytes[6] = (mapper_id & 0x0F) << 4;
        bytes[7] = mapper_id & 0xF0;

        bytes.extend(std::iter::repeat_n(0, prg_blocks as usize * PRG_BANK_SIZE));
        bytes.extend(std::iter::repeat_n(0, chr_blocks as usize * CHR_BANK_SIZE));

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("krankulator-test-{mapper_id}-{nonce}.nes"));
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn test_load_ines() {
        let l: Box<dyn Loader> = InesLoader::new();
        let result = l.load(test_rom!("instr_test-v5/all_instrs.nes"));
        assert!(result.is_ok());
        assert_eq!(result.ok().unwrap().code_start(), 0xea71);
    }

    #[test]
    fn test_load_ines_no_such_file() {
        let l: Box<dyn Loader> = InesLoader::new();
        let result = l.load("does_not_exist");
        assert!(!result.is_ok());
        assert_eq!(
            result.err(),
            Some("File does not exist: does_not_exist".to_string())
        );
    }

    #[test]
    fn test_load_ines_header() {
        let l: Box<dyn Loader> = InesLoader::new();
        let result = l.load(test_rom!("other/nestest.log"));
        assert!(!result.is_ok());
        assert_eq!(
            result.err(),
            Some("Missing iNES header magic numbers".to_string())
        );
    }

    #[test]
    fn test_load_ines_mapper_uses_flags7_upper_nibble() {
        let l: Box<dyn Loader> = InesLoader::new();

        let gxrom_path = write_test_rom(66, 2, 1);
        let gxrom_result = l.load(gxrom_path.to_str().unwrap());
        std::fs::remove_file(&gxrom_path).unwrap();
        assert_eq!(gxrom_result.unwrap().mapper_id(), 66);

        let bnrom_path = write_test_rom(34, 2, 0);
        let bnrom_result = l.load(bnrom_path.to_str().unwrap());
        std::fs::remove_file(&bnrom_path).unwrap();
        assert_eq!(bnrom_result.unwrap().mapper_id(), 34);
    }

    #[test]
    fn test_detect_region_ines1_ntsc() {
        let mut bytes = vec![0u8; INES_HEADER_SIZE];
        bytes[0..4].copy_from_slice(b"NES\x1A");
        bytes[9] = 0x00;
        assert_eq!(detect_region(&bytes), crate::emu::region::Region::Ntsc);
    }

    #[test]
    fn test_detect_region_ines1_pal() {
        let mut bytes = vec![0u8; INES_HEADER_SIZE];
        bytes[0..4].copy_from_slice(b"NES\x1A");
        bytes[9] = 0x01;
        assert_eq!(detect_region(&bytes), crate::emu::region::Region::Pal);
    }

    #[test]
    fn test_detect_region_nes2_ntsc() {
        let mut bytes = vec![0u8; INES_HEADER_SIZE];
        bytes[0..4].copy_from_slice(b"NES\x1A");
        bytes[7] = NES2_DETECT_VALUE;
        bytes[12] = 0x00;
        assert_eq!(detect_region(&bytes), crate::emu::region::Region::Ntsc);
    }

    #[test]
    fn test_detect_region_nes2_pal() {
        let mut bytes = vec![0u8; INES_HEADER_SIZE];
        bytes[0..4].copy_from_slice(b"NES\x1A");
        bytes[7] = NES2_DETECT_VALUE;
        bytes[12] = 0x01;
        assert_eq!(detect_region(&bytes), crate::emu::region::Region::Pal);
    }

    #[test]
    fn test_detect_region_short_header() {
        assert_eq!(detect_region(&[0; 4]), crate::emu::region::Region::Ntsc);
    }

    #[test]
    fn test_detect_region_filename_europe() {
        let mut bytes = vec![0u8; INES_HEADER_SIZE];
        bytes[0..4].copy_from_slice(b"NES\x1A");
        bytes[9] = 0x00;
        assert_eq!(
            detect_region_with_filename(&bytes, Some("Super Mario Bros (E).nes")),
            crate::emu::region::Region::Pal
        );
        assert_eq!(
            detect_region_with_filename(&bytes, Some("Game (Europe).nes")),
            crate::emu::region::Region::Pal
        );
        assert_eq!(
            detect_region_with_filename(&bytes, Some("Game (PAL).nes")),
            crate::emu::region::Region::Pal
        );
    }

    #[test]
    fn test_detect_region_filename_no_match() {
        let mut bytes = vec![0u8; INES_HEADER_SIZE];
        bytes[0..4].copy_from_slice(b"NES\x1A");
        bytes[9] = 0x00;
        assert_eq!(
            detect_region_with_filename(&bytes, Some("Game (U).nes")),
            crate::emu::region::Region::Ntsc
        );
    }

    #[test]
    fn test_detect_region_header_overrides_filename() {
        let mut bytes = vec![0u8; INES_HEADER_SIZE];
        bytes[0..4].copy_from_slice(b"NES\x1A");
        bytes[7] = NES2_DETECT_VALUE;
        bytes[12] = 0x00;
        assert_eq!(
            detect_region_with_filename(&bytes, Some("Game (E).nes")),
            crate::emu::region::Region::Ntsc
        );
    }
}
