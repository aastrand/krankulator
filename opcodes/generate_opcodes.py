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

    # from nesttest.log:
    # E545  A3 40    *LAX ($40,X) @ 43 = 0580 = 55    A:00 X:03 Y:77 P:67 SP:FB PPU:134,134 CYC:15276
    opcodes["LAX_INX"] = ("a3", 2, 6, "LAX = LDA + LDX (Illegal opcode)")
    # E598  A7 67    *LAX $67 = 87                    A:00 X:AA Y:57 P:67 SP:FB PPU:135, 90 CYC:15375
    opcodes["LAX_ZP"] = ("a7", 2, 3, "LAX = LDA + LDX (Illegal opcode)")
    # E5EB  AF 77 05 *LAX $0577 = 87                  A:00 X:32 Y:57 P:67 SP:FB PPU:136, 28 CYC:15468
    opcodes["LAX_ABS"] = ("af", 3, 4, "LAX = LDA + LDX (Illegal opcode)")
    # E652  B3 43    *LAX ($43),Y = 04FF @ 0580 = 55  A:00 X:03 Y:81 P:67 SP:FB PPU:137, 38 CYC:15585
    opcodes["LAX_INY"] = ("b3", 2, 6, "LAX = LDA + LDX (Illegal opcode)")
    # E6A5  B7 10    *LAX $10,Y @ 67 = 87             A:00 X:AA Y:57 P:67 SP:FB PPU:137,332 CYC:15683
    opcodes["LAX_ZPY"] = ("b7", 2, 4, "LAX = LDA + LDX (Illegal opcode)")
    # E6F8  BF 57 05 *LAX $0557,Y @ 0587 = 87         A:00 X:32 Y:30 P:67 SP:FB PPU:138,276 CYC:15778
    opcodes["LAX_ABY"] = ("bf", 3, 4, "LAX = LDA + LDX (Illegal opcode)")

    print("// GENERATED BY generate_opcodes.py")
    print("")

    for key, (opcode, size, time, comment) in opcodes.items():
        print('pub const ' + key + ': u8 = 0x' + opcode.lower() + "; " + "// " + comment)

    print("""
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
    name: "NOP",
    size: 0xffff, // Inaccurate, will have to be adjusted
    cycles: 0xff, // Inaccurate, will have to be adjusted
};

impl Lookup {
    pub fn new() -> Lookup {
        let mut lookup: [&'static Opcode; 256] = [&_SENTINEL; 256];""")

    
    for key, (opcode, size, time, comment) in opcodes.items():
        print("        lookup[" + key  + " as usize] = &Opcode { " + "// " + comment)
        print("            name: \"" + key + "\",")
        print("            size: " + str(size) + ",")
        print("            cycles: " + str(time) + ",")
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
}""")


if __name__ == '__main__':
    sys.exit(main())