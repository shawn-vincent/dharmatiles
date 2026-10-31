#!/usr/bin/env python3
"""clifflab — heroic cliffs on a DungeonBlocks tile.

Implements clifflab/DESIGN.md.  A cliff is a set of rock BODIES; each has a
plan footprint (a polygon), a FACE LINE along its edge, a height and an
inset profile.  The face texture is a depth map D(s, z) over (arc length
along the face line, height):

  1 frame ─► 2 activity (60/30/10) ─► 3 masses: large + medium + fine
  anisotropic warped Voronoi (per-cell planes, V-groove fissures) + ledges /
  shelves + long diagonals ─► weathering (convex-only rounding) + grain ─►
  end sections ─► 45° clamp per (s) column ─► top edge ─► 4 ground (dirt,
  talus, plateau caps) ─► 5 scree ─► 6 f = max(G − z, maxᵦ min(dᵦ − gᵦ(sᵦ, z),
  Pᵦ − z)) → marching cubes ─► 7 ∪ slab ∪ scree ∪ DB base → STL

Forms (FORMS): run (gen-1: a straight cliff across the tile), corner,
outcrop, spire, canyon.

Usage:
    python clifflab/clifflab.py [--seed 1] [--form run] [--preset default]
                                [--cols 1 --rows 1] [--no-audit]
"""
from __future__ import annotations

import argparse
import pathlib
import sys
import time
from dataclasses import dataclass, field

import manifold3d as m3d
import numpy as np
import shapely
import trimesh
from scipy import ndimage
from shapely.geometry import LineString, Polygon, box

ROOT = pathlib.Path(__file__).resolve().parent.parent
for _lab in ('dirtlab', 'grasslab', 'rocklab'):
    sys.path.insert(0, str(ROOT / _lab))
import dirtlab as _dirt     # noqa: E402  shared exposed-ground texture
import grasslab as _gl      # noqa: E402  to_manifold, stl_safe, audit_overhang
import rocklab as _rl       # noqa: E402  faceted rocks for the scree

# ── Parameters (DESIGN.md §2), grouped by stage ─────────────────────────────
# 1 frame
SQUARE_MM        = 35.0
FOOT_Z_MM        = 4.0         # foot ground (DB dirt band)
CLIFF_H_MM       = 35.0        # run plateau above the foot (fixed: end sections match)
FACE_Y_FRAC      = 0.4         # run face line at this share of the depth
BATTER_DEG       = 4.0         # faces lean back
PLAN_WOBBLE_MM   = 1.5         # ± plan wander of a face line…
PLAN_SIGMA_MM    = 9.0         # …at this scale
VOX_MM           = 0.18        # face maps and meshing grid

# 1b slabs and walls (run, corner, canyon, wall): a rock body textured on
# every side.  Faces lying along the tile boundary ("contact" faces) only cut
# inward and are flush at their top and bottom rims, so the piece seals
# against neighbouring cliffs and the ground beside it; open faces have full
# relief.  (Shawn 2026-10-30.)
EDGE_T_MM        = 13.0        # slab thickness against an edge (blocks need room to step)
WALL_T_MM        = 11.0        # free-standing wall thickness (wall form)
END_GAP_MM       = 0.6         # slab ends stop this far in from the tile's sides
END_R_MM         = 3.0         # convex corner rounding
CORNER_R_MM      = 8.0         # concave corner rounding (corner form; tighter streaked)
CONTACT_MM       = (1.0, 3.0)  # a face this close (outward) to the tile edge is contact…
CONTACT_RIM_MM   = 2.5         # …and eases to flush over this at its top and bottom
CONTACT_BLEND_MM = 2.5         # σ along the outline of the open → contact blend
FLOOR_TEX_MM     = 0.7         # relief on the floor of cuts that reach the thickness limit
CONTACT_KNEE_MM  = 0.12        # (tight: a loose knee left every contact face set back)
CONTACT_SCALE    = 0.8         # contact faces' (inward-only) relief, scaled to fit the slab
MIN_WALL_MM      = 1.5         # the two sides' cuts never meet: ≥ this rock between
EDGE_KNEE_MM     = 0.4         # softness of those limits
TILE_CLEAR_MM    = 0.0         # no rock pushes closer than this to a tile edge…
TILE_CLEAR_VAR_MM = 0.0       # …plus up to this (a slight relief: never a flat plane)
TILE_KNEE_MM     = 0.04
EDGE_TOP_BAND_MM = (0.6, 2.0)  # rock top shows this far back, then dirt
EDGE_CAP_EASE_MM = 3.0
BACK_FLUSH_MM    = 2.5         # within this of the back edge the top is plateau level

# 2 activity (REQ-ROCK-3)
ACT_SIGMA_MM     = 7.0
ACT_MAJOR_BUMP   = 0.8         # activity bump near major fissures…
ACT_MAJOR_MM     = 3.0         # …within about this
QUIET_FRAC       = 0.6
SHATTER_FRAC     = 0.9         # u above this is shattered (10%)

# 3 masses (anisotropic warped Voronoi at three levels)
LARGE_S_MM       = (7.0, 18.0)
LARGE_S_EXP      = 1.6         # skew small
LARGE_ASPECT     = 2.2
DIAG_FRAC        = 0.2         # share of large masses with diagonal joints
VERT_DEG         = 20.0        # others within ± this of vertical
DIAG_DEG         = (35.0, 55.0)
MEDIUM_S_MM      = (3.0, 9.0)
MEDIUM_ASPECT    = 2.2
MEDIUM_DTHETA    = 10.0
FINE_S_MM        = (1.2, 3.0)
FINE_ASPECT      = 1.4
FINE_DTHETA      = 15.0
WARP_AMP         = 0.07        # joint bend (× the level's mean cell size, mm)
WARP_SIGMA_MM    = 3.0
LARGE_OFF_MM     = 2.4         # σ of a mass's offset (proud / recessed)
LARGE_OFF_CLIP   = 6.0
LARGE_PROUD_EXP  = 1.2         # masses step forward 0…LARGE_OFF_CLIP (skewed toward small)
LARGE_TILT_DEG   = 28.0        # across
LARGE_LEAN_DEG   = (-1.5, 9.0)   # in height (+ = leans back; little forward lean)
MEDIUM_OFF_MM    = 0.45
MEDIUM_TILT_DEG  = 9.0
FINE_OFF_MM      = 0.15
GROOVE_LARGE     = ((2.0, 5.0), 0.9)    # (depth range, half-width) mm
GROOVE_MEDIUM    = ((0.6, 1.4), 0.45)
GROOVE_FINE      = ((0.25, 0.45), 0.3)
JOINT_FADE_MM    = 4.0         # joints fade out and reappear along their length
MEDIUM_PLANE     = 0.25        # medium plates' plane offsets, × this (blocks stay flat)
MEDIUM_QUIET     = 0.1         # medium strength on quiet faces

# 3b BLOCKS (rev 6): the cliff face is stacked prismatic blocks — irregular
# columns along the face, split by near-horizontal joints into blocks, each
# a faceted slab stepping out or back from its neighbours (reference B/H/N)
COL_W_MM         = (3.0, 12.0) # column widths along the face…
COL_W_EXP        = 1.5         # …skewed narrow
COL_TILT_DEG     = 7.0         # column joints within ± this of vertical…
COL_DIAG_FRAC    = 0.12        # …except a few at COL_DIAG_DEG
COL_DIAG_DEG     = (18.0, 32.0)
COL_WOBBLE_MM    = 0.35
BLOCK_H_MM       = (7.0, 24.0) # block heights in a column
BLOCK_H_EXP      = 1.2
JOINT_TILT_DEG   = 5.0         # row joints within ± this of horizontal
STRATA           = (2, 3)      # shared joint levels (subtle, discontinuous layering)
STRATA_FRAC      = 0.55        # share of columns that snap a joint to a stratum
MASS_COLS        = (1, 2)      # columns grouped into masses (buttresses)
MASS_PROUD_MM    = 8.0         # mass steps forward 0…this (skewed small)
MASS_PROUD_EXP   = 1.3
COL_JITTER_MM    = 1.8
COL_PARTIAL_P    = 0.25        # share of column joints that stop part-way (columns merge)
ROW_FADE_FLOOR   = 0.0         # row joints may fade out completely (tall blocks)
BLOCK_JITTER_MM  = 0.25
STEP_BACK_MM     = (0.6, 2.0)  # a block above steps back this much…
STEP_BACK_P      = 0.18        # …with this chance (lit block tops; stack recedes)
RECESS_P         = 0.08        # some blocks sit back (dark voids)
RECESS_MM        = (0.8, 2.0)
FACETS           = 4           # chisel facets per block face
FACET_DEG        = (18.0, 40.0)
FACET_R          = (0.45, 0.9) # facet starts this × the block's half-extent from its centre
FRONT_TILT_DEG   = 7.0
BLOCK_CHAMFER_MM = 1.0         # 45° chamfer at every block edge
ROW_JOINT        = ((0.45, 1.0), 0.32)   # (depth range, half-width) mm
COL_JOINT        = ((1.5, 3.2), 0.5)

