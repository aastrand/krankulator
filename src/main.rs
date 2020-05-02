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
    fn test_instructions() {
        let mut emu: emu::Emulator = emu::Emulator::new();
        emu.install_rom(util::read_code(&String::from("input/instructions")));
        emu.run();

        assert_eq!(emu.cpu.a, 0x85);
        assert_eq!(emu.cpu.x, 0xc1);
        assert_eq!(emu.cpu.overflow_flag(), true);

    }
}
