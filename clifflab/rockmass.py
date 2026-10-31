#!/usr/bin/env python3
"""rockmass — a constrained volume filled with stacked, chiselled rock masses.

The cliff is not a textured surface but a pile of individual rocks (the
reference sheet: blocky masses stacked regularly and irregularly):

  1 VOLUME: any union of footprint prisms (shapely polygon × [z0, z1]).
  2 CELLS: recursive splits of the volume's box by slightly tilted planes —
    mostly vertical joints (columns), some horizontal (courses) — until
    each cell is near its own drawn target size; cells ∩ volume.
  3 ROCKS: each surface cell becomes a rock that fills the whole cell, so
    every rock sits directly on the one beneath and against its neighbours
    (no gaps; joints are only the chamfered seams).  It steps out along the
    local outward normal (coherent within a column) and is swept back to the
    cell, a 45° support cut trims it where it stands proud of the rock
    beneath, then gentle chisel facets, corner/edge chips and grain.  Cells
    deep inside are left to the core.
  4 ASSEMBLY: rocks ∪ core (the volume shrunk back from the surface, so
    joints are blind) ∪ dirt cap on top ∪ dirt ground ∪ scree ∪ DB base.

Usage:
    python clifflab/rockmass.py [--form run] [--seed 1] [--cols 1 --rows 1]
"""
from __future__ import annotations

import argparse
import pathlib
import sys
import time
from dataclasses import dataclass

import manifold3d as m3d
import numpy as np
import shapely
import trimesh
from shapely.geometry import LineString, Polygon, box

ROOT = pathlib.Path(__file__).resolve().parent.parent
for _lab in ('dirtlab', 'grasslab', 'rocklab', 'clifflab'):
    sys.path.insert(0, str(ROOT / _lab))
import dirtlab as _dirt     # noqa: E402
import grasslab as _gl      # noqa: E402
import rocklab as _rl       # noqa: E402

# ── Parameters ──────────────────────────────────────────────────────────────
SQUARE_MM        = 35.0
FOOT_Z_MM        = 4.0         # foot ground (DB dirt band)
CLIFF_H_MM       = 35.0
EDGE_T_MM        = 13.0        # edge slab depth
WALL_T_MM        = 12.0

# 2 cells
TARGET_W_MM      = (7.0, 22.0) # horizontal target sizes (skewed small)
TARGET_W_EXP     = 1.6
TARGET_H_MM      = (10.0, 24.0) # vertical target sizes: blocks taller than wide
TARGET_H_EXP     = 1.3
TARGET_D_MM      = 4.5         # depth: thicker cells are split front/back
SPLIT_POS        = (0.32, 0.68)
JOINT_TILT_DEG   = 14.0         # vertical joints lean ± this
COURSE_TILT_DEG  = 10.0         # horizontal joints ± this
MIN_PART         = 0.45        # no split leaves a part thinner than this × its target
CORE_DEPTH_MM    = 5.5         # cells farther than this from the surface: core only

# 3 rocks
PROUD_MM         = (-0.8, 4.5) # displacement along the outward normal…
PROUD_FIELD_MM   = 9.0         # …coherent over this (columns, buttresses)
PROUD_JITTER_MM  = 0.9
CONTACT_BACK_MM  = 0.6         # contact rocks sit back by about their bulge (peaks touch)
CONTACT_MM       = 0.8         # surface within this of the tile edge = contact
FACET_R_MM       = (2.5, 6.0)  # rock facets (rocklab radii, unscaled)
FACET_TILT_DEG   = 6.0        # chisel planes tilt this far off the cell faces
FACET_OFFSET_MM  = (-0.1, 0.05)
TOP_DROP_MM      = (0.0, 4.0)  # top-course rocks lowered by up to this
TOP_DROP_P       = 0.45


# 4 assembly
CORE_INSET_MM    = 1.8
CORE_EDGE_MM     = 1.2         # core held back from the tile edges (contact rocks show)
CAP_INSET_MM     = 1.2         # dirt cap held back from exposed faces
CAP_DROP_MM      = 0.6
SCREE_N          = (18, 30)    # per 70 mm of exposed face
SCREE_D_MM       = (2.0, 6.5)


