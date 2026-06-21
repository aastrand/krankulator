mod audio;
pub(crate) mod bindings;
mod bindings_ui;
mod debug;
mod gamepad;
mod io;
pub(crate) mod settings;

use std::io::Read;

use clap::{Parser, ValueEnum};
use krankulator_core::emu;
use krankulator_core::emu::io::loader;
use krankulator_core::util;

fn extract_nes_from_zip(path: &str) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Failed to open {path}: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Failed to read ZIP {path}: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read ZIP entry: {e}"))?;
        if let Some(name) = entry.name().to_lowercase().strip_suffix(".nes") {
            let _ = name;
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| format!("Failed to extract from ZIP: {e}"))?;
            return Ok(buf);
        }
    }
    Err(format!("No .nes file found in {path}"))
}

fn load_rom_file(path: &str) -> Result<Box<dyn emu::memory::MemoryMapper>, String> {
    if path.to_lowercase().ends_with(".zip") {
        let bytes = extract_nes_from_zip(path)?;
        let sram_data = if loader::rom_has_battery(&bytes) {
            let mut sav = std::path::PathBuf::from(path);
            sav.set_extension("sav");
            std::fs::read(&sav).ok().inspect(|_| {
                println!("Loaded save data from {}", sav.display());
            })
        } else {
            None
        };
        let result = loader::load_nes_from_bytes_with_sram(&bytes, sram_data)?;
        Ok(result)
    } else {
        let l: Box<dyn loader::Loader> = loader::InesLoader::new();
        l.load(path)
    }
}

/// Krankulator
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Skip display
    #[clap(long)]
    headless: bool,

    /// Specify loader: nes (default), ascii, bin
    #[clap(short, long, default_value = "nes")]
    loader: String,

    /// Verbose mode
    #[clap(short, long)]
    verbose: bool,

    /// Quiet mode, overrides verbose
    #[clap(short, long)]
    quiet: bool,

    /// Debug on infinite loop
    #[clap(short, long)]
    debug: bool,

    /// Add a breakpoint
    #[clap(short, long, multiple_occurrences(true))]
    breakpoint: Vec<String>,

    /// Starting address of code
    #[clap(short, long)]
    codeaddr: Option<String>,

    /// Write captured audio to WAV file (implies headless)
    #[clap(long)]
    wav_out: Option<String>,

    /// Region: auto, ntsc, pal
    #[clap(long, value_enum, ignore_case = true, default_value_t = RegionArg::Auto)]
    region: RegionArg,

    /// Input file to use
    #[clap()]
    input: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum RegionArg {
    Auto,
    Ntsc,
    Pal,
}

fn detect_region_from_file(path: &str) -> emu::Region {
    let filename = std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str());
    if path.to_lowercase().ends_with(".zip") {
        match extract_nes_from_zip(path) {
            Ok(bytes) => loader::detect_region_with_filename(&bytes, filename),
            Err(_) => emu::Region::Ntsc,
        }
    } else {
        match std::fs::read(path) {
            Ok(bytes) => loader::detect_region_with_filename(&bytes, filename),
            Err(_) => emu::Region::Ntsc,
        }
    }
}

