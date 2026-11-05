"""Make the document elements' pictures from the demo tiles beside this file.

    python3 docs/elements/demo/pictures.py      # from the checkout, after cargo build --release

Builds each demo with `dharmatiles build --png` into a scratch folder and
writes JPEGs to docs/elements/img/:

    el-line-straight.jpg, el-line.jpg   line.dtile from above, wobble 0 and 5,
                                        its three points marked in brass
    el-region.jpg                       region.dtile
    el-stack-1.jpg … el-stack-6.jpg     stack.dtile with its first 1…6 lines
    el-use.jpg, el-define.jpg           use.dtile, define.dtile

Labels are not drawn into the pictures: the pages set them in the site's type.
Needs Pillow.
"""

from __future__ import annotations

import re
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
IMG = HERE.parent / "img"
BIN = ROOT / "target" / "release" / "dharmatiles"
PLATE = (217, 224, 235)  # the renderer's background, the site's --plate
BRASS = (168, 114, 15)


def build(doc: Path, out: Path) -> Path:
    """Build `doc` with PNG views; return the folder holding them."""
    subprocess.run([str(BIN), "build", str(doc), "--png", "--out", str(out)], check=True,
                   stdout=subprocess.DEVNULL)
    pngs = list(out.rglob(f"{doc.stem}-db-painted.png"))
    if not pngs:
        sys.exit(f"no render for {doc}")
    return pngs[0].parent


def save(im: Image.Image, name: str, width: int = 900) -> None:
    if im.width > width:
        im = im.resize((width, round(im.height * width / im.width)), Image.LANCZOS)
    im.convert("RGB").save(IMG / name, quality=86, optimize=True, progressive=True)
    print("wrote", IMG / name)


def footprint(im: Image.Image) -> tuple[int, int, int, int]:
    """The tile's box in a top view: everything that is not the plate."""
    px = im.convert("RGB").load()
    w, h = im.size
    xs, ys = [], []
    for y in range(0, h, 2):
        for x in range(0, w, 2):
            r, g, b = px[x, y]
            if abs(r - PLATE[0]) + abs(g - PLATE[1]) + abs(b - PLATE[2]) > 24:
                xs.append(x)
                ys.append(y)
    return min(xs), min(ys), max(xs), max(ys)


def mark_points(im: Image.Image, points: list[tuple[float, float]]) -> Image.Image:
    """Draw the line's points (in squares, y up) and the straight path through them."""
    im = im.convert("RGB")
    x0, y0, x1, y1 = footprint(im)
    to_px = [(x0 + u * (x1 - x0), y1 - v * (y1 - y0)) for u, v in points]
    d = ImageDraw.Draw(im)
    for a, b in zip(to_px, to_px[1:]):  # the straight path, dashed
        n = 22
        for i in range(0, n, 2):
            p = (a[0] + (b[0] - a[0]) * i / n, a[1] + (b[1] - a[1]) * i / n)
            q = (a[0] + (b[0] - a[0]) * (i + 1) / n, a[1] + (b[1] - a[1]) * (i + 1) / n)
            d.line([p, q], fill=(255, 255, 255), width=5)
            d.line([p, q], fill=BRASS, width=3)
    for x, y in to_px:
        d.ellipse([x - 11, y - 11, x + 11, y + 11], fill=(255, 255, 255))
        d.ellipse([x - 8, y - 8, x + 8, y + 8], fill=BRASS)
    return im


def stack_prefix(text: str, k: int) -> str:
    """`text` with only the first `k` lines of its stack."""
    head, rest = text.split("stack {", 1)
    body, tail = rest.split("\n    }", 1)
    lines = [ln for ln in body.splitlines() if ln.strip()]
    return head + "stack {\n" + "\n".join(lines[:k]) + "\n    }" + tail


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)

        # line: the same three points, straight and wobbled
        line = (HERE / "line.dtile").read_text()
        points = [tuple(map(float, p.split(","))) for p in re.findall(r'"([\d.]+,[\d.]+)"', line.split("through", 1)[1].split("}", 1)[0])]
        straight = tmp / "line-straight.dtile"
        straight.write_text(line.replace("wobble=5", "wobble=0"))
        for doc, name in ((straight, "el-line-straight.jpg"), (HERE / "line.dtile", "el-line.jpg")):
            top = Image.open(build(doc, tmp / doc.stem) / f"{doc.stem}-db-top.png")
            save(mark_points(top, points), name, 640)

        for stem in ("region", "use", "define"):
            doc = HERE / f"{stem}.dtile"
            save(Image.open(build(doc, tmp / stem) / f"{stem}-db-painted.png"), f"el-{stem}.jpg")

        # stack: one line at a time
        text = (HERE / "stack.dtile").read_text()
        for k in range(1, 7):
            doc = tmp / f"stack-{k}.dtile"
            doc.write_text(stack_prefix(text, k))
            save(Image.open(build(doc, tmp / f"s{k}") / f"stack-{k}-db-painted.png"), f"el-stack-{k}.jpg", 520)

if __name__ == "__main__":
    main()
