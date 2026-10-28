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
    ('swell-chaos', 'round 7: ~2x relief, broad round swells off the shore with sharper riders, stronger rock chop + wake'),
    ('swell-chaos@open', 'same, all-water tile'),
    ('spectral-chaos', 'prev #1: a wavelength band per emitter, ~1 mm relief'),
]
SCENARIO_SHEET = [
    ('swell-chaos@bank-rock', 'reference: bank on one side, one rock'),
    ('swell-chaos@open-rock', 'open sea, one rock'),
    ('swell-chaos@open-2rocks', 'open sea, two rocks'),
    ('swell-chaos@shore-shapes', 'shoreline + cube + pyramid'),
    ('swell-chaos@land-2', 'land on two (opposite) sides + rock'),
    ('swell-chaos@land-3', 'land on three sides (bay) + rock'),
    ('swell-chaos@land-4', 'land on all four sides (pond) + rock'),
    ('swell-chaos@open', 'open sea, nothing'),
]
LEGACY_NOTE = 'baseline: current dharmatiles water (noise + segment ripples) + tide'

SHEETS = None  # filled below
VIEWS = [('painted', 40, -120), ('oblique', 40, -120), ('top', 89, -90), ('low', 18, -150)]
CELL = (760, 600)
CAPTION_H = 54
ROW_HEAD_H = 56


SHEETS = {'ranking': RANKING, 'scenarios': SCENARIO_SHEET}


def font(size):
    for f in ('/System/Library/Fonts/Helvetica.ttc', '/Library/Fonts/Arial.ttf'):
        try:
            return ImageFont.truetype(f, size)
        except OSError:
            pass
    return ImageFont.load_default()


# Simulated cheap-acrylic paint job (dark basecoat, drybrushed crests).
PAINT = dict(trough=(18, 38, 92), mid=(28, 86, 156), upper=(58, 160, 178),
             crest=(150, 220, 214), tip=(225, 245, 240),
             land=(118, 110, 98), rock=(120, 124, 132), side=(42, 42, 46))


def paint_colors(mesh, sc, z):
    """Per-vertex paint: water coloured by local relief (z − blur(z, 2 mm)) —
    what a drybrush catches — plus a little absolute height; land, rock and
    tile sides flat colours."""
    from scipy import ndimage
    v = mesh.vertices
    cell = wl.CELL_MM
    rel = z - ndimage.gaussian_filter(z, 2.0 / cell)
    hi = z - ndimage.gaussian_filter(z, 6.0 / cell)
    ix = np.clip(np.round(v[:, 0] / cell).astype(int), 0, z.shape[1] - 1)
    iy = np.clip(np.round(v[:, 1] / cell).astype(int), 0, z.shape[0] - 1)
    zs = z[iy, ix]
    t = 0.65 * rel[iy, ix] / 0.5 + 0.35 * hi[iy, ix] / 0.9       # ~[-1, 1]
    t = np.clip(0.5 + 0.5 * t, 0, 1)
    stops = np.array([0.0, 0.45, 0.72, 0.9, 1.04])
    cols = np.array([PAINT[k] for k in ('trough', 'mid', 'upper', 'crest', 'tip')], float)
    water = np.stack([np.interp(t, stops, cols[:, c]) for c in range(3)], 1)
    out = water
    onland = sc.land[iy, ix] >= zs - 0.02
    out = np.where(onland[:, None], np.array(PAINT['land'], float), out)
    if sc.rock is not None:
        above = v[:, 2] > zs + 0.08
        out = np.where(above[:, None], np.array(PAINT['rock'], float), out)
    side = (v[:, 2] < zs - 0.3) | (v[:, 2] < 0.2)
    out = np.where(side[:, None], np.array(PAINT['side'], float), out)
    return np.concatenate([out, np.full((len(out), 1), 255.0)], 1).astype(np.uint8)


