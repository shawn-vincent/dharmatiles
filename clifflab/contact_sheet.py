#!/usr/bin/env python3
"""clifflab contact sheet: forms × seeds × presets, painted (simulated
drybrush) and grey.

Paint (DESIGN.md §3), per vertex then quantised to a palette:
  stone — dark base coat; a black-brown wash in recesses (cavity = how far
  the vertex sits below its Laplacian-smoothed position after many
  iterations); drybrush layers by edge convexity (a few iterations) plus a
  bonus for up-facing surfaces; occasional ochre staining (low-frequency
  mask).  Dirt — brown base, light drybrush on bumps.  Tile walls — flat dark.

Usage:
    python clifflab/contact_sheet.py                 # the full sheet
    python clifflab/contact_sheet.py --one run:1     # one row (form:seed[:preset[:cols]])
    python clifflab/contact_sheet.py --one pair:1    # an assembly (pair|outer|L:seed[:preset])
"""
from __future__ import annotations

import argparse
import datetime
import pathlib
import sys

import numpy as np
import trimesh
from PIL import Image, ImageDraw, ImageFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'clifflab'))
import clifflab as cl  # noqa: E402

# (kind, spec): 'tile' → (form, seed, preset, cols); 'pair' / 'outer' / 'L' →
# (preset, seed A, seed B) assemblies of 1 × 1 tiles
ROWS = [('tile', ('run', 1, 'default', 1)), ('tile', ('run', 2, 'default', 1)),
        ('tile', ('run', 3, 'deep', 1)), ('tile', ('run', 4, 'default', 2)),
        ('tile', ('wall', 1, 'default', 1)), ('tile', ('wall', 5, 'default', 2)),
        ('tile', ('corner', 1, 'default', 1)), ('tile', ('canyon', 2, 'default', 1)),
        ('pair', ('default', 1, 2)), ('pair', ('default', 3, 4)),
        ('outer', ('default', 1, 2)), ('L', ('default', 3, 4)),
        ('tile', ('outcrop', 1, 'default', 2)), ('tile', ('spire', 1, 'default', 2))]
# whole tiles only, no crops: obliques both sides, front, both ends, top
VIEWS = [('oblique left', 28, -125), ('oblique right', 28, -55), ('front', 6, -90),
         ('left end', 6, 180), ('right end', 6, 0), ('back', 6, 90), ('top-down', 89, -90)]
CELL = (400, 340)
HEAD_W = 230

PAL = {
    'wash': (24, 22, 22), 'base': (46, 43, 41), 'mid': (68, 64, 60), 'layer': (92, 87, 81),
    'dry': (122, 116, 107), 'edge': (158, 152, 141),
    'mid_warm': (78, 66, 52), 'layer_warm': (104, 90, 70),
    'dirt_dark': (68, 52, 38), 'dirt': (102, 80, 58), 'dirt_dry': (146, 124, 96),
    'side': (40, 40, 42),
}


def font(size):
    for f in ('/System/Library/Fonts/Helvetica.ttc', '/Library/Fonts/Arial.ttf'):
        try:
            return ImageFont.truetype(f, size)
        except OSError:
            pass
    return ImageFont.load_default()


def _smoothed(mesh, iters):
    L = trimesh.smoothing.laplacian_calculation(mesh, equal_weight=True)
    v = mesh.vertices.copy()
    out = {}
    for k in range(1, max(iters) + 1):
        v = L @ v
        if k in iters:
            out[k] = v.copy()
    return out


def vertex_paint(mesh, r):
    """Per-vertex paint inputs on the built mesh: stone value t, dirt value
    td, and the dirt / side / stain flags."""
    W, H = r.X[-1, 0], r.Y[0, -1]
    v, n = mesh.vertices, mesh.vertex_normals
    sm = _smoothed(mesh, (6, 40))
    conv_s = -np.einsum('ij,ij->i', sm[6] - v, n)        # + on convex edges
    conv_l = -np.einsum('ij,ij->i', sm[40] - v, n)       # − in recesses
    h = cl.VOX_MM
    i = np.clip(np.round(v[:, 0] / h).astype(int), 0, r.X.shape[0] - 1)
    j = np.clip(np.round(v[:, 1] / h).astype(int), 0, r.X.shape[1] - 1)
    zG = r.G[i, j]
    on_cap = np.zeros(len(v), bool)
    for b in r.bodies:
        if b.cap:
            on_cap |= np.abs(v[:, 2] - (b.top_z - cl.PLATEAU_DROP_MM + r.dirt[i, j])) < 0.3
    dirt = (n[:, 2] > 0.45) & ((np.abs(v[:, 2] - zG) < 0.45) | on_cap)
    side = ((((v[:, 0] < 0.05) | (v[:, 0] > W - 0.05) | (v[:, 1] < 0.05) | (v[:, 1] > H - 0.05))
             & (v[:, 2] < cl.FOOT_Z_MM + 0.5)) | (v[:, 2] < cl.FOOT_Z_MM - 0.6)) \
        & (np.abs(n[:, 2]) < 0.5) | (v[:, 2] < cl.FOOT_Z_MM - 1.2)
    t = (0.48 + 0.22 * conv_s / (conv_s.std() + 1e-9) + 0.2 * conv_l / (conv_l.std() + 1e-9)
         + 0.12 * np.clip(n[:, 2], 0, 1))
    L = trimesh.smoothing.laplacian_calculation(mesh, equal_weight=True)
    for _ in range(2):                    # a brush, not a per-vertex speckle
        t = L @ t
    rng = np.random.default_rng(7)               # ochre stains: a few 3-D patches
    stain = np.zeros(len(v), bool)
    lo, hi = v.min(0), v.max(0)
    for c, rad in zip(rng.uniform(lo, hi, (8, 3)), rng.uniform(2.5, 6.0, 8)):
        stain |= ((v - c) ** 2).sum(1) < rad ** 2
    td = 0.5 + 0.3 * conv_s / (conv_s.std() + 1e-9)
    return t, td, dirt, side, stain


