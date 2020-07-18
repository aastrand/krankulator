mod emu;
mod util;

use clap::clap_app;
use emu::io::loader;

fn main() {
    let matches = clap_app!(myapp =>
        (version: "0.1")
        (author: "Anders Å. <aastrand@gmail.com>")
        (@arg DISPLAY: --display "Use a mapped display")
        (@arg LOADER: -l --loader +takes_value "Specify loader: bin (default), ascii, nes")
        (@arg VERBOSE: -v --verbose "Verbose mode")
        (@arg QUIET_MODE: -q --quiet "Quiet mode, overrides verbose")
        (@arg DEBUG: -d --debg "Debug on infinite loop")
        (@arg BREAKPOINT: -p --breakpoint +multiple "Add a breakpint")
        (@arg CODEADDR: -c --codeaddr +takes_value "Starting address of code")

        (@arg INPUT: +required "Sets the input file to use")
    )
    .get_matches();

    let loader: &dyn loader::Loader =  match matches.value_of("LOADER") {
        Some("bin") => &loader::BinLoader{},
        Some("ascii") => &loader::AsciiLoader{},
        Some("nes") => &loader::InesLoader{},

        None => &loader::BinLoader{},

        _ => {
            println!("Invalid loader, see --help");
            std::process::exit(1);
        }
    };

    let mut emu: emu::Emulator = if matches.is_present("DISPLAY") {
        emu::Emulator::new()
    } else {
        emu::Emulator::new_headless()
    };

    emu.cpu.pc = loader.code_start();
    emu.install_mapper(loader.load(matches.value_of("INPUT").unwrap()));

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adc_zeropage() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_mapper(loader::load_ascii(&String::from("input/adc_zeropage")));
        emu.run();

        assert_eq!(emu.cpu.a, 0x0);
        assert_eq!(emu.mem.read_bus(0x1), 0x80);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
    }

    #[test]
    fn test_instructions() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_mapper(loader::load_ascii(&String::from("input/instructions")));
        emu.run();

        assert_eq!(emu.cpu.a, 0x84);
        assert_eq!(emu.cpu.x, 0xc1);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), true);
    }

    #[test]
    fn test_lda_sta() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_mapper(loader::load_ascii(&String::from("input/ldasta")));
        emu.run();

        assert_eq!(emu.cpu.a, 8);
        assert_eq!(emu.mem.read_bus(0x200), 1);
        assert_eq!(emu.mem.read_bus(0x201), 5);
        assert_eq!(emu.mem.read_bus(0x202), 8);
    }

    #[test]
    fn test_transfers() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_mapper(loader::load_ascii(&String::from("input/transfers")));
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
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_mapper(loader::load_ascii(&String::from("input/sbc")));
        emu.run();

        assert_eq!(emu.cpu.a, 0xfc);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), true);
    }

    #[test]
    fn test_stores() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_mapper(loader::load_ascii(&String::from("input/stores")));
        emu.run();

        assert_eq!(emu.cpu.a, 1);
        assert_eq!(emu.cpu.x, 2);
        assert_eq!(emu.cpu.y, 3);
        assert_eq!(emu.mem.read_bus(1), 1);
        assert_eq!(emu.mem.read_bus(2), 2);
        assert_eq!(emu.mem.read_bus(3), 3);
        assert_eq!(emu.mem.read_bus(0x0100), 1);
        assert_eq!(emu.mem.read_bus(0x0200), 2);
        assert_eq!(emu.mem.read_bus(0x0300), 3);
    }

    #[test]
    fn test_compares() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_mapper(loader::load_ascii(&String::from("input/compares")));
        emu.run();

        assert_eq!(emu.cpu.a, 1);
        assert_eq!(emu.cpu.x, 0);
        assert_eq!(emu.cpu.y, 1);
        assert_eq!(emu.mem.read_bus(0x100), 1);
        assert_eq!(emu.cpu.zero_flag(), true);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
    }

    #[test]
    fn test_bne() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_mapper(loader::load_ascii(&String::from("input/bne")));
        emu.run();

        assert_eq!(emu.cpu.x, 3);
        assert_eq!(emu.mem.read_bus(0x0201), 3);
        assert_eq!(emu.mem.read_bus(0x0200), 3);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_beq() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_mapper(loader::load_ascii(&String::from("input/beq")));
        emu.run();

        assert_eq!(emu.cpu.x, 1);
        assert_eq!(emu.mem.read_bus(0x0201), 1);
        assert_eq!(emu.mem.read_bus(0x0200), 1);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_take_no_branch() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_mapper(loader::load_ascii(&String::from("input/take_no_branch")));
        emu.run();

        assert_eq!(emu.cpu.y, 8);
    }

    #[test]
    fn test_take_all_branches() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_mapper(loader::load_ascii(&String::from("input/take_all_branches")));
        emu.run();

        assert_eq!(emu.cpu.x, 8);
    }

    #[test]
    fn test_stackloop() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_mapper(loader::load_ascii(&String::from("input/stackloop")));
        emu.run();

        assert_eq!(emu.cpu.a, 0);
        assert_eq!(emu.cpu.x, 0x10);
        assert_eq!(emu.cpu.y, 0x20);
        assert_eq!(emu.cpu.sp, 0xff);

        for sp in 0..15 {
            assert_eq!(emu.mem.read_bus(0x1ff - sp), sp as u8);
            assert_eq!(emu.mem.read_bus(0x200 + sp), sp as u8);
            assert_eq!(emu.mem.read_bus(0x21f - sp), sp as u8);
        }
    }

    #[test]
    fn test_jmp() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_mapper(loader::load_ascii(&String::from("input/jmp")));
        emu.run();

        assert_eq!(emu.cpu.a, 0x03);
        assert_eq!(emu.mem.read_bus(0x200), 0x03);
    }

    #[test]
    fn test_jsrrts() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_mapper(loader::load_ascii(&String::from("input/jsrtrs")));
        emu.run();

        assert_eq!(emu.cpu.x, 0x15);
        assert_eq!(emu.cpu.sp, 0xfd);
        assert_eq!(emu.mem.read_bus(0x1fe), 0x08);
        assert_eq!(emu.mem.read_bus(0x1ff), 0x06);
    }

    #[test]
    fn test_klaus_2m5() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_mapper(loader::load_bin(&String::from("input/6502_functional_test.bin")));
        emu.cpu.pc = 0x400;
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);
        emu.run();

        assert_eq!(emu.cpu.pc, 0x3469);
    }

    //#[test]
    fn test_nestest() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_mapper(loader::load_nes(&String::from("input/nestest.nes")));
        emu.cpu.pc = 0xc000;
        emu.cpu.sp = 0xfd;
        emu.cpu.set_status_flag(emu::cpu::INTERRUPT_BIT);

        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);

        if let Ok(lines) = util::read_lines(&String::from("input/nestest.log")) {
            for line in lines {
                if let Ok(expected) = line {
                    println!("{}", expected);
                    println!("{}", emu.log_str());

                    let expected_addr = &expected[0..4];
                    if util::hex_str_to_u16(expected_addr).ok().unwrap() != emu.cpu.pc {
                        panic!("Deviated from nestest.log");
                    }

                    let opcode = emu.execute_instruction();
                } else {
                    panic!("Error iterating over nesttest.log");
                }
            }
        } else {
            panic!("Could not read nestest.log");
        }

        //assert_eq!(emu.cpu.pc, 0x3469);
    }
}