# 4 ledges, shelves, diagonals (counts per 70 mm of face)
LEDGES           = (4, 8)
LEDGE_LEN_MM     = (4.0, 14.0)
LEDGE_STEP_MM    = (0.6, 1.6)
LEDGE_BAND_MM    = (2.0, 5.0)
UNDERCUT_FRAC    = 0.35
DIAGONALS        = (0, 2)
DIAG_LEN_MM      = (15.0, 40.0)
DIAG_DEPTH_MM    = (0.9, 1.8)
DIAG_HALF_W_MM   = 0.6

# 5 weathering, fine detail
ROUND_SIGMA_MM   = 0.25
GRAIN_SIGMA_MM   = 0.7
GRAIN_AMP_MM     = 0.08
PITS_PER_CM2     = 0.6         # in shattered zones
ROUND2_SIGMA_MM  = 0.15

# 6 clamp, top edge
RIM_FADE_MM      = 5.0         # talus fades out toward the tile rim
CLAMP_DEG        = 42.0
CLAMP_BLUR_MM    = (2.0, 0.5)   # (s, z) blur of the clamped surface on undersides
TOP_OFF_MM       = (-5.0, 3.0)
TOP_OFF_MEDIUM_MM = (-1.4, 0.7)  # per plate, on top of the mass's
TOP_BAND_MM      = (2.0, 7.0)  # rock top shows this far back before the dirt cap
TOP_TILT         = 0.1         # rock top slopes back (mm per mm)

# 7 ground
PLATEAU_DROP_MM  = 0.4         # dirt cap sits this below the body top
CAP_EASE_MM      = 6.0         # cap eases from the edge line to the plateau level
CAP_CROWN_MM     = 1.8         # free-standing tops rise this much toward the middle
TALUS_H_MM       = (0.6, 2.2)
TALUS_LAMBDA_MM  = (3.0, 7.0)
TALUS_SIGMA_MM   = 6.0

# 8 scree
SCREE_PER_70MM   = (25, 45)    # per 70 mm of visible face
SCREE_D_MM       = (2.5, 8.0)
SCREE_D_EXP      = 2.0
SCREE_DIST_MM    = 3.5         # mean distance in front of the face base
SCREE_SINK_MM    = 0.3
SCREE_CLEAR_MM   = 0.4
SCREE_RIM_MM     = 3.0

# 9 mesh, base
MESH_FACES       = 1_600_000
SIMPLIFY_TOL_MM  = 0.01

PRESETS = {
    'default': {},
    'quiet': dict(QUIET_FRAC=0.75, SHATTER_FRAC=0.95, MEDIUM_QUIET=0.05),
    'deep': dict(GROOVE_LARGE=((3.0, 6.5), 1.0), GROOVE_MEDIUM=((0.9, 2.0), 0.5),
                 LARGE_OFF_MM=3.2, LARGE_OFF_CLIP=6.5),
    'massive': dict(LARGE_S_MM=(14.0, 30.0), MEDIUM_S_MM=(4.0, 11.0)),
    'diagonal': dict(DIAG_FRAC=0.5, DIAGONALS=(3, 5)),
}
_DEFAULTS = {k: globals()[k] for p in PRESETS.values() for k in p}


def apply_preset(name):
    globals().update(_DEFAULTS)
    for k, v in PRESETS[name].items():
        globals()[k] = v


# ── Helpers ─────────────────────────────────────────────────────────────────
def smooth01(t):
    t = np.clip(t, 0, 1)
    return t * t * (3 - 2 * t)


def noise(rng, shape, sigma_px, wrap_s=False):
    """Unit-variance smooth noise; axis 0 (s) wraps for closed face lines."""
    mode = ['wrap' if wrap_s else 'reflect'] + ['reflect'] * (len(shape) - 1)
    n = ndimage.gaussian_filter(rng.standard_normal(shape), sigma_px, mode=mode)
    return (n - n.mean()) / (n.std() + 1e-12)


def clamp_up(D, h):
    out = D.copy()
    rise = h * np.tan(np.radians(CLAMP_DEG))
    for k in range(1, out.shape[1]):
        np.maximum(out[:, k], out[:, k - 1] - rise, out=out[:, k])
    return out


def blur(a, sigma_px, wrap_s):
    return ndimage.gaussian_filter(a, sigma_px, mode=['wrap' if wrap_s else 'nearest', 'nearest'])


def rank01(a):
    r = np.empty(a.size)
    r[np.argsort(a, axis=None)] = np.linspace(0, 1, a.size)
    return r.reshape(a.shape)


def skewed(rng, lo_hi, exp, n=None):
    return lo_hi[0] + (lo_hi[1] - lo_hi[0]) * rng.random(n) ** exp


# ── Bodies and forms ────────────────────────────────────────────────────────
@dataclass
class Body:
    """One rock body: footprint, face line (where the texture lives), height."""
    region: Polygon                # rock in plan (may extend past the tile)
    line: LineString               # face line (closed ring for free-standing bodies)
    top_z: float                   # nominal top
    closed: bool
    taper: float = 0.0             # extra inset per mm of height (spires)
    cap: bool = True               # dirt cap on the top
    backs: object = None           # edge-hugging bodies: the tile edge(s) behind the face


def _blob(rng, cx, cy, rx, ry, n=48, lump=0.12):
    a = np.linspace(0, 2 * np.pi, n, endpoint=False)
    r = 1 + lump * noise(rng, (n,), 3.0, wrap_s=True)
    return Polygon(np.c_[cx + rx * r * np.cos(a), cy + ry * r * np.sin(a)])


def _slab(poly, W, H):
    """A rock body: convex corners rounded by END_R_MM, concave by
    CORNER_R_MM, then clipped to the tile — so where a body runs past the
    tile edge its corner there is square and its face lies ON the boundary
    (butted tiles touch).  Its whole outline is the face line (closed),
    textured all round."""
    poly = poly.buffer(-END_R_MM).buffer(END_R_MM).buffer(CORNER_R_MM).buffer(-CORNER_R_MM)
    poly = poly.intersection(box(0, 0, W, H))
    return poly, LineString(poly.exterior.coords)


def _slab_round_front(poly, W, H):
    """Round only the corners that are NOT on the tile boundary (the free
    front corners); corners on the boundary stay square and flush."""
    r, rc = END_R_MM, CORNER_R_MM
    rounded = poly.buffer(-r).buffer(r).buffer(rc).buffer(-rc)
    # keep the original outline within r of the tile edge (square, flush)
    rim = box(0, 0, W, H).difference(box(r, r, W - r, H - r))
    corners = []
    for x, y in poly.exterior.coords[:-1]:
        on_edge = min(x, W - x, y, H - y) < 1e-6
        if on_edge:
            corners.append(shapely.Point(x, y).buffer(r * 1.01))
    keep = shapely.union_all(corners) if corners else shapely.Point(-1e3, -1e3)
    poly = rounded.union(poly.intersection(keep).intersection(rim)).buffer(0)
    poly = poly.intersection(box(0, 0, W, H))
    if poly.geom_type != 'Polygon':
        poly = max(poly.geoms, key=lambda q: q.area)
    return poly, LineString(poly.exterior.coords)


