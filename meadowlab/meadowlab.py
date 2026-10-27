#!/usr/bin/env python3
"""meadowlab — rocks + grass + bare earth on one DungeonBlocks tile.

Combines grasslab (hills, flow ribbons, soil lenses, banks) with rocklab
(faceted rocks).  Interaction design: meadowlab/DESIGN.md.

  rocks placed and posed first → real footprints are hard obstacles
  (no rooting under a rock; fronds stop pressed against its face) → the
  flow wraps rocks and soil alike → a minority of rocks get one-sided
  bare-earth aprons → free bare patches added until bare earth (incl.
  under rocks) reaches the target →
  rocks seated into the hills → grass ∪ rocks ∪ DB base → STL

Usage:
    python meadowlab/meadowlab.py                 # 2x2, seed 1
    python meadowlab/meadowlab.py --seed 3 --check out.png
"""
from __future__ import annotations

import argparse
import pathlib
import sys
import time

import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'grasslab'))
sys.path.insert(0, str(ROOT / 'rocklab'))
import grasslab as gl   # noqa: E402
import rocklab as rl    # noqa: E402

ROCK_PLACEMENTS_2X2 = (6, 9)        # fewer than rocklab alone: grass shares the tile
BARE_EARTH_FRAC     = 0.20          # bare earth incl. ground under rocks
GRASS_LENSES_PER_SQ = (8.0, 8.0)    # candidate pool; patches are added until the target is met
APRON_PROB          = 0.35          # most rocks have grass right up to them
APRON_HALF_W        = (0.5, 0.9)    # × rock radius
APRON_STRETCH       = (1.2, 1.9)    # half-length / half-width
APRON_OFFSET        = (0.6, 1.0)    # × R, centre offset to one side of the rock
ROCK_SINK_MM        = 0.3           # below the lowest soil under the footprint
FOOTPRINT_Z_MM      = 0.45          # rock outline sampled this far above its base
TUCK_MM             = 0.5           # fronds stop this far inside the rock face
FLOW_RING_MM        = 0.6           # flow wraps a circle this much wider than the rock


def pose_rocks(rng, placed):
    """Fix each rock's yaw now, so its real footprint is known before grass."""
    return [(x, y, R, man.rotate([0.0, 0.0, float(rng.uniform(0, 360))]), infos)
            for x, y, R, man, infos in placed]


def footprint_mask(posed, grid):
    """Bool grid of the rocks' real outlines where they meet the ground."""
    from matplotlib.path import Path
    X, Y = np.meshgrid(grid.xc, grid.yc, indexing='ij')
    pts = np.stack([X.ravel(), Y.ravel()], 1)
    mask = np.zeros(X.shape, bool)
    for x, y, R, man, _ in posed:
        sl = man.slice(FOOTPRINT_Z_MM)
        sel = (np.abs(X - x) <= R + 1) & (np.abs(Y - y) <= R + 1)
        idx = np.nonzero(sel.ravel())[0]
        for poly in sl.to_polygons():
            poly = np.asarray(poly) + [x, y]
            if len(poly) >= 3:
                hit = Path(poly).contains_points(pts[idx])
                mask.ravel()[idx[hit]] = True
    return mask


def apron_lenses(rng, flow0, posed):
    """One-sided bare-earth patches (wake / scour) for a minority of rocks."""
    c, ax, l, b = [], [], [], []
    for x, y, R, *_ in posed:
        if rng.random() >= APRON_PROB:
            continue
        f = np.array([float(flow0.ux(x, y)), float(flow0.uy(x, y))])
        f /= np.linalg.norm(f)
        side = rng.uniform(-np.pi / 2, np.pi / 2) + (0 if rng.random() < 0.6 else np.pi)
        d = np.array([np.cos(side) * f[0] - np.sin(side) * f[1],
                      np.sin(side) * f[0] + np.cos(side) * f[1]])
        bw = R * rng.uniform(*APRON_HALF_W)
        c.append([x, y] + d * R * rng.uniform(*APRON_OFFSET))
        ax.append(f)
        b.append(bw)
        l.append(bw * rng.uniform(*APRON_STRETCH))
    return np.array(c).reshape(-1, 2), np.array(ax).reshape(-1, 2), np.array(l), np.array(b)


