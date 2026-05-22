mod audio;
mod gamepad;
mod io;

use clap::Parser;
use krankulator_core::emu;
use krankulator_core::emu::io::loader;
use krankulator_core::util;

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

    /// Input file to use
    #[clap()]
    input: Option<String>,
}

fn main() -> Result<(), String> {
    let args = Args::parse();

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
            "nes" => {
                let l: Box<dyn loader::Loader> = loader::InesLoader::new();
                match l.load(input) {
                    Ok(mapper) => {
                        let mut emu: emu::Emulator = if args.wav_out.is_some() {
                            emu::Emulator::new_capturing(mapper)
                        } else if !args.headless {
                            let audio = Box::new(
                                audio::AudioOutput::try_new(emu::apu::SAMPLE_RATE)
                                    .expect("No audio output device available"),
                            );
                            let rom_name = std::path::Path::new(input.as_str())
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or(input);
                            let io = Box::new(io::WinitPixelsIOHandler::new(256, 240, rom_name));
                            emu::Emulator::new_with(io, mapper, audio)
                        } else {
                            emu::Emulator::new_headless(mapper)
                        };

                        emu.cpu.status = 0x34;
                        emu.cpu.sp = 0xfd;
                        emu.toggle_should_trigger_nmi(true);
                        emu.toggle_should_exit_on_infinite_loop(false);
                        emu.set_rom_path(input);
                        io::add_recent_rom(input);

                        emu
                    }
                    Err(msg) => panic!("{}", msg),
                }
            }
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
        let io = Box::new(io::WinitPixelsIOHandler::new(256, 240, "krankulator"));
        let mut emu = emu::Emulator::new_with(io, mapper, audio);
        emu.toggle_should_exit_on_infinite_loop(false);
        emu.toggle_should_trigger_nmi(false);
        emu.overlay.set_banner(Some("Open a ROM to play".into()));
        emu
    };

    for breakpoint in args.breakpoint {
        println!("Adding breakpoint at {}", breakpoint);
        emu::dbg::toggle_breakpoint(&breakpoint, &mut emu.breakpoints);
    }

    if args.codeaddr.is_some() {
        let input_addr = args.codeaddr.unwrap();
        match util::hex_str_to_u16(&input_addr) {
            Ok(addr) => emu.cpu.pc = addr,
            _ => {
                println!("Invalid code addr: {}", input_addr);
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
            Some(path) => {
                let l: Box<dyn loader::Loader> = loader::InesLoader::new();
                match l.load(&path) {
                    Ok(mapper) => {
                        emu.load_rom(mapper, &path);
                        io::add_recent_rom(&path);
                    }
                    Err(msg) => {
                        eprintln!("Failed to load ROM: {}", msg);
                        emu.overlay.toast(msg);
                    }
                }
            }
            None => break,
        }
    }

    if let Some(wav_path) = &args.wav_out {
        let samples = emu.drain_captured_audio();
        emu::audio::wav::write_wav(wav_path, &samples, 44100)
            .map_err(|e| format!("Failed to write WAV: {}", e))?;
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
    use krankulator_core::test_input;

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
        let mapper = loader::load_nes(&String::from(test_input!("nes/nestest.nes")));
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
