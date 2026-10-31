#!/usr/bin/env python3
"""One-object iteration sheet: build one rockmass tile, paint it, render a
few views next to a reference crop, open it.  Fast (≈ 15 s).

Usage:
    python clifflab/iterate.py [--form run] [--seed 1] [--cols 1] [--ref B] [--no-open]
"""
from __future__ import annotations

import argparse
import datetime
import pathlib
import subprocess
import sys

import numpy as np
import trimesh
from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'clifflab'))
import rockmass as rm  # noqa: E402
import rockpile as rp  # noqa: E402

PAL = np.array([(20, 19, 20), (38, 37, 38), (58, 56, 55), (84, 78, 68), (118, 104, 82),
                (168, 158, 140), (68, 52, 38), (102, 80, 58), (146, 124, 96), (40, 40, 42)], float)
REF_CELLS = {c: (i % 4, i // 4) for i, c in enumerate('ABCDEFGHIJKLMNOP')}
VIEWS = [('oblique', 24, -118), ('front', 6, -90), ('raking', 14, -40), ('end', 6, 180)]


def font(n):
    from PIL import ImageFont
    for f in ('/System/Library/Fonts/Helvetica.ttc', '/Library/Fonts/Arial.ttf'):
        try:
            return ImageFont.truetype(f, n)
        except OSError:
            pass
    return ImageFont.load_default()


def paint(mesh):
    """Drybrush-style paint: dark wash in recesses, light on convex edges and
    up-faces; dirt on up-facing ground and caps; slab sides dark."""
    v, f = trimesh.remesh.subdivide_to_size(mesh.vertices, mesh.faces, max_edge=0.6)
    m = trimesh.Trimesh(v, f, process=False)
    n = m.vertex_normals
    L = trimesh.smoothing.laplacian_calculation(m, equal_weight=True)
    s6, s40 = v.copy(), v.copy()
    for k in range(40):
        s40 = L @ s40
        if k == 5:
            s6 = s40.copy()
    cs = -np.einsum('ij,ij->i', s6 - v, n)
    cl = -np.einsum('ij,ij->i', s40 - v, n)
    t = 0.36 + 0.22 * cs / (cs.std() + 1e-9) + 0.22 * cl / (cl.std() + 1e-9) + 0.1 * np.clip(n[:, 2], 0, 1)
    for _ in range(2):
        t = L @ t
    k = np.digitize(t, [0.18, 0.36, 0.56, 0.74, 0.9])
    zmax = v[:, 2].max()
    dirt = (((n[:, 2] > 0.55) & ((v[:, 2] < rm.FOOT_Z_MM + 1.6) | (v[:, 2] > zmax - 3.0)))
            | (n[:, 2] > 0.88))                      # ledges and up-facing tops: dusty
    kd = 6 + np.digitize(0.5 + 0.3 * cs / (cs.std() + 1e-9), [0.3, 0.8])
    k = np.where(dirt, kd, k)
    k = np.where(v[:, 2] < rm.FOOT_Z_MM - 0.8, 9, k)
    kf = np.round(np.median(k[f], 1)).astype(int)
    m.visual.face_colors = np.c_[PAL[kf], np.full(len(kf), 255)].astype(np.uint8)
    return m


def ref_crop(letter, h):
    im = Image.open(ROOT / 'clifflab' / 'reference' / 'reference-sheet.png').convert('RGB')
    cx, cy = REF_CELLS[letter]
    w, hh = im.width / 4, im.height / 4
    c = im.crop((int(cx * w), int(cy * hh), int((cx + 1) * w), int((cy + 1) * hh)))
    c.thumbnail((h, h))
    return c


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--form', default='run')
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--cols', type=int, default=1)
    ap.add_argument('--ref', default='B')
    ap.add_argument('--gen', default='pile', choices=('pile', 'mass'))
    ap.add_argument('--no-open', action='store_true')
    a = ap.parse_args()
    from dharmatiles.render import render
    gen = rp if a.gen == 'pile' else rm
    r = gen.build(a.seed, a.form, a.cols, a.cols)
    out = ROOT / 'clifflab' / 'out'
    stl = out / f"{a.cols}x{a.cols}-{a.gen}-{a.form}-s{a.seed}.stl"
    r.mesh.export(stl)
    pm = paint(r.mesh)
    H = 560
    ims = []
    for name, el, az in VIEWS:
        p = out / 'iter_tmp.png'
        render([pm], p, elev=el, azim=az, resolution=(1000, 900), quiet=True, smooth=False)
        im = Image.open(p).convert('RGB')
        im.thumbnail((H, H))
        ims.append((name, im))
    # close-up: just the cliff (faces behind the open ground)
    c = pm.triangles_center
    W = a.cols * rm.SQUARE_MM
    keep = (c[:, 1] > W - 26.0) & (c[:, 2] > rm.FOOT_Z_MM - 0.5)
    close = pm.submesh([np.nonzero(keep)[0]], append=True)
    p = out / 'iter_tmp.png'
    render([close], p, elev=18, azim=-112, resolution=(1400, 1000), quiet=True, smooth=False)
    im = Image.open(p).convert('RGB')
    im.thumbnail((int(H * 1.4), H))
    ims.append(('close-up', im))
    ims.append((f'reference {a.ref}', ref_crop(a.ref, H)))
    sheet = Image.new('RGB', (sum(i.width for _, i in ims) + 20 * len(ims), H + 70), (245, 245, 242))
    d = ImageDraw.Draw(sheet)
    x = 10
    for name, im in ims:
        sheet.paste(im, (x, 60))
        d.text((x, 20), name, fill=(30, 30, 30), font=font(24))
        x += im.width + 20
    ov, _ = rm._gl.audit_overhang(r.mesh)
    d.text((x - 700, 20), f"{a.form} {a.cols}x{a.cols} s{a.seed} · {r.rocks} rocks · "
           f"overhang {ov:.0f} mm² · {r.tm['total']:.0f}s", fill=(90, 90, 90), font=font(20))
    stamp = datetime.datetime.now().strftime('%H%M%S')
    path = out / f'iter-{stamp}.png'
    sheet.save(path)
    print(f'→ {path}  ({stl.name})')
    if not a.no_open:
        subprocess.run(['open', str(path)])


if __name__ == '__main__':
    main()
