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
from scipy import ndimage

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
SIDE_LOW            = 0.5           # grass height × this at a rock's face…
SIDE_MM             = 1.2           # …back to full height this far out
FLOW_RING_MM        = 0.6           # flow wraps a circle this much wider than the rock
ROCK_SINK_MM        = 0.3           # base below the seat ground height; plinth below the lowest
ROCK_SEAT_Q         = 0.35          # seat ground height = this quantile of H0 under the footprint
ROCK_RING_MM        = 1.5           # grass-top ring just outside the footprint
ROCK_RING_Q         = 0.75          # its grass height (quantile of T)
ROCK_SHOW_MM        = 1.5           # a rock's top stands at least this far above that grass…
ROCK_SHOW_FRAC      = 0.45          # …or this fraction of its height, whichever is more
ROCK_RAISE_MAX_MM   = 1.5           # cap on the lift that ensures it
PLINTH_TOP_MM       = 0.1           # plinth top, inside the rock's base


def footprint_masks(posed, grid):
    """Per-rock bool grids of the real outlines where the rocks meet the ground."""
    from matplotlib.path import Path
    X, Y = np.meshgrid(grid.xc, grid.yc, indexing='ij')
    pts = np.stack([X.ravel(), Y.ravel()], 1)
    masks = []
    for p in posed:
        mask = np.zeros(X.shape, bool)
        sel = (np.abs(X - p.x) <= p.r + 1) & (np.abs(Y - p.y) <= p.r + 1)
        idx = np.nonzero(sel.ravel())[0]
        for poly in p.solid.slice(FOOTPRINT_Z_MM).to_polygons():
            poly = np.asarray(poly) + [p.x, p.y]
            if len(poly) >= 3:
                hit = Path(poly).contains_points(pts[idx])
                mask.ravel()[idx[hit]] = True
        masks.append(mask)
    return masks


def footprint_mask(masks, shape):
    """Union of the per-rock footprints."""
    return np.logical_or.reduce(masks) if masks else np.zeros(shape, bool)


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


def seat_rocks(posed, masks, H0, T, grid):
    """Seat each rock so it reads as a rock bedded in grass (REQ-M-2, M5).

    - Seat: base at a low quantile of the ground under the footprint,
      minus ROCK_SINK_MM, not the minimum (which buried the uphill side of
      rocks on slopes).
    - Emerge: if the top stands less than max(ROCK_SHOW_MM, ROCK_SHOW_FRAC ×
      height) above the surrounding grass top (a ring just outside the
      footprint), raise it, by at most ROCK_RAISE_MAX_MM.
    - Bed: a plinth (the rock's own base section, extruded straight down)
      reaches ROCK_SINK_MM below the LOWEST ground under the footprint, so
      no gap opens on the downhill side.  Where it shows, it reads as the
      rock's own lower face.
    Returns (rocks Manifold, per-rock report dicts).
    """
    out, rep = [], []
    vox = grid.vox
    ring_px = max(int(round(ROCK_RING_MM / vox)), 1)
    for p, m in zip(posed, masks):
        if not m.any():                          # degenerate: fall back to the centre cell
            i, j = min(int(p.x / vox), grid.nx - 1), min(int(p.y / vox), grid.ny - 1)
            m = np.zeros_like(m)
            m[i, j] = True
        g = H0[m]
        ring = ndimage.binary_dilation(m, iterations=ring_px) & ~m
        grass_top = float(np.quantile(T[ring], ROCK_RING_Q)) if ring.any() else float(g.max())
        h = p.solid.bounding_box()[5]
        z0 = float(np.quantile(g, ROCK_SEAT_Q)) - ROCK_SINK_MM
        need = max(ROCK_SHOW_MM, ROCK_SHOW_FRAC * h) - (z0 + h - grass_top)
        raise_mm = float(np.clip(need, 0.0, ROCK_RAISE_MAX_MM))
        z0 += raise_mm
        depth = z0 - (float(g.min()) - ROCK_SINK_MM)
        rock = p.solid
        if depth > 0:
            sec = p.solid.slice(PLINTH_TOP_MM + 0.05).simplify(0.005)
            plinth = m3d.Manifold.extrude(sec, depth + PLINTH_TOP_MM).translate([0, 0, -depth])
            rock = rock + plinth
        out.append(rock.translate([p.x, p.y, z0]))
        rep.append(dict(show=z0 + h - grass_top, h=h, raised=raise_mm, plinth=max(depth, 0.0)))
    rocks = m3d.Manifold.batch_boolean(out, m3d.OpType.Add) if out else None
    return rocks, rep


def attach_base(grass, rocks, cols, rows):
    """simplified grass ∪ rocks ∪ DB base → keep the largest body.
    Returns (trimesh, peg height, (crumbs dropped, their volume), zmax)."""
    from dharmatiles.core.config import BaseConfig, SurfaceConfig
    from dharmatiles.bases import dungeonblocks as db

    W, H = cols * gl.SQUARE_MM, rows * gl.SQUARE_MM
    slab = m3d.Manifold.cube([W, H, gl.SLAB_BOX_MM])      # clean face for the base
    top = (grass + slab).simplify(gl.SIMPLIFY_TOL_MM)
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
    masks = footprint_masks(posed, grid)
    foot = footprint_mask(masks, (grid.nx, grid.ny))
    free = gl.place_lenses(rng, ground, flow0, W, H, FREE_LENS_POOL_PER_SQ)
    aprons = apron_shapes(rrng, flow0, posed)
    soil_l, n_free = fill_bare_earth(free, aprons, foot, grid, BARE_EARTH_FRAC)
    # Flow wraps rocks (circles) and soil lenses alike.
    flow = gl.finish_flow(rng, flow0, combine(soil_l, None, rock_circles(flow0, posed)), W, H)

    g = gl.build_grass(rng, grid, ground, sw, soil_l, flow, W, H, blocker=foot, tuck_mm=TUCK_MM,
                       side_low=SIDE_LOW, side_mm=SIDE_MM)
    F = gl.make_field(grid, g.T, g.lips)
    added, removed = gl.fdm_backstop(F, grid.vox)
    grass, _ = gl.mesh_field(F, grid.vox)
    rocks, seats = seat_rocks(posed, masks, g.H0, g.T, grid)
    mesh, peg_h, crumbs, zmax = attach_base(grass, rocks, args.cols, args.rows)
    bare = float(((g.soil > 0.5) | foot).mean())

    out = pathlib.Path(__file__).parent / 'out'
    out.mkdir(exist_ok=True)
    path = out / f"{args.cols}x{args.rows}-meadow-s{args.seed}.stl"
    mesh = gl.stl_safe(mesh)
    mesh.export(path)
    n_stones = sum(len(p.stones) for p in posed)
    print(f"→ {path}  ({path.stat().st_size / 1e6:.1f} MB)")
    print(f"  mesh: {len(mesh.vertices):,} verts, {len(mesh.faces):,} faces, "
          f"watertight={mesh.is_watertight}, peg={peg_h} mm, "
          f"crumbs dropped {crumbs[0]} ({crumbs[1]:.3f} mm³)")
    print(f"  rocks: {len(posed)} placements, {n_stones} stones, footprint {foot.mean():.1%} of tile; "
          f"{len(aprons[0])} with bare-earth aprons")
    print("  rock tops above grass: " + ", ".join(
        f"{q['show']:.1f}/{q['h']:.1f}" + (f"↑{q['raised']:.1f}" if q['raised'] > 0 else '')
        for q in seats) + f" mm (show/height, ↑lift); deepest plinth {max(q['plinth'] for q in seats):.1f} mm")
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