def extend_lenses(lenses, c, ax, l, b):
    import copy
    out = copy.copy(lenses)
    out.c = np.concatenate([lenses.c, c])
    out.ax = np.concatenate([lenses.ax, ax])
    out.pp = np.concatenate([lenses.pp, np.stack([-ax[:, 1], ax[:, 0]], 1)])
    out.l = np.concatenate([lenses.l, l])
    out.b = np.concatenate([lenses.b, b])
    return out


def take_lenses(lenses, k):
    import copy
    t = copy.copy(lenses)
    t.c, t.ax, t.pp, t.l, t.b = (lenses.c[:k], lenses.ax[:k], lenses.pp[:k],
                                 lenses.l[:k], lenses.b[:k])
    return t


def fill_bare_earth(free, aprons, foot, grid, target):
    """Add natural-size free lenses (best-scored first) until bare earth —
    soil plus ground under rocks — reaches the target.  No size scaling."""
    s = 4
    Xc, Yc = np.meshgrid(grid.xc[::s], grid.yc[::s], indexing='ij')
    fc = foot[::s, ::s]
    for k in range(len(free.c) + 1):
        lens = extend_lenses(take_lenses(free, k), *aprons)
        frac = float(((lens.mask(Xc, Yc) > 0.5) | fc).mean())
        if frac >= target:
            break
    return lens, k


def seat_rocks(posed, H0, foot, grid):
    """Rocks sit in the ground: base just below the lowest soil under them."""
    m3d = rl._m3d()
    out = []
    vox = grid.vox
    for x, y, R, man, _ in posed:
        i0, i1 = max(int((x - R) / vox), 0), min(int((x + R) / vox) + 1, grid.nx)
        j0, j1 = max(int((y - R) / vox), 0), min(int((y + R) / vox) + 1, grid.ny)
        sub = foot[i0:i1, j0:j1]
        z0 = float(H0[i0:i1, j0:j1][sub].min() if sub.any() else H0[i0:i1, j0:j1].min())
        out.append(man.translate([x, y, z0 - ROCK_SINK_MM]))
    return m3d.Manifold.batch_boolean(out, m3d.OpType.Add) if out else None