def smooth01(t):
    t = np.clip(t, 0, 1)
    return t * t * (3 - 2 * t)


def to_mesh(man):
    o = man.to_mesh()
    return trimesh.Trimesh(np.asarray(o.vert_properties)[:, :3], np.asarray(o.tri_verts),
                           process=False)


# ── 1 Volume ────────────────────────────────────────────────────────────────
@dataclass
class Tier:
    foot: Polygon          # plan footprint
    z0: float
    z1: float


@dataclass
class Volume:
    tiers: list
    W: float
    H: float

    @property
    def plan(self):
        return shapely.union_all([t.foot for t in self.tiers])

    def manifold(self, inset=0.0, exposed_only=True):
        parts = []
        for t in self.tiers:
            fp = t.foot
            if inset:
                fp = fp.difference(self.exposed(fp).buffer(inset)) if exposed_only \
                    else fp.buffer(-inset)
            if fp.is_empty:
                continue
            for g in getattr(fp, 'geoms', [fp]):
                cs = m3d.CrossSection([np.asarray(shapely.geometry.polygon.orient(g, 1.0)
                                                  .exterior.coords)[:-1]])
                parts.append(m3d.Manifold.extrude(cs, t.z1 - t.z0 - (inset if inset else 0))
                             .translate([0, 0, t.z0]))
        return m3d.Manifold.batch_boolean(parts, m3d.OpType.Add)

    def exposed(self, fp=None):
        """The exposed part of a footprint's outline (not on the tile edge)."""
        fp = self.plan if fp is None else fp
        edge = box(0, 0, self.W, self.H).exterior.buffer(CONTACT_MM)
        return fp.exterior.difference(edge)


def make_volume(rng, form, W, H):
    zf, zt = FOOT_Z_MM - 1.0, FOOT_Z_MM + CLIFF_H_MM
    if form == 'run':
        return Volume([Tier(box(0, H - EDGE_T_MM, W, H), zf, zt)], W, H)
    if form == 'wall':
        return Volume([Tier(box(0, H / 2 - WALL_T_MM / 2, W, H / 2 + WALL_T_MM / 2), zf, zt)], W, H)
    if form == 'corner':
        return Volume([Tier(box(0, H - EDGE_T_MM, W, H).union(box(W - EDGE_T_MM, 0, W, H)),
                            zf, zt)], W, H)
    if form == 'outcrop':
        k = min(W, H) / 70.0
        return Volume([Tier(_blob(rng, W / 2, H / 2, 20 * k, 13 * k), zf, FOOT_Z_MM + 14)], W, H)
    if form == 'spire':
        k = min(W, H) / 70.0
        tiers = []
        for rx, ry, hz, dx in ((19, 16, 16, 0), (11, 9.5, 30, 1.5), (6, 5, 46, 2.5)):
            tiers.append(Tier(_blob(rng, W / 2 + dx * k, H / 2 + 0.5 * dx * k, rx * k, ry * k),
                              zf, FOOT_Z_MM + hz))
        return Volume(tiers, W, H)
    raise ValueError(form)


def _blob(rng, cx, cy, rx, ry, n=40, lump=0.14):
    a = np.linspace(0, 2 * np.pi, n, endpoint=False)
    from scipy import ndimage
    r = 1 + lump * ndimage.gaussian_filter1d(rng.standard_normal(n), 2.5, mode='wrap') * 2.2
    return Polygon(np.c_[cx + rx * r * np.cos(a), cy + ry * r * np.sin(a)]).buffer(0)


# ── 2 Cells ─────────────────────────────────────────────────────────────────
def _tilt(rng, axis, deg):
    n = np.zeros(3)
    n[axis] = 1.0
    n += rng.normal(0, np.radians(deg), 3) * (1 - n)
    return n / np.linalg.norm(n)


