#!/usr/bin/env python3
"""Build the embedded CJK bitmap subset from a Fusion Pixel 12px BDF.

Usage:
    python3 scripts/build-cjk-subset.py /path/to/fusion-pixel-12px-monospaced-zh_hans.bdf

The card only draws a small set of Chinese labels. This script collects every
non-ASCII character used in `src/`, reads those glyphs from the OFL-licensed
Fusion Pixel 12px monospaced font, scales them 2x (24x24 pixels) and writes
`assets/fusion-pixel-12px-subset.json`. The font itself is not committed; the
license text and provenance are.
"""

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ASSET = ROOT / "assets" / "fusion-pixel-12px-subset.json"
FONT_VERSION = "2026.10.25"


def wanted(ch: str) -> bool:
    cp = ord(ch)
    return (
        0x2E80 <= cp <= 0x303E  # CJK radicals and punctuation
        or 0x3041 <= cp <= 0x33FF
        or 0x3400 <= cp <= 0x4DBF
        or 0x4E00 <= cp <= 0x9FFF
        or 0xA000 <= cp <= 0xA4CF
        or 0xAC00 <= cp <= 0xD7A3
        or 0xF900 <= cp <= 0xFAFF
        or 0xFE10 <= cp <= 0xFE19
        or 0xFE30 <= cp <= 0xFE6F
        or 0xFF00 <= cp <= 0xFF60
        or 0xFFE0 <= cp <= 0xFFE6
    )


def read_bdf(path: Path):
    ascent = 0
    glyphs = {}
    enc = None
    bbx = None
    bitmap = []
    in_bitmap = False
    for raw in path.read_text(errors="replace").splitlines():
        line = raw.strip()
        if line.startswith("FONT_ASCENT"):
            ascent = int(line.split()[1])
        elif line == "STARTCHAR" or line.startswith("STARTCHAR "):
            enc, bbx, bitmap, in_bitmap = None, None, [], False
        elif line.startswith("ENCODING "):
            enc = int(line.split()[1])
        elif line.startswith("BBX "):
            bbx = tuple(int(v) for v in line.split()[1:5])
        elif line == "BITMAP":
            in_bitmap = True
        elif line == "ENDCHAR":
            if enc is not None and enc >= 0 and bbx is not None:
                glyphs[chr(enc)] = (bbx, bitmap)
            in_bitmap = False
        elif in_bitmap:
            bitmap.append(line)
    if ascent == 0:
        raise SystemExit(f"{path}: FONT_ASCENT missing")
    return ascent, glyphs


def mask(glyph, ascent: int):
    (width, height, xoff, yoff), bitmap = glyph
    design = [[0] * 12 for _ in range(12)]
    top = ascent - (yoff + height)
    for sy, row in enumerate(bitmap[:height]):
        bits = len(row) * 4
        value = int(row, 16) if row else 0
        for sx in range(width):
            x, y = xoff + sx, top + sy
            if 0 <= x < 12 and 0 <= y < 12 and value & (1 << (bits - 1 - sx)):
                design[y][x] = 1
    out = []
    for row in design:
        wide = []
        for px in row:
            wide.extend((px, px))
        out.append(wide)
    scaled = []
    for row in out:
        for _ in range(2):
            value = 0
            for x, px in enumerate(row):
                if px:
                    value |= 1 << (23 - x)
            scaled.append(value)
    return scaled


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    ascent, glyphs = read_bdf(Path(sys.argv[1]))
    chars = set()
    for source in sorted((ROOT / "src").glob("*.rs")):
        chars.update(ch for ch in source.read_text() if wanted(ch))
    missing = sorted(ch for ch in chars if ch not in glyphs)
    if missing:
        raise SystemExit("font has no glyph for: " + " ".join(missing))
    out = {}
    for ch in sorted(chars):
        rows = mask(glyphs[ch], ascent)
        out[ch] = "".join(f"{value:06x}" for value in rows)
    data = {
        "font": "Fusion Pixel 12px Mono zh_Hans",
        "version": FONT_VERSION,
        "license": "SIL Open Font License 1.1",
        "glyph_width": 24,
        "glyph_height": 24,
        "scale": 2,
        "glyphs": out,
    }
    ASSET.write_text(json.dumps(data, ensure_ascii=False, separators=(",", ":")) + "\n")
    print(f"{ASSET.relative_to(ROOT)}: {len(out)} glyphs")


if __name__ == "__main__":
    main()
