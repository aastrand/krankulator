pub const ADC_IMM: u8 = 0x69;
pub const ADC_ZP: u8 = 0x65;

pub const SBC_IMM: u8 = 0xe9;
pub const SBC_ZP: u8 = 0xe5;

pub const BRK: u8 = 0x0;
pub const INX: u8 = 0xe8;
pub const INY: u8 = 0xc8;
pub const DEX: u8 = 0xca;
pub const DEY: u8 = 0x88;

pub const LDA_ABS: u8 = 0xa9;

/*
MNEMONIC                       HEX
CLC (CLear Carry)              $18
SEC (SEt Carry)                $38
CLI (CLear Interrupt)          $58
SEI (SEt Interrupt)            $78
CLV (CLear oVerflow)           $B8
CLD (CLear Decimal)            $D8
SED (SEt Decimal)              $F8
*/
pub const CLC: u8 = 0x18;
pub const SEC: u8 = 0x38;
#[allow(dead_code)]
pub const CLI: u8 = 0x58;
#[allow(dead_code)]
pub const SEI: u8 = 0x78;
#[allow(dead_code)]
pub const CLV: u8 = 0xb8;
#[allow(dead_code)]
pub const CLD: u8 = 0xd8;
#[allow(dead_code)]
pub const SED: u8 = 0xf8;

pub const STA_ABS: u8 = 0x8d;
pub const STA_ZP: u8 = 0x85;

pub const TAX: u8 = 0xaa;
pub const TXA: u8 = 0x8a;
pub const TAY: u8 = 0xa8;
pub const TYA: u8 = 0x98;

pub const TSX: u8 = 0xba;
pub const TXS: u8 = 0x9a;