def render_painted(mesh, sc, z, path, elev, azim, res=(1200, 950)):
    """Painted preview through the standard dharmatiles renderer: per-face
    paint quantised to the palette (one sub-mesh per colour, as a real
    drybrush job is banded anyway)."""
    import trimesh
    from dharmatiles.render import render
    m = trimesh.Trimesh(mesh.vertices, mesh.faces, process=False)
    vc = paint_colors(m, sc, z).astype(float)
    fc = vc[m.faces].mean(1)
    pal = np.array([PAINT[k] for k in PAINT], float)
    k = ((fc[:, None, :3] - pal[None]) ** 2).sum(2).argmin(1)
    side = np.abs(m.face_normals[:, 2]) < 0.35          # tile walls / base
    k = np.where(side, list(PAINT).index('side'), k)
    cols = np.concatenate([pal[k], np.full((len(k), 1), 255.0)], 1).astype(np.uint8)
    m.visual.face_colors = cols
    render([m], path, elev=elev, azim=azim, resolution=res, quiet=True, smooth=False)


def render_views(mesh, tmp, tag, sc=None, z=None):
    from dharmatiles.render import render
    m = mesh.copy()
    m.visual.face_colors = np.tile([150, 150, 150, 255], (len(m.faces), 1))
    top = m.slice_plane([0, 0, 0.5], [0, 0, 1], cap=False)
    out = []
    for name, elev, azim in VIEWS:
        p = tmp / f'{tag}-{name}.png'
        if name == 'painted':
            render_painted(mesh, sc, z, p, elev, azim)
        else:
            render([m if name == 'oblique' else top], p, elev=elev, azim=azim,
                   resolution=(1200, 950), quiet=True)
        im = Image.open(p).convert('RGB')
        im.thumbnail((CELL[0] - 20, CELL[1] - 20))
        out.append(im)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--sheet', default='ranking', choices=list(SHEETS))
    ap.add_argument('--no-legacy', action='store_true')
    args = ap.parse_args()
    rows = [(m, n) for m, n in SHEETS[args.sheet] if m.split('@')[0] in wl.GENERATORS and m != 'legacy']
    if args.sheet != 'ranking':
        args.no_legacy = True
    if not args.no_legacy:
        rows.append(('legacy', LEGACY_NOTE))
    scenes = {}
    f_big, f_cap = font(36), font(30)
    row_h = ROW_HEAD_H + CELL[1] + CAPTION_H
    W = CELL[0] * len(VIEWS)
    sheet = Image.new('RGB', (W, row_h * len(rows)), (245, 245, 245))
    dr = ImageDraw.Draw(sheet)
    with tempfile.TemporaryDirectory() as td:
        tmp = pathlib.Path(td)
        for i, (mode, note) in enumerate(rows):
            gen, _, scn = mode.partition('@')
            scn = scn or 'bank-rock'
            if scn not in scenes:
                scenes[scn] = wl.Scene(args.seed, scenario=scn)
            sc = scenes[scn]
            relief = wl.GENERATORS[gen](wl.mode_rng(args.seed, gen), sc)
            mesh, z = wl.build(sc, relief, sc.tide)
            y = i * row_h
            rank = f'#{i + 1}' if mode != 'legacy' else 'baseline'
            dr.rectangle([0, y, W, y + ROW_HEAD_H - 6], fill=(225, 228, 235))
            dr.text((16, y + 8), f'{rank}  {mode} — {note}', fill=(25, 25, 25), font=f_big)
            for j, im in enumerate(render_views(mesh, tmp, mode.replace('@', '_'), sc, z)):
                x = j * CELL[0]
                sheet.paste(im, (x + (CELL[0] - im.width) // 2,
                                 y + ROW_HEAD_H + (CELL[1] - im.height) // 2))
                dr.text((x + 20, y + ROW_HEAD_H + CELL[1] + 8),
                        f'{rank} {mode} · {VIEWS[j][0]}', fill=(40, 40, 40), font=f_cap)
            print(f'  rendered {mode}')
    stamp = datetime.datetime.now().strftime('%Y-%m-%d-%H%M%S')
    tag = '' if args.sheet == 'ranking' else f'-{args.sheet}'
    out = pathlib.Path(__file__).parent / 'out' / f'{stamp}-contact-sheet{tag}-s{args.seed}.png'
    out.parent.mkdir(exist_ok=True)
    sheet.save(out)
    print(f'→ {out}')


if __name__ == '__main__':
    main()