def make_bodies(rng, form, W, H):
    """Slabs against the tile edge(s) (run, corner, canyon) and a free-standing
    wall: rock bodies textured on every side; their faces along the tile
    boundary are contact faces (cut-only, flush rims).  Free-standing forms
    sit anywhere."""
    zf = FOOT_Z_MM
    zp = zf + CLIFF_H_MM
    T, M = EDGE_T_MM, 3.0 * END_R_MM      # M: past the tile edge (square, flush corners)
    if form == 'run':                     # B / H / N: a slab against the back edge
        reg, line = _slab(box(-M, H - T, W + M, H + M), W, H)
        # ends ON the tile sides, front corners rounded inside the tile
        reg = reg.difference(box(-M, -M, 0, H + M)).difference(box(W, -M, W + M, H + M))
        reg, line = _slab_round_front(reg, W, H)
        return [Body(reg, line, zp, True, backs=LineString([(0, H), (W, H)]))]
    if form == 'wall':                    # a free-standing rock wall across the tile
        Tw = WALL_T_MM
        reg, line = _slab_round_front(box(0, H / 2 - Tw / 2, W, H / 2 + Tw / 2), W, H)
        return [Body(reg, line, zp, True)]
    if form == 'corner':                  # C: an inside corner along two edges
        reg, line = _slab_round_front(box(0, H - T, W, H).union(box(W - T, 0, W, H)), W, H)
        return [Body(reg, line, zp, True, backs=LineString([(0, H), (W, H), (W, 0)]))]
    if form == 'canyon':                  # I / K: two slabs across a gorge
        r1, l1 = _slab_round_front(box(0, H - T, W, H), W, H)
        r2, l2 = _slab_round_front(box(0, 0, W, T), W, H)
        return [Body(r1, l1, zp, True, backs=LineString([(0, H), (W, H)])),
                Body(r2, l2, zf + 0.8 * CLIFF_H_MM, True, backs=LineString([(0, 0), (W, 0)]))]
    if form == 'outcrop':                 # A: a low broad outcrop
        k = min(W, H) / 70.0
        reg = _blob(rng, W / 2, H / 2 + 2 * k, 22 * k, 15 * k)
        return [Body(reg, LineString(reg.exterior.coords), zf + 14, True)]
    if form == 'spire':                   # E / P: a tiered tower
        k = min(W, H) / 70.0
        out = []
        for rx, ry, hz, dx in ((20, 17, 18, 0), (12, 10, 32, 2), (6.5, 5.5, 46, 3.5)):
            reg = _blob(rng, W / 2 + dx * k, H / 2 + dx * 0.5 * k, rx * k, ry * k, lump=0.1)
            out.append(Body(reg, LineString(reg.exterior.coords), zf + hz, True,
                            taper=0.04, cap=rx * k > 10))
        return out
    raise ValueError(form)


# ── 3 Masses: anisotropic warped Voronoi on (s, z) ──────────────────────────
@dataclass
class Grid2:
    """A face map grid: s along the face line, z up (both VOX_MM)."""
    ss: np.ndarray
    zs: np.ndarray
    period: float | None           # closed face lines wrap in s

    @property
    def shape(self):
        return (len(self.ss), len(self.zs))

    @property
    def S(self):
        return np.broadcast_to(self.ss[:, None], self.shape)

    @property
    def Z(self):
        return np.broadcast_to(self.zs[None, :], self.shape)

    def ds(self, a, b):
        d = a - b
        if self.period:
            d = (d + self.period / 2) % self.period - self.period / 2
        return d


def dart_seeds(rng, g, s0, s1, z0, z1, sizes_fn, aspect, tries):
    """Variable-radius dart throwing (z scaled by 1/aspect, s wrapping)."""
    pts, ss = [], []
    for _ in range(tries):
        p = np.array([rng.uniform(s0, s1), rng.uniform(z0, z1)])
        s = sizes_fn()
        if pts:
            P = np.array(pts)
            d = np.hypot(g.ds(P[:, 0], p[0]), (P[:, 1] - p[1]) / aspect)
            if (d < 0.45 * (np.array(ss) + s)).any():
                continue
        pts.append(p)
        ss.append(s)
    return np.array(pts), np.array(ss)


def voronoi(g, C, S, TH, aspect, warp, phase):
    """Owner label: argmin ‖(Δ·a, Δ·b / aspect)‖ + warp·φ — an unweighted
    anisotropic Voronoi, so joints are straight-ish (size-weighted cells made
    curved, cobble-like boundaries); size variety comes from the seed spacing."""
    X, Z = g.S.ravel(), g.Z.ravel()
    w = warp.ravel()
    lab = np.empty(X.size, np.int32)
    ca, sa = np.cos(TH), np.sin(TH)               # θ from vertical: b = (sin, cos)
    for a in range(0, X.size, 20000):
        b = slice(a, a + 20000)
        dx = g.ds(X[b, None], C[None, :, 0])
        dz = Z[b, None] - C[None, :, 1]
        across = dx * ca - dz * sa
        along = dx * sa + dz * ca
        lab[b] = (np.hypot(across, along / aspect) + w[b, None] * phase[None]).argmin(1)
    return lab.reshape(g.shape)


def boundary_dist(lab, h, wrap):
    """Distance (mm) to the nearest cell boundary."""
    edge = np.zeros(lab.shape, bool)
    d0 = lab != np.roll(lab, 1, 0)
    if not wrap:
        d0[0] = False
    edge |= d0 | np.roll(d0, -1, 0)
    d1 = lab[:, 1:] != lab[:, :-1]
    edge[:, 1:] |= d1
    edge[:, :-1] |= d1
    if wrap:
        pad = lab.shape[0] // 2
        e = np.concatenate([edge[-pad:], edge, edge[:pad]])
        return ndimage.distance_transform_edt(~e)[pad:-pad] * h
    return ndimage.distance_transform_edt(~edge) * h


def level(rng, g, C, S, TH, aspect, off_sd, off_clip, tilt_deg, lean_deg, groove, fade,
          recess_clip=None):
    """One Voronoi level → (labels, plane offset map, groove map, boundary dist)."""
    h = VOX_MM
    wrap = g.period is not None
    n = len(C)
    warp = noise(rng, g.shape, WARP_SIGMA_MM / h, wrap) * WARP_AMP * S.mean()
    phase = rng.uniform(-1, 1, n)
    lab = voronoi(g, C, S, TH, aspect, warp, phase)
    if recess_clip == 'proud':
        # proud-only: the outline is the deepest plane and every mass steps
        # forward from it by its own amount (stepped blocks, slab intact)
        off = -off_clip * rng.random(n) ** LARGE_PROUD_EXP
    else:
        off = np.clip(rng.normal(0, off_sd, n), -off_clip,
                      off_clip if recess_clip is None else recess_clip)
    tx = np.tan(np.radians(rng.uniform(-tilt_deg, tilt_deg, n)))
    tz = np.tan(np.radians(rng.uniform(*lean_deg, n)))
    plane = off[lab] + tx[lab] * g.ds(g.S, C[lab, 0]) + tz[lab] * (g.Z - C[lab, 1])
    dist = boundary_dist(lab, h, wrap)
    (dlo, dhi), hw = groove
    depth = blur(rng.uniform(dlo, dhi, n)[lab], 1.0 / h, wrap) * fade
    return lab, plane, depth * np.clip(1 - dist / hw, 0, None), dist


def joint_fade(rng, g):
    return np.clip(0.55 + 0.6 * noise(rng, g.shape, JOINT_FADE_MM / VOX_MM, g.period is not None),
                   0.12, 1.0)


