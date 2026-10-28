#!/usr/bin/env python3
"""Render the current waterlab options into one timestamped contact sheet.

    waterlab/out/YYYY-MM-DD-HHMMSS-contact-sheet-s<seed>.png

Rows are ordered best → worst by Claude's current judgement (RANKING below,
edit it as opinions change); legacy is always last as the baseline.  Each
picture is labelled underneath with option + view, and each row carries a
one-line verdict, so the sheet reads anywhere when zoomed in.

Usage:
    python waterlab/contact_sheet.py [--seed 1]
"""
from __future__ import annotations

import argparse
import datetime
import pathlib
import sys
import tempfile

import numpy as np
from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import waterlab as wl  # noqa: E402

# Best first.  (option, rank note) — legacy is appended last automatically.
RANKING = [
    ('spectral-chaos', 'emitted-chaos + a wavelength BAND per emitter: land long+medium, rock short wavelets, corners long swell'),
    ('spectral-chaos@open', 'same, all-water tile: four corner emitters, long swell bands'),
    ('emitted-chaos', 'prev #1: one wavelength per emitter'),
    ('emitted-chaos@open', 'prev #2: all-water, one wavelength per corner'),
    ('rings', 'clearest rock reaction; open water only the tide'),
]
LEGACY_NOTE = 'baseline: current dharmatiles water (noise + segment ripples) + tide'

VIEWS = [('oblique', 40, -120), ('top', 89, -90), ('low', 18, -150)]
CELL = (760, 600)
CAPTION_H = 54
ROW_HEAD_H = 56


def font(size):
    for f in ('/System/Library/Fonts/Helvetica.ttc', '/Library/Fonts/Arial.ttf'):
        try:
            return ImageFont.truetype(f, size)
        except OSError:
            pass
    return ImageFont.load_default()


def render_views(mesh, tmp, tag):
    from dharmatiles.render import render
    m = mesh.copy()
    m.visual.face_colors = np.tile([150, 150, 150, 255], (len(m.faces), 1))
    top = m.slice_plane([0, 0, 0.5], [0, 0, 1], cap=False)
    out = []
    for name, elev, azim in VIEWS:
        p = tmp / f'{tag}-{name}.png'
        render([m if name == 'oblique' else top], p, elev=elev, azim=azim,
               resolution=(1200, 950), quiet=True)
        im = Image.open(p).convert('RGB')
        im.thumbnail((CELL[0] - 20, CELL[1] - 20))
        out.append(im)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--seed', type=int, default=1)
    args = ap.parse_args()
    rows = [(m, n) for m, n in RANKING if m.split('@')[0] in wl.GENERATORS and m != 'legacy']
    rows.append(('legacy', LEGACY_NOTE))
    scenes = {'': wl.Scene(args.seed), 'open': wl.Scene(args.seed, open_water=True)}
    f_big, f_cap = font(36), font(30)
    row_h = ROW_HEAD_H + CELL[1] + CAPTION_H
    W = CELL[0] * len(VIEWS)
    sheet = Image.new('RGB', (W, row_h * len(rows)), (245, 245, 245))
    dr = ImageDraw.Draw(sheet)
    with tempfile.TemporaryDirectory() as td:
        tmp = pathlib.Path(td)
        for i, (mode, note) in enumerate(rows):
            gen, _, scn = mode.partition('@')
            sc = scenes[scn]
            relief = wl.GENERATORS[gen](wl.mode_rng(args.seed, gen), sc)
            mesh, _ = wl.build(sc, relief, sc.tide)
            y = i * row_h
            rank = f'#{i + 1}' if mode != 'legacy' else 'baseline'
            dr.rectangle([0, y, W, y + ROW_HEAD_H - 6], fill=(225, 228, 235))
            dr.text((16, y + 8), f'{rank}  {mode} — {note}', fill=(25, 25, 25), font=f_big)
            for j, im in enumerate(render_views(mesh, tmp, mode)):
                x = j * CELL[0]
                sheet.paste(im, (x + (CELL[0] - im.width) // 2,
                                 y + ROW_HEAD_H + (CELL[1] - im.height) // 2))
                dr.text((x + 20, y + ROW_HEAD_H + CELL[1] + 8),
                        f'{rank} {mode} · {VIEWS[j][0]}', fill=(40, 40, 40), font=f_cap)
            print(f'  rendered {mode}')
    stamp = datetime.datetime.now().strftime('%Y-%m-%d-%H%M%S')
    out = pathlib.Path(__file__).parent / 'out' / f'{stamp}-contact-sheet-s{args.seed}.png'
    out.parent.mkdir(exist_ok=True)
    sheet.save(out)
    print(f'→ {out}')


if __name__ == '__main__':
    main()
