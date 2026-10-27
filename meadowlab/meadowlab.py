#!/usr/bin/env python3
"""meadowlab — rocks + grass + bare earth on one DungeonBlocks tile.

Combines grasslab (hills, flow ribbons, soil lenses, banks) with rocklab
(faceted rocks).  Interaction design: meadowlab/DESIGN.md.

  ground, flow, swaths (grasslab) ─► rocks scattered and posed first
  (rocklab) → real footprints become hard obstacles ─► one-sided aprons
  for a minority of rocks + free lenses added at natural size until bare
  earth (incl. under rocks) reaches the target ─► flow wraps soil lenses
  and rock circles ─► grass (no rooting under rocks, fronds stop pressed
  against them) ─► rocks seated into the hills ─► grass ∪ rocks ∪ DB base
  ─► STL

Usage:
    python meadowlab/meadowlab.py                 # 2x2, seed 1
    python meadowlab/meadowlab.py --seed 3 --check out.png
"""
from __future__ import annotations

import argparse
import pathlib
import sys
import time

import manifold3d as m3d
import numpy as np
import trimesh

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'grasslab'))
sys.path.insert(0, str(ROOT / 'rocklab'))
import grasslab as gl   # noqa: E402
import rocklab as rl    # noqa: E402

ROCK_SEED_OFFSET    = 1000003       # rock RNG stream = seed + this
ROCK_PLACEMENTS_2X2 = (6, 9)        # fewer than rocklab alone: grass shares the tile
FREE_LENS_POOL_PER_SQ = (8.0, 8.0)  # candidate pool; patches are added until the target is met
BARE_EARTH_FRAC     = 0.20          # bare earth incl. ground under rocks
BARE_SAMPLE_STRIDE  = 4             # bare fraction measured on every 4th grass column
APRON_PROB          = 0.35          # most rocks have grass right up to them
APRON_WAKE_PROB     = 0.6           # otherwise the apron is on the upstream side
APRON_HALF_W        = (0.5, 0.9)    # × rock radius
APRON_STRETCH       = (1.2, 1.9)    # half-length / half-width
APRON_OFFSET        = (0.6, 1.0)    # × R, centre offset to one side of the rock
FOOTPRINT_Z_MM      = 0.45          # rock outline sampled this far above its base
TUCK_MM             = 0.5           # fronds stop this far inside the rock face
FLOW_RING_MM        = 0.6           # flow wraps a circle this much wider than the rock
ROCK_SINK_MM        = 0.3           # below the lowest ground under the footprint


def footprint_mask(posed, grid):
    """Bool grid of the rocks' real outlines where they meet the ground."""
    from matplotlib.path import Path
    X, Y = np.meshgrid(grid.xc, grid.yc, indexing='ij')
    pts = np.stack([X.ravel(), Y.ravel()], 1)
    mask = np.zeros(X.shape, bool)
    for p in posed:
        sel = (np.abs(X - p.x) <= p.r + 1) & (np.abs(Y - p.y) <= p.r + 1)
        idx = np.nonzero(sel.ravel())[0]
        for poly in p.solid.slice(FOOTPRINT_Z_MM).to_polygons():
            poly = np.asarray(poly) + [p.x, p.y]
            if len(poly) >= 3:
                hit = Path(poly).contains_points(pts[idx])
                mask.ravel()[idx[hit]] = True
    return mask


def apron_shapes(rng, flow0, posed):
    """One-sided bare-earth patches (wake / scour) for a minority of rocks.
    Returns lens shapes (centre, axis, half_len, half_w)."""
    c, ax, l, b = [], [], [], []
    for p in posed:
        if rng.random() >= APRON_PROB:
            continue
        f = np.array([float(flow0.ux(p.x, p.y)), float(flow0.uy(p.x, p.y))])
        f /= np.linalg.norm(f)
        side = rng.uniform(-np.pi / 2, np.pi / 2) + (0 if rng.random() < APRON_WAKE_PROB else np.pi)
        d = np.array([np.cos(side) * f[0] - np.sin(side) * f[1],
                      np.sin(side) * f[0] + np.cos(side) * f[1]])
        bw = p.r * rng.uniform(*APRON_HALF_W)
        c.append([p.x, p.y] + d * p.r * rng.uniform(*APRON_OFFSET))
        ax.append(f)
        b.append(bw)
        l.append(bw * rng.uniform(*APRON_STRETCH))
    return np.array(c).reshape(-1, 2), np.array(ax).reshape(-1, 2), np.array(l), np.array(b)


def combine(lenses, k, shapes):
    """The first k of `lenses` plus extra lens shapes, sharing its border warp."""
    c, ax, l, b = shapes
    return gl.Lenses(np.concatenate([lenses.centre[:k], c]),
                     np.concatenate([lenses.axis[:k], ax]),
                     np.concatenate([lenses.half_len[:k], l]),
                     np.concatenate([lenses.half_w[:k], b]), lenses.warp)


def fill_bare_earth(free, aprons, foot, grid, target):
    """Add natural-size free lenses (best-scored first) until bare earth —
    soil plus ground under rocks — reaches the target.  No size scaling.
    Returns (lenses, number of free lenses used)."""
    s = BARE_SAMPLE_STRIDE
    Xc, Yc = np.meshgrid(grid.xc[::s], grid.yc[::s], indexing='ij')
    fc = foot[::s, ::s]
    for k in range(len(free.centre) + 1):
        lens = combine(free, k, aprons)
        if float(((lens.mask(Xc, Yc) > 0.5) | fc).mean()) >= target:
            break
    return lens, k