# ── 4 Ledges, shelves, diagonals ────────────────────────────────────────────
def ledges(rng, g, zf, zt, keep):
    out = np.zeros(g.shape)
    S, Z = g.S, g.Z
    L_face = g.ss[-1] - g.ss[0]
    n = int(round(rng.integers(*LEDGES, endpoint=True) * L_face / 70.0))
    for _ in range(n):
        L = rng.uniform(*LEDGE_LEN_MM)
        s0 = rng.uniform(*keep) if keep else rng.uniform(g.ss[0], g.ss[-1])
        z0 = rng.uniform(zf + 3, max(zt - 3, zf + 4))
        a = np.tan(np.radians(rng.uniform(-10, 10)))
        St, B = rng.uniform(*LEDGE_STEP_MM), rng.uniform(*LEDGE_BAND_MM)
        t = g.ds(S, s0) / L
        taper = smooth01(t * L / 3.0) * smooth01((1 - t) * L / 3.0)
        bend = 0.5 * noise(rng, (len(g.ss),), 2.0 / VOX_MM, g.period is not None)[:, None]
        zl = z0 + a * g.ds(S, s0) + bend
        up = smooth01((Z - zl) / 0.25 + 0.5)
        if rng.random() < UNDERCUT_FRAC:          # face below steps back
            band = (1 - up) * smooth01(1 - (zl - Z) / B)
        else:                                      # face above steps back: a shelf top
            band = up * smooth01(1 - (Z - zl) / B)
        out += St * band * taper
    return out


def diagonals(rng, g, zf, zt, keep):
    h = VOX_MM
    gro = np.zeros(g.shape)
    L_face = g.ss[-1] - g.ss[0]
    n = int(round(rng.integers(*DIAGONALS, endpoint=True) * L_face / 70.0))
    for _ in range(n):
        L = min(rng.uniform(*DIAG_LEN_MM), 1.2 * (zt - zf))
        ang = np.radians(rng.uniform(30, 60)) * rng.choice([-1, 1])   # from vertical
        p = np.array([rng.uniform(*keep) if keep else rng.uniform(g.ss[0], g.ss[-1]),
                      rng.uniform(zf + 1, max(zt - 0.5 * L, zf + 2))])
        pts = [p.copy()]
        for _ in range(int(L / 0.5)):
            ang += rng.normal(0, 0.06)
            p = p + 0.5 * np.array([np.sin(ang), np.cos(ang)])
            pts.append(p.copy())
        pts = np.array(pts)
        s_rel = pts[:, 0] - g.ss[0]
        if g.period:
            s_rel = s_rel % g.period
        ii = np.clip(np.round(s_rel / h).astype(int), 0, g.shape[0] - 1)
        kk = np.clip(np.round((pts[:, 1] - g.zs[0]) / h).astype(int), 0, g.shape[1] - 1)
        mask = np.zeros(g.shape, bool)
        mask[ii, kk] = True
        order = np.zeros(g.shape)
        order[ii, kk] = np.linspace(0, 1, len(pts))
        dist, idx = ndimage.distance_transform_edt(~mask, return_indices=True)
        t = order[idx[0], idx[1]]
        d = rng.uniform(*DIAG_DEPTH_MM) * np.sin(np.pi * np.clip(t, 0, 1)) ** 0.5
        gro = np.maximum(gro, d * np.clip(1 - dist * h / DIAG_HALF_W_MM, 0, None))
    return gro


# ── 3b Blocks ───────────────────────────────────────────────────────────────
def build_blocks(rng, g, zf, zt):
    """Stacked prismatic blocks on the face grid.  Returns (D, block label,
    distance to block edges, distance to column joints, column label,
    groove map, per-column top offsets)."""
    h = VOX_MM
    wrap = g.period is not None
    S, Z = g.S, g.Z
    zs = g.zs
    # column boundaries along s (a straight one at the seam of closed lines)
    s0 = g.ss[0] if wrap else g.ss[0] - 6.0
    s1 = g.ss[0] + g.period if wrap else g.ss[-1] + 6.0
    bs = [s0]
    while bs[-1] < s1:
        bs.append(bs[-1] + float(skewed(rng, COL_W_MM, COL_W_EXP)))
    if wrap:
        bs = bs[:-1]
        if s1 - bs[-1] < COL_W_MM[0]:
            bs = bs[:-1]
    bs = np.array(bs)
    nb = len(bs)
    diag = rng.random(nb) < COL_DIAG_FRAC
    tilt = np.radians(np.where(diag, rng.choice([-1, 1], nb) * rng.uniform(*COL_DIAG_DEG, nb),
                               rng.uniform(-COL_TILT_DEG, COL_TILT_DEG, nb)))
    tilt[0] = 0.0                                          # the seam / first joint
    zmid = 0.5 * (zf + zt)
    wob = noise(rng, (nb, len(zs)), (0, 2.0 / h)) * COL_WOBBLE_MM
    wob[0] = 0.0
    B = bs[:, None] + np.tan(tilt)[:, None] * (zs[None, :] - zmid) + wob     # (nb, nz)
    rel = S - s0
    # some joints exist only over part of the height: there the two columns
    # are one block (fractures stop and reappear)
    act = np.ones((nb, len(zs)), bool)
    for c in range(1, nb):
        if rng.random() < COL_PARTIAL_P:
            # the joint stops for one stretch (one block bridges two columns)
            za = rng.uniform(zf, zt)
            act[c] = np.abs(zs - za) > 0.5 * rng.uniform(5.0, 12.0)
    # column = the last ACTIVE joint to the left (counting active joints
    # renumbered every column right of a gap: horizontal stripes everywhere)
    col = np.zeros(g.shape, int)
    for c in range(1, nb):
        col = np.where((rel >= (B[c] - s0)[None, :]) & act[c][None, :], c, col)
    # rows: per column, joints going up; some snap to shared strata
    strata = rng.uniform(zf + 5, zt - 4, int(rng.integers(*STRATA, endpoint=True)))
    row = np.zeros(g.shape, int)
    ncol = nb
    joints = []
    for c in range(ncol):
        m = col == c
        if not m.any():
            joints.append([])
            continue
        z = zf - rng.uniform(0, 6)
        js = []
        while z < zt + 4:
            z += float(skewed(rng, BLOCK_H_MM, BLOCK_H_EXP))
            js.append(z)
        if rng.random() < STRATA_FRAC and js:
            for st in strata:
                j = int(np.argmin(np.abs(np.array(js) - st)))
                if abs(js[j] - st) < 3.0:
                    js[j] = st + rng.normal(0, 0.3)
        sc = S[m].mean()
        r_ = np.zeros(m.sum(), int)
        for zj in js:
            slope = np.tan(np.radians(rng.uniform(-JOINT_TILT_DEG, JOINT_TILT_DEG)))
            r_ += Z[m] >= zj + slope * g.ds(S[m], sc)
        row[m] = r_
        joints.append(js)
    raw = col * 1000 + row
    _, lab = np.unique(raw, return_inverse=True)
    lab = lab.reshape(g.shape)
    n = lab.max() + 1
    bcol = np.zeros(n, int)
    brow = np.zeros(n, int)
    bcol[lab.ravel()] = col.ravel()
    brow[lab.ravel()] = row.ravel()
    # offsets: mass (buttress) + column + block, stepping back going up
    mass = np.zeros(ncol, int)
    k, mi = 0, 0
    while k < ncol:
        w = int(rng.integers(*MASS_COLS, endpoint=True))
        mass[k:k + w] = mi
        k, mi = k + w, mi + 1
    om = MASS_PROUD_MM * rng.random(mi) ** MASS_PROUD_EXP
    oc = om[mass] + rng.normal(0, COL_JITTER_MM, ncol)
    nrow = brow.max() + 1
    step = np.where(rng.random((ncol, nrow)) < STEP_BACK_P,
                    rng.uniform(*STEP_BACK_MM, (ncol, nrow)), 0.0)
    back = np.cumsum(step, 1)
    # the stack recedes from its foot: offsets relative to the lowest visible row
    k0 = np.zeros(ncol, int)
    for c in range(ncol):
        mm = (col == c) & (Z >= zf)
        if mm.any():
            k0[c] = row[mm].min()
    back = back - back[np.arange(ncol), np.clip(k0, 0, nrow - 1)][:, None]
    o = oc[bcol] - back[bcol, brow] + rng.normal(0, BLOCK_JITTER_MM, n)
    rec = rng.random(n) < RECESS_P
    o = np.where(rec, o - rng.uniform(*RECESS_MM, n), o)
    # block geometry: centre and half-extents
    idx = np.arange(n)
    sc = np.asarray(ndimage.mean(S, lab, idx))
    zc = np.asarray(ndimage.mean(Z, lab, idx))
    hs = 0.5 * (np.asarray(ndimage.maximum(S, lab, idx)) - np.asarray(ndimage.minimum(S, lab, idx)))
    hz = 0.5 * (np.asarray(ndimage.maximum(Z, lab, idx)) - np.asarray(ndimage.minimum(Z, lab, idx)))
    dS = g.ds(S, sc[lab])
    dZ = Z - zc[lab]
    # chiselled face: max of a front plane and a few facet planes (convex)
    a0 = np.tan(np.radians(rng.uniform(-FRONT_TILT_DEG, FRONT_TILT_DEG, n)))
    b0 = np.tan(np.radians(rng.uniform(-1.0, FRONT_TILT_DEG, n)))
    D = -o[lab] + a0[lab] * dS + b0[lab] * dZ
    base_ang = [0.0, np.pi, None, np.pi / 2]               # sides, any, top (no 'down')
    for kf in range(FACETS):
        ba = base_ang[kf % len(base_ang)]
        ang = rng.uniform(0, 2 * np.pi, n) if ba is None else ba + rng.uniform(-0.45, 0.45, n)
        ang = np.where(np.sin(ang) < -0.3, -ang, ang)       # no downward facets
        ux, uz = np.cos(ang), np.sin(ang)
        t = np.tan(np.radians(rng.uniform(*FACET_DEG, n)))
        r = rng.uniform(*FACET_R, n) * (np.abs(ux) * hs + np.abs(uz) * hz)
        if ba == np.pi / 2:                                 # top chamfer on some blocks only
            r = np.where(rng.random(n) < 0.35, r, 1e3)
        D = np.maximum(D, -o[lab] + t[lab] * (ux[lab] * dS + uz[lab] * dZ - r[lab]))
    # chamfered block edges, joints between blocks (columns deeper)
    e = boundary_dist(lab, h, wrap)
    ec = boundary_dist(col, h, wrap)
    D = np.maximum(D, -o[lab] + (BLOCK_CHAMFER_MM - e))
    (rlo, rhi), rhw = ROW_JOINT
    (clo, chi), chw = COL_JOINT
    jr = blur(rng.uniform(rlo, rhi, n)[lab].astype(float), 0.8 / h, wrap) * np.clip(
        0.45 + 0.8 * noise(rng, g.shape, JOINT_FADE_MM / h, wrap), ROW_FADE_FLOOR, 1.0)
    jc = blur(rng.uniform(clo, chi, ncol)[col].astype(float), 0.8 / h, wrap) * joint_fade(rng, g)
    gro = np.maximum(jr * np.clip(1 - e / rhw, 0, None), jc * np.clip(1 - ec / chw, 0, None))
    D = D + gro
    toff = rng.uniform(*TOP_OFF_MM, ncol)
    return D, lab, e, ec, col, gro, jc * np.clip(1 - ec / chw, 0, None), toff


