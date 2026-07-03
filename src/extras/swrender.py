#!/usr/bin/env python3
"""Headless software renderer (numpy z-buffer) — stopgap when GL is unavailable.

Usage: python swrender.py TILE.tile.py OUT.png [--elev E] [--azim A] [--width W]
Orthographic camera, lambert shading, per-part colors like dharmatiles.render.
"""
import argparse, pathlib, sys
import numpy as np


def rot(elev, azim):
    e, a = np.radians(elev - 90.0), np.radians(azim)
    Rz = np.array([[np.cos(a), -np.sin(a), 0], [np.sin(a), np.cos(a), 0], [0, 0, 1]])
    Rx = np.array([[1, 0, 0], [0, np.cos(e), -np.sin(e)], [0, np.sin(e), np.cos(e)]])
    return Rx @ Rz


def main():
    p = argparse.ArgumentParser()
    p.add_argument('tile', type=pathlib.Path)
    p.add_argument('output', type=pathlib.Path)
    p.add_argument('--elev', type=float, default=40.0)
    p.add_argument('--azim', type=float, default=-135.0)
    p.add_argument('--width', type=int, default=1000)
    p.add_argument('--no-tree', action='store_true',
                   help='skip WOOD/FOLIAGE/LEAF parts (see the ground under a canopy)')
    p.add_argument('--no-base', action='store_true',
                   help='skip the socket base part (its huge triangles defeat --box zoom)')
    p.add_argument('--box', type=float, nargs=4, default=None,
                   metavar=('X0','Y0','X1','Y1'),
                   help='crop: only render faces whose centroid xy (mm) is in box')
    p.add_argument('--zclip', type=float, default=None,
                   help='drop faces whose centroid z (mm) is below this '
                        '(hides buried geometry x-rayed by --box crops)')
    args = p.parse_args()

    from dharmatiles.terrains.tile import build_meshes_for_render
    meshes = build_meshes_for_render(args.tile)
    print(f'{len(meshes)} parts, {sum(len(m.faces) for m in meshes):,} faces')

    GREEN = np.array([0.32, 0.72, 0.20])
    GREY  = np.array([0.75, 0.75, 0.78])
    from dharmatiles.core.color import Material
    PALETTE = {
        Material.SOIL:  np.array([0.55, 0.35, 0.17]),
        Material.ROCK:  np.array([0.42, 0.50, 0.59]),
        Material.GRASS: GREEN,
        Material.WATER: np.array([0.08, 0.61, 0.82]),
        Material.BASE:  np.array([0.42, 0.42, 0.44]),
        Material.WOOD:  np.array([0.55, 0.39, 0.25]),
    }
    verts_all, faces_all, cols_all = [], [], []
    off = 0
    for m in meshes:
        if args.no_tree and m.metadata.get('material') in (6, 7, 8):
            continue
        if args.no_base and (m.metadata.get('material') == 4
                             or m.vertices[:, 2].min() < -1.0):
            continue
        v, f = m.vertices, m.faces
        verts_all.append(v); faces_all.append(f + off); off += len(v)
        fc = getattr(m.visual, 'face_colors', None)
        if fc is not None and len(fc) == len(f) and fc[:, :3].any():
            cols_all.append(fc[:, :3].astype(float) / 255.0)
            continue
        mat = m.metadata.get('material')
        if mat is not None and mat in PALETTE:
            c = PALETTE[mat]
        else:
            v_ = m.vertices
            is_base = v_[:, 2].min() < -1.0
            is_rock = (not is_base) and len(m.faces) < 10000
            c = GREY if (is_base or is_rock) else GREEN
        cols_all.append(np.tile(c, (len(f), 1)))
    V = np.vstack(verts_all); F = np.vstack(faces_all); C = np.vstack(cols_all)

    if args.box is not None or args.zclip is not None:
        fc = V[F].mean(axis=1)
        keep = np.ones(len(F), dtype=bool)
        if args.box is not None:
            x0, y0, x1, y1 = args.box
            keep &= (fc[:,0]>=x0)&(fc[:,0]<=x1)&(fc[:,1]>=y0)&(fc[:,1]<=y1)
        if args.zclip is not None:
            keep &= fc[:,2] >= args.zclip
        F = F[keep]; C = C[keep]
        used = np.unique(F)
        remap = np.zeros(len(V), dtype=int); remap[used] = np.arange(len(used))
        V = V[used]; F = remap[F]

    R = rot(args.elev, args.azim)
    Vc = (V - V.mean(0)) @ R.T
    # screen: x -> px, y -> py (flip), z -> depth toward viewer
    span = (Vc[:, :2].max(0) - Vc[:, :2].min(0)).max() * 1.1
    W = args.width; H = int(W * 0.84)
    scale = min(W, H) / span
    px = (Vc[:, 0] - Vc[:, 0].mean()) * scale + W / 2
    py = H / 2 - (Vc[:, 1] - Vc[:, 1].mean()) * scale
    pz = Vc[:, 2]

    tri = np.stack([np.stack([px[F[:, k]], py[F[:, k]], pz[F[:, k]]], 1) for k in range(3)], 1)
    # face normals in camera space for shading
    e1 = tri[:, 1, :] - tri[:, 0, :]
    e2 = tri[:, 2, :] - tri[:, 0, :]
    n = np.cross(e1, e2)
    nl = np.linalg.norm(n, axis=1) + 1e-12
    n = n / nl[:, None]
    # orient normals toward viewer (camera looks down +z after rotation)
    n = np.where(n[:, 2:3] < 0, -n, n)
    L1 = np.array([-0.45, 0.5, 0.74]); L1 /= np.linalg.norm(L1)
    L2 = np.array([0.7, -0.3, 0.65]);  L2 /= np.linalg.norm(L2)
    lam = 0.18 + 0.62 * np.clip(n @ L1, 0, 1) + 0.30 * np.clip(n @ L2, 0, 1)
    # synthetic drybrush: world-height tint (higher geometry catches highlight)
    wz = V[:, 2]
    fz = wz[F].mean(1)
    zlo, zhi = np.percentile(fz, 5), np.percentile(fz, 99.5)
    dry = 0.72 + 0.5 * np.clip((fz - zlo) / max(zhi - zlo, 1e-9), 0, 1)
    shade = C * np.clip(lam * dry, 0, 1.35)[:, None]

    zbuf = np.full((H, W), -1e18)
    img = np.ones((H, W, 3)) * np.array([0.93, 0.95, 0.97])

    # rasterize (painter-free z-buffer), skip degenerate
    order = np.argsort(tri[:, :, 2].mean(1))    # near-far irrelevant with zbuf; keep cache-friendly
    for idx in order:
        t = tri[idx]
        x0, x1 = t[:, 0].min(), t[:, 0].max()
        y0, y1 = t[:, 1].min(), t[:, 1].max()
        ix0, ix1 = max(int(x0), 0), min(int(x1) + 1, W)
        iy0, iy1 = max(int(y0), 0), min(int(y1) + 1, H)
        if ix0 >= ix1 or iy0 >= iy1:
            continue
        xs = np.arange(ix0, ix1) + 0.5
        ys = np.arange(iy0, iy1) + 0.5
        gx, gy = np.meshgrid(xs, ys)
        d = ((t[1, 0] - t[0, 0]) * (t[2, 1] - t[0, 1])
             - (t[2, 0] - t[0, 0]) * (t[1, 1] - t[0, 1]))
        if abs(d) < 1e-12:
            continue
        w0 = ((t[1, 0] - gx) * (t[2, 1] - gy) - (t[2, 0] - gx) * (t[1, 1] - gy)) / d
        w1 = ((t[2, 0] - gx) * (t[0, 1] - gy) - (t[0, 0] - gx) * (t[2, 1] - gy)) / d
        w2 = 1.0 - w0 - w1
        inside = (w0 >= 0) & (w1 >= 0) & (w2 >= 0)
        if not inside.any():
            continue
        z = w0 * t[0, 2] + w1 * t[1, 2] + w2 * t[2, 2]
        zb = zbuf[iy0:iy1, ix0:ix1]
        upd = inside & (z > zb)
        zb[upd] = z[upd]
        img[iy0:iy1, ix0:ix1][upd] = shade[idx]

    from PIL import Image
    Image.fromarray((np.clip(img, 0, 1) * 255).astype(np.uint8)).save(args.output)
    print(f'-> {args.output}')


if __name__ == '__main__':
    main()
