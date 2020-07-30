mod emu;
mod util;

use clap::clap_app;
use emu::io::loader;

fn main() -> Result<(), String> {
    let matches = clap_app!(myapp =>
        (version: "0.1")
        (author: "Anders Å. <aastrand@gmail.com>")
        (@arg DISPLAY: --display "Use a mapped display")
        (@arg LOADER: -l --loader +takes_value "Specify loader: nes (default), ascii, bin")
        (@arg VERBOSE: -v --verbose "Verbose mode")
        (@arg QUIET_MODE: -q --quiet "Quiet mode, overrides verbose")
        (@arg DEBUG: -d --debg "Debug on infinite loop")
        (@arg BREAKPOINT: -p --breakpoint +multiple "Add a breakpint")
        (@arg CODEADDR: -c --codeaddr +takes_value "Starting address of code")

        (@arg INPUT: +required "Sets the input file to use")
    )
    .get_matches();

    let mut emu = match matches.value_of("LOADER") {
        Some("bin") => {
            let mut loader: Box<dyn loader::Loader> = Box::new(loader::BinLoader {});
            emu::Emulator::new_headless(loader.load(matches.value_of("INPUT").unwrap()))
        }
        Some("ascii") => {
            let mut loader: Box<dyn loader::Loader> = Box::new(loader::AsciiLoader {});
            emu::Emulator::new_headless(loader.load(matches.value_of("INPUT").unwrap()))
        }
        None | Some("nes") => {
            let mut loader: Box<dyn loader::Loader> = loader::InesLoader::new();
            let mapper = loader.load(matches.value_of("INPUT").unwrap());

            let mut emu: emu::Emulator = if matches.is_present("DISPLAY") {
                let sdl_context = sdl2::init()?;

                let video_subsystem = sdl_context.video()?;
                let window = video_subsystem
                    .window("Krankulator", 256 * 2, 240 * 2)
                    .position_centered()
                    .build()
                    .map_err(|e| e.to_string())?;
                let canvas = window
                    .into_canvas()
                    .target_texture()
                    .present_vsync()
                    .build()
                    .map_err(|e| e.to_string())?;

                emu::Emulator::new(mapper, sdl_context, canvas)
            } else {
                emu::Emulator::new_headless(mapper)
            };

            emu.cpu.status = 0x34;
            emu.cpu.sp = 0xfd;
            emu.toggle_should_trigger_nmi(true);

            emu
        }
        _ => {
            println!("Invalid loader, see --help");
            std::process::exit(1);
        }
    };

    if matches.is_present("BREAKPOINT") {
        for breakpoint in matches.values_of("BREAKPOINT").unwrap() {
            println!("Adding breakpoint at {}", breakpoint);
            emu::dbg::toggle_breakpoint(breakpoint, &mut emu.breakpoints);
        }
    }

    if matches.is_present("CODEADDR") {
        let input_addr = matches.value_of("CODEADDR").unwrap();
        match util::hex_str_to_u16(input_addr) {
            Ok(addr) => emu.cpu.pc = addr,
            _ => {
                println!("Invalid code addr: {}", input_addr);
                std::process::exit(1);
            }
        };
    }

    emu.toggle_verbose_mode(matches.is_present("VERBOSE") & !matches.is_present("QUIET_MODE"));
    emu.toggle_quiet_mode(matches.is_present("QUIET_MODE"));
    emu.toggle_debug_on_infinite_loop(matches.is_present("DEBUG"));

    emu.run();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instructions() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from("input/ascii/instructions")));
        emu.run();

        assert_eq!(emu.cpu.a, 0x84);
        assert_eq!(emu.cpu.x, 0xc1);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), true);
    }

    #[test]
    fn test_lda_sta() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from("input/ascii/ldasta")));
        emu.run();

        assert_eq!(emu.cpu.a, 8);
        assert_eq!(emu.mem.cpu_read(0x200), 1);
        assert_eq!(emu.mem.cpu_read(0x201), 5);
        assert_eq!(emu.mem.cpu_read(0x202), 8);
    }

    #[test]
    fn test_transfers() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from("input/ascii/transfers")));
        emu.run();

        assert_eq!(emu.cpu.a, 0x42);
        assert_eq!(emu.cpu.x, 0x42);
        assert_eq!(emu.cpu.y, 0x43);
        assert_eq!(emu.cpu.sp, 0x42);
        assert_eq!(emu.cpu.zero_flag(), false);
        assert_eq!(emu.cpu.negative_flag(), false);
    }

    #[test]
    fn test_subtract_with_carry() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from("input/ascii/sbc")));
        emu.run();

        assert_eq!(emu.cpu.a, 0xfc);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), true);
    }

    #[test]
    fn test_stores() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from("input/ascii/stores")));
        emu.run();

        assert_eq!(emu.cpu.a, 1);
        assert_eq!(emu.cpu.x, 2);
        assert_eq!(emu.cpu.y, 3);
        assert_eq!(emu.mem.cpu_read(1), 1);
        assert_eq!(emu.mem.cpu_read(2), 2);
        assert_eq!(emu.mem.cpu_read(3), 3);
        assert_eq!(emu.mem.cpu_read(0x0100), 1);
        assert_eq!(emu.mem.cpu_read(0x0200), 2);
        assert_eq!(emu.mem.cpu_read(0x0300), 3);
    }

    #[test]
    fn test_compares() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from("input/ascii/compares")));
        emu.run();

        assert_eq!(emu.cpu.a, 1);
        assert_eq!(emu.cpu.x, 0);
        assert_eq!(emu.cpu.y, 1);
        assert_eq!(emu.mem.cpu_read(0x100), 1);
        assert_eq!(emu.cpu.zero_flag(), true);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
    }

    #[test]
    fn test_bne() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from("input/ascii/bne")));
        emu.run();

        assert_eq!(emu.cpu.x, 3);
        assert_eq!(emu.mem.cpu_read(0x0201), 3);
        assert_eq!(emu.mem.cpu_read(0x0200), 3);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_beq() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from("input/ascii/beq")));
        emu.run();

        assert_eq!(emu.cpu.x, 1);
        assert_eq!(emu.mem.cpu_read(0x0201), 1);
        assert_eq!(emu.mem.cpu_read(0x0200), 1);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_take_no_branch() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from("input/ascii/take_no_branch")));
        emu.run();

        assert_eq!(emu.cpu.y, 8);
    }

    #[test]
    fn test_take_all_branches() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from(
            "input/ascii/take_all_branches",
        )));
        emu.run();

        assert_eq!(emu.cpu.x, 8);
    }

    #[test]
    fn test_stackloop() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from("input/ascii/stackloop")));
        emu.run();

        assert_eq!(emu.cpu.a, 0);
        assert_eq!(emu.cpu.x, 0x10);
        assert_eq!(emu.cpu.y, 0x20);
        assert_eq!(emu.cpu.sp, 0xff);

        for sp in 0..15 {
            assert_eq!(emu.mem.cpu_read(0x1ff - sp), sp as u8);
            assert_eq!(emu.mem.cpu_read(0x200 + sp), sp as u8);
            assert_eq!(emu.mem.cpu_read(0x21f - sp), sp as u8);
        }
    }

    #[test]
    fn test_jmp() {
        let mut emu = emu::Emulator::new_headless(loader::load_ascii(&String::from("input/ascii/jmp")));
        emu.run();

        assert_eq!(emu.cpu.a, 0x03);
        assert_eq!(emu.mem.cpu_read(0x200), 0x03);
    }

    #[test]
    fn test_jsrrts() {
        let mut emu =
            emu::Emulator::new_headless(loader::load_ascii(&String::from("input/ascii/jsrtrs")));
        emu.run();

        assert_eq!(emu.cpu.x, 0x15);
        assert_eq!(emu.cpu.sp, 0xfd);
        assert_eq!(emu.mem.cpu_read(0x1fe), 0x08);
        assert_eq!(emu.mem.cpu_read(0x1ff), 0x06);
    }

    #[test]
    fn test_klaus_2m5() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_bin(&String::from(
            "input/bin/6502_functional_test.bin",
        )));
        emu.toggle_should_trigger_nmi(false);
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);
        emu.run();

        assert_eq!(emu.cpu.pc, 0x3469);
    }

    #[test]
    fn test_nes_nestest() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_nes(&String::from(
            "input/nes/nestest.nes",
        )));
        emu.cpu.pc = 0xc000;
        emu.cpu.sp = 0xfd;
        emu.cycles = 7;
        emu.cpu.cycle = 7;
        emu.mem.ppu().cycle = 21;
        emu.cpu.set_status_flag(emu::cpu::INTERRUPT_BIT);

        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);

        if let Ok(lines) = util::read_lines(&String::from("input/nes/nestest.log")) {
            for line in lines {
                if let Ok(expected) = line {
                    let expected_addr = &expected[0..4];
                    let pc = emu.cpu.pc;

                    let expected_status = &expected[65..67];
                    let status = emu.cpu.status;

                    let expected_ppu_cycles: String = (&expected[82..85])
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect();
                    let expected_ppu_scanline: String = (&expected[78..81])
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect();
                    let expected_cycles = &expected[90..];

                    let mut cycles: u64 = emu.cpu.cycle;
                    let mut ppu_cycles: u16 = emu.mem.ppu().cycle;
                    let mut ppu_scanline: u16 = emu.mem.ppu().scanline;

                    'next_instr: loop {
                        let state = emu.cycle();
                        match state {
                            emu::CycleState::CpuAhead => {
                                cycles = emu.cpu.cycle;
                                ppu_cycles = emu.mem.ppu().cycle;
                                ppu_scanline = emu.mem.ppu().scanline;
                            }
                            _ => break 'next_instr,
                        }
                    }

                    assert_eq!(util::hex_str_to_u16(expected_addr).ok().unwrap(), pc);
                    assert_eq!(util::hex_str_to_u8(expected_status).ok().unwrap(), status);
                    assert_eq!(expected_cycles.parse::<u64>().unwrap(), cycles);
                    assert_eq!(expected_ppu_cycles.parse::<u16>().unwrap(), ppu_cycles);
                    assert_eq!(expected_ppu_scanline.parse::<u16>().unwrap(), ppu_scanline);
                } else {
                    panic!("Error iterating over nesttest.log");
                }
            }
        } else {
            panic!("Could not read nestest.log");
        }
    }

    #[test]
    fn test_nes_instr_test() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_nes(&String::from(
            "input/nes/all_instrs.nes",
        )));
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);

        emu.run();

        let expected = String::from("All 16 tests passed");

        let mut buf = String::new();
        let mut idx = 0x6004;
        for _ in 0..expected.len() {
            let chr = emu.mem.cpu_read(idx);
            if chr == 0 {
                break;
            } else {
                buf.push(chr as char);
            }
            idx += 1;
        }

        assert_eq!(expected, buf);
    }

    // TODO: get these working
    /*#[test]
    fn test_nes_instr_timing_test() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_nes(&String::from(
            "input/nes/instr_timing.nes",
        )));
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);

        emu.run();

        let expected = String::from("All 16 tests passed");

        let mut buf = String::new();
        let mut idx = 0x6004;
        //for _ in 0..expected.len() {
        loop {
            let chr = emu.mem.cpu_read(idx);
            if chr == 0 {
                break;
            } else {
                buf.push(chr as char);
            }
            idx += 1;
        }

        println!("{}", buf);
        assert_eq!(expected, buf);
    }

    #[test]
    fn test_nes_oam_read() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_nes(&String::from(
            "input/nes/ppu/oam_read.nes",
        )));
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);

        emu.run();

        let expected = String::from("All 16 tests passed");

        let mut buf = String::new();
        let mut idx = 0x6004;
        //for _ in 0..expected.len() {
        loop {
            let chr = emu.mem.cpu_read(idx);
            if chr == 0 {
                break;
            } else {
                buf.push(chr as char);
            }
            idx += 1;
        }

        println!("{:X}", emu.mem.cpu_read(0x6000));

        println!("{}", buf);
        assert_eq!(expected, buf);
    }

    #[test]
    fn test_nes_vram_access() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless(loader::load_nes(&String::from(
            "input/nes/ppu/vram_access.nes",
        )));
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);

        emu.run();

        let expected = String::from("All 16 tests passed");

        let mut buf = String::new();
        let mut idx = 0x6004;
        //for _ in 0..expected.len() {
        loop {
            let chr = emu.mem.cpu_read(idx);
            if chr == 0 {
                break;
            } else {
                buf.push(chr as char);
            }
            idx += 1;
        }

        println!("{:X}", emu.mem.cpu_read(0x6000));

        println!("{}", buf);
        assert_eq!(expected, buf);
    }*/
}
