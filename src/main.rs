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
            emu::run(util::read_code(&args[1]))
        },
        _ => help()
    }
}
