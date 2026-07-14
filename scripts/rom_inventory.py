#!/usr/bin/env python3
"""Inventory a GoodNES ROM collection by iNES mapper and compare against
the mappers implemented in krankulator's loader.

Duplicates (alt dumps, bad dumps, overdumps, trainers, translations) are
collapsed by stripping GoodNES tags from the filename. Regional releases of
the same title are counted separately only when they use different mappers,
since those are genuinely different boards.

Usage: rom_inventory.py [ROM_DIR] [--loader PATH] [--top N] [--full]
"""

import argparse
import re
import sys
from collections import defaultdict
from pathlib import Path

TAG_RE = re.compile(r"\s*[\[(][^\])]*[\])]")

# name, t-shirt effort. S = trivial discrete latch (SimpleMapper territory),
# M = banking plus IRQ/quirks, L = complex IRQ/timing or many variants,
# XL = needs a new subsystem (FM synth, protection emulation).
MAPPER_INFO = {
    0: ("NROM", "-"),
    1: ("MMC1", "-"),
    2: ("UxROM", "-"),
    3: ("CNROM", "-"),
    4: ("MMC3", "-"),
    5: ("MMC5", "-"),
    6: ("FFE F4", "M"),
    7: ("AxROM", "-"),
    8: ("FFE F3", "M"),
    9: ("MMC2", "-"),
    10: ("MMC4", "-"),
    11: ("Color Dreams", "-"),
    13: ("CPROM (Videomation)", "S"),
    15: ("K-1029 multicart", "M"),
    16: ("Bandai FCG", "-"),
    18: ("Jaleco SS88006", "-"),
    19: ("Namco 163", "-"),
    21: ("VRC4a/c", "-"),
    22: ("VRC2a", "-"),
    23: ("VRC2b/4e-f", "-"),
    24: ("VRC6a", "-"),
    25: ("VRC2c/4b/4d", "-"),
    26: ("VRC6b", "-"),
    28: ("Action 53", "-"),
    30: ("UNROM 512", "-"),
    31: ("NSF multicart", "-"),
    32: ("Irem G-101", "S"),
    33: ("Taito TC0190", "-"),
    34: ("BNROM/NINA-001", "-"),
    40: ("NTDEC 2722 (SMB2j)", "M"),
    41: ("Caltron 6-in-1", "M"),
    42: ("FDS conversion", "M"),
    43: ("TONY-I/YS-612 (SMB2j)", "M"),
    44: ("MMC3 multicart (Super Big 7-in-1)", "M"),
    45: ("MMC3 multicart (GA23C)", "M"),
    46: ("Rumble Station", "M"),
    47: ("MMC3 multicart (QJ)", "M"),
    48: ("Taito TC0690", "-"),
    49: ("MMC3 multicart (1993 Super HIK)", "M"),
    50: ("N-32 (SMB2j)", "M"),
    51: ("11-in-1 Ball Games", "M"),
    52: ("MMC3 multicart (Mario 7-in-1)", "M"),
    57: ("GK multicart", "M"),
    58: ("68-in-1 multicart", "M"),
    60: ("Reset-based 4-in-1", "M"),
    61: ("20-in-1 multicart", "M"),
    62: ("Super 700-in-1", "M"),
    64: ("Tengen RAMBO-1", "L"),
    65: ("Irem H3001", "M"),
    66: ("GxROM", "-"),
    67: ("Sunsoft-3", "M"),
    68: ("Sunsoft-4", "-"),
    69: ("Sunsoft FME-7", "-"),
    70: ("Bandai 74*161 (Family Trainer)", "S"),
    71: ("Camerica", "-"),
    72: ("Jaleco JF-17", "S"),
    73: ("VRC3", "-"),
    74: ("MMC3 pirate (CHR RAM mix)", "M"),
    75: ("VRC1", "-"),
    76: ("NAMCOT-3446", "S"),
    77: ("Irem 74*161", "S"),
    78: ("Holy Diver / Cosmo Carrier", "-"),
    79: ("NINA-03/06 (AVE)", "S"),
    80: ("Taito X1-005", "M"),
    82: ("Taito X1-017", "M"),
    83: ("Cony/Yoko", "L"),
    85: ("VRC7 (FM audio)", "XL"),
    86: ("Jaleco JF-13", "S"),
    87: ("Jaleco/Konami 74*139", "-"),
    88: ("Namco 118 variant", "-"),
    89: ("Sunsoft-2 (Sunsoft-3 board)", "S"),
    90: ("J.Y. Company", "XL"),
    91: ("J.Y. subset (Street Fighter 3)", "M"),
    92: ("Jaleco JF-19", "S"),
    93: ("Sunsoft-2 (Sunsoft-1 board)", "S"),
    94: ("UN1ROM (Senjou no Ookami)", "S"),
    95: ("NAMCOT-3425", "S"),
    96: ("Bandai Oeka Kids", "M"),
    97: ("Irem TAM-S1 (Kaiketsu Yanchamaru)", "S"),
    99: ("VS System", "M"),
    105: ("NES-EVENT", "-"),
    107: ("Magic Dragon", "S"),
    112: ("NTDEC/Asder", "M"),
    113: ("NINA-03/06 multicart", "S"),
    114: ("MMC3 pirate (Lion King)", "L"),
    115: ("MMC3 pirate (Kart Fighter)", "L"),
    116: ("SOMARI-P", "L"),
    117: ("Future Media", "M"),
    118: ("TxSROM", "-"),
    119: ("TQROM", "-"),
    132: ("TXC 05-00002-010", "M"),
    133: ("Sachen 3009", "S"),
    136: ("Sachen 3011", "M"),
    137: ("Sachen 8259D", "M"),
    138: ("Sachen 8259B", "M"),
    139: ("Sachen 8259C", "M"),
    140: ("Jaleco JF-11/14", "-"),
    141: ("Sachen 8259A", "M"),
    142: ("Kaiser KS202 (SMB2j)", "M"),
    143: ("Sachen NROM w/ protection", "S"),
    145: ("Sachen SA-72007", "S"),
    146: ("Sachen NINA-03 clone", "S"),
    147: ("Sachen 3018", "M"),
    148: ("Sachen SA-0037", "S"),
    149: ("Sachen SA-0036", "S"),
    150: ("Sachen 74LS374N", "M"),
    151: ("VS VRC1", "S"),
    152: ("Bandai 74*161 one-screen", "-"),
    153: ("Bandai LZ93D50 + fixed bank", "M"),
    154: ("NAMCOT-3453", "S"),
    155: ("MMC1A", "S"),
    156: ("DIS23C01 (Daou)", "M"),
    157: ("Bandai Datach", "M"),
    158: ("Tengen 800037", "L"),
    159: ("Bandai LZ93D50 + 24C01", "S"),
    160: ("Sachen/pirate", "L"),
    162: ("Waixing FS304", "L"),
    163: ("Nanjing", "L"),
    164: ("Dongda/Waixing", "L"),
    166: ("Subor (variant A)", "M"),
    167: ("Subor (variant B)", "M"),
    178: ("Waixing education", "M"),
    180: ("Crazy Climber", "-"),
    182: ("MMC3 pirate (Super Donkey Kong)", "M"),
    184: ("Sunsoft-1", "-"),
    185: ("CNROM w/ CHR disable", "-"),
    189: ("TXC MMC3 variant", "M"),
    192: ("Waixing MMC3 variant", "M"),
    193: ("NTDEC TC-112 (Fighting Hero)", "M"),
    194: ("Waixing MMC3 variant", "M"),
    195: ("Waixing MMC3 variant", "M"),
    198: ("Waixing MMC3 variant", "M"),
    199: ("Waixing MMC3 variant", "M"),
    200: ("36-in-1 multicart", "M"),
    201: ("NROM-256 multicart", "M"),
    202: ("150-in-1 multicart", "M"),
    203: ("35-in-1 multicart", "M"),
    204: ("64-in-1 multicart", "M"),
    205: ("MMC3 multicart (15-in-1)", "M"),
    206: ("Namco 108/DxROM", "-"),
    207: ("Taito X1-005 (alt mirroring)", "M"),
    209: ("J.Y. Company (nametable ctrl)", "XL"),
    210: ("Namco 175/340", "-"),
    211: ("J.Y. Company (variant)", "XL"),
    212: ("Super HIK 300-in-1", "M"),
    213: ("9999999-in-1", "M"),
    214: ("Super Gun 20-in-1", "M"),
    216: ("Bonza / Magic Jewelry 2", "M"),
    222: ("Dragon Ninja pirate", "M"),
    225: ("ET-4310 multicart", "M"),
    226: ("76-in-1 multicart", "M"),
    227: ("1200-in-1 multicart", "M"),
    228: ("Action 52 / Cheetahmen II", "M"),
    229: ("31-in-1 multicart", "S"),
    230: ("22-in-1 multicart", "M"),
    231: ("20-in-1 multicart", "M"),
    232: ("Camerica Quattro", "S"),
    233: ("Super 42-in-1", "M"),
    234: ("Maxi 15", "M"),
    235: ("Golden Game 150-in-1", "M"),
    240: ("C&E (Sheng Huo Lie Zhuan)", "S"),
    241: ("BNROM-like (Fan Kong Jing Ying)", "S"),
    242: ("Wai Xing Zhan Shi", "S"),
    243: ("Sachen 74LS374N variant", "M"),
    245: ("Waixing MMC3 variant", "M"),
    246: ("Feng Shen Bang", "M"),
    255: ("110-in-1 multicart", "M"),
}