def classify(t, td, dirt, side, stain):
    names = list(PAL)
    k = np.select([t < 0.18, t < 0.36, t < 0.56, t < 0.74, t < 0.9],
                  [names.index('wash'), names.index('base'), names.index('mid'),
                   names.index('layer'), names.index('dry')], names.index('edge'))
    k = np.where(stain & (k == names.index('mid')), names.index('mid_warm'), k)
    k = np.where(stain & (k == names.index('layer')), names.index('layer_warm'), k)
    kd = np.select([td < 0.3, td < 0.8], [names.index('dirt_dark'), names.index('dirt')],
                   names.index('dirt_dry'))
    k = np.where(dirt, kd, k)
    return np.where(side, names.index('side'), k)


def face_paint(mesh, k, W, H):
    """Per-face palette index from per-vertex k; faces on the tile boundary
    (slab sides, backs against the edge) are tile walls."""
    names = list(PAL)
    kf = np.round(np.median(k[mesh.faces], 1)).astype(int)
    c, fn = mesh.triangles_center, mesh.face_normals
    # slab sides only: rock faces now lie ON the tile boundary (butted tiles
    # touch), so boundary faces above the foot ground are rock, not wall
    wall = (np.abs(fn[:, 2]) < 0.5) & ((c[:, 0] < 0.02) | (c[:, 0] > W - 0.02)
                                       | (c[:, 1] < 0.02) | (c[:, 1] > H - 0.02)) \
        & (c[:, 2] < cl.FOOT_Z_MM + 0.5)
    return np.where(wall | (c[:, 2] < cl.FOOT_Z_MM - 1.0), names.index('side'), kf)


def painted(r):
    """Painted mesh.  Paint is measured on the built mesh, then carried onto a
    subdivided copy by barycentric interpolation: big flat triangles (the
    simplifier merges coplanar areas) get a smooth paint gradient instead of
    one solid band from their three corners."""
    pal = np.array(list(PAL.values()), float)
    W, H = r.X[-1, 0], r.Y[0, -1]
    base = r.mesh
    t, td, dirt, side, stain = vertex_paint(base, r)
    v, f, idx = trimesh.remesh.subdivide_to_size(base.vertices, base.faces, max_edge=0.6,
                                                 return_index=True)
    src = base.faces[idx]                                   # original face per new face
    tri = base.vertices[src]                                # (m, 3, 3)
    lerp = np.zeros((len(v), 3))
    for c in range(3):                                      # barycentric of each corner
        lerp[f[:, c]] = trimesh.triangles.points_to_barycentric(tri, v[f[:, c]])
    owner = np.zeros(len(v), int)
    for c in range(3):
        owner[f[:, c]] = np.arange(len(f))
    corners = src[owner]                                    # (nv, 3) original vertex ids
    tv = (lerp * t[corners]).sum(1)
    tdv = (lerp * td[corners]).sum(1)
    near = corners[np.arange(len(v)), lerp.argmax(1)]
    k = classify(tv, tdv, dirt[near], side[near], stain[near])
    mesh = trimesh.Trimesh(v, f, process=False)
    kf = face_paint(mesh, k, W, H)
    mesh.visual.face_colors = np.c_[pal[kf], np.full(len(kf), 255)].astype(np.uint8)
    return mesh


def placed(m, rot_deg=0.0, dx=0.0, dy=0.0, c=17.5):
    m = m.copy()
    T = trimesh.transformations.rotation_matrix(np.radians(rot_deg), [0, 0, 1], [c, c, 0])
    T[:2, 3] += [dx, dy]
    m.apply_transform(T)
    return m


