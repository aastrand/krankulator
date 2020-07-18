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
                0x0 => 1, // #i
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