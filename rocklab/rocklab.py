#!/usr/bin/env python3
"""rocklab v1 — chiselled faceted rocks scattered on a flat DungeonBlocks tile.

Implements rocklab/DESIGN.md:

  per rock: smooth convex body on a non-increasing profile → blue-noise
            facet seeds (mixed radii) → intersection of tilted tangent
            half-spaces → convex, grounded, printable
  clusters: union of 2–4 rocks
  scatter:  dart throwing with clearance, rim inset
  tile:     flat slab at 4.0 mm ∪ rocks ∪ DB socket base → STL

Usage:
    python rocklab/rocklab.py                 # 2x2, seed 1
    python rocklab/rocklab.py --seed 4 --check out.png
"""
from __future__ import annotations

import argparse
import pathlib
import time

import numpy as np

SQUARE_MM        = 35.0
SURFACE_MM       = 4.0        # flat floor, above terrain datum (DB dirt band)
SINK_MM          = 0.3

WIDTH_MM         = (6.0, 15.0)
WIDTH_EXP        = 1.4        # skew toward small
ASPECT           = (1.0, 2.0)
HEIGHT_RATIO     = (0.45, 0.85)
HEIGHT_CAP_MM    = 9.0
FAMILIES         = {'dome': 0.35, 'slab': 0.2, 'block': 0.2, 'wedge': 0.25}

BASE_PTS         = (14, 22)
BODY_PTS         = (60, 120)    # smooth-ish body the facets are seeded on
TOP_PTS          = (1, 3)

FACET_R_MM       = (0.9, 2.5)
LUMP_AMP         = 0.08           # body radius lumpiness (± fraction, angle-only)     # facet seed spacing at the largest width (mixed sizes)
FACET_R_EXP      = 1.6            # skew toward small facets
FACET_TILT_DEG   = 14.0           # random tilt of each facet plane off the tangent
FACET_OFFSET_MM  = (-0.15, 0.1)   # plane offset jitter along its normal
FACET_MIN_NZ     = -0.5           # facets never point down past 30°

# scale 2: chipped corners and edges (planar, convex)
CORNER_CHIP_FRAC = 0.45
MAX_CHIPS        = (8, 16)        # per rock (more corners must not erode it)
CORNER_CHIP_MM   = (0.3, 0.9)     # × √(D/15)
EDGE_CHIP_FRAC   = 0.35
EDGE_CHIP_MIN_DEG = 18.0          # only edges sharper than this get chips
EDGE_CHIP_MM     = (0.2, 0.55)
CHIP_TILT_DEG    = 25.0

# scale 3: fine grain (displacement); fracture scallops removed (Shawn: "horrible")
TEX_EDGE_MM      = 0.25
GRAIN_MM         = 0.45
GRAIN_AMP_MM     = 0.12
TEX_BASE_FADE_MM = 0.8
EDGE_KEEP_MM     = 0.5            # grain is halved this close to sharp edges

CLUSTER_PROB     = 0.25
CLUSTER_STONES   = (2, 4)
CLUSTER_SIZE     = (0.45, 0.9)
MIN_STONE_MM     = 4.0
CLUSTER_DIST     = (0.35, 0.6)

PLACEMENTS_2X2   = (9, 12)
CLEARANCE_MM     = 1.5
RIM_MM           = 1.0
OVERHANG_DEG     = 45.0


def _m3d():
    import manifold3d as m3d
    return m3d


def profile(family, t):
    t = np.clip(t, 0, 1)
    if family == 'dome':
        return np.sqrt(np.clip(1 - t ** 2.2, 0, 1))
    if family == 'slab':
        return np.clip(1 - t ** 2.8, 0, 1) ** 0.45
    if family == 'block':
        return np.clip(1 - t ** 3.5, 0, 1) ** 0.4
    return np.clip(1 - t, 0, 1) ** 0.8          # wedge


def footprint_radius(ang, a, b, family):
    """Ellipse radius at angle; 'block' squares the footprint (superellipse)."""
    p = 3.0 if family == 'block' else 2.0
    c, s = np.abs(np.cos(ang)), np.abs(np.sin(ang))
    return 1.0 / ((c / a) ** p + (s / b) ** p) ** (1 / p)