# ── Face map of one body ────────────────────────────────────────────────────
@dataclass
class FaceMap:
    g: Grid2
    D: np.ndarray          # inset from the face line (mm, + = into the rock), clamped
    top: np.ndarray        # top edge height per s
    split: tuple           # quiet / fracture / detail shares
    area: float            # visible face area (mm²)
    major_s: np.ndarray    # s of major fissures at the base


def _ray_to_box(P, d, W, H):
    """Distance from points P along unit directions d to the tile boundary
    (0 for points already outside it)."""
    with np.errstate(divide='ignore', invalid='ignore'):
        tx = np.where(d[:, 0] > 1e-9, (W - P[:, 0]) / d[:, 0],
                      np.where(d[:, 0] < -1e-9, -P[:, 0] / d[:, 0], np.inf))
        ty = np.where(d[:, 1] > 1e-9, (H - P[:, 1]) / d[:, 1],
                      np.where(d[:, 1] < -1e-9, -P[:, 1] / d[:, 1], np.inf))
    inside = (P[:, 0] >= 0) & (P[:, 0] <= W) & (P[:, 1] >= 0) & (P[:, 1] <= H)
    return np.where(inside, np.minimum(np.minimum(tx, ty), 1e3), 0.0)


def _chord(region, P, d, step=0.2, reach=40.0):
    """Length of the ray from P along d that stays inside region."""
    t = np.arange(step, reach, step)
    Q = P[:, None, :] + t[None, :, None] * d[:, None, :]
    inside = shapely.contains_xy(region, Q[..., 0], Q[..., 1])
    first_out = np.where(inside.all(1), len(t) - 1, np.argmin(inside, 1))
    return t[first_out]