def split_cells(rng, vol_man, normal_at):
    """Recursive tilted splits of the volume's box → [Manifold cells].
    `normal_at(p)` gives the local outward horizontal normal (for the
    front/back depth split)."""
    lo, hi = np.array(vol_man.bounding_box()[:3]), np.array(vol_man.bounding_box()[3:])
    root = m3d.Manifold.cube((hi - lo + 2.0).tolist()).translate((lo - 1.0).tolist())
    todo, leaves = [root], []
    while todo:
        c = todo.pop()
        b = np.array(c.bounding_box())
        ext = b[3:] - b[:3]
        cen = 0.5 * (b[:3] + b[3:])
        tw = TARGET_W_MM[0] + (TARGET_W_MM[1] - TARGET_W_MM[0]) * rng.random() ** TARGET_W_EXP
        th = TARGET_H_MM[0] + (TARGET_H_MM[1] - TARGET_H_MM[0]) * rng.random() ** TARGET_H_EXP
        nrm = normal_at(cen)
        along = np.array([-nrm[1], nrm[0], 0.0])
        # extents along the face, across it (depth) and vertically
        P = np.array(list(np.ndindex(2, 2, 2))) * ext + b[:3]
        e_al = np.ptp(P @ along)
        e_dp = np.ptp(P @ nrm)
        e_z = ext[2]
        ratios = {'along': e_al / tw, 'depth': e_dp / (2 * TARGET_D_MM), 'z': e_z / th}
        key = max(ratios, key=ratios.get)
        if ratios[key] <= 1.0:
            leaves.append(c)
            continue
        if key == 'z':
            n = _tilt(rng, 2, COURSE_TILT_DEG)
        else:
            d = along if key == 'along' else nrm
            ang = np.radians(rng.normal(0, JOINT_TILT_DEG))
            d = np.array([d[0] * np.cos(ang) - d[1] * np.sin(ang),
                          d[0] * np.sin(ang) + d[1] * np.cos(ang), 0.0])
            d[2] = rng.normal(0, np.radians(JOINT_TILT_DEG) * 0.6)      # lean
            n = d / np.linalg.norm(d)
        proj = P @ n
        tgt = {'along': tw, 'depth': TARGET_D_MM, 'z': th}[key]
        span = np.ptp(proj)
        lo_f = min(max(SPLIT_POS[0], MIN_PART * tgt / max(span, 1e-6)), 0.5)
        off = proj.min() + rng.uniform(lo_f, 1 - lo_f) * span
        a, bb = c.split_by_plane(n.tolist(), float(off))
        for part in (a, bb):
            part = part ^ vol_man if part.volume() > 0 else part
            if not part.is_empty() and part.volume() > 1.0:
                todo.append(part)
    return leaves


# ── 3 Rocks ─────────────────────────────────────────────────────────────────
def rockify(rng, cell, n_out, proud, support, D):
    """One rock from a cell: the whole cell, stepped out (or back) along the
    face normal and swept back to where it was, so it sits on the rock
    beneath and against its neighbours with no void behind it; then a 45°
    support cut, and gentle chisel, chips and grain."""
    rock = cell
    if proud > 0:
        rock = m3d.Manifold.batch_hull([cell, cell.translate((n_out * proud).tolist())])
    elif proud < 0:
        rock = cell.translate((n_out * proud).tolist())
    zb = rock.bounding_box()[2]
    if support is not None:
        # ≤ 45°: the rock may reach past the rock beneath it (whose face is
        # at outward position `support`) only as much as it rises above
        # its own bottom
        # keep  n·p − (z − zb) ≤ support   ⇔   a·p ≤ support − zb, a = (n, −1)
        a = np.array([n_out[0], n_out[1], -1.0])
        na = np.linalg.norm(a)                 # trim_by_plane normalises the normal only
        rock = rock.trim_by_plane((-a / na).tolist(), -(support - zb) / na)
        if rock.is_empty():
            return None
    # rocklab's chisel works on a rock standing on z = 0
    rock = rock.translate([0, 0, -zb])
    saved = (_rl.FACET_R_MM, _rl.FACET_TILT_DEG, _rl.FACET_OFFSET_MM)
    _rl.FACET_R_MM, _rl.FACET_TILT_DEG, _rl.FACET_OFFSET_MM = \
        FACET_R_MM, FACET_TILT_DEG, FACET_OFFSET_MM
    try:
        faceted, _ = _rl.cut_facets(rng, rock, max(D, 4.0))
        faceted = faceted ^ rock
        if faceted.is_empty() or faceted.volume() < 0.4 * rock.volume():
            faceted = rock
        faceted, _ = _rl.chip_corners_edges(rng, faceted, max(D, 4.0))
        faceted = _rl.texture(rng, faceted)
    finally:
        _rl.FACET_R_MM, _rl.FACET_TILT_DEG, _rl.FACET_OFFSET_MM = saved
    return faceted.translate([0, 0, zb])