def make_rock(rng, D, family=None):
    """One chiselled convex rock, base at z = 0.  Returns (Manifold, info)."""
    m3d = _m3d()
    if family is None:
        fam = list(FAMILIES)
        family = rng.choice(fam, p=np.array([FAMILIES[f] for f in fam]))
    a = D / 2
    b = a / rng.uniform(*ASPECT)
    h = min(D * rng.uniform(*HEIGHT_RATIO), HEIGHT_CAP_MM)
    if family == 'slab':
        h *= 0.6
    h = max(h, 2.5)
    yaw = rng.uniform(0, np.pi)
    # wedge apex offset direction/amount (fraction of the footprint)
    wd = rng.uniform(0, 2 * np.pi)
    wo = rng.uniform(0.2, 0.45) if family == 'wedge' else 0.0

    pts = []
    # Gentle lumpiness: a smooth function of angle only, so every vertical
    # line is scaled by a constant (profile stays non-increasing) while
    # neighbouring tangent planes always differ — no flat body regions,
    # hence no oversized facets.
    k = np.arange(2, 6)
    la = rng.normal(0, 1, len(k)) * LUMP_AMP / np.sqrt(len(k))
    lp = rng.uniform(0, 2 * np.pi, len(k))

    def lump(ang):
        return 1.0 + float(np.sum(la * np.cos(k * ang + lp)))

    def add(t, ang, shrink):
        r = footprint_radius(ang - yaw, a, b, family) * profile(family, t) * shrink * lump(ang)
        cx = wo * t * a * np.cos(wd)
        cy = wo * t * b * np.sin(wd)
        pts.append([cx + r * np.cos(ang), cy + r * np.sin(ang), t * h])

    for ang in np.sort(rng.uniform(0, 2 * np.pi, rng.integers(*BASE_PTS, endpoint=True))):
        add(0.0, ang, rng.uniform(0.94, 1.0))
    for _ in range(rng.integers(*BODY_PTS, endpoint=True)):
        add(rng.uniform(0.05, 0.97), rng.uniform(0, 2 * np.pi), rng.uniform(0.93, 1.0))
    for _ in range(rng.integers(*TOP_PTS, endpoint=True)):
        add(rng.uniform(0.95, 1.0), rng.uniform(0, 2 * np.pi), rng.uniform(0.0, 0.4))
    body = m3d.Manifold.hull_points(np.array(pts, dtype=np.float32))
    bm = to_trimesh(body)

    # Facets: blue-noise seeds with MIXED radii on the smooth body; each
    # seed's tilted tangent plane is one facet; the rock is the intersection
    # of those half-spaces (convex, one facet per seed, sizes from spacing).
    scale = np.sqrt(D / WIDTH_MM[1])
    rmin, rmax = FACET_R_MM[0] * scale, FACET_R_MM[1] * scale
    cand, fi = bm.sample(4000, return_index=True)
    nrm = bm.face_normals[fi]
    seeds, radii, normals = [], [], []
    for p_, n_ in zip(cand, nrm):
        if n_[2] < -0.3:                     # base region: the flat base is the facet
            continue
        r_ = rmin + (rmax - rmin) * rng.random() ** FACET_R_EXP
        if all(np.linalg.norm(p_ - q) >= 0.5 * (r_ + rq) for q, rq in zip(seeds, radii)):
            seeds.append(p_); radii.append(r_); normals.append(n_)
    lo, hi = body.bounding_box()[:3], body.bounding_box()[3:]
    pad = 1.0
    rock = m3d.Manifold.cube([hi[0] - lo[0] + 2 * pad, hi[1] - lo[1] + 2 * pad,
                              hi[2] + pad]).translate([lo[0] - pad, lo[1] - pad, 0.0])
    done = 0
    for p_, n_ in zip(seeds, normals):
        t = rng.normal(size=3)
        t -= (t @ n_) * n_
        t /= max(np.linalg.norm(t), 1e-9)
        tilt = np.radians(rng.uniform(0, FACET_TILT_DEG))
        n2 = np.cos(tilt) * n_ + np.sin(tilt) * t
        if n2[2] < FACET_MIN_NZ:             # never a downward face past 30°
            continue
        n2 /= np.linalg.norm(n2)
        d = float(n2 @ p_) + rng.uniform(*FACET_OFFSET_MM)
        cut = rock.trim_by_plane((-n2).tolist(), -d)
        if cut.is_empty():
            continue
        rock = cut
        done += 1
    vol0 = body.volume()
    rock, n_chips = chip_corners_edges(rng, rock, D)
    rock = texture(rng, rock, D)
    info = dict(family=str(family), D=D, h=h, chips=done, corner_chips=n_chips,
                faces=rock.num_tri(), vol_frac=rock.volume() / vol0)
    return rock, info


