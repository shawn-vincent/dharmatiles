#!/usr/bin/env python3
"""rockpile — a cliff built by stacking rocks under gravity inside planes.

The reference sheet's cliffs are piles of real rock masses: upright columns,
chunky blocks, slabs lying tilted across whatever is under them, the odd
irregular lump.  Each rock rests on the rocks beneath it; joints are tight;
whole masses stand forward or back.  So:

  1 FORM: contact planes the rocks are meant to touch (run: back, left,
    right, top) and a target height field T(x, y) — the cliff profile the
    pile grows toward (stepped tiers, gullies cut deep back, buttresses
    standing far out).  The front is open.  Every seed ends in the same
    full-height band at both tile ends, so tiles run on end to end (S4).
  2 STACK: a support grid S(x, y) starts as the ground.  Repeatedly take the
    lowest spot still short of T, pick a rock kind, grow its footprint over
    the pocket there (inside a jittered rectangle, over cells no higher than
    the bed), rest it on its highest supports — tilted where they are
    uneven — and fill its underside down to S.  No gaps by construction:
    every rock's footprint abuts its neighbours' and its bed is the pile.
  3 FINISH: each rock, while still convex, is cleaved by whole planes —
    its open faces dressed off true (turned a few degrees in plan and
    battered back, so it reads wedge-shaped, not boxed), face breaks (some ending in an undercut roof or a short shelf), broken
    corners and shoulders, 45° V joints under the rock it carries — then
    gets rocklab's chips and grain.  Detail gathers in shattered zones.  A
    rock resting on rock carries a 45° keel sunk into the one below, so
    nothing the finishing takes off leaves a ceiling.
  4 ASSEMBLY: rocks, minus V-wedge fissures cut across the assembled face
    (bending, branching, running on across joints) ∪ dirt over the top ∪
    dirt ground banked up against the face ∪ scree (angular rocklab
    stones, 1–8 mm, densest at the face, in the shattered zones and under
    fissure mouths; grit half-buried in the bank) ∪ DB base.  Rock size
    follows the zones too: big quiet masses between them, small shattered
    blocks in them.

Usage:
    python clifflab/rockpile.py [--form run] [--seed 1] [--cols 1]
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
from scipy import ndimage
from shapely.geometry import box

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
GRID_MM          = 0.25

# 1 form (run)
DEPTH_MM         = (8.0, 20.0)  # how far the cliff reaches out from the back plane…
TOP_DEPTH_MM     = (5.0, 10.0)   # …and the full-height part of it
MASS_W_MM        = (6.0, 15.0)   # along-run segments with their own profile
FULL_P           = 0.35          # a segment is full height all the way out
TIER_Z           = (0.3, 0.7)    # lower tier top, × cliff height
GULLIES_PER_SQUARE = 0.5         # masses cut deep back into the band (re-entrants)…
GULLY_DEPTH_MM   = (5.0, 7.0)    # …reaching only this far out
BUTTRESSES_PER_SQUARE = 0.5      # masses standing far out, full height…
BUTTRESS_DEPTH_MM = (20.0, 25.0)
END_MM           = 4.0           # standard end section (REQ-RUN-3, S4): every seed's
END_DEPTH_MM     = 11.0          # cliff ends as a full-height band this deep, so tiles
                                 # meet end to end without a step

# 2 stack
KINDS = {  #        p     along (mm)     depth (mm)    height (mm)   bridge  tilt°
    'column': (0.34, (8.0, 14.0), (7.0, 12.0), (10.0, 18.0), 1.2, 8.0),
    'block':  (0.34, (10.0, 18.0), (8.0, 14.0), (7.0, 13.0), 1.2, 10.0),
    'slab':   (0.22, (14.0, 24.0), (8.0, 14.0), (4.5, 8.0), 5.0, 8.0),
    'odd':    (0.10, (6.0, 10.0), (6.0, 10.0), (5.0, 10.0), 2.0, 14.0),
    'monolith': (0.06, (8.0, 16.0), (7.0, 12.0), None, 1.2, 3.0),  # ground to top
}
STAGGER_MM       = 2.0         # a rock's top never lines up with a neighbour's within this…
STAGGER_SHARE    = 0.25        # …along more than this share of their shared edge
ZONE_SIZE        = (1.45, 0.6)  # rock size × this between zones / in a shattered zone
ZONE_SIZE_H_EXP  = 0.5           # …its height × that to this power
TIER_STEPS       = (1, 3)      # a mass's lower tier steps this many times along the run
TIER_STEP_MM     = 5.0         # …by up to this much
MIN_STEP_MM      = 6.0         # tier steps no shorter than this along the run…
MIN_STRIP_MM     = 4.0         # …and depth changes 0 or at least this
MIN_SIDE_MM      = 3.5         # a thinner footprint bridges over its neighbours instead
MIN_DEFICIT_MM   = 1.0         # cells closer than this to T count as full
MIN_H_MM         = 3.5         # no rock (or leftover above one) thinner than this
OVERSHOOT_MM     = (0.0, 2.5)  # tier rocks may stand above T (rough ledges)
TOP_TILT_DEG     = 6.0         # extra random tilt of each rock's top
SINK_MM          = 0.8         # resting rocks sink this far into their support (grain,
                               # chips and trims take that much off the rock below)
MIN_AREA_MM2     = 8.0         # smaller pockets are bridged by a rock over them
SNAP_MM          = 3.5         # footprints this close to a contact plane reach it
PUSH_P           = 0.3         # share of plane-touching rocks set back…
PUSH_MM          = (0.25, 0.6) # …by this much (≤ BEVEL_MM: the rock above steps back at 45°)
MAX_ROCKS        = 400
JUT_P            = 0.5        # a rock at the cliff front juts out past it…
JUT_MM           = (1.5, 5.0)  # …this far (a column or block standing proud)

# 3 finish
JOINT_OVERLAP_MM = 0.03        # rock footprints overlap their neighbours this much
BEVEL_MM         = 0.6         # 45° bevel under rocks resting on rocks
UNDERFILL_SLICES_MM = (0.05, 0.3, 0.7)  # the keel under a resting rock follows its base outline at these heights
CORNER_MM        = (0.8, 1.6)  # vertical corner chamfers: neighbours meet in a V seam
ZONES_PER_SQUARE = 1.5         # shattered zones (REQ-ROCK-3: detail comes in clusters)…
ZONE_SIGMA_MM    = (5.0, 10.0) # …this wide along the run
ZONE_FLOOR       = 0.25        # intensity between zones
#   cleaving: whole-plane cuts of each (still convex) rock — faces turn into
#   planes meeting at creases, never steps (REQ-ROCK-4)
BREAKS_PER_100MM2 = 1.2        # mean face breaks per 100 mm² of exposed face (full zone)…
BREAKS_MAX       = 5           # …at most this many per face
JOINT_MM         = (0.8, 1.4)  # a loaded rock's exposed top edge: 45° chamfer (horizontal V joint)
BREAK_TILT_DEG   = (12.0, 28.0)  # a face break turns the face this far about its crease…
BREAK_DEPTH_MM   = (1.0, 3.2)  # …taking a wedge at most this deep (between zones → full zone)
RIM_DEPTH_MM     = 0.8         # faces on the tile boundary: shallow breaks only
CREASE_LEAN_DEG  = 12.0        # crease lean off vertical (σ)…
CREASE_DIAG_P    = 0.25        # …or, this often, a diagonal crease…
CREASE_DIAG_DEG  = (30.0, 50.0)  # …at this lean
CREASE_FLAT_P    = 0.3         # on tall faces, this often a near-horizontal crease…
CREASE_FLAT_DEG  = (65.0, 85.0)  # …(breaks a column into blocks instead of strips)
CORNER_BREAK_P   = 0.6         # an exposed vertical corner is broken off…
CORNER_BREAK_MM  = (1.0, 3.0)  # …this deep
SHOULDER_P       = 0.6         # an exposed ledge top's front edge is broken…
SHOULDER_MM      = (0.8, 2.5)  # …this deep
DRESS_P          = 0.85        # an exposed face is dressed off true: turned in plan…
DRESS_YAW_DEG    = (3.0, 12.0) # …this much (the rock reads wedge-shaped, not boxed)…
DRESS_BATTER_DEG = (-3.0, 10.0)  # …and leaning back (taper) or a little forward
DRESS_MAX_MM     = 2.5         # …taking at most this off the face
BREAK_END_P      = (0.3, 0.3)  # a face break ends in a roof (undercut) / a shelf (ledge)
BREAK_ROOF_DEG   = (50.0, 70.0)  # roof slope back from the break line (≥ 50°: printable)
BREAK_SHELF_DEG  = (10.0, 40.0)
CLEAVE_MAX_LOSS  = 0.15        # one cut takes at most this share of the rock…
CLEAVE_MIN_KEEP  = 0.6         # …and all of them together leave at least this
CLEAVE_SUPPORT_MM3 = 1.0      # a cut may take this much from under a rock resting here
#   fissures: V wedges cut across the assembled face (REQ-ROCK-2, FDM-3)
FISSURES_PER_SQUARE = (2.0, 4.0)   # (major, minor) at full intensity
MAJOR_DEPTH_MM   = (1.8, 3.5)
MAJOR_LEN_MM     = (9.0, 24.0)
MINOR_DEPTH_MM   = (0.8, 1.6)
MINOR_LEN_MM     = (3.0, 9.0)
DEEP_FISSURES    = (1, 2)          # per square, the first majors go deep and dark…
DEEP_DEPTH_MM    = (3.5, 5.0)
DEEP_HALF_DEG    = (12.0, 16.0)
FISSURE_HALF_DEG = (8.0, 14.0)     # V half-angle…
FISSURE_MOUTH_MM = 0.6             # …but the mouth never narrower than this
FISSURE_LEAN_DEG = 14.0            # lean off vertical (σ)…
FISSURE_DIAG_P   = 0.25            # …or, this often, diagonal…
FISSURE_DIAG_DEG = (28.0, 40.0)    # …at this lean (≤ 40°: the V's upper wall stays printable)
BRANCH_P         = 0.5             # a major fissure sends off a minor branch
MAJOR_SEGMENTS   = (2, 3)          # a major fissure bends this many times…
BEND_DEG         = (10.0, 25.0)    # …by this much each
FISSURE_CAP_MM   = 2.0             # wedges reach no further than this in front of the face
FISSURE_RAY_MM   = 8.0             # bends are found on the surface within this of the face
FISSURE_END_DEG  = 55.0            # end walls slope back this far from the fissure line
FISSURE_TOP_GAP_MM = 2.0           # fissures stop this far under the top plane
FISSURE_GAP_MM   = (6.0, 3.5)      # sites at least this far from earlier ones (major, minor)

# 4 assembly
APRON_H_MM       = (1.5, 3.5)  # a dirt bank in the angle of cliff and ground (between
APRON_R_MM       = (3.0, 6.5)  # zones → shattered zone): this high at the face, this far out
APRON_END        = (2.5, 4.5)  # (height, reach) at the tile ends, same for every seed (S4)
GROUND_END_MM    = 3.0         # the dirt texture fades flat this close to the tile ends
APRON_EXP        = 1.6         # profile: concave, like a talus foot
GRIT_PER_SQUARE  = 45          # small stones half-buried in the bank…
GRIT_D_MM        = (1.0, 2.2)
GRIT_BURY        = (0.2, 0.5)  # …sunk this × D
SCREE_PER_SQUARE = 55          # fallen stones along each square of cliff foot…
SCREE_D_MM       = (1.2, 8.0)  # …sizes mixed, skewed small (no scale-marker carpet)…
SCREE_D_EXP      = 1.5
SCREE_REACH_MM   = 4.0         # …lying mostly this close to the face (exponential mean)…
SCREE_LEAN       = 0.35        # …the nearest leaning in, up to this × D into the face
SCREE_MOUTH      = (3, 6)      # extra stones under each major fissure's mouth
SCREE_BLOCKS     = 2.5         # big fallen blocks per square, against the face…
SCREE_BLOCK_MM   = (5.0, 8.0)  # …this size
SCREE_GAP        = 0.38        # centres at least this × (Di + Dj) apart
SCREE_TILT_DEG   = 8.0
SCREE_FAMILIES   = {'block': 0.45, 'wedge': 0.35, 'slab': 0.2}  # angular fragments, no domes
SCREE_SINK_MM    = 0.3         # into the dirt (plus whatever the tilt lifts)
SCREE_FRONT_MM   = 6.0         # none closer than this to the tile's front edge
CAP_DROP_MM      = 1.0         # dirt top sits this far under the top plane…
CAP_EDGE_MM      = 3.0         # …held back this far from the cliff edge


def to_mesh(man):
    o = man.to_mesh()
    return trimesh.Trimesh(np.asarray(o.vert_properties)[:, :3], np.asarray(o.tri_verts),
                           process=False)


# ── 1 Form ──────────────────────────────────────────────────────────────────
@dataclass
class Plane:
    """A contact plane: rocks touch it but never cross it.  `n` points out
    of the volume."""
    p: np.ndarray
    n: np.ndarray


@dataclass
class Form:
    W: float
    H: float
    T: np.ndarray            # target height per grid cell (FOOT_Z_MM = nothing)
    planes: list
    ztop: float | None       # the top contact plane, if any


def _segments(rng, length):
    """Cut [0, length] into masses of MASS_W_MM."""
    xs = [0.0]
    while xs[-1] < length:
        xs.append(xs[-1] + rng.uniform(*MASS_W_MM))
    xs[-1] = length
    if len(xs) > 2 and xs[-1] - xs[-2] < MASS_W_MM[0] * 0.5:
        del xs[-2]
    return xs


def make_form(rng, form, W, H):
    nx, ny = int(round(W / GRID_MM)), int(round(H / GRID_MM))
    x = (np.arange(nx) + 0.5) * GRID_MM
    y = (np.arange(ny) + 0.5) * GRID_MM
    X, Y = np.meshgrid(x, y, indexing='ij')
    zt = FOOT_Z_MM + CLIFF_H_MM
    T = np.full((nx, ny), FOOT_Z_MM)
    if form != 'run':
        raise ValueError(form)
    back = H - Y                                     # distance from the back plane
    xs = _segments(rng, W)
    # big plan forms: a few masses cut deep back (gullies), a few standing
    # far out (buttresses) — never the end masses (standard end section)
    inner = list(rng.permutation(np.arange(1, len(xs) - 2))) if len(xs) > 3 else []
    n_g = int(rng.poisson(GULLIES_PER_SQUARE * W / SQUARE_MM) or 1)
    n_b = int(rng.poisson(BUTTRESSES_PER_SQUARE * W / SQUARE_MM) or 1)
    role = {k: 'gully' for k in inner[:n_g]}
    role.update({k: 'buttress' for k in inner[n_g:n_g + n_b]})
    for si_, (a, b) in enumerate(zip(xs[:-1], xs[1:])):
        d_all = rng.uniform(*DEPTH_MM)
        d_top = d_all if rng.random() < FULL_P else rng.uniform(*TOP_DEPTH_MM)
        if role.get(si_) == 'gully':
            d_all = d_top = rng.uniform(*GULLY_DEPTH_MM)
        elif role.get(si_) == 'buttress':
            d_all = d_top = rng.uniform(*BUTTRESS_DEPTH_MM)
        if d_top < d_all:                            # no tier thinner than a rock
            d_all = max(d_all, d_top + MIN_STRIP_MM)
        z_mid = FOOT_Z_MM + rng.uniform(*TIER_Z) * CLIFF_H_MM
        # the lower tier steps along the run (no ledge runs level for long),
        # in steps no shorter than MIN_STEP_MM, its depth changing by 0 or ≥
        # MIN_STRIP_MM — anything finer is filled by sliver rocks (organ pipes)
        k = int(rng.integers(TIER_STEPS[0], TIER_STEPS[1] + 1))
        k = max(1, min(k, int((b - a) // MIN_STEP_MM)))
        cuts = []
        if k > 1:
            free = (b - a) - k * MIN_STEP_MM
            cuts = list(a + MIN_STEP_MM * np.arange(1, k)
                        + np.sort(rng.uniform(0, free, k - 1)))
        for a2, b2 in zip([a, *cuts], [*cuts, b]):
            sel = (X >= a2) & (X < b2)
            z2 = np.clip(z_mid + rng.uniform(-TIER_STEP_MM, TIER_STEP_MM),
                         FOOT_Z_MM + MIN_H_MM + 1.0, zt - MIN_H_MM - 1.0)
            d2 = d_all
            if d_top < d_all and rng.random() < 0.5:
                d2 = d_all + rng.choice((-1.0, 1.0)) * rng.uniform(MIN_STRIP_MM, 1.5 * MIN_STRIP_MM)
                if d2 < d_top + MIN_STRIP_MM:
                    d2 = d_all + abs(d2 - d_all)
            T[sel & (back <= d2)] = z2
        sel = (X >= a) & (X < b)
        T[sel & (back <= d_top)] = zt
    ends = (X < END_MM) | (X > W - END_MM)
    T[ends] = FOOT_Z_MM
    T[ends & (back <= END_DEPTH_MM)] = zt
    planes = [Plane(np.array([0, 0, 0.]), np.array([-1, 0, 0.])),
              Plane(np.array([W, 0, 0.]), np.array([1, 0, 0.])),
              Plane(np.array([0, H, 0.]), np.array([0, 1, 0.])),
              Plane(np.array([0, 0, zt]), np.array([0, 0, 1.]))]
    return Form(W, H, T, planes, zt)


# ── 2 Stack ─────────────────────────────────────────────────────────────────
@dataclass
class Placed:
    cells: np.ndarray         # bool grid: footprint
    z0: float                 # underside (lowest support)
    top: tuple                # top plane z = c + a·x + b·y
    kind: str
    D: float                  # size, for the finishing passes


def stack(rng, form, zone=None):
    T = form.T
    nx, ny = T.shape
    X, Y = np.meshgrid((np.arange(nx) + 0.5) * GRID_MM, (np.arange(ny) + 0.5) * GRID_MM,
                       indexing='ij')
    S = np.full_like(T, FOOT_Z_MM - 1.0)
    want = T > FOOT_Z_MM
    rocks = []
    kinds = list(KINDS)
    probs = np.array([KINDS[k][0] for k in kinds])
    probs /= probs.sum()
    noise = rng.random(T.shape) * 1.5
    for _ in range(MAX_ROCKS):
        deficit = np.where(want, T - S, 0.0)
        open_ = deficit > MIN_DEFICIT_MM
        if not open_.any():
            break
        key = np.where(open_, S + noise, np.inf)
        si, sj = np.unravel_index(np.argmin(key), key.shape)
        site = np.array([X[si, sj], Y[si, sj]])
        kind = kinds[rng.choice(len(kinds), p=probs)]
        is_top = form.ztop is not None and T[si, sj] >= form.ztop - 1e-6
        if kind == 'monolith' and not (is_top and S[si, sj] <= FOOT_Z_MM):
            kind = 'column'                           # monoliths stand on the ground
        _, al, dp, hr, bridge, tilt = KINDS[kind]
        w, d = rng.uniform(*al), rng.uniform(*dp)
        sz = 1.0
        if zone is not None:              # big quiet masses, small shattered blocks
            u = (float(zone(site[0])[0]) - ZONE_FLOOR) / (1 - ZONE_FLOOR)
            sz = ZONE_SIZE[0] + (ZONE_SIZE[1] - ZONE_SIZE[0]) * u
            w *= sz
        cells = None
        for attempt in range(4):
            cells = _grow(rng, X, Y, S, T, want, site, (si, sj), w, d,
                          S[si, sj] + bridge + 3.0 * attempt, form)
            ext = np.nonzero(cells.any(1))[0], np.nonzero(cells.any(0))[0]
            side = min((e.max() - e.min() + 1) * GRID_MM for e in ext) if cells.any() else 0.0
            if cells.sum() * GRID_MM ** 2 >= MIN_AREA_MM2 and side >= MIN_SIDE_MM:
                break
        if cells.sum() == 0:
            S[si, sj] = T[si, sj]
            continue
        if rng.random() < JUT_P:
            cells = _jut(rng, cells, want, S)
        Sv, Xv, Yv = S[cells], X[cells], Y[cells]
        # rest on the highest supports: fitted slope (capped), raised to touch
        A = np.c_[np.ones_like(Xv), Xv - Xv.mean(), Yv - Yv.mean()]
        coef = np.linalg.lstsq(A, Sv, rcond=None)[0] if len(Sv) > 3 else np.zeros(3)
        g = np.hypot(coef[1], coef[2])
        gmax = np.tan(np.radians(tilt))
        if g > gmax:
            coef[1:] *= gmax / g
        t2 = np.radians(TOP_TILT_DEG) * rng.standard_normal(2) * 0.6
        a, b = coef[1] + t2[0], coef[2] + t2[1]
        bed = Sv - a * (Xv - Xv.mean()) - b * (Yv - Yv.mean())
        c = bed.max()                                 # bed plane at the footprint centre
        tgt = T[si, sj] - (c + a * (X[si, sj] - Xv.mean()) + b * (Y[si, sj] - Yv.mean()))
        def finish(h):
            if tgt - h < MIN_H_MM:                    # too little left above: reach T
                h = tgt + 1.0 if is_top else tgt + rng.uniform(*OVERSHOOT_MM)
            return max(h, MIN_H_MM)

        # height follows size too (more gently): wide masses stay masses, not
        # squat slabs stacked like masonry
        h = finish(tgt + 1.0 if hr is None else rng.uniform(*hr) * sz ** ZONE_SIZE_H_EXP)
        # stagger: a rock stopping short of T must not end level with its
        # neighbours along their shared edge — that is how courses appear
        ring = (ndimage.binary_dilation(cells, iterations=2) & ~cells
                & (S > FOOT_Z_MM + 0.5))
        for _ in range(4):
            if not ring.any() or tgt - h < MIN_H_MM:
                break
            top_ring = c + h + a * (X[ring] - Xv.mean()) + b * (Y[ring] - Yv.mean())
            if (np.abs(top_ring - S[ring]) < STAGGER_MM).mean() <= STAGGER_SHARE:
                break
            h = finish(h + rng.choice((-1.0, 1.0)) * rng.uniform(STAGGER_MM, 2.5 * STAGGER_MM))
        c_top = c + h - a * Xv.mean() - b * Yv.mean()
        topz = c_top + a * X + b * Y
        if form.ztop is not None:
            topz = np.minimum(topz, form.ztop)
        S = np.where(cells, np.maximum(S, topz), S)
        z0 = float(Sv.min()) - (SINK_MM if Sv.min() > FOOT_Z_MM else 0.0)
        rocks.append(Placed(cells, z0, (c_top, a, b), kind,
                            float(np.median([w, d, h]))))
    return rocks, S


def _grow(rng, X, Y, S, T, want, site, ij, w, d, zmax, form):
    """The footprint: an axis-aligned rectangle grown from the site one row
    at a time, only into cells no higher than `zmax` that still need
    filling, toward a w × d target; a side that ends within SNAP_MM of the
    pocket's edge (a neighbour or a contact plane) runs on to it, so the
    leftover pockets stay rectangular and never become slivers."""
    ok = want & (S <= zmax) & (T - S > 0.3)
    nx, ny = ok.shape
    i0, i1, j0, j1 = ij[0], ij[0] + 1, ij[1], ij[1] + 1
    goal = {'x': w / GRID_MM, 'y': d / GRID_MM}
    snap = SNAP_MM / GRID_MM
    for limit in ('goal', 'snap'):
        for side in rng.permutation(4):
            while True:
                if side == 0 and i0 > 0 and ok[i0 - 1, j0:j1].all():
                    pass
                elif side == 1 and i1 < nx and ok[i1, j0:j1].all():
                    pass
                elif side == 2 and j0 > 0 and ok[i0:i1, j0 - 1].all():
                    pass
                elif side == 3 and j1 < ny and ok[i0:i1, j1].all():
                    pass
                else:
                    break
                span = (i1 - i0) if side < 2 else (j1 - j0)
                cap = goal['x' if side < 2 else 'y']
                if limit == 'snap':
                    # run on only if the pocket ends within the snap distance
                    k = 0
                    while k <= snap:
                        if side == 0:
                            free = i0 - 1 - k >= 0 and ok[i0 - 1 - k, j0:j1].all()
                        elif side == 1:
                            free = i1 + k < nx and ok[i1 + k, j0:j1].all()
                        elif side == 2:
                            free = j0 - 1 - k >= 0 and ok[i0:i1, j0 - 1 - k].all()
                        else:
                            free = j1 + k < ny and ok[i0:i1, j1 + k].all()
                        if not free:
                            break
                        k += 1
                    if k > snap:
                        break
                elif span >= cap:
                    break
                if side == 0:
                    i0 -= 1
                elif side == 1:
                    i1 += 1
                elif side == 2:
                    j0 -= 1
                else:
                    j1 += 1
    cells = np.zeros_like(ok)
    cells[i0:i1, j0:j1] = True
    return cells


def _jut(rng, cells, want, S):
    """Extend a footprint that ends at the cliff front (−y, the open side)
    forward over bare ground."""
    jj = np.nonzero(cells.any(0))[0]
    ii = np.nonzero(cells.any(1))[0]
    j0 = jj.min()
    k = int(round(rng.uniform(*JUT_MM) / GRID_MM))
    if j0 - k < 0:
        return cells
    front = np.s_[ii.min():ii.max() + 1, j0 - k:j0]
    if want[front].any() or (S[front] > FOOT_Z_MM).any():
        return cells
    out = cells.copy()
    out[front] = True
    return out


# ── 3 Finish ────────────────────────────────────────────────────────────────
def footprint_polygon(cells):
    ii, jj = np.nonzero(cells)
    sq = [box(i * GRID_MM, j * GRID_MM, (i + 1) * GRID_MM, (j + 1) * GRID_MM)
          for i, j in zip(ii, jj)]
    poly = shapely.union_all(sq).buffer(0.01).simplify(0.2)
    if poly.geom_type == 'MultiPolygon':
        poly = max(poly.geoms, key=lambda g: g.area)
    return shapely.geometry.Polygon(poly.exterior)


def occupied(form, S, P):
    """Is each point inside the finished pile (or below the ground)?"""
    P = np.atleast_2d(P)
    i = np.floor(P[:, 0] / GRID_MM).astype(int)
    j = np.floor(P[:, 1] / GRID_MM).astype(int)
    inside = (i >= 0) & (j >= 0) & (i < S.shape[0]) & (j < S.shape[1])
    out = np.zeros(len(P), bool)
    out[inside] = P[inside, 2] < S[i[inside], j[inside]] - 0.05
    if form.ztop is not None:                    # under the dirt top: covered
        out |= P[:, 2] > form.ztop - 0.1
    return out


def rock_manifold(rng, r, form, S, support, zone=1.0):
    # neighbours overlap by a hair instead of touching face to face: exact
    # tangent faces survive the union but pinch on float32 STL reload
    poly = footprint_polygon(r.cells).buffer(JOINT_OVERLAP_MM, join_style=2)
    cs = m3d.CrossSection([np.asarray(shapely.geometry.polygon.orient(poly, 1.0)
                                      .exterior.coords)[:-1]])
    c, a, b = r.top
    xb = np.array(poly.bounds)
    zhi = max(c + a * x + b * y for x in xb[[0, 2]] for y in xb[[1, 3]]) + 1.0
    rock = m3d.Manifold.extrude(cs, zhi - r.z0).translate([0, 0, r.z0])
    an = np.array([-a, -b, 1.0])
    na = np.linalg.norm(an)
    rock = rock.trim_by_plane((-an / na).tolist(), -c / na)            # z ≤ c + a x + b y
    if form.ztop is not None:
        rock = rock.trim_by_plane([0, 0, -1], -form.ztop)
    if rock.is_empty():
        return None
    # some rocks touching a side plane sit back from it a little
    for pl in form.planes:
        if abs(pl.n[2]) > 0.5 or abs(pl.n[0]) > 0.5:   # ends stay flush (S4)
            continue
        reach = max((np.array(v) - pl.p[:2]) @ pl.n[:2] for v in poly.exterior.coords)
        if reach > -0.1 and rng.random() < PUSH_P:
            off = rng.uniform(*PUSH_MM)
            cut = rock.trim_by_plane((-pl.n).tolist(), -(pl.p @ pl.n - off))
            if not cut.is_empty():
                rock = cut
    rock = _chamfer_corners(rng, rock, poly)
    zb = rock.bounding_box()[2]
    if zb > FOOT_Z_MM:                       # sits on rock: 45° bevel under it
        rock = _bevel_bottom(rock, poly, zb, lambda P: occupied(form, S, P))
    rock = _cleave(rng, rock, poly, r, form, S, support, zone)
    rock = rock.translate([0, 0, -zb])
    D = max(r.D, 4.0)
    saved = _rl.FACET_MIN_NZ
    try:
        rock, _ = _rl.chip_corners_edges(rng, rock, D)
        rock = _rl.texture(rng, rock)
    finally:
        _rl.FACET_MIN_NZ = saved
    rock = rock.translate([0, 0, zb])
    if zb > FOOT_Z_MM:                       # sits on rock: embed a 45° keel in it
        rock = rock + _underfill(rock, zb)
    return rock


def _underfill(rock, zb):
    """A 45° keel under a resting rock, sunk into the rock below: whatever
    the finishing took off that rock's top edge (chips, grain) opens as a V
    seam under this one instead of a flat ceiling.  Its outline is the hull
    of the rock's base (sliced at a few heights: chips and bevels make the
    base narrower than the rock in places); every side is a 45° plane
    through an outline edge, so the keel is a roof with no flatter face."""
    pts = [np.vstack(pp) for dz in UNDERFILL_SLICES_MM
           if (pp := rock.slice(zb + dz).to_polygons())]
    if not pts:
        return m3d.Manifold()
    poly = shapely.MultiPoint(np.vstack(pts)).convex_hull.simplify(0.05)
    if poly.geom_type != 'Polygon':
        return m3d.Manifold()
    poly = shapely.geometry.polygon.orient(poly, 1.0)
    zk = zb + UNDERFILL_SLICES_MM[0]
    x0, y0, x1, y1 = poly.bounds
    depth = 0.5 * min(x1 - x0, y1 - y0) + 1.0
    keel = m3d.Manifold.cube([x1 - x0 + 2, y1 - y0 + 2, depth]).translate(
        [x0 - 1, y0 - 1, zk - depth])
    s = np.sqrt(0.5)
    xy = np.asarray(poly.exterior.coords)
    for p0, p1 in zip(xy[:-1], xy[1:]):
        e = p1 - p0
        if np.hypot(*e) < 1e-6:
            continue
        m = np.array([e[1], -e[0]]) / np.hypot(*e)          # outward (ccw ring)
        n = np.array([m[0] * s, m[1] * s, -s])              # 45° plane through the edge
        keel = keel.trim_by_plane((-n).tolist(), -float(n @ [p0[0], p0[1], zk]))
    return keel


def _support_cone(r):
    """The 45° wedge under a resting rock: its footprint at its underside,
    narrowing at 45° all the way to a ridge — no flat bottom, so whatever
    the facets leave beneath a rock is a printable slope."""
    poly = footprint_polygon(r.cells)
    lo, hi = 0.0, 0.5 * min(poly.bounds[2] - poly.bounds[0], poly.bounds[3] - poly.bounds[1])
    for _ in range(12):                                    # deepest non-empty inset
        mid = 0.5 * (lo + hi)
        inner = poly.buffer(-mid, join_style=2)
        lo, hi = (mid, hi) if (not inner.is_empty and inner.area > 1e-3) else (lo, mid)
    inner = poly.buffer(-lo, join_style=2)
    rings = [np.c_[np.asarray(poly.exterior.coords), np.full(len(poly.exterior.coords), r.z0)]]
    for g in getattr(inner, 'geoms', [inner]):
        c = np.asarray(g.exterior.coords)
        rings.append(np.c_[c, np.full(len(c), r.z0 - lo)])
    return m3d.Manifold.hull_points(np.vstack(rings).astype(np.float32))


def _chamfer_corners(rng, rock, poly):
    """Each vertical corner of the footprint cut by a vertical plane along
    its bisector — where four rocks meet the seam opens into a V."""
    xy = np.asarray(shapely.geometry.polygon.orient(poly, 1.0).exterior.coords)[:-1]
    for k in range(len(xy)):
        a, b, c = xy[k - 1], xy[k], xy[(k + 1) % len(xy)]
        e1, e2 = b - a, c - b
        if np.cross(e1, e2) <= 0 or min(np.hypot(*e1), np.hypot(*e2)) < 2.0:
            continue                                     # reflex or tiny: leave it
        m = np.array([e1[1], -e1[0]]) / np.hypot(*e1) + np.array([e2[1], -e2[0]]) / np.hypot(*e2)
        m /= np.linalg.norm(m)                          # outward bisector
        n = np.array([m[0], m[1], 0.0])
        d = float(m @ b) - rng.uniform(*CORNER_MM)
        cut = rock.trim_by_plane((-n).tolist(), -d)
        if not cut.is_empty():
            rock = cut
    return rock


def _bevel_bottom(rock, poly, zb, occ):
    """Each exposed bottom edge cut back at 45° (a printable half of the V
    joint the rock beneath completes with its own top bevel).  Edges against
    another rock stay square, so no hidden voids."""
    s = np.sqrt(0.5)
    xy = np.asarray(shapely.geometry.polygon.orient(poly, 1.0).exterior.coords)
    for p0, p1 in zip(xy[:-1], xy[1:]):
        e = p1 - p0
        if np.hypot(*e) < 1.0:
            continue
        m = np.array([e[1], -e[0]]) / np.hypot(*e)          # outward (ccw ring)
        mid = 0.5 * (p0 + p1) + m * 0.4
        if occ([[mid[0], mid[1], zb + 0.4]])[0]:
            continue
        n = np.array([m[0] * s, m[1] * s, -s])
        q = np.array([p0[0] - m[0] * BEVEL_MM, p0[1] - m[1] * BEVEL_MM, zb])
        cut = rock.trim_by_plane((-n).tolist(), -float(n @ q))
        if not cut.is_empty():
            rock = cut
    return rock


def _cleave(rng, rock, poly, r, form, S, support, zone):
    """Break the rock with whole planes while it is still convex: face
    breaks (a crease across an exposed face, the face turned 12–28° about
    it, so one face becomes two planes), broken-off vertical corners, and
    broken shoulders along the front of a ledge top.  A plane cut of a
    convex body leaves only that plane, so every edge is a real change of
    plane (REQ-ROCK-4); some face breaks stop partway at a sloped roof (an
    undercut) or a shelf (a short ledge), every surface still ≥ 45°.  `zone` (0–1) is the local
    shattered-zone intensity (REQ-ROCK-3)."""
    V = to_mesh(rock).vertices
    zlo, zhi = V[:, 2].min(), V[:, 2].max()
    hgt = zhi - zlo
    if hgt < 2.0:
        return rock
    z = np.array([0, 0, 1.0])
    xy = np.asarray(shapely.geometry.polygon.orient(poly, 1.0).exterior.coords)
    zs = zlo + hgt * np.array([0.2, 0.5, 0.8])
    faces = []
    for p0, p1 in zip(xy[:-1], xy[1:]):
        e = p1 - p0
        if np.hypot(*e) < 2.0:
            continue
        m2 = np.array([e[1], -e[0]]) / np.hypot(*e)          # outward (ccw ring)
        mid = 0.5 * (p0 + p1) + 0.5 * m2
        rim = not (0 < mid[0] < form.W and 0 < mid[1] < form.H)
        if rim or occupied(form, S, np.c_[np.tile(mid, (3, 1)), zs]).sum() <= 1:
            faces.append((p0, p1, np.array([m2[0], m2[1], 0.0]), rim))
    cuts = []                                       # (normal, point on plane, max depth)
    for p0, p1, m, rim in faces:                    # face breaks
        h = np.cross(m, z)
        area = np.hypot(*(p1 - p0)) * hgt
        lam = BREAKS_PER_100MM2 * (0.4 + 0.6 * zone) * area / 100.0
        for _ in range(min(int(rng.poisson(lam)), BREAKS_MAX)):
            if not rim and hgt > 1.5 * np.hypot(*(p1 - p0)) and rng.random() < CREASE_FLAT_P:
                th = rng.choice((-1, 1)) * rng.uniform(*CREASE_FLAT_DEG)
            elif not rim and rng.random() < CREASE_DIAG_P:
                th = rng.choice((-1, 1)) * rng.uniform(*CREASE_DIAG_DEG)
            else:
                th = np.clip(rng.normal(0.0, CREASE_LEAN_DEG), -25.0, 25.0)
            th = np.radians(th)
            d = np.cos(th) * z + np.sin(th) * h               # the crease
            e = np.cross(d, m)
            al = np.radians(rng.uniform(*BREAK_TILT_DEG))
            n = m * np.cos(al) + e * np.sin(al) * rng.choice((-1, 1))
            if n[2] < -0.5:                                   # keep it printable
                n = 2 * m * np.cos(al) - n
            pxy = p0 + rng.uniform(0.2, 0.8) * (p1 - p0)
            p = np.array([pxy[0], pxy[1], zlo + hgt * rng.uniform(0.2, 0.8)])
            lim = RIM_DEPTH_MM if rim else (BREAK_DEPTH_MM[0]
                                            + (BREAK_DEPTH_MM[1] - BREAK_DEPTH_MM[0]) * zone)
            ends = []                     # the break stops partway: a roof or a shelf
            u = rng.random()
            q = p + d * hgt * rng.uniform(-0.3, 0.3)
            if not rim and u < BREAK_END_P[0]:
                ph = np.radians(rng.uniform(*BREAK_ROOF_DEG))
                ends = [(-d * np.cos(ph) + m * np.sin(ph), q)]
            elif not rim and u < sum(BREAK_END_P):
                ph = np.radians(rng.uniform(*BREAK_SHELF_DEG))
                ends = [(d * np.cos(ph) + m * np.sin(ph), q)]
            cuts.append((n, p, lim, ends))
    for (a0, a1, ma, ra), (b0, b1, mb, rb) in zip(faces, faces[1:] + faces[:1]):
        if ra or rb or np.hypot(*(a1 - b0)) > 1e-6 or rng.random() > CORNER_BREAK_P * zone:
            continue                                  # broken-off vertical corners
        n = ma + mb
        n = n / np.linalg.norm(n)
        sp = np.radians(rng.uniform(-20, 20))
        n = np.array([n[0] * np.cos(sp) - n[1] * np.sin(sp),
                      n[0] * np.sin(sp) + n[1] * np.cos(sp), rng.uniform(-0.25, 0.35)])
        n /= np.linalg.norm(n)
        dep = rng.uniform(*CORNER_BREAK_MM)
        cuts.append((n, np.array([a1[0], a1[1], zlo + 0.5 * hgt]) - n * dep, dep + 1.0, []))
    cx, cy = poly.centroid.coords[0]
    loaded = occupied(form, S, [[cx, cy, zhi + 0.4]])[0]
    if loaded and (form.ztop is None or zhi < form.ztop - 0.5):
        c, a, b = r.top                               # horizontal V joint under the next rock
        for p0, p1, m, rim in faces:
            if rim:
                continue
            n = (m + z) / np.sqrt(2.0)
            q = 0.5 * (p0 + p1)
            dep = rng.uniform(*JOINT_MM)
            cuts.append((n, np.array([q[0], q[1], c + a * q[0] + b * q[1]]) - n * dep,
                         dep + 0.5, []))
    if (form.ztop is None or zhi < form.ztop - 0.5) and not loaded:
        c, a, b = r.top                               # an open ledge top: broken shoulders
        for p0, p1, m, rim in faces:
            if rim or rng.random() > SHOULDER_P * zone:
                continue
            n = m + z * rng.uniform(0.6, 1.6)
            n /= np.linalg.norm(n)
            q = 0.5 * (p0 + p1)
            dep = rng.uniform(*SHOULDER_MM)
            cuts.append((n, np.array([q[0], q[1], c + a * q[0] + b * q[1]]) - n * dep,
                         dep + 1.0, []))
    dress = []                       # faces turned off true in plan, battered back
    for p0, p1, m, rim in faces:
        L = np.hypot(*(p1 - p0))
        if rim or L < 3.0 or rng.random() > DRESS_P:
            continue
        piv, far = (p0, p1) if rng.random() < 0.5 else (p1, p0)
        t = np.r_[(far - piv) / L, 0.0]
        psi = np.radians(min(rng.uniform(*DRESS_YAW_DEG),
                             np.degrees(np.arcsin(min(DRESS_MAX_MM / L, 1.0)))))
        nh = m * np.cos(psi) + t * np.sin(psi)            # recedes toward `far`
        be = np.radians(rng.uniform(*DRESS_BATTER_DEG))
        n = nh * np.cos(be) + z * np.sin(be)
        P = np.array([piv[0], piv[1], zlo + hgt * rng.uniform(0.0, 0.3)])
        dress.append((n, P, DRESS_MAX_MM, []))
    vol0 = rock.volume()
    for n, p, lim, ends in dress + [cuts[i] for i in rng.permutation(len(cuts))]:
        depth = (to_mesh(rock).vertices @ n).max() - n @ p
        if depth < 0.3:
            continue
        if depth > lim:                               # shallower, same angle
            p = p + n * (depth - lim)
        if ends:                                      # a convex piece out of the face
            bb = np.array(rock.bounding_box())
            piece = m3d.Manifold.cube((bb[3:] - bb[:3] + 2).tolist()).translate(
                (bb[:3] - 1).tolist())
            for nn, qq in [(n, p), *ends]:
                piece = piece.trim_by_plane(nn.tolist(), float(nn @ qq))
            cut = rock - piece
        else:
            cut = rock.trim_by_plane((-n).tolist(), -float(n @ p))
        if cut.is_empty():
            continue
        v = cut.volume()
        if v < (1 - CLEAVE_MAX_LOSS) * rock.volume() or v < CLEAVE_MIN_KEEP * vol0:
            continue
        if not support.is_empty() and ((rock - cut) ^ support).volume() > CLEAVE_SUPPORT_MM3:
            continue                                  # would undercut a rock resting here
        rock = cut
    return rock


# ── 4 Assembly ──────────────────────────────────────────────────────────────
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
    f = make_form(rng, form, W, H)
    zone = make_zones(rng, W)
    placed, S = stack(rng, f, zone)
    tm['stack'] = time.perf_counter() - t0
    t1 = time.perf_counter()
    cones = [_support_cone(r) for r in placed if r.z0 > FOOT_Z_MM]
    support = m3d.Manifold.batch_boolean(cones, m3d.OpType.Add) if cones else m3d.Manifold()
    rocks = [m for m in (rock_manifold(rng, r, f, S, support,
                                       float(zone(footprint_polygon(r.cells).centroid.x)[0]))
                         for r in placed) if m is not None]
    tm['rocks'] = time.perf_counter() - t1
    t2 = time.perf_counter()
    pile = m3d.Manifold.batch_boolean(rocks, m3d.OpType.Add)
    wedges, mouths = fissures(rng, pile, f, zone)
    if not wedges.is_empty():
        pile = pile - wedges
    tm['fissures'] = time.perf_counter() - t2
    t2 = time.perf_counter()
    ground, gz = foot_ground(f, S, zone, seed)
    parts = [pile, ground, *scree(rng, f, S, zone, mouths, gz)]
    tm['scree'] = time.perf_counter() - t2
    if f.ztop is not None:
        parts.append(_cap(rng, f, S, seed))
    top_ = m3d.Manifold.batch_boolean(parts, m3d.OpType.Add)
    top_ = top_ ^ m3d.Manifold.cube([W, H, 200.0])
    import clifflab as cl
    mesh, peg, zmax, _ = cl.attach_base(top_, cols, rows)
    tm['assemble'] = time.perf_counter() - t2
    tm['total'] = time.perf_counter() - t0
    return Built(mesh, len(rocks), len(placed), peg, zmax, tm)


def foot_ground(f, S, zone, seed):
    """The dirt ground with a bank in the angle of cliff and ground: highest
    against the face, falling away concave, higher and wider in the
    shattered zones; the same at the tile ends for every seed (S4).
    Returns (solid, absolute height grid (y, x))."""
    from dharmatiles.core.mesh import make_heightmap_solid
    relief = _dirt.dirt_relief(seed, f.W, f.H).relief           # (y, x)
    rocky = S > FOOT_Z_MM + 0.5                                  # (x, y)
    dist = ndimage.distance_transform_edt(~rocky) * GRID_MM
    xs = (np.arange(S.shape[0]) + 0.5) * GRID_MM
    u = (zone(xs) - ZONE_FLOOR) / (1 - ZONE_FLOOR)
    e = np.clip(np.minimum(xs, f.W - xs) / (2 * END_MM), 0, 1)   # 0 at the ends
    e = e * e * (3 - 2 * e)
    hh = APRON_END[0] + e * (APRON_H_MM[0] + (APRON_H_MM[1] - APRON_H_MM[0]) * u - APRON_END[0])
    rr = APRON_END[1] + e * (APRON_R_MM[0] + (APRON_R_MM[1] - APRON_R_MM[0]) * u - APRON_END[1])
    bank = hh[:, None] * np.clip(1 - dist / rr[:, None], 0, 1) ** APRON_EXP
    xr = (np.arange(relief.shape[1]) + 0.5) * f.W / relief.shape[1]
    fade = np.clip(np.minimum(xr, f.W - xr) / GROUND_END_MM, 0, 1)
    fade = fade * fade * (3 - 2 * fade)               # dirt meets the next tile level
    gz = FOOT_Z_MM + relief * fade[None, :] + _resample(bank.T, relief.shape)
    return _gl.to_manifold(make_heightmap_solid(gz, f.W, f.H, base_h=0.0)), gz


def scree(rng, f, S, zone, mouths, gz):
    """Fallen stones of the same rock along the foot (REQ-FOOT-1): rocklab
    stones 1–8 mm, mixed and skewed small, densest against the face, in
    the shattered zones and under fissure mouths, thinning toward the
    front.  Each sits on the dirt where it lands, sunk past any lift its
    tilt gives its base, so no base edge shows as a ceiling.  Grit — small
    stones half-buried — fills the bank against the face."""
    ry, rx = gz.shape

    def ground_min(x, y, r):
        i0, i1 = (int(np.clip(v / f.W * rx, 0, rx - 1)) for v in (x - r, x + r))
        j0, j1 = (int(np.clip(v / f.H * ry, 0, ry - 1)) for v in (y - r, y + r))
        return float(gz[j0:j1 + 1, i0:i1 + 1].min())

    nx = S.shape[0]
    rocky = S > FOOT_Z_MM + 0.5
    front = np.where(rocky.any(1), rocky.argmax(1) * GRID_MM, np.nan)   # face y per column
    xs = (np.arange(nx) + 0.5) * GRID_MM
    w = np.where(np.isnan(front), 0.0, 0.5 + zone(xs))
    sites = []                                    # (x, y_face, D, reach)
    n = int(round(SCREE_PER_SQUARE * f.W / SQUARE_MM))
    for i in rng.choice(nx, size=n, p=w / w.sum()):
        D = SCREE_D_MM[0] + (SCREE_D_MM[1] - SCREE_D_MM[0]) * rng.random() ** SCREE_D_EXP
        sites.append((xs[i] + rng.uniform(-0.5, 0.5) * GRID_MM, front[i], D,
                      rng.exponential(SCREE_REACH_MM)))
    for i in rng.choice(nx, size=int(round(SCREE_BLOCKS * f.W / SQUARE_MM)), p=w / w.sum()):
        sites.append((xs[i], front[i], rng.uniform(*SCREE_BLOCK_MM),
                      rng.exponential(0.3 * SCREE_REACH_MM)))   # big fallen blocks
    for mo in mouths:                             # fallen out of the fissures
        i = int(np.clip(mo[0] / GRID_MM, 0, nx - 1))
        if np.isnan(front[i]):
            continue
        for _ in range(int(rng.integers(SCREE_MOUTH[0], SCREE_MOUTH[1] + 1))):
            D = SCREE_D_MM[0] + 4.0 * rng.random() ** SCREE_D_EXP
            sites.append((mo[0] + rng.normal(0, 1.5), front[i], D,
                          rng.exponential(0.5 * SCREE_REACH_MM)))
    for i in rng.choice(nx, size=int(round(GRIT_PER_SQUARE * f.W / SQUARE_MM)), p=w / w.sum()):
        sites.append((xs[i], front[i], -rng.uniform(*GRIT_D_MM),     # D < 0: grit
                      rng.uniform(0, APRON_R_MM[0])))
    rng.shuffle(sites)
    sites.sort(key=lambda s_: -abs(s_[2]))        # big ones first, small ones fill in
    placed, stones = [], []
    saved, fams = _rl.HEIGHT_MIN_MM, _rl.FAMILIES
    _rl.FAMILIES = SCREE_FAMILIES
    try:
        for x, yf, D, reach in sites:
            grit, D = D < 0, abs(D)
            y = yf - D * (0.5 - SCREE_LEAN) - reach
            if (y < SCREE_FRONT_MM + D / 2 or x < D / 2 + 0.3 or x > f.W - D / 2 - 0.3):
                continue
            if any(np.hypot(x - a, y - b) < SCREE_GAP * (D + d) for a, b, d in placed):
                continue
            _rl.HEIGHT_MIN_MM = min(saved, 0.55 * D)
            st, _ = _rl.make_rock(rng, D)
            st = st.rotate([0.0, 0.0, float(rng.uniform(0, 360))])
            tilt, ax = rng.uniform(0, SCREE_TILT_DEG), rng.uniform(0, 2 * np.pi)
            st = st.rotate([float(tilt * np.cos(ax)), float(tilt * np.sin(ax)), 0.0])
            b = np.array(st.bounding_box())
            lift = 0.5 * D * np.sin(np.radians(tilt))
            z0 = ground_min(x, y, 0.5 * D) - SCREE_SINK_MM - lift
            if grit:
                z0 -= rng.uniform(*GRIT_BURY) * D
            stones.append(st.translate([x, y, z0 - b[2]]))
            placed.append((x, y, D))
    finally:
        _rl.HEIGHT_MIN_MM, _rl.FAMILIES = saved, fams
    return stones


def make_zones(rng, W):
    """Shattered-zone intensity along the run, ZONE_FLOOR..1: a few
    gaussian clusters, so detail gathers instead of spreading evenly."""
    k = max(1, int(round(W / SQUARE_MM * ZONES_PER_SQUARE)))
    cx, sg = rng.uniform(0, W, k), rng.uniform(*ZONE_SIGMA_MM, k)

    def zone(x):
        g = np.exp(-0.5 * ((np.atleast_1d(x)[:, None] - cx) / sg) ** 2).max(1)
        return ZONE_FLOOR + (1 - ZONE_FLOOR) * g
    return zone


def fissures(rng, pile, f, zone):
    """V-wedge fissures across the assembled face: major ones 2–5 mm deep
    (some sending off a branch), minor ones ≈ 1 mm, sited on the open faces
    and mostly in the shattered zones.  Each is a convex wedge cut through
    whatever rocks it crosses, so one fracture runs on across a joint."""
    t = to_mesh(pile)
    pts, fi = trimesh.sample.sample_surface(t, 6000, seed=rng)
    nrm = t.face_normals[fi]
    hz = np.hypot(nrm[:, 0], nrm[:, 1])
    ok = ((hz > 0.9) & (pts[:, 2] > FOOT_Z_MM + 2.0)
          & (pts[:, 2] < (f.ztop or np.inf) - FISSURE_TOP_GAP_MM - 2.0))
    face_w = np.where(nrm[:, 1] < -0.5, 1.0, np.where(nrm[:, 1] > 0.5, 0.2, 0.5))
    w = face_w * zone(pts[:, 0]) ** 2 * ok
    if w.sum() == 0:
        return m3d.Manifold(), []
    sq = f.W / SQUARE_MM
    n_maj, n_min = (int(round(k * sq)) for k in FISSURES_PER_SQUARE)
    n_deep = min(int(round(rng.uniform(*DEEP_FISSURES) * sq)), n_maj)
    pool = rng.choice(len(pts), size=min(300, int((w > 0).sum())), replace=False,
                      p=w / w.sum())
    idx = []                              # spaced out: overlapping wedges shatter into shards
    for want, gap in ((n_maj, FISSURE_GAP_MM[0]), (n_min, FISSURE_GAP_MM[1])):
        got = 0
        for i in pool:
            if got == want:
                break
            if all(np.linalg.norm(pts[i] - pts[j]) >= gap for j in idx):
                idx.append(i)
                got += 1
        if want == n_maj:
            n_maj = got
    top = (f.ztop or np.inf) - FISSURE_TOP_GAP_MM
    wedges, mouths = [], []

    def lean():
        if rng.random() < FISSURE_DIAG_P:
            return rng.choice((-1, 1)) * rng.uniform(*FISSURE_DIAG_DEG)
        return float(np.clip(rng.normal(0.0, FISSURE_LEAN_DEG), -25.0, 25.0))

    rays = t.ray

    def onto_surface(q, m):
        """q moved along m onto the pile's actual surface (faces wander)."""
        hit, _, _ = rays.intersects_location([q + m * FISSURE_RAY_MM], [-m],
                                             multiple_hits=False)
        return hit[0] if len(hit) else None

    for k, i in enumerate(idx):
        m = np.array([nrm[i, 0], nrm[i, 1], 0.0]) / hz[i]
        p = pts[i]
        major = k < n_maj
        deep = k < min(n_deep, n_maj)
        dep = rng.uniform(*(DEEP_DEPTH_MM if deep else MAJOR_DEPTH_MM if major
                            else MINOR_DEPTH_MM))
        th = lean()
        if not major:
            L = rng.uniform(*MINOR_LEN_MM)
            up = rng.uniform(0.3, 0.7) * L
            wedges.append(_wedge(rng, p, m, th, dep, up, L - up, top))
            continue
        # a major fissure runs down in bending segments, each on the surface
        nseg = int(rng.integers(MAJOR_SEGMENTS[0], MAJOR_SEGMENTS[1] + 1))
        L = rng.uniform(*MAJOR_LEN_MM) / nseg
        for sgi in range(nseg):
            wedges.append(_wedge(rng, p, m, th, dep, 0.4 if sgi else 0.5 * L,
                                 L, top, DEEP_HALF_DEG if deep else FISSURE_HALF_DEG))
            if sgi == 0 and rng.random() < BRANCH_P:      # a branch leaves it
                thb = float(np.clip(th + rng.choice((-1, 1)) * rng.uniform(20, 35), -40, 40))
                wedges.append(_wedge(rng, p, m, thb, rng.uniform(*MINOR_DEPTH_MM), 0.3,
                                     rng.uniform(*MINOR_LEN_MM), top))
            q = onto_surface(p - _fissure_dir(m, th) * L, m)
            if q is None or q[2] < FOOT_Z_MM + 1.0 or sgi == nseg - 1:
                mouths.append(p - _fissure_dir(m, th) * L)
                break
            p = q
            dep = float(np.clip(dep * rng.uniform(0.7, 1.1), MINOR_DEPTH_MM[1], MAJOR_DEPTH_MM[1]))
            th = float(np.clip(th + rng.choice((-1, 1)) * rng.uniform(*BEND_DEG), -40, 40))
    wedges = [w_ for w_ in wedges if not w_.is_empty()]
    return (m3d.Manifold.batch_boolean(wedges, m3d.OpType.Add) if wedges
            else m3d.Manifold()), mouths


