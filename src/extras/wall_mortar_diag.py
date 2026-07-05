#!/usr/bin/env python3
"""Red-mortar diagnostic for wall layers (walls campaign).

Builds a wall with the CORE (mortar) and the STONES as separate parts —
no union — and renders top + four side views with the mortar in red.
The acceptance test (Shawn, fieldstone round 5): a drystone wall is
STACKED FIELDSTONES; anywhere red reads on the surface is a defect.

Usage: python src/extras/wall_mortar_diag.py OUT_PREFIX [--rubble]
Writes OUT_PREFIX-{top,south,east,north,west}.png

--rubble colours the rubble hearting red instead of the core: not an
acceptance test (the hearting is SUPPOSED to show as little rocks in
the gaps since E27) but a visibility check on how much fill the cracks
expose and where.
"""
import pathlib
import sys

import numpy as np
import trimesh

sys.path.insert(0, 'src')

from dharmatiles.core.config import SurfaceConfig            # noqa: E402
from dharmatiles.walls.fieldstone import FieldstoneWall      # noqa: E402
from dharmatiles.walls.masonry import _segments              # noqa: E402

GREY = np.array([0.62, 0.64, 0.70])
RED  = np.array([0.85, 0.15, 0.12])


def build_parts(wall: FieldstoneWall, sq: float):
    """Replicate CutStoneWall.apply()'s build, keeping parts separate.
    Mirrors the rng consumption order of apply() exactly.
    Returns (core, stones, rubble)."""
    segs = _segments([(x * sq, y * sq) for x, y in wall.spine])
    rng  = np.random.default_rng(wall.seed)
    seat_z = 0.0
    cells = wall._cells(segs, wall.thickness_mm, wall.height_mm, rng)
    core   = wall._core_boxes(segs, seat_z)
    stones = [wall._place_block(c, segs, seat_z, rng) for c in cells]
    stones = [s for s in stones if s is not None]
    rubble = wall._extra_parts(segs, seat_z, rng)
    return core, stones, rubble


def render(meshes, colors, out, elev, azim, width=1300):
    V = np.vstack([np.asarray(m.vertices) for m in meshes])
    F, C, off = [], [], 0
    for m, col in zip(meshes, colors):
        f = np.asarray(m.faces)
        F.append(f + off)
        C.append(np.tile(col, (len(f), 1)))
        off += len(m.vertices)
    F = np.vstack(F)
    C = np.vstack(C)

    e, a = np.radians(elev - 90.0), np.radians(azim)
    Rz = np.array([[np.cos(a), -np.sin(a), 0],
                   [np.sin(a), np.cos(a), 0], [0, 0, 1]])
    Rx = np.array([[1, 0, 0], [0, np.cos(e), -np.sin(e)],
                   [0, np.sin(e), np.cos(e)]])
    R = Rx @ Rz
    Vc = (V - V.mean(0)) @ R.T
    span = (Vc[:, :2].max(0) - Vc[:, :2].min(0)).max() * 1.08
    W = width
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
    lam = 0.20 + 0.60 * np.clip(n @ L1, 0, 1) + 0.30 * np.clip(n @ L2, 0, 1)
    shade = C * np.clip(lam, 0, 1.3)[:, None]

    from PIL import Image
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

    Image.fromarray((np.clip(img, 0, 1) * 255).astype(np.uint8)).save(out)
    frac = float((np.abs(img[..., 0] - img[..., 1]) > 0.15).mean())
    print(f'{out}  red-pixel fraction: {frac:.4f}')


def main():
    args = [a for a in sys.argv[1:] if a != '--rubble']
    show_rubble = '--rubble' in sys.argv[1:]
    prefix = args[0] if args else 'wall-diag'
    surface = SurfaceConfig(seed=17)
    sq = surface.square_mm
    wall = FieldstoneWall(spine=[(1.0, 1.0), (0.0, 1.0), (0.0, 0.0)],
                          seed=7)
    core, stones, rubble = build_parts(wall, sq)
    meshes = core + stones + rubble
    colors = ([RED] * len(core) + [GREY] * len(stones)
              + [(RED if show_rubble else GREY)] * len(rubble))
    # Clip below the soil line: the embedded base is buried on the real
    # tile and would dominate the red measurement.
    clipped = []
    for m in meshes:
        c = m.slice_plane([0, 0, 0.2], [0, 0, 1.0], cap=True)
        clipped.append(c if len(c.faces) else m)
    meshes = clipped
    views = {'top':   (80, -45),
             'south': (10, 180),
             'east':  (10, -90),
             'north': (10, 0),
             'west':  (10, 90)}
    for name, (elev, azim) in views.items():
        render(meshes, colors, f'{prefix}-{name}.png', elev, azim)


if __name__ == '__main__':
    main()
