mod asm;
mod emu;

use asm::util;

use std::env;

fn help() {
    println!("Usage: krankulator <path-to-code>");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.len() {
        2 => {
            let mut emu: emu::Emulator = emu::Emulator::new();
            emu.install_rom(util::read_code(&args[1]));
            emu.run();
        },
        _ => help()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adc_zeropage() {
        let mut emu: emu::Emulator = emu::Emulator::new();
        emu.install_rom(util::read_code(&String::from("input/adc_zeropage")));
        emu.run();

        assert_eq!(emu.cpu.a, 0x0);
        assert_eq!(emu.mem.ram[0x1], 0x80);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.zero_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), false);
    }

    #[test]
    fn test_instructions() {
        let mut emu: emu::Emulator = emu::Emulator::new();
        emu.install_rom(util::read_code(&String::from("input/instructions")));
        emu.run();

        assert_eq!(emu.cpu.a, 0x84);
        assert_eq!(emu.cpu.x, 0xc1);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), true);
    }

    #[test]
    fn test_lda_sta() {
        let mut emu: emu::Emulator = emu::Emulator::new();
        emu.install_rom(util::read_code(&String::from("input/ldasta")));
        emu.run();

        assert_eq!(emu.cpu.a, 8);
        assert_eq!(emu.mem.ram[0x200], 1);
        assert_eq!(emu.mem.ram[0x201], 5);
        assert_eq!(emu.mem.ram[0x202], 8);
    }

    #[test]
    fn test_transfers() {
        let mut emu: emu::Emulator = emu::Emulator::new();
        emu.install_rom(util::read_code(&String::from("input/transfers")));
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
        let mut emu: emu::Emulator = emu::Emulator::new();
        emu.install_rom(util::read_code(&String::from("input/sbc")));
        emu.run();

        assert_eq!(emu.cpu.a, 0xfc);
        assert_eq!(emu.cpu.carry_flag(), true);
        assert_eq!(emu.cpu.negative_flag(), true);
    }

    #[test]
    fn test_stores() {
        let mut emu: emu::Emulator = emu::Emulator::new();
        emu.install_rom(util::read_code(&String::from("input/stores")));
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
}
