#!/usr/bin/env python3
"""A few grass blades on a small flat slab, for close inspection.

Builds the N blades rooted nearest the centre with grasslab's own blade
builder (blade_layers → blade_mesh: print-native strokes, smoothed), on flat
ground with no dirt, so each blade's shape can be examined on its own.

--strokes writes the unsmoothed stroke stack instead: every LAYER_MM layer a
round-ended stroke a whole number of LINE_W_MM lines wide — exactly what an
FDM slicer would draw.

Usage:
    python grasslab/blades_demo.py [--n 3] [--seed 1] [--size 16] [--strokes]
"""
from __future__ import annotations

import argparse
import pathlib
import sys

import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'grasslab'))

SLAB_MM = 2.0           # slab under the blades (a whole number of layers)


def stroke_stack(layers, m3d, layer_mm):
    """The unsmoothed layer stack of one blade, standing on z = SLAB_MM."""
    from shapely.geometry.polygon import orient
    out = []
    for k, poly in layers:
        for p in getattr(poly, 'geoms', [poly]):
            p = orient(p, 1.0)                      # CCW outer ring, CW holes
            cs = m3d.CrossSection([np.asarray(p.exterior.coords)[:-1]]
                                  + [np.asarray(r.coords)[:-1] for r in p.interiors])
            # +0.01 overlap so stacked layers fuse (no coplanar seams)
            out.append(m3d.Manifold.extrude(cs, layer_mm + 0.01).translate(
                [0, 0, SLAB_MM + k * layer_mm - 0.005]))
    return out


def printed_fraction(layers, line_w):
    """Area kept when each layer is opened by (just under) half a line — the
    slicer's view of what is wide enough to extrude."""
    r = 0.95 * line_w / 2
    a = sum(p.area for _, p in layers)
    kept = sum(p.buffer(-r).buffer(r).area for _, p in layers)
    return kept / max(a, 1e-9)


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--n', type=int, default=3)
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--size', type=float, default=16.0)
    ap.add_argument('--strokes', action='store_true')
    args = ap.parse_args()
    import grasslab as gl
    import manifold3d as m3d
    import trimesh

    W = H = args.size
    rng = np.random.default_rng(args.seed)
    grid = gl.Grid(W, H, W / round(W / gl.VOX_MM))
    ground, _ = gl.make_ground(rng, W, H, flat=True)
    flow0 = gl.make_direction(rng, ground)
    sw = gl.Swaths(rng, W, H, flow0)
    lenses = gl.place_lenses(rng, ground, flow0, W, H, (0.0, 0.0))
    lenses = gl.Lenses(lenses.centre[:0], lenses.axis[:0], lenses.half_len[:0],
                       lenses.half_w[:0], lenses.warp)
    flow = gl.finish_flow(rng, flow0, lenses, W, H)
    surf = gl.Surface(rng, grid, ground, lenses)
    surf.seat = lambda x, y: np.full(np.shape(x), SLAB_MM)     # flat slab, no dirt
    plan = gl.plan_fronds(rng, surf, sw, flow, W, H)
    frs = [gl.draw_frond(rng, plan, f, surf, W, H) for f in range(len(plan.roots))]
    frs = [fr for fr in frs if fr is not None and not fr.sprig]
    c = np.array([W / 2, H / 2])
    frs.sort(key=lambda fr: np.hypot(*(plan.roots[fr.f] - c)))

    parts, stats = [m3d.Manifold.cube([W, H, SLAB_MM])], []
    for fr in frs[:args.n]:
        mesh, layers = gl.build_blade(fr, plan, surf)
        if args.strokes:
            parts += stroke_stack(layers, m3d, gl.LAYER_MM)
        elif mesh is not None:
            parts.append(gl.to_manifold(mesh))
        stats.append(f"L={fr.L:.1f} {len(layers)} layers, "
                     f"printed {printed_fraction(layers, gl.LINE_W_MM):.1%}")
    solid = m3d.Manifold.batch_boolean(parts, m3d.OpType.Add)
    o = solid.to_mesh()
    mesh = trimesh.Trimesh(np.asarray(o.vert_properties)[:, :3], np.asarray(o.tri_verts))
    # drop 2-triangle slivers left where stroke layers meet
    mesh = trimesh.util.concatenate([b for b in mesh.split(only_watertight=False)
                                     if len(b.faces) >= 4])
    out = ROOT / 'grasslab' / 'out'
    out.mkdir(exist_ok=True)
    kind = 'strokes' if args.strokes else 'smooth'
    path = out / f"blades-{kind}-{args.n}-s{args.seed}.stl"
    mesh.export(path)
    print(f"→ {path}  {len(mesh.faces):,} faces, watertight={mesh.is_watertight}; "
          + "; ".join(stats))


if __name__ == '__main__':
    main()
