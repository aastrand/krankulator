// GENERATED BY generate_opcodes.py

pub const ADC_IMM: u8 = 0x69; // (ADd with Carry)
pub const ADC_ZP: u8 = 0x65; // (ADd with Carry)
pub const ADC_ZPX: u8 = 0x75; // (ADd with Carry)
pub const ADC_ABS: u8 = 0x6d; // (ADd with Carry)
pub const ADC_ABX: u8 = 0x7d; // (ADd with Carry)
pub const ADC_ABY: u8 = 0x79; // (ADd with Carry)
pub const ADC_INX: u8 = 0x61; // (ADd with Carry)
pub const ADC_INY: u8 = 0x71; // (ADd with Carry)
pub const AND_IMM: u8 = 0x29; // (bitwise AND with accumulator)
pub const AND_ZP: u8 = 0x25; // (bitwise AND with accumulator)
pub const AND_ZPX: u8 = 0x35; // (bitwise AND with accumulator)
pub const AND_ABS: u8 = 0x2d; // (bitwise AND with accumulator)
pub const AND_ABX: u8 = 0x3d; // (bitwise AND with accumulator)
pub const AND_ABY: u8 = 0x39; // (bitwise AND with accumulator)
pub const AND_INX: u8 = 0x21; // (bitwise AND with accumulator)
pub const AND_INY: u8 = 0x31; // (bitwise AND with accumulator)
pub const ASL: u8 = 0x0a; // (Arithmetic Shift Left)
pub const ASL_ZP: u8 = 0x06; // (Arithmetic Shift Left)
pub const ASL_ZPX: u8 = 0x16; // (Arithmetic Shift Left)
pub const ASL_ABS: u8 = 0x0e; // (Arithmetic Shift Left)
pub const ASL_ABX: u8 = 0x1e; // (Arithmetic Shift Left)
pub const BIT_ZP: u8 = 0x24; // (test BITs)
pub const BIT_ABS: u8 = 0x2c; // (test BITs)
pub const BPL: u8 = 0x10; // (Branch on PLus)          
pub const BMI: u8 = 0x30; // (Branch on MInus)         
pub const BVC: u8 = 0x50; // (Branch on oVerflow Clear)
pub const BVS: u8 = 0x70; // (Branch on oVerflow Set)  
pub const BCC: u8 = 0x90; // (Branch on Carry Clear)   
pub const BCS: u8 = 0xb0; // (Branch on Carry Set)     
pub const BNE: u8 = 0xd0; // (Branch on Not Equal)     
pub const BEQ: u8 = 0xf0; // (Branch on EQual)         
pub const BRK: u8 = 0x00; // (BReaK)
pub const CMP_IMM: u8 = 0xc9; // (CoMPare accumulator)
pub const CMP_ZP: u8 = 0xc5; // (CoMPare accumulator)
pub const CMP_ZPX: u8 = 0xd5; // (CoMPare accumulator)
pub const CMP_ABS: u8 = 0xcd; // (CoMPare accumulator)
pub const CMP_ABX: u8 = 0xdd; // (CoMPare accumulator)
pub const CMP_ABY: u8 = 0xd9; // (CoMPare accumulator)
pub const CMP_INX: u8 = 0xc1; // (CoMPare accumulator)
pub const CMP_INY: u8 = 0xd1; // (CoMPare accumulator)
pub const CPX_IMM: u8 = 0xe0; // (ComPare X register)
pub const CPX_ZP: u8 = 0xe4; // (ComPare X register)
pub const CPX_ABS: u8 = 0xec; // (ComPare X register)
pub const CPY_IMM: u8 = 0xc0; // (ComPare Y register)
pub const CPY_ZP: u8 = 0xc4; // (ComPare Y register)
pub const CPY_ABS: u8 = 0xcc; // (ComPare Y register)
pub const DEC_ZP: u8 = 0xc6; // (DECrement memory)
pub const DEC_ZPX: u8 = 0xd6; // (DECrement memory)
pub const DEC_ABS: u8 = 0xce; // (DECrement memory)
pub const DEC_ABX: u8 = 0xde; // (DECrement memory)
pub const EOR_IMM: u8 = 0x49; // (bitwise Exclusive OR)
pub const EOR_ZP: u8 = 0x45; // (bitwise Exclusive OR)
pub const EOR_ZPX: u8 = 0x55; // (bitwise Exclusive OR)
pub const EOR_ABS: u8 = 0x4d; // (bitwise Exclusive OR)
pub const EOR_ABX: u8 = 0x5d; // (bitwise Exclusive OR)
pub const EOR_ABY: u8 = 0x59; // (bitwise Exclusive OR)
pub const EOR_INX: u8 = 0x41; // (bitwise Exclusive OR)
pub const EOR_INY: u8 = 0x51; // (bitwise Exclusive OR)
pub const CLC: u8 = 0x18; // (CLear Carry)             
pub const SEC: u8 = 0x38; // (SEt Carry)               
pub const CLI: u8 = 0x58; // (CLear Interrupt)         
pub const SEI: u8 = 0x78; // (SEt Interrupt)           
pub const CLV: u8 = 0xb8; // (CLear oVerflow)          
pub const CLD: u8 = 0xd8; // (CLear Decimal)           
pub const SED: u8 = 0xf8; // (SEt Decimal)             
pub const INC_ZP: u8 = 0xe6; // (INCrement memory)
pub const INC_ZPX: u8 = 0xf6; // (INCrement memory)
pub const INC_ABS: u8 = 0xee; // (INCrement memory)
pub const INC_ABX: u8 = 0xfe; // (INCrement memory)
pub const JMP_ABS: u8 = 0x4c; // (JuMP)
pub const JMP_IND: u8 = 0x6c; // (JuMP)
pub const JSR_ABS: u8 = 0x20; // (Jump to SubRoutine)
pub const LDA_IMM: u8 = 0xa9; // (LoaD Accumulator)
pub const LDA_ZP: u8 = 0xa5; // (LoaD Accumulator)
pub const LDA_ZPX: u8 = 0xb5; // (LoaD Accumulator)
pub const LDA_ABS: u8 = 0xad; // (LoaD Accumulator)
pub const LDA_ABX: u8 = 0xbd; // (LoaD Accumulator)
pub const LDA_ABY: u8 = 0xb9; // (LoaD Accumulator)
pub const LDA_INX: u8 = 0xa1; // (LoaD Accumulator)
pub const LDA_INY: u8 = 0xb1; // (LoaD Accumulator)
pub const LDX_IMM: u8 = 0xa2; // (LoaD X register)
pub const LDX_ZP: u8 = 0xa6; // (LoaD X register)
pub const LDX_ZPY: u8 = 0xb6; // (LoaD X register)
pub const LDX_ABS: u8 = 0xae; // (LoaD X register)
pub const LDX_ABY: u8 = 0xbe; // (LoaD X register)
pub const LDY_IMM: u8 = 0xa0; // (LoaD Y register)
pub const LDY_ZP: u8 = 0xa4; // (LoaD Y register)
pub const LDY_ZPX: u8 = 0xb4; // (LoaD Y register)
pub const LDY_ABS: u8 = 0xac; // (LoaD Y register)
pub const LDY_ABX: u8 = 0xbc; // (LoaD Y register)
pub const LSR: u8 = 0x4a; // (Logical Shift Right)
pub const LSR_ZP: u8 = 0x46; // (Logical Shift Right)
pub const LSR_ZPX: u8 = 0x56; // (Logical Shift Right)
pub const LSR_ABS: u8 = 0x4e; // (Logical Shift Right)
pub const LSR_ABX: u8 = 0x5e; // (Logical Shift Right)
pub const NOP: u8 = 0xea; // (No OPeration)
pub const ORA_IMM: u8 = 0x09; // (bitwise OR with Accumulator)
pub const ORA_ZP: u8 = 0x05; // (bitwise OR with Accumulator)
pub const ORA_ZPX: u8 = 0x15; // (bitwise OR with Accumulator)
pub const ORA_ABS: u8 = 0x0d; // (bitwise OR with Accumulator)
pub const ORA_ABX: u8 = 0x1d; // (bitwise OR with Accumulator)
pub const ORA_ABY: u8 = 0x19; // (bitwise OR with Accumulator)
pub const ORA_INX: u8 = 0x01; // (bitwise OR with Accumulator)
pub const ORA_INY: u8 = 0x11; // (bitwise OR with Accumulator)
pub const TAX: u8 = 0xaa; // (Transfer A to X)   
pub const TXA: u8 = 0x8a; // (Transfer X to A)   
pub const DEX: u8 = 0xca; // (DEcrement X)       
pub const INX: u8 = 0xe8; // (INcrement X)       
pub const TAY: u8 = 0xa8; // (Transfer A to Y)   
pub const TYA: u8 = 0x98; // (Transfer Y to A)   
pub const DEY: u8 = 0x88; // (DEcrement Y)       
pub const INY: u8 = 0xc8; // (INcrement Y)       
pub const ROL: u8 = 0x2a; // (ROtate Left)
pub const ROL_ZP: u8 = 0x26; // (ROtate Left)
pub const ROL_ZPX: u8 = 0x36; // (ROtate Left)
pub const ROL_ABS: u8 = 0x2e; // (ROtate Left)
pub const ROL_ABX: u8 = 0x3e; // (ROtate Left)
pub const ROR: u8 = 0x6a; // (ROtate Right)
pub const ROR_ZP: u8 = 0x66; // (ROtate Right)
pub const ROR_ZPX: u8 = 0x76; // (ROtate Right)
pub const ROR_ABS: u8 = 0x6e; // (ROtate Right)
pub const ROR_ABX: u8 = 0x7e; // (ROtate Right)
pub const RTI: u8 = 0x40; // (ReTurn from Interrupt)
pub const RTS: u8 = 0x60; // (ReTurn from Subroutine)
pub const SBC_IMM: u8 = 0xe9; // (SuBtract with Carry)
pub const SBC_ZP: u8 = 0xe5; // (SuBtract with Carry)
pub const SBC_ZPX: u8 = 0xf5; // (SuBtract with Carry)
pub const SBC_ABS: u8 = 0xed; // (SuBtract with Carry)
pub const SBC_ABX: u8 = 0xfd; // (SuBtract with Carry)
pub const SBC_ABY: u8 = 0xf9; // (SuBtract with Carry)
pub const SBC_INX: u8 = 0xe1; // (SuBtract with Carry)
pub const SBC_INY: u8 = 0xf1; // (SuBtract with Carry)
pub const STA_ZP: u8 = 0x85; // (STore Accumulator)
pub const STA_ZPX: u8 = 0x95; // (STore Accumulator)
pub const STA_ABS: u8 = 0x8d; // (STore Accumulator)
pub const STA_ABX: u8 = 0x9d; // (STore Accumulator)
pub const STA_ABY: u8 = 0x99; // (STore Accumulator)
pub const STA_INX: u8 = 0x81; // (STore Accumulator)
pub const STA_INY: u8 = 0x91; // (STore Accumulator)
pub const TXS: u8 = 0x9a; // (Transfer X to Stack ptr)  
pub const TSX: u8 = 0xba; // (Transfer Stack ptr to X)  
pub const PHA: u8 = 0x48; // (PusH Accumulator)         
pub const PLA: u8 = 0x68; // (PuLl Accumulator)         
pub const PHP: u8 = 0x08; // (PusH Processor status)    
pub const PLP: u8 = 0x28; // (PuLl Processor status)    
pub const STX_ZP: u8 = 0x86; // (STore X register)
pub const STX_ZPY: u8 = 0x96; // (STore X register)
pub const STX_ABS: u8 = 0x8e; // (STore X register)
pub const STY_ZP: u8 = 0x84; // (STore Y register)
pub const STY_ZPX: u8 = 0x94; // (STore Y register)
pub const STY_ABS: u8 = 0x8c; // (STore Y register)