def rock_circles(flow0, posed):
    """Each rock as a flow obstacle: a circle R + FLOW_RING_MM, axis = flow."""
    rc = np.array([[p.x, p.y] for p in posed])
    rax = np.stack([flow0.ux(rc[:, 0], rc[:, 1]), flow0.uy(rc[:, 0], rc[:, 1])], 1)
    rax /= np.linalg.norm(rax, axis=1, keepdims=True)
    rr = np.array([p.r for p in posed]) + FLOW_RING_MM
    return rc, rax, rr, rr


def seat_rocks(posed, H0, foot, grid):
    """Rocks sit in the ground: base just below the lowest ground under them."""
    out = []
    vox = grid.vox
    for p in posed:
        i0, i1 = max(int((p.x - p.r) / vox), 0), min(int((p.x + p.r) / vox) + 1, grid.nx)
        j0, j1 = max(int((p.y - p.r) / vox), 0), min(int((p.y + p.r) / vox) + 1, grid.ny)
        sub = foot[i0:i1, j0:j1]
        z0 = float(H0[i0:i1, j0:j1][sub].min() if sub.any() else H0[i0:i1, j0:j1].min())
        out.append(p.solid.translate([p.x, p.y, z0 - ROCK_SINK_MM]))
    return m3d.Manifold.batch_boolean(out, m3d.OpType.Add) if out else None


def attach_base(grass, rocks, cols, rows):
    """simplified grass ∪ rocks ∪ DB base → keep the largest body.
    Returns (trimesh, peg height, (crumbs dropped, their volume), zmax)."""
    from dharmatiles.core.config import BaseConfig, SurfaceConfig
    from dharmatiles.bases import dungeonblocks as db

    top = grass.simplify(gl.SIMPLIFY_TOL_MM)
    if rocks is not None:
        top = top + rocks
    zmax = top.bounding_box()[5]
    surface = SurfaceConfig(cols=cols, rows=rows, square_mm=gl.SQUARE_MM)
    base_cfg = BaseConfig()
    peg_h = db.select_peg_height(np.zeros((2, 2)), base_cfg, zmax)
    solid = top + gl.to_manifold(db.make_base(surface, peg_h, base_cfg))
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
    rng = np.random.default_rng(args.seed)                      # grass stream
    rrng = np.random.default_rng(args.seed + ROCK_SEED_OFFSET)  # rock stream
    W, H = args.cols * gl.SQUARE_MM, args.rows * gl.SQUARE_MM
    grid = gl.Grid(W, H, W / round(W / gl.VOX_MM))

    ground, gstats = gl.make_ground(rng, W, H)
    flow0 = gl.make_direction(rng, ground)
    sw = gl.Swaths(rng, W, H, flow0)
    posed = rl.pose(rrng, rl.scatter(rrng, W, H, ROCK_PLACEMENTS_2X2))   # rocks first
    foot = footprint_mask(posed, grid)
    free = gl.place_lenses(rng, ground, flow0, W, H, FREE_LENS_POOL_PER_SQ)
    aprons = apron_shapes(rrng, flow0, posed)
    soil_l, n_free = fill_bare_earth(free, aprons, foot, grid, BARE_EARTH_FRAC)
    # Flow wraps rocks (circles) and soil lenses alike.
    flow = gl.finish_flow(rng, flow0, combine(soil_l, None, rock_circles(flow0, posed)), W, H)

    g = gl.build_grass(rng, grid, ground, sw, soil_l, flow, W, H, blocker=foot, tuck_mm=TUCK_MM)
    F = gl.make_field(grid, g.T, g.lips)
    added, removed = gl.fdm_backstop(F, grid.vox)
    grass, _ = gl.mesh_field(F, grid.vox)
    rocks = seat_rocks(posed, g.H0, foot, grid)
    mesh, peg_h, crumbs, zmax = attach_base(grass, rocks, args.cols, args.rows)
    bare = float(((g.soil > 0.5) | foot).mean())

    out = pathlib.Path(__file__).parent / 'out'
    out.mkdir(exist_ok=True)
    path = out / f"{args.cols}x{args.rows}-meadow-s{args.seed}.stl"
    mesh.export(path)
    n_stones = sum(len(p.stones) for p in posed)
    print(f"→ {path}  ({path.stat().st_size / 1e6:.1f} MB)")
    print(f"  mesh: {len(mesh.vertices):,} verts, {len(mesh.faces):,} faces, "
          f"watertight={mesh.is_watertight}, peg={peg_h} mm, "
          f"crumbs dropped {crumbs[0]} ({crumbs[1]:.3f} mm³)")
    print(f"  rocks: {len(posed)} placements, {n_stones} stones, footprint {foot.mean():.1%} of tile; "
          f"{len(aprons[0])} with bare-earth aprons")
    print(f"  bare earth incl. under rocks: {bare:.1%} (target {BARE_EARTH_FRAC:.0%}); "
          f"visible {float(((g.soil > 0.5) & ~foot).mean()):.1%}; {n_free} free lenses")
    print(f"  {g.n_fronds} fronds in {g.n_strands} strands; ground {gstats['ground_min']:.2f}–"
          f"{gstats['ground_max']:.2f} mm; top z {zmax:.2f} mm")
    print(f"  FDM backstop +{added:,}/−{removed:,}; overhang {gl.audit_overhang(mesh)[0]:.1f} mm²; "
          f"total {time.perf_counter() - t0:.1f}s")
    if args.check:
        gl.self_check(mesh, args.check, W, H)
        print(f"  self-check → {args.check}")


if __name__ == '__main__':
    main()
