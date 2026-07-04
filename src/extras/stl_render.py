#!/usr/bin/env python3
"""Render an arbitrary STL headlessly (numpy z-buffer, lambert shading).

Companion to swrender.py (which renders .tile.py specs) — this one is for
analyzing external meshes, e.g. commercial DungeonBlocks pieces.

Usage: python stl_render.py IN.stl OUT.png [--elev E] [--azim A]
       [--width W] [--zclip Z] [--box X0 Y0 X1 Y1]
"""
import argparse
import pathlib

import numpy as np
import trimesh
from PIL import Image


def rot(elev, azim):
    e, a = np.radians(elev - 90.0), np.radians(azim)
    Rz = np.array([[np.cos(a), -np.sin(a), 0],
                   [np.sin(a), np.cos(a), 0], [0, 0, 1]])
    Rx = np.array([[1, 0, 0], [0, np.cos(e), -np.sin(e)],
                   [0, np.sin(e), np.cos(e)]])
    return Rx @ Rz


def main():
    p = argparse.ArgumentParser()
    p.add_argument('stl', type=pathlib.Path)
    p.add_argument('output', type=pathlib.Path)
    p.add_argument('--elev', type=float, default=25.0)
    p.add_argument('--azim', type=float, default=-135.0)
    p.add_argument('--width', type=int, default=1200)
    p.add_argument('--zclip', type=float, default=None)
    p.add_argument('--box', type=float, nargs=4, default=None,
                   metavar=('X0', 'Y0', 'X1', 'Y1'))
    args = p.parse_args()

    m = trimesh.load(args.stl, force='mesh')
    V, F = np.asarray(m.vertices), np.asarray(m.faces)

    if args.box is not None or args.zclip is not None:
        fc = V[F].mean(axis=1)
        keep = np.ones(len(F), dtype=bool)
        if args.box is not None:
            x0, y0, x1, y1 = args.box
            keep &= ((fc[:, 0] >= x0) & (fc[:, 0] <= x1)
                     & (fc[:, 1] >= y0) & (fc[:, 1] <= y1))
        if args.zclip is not None:
            keep &= fc[:, 2] >= args.zclip
        F = F[keep]
        used = np.unique(F)
        remap = np.zeros(len(V), dtype=int)
        remap[used] = np.arange(len(used))
        V, F = V[used], remap[F]

    R = rot(args.elev, args.azim)
    Vc = (V - V.mean(0)) @ R.T
    span = (Vc[:, :2].max(0) - Vc[:, :2].min(0)).max() * 1.08
    W = args.width
    H = int(W * 0.9)
    scale = min(W, H) / span
    px = (Vc[:, 0] - Vc[:, 0].mean()) * scale + W / 2
    py = H / 2 - (Vc[:, 1] - Vc[:, 1].mean()) * scale
    pz = Vc[:, 2]

    tri = np.stack([np.stack([px[F[:, k]], py[F[:, k]], pz[F[:, k]]], 1)
                    for k in range(3)], 1)
    e1 = tri[:, 1, :] - tri[:, 0, :]
    e2 = tri[:, 2, :] - tri[:, 0, :]
    n = np.cross(e1, e2)
    n = n / (np.linalg.norm(n, axis=1) + 1e-12)[:, None]
    n = np.where(n[:, 2:3] < 0, -n, n)
    L1 = np.array([-0.45, 0.5, 0.74]); L1 /= np.linalg.norm(L1)
    L2 = np.array([0.7, -0.3, 0.65]);  L2 /= np.linalg.norm(L2)
    lam = 0.16 + 0.60 * np.clip(n @ L1, 0, 1) + 0.30 * np.clip(n @ L2, 0, 1)
    base = np.array([0.62, 0.64, 0.70])
    shade = base[None, :] * np.clip(lam, 0, 1.3)[:, None]

    zbuf = np.full((H, W), -1e18)
    img = np.ones((H, W, 3)) * np.array([0.93, 0.95, 0.97])
    for idx in np.argsort(tri[:, :, 2].mean(1)):
        t = tri[idx]
        ix0 = max(int(t[:, 0].min()), 0)
        ix1 = min(int(t[:, 0].max()) + 1, W)
        iy0 = max(int(t[:, 1].min()), 0)
        iy1 = min(int(t[:, 1].max()) + 1, H)
        if ix0 >= ix1 or iy0 >= iy1:
            continue
        gx, gy = np.meshgrid(np.arange(ix0, ix1) + 0.5,
                             np.arange(iy0, iy1) + 0.5)
        d = ((t[1, 0] - t[0, 0]) * (t[2, 1] - t[0, 1])
             - (t[2, 0] - t[0, 0]) * (t[1, 1] - t[0, 1]))
        if abs(d) < 1e-12:
            continue
        w0 = ((t[1, 0] - gx) * (t[2, 1] - gy)
              - (t[2, 0] - gx) * (t[1, 1] - gy)) / d
        w1 = ((t[2, 0] - gx) * (t[0, 1] - gy)
              - (t[0, 0] - gx) * (t[2, 1] - gy)) / d
        w2 = 1.0 - w0 - w1
        inside = (w0 >= 0) & (w1 >= 0) & (w2 >= 0)
        if not inside.any():
            continue
        z = w0 * t[0, 2] + w1 * t[1, 2] + w2 * t[2, 2]
        sl = zbuf[iy0:iy1, ix0:ix1]
        upd = inside & (z > sl)
        sl[upd] = z[upd]
        img[iy0:iy1, ix0:ix1][upd] = shade[idx]

    Image.fromarray((np.clip(img, 0, 1) * 255).astype(np.uint8)).save(
        args.output)
    print(f'{len(F):,} faces -> {args.output}')


if __name__ == '__main__':
    main()