def chip_corners_edges(rng, rock, D):
    """Scale 2: small planar chips at polytope corners and along sharp edges.

    Plane normals are the corner/edge normals (mean of the adjacent facet
    normals) jittered; still half-space cuts, so the rock stays convex,
    grounded and printable (n_z ≥ FACET_MIN_NZ).
    """
    tm = to_trimesh(rock)
    fn = tm.face_normals
    scale = np.sqrt(D / WIDTH_MM[1])
    planes = []
    # corners: vertices where facets of clearly different normals meet
    vn = np.zeros((len(tm.vertices), 3))
    for k in range(3):
        np.add.at(vn, tm.faces[:, k], fn)
    for vi in rng.permutation(len(tm.vertices)):
        if rng.random() > CORNER_CHIP_FRAC or tm.vertices[vi, 2] < 0.6:
            continue
        n = vn[vi] / max(np.linalg.norm(vn[vi]), 1e-9)
        planes.append((tm.vertices[vi], n, rng.uniform(*CORNER_CHIP_MM) * scale))
    # edges: sharp facet edges get bevel chips along part of their length
    ang = tm.face_adjacency_angles
    for e in np.nonzero(ang > np.radians(EDGE_CHIP_MIN_DEG))[0]:
        if rng.random() > EDGE_CHIP_FRAC:
            continue
        a, b = tm.face_adjacency_edges[e]
        pa, pb = tm.vertices[a], tm.vertices[b]
        if min(pa[2], pb[2]) < 0.6:
            continue
        t = rng.uniform(0.2, 0.8)
        n = fn[tm.face_adjacency[e]].sum(0)
        n /= max(np.linalg.norm(n), 1e-9)
        planes.append((pa + t * (pb - pa), n, rng.uniform(*EDGE_CHIP_MM) * scale))
    done = 0
    cap = int(rng.integers(*MAX_CHIPS, endpoint=True))
    order = rng.permutation(len(planes))
    planes = [planes[i] for i in order]
    for p_, n, dep in planes:
        if done >= cap:
            break
        j = rng.normal(size=3)
        j -= (j @ n) * n
        j /= max(np.linalg.norm(j), 1e-9)
        tl = np.radians(rng.uniform(0, CHIP_TILT_DEG))
        n2 = np.cos(tl) * n + np.sin(tl) * j
        n2 /= np.linalg.norm(n2)
        if n2[2] < FACET_MIN_NZ:
            continue
        cut = rock.trim_by_plane((-n2).tolist(), -(float(n2 @ p_) - dep))
        if cut.is_empty() or cut.volume() < 0.97 * rock.volume():
            continue
        rock = cut
        done += 1
    return rock, done


