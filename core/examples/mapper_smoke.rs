//! Headless smoke test for mapper verification.
//!
//! Runs a ROM for N frames while tapping Start/A to get past title screens,
//! then reports framebuffer statistics so a script (or human) can judge
//! whether the game boots and renders:
//!
//! ```sh
//! cargo run --release -p krankulator-core --example mapper_smoke -- game.nes \
//!     [--frames 900] [--ppm out.ppm]
//! ```

use std::cell::RefCell;
use std::rc::Rc;

use krankulator_core::emu;
use krankulator_core::emu::audio::SilentAudioOutput;
use krankulator_core::emu::gfx::buf::Buffer;
use krankulator_core::emu::io::{controller, IOHandler, PollResult};
use krankulator_core::emu::memory::MemoryMapper;

struct Shared {
    frame: Vec<u8>,
    width: usize,
    height: usize,
    frames_rendered: u64,
    frame_hashes: Vec<u64>,
    polls: u64,
}

struct SmokeIO {
    shared: Rc<RefCell<Shared>>,
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in data {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

impl IOHandler for SmokeIO {
    fn init(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn log(&self, logline: String) {
        eprintln!("{logline}");
    }

    fn poll(&mut self, mem: &mut dyn MemoryMapper, _apu: &mut emu::apu::APU) -> PollResult {
        let mut shared = self.shared.borrow_mut();
        shared.polls += 1;
        // Tap Start then A periodically to get past title/menu screens
        let frame = shared.frames_rendered;
        let pressed = match frame % 240 {
            120..=130 => controller::START,
            180..=190 => controller::A,
            _ => 0,
        };
        mem.controllers()[0].load_status(pressed);
        PollResult::default()
    }

    fn render(&mut self, buf: &Buffer) {
        let mut shared = self.shared.borrow_mut();
        shared.frame.clear();
        shared.frame.extend_from_slice(&buf.data);
        shared.width = buf.width;
        shared.height = buf.height;
        shared.frames_rendered += 1;
        if shared.frames_rendered % 60 == 0 {
            let h = fnv1a(&buf.data);
            shared.frame_hashes.push(h);
        }
    }

    fn exit(&self, s: String) {
        eprintln!("{s}");
    }
}

fn write_ppm(path: &str, shared: &Shared) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    writeln!(f, "P6\n{} {}\n255", shared.width, shared.height)?;
    f.write_all(&shared.frame)?;
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut rom_path = None;
    let mut frames: u64 = 900;
    let mut ppm_out = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--frames" => {
                i += 1;
                frames = args[i].parse().expect("bad --frames value");
            }
            "--ppm" => {
                i += 1;
                ppm_out = Some(args[i].clone());
            }
            p => rom_path = Some(p.to_string()),
        }
        i += 1;
    }

    let rom_path = rom_path.expect("usage: mapper_smoke <rom.nes> [--frames N] [--ppm out.ppm]");
    let bytes = std::fs::read(&rom_path).expect("failed to read ROM");

    let mapper_id = if bytes.len() > 8 {
        let low = (bytes[6] >> 4) as u16;
        let high = (bytes[7] & 0xF0) as u16;
        low | high
    } else {
        0
    };

    let mapper = match emu::io::loader::load_nes_from_bytes(&bytes) {
        Ok(m) => m,
        Err(e) => {
            println!("result=load_error mapper={mapper_id} error=\"{e}\"");
            std::process::exit(2);
        }
    };

    let region = emu::io::loader::detect_region_with_filename(&bytes, Some(&rom_path));

    let shared = Rc::new(RefCell::new(Shared {
        frame: vec![],
        width: 0,
        height: 0,
        frames_rendered: 0,
        frame_hashes: vec![],
        polls: 0,
    }));

    let io = Box::new(SmokeIO {
        shared: shared.clone(),
    });

    let mut emulator =
        emu::Emulator::new_with_region(io, mapper, Box::new(SilentAudioOutput::new()), region);
    emulator.cpu.status = 0x34;
    emulator.cpu.sp = 0xfd;
    emulator.toggle_should_trigger_nmi(true);
    emulator.toggle_should_exit_on_infinite_loop(false);
    emulator.toggle_quiet_mode(true);
    if let Err(msg) = emulator.init() {
        println!("result=init_error mapper={mapper_id} error=\"{msg}\"");
        std::process::exit(2);
    }

    let mut early_exit = false;
    for _ in 0..frames {
        if !emulator.run_one_frame() {
            early_exit = true;
            break;
        }
    }

    let shared = shared.borrow();
    let mut colors = std::collections::HashSet::new();
    for px in shared.frame.chunks_exact(3) {
        colors.insert([px[0], px[1], px[2]]);
    }
    let unique_hashes: std::collections::HashSet<_> = shared.frame_hashes.iter().collect();

    if let Some(path) = &ppm_out {
        if !shared.frame.is_empty() {
            write_ppm(path, &shared).expect("failed to write ppm");
        }
    }

    println!(
        "result={} mapper={} frames={} colors={} unique_frames={} final_hash={:016x}",
        if early_exit { "early_exit" } else { "ok" },
        mapper_id,
        shared.frames_rendered,
        colors.len(),
        unique_hashes.len(),
        shared.frame_hashes.last().copied().unwrap_or(0),
    );
}