def _fissure_dir(m, th_deg):
    th = np.radians(th_deg)
    return np.cos(th) * np.array([0, 0, 1.0]) + np.sin(th) * np.cross(m, [0, 0, 1.0])


def _wedge(rng, p, m, th_deg, dep, l_up, l_down, ztop, half_deg=FISSURE_HALF_DEG):
    """One fissure: the V between two planes meeting along a line `dep`
    behind the face point p (running along the fissure direction d), opening
    outward at a half-angle that keeps the mouth ≥ FISSURE_MOUTH_MM.  Its
    ends slope back into the rock (FISSURE_END_DEG), so the rock over the
    top end is a printable roof, not a ceiling."""
    d = _fissure_dir(m, th_deg)
    e = np.cross(d, m)
    half = max(np.radians(rng.uniform(*half_deg)),
               np.arctan(0.5 * FISSURE_MOUTH_MM / dep))
    top = p + d * l_up
    if top[2] > ztop:                                  # stop under the top plane
        l_up = max((ztop - p[2]) / max(d[2], 1e-6), 0.0)
        top = p + d * l_up
    bot = p - d * l_down
    ph = np.radians(FISSURE_END_DEG)
    a = p - m * dep
    planes = [(m * np.sin(half) - e * np.cos(half), a),
              (m * np.sin(half) + e * np.cos(half), a),
              (-d * np.cos(ph) + m * np.sin(ph), top),
              (d * np.cos(ph) + m * np.sin(ph), bot),
              (-m, p + m * FISSURE_CAP_MM)]               # no further out than this
    # big enough that only the five planes bound it (a cube face left inside
    # a rock standing proud of the face would be a flat ceiling)
    size = 2 * (l_up + l_down + dep) + 6 * FISSURE_CAP_MM + 4.0
    w = m3d.Manifold.cube([size, size, size], center=True).translate(p.tolist())
    for n, q in planes:
        w = w.trim_by_plane(n.tolist(), float(n @ q))
    return w


