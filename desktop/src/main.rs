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
    input: String,
}

fn main() -> Result<(), String> {
    let args = Args::parse();

    let mut emu = match args.loader.as_str() {
        "bin" => {
            let loader: Box<dyn loader::Loader> = Box::new(loader::BinLoader {});
            let result = loader.load(&args.input);
            match result {
                Ok(mapper) => emu::Emulator::new_headless(mapper),
                Err(msg) => panic!("{}", msg),
            }
        }
        "ascii" => {
            let loader: Box<dyn loader::Loader> = Box::new(loader::AsciiLoader {});
            let result = loader.load(&args.input);
            match result {
                Ok(mapper) => emu::Emulator::new_headless(mapper),
                Err(msg) => panic!("{}", msg),
            }
        }
        "nes" => {
            let loader: Box<dyn loader::Loader> = loader::InesLoader::new();
            let file = &args.input;
            match loader.load(file) {
                Ok(mapper) => {
                    let mut emu: emu::Emulator = if args.wav_out.is_some() {
                        emu::Emulator::new_capturing(mapper)
                    } else if !args.headless {
                        let audio = Box::new(audio::AudioOutput::try_new(emu::apu::SAMPLE_RATE)
                            .expect("No audio output device available"));
                        let io = Box::new(io::WinitPixelsIOHandler::new(256, 240));
                        emu::Emulator::new_with(io, mapper, audio)
                    } else {
                        emu::Emulator::new_headless(mapper)
                    };

                    emu.cpu.status = 0x34;
                    emu.cpu.sp = 0xfd;
                    emu.toggle_should_trigger_nmi(true);
                    emu.toggle_should_exit_on_infinite_loop(false);
                    emu.set_rom_path(file);

                    emu
                }
                Err(msg) => panic!("{}", msg),
            }
        }
        _ => {
            println!("Invalid loader, see --help");
            std::process::exit(1);
        }
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

    emu.run();

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
        let mut emu = emu::Emulator::new_with(
            Box::new(emu::io::HeadlessIOHandler {}),
            mapper,
            audio,
        );
        emu.cpu.pc = 0xc000;
        emu.cpu.sp = 0xfd;
        emu.toggle_quiet_mode(true);
        for _ in 0..1000 {
            emu.cycle();
        }
    }
}