def attach(top_mesh, rocks, cols, rows):
    import manifold3d as m3d
    import trimesh
    from dharmatiles.core.config import BaseConfig, SurfaceConfig
    from dharmatiles.bases import dungeonblocks as db

    def to_m(t):
        return m3d.Manifold(mesh=m3d.Mesh(
            vert_properties=np.asarray(t.vertices, dtype=np.float32),
            tri_verts=np.asarray(t.faces, dtype=np.uint32)))

    top = to_m(top_mesh).simplify(gl.SIMPLIFY_TOL_MM)
    if rocks is not None:
        top = top + rocks
    zmax = top.bounding_box()[5]
    surface = SurfaceConfig(cols=cols, rows=rows, square_mm=gl.SQUARE_MM)
    base_cfg = BaseConfig()
    peg_h = db.select_peg_height(np.zeros((2, 2)), base_cfg, zmax)
    solid = top + to_m(db.make_base(surface, peg_h, base_cfg))
    parts = solid.decompose()
    vols = [q.volume() for q in parts]
    k = int(np.argmax(vols))
    crumbs = (len(parts) - 1, float(sum(vols) - vols[k]))
    out = parts[k].to_mesh()
    mesh = trimesh.Trimesh(np.asarray(out.vert_properties)[:, :3],
                           np.asarray(out.tri_verts), process=False)
    return mesh, peg_h, crumbs, zmax


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--cols', type=int, default=2)
    ap.add_argument('--rows', type=int, default=2)
    ap.add_argument('--check', type=pathlib.Path, default=None)
    args = ap.parse_args()
    t0 = time.perf_counter()
    rng = np.random.default_rng(args.seed)                  # grass stream
    rrng = np.random.default_rng(args.seed + 1000003)       # rock stream
    W, H = args.cols * gl.SQUARE_MM, args.rows * gl.SQUARE_MM
    grid = gl.Grid(W, H, W / round(W / 0.12))

    rl.PLACEMENTS_2X2 = ROCK_PLACEMENTS_2X2
    gl.SOIL_LENSES_PER_SQ = GRASS_LENSES_PER_SQ

    ground, gstats = gl.make_ground(rng, W, H, 0.25, False)
    flow0 = gl.make_direction(rng, ground)
    sw = gl.Swaths(rng, W, H, flow0.ux, flow0.uy)
    posed = pose_rocks(rrng, rl.scatter(rrng, W, H))         # rocks first
    foot = footprint_mask(posed, grid)
    # Soil: free lenses + one-sided aprons, scaled to the visible-bare target.
    free = gl.Lenses(rng, ground, flow0, W, H)
    ac, aax, al, ab = apron_lenses(rrng, flow0, posed)
    soil_l, n_free = fill_bare_earth(free, (ac, aax, al, ab), foot, grid, BARE_EARTH_FRAC)
    # Flow wraps rocks (circles) and soil lenses alike.
    rc = np.array([[x, y] for x, y, *_ in posed])
    rax = np.stack([flow0.ux(rc[:, 0], rc[:, 1]), flow0.uy(rc[:, 0], rc[:, 1])], 1)
    rax /= np.linalg.norm(rax, axis=1, keepdims=True)
    rr = np.array([R for _, _, R, *_ in posed]) + FLOW_RING_MM
    flow = gl.finish_flow(rng, flow0, extend_lenses(soil_l, rc, rax, rr, rr), W, H)

    H0, T, lips, n_fronds, soil, st = gl.build_grass(
        rng, grid, ground, sw, soil_l, flow, W, H, blocker=foot, tuck_mm=TUCK_MM)
    top_z = max(T.max(), max((tp.max() for *_, tp in lips), default=0)) + 0.3
    F = gl.make_field(grid, T, lips, top_z)
    added, removed = gl.fdm_backstop(F, grid.vox)
    top_mesh = gl.mesh_field(F, grid.vox)
    rocks = seat_rocks(posed, H0, foot, grid)
    mesh, peg_h, crumbs, zmax = attach(top_mesh, rocks, args.cols, args.rows)
    placed = posed
    bare = float(((soil > 0.5) | foot).mean())

    out = pathlib.Path(__file__).parent / 'out'
    out.mkdir(exist_ok=True)
    path = out / f"{args.cols}x{args.rows}-meadow-s{args.seed}.stl"
    mesh.export(path)
    n_stones = sum(len(p[4]) for p in placed)
    print(f"→ {path}  ({path.stat().st_size / 1e6:.1f} MB)")
    print(f"  mesh: {len(mesh.vertices):,} verts, {len(mesh.faces):,} faces, "
          f"watertight={mesh.is_watertight}, peg={peg_h} mm, crumbs {crumbs[0]}")
    print(f"  rocks: {len(placed)} placements, {n_stones} stones, footprint {foot.mean():.1%} of tile; "
          f"{len(ac)} with bare-earth aprons")
    print(f"  bare earth incl. under rocks: {bare:.1%} (target {BARE_EARTH_FRAC:.0%}); "
          f"visible {float(((soil > 0.5) & ~foot).mean()):.1%}; {n_free} free lenses")
    print(f"  {n_fronds} fronds in {st[0]} strands; ground {gstats['ground_min']:.2f}–"
          f"{gstats['ground_max']:.2f} mm; top z {zmax:.2f} mm")
    print(f"  FDM backstop +{added:,}/−{removed:,}; overhang {gl.audit_overhang(mesh)[0]:.1f} mm²; "
          f"total {time.perf_counter() - t0:.1f}s")
    if args.check:
        gl.self_check(mesh, args.check, W, H)
        print(f"  self-check → {args.check}")


if __name__ == '__main__':
    main()