def _cap(rng, f, S, seed):
    """Dirt over the rock tops at the top plane, held back from the edge."""
    from dharmatiles.core.mesh import make_heightmap_solid
    full = S >= f.ztop - 2.5
    k = int(round(CAP_EDGE_MM / GRID_MM))
    mask = ndimage.binary_erosion(full, iterations=k, border_value=0)
    if not mask.any():
        return m3d.Manifold()
    poly = footprint_polygon_all(mask)
    dirt = _dirt.dirt_relief(seed + 7, f.W, f.H)
    # thins out toward the edge: CAP_EDGE_MM in from it, it is under the rock tops
    dist = ndimage.distance_transform_edt(full) * GRID_MM
    fade = np.clip((dist - CAP_EDGE_MM) / CAP_EDGE_MM, 0, 1)
    base = f.ztop - CAP_DROP_MM - 2.5 * (1 - fade)          # (x, y) grid
    zi = _resample(base.T, dirt.relief.shape) + dirt.relief  # heightmaps are (y, x)
    hf = _gl.to_manifold(make_heightmap_solid(zi - 4.0, f.W, f.H,
                                              base_h=0.0)).translate([0, 0, 4.0])
    parts = []
    for g in getattr(poly, 'geoms', [poly]):
        cs = m3d.CrossSection([np.asarray(shapely.geometry.polygon.orient(g, 1.0)
                                          .exterior.coords)[:-1]])
        # down to the foot: hidden in the pile, and where chips opened the
        # rock tops under it, a straight wall instead of a flat ceiling
        parts.append(m3d.Manifold.extrude(cs, f.ztop + 1.0 - FOOT_Z_MM)
                     .translate([0, 0, FOOT_Z_MM]))
    return hf ^ m3d.Manifold.batch_boolean(parts, m3d.OpType.Add)