def assembly(kind, preset, sa, sb):
    """Painted 1 × 1 tiles placed together → (meshes, builds)."""
    A = cl.build(sa, 'run', preset, 1, 1)
    B = cl.build(sb, 'run', preset, 1, 1)
    if kind == 'pair':                    # side by side: the ends meet at x = 35
        return [painted(A), placed(painted(B), dx=35)], (A, B)
    if kind == 'outer':                   # outside corner: plateau NE, ends meet at (35, 35)
        return [placed(painted(A), dx=35), placed(painted(B), -90, 0, 35)], (A, B)
    C = cl.build(sa + 10, 'corner', preset, 1, 1)   # L: run → corner tile → rotated run
    return [painted(A), placed(painted(C), dx=35), placed(painted(B), -90, 35, -35)], (A, B, C)


def render_row(meshes, tmp, tag):
    from dharmatiles.render import render
    out = []
    for name, elev, azim in VIEWS:
        p = tmp / f'{tag}-{name.replace(" ", "_")}.png'
        render(meshes, p, elev=elev, azim=azim, resolution=(1000, 800), quiet=True, smooth=False)
        im = Image.open(p).convert('RGB')
        im.thumbnail((CELL[0] - 12, CELL[1] - 12))
        out.append(im)
    return out


def row_image(kind, spec, tmp):
    if kind == 'tile':
        form, seed, preset, cols = spec
        r = cl.build(seed, form, preset, cols, cols)
        r.mesh.export(cl.out_path(seed, form, preset, cols, cols))
        meshes, rs = [painted(r)], (r,)
        tag = f'{cols}x{cols}-{form}-{preset}-s{seed}'
        title = [form, f'{cols} × {cols}, seed {seed}', f'preset {preset}']
    else:
        preset, sa, sb = spec
        meshes, rs = assembly(kind, preset, sa, sb)
        tag = f'{kind}-{preset}-s{sa}-{sb}'
        what = {'pair': 'butted pair', 'outer': 'outside corner', 'L': 'run + corner + run'}[kind]
        title = [what, f'1 × 1 tiles, seeds {sa}, {sb}', f'preset {preset}']
    ims = render_row(meshes, tmp, tag)
    r = rs[0]
    q, f_, d = r.split
    ov = max(cl._gl.audit_overhang(x.mesh)[0] for x in rs)
    lines = title + ['', f'quiet {q:.0%} / fracture {f_:.0%}', f'detail {d:.0%}',
                     f'overhang ≤ {ov:.1f} mm²',
                     'watertight' if all(x.mesh.is_watertight for x in rs) else 'NOT WATERTIGHT']
    row = Image.new('RGB', (HEAD_W + CELL[0] * len(ims), CELL[1]), (245, 245, 242))
    dr = ImageDraw.Draw(row)
    y = 18
    for k, t in enumerate(lines):
        dr.text((16, y), t, fill=(20, 20, 20) if k == 0 else (70, 70, 70), font=font(28 if k == 0 else 19))
        y += 36 if k == 0 else 25
    for c, im in enumerate(ims):
        row.paste(im, (HEAD_W + c * CELL[0] + (CELL[0] - im.width) // 2, (CELL[1] - im.height) // 2))
    print(f'  {tag}: split {q:.0%}/{f_:.0%}/{d:.0%}, overhang ≤ {ov:.1f} mm²', flush=True)
    return row


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--one', default=None, help='form:seed[:preset] — one row only')
    args = ap.parse_args()
    out = ROOT / 'clifflab' / 'out'
    tmp = out / 'sheet_tmp'
    tmp.mkdir(parents=True, exist_ok=True)
    rows = ROWS
    if args.one:
        f, s_, *p = args.one.split(':')
        rows = [('tile', (f, int(s_), p[0] if p else 'default', int(p[1]) if len(p) > 1 else 1))] \
            if f not in ('pair', 'outer', 'L') else [(f, (p[0] if p else 'default', int(s_), int(s_) + 1))]
    imgs = [row_image(k, spec, tmp) for k, spec in rows]
    head = Image.new('RGB', (imgs[0].width, 60), (30, 30, 32))
    dr = ImageDraw.Draw(head)
    for c, (name, _, _) in enumerate(VIEWS):
        dr.text((HEAD_W + c * CELL[0] + 16, 16), name, fill=(235, 235, 235), font=font(24))
    sheet = Image.new('RGB', (imgs[0].width, 60 + sum(i.height for i in imgs)), 'white')
    sheet.paste(head, (0, 0))
    y = 60
    for im in imgs:
        sheet.paste(im, (0, y))
        y += im.height
    stamp = datetime.datetime.now().strftime('%Y%m%d-%H%M')
    name = f'contact-{args.one.replace(":", "-")}-{stamp}.png' if args.one else f'contact-{stamp}.png'
    sheet.save(out / name)
    print(f'→ {out / name}')


if __name__ == '__main__':
    main()
