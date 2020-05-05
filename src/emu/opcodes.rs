pub const ADC_IMM: u8 = 0x69;
pub const ADC_ZP: u8 = 0x65;

pub const SBC_IMM: u8 = 0xe9;
pub const SBC_ZP: u8 = 0xe5;

/*
MNEMONIC                       HEX
BPL (Branch on PLus)           $10
BMI (Branch on MInus)          $30
BVC (Branch on oVerflow Clear) $50
BVS (Branch on oVerflow Set)   $70
BCC (Branch on Carry Clear)    $90
BCS (Branch on Carry Set)      $B0
BNE (Branch on Not Equal)      $D0
BEQ (Branch on EQual)          $F0
*/
pub const BPL: u8 = 0x10;
pub const BMI: u8 = 0x30;
pub const BVC: u8 = 0x50;
pub const BVS: u8 = 0x70;
pub const BCC: u8 = 0x90;
pub const BCS: u8 = 0xb0;
pub const BNE: u8 = 0xd0;
pub const BEQ: u8 = 0xf0;

pub const BRK: u8 = 0x0;

pub const CMP_ABS: u8 = 0xcd;
pub const CMP_IMM: u8 = 0xc9;
pub const CMP_ZP: u8 = 0xc5;

pub const CPX_ABS: u8 = 0x0ec;
pub const CPX_IMM: u8 = 0xe0;
pub const CPX_ZP: u8 = 0xe4;

pub const CPY_ABS: u8 = 0xcc;
pub const CPY_IMM: u8 = 0xc0;
pub const CPY_ZP: u8 = 0xc4;

pub const INX: u8 = 0xe8;
pub const INY: u8 = 0xc8;
pub const DEX: u8 = 0xca;
pub const DEY: u8 = 0x88;

pub const LDA_ABS: u8 = 0xad;
pub const LDA_ABX: u8 = 0xbd;
pub const LDA_ABY: u8 = 0xb9;
pub const LDA_IMM: u8 = 0xa9;
pub const LDA_INX: u8 = 0xa1;
pub const LDA_INY: u8 = 0xb1;
pub const LDA_ZP: u8 = 0xa5;
pub const LDA_ZPX: u8 = 0xb5;

pub const LDX_ABS: u8 = 0xae;
pub const LDX_IMM: u8 = 0xa2;
pub const LDY_ABS: u8 = 0xac;
pub const LDY_IMM: u8 = 0xa0;

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

pub const PHA: u8 = 0x48;
pub const PLA: u8 = 0x68;
pub const PHP: u8 = 0x08;
pub const PLP: u8 = 0x28;

//pub const SED: u8 = 0xf8;

pub const STA_ABS: u8 = 0x8d;
pub const STA_ZP: u8 = 0x85;
pub const STA_ABY: u8 = 0x99;

pub const STX_ABS: u8 = 0x8e;
pub const STX_ZP: u8 = 0x86;
pub const STY_ABS: u8 = 0x8c;
pub const STY_ZP: u8 = 0x84;

pub const TAX: u8 = 0xaa;
pub const TXA: u8 = 0x8a;
pub const TAY: u8 = 0xa8;
pub const TYA: u8 = 0x98;

pub const TSX: u8 = 0xba;
pub const TXS: u8 = 0x9a;

pub struct Opcode {
    name: &'static str,
    size: u16,
}