EFFORT_WEIGHT = {"S": 1, "M": 2, "L": 4, "XL": 8}
EFFORT_LABEL = {
    "S": "S (hours; SimpleMapper-style latch)",
    "M": "M (~a day; banking + IRQ/quirks)",
    "L": "L (days; complex timing/variants)",
    "XL": "XL (week+; new subsystem)",
}


def base_title(stem: str) -> str:
    return TAG_RE.sub("", stem).strip().lower()


def parse_mapper(header: bytes):
    if len(header) < 16 or header[:4] != b"NES\x1a":
        return None
    mapper = (header[7] & 0xF0) | (header[6] >> 4)
    if (header[7] & 0x0C) == 0x08:
        mapper |= (header[8] & 0x0F) << 8
    return mapper


def implemented_mappers(loader_path: Path) -> set:
    arms = set()
    in_match = False
    for line in loader_path.read_text().splitlines():
        if "match mapper_id" in line:
            in_match = True
            continue
        if not in_match:
            continue
        if re.match(r"\s+_\s*=>", line):
            break
        m = re.match(r"\s{8}(\d+(?:\s*\|\s*\d+)*)\s*=>", line)
        if m:
            arms.update(int(n) for n in re.findall(r"\d+", m.group(1)))
    if not arms:
        sys.exit(f"error: no mapper match arms found in {loader_path}")
    return arms


