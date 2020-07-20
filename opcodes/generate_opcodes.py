#!/usr/bin/env python3

import re
import sys


COMMENT_RE = re.compile('^([A-Z]{3}) ([a-zA-Z \(\)]+)$')
IMPLIED_OPCODE_RE = re.compile('^([A-Z]{3}) ([a-zA-Z \(\)]+)\s{1,}\$([A-Z0-9]{2})+$')
SIMPLE_OPCODE_RE = re.compile('^([a-zA-Z ,]+)\s{2,}([A-Z]{3})\s{1,}\$([A-Z0-9]{2})\s{1,}([0-9]{1})\s{1,}([0-9]{1})\+?$')
ONLY_TIME_OPCODE_RE = re.compile('^([A-Z]{3}) ([a-zA-Z \(\)]+)\s{1,}\$([A-Z0-9]{2})+\s{1,}([0-9]{1})$')
ADDR_MODE_OPCODE_RE = re.compile('^([a-zA-Z ,]+)\s{2,}([A-Z]{3}) [\#\$0-9,AXY\(\)]+\s{1,}\$([A-Z0-9]{2})\s{1,}([0-9]{1})\s{1,}([0-9]{1})\+?$')

SUFFIX_MAP = {
    'Absolute': '_ABS',    
    'Absolute,X': '_ABX',  
    'Absolute,Y': '_ABY',  
    'Accumulator': '',
    'Immediate': '_IMM',
    'Immediate': '_IMM',
    'Implied': '',
    'Indirect': '_IND',
    'Indirect,X': '_INX',  
    'Indirect,Y': '_INY',
    'Zero Page': '_ZP', 
    'Zero Page,Y': '_ZPY',
    'Zero Page,X': '_ZPX',
}

ADDRESSING_MODES = set([
    "ABS",
    "ABX",
    "ABY",
    "IMM",
    "INX",
    "INY",
    "ZP",
    "ZPX",
    "ZPY",
    "NA"]
)