def face_map(rng, body, W, H):
    zf, zt = FOOT_Z_MM, body.top_z
    h = VOX_MM
    Lb = body.line.length
    if body.closed:
        ss = np.arange(0, Lb, Lb / max(int(round(Lb / h)), 8))
        period = Lb
    else:
        ss = np.arange(0, Lb, h)
        period = None
    zs = np.arange(zf - 1.5, zt + 3.0, h)
    g = Grid2(ss, zs, period)
    wrap = period is not None
    sa, sb = ss[0], ss[-1] + (h if wrap else 0)
    keep = None
    # where each face-line sample is, its outward normal, and how far it is
    # (along the normal) to the tile boundary outward and inward
    P_ = np.array([body.line.interpolate(s_).coords[0] for s_ in ss])
    tg = np.gradient(P_, axis=0)
    nrm = np.c_[tg[:, 1], -tg[:, 0]] / np.maximum(np.linalg.norm(tg, axis=1), 1e-9)[:, None]
    flip = shapely.contains_xy(body.region, *(P_ + 0.05 * nrm).T)
    nrm[flip] *= -1                                       # outward
    on_tile = (P_[:, 0] > 0.5) & (P_[:, 0] < W - 0.5) & (P_[:, 1] > 0.5) & (P_[:, 1] < H - 0.5)
    # contact faces lie along the tile boundary (c → 1); open faces look out
    # over the tile (c → 0).  din = chord through the body along −normal.
    out0 = _ray_to_box(P_, nrm, W, H)
    c = 1 - smooth01((out0 - CONTACT_MM[0]) / (CONTACT_MM[1] - CONTACT_MM[0]))
    # blend open → contact over a good length of the outline: a short blend
    # round a 3 mm corner left a vertical crease where the two treatments met
    c = blur(c[:, None], CONTACT_BLEND_MM / VOX_MM, period is not None)[:, 0]
    din = _chord(body.region, P_, -nrm)
    S_, Z = g.S, g.Z
    m = 0 if wrap else 10

    Dbl, labB, eB, ecB, colB, groB, groC, toffC = build_blocks(rng, g, zf, zt)
    distL = ecB

    # 2 activity: noise + bump near major fissures, ranked over the visible face
    a = noise(rng, g.shape, ACT_SIGMA_MM / h, wrap) + ACT_MAJOR_BUMP * np.exp(-distL / ACT_MAJOR_MM)
    vis = (Z >= zf) & (Z <= zt) & on_tile[:, None]    # rank over the face on the tile
    u = np.interp(a, np.sort(a[vis]), np.linspace(0, 1, vis.sum()))
    sm = blur(np.where(u < QUIET_FRAC, MEDIUM_QUIET, 1.0), 1.0 / h, wrap)
    sf = blur(np.where(u < QUIET_FRAC, 0.0, np.where(u < SHATTER_FRAC, 0.15, 1.0)), 0.8 / h, wrap)

    def ix(C):
        rel = C[:, 0] - ss[0]
        if wrap:
            rel = rel % period
        return (np.clip((rel / h).astype(int), 0, len(ss) - 1),
                np.clip(((C[:, 1] - zs[0]) / h).astype(int), 0, len(zs) - 1))

    def ms():
        return rng.uniform(*MEDIUM_S_MM)
    m = 0 if wrap else 3
    CM, SM = dart_seeds(rng, g, sa - m, sb + m, zf - 3, zt + 3, ms, MEDIUM_ASPECT,
                        int(6000 * (sb - sa) / 70))
    THM = np.radians(rng.uniform(-VERT_DEG, VERT_DEG, len(CM)))
    labM, planeM, groM, _ = level(rng, g, CM, SM, THM, MEDIUM_ASPECT, MEDIUM_OFF_MM, 1.2,
                                  MEDIUM_TILT_DEG, (-1.5, 6.0), GROOVE_MEDIUM, joint_fade(rng, g))

    wob = -PLAN_WOBBLE_MM * smooth01(0.5 + 0.5 * noise(rng, (len(ss),), PLAN_SIGMA_MM / h, wrap))
    # blocks carry the form; medium cracks only score them (no extra planes)
    D = ((1 - c) * wob)[:, None] + Dbl + sm * (MEDIUM_PLANE * planeM + groM)
    D = D + diagonals(rng, g, zf, zt, keep)

    # 5 weathering: convex-only rounding, then fine shatter + grain + pits
    D = np.maximum(D, blur(D, ROUND_SIGMA_MM / h, wrap))

    def fs():
        return rng.uniform(*FINE_S_MM)
    m = 0 if wrap else 2
    CF, SF = dart_seeds(rng, g, sa - m, sb + m, zf - 2, zt + 2, fs, FINE_ASPECT,
                        int(40000 * (sb - sa) / 70 * (zt - zf + 4) / 39))
    THF = THM[labM[ix(CF)]] + np.radians(rng.uniform(-FINE_DTHETA, FINE_DTHETA, len(CF)))
    _, planeF, groF, _ = level(rng, g, CF, SF, THF, FINE_ASPECT, FINE_OFF_MM, 0.4,
                               6.0, (-3.0, 3.0), GROOVE_FINE, joint_fade(rng, g))
    D = D + sf * (planeF + groF)
    proud = D < blur(D, 1.0 / h, wrap) - 0.1
    D = D + GRAIN_AMP_MM * noise(rng, g.shape, GRAIN_SIGMA_MM / h, wrap) * np.where(proud, 0.5, 1.0)
    shat = np.argwhere(u > SHATTER_FRAC)
    npits = min(int(PITS_PER_CM2 * len(shat) * h * h / 100), len(shat))
    for i, k in shat[rng.choice(len(shat), npits, replace=False)] if npits else []:
        r = rng.uniform(0.3, 0.6)
        d2 = (g.ds(S_, ss[i]) ** 2 + (Z - zs[k]) ** 2) / r ** 2
        D = D + 0.2 * np.clip(1 - d2, 0, None)
    D = np.maximum(D, blur(D, ROUND2_SIGMA_MM / h, wrap))

    # profile: open faces lean back (+ taper for spires); contact faces stay
    # upright on the boundary
    batter = np.tan(np.radians(BATTER_DEG)) * (1 - c)
    D = D + (batter[:, None] + body.taper) * (Z - zf)
    k = EDGE_KNEE_MM
    # contact faces: the texture is shifted back so its proudest point (per
    # column, smoothed along s) just touches the boundary, so it only cuts
    # inward but keeps its whole relief (clipping the bulges left flat
    # panels and foil); it eases to flush at the top and bottom rims, so the
    # piece seals against the ground and neighbours there
    vis_z = (zs >= zf) & (zs <= zt)
    # reference level = the face's proudest points (per column, a running
    # minimum along s so the smoothing never lifts a peak past it): the full
    # texture, its outermost points just touching the boundary — never clipped
    mn = D[:, vis_z].min(1)
    mn = ndimage.minimum_filter1d(mn, int(2.0 / h) | 1, mode='wrap' if wrap else 'nearest')
    lo = np.minimum(blur(mn[:, None], 1.0 / h, wrap)[:, 0], D[:, vis_z].min(1))
    rim = smooth01((Z - zf) / CONTACT_RIM_MM) * smooth01((zt - Z) / CONTACT_RIM_MM)
    kc = CONTACT_KNEE_MM
    Dc = rim * CONTACT_SCALE * (kc * np.logaddexp(0, (D - lo[:, None]) / kc) - kc * np.log(2))
    D = (1 - c)[:, None] * D + c[:, None] * Dc
    # the two sides' cuts never meet: each takes at most half the chord.  The
    # limit's floor carries a rock relief: a flat floor made planes that the
    # mesh simplifier turned into big triangles
    floor = FLOOR_TEX_MM * smooth01(0.5 + 0.35 * noise(rng, g.shape, 1.0 / h, wrap)
                                    + 0.2 * noise(rng, g.shape, 0.4 / h, wrap))
    lim = (0.5 * din - 0.5 * MIN_WALL_MM)[:, None] - floor
    D = lim - k * np.logaddexp(0, (lim - D) / k)              # soft min(D, lim)
    # nothing pushes past the tile boundary (ends especially)
    # the clearance itself is rock-textured, so whatever presses against the
    # tile edge (a proud mass bulging round an end) is never a flat plane
    clear = TILE_CLEAR_MM + TILE_CLEAR_VAR_MM * smooth01(
        0.5 + 0.35 * noise(rng, g.shape, 0.8 / h, wrap) + 0.2 * noise(rng, g.shape, 0.3 / h, wrap))
    out = _ray_to_box(P_, nrm, W, H)[:, None] - clear
    ko = TILE_KNEE_MM
    D = -out + ko * np.logaddexp(0, (D + out) / ko)            # soft max(D, −out)
    # clamp: the face may advance at most tan(CLAMP_DEG) per mm going up.
    # The correction is blurred along s and re-clamped: raw per-column ramps
    # start at different heights and comb the shelf undersides.
    # Undersides become the blur of the clamped surface (averaging can't
    # steepen a slope, so the 45° rule still holds): a smooth chamfer instead
    # of the edge profile above extruded down as vertical streaks.
    Dc = clamp_up(D, h)
    ramp = blur(((Dc - D) > 0.02).astype(float), 0.6 / h, wrap)
    D = clamp_up((1 - ramp) * Dc + ramp * blur(Dc, (CLAMP_BLUR_MM[0] / h, CLAMP_BLUR_MM[1] / h),
                                                 wrap), h)
    # the blur along s can carry a front's proud masses past the tile edge
    # at an end (the tile clip then sliced them flat): limit again, re-clamp
    D = clamp_up(-out + ko * np.logaddexp(0, (D + out) / ko), h)

    # top edge: per-column heights (blocks standing proud or broken off),
    # notched by the column joints
    ktop = np.searchsorted(zs, zt - 1.0)
    top = zt + blur(toffC[colB[:, ktop]].astype(float)[:, None], 0.2 / h, wrap)[:, 0]
    top = top - 0.6 * groC[:, ktop]
    top = zt + (1 - c) * (top - zt)          # contact faces: top flush at plateau level

    # measured split (REQ-ROCK-3) on the visible face
    gs, gz = np.gradient(D, h)
    visf = vis & (Z <= top[:, None])
    detail = (u > SHATTER_FRAC) & visf
    fract = ((np.hypot(gs, gz) > 1.0) | ((groB + sm * groM) > 0.3)) & visf & ~detail
    nv = max(visf.sum(), 1)
    split = (1 - (detail.sum() + fract.sum()) / nv, fract.sum() / nv, detail.sum() / nv)
    k0 = np.searchsorted(zs, zf + 1.0)
    return FaceMap(g, D, top, split, nv * h * h, ss[(groC[:, k0] > 1.5) & on_tile])


# ── 7 Plan fields and ground ────────────────────────────────────────────────
@dataclass
class Plan:
    """Per body on the (x, y) mesh grid: signed plan distance d (+ inside)
    and arc position s of the nearest face-line point."""
    d: np.ndarray
    s: np.ndarray


def plan_fields(body, X, Y):
    pts = shapely.points(X.ravel(), Y.ravel())
    dist = shapely.distance(body.region.exterior, pts)
    inside = shapely.contains_xy(body.region, X.ravel(), Y.ravel())
    s = shapely.line_locate_point(body.line, pts)
    return Plan(np.where(inside, dist, -dist).reshape(X.shape), s.reshape(X.shape))


def sample_s(fm, s, k):
    """D(s, z_k) at arc positions s (linear in s)."""
    g = fm.g
    h = g.ss[1] - g.ss[0]
    if g.period:
        t = (s % g.period) / h
        i0 = np.floor(t).astype(int) % len(g.ss)
        i1 = (i0 + 1) % len(g.ss)
    else:
        t = np.clip((s - g.ss[0]) / h, 0, len(g.ss) - 1)
        i0 = np.floor(t).astype(int)
        i1 = np.minimum(i0 + 1, len(g.ss) - 1)
    f = t - np.floor(t)
    col = fm.D[:, min(k, fm.D.shape[1] - 1)]
    return col[i0] * (1 - f) + col[i1] * f