fn main() -> Result<(), String> {
    let args = Args::parse();
    let mut settings = settings::load_settings();

    let mut emu = if let Some(ref input) = args.input {
        match args.loader.as_str() {
            "bin" => {
                let loader: Box<dyn loader::Loader> = Box::new(loader::BinLoader {});
                match loader.load(input) {
                    Ok(mapper) => emu::Emulator::new_headless(mapper),
                    Err(msg) => panic!("{}", msg),
                }
            }
            "ascii" => {
                let loader: Box<dyn loader::Loader> = Box::new(loader::AsciiLoader {});
                match loader.load(input) {
                    Ok(mapper) => emu::Emulator::new_headless(mapper),
                    Err(msg) => panic!("{}", msg),
                }
            }
            "nes" => match load_rom_file(input) {
                Ok(mapper) => {
                    let region = match args.region {
                        RegionArg::Auto => detect_region_from_file(input),
                        RegionArg::Ntsc => emu::Region::Ntsc,
                        RegionArg::Pal => emu::Region::Pal,
                    };
                    println!(
                        "Loaded {} (mapper {}, {})",
                        input,
                        mapper.mapper_id(),
                        region
                    );
                    let mut emu: emu::Emulator = if args.wav_out.is_some() {
                        emu::Emulator::new_capturing_with_region(mapper, region)
                    } else if !args.headless {
                        let audio = Box::new(
                            audio::AudioOutput::try_new(emu::apu::SAMPLE_RATE)
                                .expect("No audio output device available"),
                        );
                        let rom_name = std::path::Path::new(input.as_str())
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or(input);
                        let io = Box::new(io::PlatformIOHandler::new(
                            256,
                            240,
                            rom_name,
                            &mut settings,
                        ));
                        emu::Emulator::new_with_region(io, mapper, audio, region)
                    } else {
                        emu::Emulator::new_headless_with_region(mapper, region)
                    };

                    emu.cpu.status = 0x34;
                    emu.cpu.sp = 0xfd;
                    emu.toggle_should_trigger_nmi(true);
                    emu.toggle_should_exit_on_infinite_loop(false);
                    emu.set_overscan(settings.overscan);
                    emu.set_rom_path(input);
                    io::add_recent_rom(input);

                    emu
                }
                Err(msg) => panic!("{}", msg),
            },
            _ => {
                println!("Invalid loader, see --help");
                std::process::exit(1);
            }
        }
    } else {
        let mapper: Box<dyn emu::memory::MemoryMapper> =
            Box::new(emu::memory::IdentityMapper::new(0x600));
        let audio = Box::new(
            audio::AudioOutput::try_new(emu::apu::SAMPLE_RATE)
                .expect("No audio output device available"),
        );
        let io = Box::new(io::PlatformIOHandler::new(
            256,
            240,
            "krankulator",
            &mut settings,
        ));
        let mut emu = emu::Emulator::new_with(io, mapper, audio);
        emu.toggle_should_exit_on_infinite_loop(false);
        emu.toggle_should_trigger_nmi(false);
        emu.set_overscan(settings.overscan);
        emu.overlay.set_banner(Some("Open a ROM to play".into()));
        emu.set_static_noise(true);
        emu
    };

    for breakpoint in args.breakpoint {
        println!("Adding breakpoint at {breakpoint}");
        emu::dbg::toggle_breakpoint(&breakpoint, &mut emu.breakpoints);
    }

    if let Some(input_addr) = args.codeaddr {
        match util::hex_str_to_u16(&input_addr) {
            Ok(addr) => emu.cpu.pc = addr,
            _ => {
                println!("Invalid code addr: {input_addr}");
                std::process::exit(1);
            }
        };
    }

    emu.toggle_verbose_mode(args.verbose & !args.quiet);
    emu.toggle_quiet_mode(args.quiet);
    emu.toggle_debug_on_infinite_loop(args.debug);

    loop {
        emu.run();
        match emu.take_pending_open_rom() {
            Some(path) => match load_rom_file(&path) {
                Ok(mapper) => {
                    let region = match args.region {
                        RegionArg::Auto => detect_region_from_file(&path),
                        RegionArg::Ntsc => emu::Region::Ntsc,
                        RegionArg::Pal => emu::Region::Pal,
                    };
                    println!(
                        "Loaded {} (mapper {}, {})",
                        path,
                        mapper.mapper_id(),
                        region
                    );
                    emu.load_rom_with_region(mapper, &path, region);
                    io::add_recent_rom(&path);
                }
                Err(msg) => {
                    eprintln!("Failed to load ROM: {msg}");
                    emu.overlay.toast(msg);
                }
            },
            None => break,
        }
    }

    if let Some(wav_path) = &args.wav_out {
        let samples = emu.drain_captured_audio();
        emu::audio::wav::write_wav(wav_path, &samples, 44100)
            .map_err(|e| format!("Failed to write WAV: {e}"))?;
        println!(
            "Wrote {} samples ({:.1}s) to {}",
            samples.len(),
            samples.len() as f64 / 44100.0,
            wav_path
        );
    }

    Ok(())
}

fn config_dir() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("krankulator"))
}

pub(crate) fn load_last_rom_dir() -> Option<std::path::PathBuf> {
    let path = config_dir()?.join("last_rom_dir");
    let dir = std::fs::read_to_string(path).ok()?;
    let dir = std::path::PathBuf::from(dir.trim());
    dir.is_dir().then_some(dir)
}

pub(crate) fn save_last_rom_dir(dir: &std::path::Path) {
    if let Some(config) = config_dir() {
        let _ = std::fs::create_dir_all(&config);
        let _ = std::fs::write(
            config.join("last_rom_dir"),
            dir.to_string_lossy().as_bytes(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krankulator_core::test_rom;

    #[test]
    fn test_audio_backend_wires_to_core() {
        let audio: Box<dyn krankulator_core::emu::audio::AudioBackend> =
            match audio::AudioOutput::try_new(emu::apu::SAMPLE_RATE) {
                Some(a) => Box::new(a),
                None => {
                    eprintln!("No audio device available, skipping test");
                    return;
                }
            };
        let mapper = loader::load_nes(&String::from(test_rom!("other/nestest.nes")));
        let mut emu =
            emu::Emulator::new_with(Box::new(emu::io::HeadlessIOHandler {}), mapper, audio);
        emu.cpu.pc = 0xc000;
        emu.cpu.sp = 0xfd;
        emu.toggle_quiet_mode(true);
        for _ in 0..1000 {
            emu.cycle();
        }
    }
}