def _resample(a, shape):
    if a.shape == tuple(shape):
        return a
    return ndimage.zoom(a, (shape[0] / a.shape[0], shape[1] / a.shape[1]), order=1)


def footprint_polygon_all(mask):
    ii, jj = np.nonzero(mask)
    sq = [box(i * GRID_MM, j * GRID_MM, (i + 1) * GRID_MM, (j + 1) * GRID_MM)
          for i, j in zip(ii, jj)]
    poly = shapely.union_all(sq).buffer(0.3).buffer(-0.3).simplify(0.2)
    return shapely.geometry.MultiPolygon(
        [shapely.geometry.Polygon(g.exterior) for g in getattr(poly, 'geoms', [poly])])


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--form', default='run')
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--cols', type=int, default=1)
    args = ap.parse_args()
    r = build(args.seed, args.form, args.cols, args.cols)
    out = ROOT / 'clifflab' / 'out'
    out.mkdir(exist_ok=True)
    path = out / f"{args.cols}x{args.cols}-pile-{args.form}-s{args.seed}.stl"
    r.mesh.export(path)
    ov, _ = _gl.audit_overhang(r.mesh)
    print(f"→ {path}  {len(r.mesh.faces):,} faces, watertight={r.mesh.is_watertight}; "
          f"{r.rocks} rocks; overhang {ov:.1f} mm²; "
          + ", ".join(f"{k} {v:.1f}s" for k, v in r.tm.items()))


if __name__ == '__main__':
    main()