def top_at(fm, s):
    g = fm.g
    if g.period:
        return np.interp(s % g.period, g.ss, fm.top, period=g.period)
    return np.interp(s, g.ss, fm.top)


def build_ground(rng, bodies, fms, plans, X, Y, W, H, seed):
    """Foot G and per-body caps Pᵦ on the mesh grid."""
    h = VOX_MM
    dirt = _dirt.dirt_relief(seed, W, H)(X, Y)
    zf = FOOT_Z_MM
    dmax = np.max([p.d for p in plans], 0)
    th = TALUS_H_MM[0] + (TALUS_H_MM[1] - TALUS_H_MM[0]) * smooth01(
        0.5 + 0.5 * noise(rng, X.shape, TALUS_SIGMA_MM / h))
    lam = rng.uniform(*TALUS_LAMBDA_MM)
    rim = np.minimum.reduce([X, W - X, Y, H - Y])
    talus = th * np.exp(-np.clip(-dmax, 0, None) / lam) * smooth01(rim / RIM_FADE_MM)
    G = zf + dirt + talus
    P = []
    for b, fm, p in zip(bodies, fms, plans):
        tb = EDGE_TOP_BAND_MM if b.backs is not None else TOP_BAND_MM
        ease = EDGE_CAP_EASE_MM if b.backs is not None else CAP_EASE_MM
        band = np.clip(rng.uniform(*tb) + 0.5 * (tb[1] - tb[0]) * noise(rng, X.shape, 4.0 / h), *tb)
        # the rock top recedes from the clamped face top
        ktop = np.clip(np.searchsorted(fm.g.zs, fm.top - 0.3), 0, fm.D.shape[1] - 1)
        dtop = fm.D[np.arange(len(fm.top)), ktop]
        dt = np.interp(p.s % fm.g.period, fm.g.ss, dtop, period=fm.g.period) if fm.g.period \
            else np.interp(p.s, fm.g.ss, dtop)
        e = p.d - dt                                  # depth behind the top edge
        te = top_at(fm, p.s)
        rock = te - TOP_TILT * e - np.clip(e - band, 0, None)
        # the dirt cap follows the rock's top line near the edge (no ruled edge),
        # easing to the plateau level further back
        wcap = smooth01((e - band) / ease)
        cap = ((1 - wcap) * np.minimum(te, b.top_z) + wcap * b.top_z - PLATEAU_DROP_MM + dirt) \
            if b.cap else -1e9
        if b.backs is None and b.cap:
            # free-standing tops are crowned, not a dirt dish inside a rock rim
            cap = cap + CAP_CROWN_MM * smooth01((e - band) / 8.0)
        Pb = np.maximum(cap, rock)
        if b.backs is not None:
            # the top meets the tile's back edge exactly at plateau level, so
            # raised ground set behind the cliff merges with it
            db = shapely.distance(b.backs, shapely.points(X.ravel(), Y.ravel())).reshape(X.shape)
            wb = smooth01(1 - db / BACK_FLUSH_MM)
            Pb = (1 - wb) * Pb + wb * (b.top_z - PLATEAU_DROP_MM + dirt)
        P.append(Pb)
    return dirt, G, P


# ── 8 Scree ─────────────────────────────────────────────────────────────────
def build_scree(rng, bodies, fms, plans, X, Y, G, W, H):
    h = VOX_MM
    dmax = np.max([p.d for p in plans], 0)
    gy, gx = np.gradient(dmax, h, axis=1), np.gradient(dmax, h, axis=0)
    cands = []                     # (body, s, weight)
    for b, fm in zip(bodies, fms):
        ss = fm.g.ss[::4]
        w = np.full(len(ss), 0.4)
        for ms_ in fm.major_s:
            w += np.exp(-(fm.g.ds(ss, ms_) / 3.0) ** 2) * 40 / max(len(fm.major_s), 1)
        pts = np.array([b.line.interpolate(s).coords[0] for s in ss])
        ok = (pts[:, 0] > SCREE_RIM_MM) & (pts[:, 0] < W - SCREE_RIM_MM) \
            & (pts[:, 1] > SCREE_RIM_MM) & (pts[:, 1] < H - SCREE_RIM_MM)
        cands += [(p, wi) for p, wi, o in zip(pts, w, ok) if o]
    if not cands:
        return []
    P = np.array([c[0] for c in cands])
    w = np.array([c[1] for c in cands])
    w /= w.sum()
    vis = sum(fm.area for fm in fms) / max(CLIFF_H_MM, 1)
    n = int(rng.integers(*SCREE_PER_70MM, endpoint=True) * min(vis / 70.0, 2.0))
    saved = (_rl.HEIGHT_MIN_MM, _rl.CHIP_MIN_Z_MM, _rl.HEIGHT_RATIO, _rl.FAMILIES)
    # fallen blocks: low and blocky (rocklab's small domes read as spikes)
    _rl.HEIGHT_MIN_MM, _rl.CHIP_MIN_Z_MM = 1.0, 0.3
    _rl.HEIGHT_RATIO = (0.3, 0.55)
    _rl.FAMILIES = {'block': 0.45, 'slab': 0.35, 'wedge': 0.2}
    placed, parts = [], []
    try:
        for D in sorted(skewed(rng, SCREE_D_MM, SCREE_D_EXP, n), reverse=True):
            for _ in range(60):
                p0 = P[rng.choice(len(P), p=w)]
                i = int(np.clip(p0[0] / h, 0, X.shape[0] - 1))
                j = int(np.clip(p0[1] / h, 0, X.shape[1] - 1))
                nrm = -np.array([gx[i, j], gy[i, j]])
                nrm /= max(np.linalg.norm(nrm), 1e-9)
                q = p0 + nrm * (0.3 * D + rng.exponential(SCREE_DIST_MM)) \
                    + rng.normal(0, 0.8, 2) * np.array([-nrm[1], nrm[0]])
                if not (SCREE_RIM_MM + D / 2 < q[0] < W - SCREE_RIM_MM - D / 2
                        and SCREE_RIM_MM + D / 2 < q[1] < H - SCREE_RIM_MM - D / 2):
                    continue
                qi, qj = int(q[0] / h), int(q[1] / h)
                if dmax[qi, qj] > -0.2 * D:
                    continue
                if all(np.hypot(q[0] - px, q[1] - py) > (D + pd) / 2 * 0.85 + SCREE_CLEAR_MM
                       for px, py, pd in placed):
                    break
            else:
                continue
            rock, _ = _rl.make_rock(rng, float(D))
            r = int(np.ceil(D / 2 / h))
            z = G[max(qi - r, 0):qi + r + 1, max(qj - r, 0):qj + r + 1].min() - SCREE_SINK_MM
            parts.append(rock.rotate([0, 0, float(rng.uniform(0, 360))]).translate([q[0], q[1], z]))
            placed.append((q[0], q[1], D))
    finally:
        _rl.HEIGHT_MIN_MM, _rl.CHIP_MIN_Z_MM, _rl.HEIGHT_RATIO, _rl.FAMILIES = saved
    return parts


