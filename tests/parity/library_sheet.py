#!/usr/bin/env python3
"""One contact sheet of the whole library, for Shawn's sitting (plan P6).

    dharmatiles build --all --system both --png
    python tests/parity/library_sheet.py [out-dir]      # -> out/sheet/library-{db,ol}.png

Each cell is a tile's painted preview (overhang red), captioned with its
name and the printability gate's numbers from its build record.
"""
import json
import pathlib
import sys

from PIL import Image, ImageDraw, ImageFont

COLS, CELL, CAP = 5, (420, 330), 44
BG = (245, 245, 242)


def font(n):
    for f in ("/System/Library/Fonts/Helvetica.ttc", "/Library/Fonts/Arial.ttf",
              "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(f, n)
        except OSError:
            pass
    return ImageFont.load_default()


def sheet(out: pathlib.Path, system: str) -> pathlib.Path | None:
    tiles = sorted((out / system).rglob(f"*-{system}-painted.png"))
    if not tiles:
        return None
    rows = (len(tiles) + COLS - 1) // COLS
    img = Image.new("RGB", (COLS * CELL[0], rows * (CELL[1] + CAP)), BG)
    dr = ImageDraw.Draw(img)
    f, g = font(15), font(12)
    for i, p in enumerate(tiles):
        x, y = (i % COLS) * CELL[0], (i // COLS) * (CELL[1] + CAP)
        im = Image.open(p).convert("RGB")
        im.thumbnail(CELL)
        img.paste(im, (x + (CELL[0] - im.width) // 2, y))
        name = p.name.removesuffix(f"-{system}-painted.png")
        rec = p.with_name(p.name.replace("-painted.png", ".build.json"))
        line = ""
        if rec.exists():
            a = json.loads(rec.read_text()).get("audit", {})
            line = (f"{a.get('triangles', 0):,} tris · overhang {a.get('overhang_mm2', 0):.2f} mm² · "
                    f"watertight {a.get('watertight')}")
        dr.text((x + 8, y + CELL[1] + 4), name, fill=(20, 20, 20), font=f)
        dr.text((x + 8, y + CELL[1] + 24), line, fill=(90, 90, 90), font=g)
    dest = out / "sheet"
    dest.mkdir(exist_ok=True)
    path = dest / f"library-{system}.png"
    img.save(path)
    return path


if __name__ == "__main__":
    out = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "out")
    for system in ("db", "ol"):
        p = sheet(out, system)
        print(p or f"no {system} previews under {out}")
