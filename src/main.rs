mod asm;
mod emu;

use asm::util;

use clap::clap_app;

fn main() {
    let matches = clap_app!(myapp =>
        (version: "0.1")
        (author: "Anders Å. <aastrand@gmail.com>")
        (@arg DISPLAY: --display "Use a mapped display")
        (@arg BIN: -b --binary "Read input as binary format")
        (@arg VERBOSE: -v --verbose "Verbose mode")
        (@arg QUIET_MODE: -q --quiet "Quiet mode, overrides verbose")
        (@arg DEBUG: -d --debg "Debug on infinite loop")
        (@arg BREAKPOINT: -p --breakpoint +multiple "Add a breakpint")

        (@arg INPUT: +required "Sets the input file to use")
    )
    .get_matches();

    let mut emu: emu::Emulator = if matches.is_present("DISPLAY") {
        emu::Emulator::new()
    } else {
        emu::Emulator::new_headless()
    };

    let offset: u16 = if matches.is_present("BIN") {
        0 // Binary loads the complete memory
    } else {
        emu::memory::CODE_START_ADDR
    };
    let code: Vec<u8> = if matches.is_present("BIN") {
        emu.cpu.pc = 0x400; // TODO: this needs cleanup
        util::read_code_bin(matches.value_of("INPUT").unwrap())
    } else {
        util::read_code_ascii(matches.value_of("INPUT").unwrap())
    };

    if matches.is_present("BREAKPOINT") {
        for breakpoint in matches.values_of("BREAKPOINT").unwrap() {
            println!("Adding breakpoint at {}", breakpoint);
            emu::dbg::toggle_breakpoint(breakpoint, &mut emu.breakpoints);
        }
    }

    emu.install_rom(code, offset);
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
            util::read_code_ascii(&String::from("input/adc_zeropage")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.a, 0x0);
        assert_eq!(emu.mem.ram[0x1], 0x80);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
    }

    #[test]
    fn test_instructions() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            util::read_code_ascii(&String::from("input/instructions")),
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
            util::read_code_ascii(&String::from("input/ldasta")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.a, 8);
        assert_eq!(emu.mem.ram[0x200], 1);
        assert_eq!(emu.mem.ram[0x201], 5);
        assert_eq!(emu.mem.ram[0x202], 8);
    }

    #[test]
    fn test_transfers() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            util::read_code_ascii(&String::from("input/transfers")),
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
            util::read_code_ascii(&String::from("input/sbc")),
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
            util::read_code_ascii(&String::from("input/stores")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.a, 1);
        assert_eq!(emu.cpu.x, 2);
        assert_eq!(emu.cpu.y, 3);
        assert_eq!(emu.mem.ram[1], 1);
        assert_eq!(emu.mem.ram[2], 2);
        assert_eq!(emu.mem.ram[3], 3);
        assert_eq!(emu.mem.ram[0x0100], 1);
        assert_eq!(emu.mem.ram[0x0200], 2);
        assert_eq!(emu.mem.ram[0x0300], 3);
    }

    #[test]
    fn test_compares() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            util::read_code_ascii(&String::from("input/compares")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.a, 1);
        assert_eq!(emu.cpu.x, 0);
        assert_eq!(emu.cpu.y, 1);
        assert_eq!(emu.mem.ram[0x100], 1);
        assert_eq!(emu.cpu.zero_flag(), true);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
    }

    #[test]
    fn test_bne() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            util::read_code_ascii(&String::from("input/bne")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.x, 3);
        assert_eq!(emu.mem.ram[0x0201], 3);
        assert_eq!(emu.mem.ram[0x0200], 3);
        assert_eq!(emu.cpu.zero_flag(), true);
    }

    #[test]
    fn test_beq() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            util::read_code_ascii(&String::from("input/beq")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.x, 1);
        assert_eq!(emu.mem.ram[0x0201], 1);
        assert_eq!(emu.mem.ram[0x0200], 1);
        assert_eq!(emu.cpu.zero_flag(), false);
    }

    #[test]
    fn test_take_no_branch() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            util::read_code_ascii(&String::from("input/take_no_branch")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.y, 8);
    }

    #[test]
    fn test_take_all_branches() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            util::read_code_ascii(&String::from("input/take_all_branches")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.x, 8);
    }

    #[test]
    fn test_stackloop() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            util::read_code_ascii(&String::from("input/stackloop")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.a, 0);
        assert_eq!(emu.cpu.x, 0x10);
        assert_eq!(emu.cpu.y, 0x20);
        assert_eq!(emu.cpu.sp, 0xff);

        for sp in 0..15 {
            assert_eq!(emu.mem.ram[(0x1ff - sp) as usize], sp as u8);
            assert_eq!(emu.mem.ram[(0x200 + sp) as usize], sp as u8);
            assert_eq!(emu.mem.ram[(0x21f - sp) as usize], sp as u8);
        }
    }

    #[test]
    fn test_jmp() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            util::read_code_ascii(&String::from("input/jmp")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.a, 0x03);
        assert_eq!(emu.mem.ram[0x200], 0x03);
    }

    #[test]
    fn test_jsrrts() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            util::read_code_ascii(&String::from("input/jsrtrs")),
            emu::memory::CODE_START_ADDR,
        );
        emu.run();

        assert_eq!(emu.cpu.x, 0x15);
        assert_eq!(emu.cpu.sp, 0xfd);
        assert_eq!(emu.mem.ram[0x1fe], 0x08);
        assert_eq!(emu.mem.ram[0x1ff], 0x06);
    }

    #[test]
    fn test_klaus_2m5() {
        let mut emu: emu::Emulator = emu::Emulator::new_headless();
        emu.install_rom(
            util::read_code_bin(&String::from("input/6502_functional_test.bin")),
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
