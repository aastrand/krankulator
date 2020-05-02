const BRK: u8 = 0x0;
const LDA: u8 = 0xa9;
const STA: u8 = 0x8d;

fn main() {
    let mut mem: [u8; 65536] = [0; 65536];

    let mut pc: u16 = 0x400;

    let mut a: u8 = 0;
    let mut x: u8 = 0;
    let mut y: u8 = 0;

    let mut stack: u8 = 0;
    let mut status: u8 = 0;

    /*
    LDA #$01
    STA $0200
    LDA #$05
    STA $0201
    LDA #$08
    STA $0202
    */
    let rom: [u8; 15] = [
    0xa9, 0x01,
    0x8d, 0x00, 0x02,
    0xa9, 0x05,
    0x8d, 0x01, 0x02,
    0xa9, 0x08,
    0x8d, 0x02, 0x02
    ];

    let mut i = 0;
    for code in rom.iter() {
        mem[0x400 + i] = *code;
        i += 1;
    }

    loop {
        let opcode = mem[pc as usize];
        match opcode {
            BRK => {
                println!("BRK");
                break;
            },
            LDA => {
                // TODO: addressing modes
                a = mem[pc as usize+1];
                println!("LDA 0x{:x}, a={:x}", pc+1, a);
                pc += 2
            },
            STA => {
                let addr: u16 = ((mem[pc as usize+2] as u16) << 8) + mem[pc as usize+1] as u16;
                mem[addr as usize] = a;
                println!("STA 0x{:x}, a={:x}", addr, a);
                pc += 3
            },
            _ => panic!("Unkown opcode: 0x{:x}", opcode)
        }
    }
}