pub struct Opcode {
    name: &'static str,
    size: u16,
    #[allow(dead_code)]
    cycles: u8,
}

pub struct Lookup {
    opcodes: [&'static Opcode; 256],
}

static _SENTINEL: Opcode = Opcode {
    name: "OPCODE MISSING IN LOOKUP SEE opcodes.rs",
    size: 0xffff,
    cycles: 255,
};

impl Lookup {
    pub fn new() -> Lookup {
        let mut lookup: [&'static Opcode; 256] = [&_SENTINEL; 256];
        lookup[ADC_IMM as usize] = &Opcode { // (ADd with Carry)
            name: "ADC_IMM",
            size: 2,
            cycles: 2,
        };
        lookup[ADC_ZP as usize] = &Opcode { // (ADd with Carry)
            name: "ADC_ZP",
            size: 2,
            cycles: 3,
        };
        lookup[ADC_ZPX as usize] = &Opcode { // (ADd with Carry)
            name: "ADC_ZPX",
            size: 2,
            cycles: 4,
        };
        lookup[ADC_ABS as usize] = &Opcode { // (ADd with Carry)
            name: "ADC_ABS",
            size: 3,
            cycles: 4,
        };
        lookup[ADC_ABX as usize] = &Opcode { // (ADd with Carry)
            name: "ADC_ABX",
            size: 3,
            cycles: 4,
        };
        lookup[ADC_ABY as usize] = &Opcode { // (ADd with Carry)
            name: "ADC_ABY",
            size: 3,
            cycles: 4,
        };
        lookup[ADC_INX as usize] = &Opcode { // (ADd with Carry)
            name: "ADC_INX",
            size: 2,
            cycles: 6,
        };
        lookup[ADC_INY as usize] = &Opcode { // (ADd with Carry)
            name: "ADC_INY",
            size: 2,
            cycles: 5,
        };
        lookup[AND_IMM as usize] = &Opcode { // (bitwise AND with accumulator)
            name: "AND_IMM",
            size: 2,
            cycles: 2,
        };
        lookup[AND_ZP as usize] = &Opcode { // (bitwise AND with accumulator)
            name: "AND_ZP",
            size: 2,
            cycles: 3,
        };
        lookup[AND_ZPX as usize] = &Opcode { // (bitwise AND with accumulator)
            name: "AND_ZPX",
            size: 2,
            cycles: 4,
        };
        lookup[AND_ABS as usize] = &Opcode { // (bitwise AND with accumulator)
            name: "AND_ABS",
            size: 3,
            cycles: 4,
        };
        lookup[AND_ABX as usize] = &Opcode { // (bitwise AND with accumulator)
            name: "AND_ABX",
            size: 3,
            cycles: 4,
        };
        lookup[AND_ABY as usize] = &Opcode { // (bitwise AND with accumulator)
            name: "AND_ABY",
            size: 3,
            cycles: 4,
        };
        lookup[AND_INX as usize] = &Opcode { // (bitwise AND with accumulator)
            name: "AND_INX",
            size: 2,
            cycles: 6,
        };
        lookup[AND_INY as usize] = &Opcode { // (bitwise AND with accumulator)
            name: "AND_INY",
            size: 2,
            cycles: 5,
        };
        lookup[ASL as usize] = &Opcode { // (Arithmetic Shift Left)
            name: "ASL",
            size: 1,
            cycles: 2,
        };
        lookup[ASL_ZP as usize] = &Opcode { // (Arithmetic Shift Left)
            name: "ASL_ZP",
            size: 2,
            cycles: 5,
        };
        lookup[ASL_ZPX as usize] = &Opcode { // (Arithmetic Shift Left)
            name: "ASL_ZPX",
            size: 2,
            cycles: 6,
        };
        lookup[ASL_ABS as usize] = &Opcode { // (Arithmetic Shift Left)
            name: "ASL_ABS",
            size: 3,
            cycles: 6,
        };
        lookup[ASL_ABX as usize] = &Opcode { // (Arithmetic Shift Left)
            name: "ASL_ABX",
            size: 3,
            cycles: 7,
        };
        lookup[BIT_ZP as usize] = &Opcode { // (test BITs)
            name: "BIT_ZP",
            size: 2,
            cycles: 3,
        };
        lookup[BIT_ABS as usize] = &Opcode { // (test BITs)
            name: "BIT_ABS",
            size: 3,
            cycles: 4,
        };
        lookup[BPL as usize] = &Opcode { // (Branch on PLus)          
            name: "BPL",
            size: 2,
            cycles: 2,
        };
        lookup[BMI as usize] = &Opcode { // (Branch on MInus)         
            name: "BMI",
            size: 2,
            cycles: 2,
        };
        lookup[BVC as usize] = &Opcode { // (Branch on oVerflow Clear)
            name: "BVC",
            size: 2,
            cycles: 2,
        };
        lookup[BVS as usize] = &Opcode { // (Branch on oVerflow Set)  
            name: "BVS",
            size: 2,
            cycles: 2,
        };
        lookup[BCC as usize] = &Opcode { // (Branch on Carry Clear)   
            name: "BCC",
            size: 2,
            cycles: 2,
        };
        lookup[BCS as usize] = &Opcode { // (Branch on Carry Set)     
            name: "BCS",
            size: 2,
            cycles: 2,
        };
        lookup[BNE as usize] = &Opcode { // (Branch on Not Equal)     
            name: "BNE",
            size: 2,
            cycles: 2,
        };
        lookup[BEQ as usize] = &Opcode { // (Branch on EQual)         
            name: "BEQ",
            size: 2,
            cycles: 2,
        };
        lookup[BRK as usize] = &Opcode { // (BReaK)
            name: "BRK",
            size: 1,
            cycles: 7,
        };
        lookup[CMP_IMM as usize] = &Opcode { // (CoMPare accumulator)
            name: "CMP_IMM",
            size: 2,
            cycles: 2,
        };
        lookup[CMP_ZP as usize] = &Opcode { // (CoMPare accumulator)
            name: "CMP_ZP",
            size: 2,
            cycles: 3,
        };
        lookup[CMP_ZPX as usize] = &Opcode { // (CoMPare accumulator)
            name: "CMP_ZPX",
            size: 2,
            cycles: 4,
        };
        lookup[CMP_ABS as usize] = &Opcode { // (CoMPare accumulator)
            name: "CMP_ABS",
            size: 3,
            cycles: 4,
        };
        lookup[CMP_ABX as usize] = &Opcode { // (CoMPare accumulator)
            name: "CMP_ABX",
            size: 3,
            cycles: 4,
        };
        lookup[CMP_ABY as usize] = &Opcode { // (CoMPare accumulator)
            name: "CMP_ABY",
            size: 3,
            cycles: 4,
        };
        lookup[CMP_INX as usize] = &Opcode { // (CoMPare accumulator)
            name: "CMP_INX",
            size: 2,
            cycles: 6,
        };
        lookup[CMP_INY as usize] = &Opcode { // (CoMPare accumulator)
            name: "CMP_INY",
            size: 2,
            cycles: 5,
        };
        lookup[CPX_IMM as usize] = &Opcode { // (ComPare X register)
            name: "CPX_IMM",
            size: 2,
            cycles: 2,
        };
        lookup[CPX_ZP as usize] = &Opcode { // (ComPare X register)
            name: "CPX_ZP",
            size: 2,
            cycles: 3,
        };
        lookup[CPX_ABS as usize] = &Opcode { // (ComPare X register)
            name: "CPX_ABS",
            size: 3,
            cycles: 4,
        };
        lookup[CPY_IMM as usize] = &Opcode { // (ComPare Y register)
            name: "CPY_IMM",
            size: 2,
            cycles: 2,
        };
        lookup[CPY_ZP as usize] = &Opcode { // (ComPare Y register)
            name: "CPY_ZP",
            size: 2,
            cycles: 3,
        };
        lookup[CPY_ABS as usize] = &Opcode { // (ComPare Y register)
            name: "CPY_ABS",
            size: 3,
            cycles: 4,
        };
        lookup[DEC_ZP as usize] = &Opcode { // (DECrement memory)
            name: "DEC_ZP",
            size: 2,
            cycles: 5,
        };
        lookup[DEC_ZPX as usize] = &Opcode { // (DECrement memory)
            name: "DEC_ZPX",
            size: 2,
            cycles: 6,
        };
        lookup[DEC_ABS as usize] = &Opcode { // (DECrement memory)
            name: "DEC_ABS",
            size: 3,
            cycles: 6,
        };
        lookup[DEC_ABX as usize] = &Opcode { // (DECrement memory)
            name: "DEC_ABX",
            size: 3,
            cycles: 7,
        };
        lookup[EOR_IMM as usize] = &Opcode { // (bitwise Exclusive OR)
            name: "EOR_IMM",
            size: 2,
            cycles: 2,
        };
        lookup[EOR_ZP as usize] = &Opcode { // (bitwise Exclusive OR)
            name: "EOR_ZP",
            size: 2,
            cycles: 3,
        };
        lookup[EOR_ZPX as usize] = &Opcode { // (bitwise Exclusive OR)
            name: "EOR_ZPX",
            size: 2,
            cycles: 4,
        };
        lookup[EOR_ABS as usize] = &Opcode { // (bitwise Exclusive OR)
            name: "EOR_ABS",
            size: 3,
            cycles: 4,
        };
        lookup[EOR_ABX as usize] = &Opcode { // (bitwise Exclusive OR)
            name: "EOR_ABX",
            size: 3,
            cycles: 4,
        };
        lookup[EOR_ABY as usize] = &Opcode { // (bitwise Exclusive OR)
            name: "EOR_ABY",
            size: 3,
            cycles: 4,
        };
        lookup[EOR_INX as usize] = &Opcode { // (bitwise Exclusive OR)
            name: "EOR_INX",
            size: 2,
            cycles: 6,
        };
        lookup[EOR_INY as usize] = &Opcode { // (bitwise Exclusive OR)
            name: "EOR_INY",
            size: 2,
            cycles: 5,
        };
        lookup[CLC as usize] = &Opcode { // (CLear Carry)             
            name: "CLC",
            size: 1,
            cycles: 2,
        };
        lookup[SEC as usize] = &Opcode { // (SEt Carry)               
            name: "SEC",
            size: 1,
            cycles: 2,
        };
        lookup[CLI as usize] = &Opcode { // (CLear Interrupt)         
            name: "CLI",
            size: 1,
            cycles: 2,
        };
        lookup[SEI as usize] = &Opcode { // (SEt Interrupt)           
            name: "SEI",
            size: 1,
            cycles: 2,
        };
        lookup[CLV as usize] = &Opcode { // (CLear oVerflow)          
            name: "CLV",
            size: 1,
            cycles: 2,
        };
        lookup[CLD as usize] = &Opcode { // (CLear Decimal)           
            name: "CLD",
            size: 1,
            cycles: 2,
        };
        lookup[SED as usize] = &Opcode { // (SEt Decimal)             
            name: "SED",
            size: 1,
            cycles: 2,
        };
        lookup[INC_ZP as usize] = &Opcode { // (INCrement memory)
            name: "INC_ZP",
            size: 2,
            cycles: 5,
        };
        lookup[INC_ZPX as usize] = &Opcode { // (INCrement memory)
            name: "INC_ZPX",
            size: 2,
            cycles: 6,
        };
        lookup[INC_ABS as usize] = &Opcode { // (INCrement memory)
            name: "INC_ABS",
            size: 3,
            cycles: 6,
        };
        lookup[INC_ABX as usize] = &Opcode { // (INCrement memory)
            name: "INC_ABX",
            size: 3,
            cycles: 7,
        };
        lookup[JMP_ABS as usize] = &Opcode { // (JuMP)
            name: "JMP_ABS",
            size: 3,
            cycles: 3,
        };
        lookup[JMP_IND as usize] = &Opcode { // (JuMP)
            name: "JMP_IND",
            size: 3,
            cycles: 5,
        };
        lookup[JSR_ABS as usize] = &Opcode { // (Jump to SubRoutine)
            name: "JSR_ABS",
            size: 3,
            cycles: 6,
        };
        lookup[LDA_IMM as usize] = &Opcode { // (LoaD Accumulator)
            name: "LDA_IMM",
            size: 2,
            cycles: 2,
        };
        lookup[LDA_ZP as usize] = &Opcode { // (LoaD Accumulator)
            name: "LDA_ZP",
            size: 2,
            cycles: 3,
        };
        lookup[LDA_ZPX as usize] = &Opcode { // (LoaD Accumulator)
            name: "LDA_ZPX",
            size: 2,
            cycles: 4,
        };
        lookup[LDA_ABS as usize] = &Opcode { // (LoaD Accumulator)
            name: "LDA_ABS",
            size: 3,
            cycles: 4,
        };
        lookup[LDA_ABX as usize] = &Opcode { // (LoaD Accumulator)
            name: "LDA_ABX",
            size: 3,
            cycles: 4,
        };
        lookup[LDA_ABY as usize] = &Opcode { // (LoaD Accumulator)
            name: "LDA_ABY",
            size: 3,
            cycles: 4,
        };
        lookup[LDA_INX as usize] = &Opcode { // (LoaD Accumulator)
            name: "LDA_INX",
            size: 2,
            cycles: 6,
        };
        lookup[LDA_INY as usize] = &Opcode { // (LoaD Accumulator)
            name: "LDA_INY",
            size: 2,
            cycles: 5,
        };
        lookup[LDX_IMM as usize] = &Opcode { // (LoaD X register)
            name: "LDX_IMM",
            size: 2,
            cycles: 2,
        };
        lookup[LDX_ZP as usize] = &Opcode { // (LoaD X register)
            name: "LDX_ZP",
            size: 2,
            cycles: 3,
        };
        lookup[LDX_ZPY as usize] = &Opcode { // (LoaD X register)
            name: "LDX_ZPY",
            size: 2,
            cycles: 4,
        };
        lookup[LDX_ABS as usize] = &Opcode { // (LoaD X register)
            name: "LDX_ABS",
            size: 3,
            cycles: 4,
        };
        lookup[LDX_ABY as usize] = &Opcode { // (LoaD X register)
            name: "LDX_ABY",
            size: 3,
            cycles: 4,
        };
        lookup[LDY_IMM as usize] = &Opcode { // (LoaD Y register)
            name: "LDY_IMM",
            size: 2,
            cycles: 2,
        };
        lookup[LDY_ZP as usize] = &Opcode { // (LoaD Y register)
            name: "LDY_ZP",
            size: 2,
            cycles: 3,
        };
        lookup[LDY_ZPX as usize] = &Opcode { // (LoaD Y register)
            name: "LDY_ZPX",
            size: 2,
            cycles: 4,
        };
        lookup[LDY_ABS as usize] = &Opcode { // (LoaD Y register)
            name: "LDY_ABS",
            size: 3,
            cycles: 4,
        };
        lookup[LDY_ABX as usize] = &Opcode { // (LoaD Y register)
            name: "LDY_ABX",
            size: 3,
            cycles: 4,
        };
        lookup[LSR as usize] = &Opcode { // (Logical Shift Right)
            name: "LSR",
            size: 1,
            cycles: 2,
        };
        lookup[LSR_ZP as usize] = &Opcode { // (Logical Shift Right)
            name: "LSR_ZP",
            size: 2,
            cycles: 5,
        };
        lookup[LSR_ZPX as usize] = &Opcode { // (Logical Shift Right)
            name: "LSR_ZPX",
            size: 2,
            cycles: 6,
        };
        lookup[LSR_ABS as usize] = &Opcode { // (Logical Shift Right)
            name: "LSR_ABS",
            size: 3,
            cycles: 6,
        };
        lookup[LSR_ABX as usize] = &Opcode { // (Logical Shift Right)
            name: "LSR_ABX",
            size: 3,
            cycles: 7,
        };
        lookup[NOP as usize] = &Opcode { // (No OPeration)
            name: "NOP",
            size: 1,
            cycles: 2,
        };
        lookup[ORA_IMM as usize] = &Opcode { // (bitwise OR with Accumulator)
            name: "ORA_IMM",
            size: 2,
            cycles: 2,
        };
        lookup[ORA_ZP as usize] = &Opcode { // (bitwise OR with Accumulator)
            name: "ORA_ZP",
            size: 2,
            cycles: 3,
        };
        lookup[ORA_ZPX as usize] = &Opcode { // (bitwise OR with Accumulator)
            name: "ORA_ZPX",
            size: 2,
            cycles: 4,
        };
        lookup[ORA_ABS as usize] = &Opcode { // (bitwise OR with Accumulator)
            name: "ORA_ABS",
            size: 3,
            cycles: 4,
        };
        lookup[ORA_ABX as usize] = &Opcode { // (bitwise OR with Accumulator)
            name: "ORA_ABX",
            size: 3,
            cycles: 4,
        };
        lookup[ORA_ABY as usize] = &Opcode { // (bitwise OR with Accumulator)
            name: "ORA_ABY",
            size: 3,
            cycles: 4,
        };
        lookup[ORA_INX as usize] = &Opcode { // (bitwise OR with Accumulator)
            name: "ORA_INX",
            size: 2,
            cycles: 6,
        };
        lookup[ORA_INY as usize] = &Opcode { // (bitwise OR with Accumulator)
            name: "ORA_INY",
            size: 2,
            cycles: 5,
        };
        lookup[TAX as usize] = &Opcode { // (Transfer A to X)   
            name: "TAX",
            size: 1,
            cycles: 2,
        };
        lookup[TXA as usize] = &Opcode { // (Transfer X to A)   
            name: "TXA",
            size: 1,
            cycles: 2,
        };
        lookup[DEX as usize] = &Opcode { // (DEcrement X)       
            name: "DEX",
            size: 1,
            cycles: 2,
        };
        lookup[INX as usize] = &Opcode { // (INcrement X)       
            name: "INX",
            size: 1,
            cycles: 2,
        };
        lookup[TAY as usize] = &Opcode { // (Transfer A to Y)   
            name: "TAY",
            size: 1,
            cycles: 2,
        };
        lookup[TYA as usize] = &Opcode { // (Transfer Y to A)   
            name: "TYA",
            size: 1,
            cycles: 2,
        };
        lookup[DEY as usize] = &Opcode { // (DEcrement Y)       
            name: "DEY",
            size: 1,
            cycles: 2,
        };
        lookup[INY as usize] = &Opcode { // (INcrement Y)       
            name: "INY",
            size: 1,
            cycles: 2,
        };
        lookup[ROL as usize] = &Opcode { // (ROtate Left)
            name: "ROL",
            size: 1,
            cycles: 2,
        };
        lookup[ROL_ZP as usize] = &Opcode { // (ROtate Left)
            name: "ROL_ZP",
            size: 2,
            cycles: 5,
        };
        lookup[ROL_ZPX as usize] = &Opcode { // (ROtate Left)
            name: "ROL_ZPX",
            size: 2,
            cycles: 6,
        };
        lookup[ROL_ABS as usize] = &Opcode { // (ROtate Left)
            name: "ROL_ABS",
            size: 3,
            cycles: 6,
        };
        lookup[ROL_ABX as usize] = &Opcode { // (ROtate Left)
            name: "ROL_ABX",
            size: 3,
            cycles: 7,
        };
        lookup[ROR as usize] = &Opcode { // (ROtate Right)
            name: "ROR",
            size: 1,
            cycles: 2,
        };
        lookup[ROR_ZP as usize] = &Opcode { // (ROtate Right)
            name: "ROR_ZP",
            size: 2,
            cycles: 5,
        };
        lookup[ROR_ZPX as usize] = &Opcode { // (ROtate Right)
            name: "ROR_ZPX",
            size: 2,
            cycles: 6,
        };
        lookup[ROR_ABS as usize] = &Opcode { // (ROtate Right)
            name: "ROR_ABS",
            size: 3,
            cycles: 6,
        };
        lookup[ROR_ABX as usize] = &Opcode { // (ROtate Right)
            name: "ROR_ABX",
            size: 3,
            cycles: 7,
        };
        lookup[RTI as usize] = &Opcode { // (ReTurn from Interrupt)
            name: "RTI",
            size: 1,
            cycles: 6,
        };
        lookup[RTS as usize] = &Opcode { // (ReTurn from Subroutine)
            name: "RTS",
            size: 1,
            cycles: 6,
        };
        lookup[SBC_IMM as usize] = &Opcode { // (SuBtract with Carry)
            name: "SBC_IMM",
            size: 2,
            cycles: 2,
        };
        lookup[SBC_ZP as usize] = &Opcode { // (SuBtract with Carry)
            name: "SBC_ZP",
            size: 2,
            cycles: 3,
        };
        lookup[SBC_ZPX as usize] = &Opcode { // (SuBtract with Carry)
            name: "SBC_ZPX",
            size: 2,
            cycles: 4,
        };
        lookup[SBC_ABS as usize] = &Opcode { // (SuBtract with Carry)
            name: "SBC_ABS",
            size: 3,
            cycles: 4,
        };
        lookup[SBC_ABX as usize] = &Opcode { // (SuBtract with Carry)
            name: "SBC_ABX",
            size: 3,
            cycles: 4,
        };
        lookup[SBC_ABY as usize] = &Opcode { // (SuBtract with Carry)
            name: "SBC_ABY",
            size: 3,
            cycles: 4,
        };
        lookup[SBC_INX as usize] = &Opcode { // (SuBtract with Carry)
            name: "SBC_INX",
            size: 2,
            cycles: 6,
        };
        lookup[SBC_INY as usize] = &Opcode { // (SuBtract with Carry)
            name: "SBC_INY",
            size: 2,
            cycles: 5,
        };
        lookup[STA_ZP as usize] = &Opcode { // (STore Accumulator)
            name: "STA_ZP",
            size: 2,
            cycles: 3,
        };
        lookup[STA_ZPX as usize] = &Opcode { // (STore Accumulator)
            name: "STA_ZPX",
            size: 2,
            cycles: 4,
        };
        lookup[STA_ABS as usize] = &Opcode { // (STore Accumulator)
            name: "STA_ABS",
            size: 3,
            cycles: 4,
        };
        lookup[STA_ABX as usize] = &Opcode { // (STore Accumulator)
            name: "STA_ABX",
            size: 3,
            cycles: 5,
        };
        lookup[STA_ABY as usize] = &Opcode { // (STore Accumulator)
            name: "STA_ABY",
            size: 3,
            cycles: 5,
        };
        lookup[STA_INX as usize] = &Opcode { // (STore Accumulator)
            name: "STA_INX",
            size: 2,
            cycles: 6,
        };
        lookup[STA_INY as usize] = &Opcode { // (STore Accumulator)
            name: "STA_INY",
            size: 2,
            cycles: 6,
        };
        lookup[TXS as usize] = &Opcode { // (Transfer X to Stack ptr)  
            name: "TXS",
            size: 1,
            cycles: 2,
        };
        lookup[TSX as usize] = &Opcode { // (Transfer Stack ptr to X)  
            name: "TSX",
            size: 1,
            cycles: 2,
        };
        lookup[PHA as usize] = &Opcode { // (PusH Accumulator)         
            name: "PHA",
            size: 1,
            cycles: 3,
        };
        lookup[PLA as usize] = &Opcode { // (PuLl Accumulator)         
            name: "PLA",
            size: 1,
            cycles: 4,
        };
        lookup[PHP as usize] = &Opcode { // (PusH Processor status)    
            name: "PHP",
            size: 1,
            cycles: 3,
        };
        lookup[PLP as usize] = &Opcode { // (PuLl Processor status)    
            name: "PLP",
            size: 1,
            cycles: 4,
        };
        lookup[STX_ZP as usize] = &Opcode { // (STore X register)
            name: "STX_ZP",
            size: 2,
            cycles: 3,
        };
        lookup[STX_ZPY as usize] = &Opcode { // (STore X register)
            name: "STX_ZPY",
            size: 2,
            cycles: 4,
        };
        lookup[STX_ABS as usize] = &Opcode { // (STore X register)
            name: "STX_ABS",
            size: 3,
            cycles: 4,
        };
        lookup[STY_ZP as usize] = &Opcode { // (STore Y register)
            name: "STY_ZP",
            size: 2,
            cycles: 3,
        };
        lookup[STY_ZPX as usize] = &Opcode { // (STore Y register)
            name: "STY_ZPX",
            size: 2,
            cycles: 4,
        };
        lookup[STY_ABS as usize] = &Opcode { // (STore Y register)
            name: "STY_ABS",
            size: 3,
            cycles: 4,
        };
        Lookup { opcodes: lookup }
    }

    pub fn name(&self, opcode: u8) -> &str {
        self.opcodes[opcode as usize].name
    }

    pub fn size(&self, opcode: u8) -> u16 {
        self.opcodes[opcode as usize].size
    }

    pub fn cycles(&self, opcode: u8) -> u8 {
        self.opcodes[opcode as usize].cycles
    }
}
