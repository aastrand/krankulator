pub mod opcodes;

#[derive(Debug)]
pub struct Cpu {
    pub pc: u16,

    pub a: u8,
    pub x: u8,
    pub y: u8,

    pub stack: u8,
    status: u8
}

impl Cpu {
    pub fn new() -> Cpu {
        Cpu{pc: 0x400, a: 0, x: 0, y: 0, stack: 0, status: 0}
    }
}