def texture(rng, rock, D):
    """Scale 3: fine grain (~0.45 mm, ±0.12 mm) along vertex normals.

    Faded to zero near the base so the footprint stays seated, and halved
    near sharp edges so facet and chip ridges stay crisp.
    """
    import trimesh
    from dharmatiles.stone.noise import fbm
    m3d = _m3d()
    # Sharp edges of the faceted rock (facets + chips) stay crisp ridges.
    base_tm = to_trimesh(rock)
    sharp = base_tm.face_adjacency_angles > np.radians(EDGE_CHIP_MIN_DEG)
    E = base_tm.vertices[base_tm.face_adjacency_edges[sharp]]       # (k, 2, 3)
    fine = rock.refine_to_length(TEX_EDGE_MM)        # stays manifold (trimesh's
    out0 = fine.to_mesh()                            # subdivide left T-junctions)
    tm = trimesh.Trimesh(np.asarray(out0.vert_properties)[:, :3],
                         np.asarray(out0.tri_verts), process=False)
    P = tm.vertices.copy()
    N = tm.vertex_normals
    seed = int(rng.integers(1 << 30))
    gr = fbm(P, seed ^ 0x5A5A, GRAIN_MM, octaves=4)
    if len(E):
        A, B = E[:, 0], E[:, 1]
        AB = B - A
        L2 = np.maximum((AB * AB).sum(1), 1e-12)
        dmin = np.full(len(P), np.inf)
        for c0 in range(0, len(P), 4000):
            Q = P[c0:c0 + 4000, None, :]
            t = np.clip(((Q - A) * AB).sum(2) / L2, 0, 1)
            d = np.linalg.norm(Q - (A + t[..., None] * AB), axis=2).min(1)
            dmin[c0:c0 + 4000] = d
        we = np.clip(dmin / EDGE_KEEP_MM, 0, 1)
        we = we * we * (3 - 2 * we)
    else:
        we = np.ones(len(P))
    # Grain is halved on sharp edges so facet/chip ridges stay crisp.
    disp = GRAIN_AMP_MM * gr * (0.5 + 0.5 * we)
    w = np.clip((P[:, 2] - 0.15) / TEX_BASE_FADE_MM, 0, 1)
    w = w * w * (3 - 2 * w) * (N[:, 2] > -0.3)
    V = P + (disp * w)[:, None] * N
    out = m3d.Manifold(mesh=m3d.Mesh(
        vert_properties=np.asarray(V, dtype=np.float32),
        tri_verts=np.asarray(tm.faces, dtype=np.uint32)))
    if out.is_empty():
        return rock
    return out.trim_by_plane([0, 0, 1], 0.0)


def make_item(rng):
    """A single rock or a cluster.  Returns (Manifold, radius, [infos])."""
    m3d = _m3d()
    D = WIDTH_MM[0] + (WIDTH_MM[1] - WIDTH_MM[0]) * rng.random() ** WIDTH_EXP
    if rng.random() >= CLUSTER_PROB:
        r, info = make_rock(rng, D)
        return r, D / 2, [info]
    k = int(rng.integers(*CLUSTER_STONES, endpoint=True))
    main, info = make_rock(rng, D)
    parts, infos, R = [main], [info], D / 2
    ang0 = rng.uniform(0, 2 * np.pi)
    for j in range(1, k):
        d = max(D * rng.uniform(*CLUSTER_SIZE), MIN_STONE_MM)
        r, inf = make_rock(rng, d)
        ang = ang0 + j * 2 * np.pi / k + rng.uniform(-0.5, 0.5)
        dist = (D / 2 + d / 2) * rng.uniform(*CLUSTER_DIST)
        off = [dist * np.cos(ang), dist * np.sin(ang), 0.0]
        parts.append(r.translate(off))
        infos.append(inf)
        R = max(R, dist + d / 2)
    return m3d.Manifold.batch_boolean(parts, m3d.OpType.Add), R, infos


def scatter(rng, W, H):
    n_sq = (W / SQUARE_MM) * (H / SQUARE_MM)
    n = int(round(rng.uniform(*PLACEMENTS_2X2) * n_sq / 4))
    items = sorted((make_item(rng) for _ in range(n)), key=lambda it: -it[1])
    placed = []
    for man, R, infos in items:
        for _ in range(400):
            x = rng.uniform(RIM_MM + R, W - RIM_MM - R)
            y = rng.uniform(RIM_MM + R, H - RIM_MM - R)
            if all(np.hypot(x - px, y - py) >= CLEARANCE_MM + R + pr for px, py, pr, *_ in placed):
                placed.append((x, y, R, man, infos))
                break
    return placed