def main():
    opcodes = {}

    for l in open('opcodes/opcodes.txt'):
        m = COMMENT_RE.match(l)
        if m:
            comment = m.group(2)

        m = ADDR_MODE_OPCODE_RE.match(l)
        if m:
            basename = m.group(2)
            suffix = m.group(1)
            opcode = m.group(3)
            size = m.group(4)
            time = m.group(5)
            opcodes[basename + SUFFIX_MAP[suffix.strip()]] = (opcode, size, time, comment)

        m = ONLY_TIME_OPCODE_RE.match(l)
        if m:
            basename = m.group(1)
            comment = m.group(2)
            opcode = m.group(3)
            time = m.group(4)
            opcodes[basename] = (opcode, 1, time,  comment)

        m = SIMPLE_OPCODE_RE.match(l)
        if m:
            basename = m.group(2)
            suffix = m.group(1)
            opcode = m.group(3)
            size = m.group(4)
            time = m.group(5)
            opcodes[basename + SUFFIX_MAP[suffix.strip()]] = (opcode, size, time, comment)

        m = IMPLIED_OPCODE_RE.match(l)
        if m:
            basename = m.group(1)
            comment = m.group(2)
            if "Branch" in comment:
                size = 2
            else:
                size = 1
            opcode = m.group(3)
            opcodes[basename] = (opcode, size, 2, comment)

    # Unofficial opcodes from nesttest.log:
    # http://ist.uwaterloo.ca/~schepers/MJK/ascii/65xx_ill.txt

    # LAX
    # E545  A3 40    *LAX ($40,X) @ 43 = 0580 = 55    A:00 X:03 Y:77 P:67 SP:FB PPU:134,134 CYC:15276
    opcodes["LAX_INX"] = ("a3", 2, 6, "LAX = LDA + LDX (Unofficial opcode)")
    # E598  A7 67    *LAX $67 = 87                    A:00 X:AA Y:57 P:67 SP:FB PPU:135, 90 CYC:15375
    opcodes["LAX_ZP"] = ("a7", 2, 3, "LAX = LDA + LDX (Unofficial opcode)")
    # E5EB  AF 77 05 *LAX $0577 = 87                  A:00 X:32 Y:57 P:67 SP:FB PPU:136, 28 CYC:15468
    opcodes["LAX_ABS"] = ("af", 3, 4, "LAX = LDA + LDX (Unofficial opcode)")
    # E652  B3 43    *LAX ($43),Y = 04FF @ 0580 = 55  A:00 X:03 Y:81 P:67 SP:FB PPU:137, 38 CYC:15585
    opcodes["LAX_INY"] = ("b3", 2, 6, "LAX = LDA + LDX (Unofficial opcode)")
    # E6A5  B7 10    *LAX $10,Y @ 67 = 87             A:00 X:AA Y:57 P:67 SP:FB PPU:137,332 CYC:15683
    opcodes["LAX_ZPY"] = ("b7", 2, 4, "LAX = LDA + LDX (Unofficial opcode)")
    # E6F8  BF 57 05 *LAX $0557,Y @ 0587 = 87         A:00 X:32 Y:30 P:67 SP:FB PPU:138,276 CYC:15778
    opcodes["LAX_ABY"] = ("bf", 3, 4, "LAX = LDA + LDX (Unofficial opcode)")

    # SAX
    # E757  83 49    *SAX ($49,X) @ 60 = 0489 = 00    A:3E X:17 Y:44 P:E6 SP:FB PPU:139,289 CYC:15896
    opcodes["SAX_INX"] = ("83", 2, 6, "SAX = Store A&X (Unofficial opcode)")
    # E7B6  87 49    *SAX $49 = FF                    A:55 X:AA Y:44 P:E4 SP:FB PPU:140,284 CYC:16008
    opcodes["SAX_ZP"] = ("87", 2, 3, "SAX = Store A&X (Unofficial opcode)")
    # E818  8F 49 05 *SAX $0549 = FF                  A:F5 X:AF Y:E5 P:E4 SP:FB PPU:141,273 CYC:16118
    opcodes["SAX_ABS"] = ("8f", 3, 4, "SAX = Store A&X (Unofficial opcode)")
    # E87E  97 4A    *SAX $4A,Y @ 49 = FF             A:55 X:AA Y:FF P:E4 SP:FB PPU:142,274 CYC:16232
    opcodes["SAX_ZPY"] = ("97", 2, 4, "SAX = Store A&X (Unofficial opcode)")

    # SBC
    # E8D8  EB 40    *SBC #$40                        A:40 X:EF Y:90 P:65 SP:FB PPU:143,317 CYC:16360
    opcodes["SNC_IMM"] = ("eb", 2, 2, "SNC = SBC + NOP (Unofficial opcode)")

    # DCP
    # E92E  C3 45    *DCP ($45,X) @ 47 = 0647 = EB    A:40 X:02 Y:95 P:64 SP:FB PPU:146,173 CYC:16653
    opcodes["DCP_INX"] = ("c3", 2, 8, "DCP = DEC + CMP (Unofficial opcode)")
    # E97E  C7 47    *DCP $47 = EB                    A:40 X:02 Y:98 P:64 SP:FB PPU:148,160 CYC:16876
    opcodes["DCP_ZP"] = ("c7", 2, 5, "DCP = DEC + CMP (Unofficial opcode)")
    # E9CA  CF 47 06 *DCP $0647 = EB                  A:40 X:02 Y:9B P:64 SP:FB PPU:150,108 CYC:17086
    opcodes["DCP_ABS"] = ("cf", 3, 6, "DCP = DEC + CMP (Unofficial opcode)")
    # EA27  D3 45    *DCP ($45),Y = 0548 @ 0647 = EB  A:40 X:02 Y:FF P:64 SP:FB PPU:152,110 CYC:17314
    opcodes["DCP_INY"] = ("d3", 2, 8, "DCP = DEC + CMP (Unofficial opcode)")
    # EA88  D7 48    *DCP $48,X @ 47 = EB             A:40 X:FF Y:A1 P:64 SP:FB PPU:154,211 CYC:17575
    opcodes["DCP_ZPX"] = ("d7", 2, 6, "DCP = DEC + CMP (Unofficial opcode)")
    # EAD5  DB 48 05 *DCP $0548,Y @ 0647 = EB         A:40 X:FF Y:FF P:64 SP:FB PPU:156,168 CYC:17788
    opcodes["DCP_ABY"] = ("db", 3, 7, "DCP = DEC + CMP (Unofficial opcode)")
    # EB3A  DF 48 05 *DCP $0548,X @ 0647 = EB         A:40 X:FF Y:A7 P:64 SP:FB PPU:158,263 CYC:18047
    opcodes["DCP_ABX"] = ("df", 3, 7, "DCP = DEC + CMP (Unofficial opcode)")

    # EB9E  E3 45    *ISB ($45,X) @ 47 = 0647 = EB    A:40 X:02 Y:AA P:64 SP:FB PPU:160,331 CYC:18297
    opcodes["ISB_INX"] = ("e3", 2, 8, "ISB = INC + SBC (Unofficial opcode)")
    # EBEE  E7 47    *ISB $47 = EB                    A:40 X:02 Y:AD P:64 SP:FB PPU:162,324 CYC:18522
    opcodes["ISB_ZP"] = ("e7", 2, 5, "ISB = INC + SBC (Unofficial opcode)")
    # EC3A  EF 47 06 *ISB $0647 = EB                  A:40 X:02 Y:B0 P:64 SP:FB PPU:164,278 CYC:18734
    opcodes["ISB_ABS"] = ("ef", 3, 6, "ISB = INC + SBC (Unofficial opcode)")
    # EC97  F3 45    *ISB ($45),Y = 0548 @ 0647 = EB  A:40 X:02 Y:FF P:64 SP:FB PPU:166,286 CYC:18964
    opcodes["ISB_INY"] = ("f3", 2, 8, "ISB = INC + SBC (Unofficial opcode)")
    # ECF8  F7 48    *ISB $48,X @ 47 = EB             A:40 X:FF Y:B6 P:64 SP:FB PPU:169, 52 CYC:19227
    opcodes["ISB_ZPX"] = ("f7", 2, 6, "ISB = INC + SBC (Unofficial opcode)")
    # ED45  FB 48 05 *ISB $0548,Y @ 0647 = EB         A:40 X:FF Y:FF P:64 SP:FB PPU:171, 15 CYC:19442
    opcodes["ISB_ABY"] = ("fb", 3, 7, "ISB = INC + SBC (Unofficial opcode)")
    # EDAA  FF 48 05 *ISB $0548,X @ 0647 = EB         A:40 X:FF Y:BC P:64 SP:FB PPU:173,116 CYC:19703
    opcodes["ISB_ABX"] = ("ff", 3, 7, "ISB = INC + SBC (Unofficial opcode)")

    print("// GENERATED BY generate_opcodes.py")
    print("")

    print("#[derive(Copy, Clone)]")
    print("pub enum AddressingMode {")
    for mode in ADDRESSING_MODES:
        print("    " + mode + ",")
    print("}")
    print("")

    for key, (opcode, size, time, comment) in opcodes.items():
        print('pub const ' + key + ': u8 = 0x' + opcode.lower() + "; " + "// " + comment)

    print("""
pub struct Opcode {
    name: &'static str,
    size: u16,
    #[allow(dead_code)]
    cycles: u8,
    mode: AddressingMode
}

pub struct Lookup {
    opcodes: [&'static Opcode; 256],
}

static _SENTINEL: Opcode = Opcode {
    name: "NOP",
    size: 0xffff, // Inaccurate, will have to be adjusted
    cycles: 0xff, // Inaccurate, will have to be adjusted
    mode: AddressingMode::NA
};

impl Lookup {
    pub fn new() -> Lookup {
        let mut lookup: [&'static Opcode; 256] = [&_SENTINEL; 256];""")

    
    for key, (opcode, size, time, comment) in opcodes.items():
        mode = key[4:] if len(key) > 3 and key[4:] in ADDRESSING_MODES else "NA"
        print("        lookup[" + key  + " as usize] = &Opcode { " + "// " + comment)
        print("            name: \"" + key + "\",")
        print("            size: " + str(size) + ",")
        print("            cycles: " + str(time) + ",")
        print("            mode: AddressingMode::" + mode + ",")
        print("        };")

    print("""        Lookup { opcodes: lookup }
    }

    pub fn name(&self, opcode: u8) -> &str {
        self.opcodes[opcode as usize].name
    }

    pub fn size(&self, opcode: u8) -> u16 {
        let size = self.opcodes[opcode as usize].size;
        if size == 0xffff {
            // Handle NOPs
            // https://wiki.nesdev.com/w/index.php/CPU_unofficial_opcodes
            match opcode & 0xf {
                0x0 => 2, // #i
                0x2 => 1, // #
                0x4 => 2, // d
                0xc => 3, // a
                _ => {
                    1
                }
            }
        } else {
            size
        }
    }

    pub fn cycles(&self, opcode: u8) -> u8 {
        let cycles = self.opcodes[opcode as usize].cycles;
        if cycles == 0xff {
            // Handle NOPs
            2 // TODO: make accurate
        } else {
            cycles
        }
    }

    pub fn mode(&self, opcode: u8) -> AddressingMode {
        self.opcodes[opcode as usize].mode
    }
}""")


if __name__ == '__main__':
    sys.exit(main())