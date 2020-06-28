mod emu;
mod util;

use clap::clap_app;

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

    let mut emu: emu::Emulator = if matches.is_present("DISPLAY") {
        emu::Emulator::new()
    } else {
        emu::Emulator::new_headless()
    };

    let loader: &dyn emu::loaders::Loader =  match matches.value_of("LOADER") {
        Some("bin") => &emu::loaders::BinLoader{},
        Some("ascii") => &emu::loaders::AsciiLoader{},
        Some("nes") => &emu::loaders::InesLoader{},

        None => &emu::loaders::BinLoader{},

        _ => {
            println!("Invalid loader, see --help");
            std::process::exit(1);
        }
    };

    let code: Vec<u8> = loader.load(matches.value_of("INPUT").unwrap());
    emu.cpu.pc = loader.code_start();

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

    emu.install_rom(code, loader.offset());
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
        emu.install_rom(
            emu::loaders::load_ascii(&String::from("input/adc_zeropage")),
            emu::memory::CODE_START_ADDR,
        );
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
        emu.install_rom(
            emu::loaders::load_ascii(&String::from("input/instructions")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.a, 0x84);
        assert_eq!(emu.cpu.x, 0xc1);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), true);
    }

    #[test]
    fn test_lda_sta() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            emu::loaders::load_ascii(&String::from("input/ldasta")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.a, 8);
        assert_eq!(emu.mem.read_bus(0x200), 1);
        assert_eq!(emu.mem.read_bus(0x201), 5);
        assert_eq!(emu.mem.read_bus(0x202), 8);
    }

    #[test]
    fn test_transfers() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            emu::loaders::load_ascii(&String::from("input/transfers")),
            emu::memory::CODE_START_ADDR,
        );
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
        emu.install_rom(
            emu::loaders::load_ascii(&String::from("input/sbc")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.a, 0xfc);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), true);
    }

    #[test]
    fn test_stores() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            emu::loaders::load_ascii(&String::from("input/stores")),
            emu::memory::CODE_START_ADDR,
        );
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
        emu.install_rom(
            emu::loaders::load_ascii(&String::from("input/compares")),
            emu::memory::CODE_START_ADDR,
        );
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
        emu.install_rom(
            emu::loaders::load_ascii(&String::from("input/bne")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.x, 3);
        assert_eq!(emu.mem.read_bus(0x0201), 3);
        assert_eq!(emu.mem.read_bus(0x0200), 3);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_beq() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            emu::loaders::load_ascii(&String::from("input/beq")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.x, 1);
        assert_eq!(emu.mem.read_bus(0x0201), 1);
        assert_eq!(emu.mem.read_bus(0x0200), 1);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_take_no_branch() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            emu::loaders::load_ascii(&String::from("input/take_no_branch")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.y, 8);
    }

    #[test]
    fn test_take_all_branches() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            emu::loaders::load_ascii(&String::from("input/take_all_branches")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.x, 8);
    }

    #[test]
    fn test_stackloop() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            emu::loaders::load_ascii(&String::from("input/stackloop")),
            emu::memory::CODE_START_ADDR,
        );
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
        emu.install_rom(
            emu::loaders::load_ascii(&String::from("input/jmp")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.a, 0x03);
        assert_eq!(emu.mem.read_bus(0x200), 0x03);
    }

    #[test]
    fn test_jsrrts() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            emu::loaders::load_ascii(&String::from("input/jsrtrs")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.x, 0x15);
        assert_eq!(emu.cpu.sp, 0xfd);
        assert_eq!(emu.mem.read_bus(0x1fe), 0x08);
        assert_eq!(emu.mem.read_bus(0x1ff), 0x06);
    }

    #[test]
    fn test_klaus_2m5() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            emu::loaders::load_bin(&String::from("input/6502_functional_test.bin")),
            0,
        );
        emu.cpu.pc = 0x400;
        emu.toggle_debug_on_infinite_loop(false);
        emu.toggle_quiet_mode(true);
        emu.toggle_verbose_mode(false);
        emu.run();

        assert_eq!(emu.cpu.pc, 0x3469);
    }
}