def proud_field(rng, W, H):
    """Coherent displacement over the volume: a smooth 3-D noise, so rocks in
    a column or buttress step together."""
    from dharmatiles.stone.noise import fbm
    seed = int(rng.integers(1 << 30))

    def f(p):
        v = fbm(np.atleast_2d(p) * [1.0, 1.0, 0.35], seed, PROUD_FIELD_MM, octaves=2)
        return PROUD_MM[0] + (PROUD_MM[1] - PROUD_MM[0]) * smooth01(0.5 + 0.9 * v)
    return f


@dataclass
class Built:
    mesh: trimesh.Trimesh
    rocks: int
    cells: int
    peg: float
    zmax: float
    tm: dict


def build(seed=1, form='run', cols=1, rows=1):
    t0 = time.perf_counter()
    tm = {}
    rng = np.random.default_rng(seed)
    W, H = cols * SQUARE_MM, rows * SQUARE_MM
    vol = make_volume(rng, form, W, H)
    vman = vol.manifold()
    plan = vol.plan
    exposed = vol.exposed()
    tile_edge = box(0, 0, W, H).exterior

    def normal_at(p):
        q = shapely.Point(p[0], p[1])
        bd = plan.exterior if plan.geom_type == 'Polygon' else plan.boundary
        nearest = bd.interpolate(bd.project(q))
        v = np.array([q.x - nearest.x, q.y - nearest.y])
        if not plan.contains(q):
            v = -v
        v = -v                                       # outward
        nv = np.linalg.norm(v)
        if nv < 1e-6:
            return np.array([0.0, -1.0, 0.0])
        return np.array([v[0] / nv, v[1] / nv, 0.0])

    cells = split_cells(rng, vman, normal_at)
    shell = vman - vol.manifold(inset=CORE_DEPTH_MM, exposed_only=False)
    tm['cells'] = time.perf_counter() - t0
    pf = proud_field(rng, W, H)
    zt = max(t.z1 for t in vol.tiers)
    # per cell: centroid, outward normal, depth below the surface, contact?
    info = []
    for c in cells:
        b = np.array(c.bounding_box())
        cen = 0.5 * (b[:3] + b[3:])
        q = shapely.Point(cen[0], cen[1])
        bd = plan.boundary
        surf = (c ^ shell).volume() > 0.5
        near = bd.interpolate(bd.project(q))
        contact = tile_edge.distance(near) < CONTACT_MM
        top = b[5] > zt - 0.5
        info.append((cen, normal_at(cen), 0.0 if surf else 1e3, contact, top, b))
    # proud per rock: field + jitter (contact faces stay on the tile edge)
    prouds = []
    for cen, n, depth, contact, top, b in info:
        p = float(pf(cen)[0]) + rng.normal(0, PROUD_JITTER_MM)
        prouds.append(-CONTACT_BACK_MM if contact else p)
    # support: the outward position of whatever is beneath each rock's front
    rocks, t1 = [], time.perf_counter()
    for k, ((cen, n, depth, contact, top, b), p) in enumerate(zip(info, prouds)):
        if depth > CORE_DEPTH_MM and not top:
            continue
        cell = cells[k]
        if top and rng.random() < TOP_DROP_P:            # broken skyline
            cell = cell.trim_by_plane([0, 0, -1], -(b[5] - rng.uniform(*TOP_DROP_MM)))
            if cell.is_empty():
                continue
        support = None
        if b[2] > FOOT_Z_MM + 0.5 and not contact:
            # whatever lies just beneath this rock's footprint (any cell whose
            # box holds a point 0.6 mm under its bottom); the most recessed wins
            fx = np.linspace(b[0], b[3], 4)[1:-1]
            fy = np.linspace(b[1], b[4], 4)[1:-1]
            below = [prouds[j] for j, (c2, n2, d2, ct2, tp2, b2) in enumerate(info)
                     if j != k and b2[2] < b[2] - 0.6 < b2[5]
                     and any(b2[0] <= x <= b2[3] and b2[1] <= y <= b2[4] for x in fx for y in fy)]
            support = min(below) if below else 0.0
        D = float(np.median(b[3:] - b[:3]))
        r = rockify(rng, cell, n, p, None if support is None else
                    float(n[:2] @ _face_point(plan, cen)[:2]) + support, D)
        if r is not None and not r.is_empty():
            rocks.append(r)
    tm['rocks'] = time.perf_counter() - t1
    t2 = time.perf_counter()
    core = vol.manifold(inset=CORE_INSET_MM) ^ m3d.Manifold.cube(
        [W - 2 * CORE_EDGE_MM, H - 2 * CORE_EDGE_MM, 200.0]).translate([CORE_EDGE_MM, CORE_EDGE_MM, 0])
    parts = rocks + [core, _ground(seed, W, H), _cap(rng, vol, seed, W, H)]
    top_ = m3d.Manifold.batch_boolean(parts, m3d.OpType.Add)
    top_ = top_ ^ m3d.Manifold.cube([W, H, 200.0])
    import clifflab as cl
    mesh, peg, zmax, _ = cl.attach_base(top_, cols, rows)
    tm['assemble'] = time.perf_counter() - t2
    tm['total'] = time.perf_counter() - t0
    return Built(mesh, len(rocks), len(cells), peg, zmax, tm)