# ── 9 Solid ─────────────────────────────────────────────────────────────────
def mesh_solid(bodies, fms, plans, G, P, xs, ys, W, H):
    """Marching cubes on f = max(G − z, maxᵦ min(dᵦ − Dᵦ(sᵦ, z), Pᵦ − z))."""
    from skimage.measure import marching_cubes
    h = VOX_MM
    z0 = FOOT_Z_MM - 1.5
    z1 = max(max(p.max() for p in P), max(fm.top.max() for fm in fms)) + 0.5
    zs = np.arange(z0, z1, h)
    f = np.empty((len(xs), len(ys), len(zs)), np.float32)
    for k, z in enumerate(zs):
        c = np.full(G.shape, -2.0)
        for fm, p, Pb in zip(fms, plans, P):
            kk = int(round((z - fm.g.zs[0]) / h))
            c = np.maximum(c, np.minimum(p.d - sample_s(fm, p.s, kk), Pb - z))
        f[:, :, k] = np.clip(np.maximum(G - z, c), -2.0, 2.0)
    # Support backstop: every voxel of rock needs rock below it within one
    # voxel (≈ 45° at this grid).  By construction this removes only specks
    # where two surfaces meet.
    occ = f > 0
    cross = ndimage.generate_binary_structure(2, 1)
    for k in range(1, occ.shape[2]):
        sup = ndimage.binary_dilation(occ[:, :, k - 1], cross)
        drop = occ[:, :, k] & ~sup
        if drop.any():
            occ[:, :, k] &= sup
            f[:, :, k][drop] = -0.05
    # pad x and y with the field's own continuation (one cell further out it
    # is a cell's distance lower), then air: a face whose outermost points
    # reach the tile edge meshes exactly to it (a constant air pad pulled every
    # surface ≈ 0.1 mm inside); anything past it is clipped by the tile box
    f = np.pad(f, ((1, 1), (1, 1), (0, 0)), mode='edge')
    f[0] -= h
    f[-1] -= h
    f[:, 0] -= h
    f[:, -1] -= h
    f = np.pad(f, 1, constant_values=-1.0)
    verts, faces, _, _ = marching_cubes(f, 0.0, spacing=(h, h, h))
    verts += [xs[0] - 2 * h, ys[0] - 2 * h, zs[0] - h]
    mesh = trimesh.Trimesh(verts, faces, process=True)
    if mesh.volume < 0:
        mesh.invert()
    if len(mesh.faces) > MESH_FACES:
        mesh = mesh.simplify_quadric_decimation(face_count=MESH_FACES)
    solid = _gl.to_manifold(mesh) ^ m3d.Manifold.cube([W, H, 200.0])
    # slab under the chunk (the chunk's flat bottom sits inside it)
    return solid + m3d.Manifold.cube([W, H, z0 + 0.5])


def attach_base(top, cols, rows):
    from dharmatiles.core.config import BaseConfig, SurfaceConfig
    from dharmatiles.bases import dungeonblocks as db
    zmax = top.bounding_box()[5]
    surface = SurfaceConfig(cols=cols, rows=rows, square_mm=SQUARE_MM)
    base_cfg = BaseConfig()
    peg = db.select_peg_height(np.zeros((2, 2)), base_cfg, zmax)
    solid = (top + _gl.to_manifold(db.make_base(surface, peg, base_cfg))).simplify(SIMPLIFY_TOL_MM)
    parts = solid.decompose()
    vols = [q.volume() for q in parts]
    k = int(np.argmax(vols))
    out = parts[k].to_mesh()
    mesh = trimesh.Trimesh(np.asarray(out.vert_properties)[:, :3], np.asarray(out.tri_verts),
                           process=False)
    return _gl.stl_safe(mesh), peg, zmax, (len(parts) - 1, float(sum(vols) - vols[k]))


def islands(mesh, layer=0.2):
    """Slicer-style floating-region count at `layer` mm layers."""
    from shapely.ops import unary_union
    z0, z1 = max(mesh.bounds[0][2], 0.0), mesh.bounds[1][2]
    zs = np.arange(z0 + layer / 2, z1, layer)
    prev, found = None, 0
    for s in mesh.section_multiplane([0, 0, 0], [0, 0, 1], zs):
        if s is None:
            prev = None
            continue
        cur = unary_union([Polygon(p.exterior.coords, [q.coords for q in p.interiors]).buffer(0)
                           for p in s.polygons_full])
        if prev is not None:
            sup = prev.buffer(0.05)
            for gm in getattr(cur, 'geoms', [cur]):
                if gm.area > 1e-4 and gm.intersection(sup).area / gm.area < 0.05:
                    found += 1
        prev = cur
    return found


@dataclass
class Result:
    mesh: trimesh.Trimesh
    bodies: list
    fms: list
    X: np.ndarray
    Y: np.ndarray
    G: np.ndarray
    dirt: np.ndarray
    P: list
    peg: float
    zmax: float
    crumbs: tuple
    n_scree: int
    tm: dict

    @property
    def split(self):
        a = np.array([fm.area for fm in self.fms])
        s = np.array([fm.split for fm in self.fms])
        return tuple((s * a[:, None]).sum(0) / a.sum())


def build(seed=1, form='run', preset='default', cols=2, rows=2):
    apply_preset(preset)
    t0 = time.perf_counter()
    tm = {}
    W, H = cols * SQUARE_MM, rows * SQUARE_MM
    rng = np.random.default_rng(seed)
    bodies = make_bodies(rng, form, W, H)
    fms = [face_map(rng, b, W, H) for b in bodies]
    tm['faces'] = time.perf_counter() - t0
    t = time.perf_counter()
    # cell-centred: no sample lies ON the tile boundary (a sample row at x = 0
    # meshed the whole left end flat against it; the right end had none)
    def centred(L):
        n = int(L // VOX_MM)
        return (L - (n - 1) * VOX_MM) / 2 + np.arange(n) * VOX_MM
    xs, ys = centred(W), centred(H)
    X, Y = np.meshgrid(xs, ys, indexing='ij')
    plans = [plan_fields(b, X, Y) for b in bodies]
    dirt, G, P = build_ground(rng, bodies, fms, plans, X, Y, W, H, seed)
    scree = build_scree(rng, bodies, fms, plans, X, Y, G, W, H)
    tm['ground+scree'] = time.perf_counter() - t
    t = time.perf_counter()
    solid = mesh_solid(bodies, fms, plans, G, P, xs, ys, W, H)
    tm['mesh'] = time.perf_counter() - t
    t = time.perf_counter()
    top = m3d.Manifold.batch_boolean([solid] + scree, m3d.OpType.Add)
    mesh, peg, zmax, crumbs = attach_base(top, cols, rows)
    tm['base'] = time.perf_counter() - t
    return Result(mesh, bodies, fms, X, Y, G, dirt, P, peg, zmax, crumbs, len(scree), tm)


def out_path(seed, form='run', preset='default', cols=2, rows=2):
    d = ROOT / 'clifflab' / 'out'
    d.mkdir(exist_ok=True)
    tag = form + ('' if preset == 'default' else '-' + preset)
    return d / f"{cols}x{rows}-cliff-{tag}-s{seed}.stl"


def report(r, audit=True):
    q, f_, d = r.split
    print(f"  mesh: {len(r.mesh.faces):,} faces, watertight={r.mesh.is_watertight}, "
          f"peg={r.peg} mm, top z {r.zmax:.2f} mm, crumbs dropped {r.crumbs[0]}")
    print(f"  {len(r.bodies)} bodies; tops "
          + ", ".join(f"{fm.top.min() - FOOT_Z_MM:.1f}–{fm.top.max() - FOOT_Z_MM:.1f}" for fm in r.fms)
          + " mm above the foot")
    print(f"  split quiet / fracture / detail: {q:.0%} / {f_:.0%} / {d:.0%}; "
          f"{sum(len(fm.major_s) for fm in r.fms)} major-fissure columns; {r.n_scree} scree rocks")
    ov, ovf = _gl.audit_overhang(r.mesh)
    msg = f"  >45°(+3) overhang {ov:.1f} mm² ({ovf:.2%})"
    if audit:
        msg += f"; slicer islands at 0.2 mm: {islands(r.mesh)}"
    print(msg)


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--form', default='run',
                    choices=['run', 'wall', 'corner', 'canyon', 'outcrop', 'spire'])
    ap.add_argument('--preset', default='default', choices=sorted(PRESETS))
    ap.add_argument('--cols', type=int, default=1)
    ap.add_argument('--rows', type=int, default=1)
    ap.add_argument('--no-audit', action='store_true')
    args = ap.parse_args()
    t0 = time.perf_counter()
    r = build(args.seed, args.form, args.preset, args.cols, args.rows)
    path = out_path(args.seed, args.form, args.preset, args.cols, args.rows)
    r.mesh.export(path)
    print(f"→ {path}  ({path.stat().st_size / 1e6:.1f} MB)")
    report(r, not args.no_audit)
    print("  timing: " + ", ".join(f"{k} {v:.1f}s" for k, v in r.tm.items())
          + f"; total {time.perf_counter() - t0:.1f}s")


if __name__ == '__main__':
    main()