pub struct Lookup {
    opcodes: [&'static Opcode; 256],
}

static _SENTINEL: Opcode = Opcode {
    name: "OPCODE MISSING IN LOOKUP SEE opcodes.rs",
    size: 0,
};

impl Lookup {
    pub fn new() -> Lookup {
        let mut lookup: [&'static Opcode; 256] = [&_SENTINEL; 256];
        lookup[ADC_IMM as usize] = &Opcode {
            name: "ADC_IMM",
            size: 2,
        };
        lookup[ADC_ZP as usize] = &Opcode {
            name: "ADC_ZP",
            size: 2,
        };

        lookup[BPL as usize] = &Opcode {
            name: "BPL",
            size: 2,
        };
        lookup[BMI as usize] = &Opcode {
            name: "BMI",
            size: 2,
        };
        lookup[BVC as usize] = &Opcode {
            name: "BVC",
            size: 2,
        };
        lookup[BVS as usize] = &Opcode {
            name: "BVS",
            size: 2,
        };
        lookup[BCC as usize] = &Opcode {
            name: "BCC",
            size: 2,
        };
        lookup[BCS as usize] = &Opcode {
            name: "BCS",
            size: 2,
        };
        lookup[BEQ as usize] = &Opcode {
            name: "BEQ",
            size: 2,
        };
        lookup[BNE as usize] = &Opcode {
            name: "BNE",
            size: 2,
        };

        lookup[CLC as usize] = &Opcode {
            name: "CLC",
            size: 1,
        };

        lookup[CMP_ABS as usize] = &Opcode {
            name: "CMP_ABS",
            size: 3,
        };
        lookup[CMP_IMM as usize] = &Opcode {
            name: "CMP_IMM",
            size: 2,
        };
        lookup[CMP_ZP as usize] = &Opcode {
            name: "CMP_ZP",
            size: 2,
        };
        lookup[CPX_ABS as usize] = &Opcode {
            name: "CPX_ABS",
            size: 3,
        };
        lookup[CPX_IMM as usize] = &Opcode {
            name: "CPX_IMM",
            size: 2,
        };
        lookup[CPX_ZP as usize] = &Opcode {
            name: "CPX_ZP",
            size: 2,
        };
        lookup[CPY_ABS as usize] = &Opcode {
            name: "CPY_ABS",
            size: 3,
        };
        lookup[CPY_IMM as usize] = &Opcode {
            name: "CPY_IMM",
            size: 2,
        };
        lookup[CPY_ZP as usize] = &Opcode {
            name: "CPY_ZP",
            size: 2,
        };

        lookup[DEX as usize] = &Opcode {
            name: "DEX",
            size: 1,
        };
        lookup[DEY as usize] = &Opcode {
            name: "DEY",
            size: 1,
        };
        lookup[INX as usize] = &Opcode {
            name: "INX",
            size: 1,
        };
        lookup[INY as usize] = &Opcode {
            name: "INY",
            size: 1,
        };

        lookup[LDA_ABS as usize] = &Opcode {
            name: "LDA_ABS",
            size: 3,
        };
        lookup[LDA_ABX as usize] = &Opcode {
            name: "LDA_ABX",
            size: 3,
        };
        lookup[LDA_ABY as usize] = &Opcode {
            name: "LDA_ABY",
            size: 3,
        };
        lookup[LDA_IMM as usize] = &Opcode {
            name: "LDA_IMM",
            size: 2,
        };
        lookup[LDA_INX as usize] = &Opcode {
            name: "LDA_INX",
            size: 2,
        };
        lookup[LDA_INY as usize] = &Opcode {
            name: "LDA_INY",
            size: 3,
        };
        lookup[LDA_ZP as usize] = &Opcode {
            name: "LDA_ZP",
            size: 2,
        };
        lookup[LDA_ZPX as usize] = &Opcode {
            name: "LDA_ZPX",
            size: 2,
        };

        lookup[LDX_IMM as usize] = &Opcode {
            name: "LDX_IMM",
            size: 2,
        };
        lookup[LDY_ABS as usize] = &Opcode {
            name: "LDY_ABS",
            size: 2,
        };
        lookup[LDY_IMM as usize] = &Opcode {
            name: "LDY_IMM",
            size: 2,
        };

        lookup[PHA as usize] = &Opcode {
            name: "PHA",
            size: 1,
        };
        lookup[PLA as usize] = &Opcode {
            name: "PLA",
            size: 1,
        };
        lookup[PHP as usize] = &Opcode {
            name: "PHP",
            size: 1,
        };
        lookup[PLP as usize] = &Opcode {
            name: "PLP",
            size: 1,
        };

        lookup[SBC_IMM as usize] = &Opcode {
            name: "SBC_IMM",
            size: 2,
        };
        lookup[SBC_ZP as usize] = &Opcode {
            name: "SBC_ZP",
            size: 2,
        };

        lookup[SEC as usize] = &Opcode {
            name: "SEC",
            size: 1,
        };

        lookup[STA_ABS as usize] = &Opcode {
            name: "STA_ABS",
            size: 3,
        };
        lookup[STA_ZP as usize] = &Opcode {
            name: "STA_ZP",
            size: 2,
        };
        lookup[STA_ABY as usize] = &Opcode {
            name: "STA_ABY",
            size: 3,
        };
        lookup[STX_ABS as usize] = &Opcode {
            name: "STX_ABS",
            size: 3,
        };
        lookup[STX_ZP as usize] = &Opcode {
            name: "STX_ZP",
            size: 2,
        };
        lookup[STY_ABS as usize] = &Opcode {
            name: "STY_ABS",
            size: 3,
        };
        lookup[STY_ZP as usize] = &Opcode {
            name: "STY_ZP",
            size: 2,
        };

        lookup[TAX as usize] = &Opcode {
            name: "TAX",
            size: 1,
        };
        lookup[TXA as usize] = &Opcode {
            name: "TXA",
            size: 1,
        };
        lookup[TAY as usize] = &Opcode {
            name: "TAY",
            size: 1,
        };
        lookup[TYA as usize] = &Opcode {
            name: "TYA",
            size: 1,
        };

        lookup[TSX as usize] = &Opcode {
            name: "TSX",
            size: 1,
        };
        lookup[TXS as usize] = &Opcode {
            name: "TXS",
            size: 1,
        };

        Lookup { opcodes: lookup }
    }

    pub fn name(&self, opcode: u8) -> &str {
        self.opcodes[opcode as usize].name
    }

    pub fn size(&self, opcode: u8) -> u16 {
        self.opcodes[opcode as usize].size
    }
}