def build_tile(rng, cols, rows):
    m3d = _m3d()
    from dharmatiles.core.config import BaseConfig, SurfaceConfig
    from dharmatiles.bases import dungeonblocks as db
    W, H = cols * SQUARE_MM, rows * SQUARE_MM
    placed = scatter(rng, W, H)
    slab = m3d.Manifold.cube([W, H, SURFACE_MM])
    rocks = []
    for x, y, R, man, infos in placed:
        rot = man.rotate([0.0, 0.0, float(rng.uniform(0, 360))])
        rocks.append(rot.translate([x, y, SURFACE_MM - SINK_MM]))
    top = m3d.Manifold.batch_boolean([slab] + rocks, m3d.OpType.Add)
    zmax = top.bounding_box()[5]
    surface = SurfaceConfig(cols=cols, rows=rows, square_mm=SQUARE_MM)
    base_cfg = BaseConfig()
    peg_h = db.select_peg_height(np.zeros((2, 2)), base_cfg, zmax)
    base = db.make_base(surface, peg_h, base_cfg)
    bm = m3d.Manifold(mesh=m3d.Mesh(
        vert_properties=np.asarray(base.vertices, dtype=np.float32),
        tri_verts=np.asarray(base.faces, dtype=np.uint32)))
    solid = top + bm
    return solid, placed, peg_h, zmax


def to_trimesh(man):
    import trimesh
    out = man.to_mesh()
    return trimesh.Trimesh(np.asarray(out.vert_properties)[:, :3],
                           np.asarray(out.tri_verts), process=True)


def audit_overhang(mesh):
    n = mesh.face_normals
    cz = mesh.triangles_center[:, 2]
    bad = (n[:, 2] < -np.sin(np.radians(OVERHANG_DEG + 3.0))) & (cz > 0.05)
    return float(mesh.area_faces[bad].sum())


def self_check(mesh, path):
    from dharmatiles.render import render
    m = mesh.copy()
    m.visual.face_colors = np.tile([150, 150, 150, 255], (len(m.faces), 1))
    render([m], path, elev=35, azim=-125, resolution=(1400, 1100), quiet=True, smooth=False)
    s = m.slice_plane([0, 0, 0.5], [0, 0, 1], cap=False)
    render([s], path.with_name(path.stem + '-low.png'), elev=18, azim=-125,
           resolution=(1400, 900), quiet=True, smooth=False)


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--cols', type=int, default=2)
    ap.add_argument('--rows', type=int, default=2)
    ap.add_argument('--check', type=pathlib.Path, default=None)
    args = ap.parse_args()
    t0 = time.perf_counter()
    rng = np.random.default_rng(args.seed)
    solid, placed, peg_h, zmax = build_tile(rng, args.cols, args.rows)
    mesh = to_trimesh(solid)
    out = pathlib.Path(__file__).parent / 'out'
    out.mkdir(exist_ok=True)
    path = out / f"{args.cols}x{args.rows}-rocks-s{args.seed}.stl"
    mesh.export(path)
    infos = [i for *_, inf in placed for i in inf]
    fams = {f: sum(i['family'] == f for i in infos) for f in FAMILIES}
    n_cl = sum(len(inf) > 1 for *_, inf in placed)
    print(f"→ {path}  ({path.stat().st_size / 1e6:.1f} MB)")
    print(f"  mesh: {len(mesh.vertices):,} verts, {len(mesh.faces):,} faces, "
          f"watertight={mesh.is_watertight}, bodies={len(solid.decompose())}, peg={peg_h} mm")
    print(f"  {len(placed)} placements ({n_cl} clusters), {len(infos)} stones; families {fams}")
    print(f"  stone width {min(i['D'] for i in infos):.1f}–{max(i['D'] for i in infos):.1f} mm, "
          f"facets {min(i['chips'] for i in infos)}–{max(i['chips'] for i in infos)}, "
          f"top z {zmax:.2f} mm above datum")
    print(f"  overhang >{OVERHANG_DEG:.0f}°(+3): {audit_overhang(mesh):.2f} mm²; "
          f"total {time.perf_counter() - t0:.1f}s")
    if args.check:
        self_check(mesh, args.check)
        print(f"  self-check → {args.check}")


if __name__ == '__main__':
    main()