def _face_point(plan, cen):
    bd = plan.boundary
    q = shapely.Point(cen[0], cen[1])
    p = bd.interpolate(bd.project(q))
    return np.array([p.x, p.y, cen[2]])


def _ground(seed, W, H):
    from dharmatiles.core.mesh import make_heightmap_solid
    dirt = _dirt.dirt_relief(seed, W, H)
    return _gl.to_manifold(make_heightmap_solid(FOOT_Z_MM + dirt.relief, W, H, base_h=0.0))


def _cap(rng, vol, seed, W, H):
    """Dirt on top of the volume, held back from the exposed faces."""
    from dharmatiles.core.mesh import make_heightmap_solid
    zt = max(t.z1 for t in vol.tiers)
    dirt = _dirt.dirt_relief(seed + 7, W, H)
    hf = _gl.to_manifold(make_heightmap_solid(zt - CAP_DROP_MM - 4.0 + dirt.relief, W, H,
                                              base_h=0.0)).translate([0, 0, 4.0])
    parts = []
    for t in vol.tiers:
        fp = t.foot.difference(vol.exposed(t.foot).buffer(CAP_INSET_MM))
        if fp.is_empty:
            continue
        lift = t.z1 - zt
        for g in getattr(fp, 'geoms', [fp]):
            cs = m3d.CrossSection([np.asarray(shapely.geometry.polygon.orient(g, 1.0)
                                              .exterior.coords)[:-1]])
            prism = m3d.Manifold.extrude(cs, 6.0).translate([0, 0, t.z1 - 5.0])
            parts.append((hf.translate([0, 0, lift])) ^ prism)
    return m3d.Manifold.batch_boolean(parts, m3d.OpType.Add) if parts else m3d.Manifold()


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--form', default='run')
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--cols', type=int, default=1)
    ap.add_argument('--rows', type=int, default=1)
    args = ap.parse_args()
    r = build(args.seed, args.form, args.cols, args.rows)
    out = ROOT / 'clifflab' / 'out'
    out.mkdir(exist_ok=True)
    path = out / f"{args.cols}x{args.rows}-rocks-{args.form}-s{args.seed}.stl"
    r.mesh.export(path)
    ov, _ = _gl.audit_overhang(r.mesh)
    print(f"→ {path}  {len(r.mesh.faces):,} faces, watertight={r.mesh.is_watertight}; "
          f"{r.cells} cells, {r.rocks} rocks; overhang {ov:.1f} mm²; "
          + ", ".join(f"{k} {v:.1f}s" for k, v in r.tm.items()))


if __name__ == '__main__':
    main()
