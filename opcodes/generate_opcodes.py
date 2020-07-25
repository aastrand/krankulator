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
    "ZPY"]
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
    opcodes["LAX_INY"] = ("b3", 2, 5, "LAX = LDA + LDX (Unofficial opcode)")
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
    opcodes["DCP_INY"] = ("d3", 2, 7, "DCP = DEC + CMP (Unofficial opcode)")
    # EA88  D7 48    *DCP $48,X @ 47 = EB             A:40 X:FF Y:A1 P:64 SP:FB PPU:154,211 CYC:17575
    opcodes["DCP_ZPX"] = ("d7", 2, 6, "DCP = DEC + CMP (Unofficial opcode)")
    # EAD5  DB 48 05 *DCP $0548,Y @ 0647 = EB         A:40 X:FF Y:FF P:64 SP:FB PPU:156,168 CYC:17788
    opcodes["DCP_ABY"] = ("db", 3, 6, "DCP = DEC + CMP (Unofficial opcode)")
    # EB3A  DF 48 05 *DCP $0548,X @ 0647 = EB         A:40 X:FF Y:A7 P:64 SP:FB PPU:158,263 CYC:18047
    opcodes["DCP_ABX"] = ("df", 3, 6, "DCP = DEC + CMP (Unofficial opcode)")

    # ISB
    # EB9E  E3 45    *ISB ($45,X) @ 47 = 0647 = EB    A:40 X:02 Y:AA P:64 SP:FB PPU:160,331 CYC:18297
    opcodes["ISB_INX"] = ("e3", 2, 8, "ISB = INC + SBC (Unofficial opcode)")
    # EBEE  E7 47    *ISB $47 = EB                    A:40 X:02 Y:AD P:64 SP:FB PPU:162,324 CYC:18522
    opcodes["ISB_ZP"] = ("e7", 2, 5, "ISB = INC + SBC (Unofficial opcode)")
    # EC3A  EF 47 06 *ISB $0647 = EB                  A:40 X:02 Y:B0 P:64 SP:FB PPU:164,278 CYC:18734
    opcodes["ISB_ABS"] = ("ef", 3, 6, "ISB = INC + SBC (Unofficial opcode)")
    # EC97  F3 45    *ISB ($45),Y = 0548 @ 0647 = EB  A:40 X:02 Y:FF P:64 SP:FB PPU:166,286 CYC:18964
    opcodes["ISB_INY"] = ("f3", 2, 7, "ISB = INC + SBC (Unofficial opcode)")
    # ECF8  F7 48    *ISB $48,X @ 47 = EB             A:40 X:FF Y:B6 P:64 SP:FB PPU:169, 52 CYC:19227
    opcodes["ISB_ZPX"] = ("f7", 2, 6, "ISB = INC + SBC (Unofficial opcode)")
    # ED45  FB 48 05 *ISB $0548,Y @ 0647 = EB         A:40 X:FF Y:FF P:64 SP:FB PPU:171, 15 CYC:19442
    opcodes["ISB_ABY"] = ("fb", 3, 6, "ISB = INC + SBC (Unofficial opcode)")
    # EDAA  FF 48 05 *ISB $0548,X @ 0647 = EB         A:40 X:FF Y:BC P:64 SP:FB PPU:173,116 CYC:19703
    opcodes["ISB_ABX"] = ("ff", 3, 6, "ISB = INC + SBC (Unofficial opcode)")

    # SLO
    # EE0E  03 45    *SLO ($45,X) @ 47 = 0647 = A5    A:B3 X:02 Y:BF P:E4 SP:FB PPU:175,190 CYC:19955
    opcodes["SLO_INX"] = ("03", 2, 8, "SLO = ASL + ORA (Unofficial opcode)")
    # EE5E  07 47    *SLO $47 = A5                    A:B3 X:02 Y:C2 P:E4 SP:FB PPU:177,180 CYC:20179
    opcodes["SLO_ZP"] = ("07", 2, 5, "SLO = ASL + ORA (Unofficial opcode)")
    # EEAA  0F 47 06 *SLO $0647 = A5                  A:B3 X:02 Y:C5 P:E4 SP:FB PPU:179,131 CYC:20390
    opcodes["SLO_ABS"] = ("0f", 3, 6, "SLO = ASL + ORA (Unofficial opcode)")
    # EF07  13 45    *SLO ($45),Y = 0548 @ 0647 = A5  A:B3 X:02 Y:FF P:E4 SP:FB PPU:181,136 CYC:20619
    opcodes["SLO_INY"] = ("13", 2, 7, "SLO = ASL + ORA (Unofficial opcode)")
    # EF68  17 48    *SLO $48,X @ 47 = A5             A:B3 X:FF Y:CB P:E4 SP:FB PPU:183,240 CYC:20881
    opcodes["SLO_ZPX"] = ("17", 2, 6, "SLO = ASL + ORA (Unofficial opcode)")
    # EFB5  1B 48 05 *SLO $0548,Y @ 0647 = A5         A:B3 X:FF Y:FF P:E4 SP:FB PPU:185,200 CYC:21095
    opcodes["SLO_ABY"] = ("1b", 3, 6, "SLO = ASL + ORA (Unofficial opcode)")
    # F01A  1F 48 05 *SLO $0548,X @ 0647 = A5         A:B3 X:FF Y:D1 P:E4 SP:FB PPU:187,298 CYC:21355
    opcodes["SLO_ABX"] = ("1f", 3, 6, "SLO = ASL + ORA (Unofficial opcode)")

    # RLA
    # F07E  23 45    *RLA ($45,X) @ 47 = 0647 = A5    A:B3 X:02 Y:D4 P:E4 SP:FB PPU:190, 28 CYC:21606
    opcodes["RLA_INX"] = ("23", 2, 8, "RLA = ROL + AND (Unofficial opcode)")
    # F0CE  27 47    *RLA $47 = A5                    A:B3 X:02 Y:D7 P:E4 SP:FB PPU:192, 21 CYC:21831
    opcodes["RLA_ZP"] = ("27", 2, 5, "RLA = ROL + AND (Unofficial opcode)")
    # F11A  2F 47 06 *RLA $0647 = A5                  A:B3 X:02 Y:DA P:E4 SP:FB PPU:193,316 CYC:22043
    opcodes["RLA_ABS"] = ("2f", 3, 6, "RLA = ROL + AND (Unofficial opcode)")
    # F177  33 45    *RLA ($45),Y = 0548 @ 0647 = A5  A:B3 X:02 Y:FF P:E4 SP:FB PPU:195,324 CYC:22273
    opcodes["RLA_INY"] = ("33", 2, 7, "RLA = ROL + AND (Unofficial opcode)")
    # F1D8  37 48    *RLA $48,X @ 47 = A5             A:B3 X:FF Y:E0 P:E4 SP:FB PPU:198, 90 CYC:22536
    opcodes["RLA_ZPX"] = ("37", 2, 6, "RLA = ROL + AND (Unofficial opcode)")
    # F225  3B 48 05 *RLA $0548,Y @ 0647 = A5         A:B3 X:FF Y:FF P:E4 SP:FB PPU:200, 53 CYC:22751
    opcodes["RLA_ABY"] = ("3b", 3, 6, "RLA = ROL + AND (Unofficial opcode)")
    # F28A  3F 48 05 *RLA $0548,X @ 0647 = A5         A:B3 X:FF Y:E6 P:E4 SP:FB PPU:202,154 CYC:23012
    opcodes["RLA_ABX"] = ("3f", 3, 6, "RLA = ROL + AND (Unofficial opcode)")

    # SRE
    # F2EE  43 45    *SRE ($45,X) @ 47 = 0647 = A5    A:B3 X:02 Y:E9 P:E4 SP:FB PPU:204,228 CYC:23264
    opcodes["SRE_INX"] = ("43", 2, 8, "SRE = LSR + EOR (Unofficial opcode)")
    # F33E  47 47    *SRE $47 = A5                    A:B3 X:02 Y:EC P:E4 SP:FB PPU:206,221 CYC:23489
    opcodes["SRE_ZP"] = ("47", 2, 5, "SRE = LSR + EOR (Unofficial opcode)")
    # F38A  4F 47 06 *SRE $0647 = A5                  A:B3 X:02 Y:EF P:E4 SP:FB PPU:208,172 CYC:23700
    opcodes["SRE_ABS"] = ("4f", 3, 6, "SRE = LSR + EOR (Unofficial opcode))")
    # F3E7  53 45    *SRE ($45),Y = 0548 @ 0647 = A5  A:B3 X:02 Y:FF P:E4 SP:FB PPU:210,177 CYC:23929
    opcodes["SRE_INY"] = ("53", 2, 7, "SRE = LSR + EOR (Unofficial opcode)")
    # F448  57 48    *SRE $48,X @ 47 = A5             A:B3 X:FF Y:F5 P:E4 SP:FB PPU:212,281 CYC:24191
    opcodes["SRE_ZPX"] = ("57", 2, 6, "SRE = LSR + EOR (Unofficial opcode)")
    # F495  5B 48 05 *SRE $0548,Y @ 0647 = A5         A:B3 X:FF Y:FF P:E4 SP:FB PPU:214,241 CYC:24405
    opcodes["SRE_ABY"] = ("5b", 3, 6, "SRE = LSR + EOR (Unofficial opcode)")
    # F4FA  5F 48 05 *SRE $0548,X @ 0647 = A5         A:B3 X:FF Y:FB P:E4 SP:FB PPU:216,339 CYC:24665
    opcodes["SRE_ABX"] = ("5f", 3, 6, "SRE = LSR + EOR (Unofficial opcode)")

    # RRA
    # F55E  63 45    *RRA ($45,X) @ 47 = 0647 = A5    A:B2 X:02 Y:01 P:E4 SP:FB PPU:219,102 CYC:24927
    opcodes["RRA_INX"] = ("63", 2, 8, "RRA = ROR + ADC (Unofficial opcode)")
    # F5AE  67 47    *RRA $47 = A5                    A:B2 X:02 Y:04 P:E4 SP:FB PPU:221, 80 CYC:25147
    opcodes["RRA_ZP"] = ("67", 2, 5, "RRA = ROR + ADC (Unofficial opcode)")
    # F5FA  6F 47 06 *RRA $0647 = A5                  A:B2 X:02 Y:07 P:E4 SP:FB PPU:223, 19 CYC:25354
    opcodes["RRA_ABS"] = ("6f", 3, 6, "RRA = ROR + ADC (Unofficial opcode)")
    # F657  73 45    *RRA ($45),Y = 0548 @ 0647 = A5  A:B2 X:02 Y:FF P:E4 SP:FB PPU:225, 12 CYC:25579
    opcodes["RRA_INY"] = ("73", 2, 7, "RRA = ROR + ADC (Unofficial opcode)")
    # F6B8  77 48    *RRA $48,X @ 47 = A5             A:B2 X:FF Y:0D P:E4 SP:FB PPU:227,104 CYC:25837
    opcodes["RRA_ZPX"] = ("77", 2, 6, "RRA = ROR + ADC (Unofficial opcode)")
    # F705  7B 48 05 *RRA $0548,Y @ 0647 = A5         A:B2 X:FF Y:FF P:E4 SP:FB PPU:229, 52 CYC:26047
    opcodes["RRA_ABY"] = ("7b", 3, 6, "RRA = ROR + ADC (Unofficial opcode)")
    # F76A  7F 48 05 *RRA $0548,X @ 0647 = A5         A:B2 X:FF Y:13 P:E4 SP:FB PPU:231,138 CYC:26303
    opcodes["RRA_ABX"] = ("7f", 3, 6, "RRA = ROR + ADC (Unofficial opcode)")

    # NOP with ABX, needed for page boundary penalty chek
    # C6F2  1C A9 A9 *NOP $A9A9,X @ A9A9 = A9         A:55 X:00 Y:53 P:24 SP:F1 PPU:132, 96 CYC:15036
    opcodes["N1C_ABX"] = ("1c", 3, 4, "NOP (Unofficial opcode)")
    # C6F5  3C A9 A9 *NOP $A9A9,X @ A9A9 = A9         A:55 X:00 Y:53 P:24 SP:F1 PPU:132,108 CYC:15040
    opcodes["N3C_ABX"] = ("3c", 3, 4, "NOP (Unofficial opcode)")
    # C6F8  5C A9 A9 *NOP $A9A9,X @ A9A9 = A9         A:55 X:00 Y:53 P:24 SP:F1 PPU:132,120 CYC:15044
    opcodes["N5C_ABX"] = ("5c", 3, 4, "NOP (Unofficial opcode)")
    # C6FB  7C A9 A9 *NOP $A9A9,X @ A9A9 = A9         A:55 X:00 Y:53 P:24 SP:F1 PPU:132,132 CYC:15048
    opcodes["N7C_ABX"] = ("7c", 3, 4, "NOP (Unofficial opcode)")
    # C6FE  DC A9 A9 *NOP $A9A9,X @ A9A9 = A9         A:55 X:00 Y:53 P:24 SP:F1 PPU:132,144 CYC:15052
    opcodes["NDC_ABX"] = ("dc", 3, 4, "NOP (Unofficial opcode)")
    # C701  FC A9 A9 *NOP $A9A9,X @ A9A9 = A9         A:55 X:00 Y:53 P:24 SP:F1 PPU:132,156 CYC:15056
    opcodes["NFC_ABX"] = ("fc", 3, 4, "NOP (Unofficial opcode)")

    # NOP with ZPX, needed for cycle accuracy
    # C6D2  14 A9    *NOP $A9,X @ A9 = 00             A:55 X:00 Y:53 P:24 SP:F5 PPU:131,239 CYC:14970
    opcodes["NOP_14"] = ("14", 2, 4, "NOP (Unofficial opcode)")
    # C6D4  34 A9    *NOP $A9,X @ A9 = 00             A:55 X:00 Y:53 P:24 SP:F5 PPU:131,251 CYC:14974
    opcodes["NOP_34"] = ("34", 2, 4, "NOP (Unofficial opcode)")
    # C6D6  54 A9    *NOP $A9,X @ A9 = 00             A:55 X:00 Y:53 P:24 SP:F5 PPU:131,263 CYC:14978
    opcodes["NOP_54"] = ("54", 2, 4, "NOP (Unofficial opcode)")
    # C6D8  74 A9    *NOP $A9,X @ A9 = 00             A:55 X:00 Y:53 P:24 SP:F5 PPU:131,275 CYC:14982
    opcodes["NOP_74"] = ("74", 2, 4, "NOP (Unofficial opcode)")
    # C6DA  D4 A9    *NOP $A9,X @ A9 = 00             A:55 X:00 Y:53 P:24 SP:F5 PPU:131,287 CYC:14986
    opcodes["NOP_D4"] = ("d4", 2, 4, "NOP (Unofficial opcode)")
    # C6DC  F4 A9    *NOP $A9,X @ A9 = 00             A:55 X:00 Y:53 P:24 SP:F5 PPU:131,299 CYC:14990
    opcodes["NOP_F4"] = ("f4", 2, 4, "NOP (Unofficial opcode)")

    print("// GENERATED BY generate_opcodes.py")
    print("")

    for i, mode in enumerate(sorted(ADDRESSING_MODES)):
        print("pub const ADDR_MODE_" + mode + ": usize = " + str(i) + ";")
    print("")

    for key, (opcode, size, time, comment) in opcodes.items():
        print('pub const ' + key + ': u8 = 0x' + opcode.lower() + "; " + "// " + comment)

    print("""
pub struct Opcode {
    name: &'static str,
    size: u16,
    #[allow(dead_code)]
    cycles: u8,
    mode: usize,
}

pub struct Lookup {
    opcodes: [&'static Opcode; 256],
}

static _SENTINEL: Opcode = Opcode {
    name: "NOP",
    size: 0xffff, // Inaccurate, will have to be adjusted
    cycles: 0xff, // Inaccurate, will have to be adjusted
    mode: 0xff
};

impl Lookup {
    pub fn new() -> Lookup {
        let mut lookup: [&'static Opcode; 256] = [&_SENTINEL; 256];""")

    
    for key, (opcode, size, time, comment) in opcodes.items():
        mode = "ADDR_MODE_" + key[4:] if len(key) > 3 and key[4:] in ADDRESSING_MODES else "0xff"
        print("        lookup[" + key  + " as usize] = &Opcode { " + "// " + comment)
        print("            name: \"" + key + "\",")
        print("            size: " + str(size) + ",")
        print("            cycles: " + str(time) + ",")
        print("            mode: " + mode + ",")
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
            match opcode & 0xf {
                0x0 => 2,
                0x2 => 2,
                0x4 => 3,
                0xc => 4,
                _ => 2,
            }
        } else {
            cycles
        }
    }

    pub fn mode(&self, opcode: u8) -> usize {
        self.opcodes[opcode as usize].mode
    }
}""")


if __name__ == '__main__':
    sys.exit(main())