def main():
    default_loader = Path(__file__).resolve().parent.parent / "core/src/emu/io/loader.rs"
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument(
        "rom_dir",
        nargs="?",
        default=str(Path.home() / "Downloads/All NES Roms (GoodNES)"),
    )
    ap.add_argument("--loader", default=str(default_loader))
    ap.add_argument("--top", type=int, default=25, help="missing mappers to show")
    ap.add_argument("--full", action="store_true", help="also list implemented mapper counts")
    args = ap.parse_args()

    rom_dir = Path(args.rom_dir).expanduser()
    if not rom_dir.is_dir():
        sys.exit(f"error: {rom_dir} is not a directory")
    implemented = implemented_mappers(Path(args.loader).expanduser())

    # (title, mapper) -> set of filenames; collapses alt/bad dumps and
    # same-mapper regional releases, keeps cross-region board differences
    games = defaultdict(set)
    unreadable = []
    total_files = 0
    for path in sorted(rom_dir.rglob("*.nes")):
        total_files += 1
        try:
            with open(path, "rb") as f:
                mapper = parse_mapper(f.read(16))
        except OSError:
            mapper = None
        if mapper is None:
            unreadable.append(path.name)
            continue
        games[(base_title(path.stem), mapper)].add(path.name)

    per_mapper = defaultdict(set)
    for (title, mapper), _ in games.items():
        per_mapper[mapper].add(title)

    total_games = sum(len(t) for t in per_mapper.values())
    covered = sum(len(t) for m, t in per_mapper.items() if m in implemented)

    print(f"Scanned {total_files} .nes files in {rom_dir}")
    print(f"Unique games (title+mapper): {total_games}"
          f"   Bad/missing iNES header: {len(unreadable)}")
    print(f"Implemented mappers in loader: {len(implemented)}")
    print(f"Coverage: {covered}/{total_games} games ({100 * covered / total_games:.1f}%)\n")

    missing = []
    for mapper, titles in per_mapper.items():
        if mapper in implemented:
            continue
        name, effort = MAPPER_INFO.get(mapper, (f"(unknown mapper {mapper})", "M"))
        weight = EFFORT_WEIGHT[effort]
        missing.append((len(titles) / weight, len(titles), mapper, name, effort, titles))
    missing.sort(key=lambda x: (-x[0], -x[1], x[2]))

    print(f"Missing mappers by bang-for-buck (games / effort weight, "
          f"S=1 M=2 L=4 XL=8), top {args.top}:\n")
    print(f"{'rank':>4}  {'mapper':>6}  {'games':>5}  {'effort':>6}  "
          f"{'score':>6}  name / examples")
    for rank, (score, count, mapper, name, effort, titles) in enumerate(
        missing[: args.top], 1
    ):
        examples = ", ".join(sorted(titles)[:3])
        print(f"{rank:>4}  {mapper:>6}  {count:>5}  {effort:>6}  "
              f"{score:>6.1f}  {name}")
        print(f"{'':>36}e.g. {examples}")

    remaining = missing[args.top:]
    if remaining:
        rem_games = sum(m[1] for m in remaining)
        print(f"\n... plus {len(remaining)} more missing mappers "
              f"covering {rem_games} games (use --top to see more)")

    print("\nEffort key:")
    for k in ("S", "M", "L", "XL"):
        print(f"  {EFFORT_LABEL[k]}")

    if args.full:
        print("\nImplemented mapper counts:")
        for mapper in sorted(per_mapper):
            if mapper in implemented:
                name = MAPPER_INFO.get(mapper, ("?",))[0]
                print(f"  {mapper:>4}  {len(per_mapper[mapper]):>5}  {name}")


if __name__ == "__main__":
    main()
