#!/usr/bin/env python3
"""Run real ROMs for a set of mappers through the mapper_smoke example and
report whether each boots and renders.

For every requested mapper this scans ROM_DIR recursively, picks up to
--per-mapper distinct games (preferring (J)/(U) dumps), runs each headless for
--frames frames, and prints one line per ROM plus a per-mapper verdict.
Screenshots are written to --out-dir as PNG (via sips) or PPM.

Usage:
    verify_mappers.py [ROM_DIR] --mappers 13,32,70,... [--per-mapper 2]
                      [--frames 900] [--out-dir /tmp/mapper-shots]

Build the example first:
    cargo build --release -p krankulator-core --example mapper_smoke
"""

import argparse
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

DEFAULT_ROM_DIR = Path.home() / "Downloads" / "All NES Roms (GoodNES)"
SMOKE_BIN = Path(__file__).resolve().parent.parent / "target/release/examples/mapper_smoke"


def parse_header(path: Path):
    try:
        with open(path, "rb") as f:
            header = f.read(16)
    except OSError:
        return None
    if len(header) < 16 or header[:4] != b"NES\x1a":
        return None
    mapper = (header[6] >> 4) | (header[7] & 0xF0)
    return mapper


def title_key(name: str) -> str:
    base = re.sub(r"\(.*?\)|\[.*?\]", "", name.lower())
    return re.sub(r"[^a-z0-9]", "", base)


def dump_rank(name: str) -> int:
    n = name.lower()
    if "[b" in n or "[o" in n or "[h" in n:
        return 3
    if "[!]" in n:
        return 0
    return 1


def find_roms(rom_dir: Path, wanted: set):
    per_mapper = defaultdict(dict)  # mapper -> title_key -> best (rank, path)
    for path in rom_dir.rglob("*.nes"):
        mapper = parse_header(path)
        if mapper not in wanted:
            continue
        key = title_key(path.stem)
        rank = dump_rank(path.name)
        cur = per_mapper[mapper].get(key)
        if cur is None or rank < cur[0]:
            per_mapper[mapper][key] = (rank, path)
    return per_mapper


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("rom_dir", nargs="?", default=str(DEFAULT_ROM_DIR))
    ap.add_argument("--mappers", required=True, help="comma-separated mapper ids")
    ap.add_argument("--per-mapper", type=int, default=2)
    ap.add_argument("--frames", type=int, default=900)
    ap.add_argument("--out-dir", default="/tmp/mapper-shots")
    args = ap.parse_args()

    if not SMOKE_BIN.exists():
        sys.exit(f"error: {SMOKE_BIN} not found — build it first")

    rom_dir = Path(args.rom_dir).expanduser()
    if not rom_dir.is_dir():
        sys.exit(f"error: ROM dir {rom_dir} not found")

    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    wanted = {int(m) for m in args.mappers.split(",")}
    per_mapper = find_roms(rom_dir, wanted)

    verdicts = {}
    for mapper in sorted(wanted):
        games = sorted(per_mapper.get(mapper, {}).values(), key=lambda t: t[1].name)
        if not games:
            verdicts[mapper] = "NO_ROMS"
            print(f"mapper {mapper:>3}: no ROMs found")
            continue
        results = []
        for _, path in games[: args.per_mapper]:
            shot = out_dir / f"m{mapper}_{title_key(path.stem)[:40]}.ppm"
            cmd = [
                str(SMOKE_BIN),
                str(path),
                "--frames",
                str(args.frames),
                "--ppm",
                str(shot),
            ]
            try:
                proc = subprocess.run(
                    cmd, capture_output=True, text=True, timeout=120
                )
                out = proc.stdout.strip()
            except subprocess.TimeoutExpired:
                out = "result=timeout"
            fields = dict(
                kv.split("=", 1) for kv in out.split() if "=" in kv
            )
            ok = (
                fields.get("result") == "ok"
                and int(fields.get("colors", 0)) >= 3
                and int(fields.get("unique_frames", 0)) >= 2
            )
            results.append(ok)
            png = shot.with_suffix(".png")
            subprocess.run(
                ["sips", "-s", "format", "png", str(shot), "--out", str(png)],
                capture_output=True,
            )
            status = "PASS" if ok else "FAIL"
            print(f"mapper {mapper:>3}: {status}  {path.name}  {out}")
        verdicts[mapper] = (
            "PASS" if all(results) else "PARTIAL" if any(results) else "FAIL"
        )

    print("\n=== Summary ===")
    for mapper in sorted(verdicts):
        print(f"mapper {mapper:>3}: {verdicts[mapper]}")

    if any(v not in ("PASS",) for v in verdicts.values()):
        sys.exit(1)


if __name__ == "__main__":
    main()
