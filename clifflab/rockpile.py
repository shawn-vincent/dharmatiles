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
  3 FINISH: stones sharing a footprint stand in one column, cleaved by
    whole planes planned once on its convex hull and cut through every
    stone in it — open faces dressed off true (turned a few degrees in plan
    and battered back, so it reads wedge-shaped, not boxed); then each
    stone gets its own shoulders, 45° V joints under the rock it carries,
    chipped arrises and rocklab's corner chips, and its open faces (the boundary's
    too) are carved as blocks (N): flat sub-faces, each tilted its own way,
    split by a joined network of cracks (random walks ending on an edge, a
    joint or another crack, planned once per face of the pile so they run
    on across stones, H), each a narrow slot cut straight in, the odd long
    one a split with its far side stepped back; then grain riding on them.  Detail gathers in
    shattered zones.  A rock resting on rock is seated on its bed at the
    overhang limit, so nothing the finishing takes off leaves a ceiling.
  4 ASSEMBLY: rocks ∪ a core under them (so a gap between stones is a
    recess, never a hole; carved like the faces it lies behind) ∪ dirt
    resting on the rock tops ∪ dirt ground banked up against the face ∪
    scree (angular rocklab stones, 1–8 mm, densest at the face and in the
    shattered zones; grit half-buried in the bank) ∪ DB base.  Rock size
    follows the zones too: big quiet masses between them, small shattered
    blocks in them.

Usage:
    python clifflab/rockpile.py [--form run] [--seed 1] [--cols 1]
"""
from __future__ import annotations

import argparse
import contextlib
import functools
import itertools
import pathlib
import sys
import time
from dataclasses import dataclass, field

import manifold3d as m3d
import numpy as np
import shapely
import trimesh
from scipy import ndimage
from scipy.spatial import cKDTree
from shapely.geometry import box

ROOT = pathlib.Path(__file__).resolve().parent.parent
for _lab in ('dirtlab', 'grasslab', 'rocklab', 'clifflab'):
    sys.path.insert(0, str(ROOT / _lab))
import dirtlab as _dirt     # noqa: E402
import grasslab as _gl      # noqa: E402
import rocklab as _rl       # noqa: E402
from dharmatiles.core.mesh import make_heightmap_solid   # noqa: E402
from dharmatiles.stone.noise import fbm                  # noqa: E402

# ── Parameters ──────────────────────────────────────────────────────────────
SQUARE_MM        = 35.0
FOOT_Z_MM        = 4.0         # foot ground (DB dirt band)
GRID_MM          = 0.25

# 1 form (run)
DEPTH_MM         = (8.0, 20.0)  # how far the cliff reaches out from the back plane…
TOP_DEPTH_MM     = (5.0, 10.0)   # …and the full-height part of it
MASS_W_MM        = (6.0, 15.0)   # along-run segments with their own profile
FULL_P           = 0.35          # a segment is full height all the way out
TIER_Z           = (0.3, 0.7)    # lower tier top, × cliff height
GULLIES_PER_SQUARE = 0.5         # masses cut deep back into the band (re-entrants)…
GULLY_DEPTH_MM   = (5.0, 7.0)    # …reaching only this far out, on masses at least this
GULLY_MIN_W_MM   = 10.0          # wide (a narrower one is a chimney: a pit from above)
BUTTRESSES_PER_SQUARE = 0.5      # masses standing far out, full height…
BUTTRESS_DEPTH_MM = (20.0, 25.0)
END_MM           = 4.0           # standard end section (REQ-RUN-3, S4): every seed's
END_DEPTH_MM     = 11.0          # cliff ends as a full-height band this deep, so tiles
                                 # meet end to end without a step

# 2 stack
KINDS = {  #        p     along (mm)     depth (mm)    height (mm)   bridge  tilt°
    'column': (0.38, (8.0, 14.0), (7.0, 12.0), (15.0, 24.0), 1.2, 8.0),
    'block':  (0.38, (10.0, 18.0), (8.0, 14.0), (11.0, 18.0), 1.2, 10.0),
    'slab':   (0.12, (14.0, 24.0), (8.0, 14.0), (5.0, 8.0), 5.0, 8.0),
    'odd':    (0.06, (6.0, 10.0), (6.0, 10.0), (6.0, 11.0), 2.0, 14.0),
    'monolith': (0.06, (8.0, 16.0), (7.0, 12.0), None, 1.2, 3.0),  # ground to top
}
STAGGER_MM       = 2.0         # a rock's top never lines up with a neighbour's within this…
STAGGER_SHARE    = 0.25        # …along more than this share of their shared edge
COURSE_MM        = 3.0         # (nor with any joint of the columns this close round it:
                               # joints lining up across columns are courses, C4)
CAP_MIN_MM       = 6.0         # a top stone no shorter than this (no row of capstones):
                               # the stone under it runs on up instead
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
MIN_AREA_MM2     = 8.0         # smaller pockets are bridged by a rock over them, or
THIN_AREA_MM2    = 20.0        # (a thin pocket nothing takes, this big, gets its own stone)
CRUMB_RETRY      = 3           # (none fitting yet) in the top band come back this many
CRUMB_RETRY_MM   = 6.0         # times, each after the pile has grown this much more round
                               # them, before they are left as a notch (left at once: a pit
                               # in the plateau)
SNAP_MM          = 3.5         # footprints this close to a contact plane reach it
COLUMN_STEP_MM   = (0.0, 1.6)  # each column stands back from its outline this much (N: they
STONE_STEP_MM    = (0.0, 0.8)  # jut and recede), each stone this much more than the one under it
RIM_SETBACK_MM   = (0.0, 1.0)  # a column on a tile-boundary wall stands back from it this
                               # much (shrunk, not cut): the face steps in and out (more
                               # bares the core, CORE_RIM_MM in, down the joints)
MAX_ROCKS        = 400
JUT_P            = 0.5        # a rock on the ground at the cliff front juts out past it…
JUT_MM           = (1.5, 5.0)  # …this far (a column or block standing proud), only into
JUT_CLEAR_MM     = 6.0         # open ground clear this far beyond it (a jut into a gully
                               # leaves a pit behind it)
BLOCK_ASPECT     = {           # a stone stands at most (drawn from) this × its narrow side:
    'column': (1.6, 2.4), 'block': (1.4, 2.2), 'slab': (0.6, 1.0),   # varied, so no courses
    'odd': (1.0, 1.8), 'monolith': (3.0, 4.0)}
STONE_H_SHARE    = (0.32, 0.55) # a stone stands at most this × the cliff height (≥ 1 bed
TALL_P           = 0.02          # joint per column), but this share of them (and monoliths)
WIDTH_APART      = 0.25          # a stone's width differs from the last one's by ≥ this share
SIZE_LOW_GAIN    = 0.5         # stones low in the mass × (1 + this/2), high × (1 − this/2)
ROUND_YAW_DEG    = 8.0         # round forms: stones face the outline's tangent, ± this (σ)
ROUND_SMOOTH_MM  = 3.0         # …the tangent read off the outline smoothed this much

# 3 finish
COLUMN_SHARE     = 0.6         # stones sharing this share of each one's footprint stand in
                               # one column: its face planes are chosen once, for all of them
JOINT_OVERLAP_MM = 0.03        # rock footprints overlap their neighbours this much
FOOTPRINT_SIMPLIFY_MM = 0.45   # footprint outline straightened to this tolerance
CHIP_DEG         = 40.0        # every arris sharper than this (but the underside's) broken (N):
CHIP_MM          = (0.25, 0.9) # a row of flat chips knocked this deep off it…
CHIP_LEN_MM      = (1.5, 5.0)  # …each this long, running out at its ends over CHIP_RUN × its
CHIP_RUN         = 2.5         # depth (a flake scar; at the upper end of a steep edge at 45°:
                               # no ceiling), overlapping the last…
CHIP_SKIP_P      = 0.1         # …now and then a stretch left sharp
ROCK_GRAIN_MM    = 0.05        # grain amplitude on scree stones
FACE_GRAIN_MM    = 0.25        # grain amplitude on cliff stones: deep enough to catch
FACE_GRAIN_LEN_MM = 1.0        # …a drybrush, ≈ 2 lines across so it prints (rides on the planes)
                               # (isotropic: stretched down the column it hangs as folds)
FIT_GAP_MM       = 0.02        # stones stop this short of the tile boundary (not cut)
EXPORT_RETRY_TOL_MM = (None, 0.015, 0.02, 0.03, 0.06, 0.1)  # export: simplify the top this much if needed
CHIP_MAX_MM3     = 3.0         # a chip (rocklab's) takes at most this: a plane through a
                               # corner of a non-convex stone would take a whole arm
Z_UP             = np.array([0.0, 0.0, 1.0])
BEVEL_MM         = 0.8         # bevel (at the overhang limit) under rocks resting on rocks
OVERHANG_DEG     = 55.0        # FDM: no downward face leans further than this from vertical…
LAYER_MM         = 0.2         # …but what a printer minds is each layer (this thick) reaching
REACH_MM         = 0.6         # out over the one below: past this (≈ a 0.4 mm nozzle's line),
                               # beyond the OVERHANG_DEG cone of any layer up to 3 mm below, it
                               # droops (a thin crack's ceiling is fine; a long overhang is not)
SEAT_GRID_MM     = 0.2         # resting rocks' underside is shaped on this grid…
SEAT_SLOPE       = float(np.tan(np.radians(90.0 - OVERHANG_DEG + 3.0)))  # …3° inside the limit
SEAT_SPAN_MM     = 1.5         # …bridging every low strip in its bed narrower than this
SEAT_KEEP_MM     = (4.5, 1.0)  # …and within this of an open face no deeper than this
                               # (under the joint's groove: a groove, not a belly)
CORNER_MM        = (1.4, 2.2)  # vertical corner chamfers: neighbours meet in a V seam
ZONES_PER_SQUARE = 1.5         # shattered zones (REQ-ROCK-3: detail comes in clusters)…
ZONE_SIGMA_MM    = (5.0, 10.0) # …this wide along the run
ZONE_FLOOR       = 0.25        # intensity between zones
#   cleaving: whole-plane cuts of each (still convex) rock — faces turn into
#   planes meeting at creases, never steps (REQ-ROCK-4)
JOINT_MM         = (0.8, 1.2)  # a loaded rock's exposed top edge: 45° chamfer (the joint's
                               # narrow groove, with the seat bevel above), one plane per
JOINT_MIN_MM     = 4.0         # edge, on edges no shorter than this
SHOULDER_P       = 0.6         # an exposed ledge top's front edge is broken…
SHOULDER_MM      = (0.8, 2.5)  # …this deep
DRESS_P          = 0.85        # an exposed face is dressed off true: turned in plan…
DRESS_YAW_DEG    = (3.0, 12.0) # …this much (the rock reads wedge-shaped, not boxed)…
DRESS_BATTER_DEG = (-3.0, 10.0)  # …and leaning back (taper) or a little forward
DRESS_MAX_MM     = 2.5         # …taking at most this off the face
CLEAVE_MAX_LOSS  = 0.15        # one cut takes at most this share of the rock…
CLEAVE_MIN_KEEP  = 0.6         # …and all of them together leave at least this (and takes
CLEAVE_MIN_MM3   = 0.02        # at least this: no sliver)
THIN_GAIN_MM2    = 0.4         # …nor leave it thinner than MIN_WALL_MM anywhere (a fin) over
MIN_WALL_MM      = 1.2         # more than this much more of its plan
CLEAVE_SUPPORT_MM3 = 1.0      # a cut may take this much from under a rock resting here
                               # (or from the core)
CORE_SET_MM      = 2.0         # a solid core under the pile: in this far from open faces…
CORE_RIM_MM      = 2.8         # …this far from a boundary wall (behind its deepest sub-face)…
CORE_END_RIM_MM  = 0.5         # …(from a band's end planes only this: the joints there
                               # are backed, never see-through)…
CORE_DROP_MM     = 0.5         # …and this far under every top: where rounded corners meet,
                               # the gap is a dark recess, never a hole
#   block faces: one crack network per open column face (the boundary's too),
#   every stone of the column (and the core behind it) cut back along it (N:
#   massive blocks, calm flat faces, the joints the strongest lines; now and
#   then a block split in two by a long straight crack, one side standing
#   proud of the other)
CRACK_PER_MM2    = 0.06        # crack length (mm) per mm² of face (× 0.6–1 by zone)…
CRACK_STONE_MM   = 10.0        # …but no more than this per stone on it (0–1 a block)…
CRACK_STEP_MM    = (4.0, 8.0)  # …random walks: straight runs this long, kinked…
CRACK_BEND_DEG   = 6.0         # …by this much (σ), within this of their heading…
CRACK_WANDER_DEG = 12.0
CRACK_LEVEL_P    = 0.35        # …near level this often, this often diagonal, else near
CRACK_DIAG_P     = 0.05        # vertical (lean: N(0, CRACK_LEAN_DEG))
CRACK_DIAG_DEG   = (30.0, 50.0)
CRACK_LEAN_DEG   = 8.0
CRACK_GAP_MM     = 2.0         # none along an edge, joint or crack closer than this…
CRACK_ALONG      = (25.0, 3.0)  # …nor running within this (°) of an edge or joint closer
                               # than this (mm): a raised welt between them, upholstery
                               # (it turns into it instead; started this far from all)
CRACK_MIN_MM     = 3.0         # …nor shorter than this…
CRACK_MEET_DEG   = 35.0        # …nor meeting another under this (a lens)
CRACK_CORNER_MM  = 2.0         # face ends this close meet at a corner (a crack runs on round it,
CRACK_WRAP_DEG   = (5.0, 15.0) # turned this much: no ruled line round the stone)
CRACK_SLOT_MM    = (0.4, 0.5)  # grooves this wide across (what a 0.4 mm nozzle leaves open)…
CRACK_DEPTH_MM   = (0.5, 0.65)  # …this deep, cut straight in (a step lighter than a joint)…
CRACK_PINCH      = (0.7, 1.2)  # …pinching and swelling across along it within this (×)…
CRACK_WALL_MM    = 1.6         # …never closer than this to the stone's back
CRACK_SHIFT_MM   = (0.3, 0.7)  # a split's far side set back this much more all along it:
CRACK_LEVEL_DEG  = 30.0        # within this of level the upper one (a shelf; the groove's
                               # ceiling then reaches under REACH_MM)
CRACK_FINE_MM    = 5.0         # a crack shorter than this a hairline: no step, depth ×
CRACK_FINE       = 0.8
FACET_TILT_DEG   = (3.0, 8.0)   # sub-faces: flat planes tilted off the face this much, any way…
FACET_MM         = (0.0, 0.3, 0.6)  # …high edge set back this, the low at most this…
JOINT_PROUD_MM   = 0.3         # …a stone's no further than this proud of the one under it…
FACET_DEEP_MM    = (1.4, 0.7)  # …the high edge, across splits, at most this (on the tile
                               # boundary this: the next tile's band meets it)
RELIEF_OFF_MM    = 4.0         # (none on the core further than this off the face's plane)
FACET_SEAM_MM    = 0.1         # (where two meet: eased over this, no jaggies down a crack)
FACET_END_MM     = 1.5         # (both eased out to the face's ends: a joint there would open)
SHEAR_DEG        = 45.0        # the planes cut down-and-in, this far below level: a step is
                               # a bevel slanting down, never past the overhang limit
RELIEF_GRID_MM   = 0.1         # (the face's raster, and the stone's cut on this grid…
CRACK_GRID_MM    = 0.05        # …the cracks' on this: a 0.4 mm groove's diagonal walls clean)
CUT_BLUR_MM      = (0.08, 0.3)  # (crisp; by its outline more: no hooks at its joints)
CUT_SHORT_MM     = 0.1         # (stopping this short of what it leaves: no coplanar slivers)

# 4 assembly
APRON_H_MM       = (1.5, 3.5)  # a dirt bank in the angle of cliff and ground (between
APRON_R_MM       = (3.0, 6.5)  # zones → shattered zone): this high at the face, this far out
APRON_END        = (2.5, 4.5)  # (height, reach) at the tile ends, same for every seed (S4)
GROUND_END_MM    = 3.0         # the dirt texture fades flat this close to the tile ends
APRON_EXP        = 1.6         # profile: concave, like a talus foot
APRON_SHARE      = 0.25        # never higher than this × the rock beside it
GRIT_PER_SQUARE  = 45          # small stones half-buried in the bank…
GRIT_D_MM        = (1.0, 2.2)
GRIT_BURY        = (0.2, 0.5)  # …sunk this × D
SCREE_PER_SQUARE = 55          # fallen stones along each square of cliff foot…
SCREE_D_MM       = (1.0, 6.0)  # …sizes mixed, skewed small (no scale-marker carpet)…
SCREE_D_EXP      = 2.0
SCREE_REACH_MM   = 4.0         # …lying mostly this close to the face (exponential mean)…
SCREE_LEAN       = 0.35        # …the nearest leaning in, up to this × D into the face
SCREE_BLOCKS     = 2.5         # big fallen blocks per square, against the face…
SCREE_BLOCK_MM   = (4.5, 8.0)  # …this size, cut like the cliff's blocks
SCREE_BLOCK_CUTS = (2, 4)      # …broken by this many planes
SCREE_GAP        = 0.38        # centres at least this × (Di + Dj) apart
SCREE_TILT_DEG   = 8.0
SCREE_FAMILIES   = {'block': 0.45, 'wedge': 0.35, 'slab': 0.2}  # angular fragments, no domes
SCREE_SINK_MM    = 0.3         # into the dirt (plus whatever the tilt lifts)
DIRT_FILL_MM     = 3.0         # dirt on the rock tops lodges in the lows a grey closing this
DIRT_FILL_DEPTH_MM = 1.5       # wide fills (joints, hollows), at most this deep…
DIRT_FILM_MM     = (0.1, 0.5)  # …and over open stone a film this thick + this × the dirtlab
                               # relief (the grain's peaks show through)…
DIRT_STEP_MM     = 0.5         # …thinning to nothing at lips and steps (stepping more per
DIRT_SLOPE_DEG   = 35.0        # cell, nor sloping more than this (over 0.5 mm: not the
                               # grain) — none on a steep face —
DIRT_RAMP_MM     = 1.5         # …over this (on the closed top: a joint is no lip)
DIRT_BLUR_MM     = 0.4         # the base blurred this much (no cell steps)
DIRT_LOW_MM      = 5.0         # none on rock tops lower than this over the foot (+3 mm ramp)
DIRT_SINK_MM     = 0.3         # its underside the rock top sunk this far
DIRT_PLUG_MM     = (1.5, 3.0)  # slits between rock tops narrower / shallower than this
                               # filled (a dark gap under a mm wide reads as a crack in the
                               # print, and the slicer leaves it ragged)


def _grid(nx, ny):
    """Cell-centre coordinates X, Y of an (nx, ny) GRID_MM grid."""
    return np.meshgrid((np.arange(nx) + 0.5) * GRID_MM, (np.arange(ny) + 0.5) * GRID_MM,
                       indexing='ij')


def to_mesh(man):
    o = man.to_mesh()
    return trimesh.Trimesh(np.asarray(o.vert_properties)[:, :3], np.asarray(o.tri_verts),
                           process=False)


def _disc(r_mm):
    """A disc footprint of radius r_mm on the grid."""
    k = int(round(r_mm / GRID_MM))
    return np.hypot(*np.mgrid[-k:k + 1, -k:k + 1]) <= k


def _signed(rng, lo_hi):
    """± U(lo, hi), the sign drawn first."""
    return rng.choice((-1, 1)) * rng.uniform(*lo_hi)


def _count(rng, lo_hi):
    """An integer in lo..hi, both included."""
    return int(rng.integers(lo_hi[0], lo_hi[1] + 1))


def _lerp(lo_hi, u):
    return lo_hi[0] + (lo_hi[1] - lo_hi[0]) * u


@contextlib.contextmanager
def _rl_params(**kw):
    """rocklab's module parameters set to `kw` for the duration."""
    saved = {k: getattr(_rl, k) for k in kw}
    try:
        for k, v in kw.items():
            setattr(_rl, k, v)
        yield
    finally:
        for k, v in saved.items():
            setattr(_rl, k, v)


# ── 1 Form ──────────────────────────────────────────────────────────────────
@dataclass
class Form:
    W: float
    H: float
    T: np.ndarray            # target height per grid cell (FOOT_Z_MM = nothing)
    ztop: float | None       # the top contact plane, if any
    round: bool = False      # free-standing outline: stones face its tangent
    mass: float = 1.0        # stone size × this (massive blocks vs. broken rubble)
    along_y: np.ndarray = None  # band forms: cells whose band runs along y (left/right walls)
    kind_p: dict = None      # stone kind probabilities overriding KINDS'
    rims: tuple = ()         # the tile-boundary planes the cliff stands against:
                             # (axis, high side); the others are band ends (S4)


def _segments(rng, length):
    """Cut [0, length] into masses of MASS_W_MM."""
    xs = [0.0]
    while xs[-1] < length:
        xs.append(xs[-1] + rng.uniform(*MASS_W_MM))
    xs[-1] = length
    if len(xs) > 2 and xs[-1] - xs[-2] < MASS_W_MM[0] * 0.5:
        del xs[-2]
    return xs


# Forms of the reference sheet.  'band' forms are cliff bands grown out
# from walls (tile edges or a tile's centre line); 'rings' forms are
# free-standing masses: concentric tiers around one or more centres.
#   height     : top of the highest tier above the foot (mm); a pair = a ramp
#                along the band (low end → high end, either way round)
#   cap        : dirt on the flat top (needs one top height)
#   kinds      : stone kind probabilities overriding KINDS'
#   rings      : ((outer radius range mm), (height × range)) outer → inner;
#                radii scale with the tile (× min(W, H) / 70)
RIM_PLANES = {'back': (1, True), 'front': (1, False), 'left': (0, False), 'right': (0, True)}
CORNER_KINDS = {'column': 0.5, 'block': 0.24, 'slab': 0.08, 'odd': 0.06, 'monolith': 0.12}
#   (two outer faces on show: tall stones, or they read as coursed walling)
FORMS = {
    'run':        dict(kind='band', walls=('back',), height=35.0, cap=True, mass=1.35),  # B H N
    'corner':     dict(kind='band', walls=('back', 'left'), height=35.0, cap=True,     # C
                       mass=1.35, kinds=CORNER_KINDS),
    'canyon':     dict(kind='band', walls=('left', 'right'), height=35.0, cap=True,  # I K
                       mass=1.3,
                       depth=(8.0, 14.0), top_depth=(5.0, 9.0), buttress=(14.0, 17.0)),
    'escarpment': dict(kind='band', walls=('back',), height=(10.0, 40.0), cap=False,  # D O
                       mass=1.2),
    'plateau':    dict(kind='rings', height=14.0, cap=True, mass=1.3,               # A
                       rings=(((22.0, 28.0), (1.0, 1.0)),)),
    'massif':     dict(kind='rings', height=35.0, cap=True, mass=1.4,               # M
                       rings=(((24.0, 29.0), (0.4, 0.6)), ((13.0, 17.0), (1.0, 1.0)))),
    'mound':      dict(kind='rings', height=34.0, cap=False, mass=1.15,             # F
                       rings=(((25.0, 29.0), (0.12, 0.22)), ((19.0, 22.0), (0.32, 0.45)),
                              ((13.0, 16.0), (0.52, 0.66)), ((8.0, 10.0), (0.74, 0.86)),
                              ((5.0, 7.0), (1.0, 1.0)))),
    'spire':      dict(kind='rings', height=58.0, cap=False,                        # E P
                       rings=(((24.0, 28.0), (0.08, 0.14)), ((17.0, 20.0), (0.25, 0.4)),
                              ((11.0, 13.0), (0.5, 0.68)), ((6.5, 8.0), (1.0, 1.0)))),
    'needles':    dict(kind='rings', height=55.0, cap=False, centres=(2, 3),         # J
                       base=((1.2, 1.4), (0.12, 0.2)),
                       rings=(((14.0, 17.0), (0.1, 0.18)), ((9.0, 11.0), (0.35, 0.5)),
                              ((5.0, 6.5), (1.0, 1.0)))),
}
RING_JITTER      = 0.15        # ring radii vary ± this share between angular masses…
RING_NOTCH_P     = 0.12        # …a mass is notched back (a gully) this often…
RING_LOBE_P      = 0.12        # …or stands out as a lobe (a buttress)
RING_STEP_MM     = 3.0         # a ring's height steps ± this between masses
NEEDLE_SCALE     = (0.45, 0.8) # secondary needles: rings × this, height × this…
NEEDLE_SPREAD    = (0.45, 0.7) # …standing this × (sum of outer radii) from the first
#   base: ((radius × outer radius), (height × range)) — one low tier round all
#   the centres, so a cluster stands on a shared footing
BASE_RAGGED      = 0.15        # its outline wanders ± this share (σ)


def make_form(rng, form, W, H):
    if form not in FORMS:
        raise ValueError(f"form {form!r}: one of {', '.join(FORMS)}")
    spec = FORMS[form]
    X, Y = _grid(int(round(W / GRID_MM)), int(round(H / GRID_MM)))
    T = np.full(X.shape, FOOT_Z_MM)
    along_y, rims = None, ()
    if spec['kind'] == 'band':
        rims = tuple(RIM_PLANES[w] for w in spec['walls'])
        walls = {'back': (X, H - Y, W), 'front': (X, Y, W),
                 'left': (Y, X, H), 'right': (Y, W - X, H)}
        along_y = np.zeros(X.shape, bool)
        for wall in spec['walls']:
            Tw = _band(rng, spec, *walls[wall])
            along_y = np.where(Tw > T, wall in ('left', 'right'), along_y)
            T = np.maximum(T, Tw)
    else:
        T = _rings(rng, spec, X, Y, W, H)
    h = spec['height']
    zt = FOOT_Z_MM + h if spec['cap'] and np.isscalar(h) else None
    return Form(W, H, T, zt, round=spec['kind'] == 'rings',
                mass=spec.get('mass', 1.0), along_y=along_y, kind_p=spec.get('kinds', {}),
                rims=rims)


def _band(rng, spec, A, Dp, L):
    """One cliff band grown out from a wall: A is the along-wall coordinate
    (0..L), Dp the distance out from the wall.  Masses along the wall, each
    with its own depth, a stepped lower tier, the odd gully or buttress, and
    the standard end section at both ends (S4)."""
    T = np.full(A.shape, FOOT_Z_MM)
    h = spec['height']
    ramp = None if np.isscalar(h) else (h if rng.random() < 0.5 else h[::-1])

    def top_at(a):                                    # the band's top along the wall
        if ramp is None:
            return FOOT_Z_MM + h
        return FOOT_Z_MM + ramp[0] + (ramp[1] - ramp[0]) * np.clip(a / L, 0, 1)

    depth = spec.get('depth', DEPTH_MM)
    top_depth = spec.get('top_depth', TOP_DEPTH_MM)
    xs = _segments(rng, L)
    # big plan forms: a few masses cut deep back (gullies), a few standing
    # far out (buttresses) — never the end masses (standard end section)
    inner = list(rng.permutation(np.arange(1, len(xs) - 2))) if len(xs) > 3 else []
    n_g = int(rng.poisson(GULLIES_PER_SQUARE * L / SQUARE_MM) or 1)
    n_b = int(rng.poisson(BUTTRESSES_PER_SQUARE * L / SQUARE_MM) or 1)
    gul = [k for k in inner if xs[k + 1] - xs[k] >= GULLY_MIN_W_MM][:n_g]
    role = {k: 'gully' for k in gul}
    role.update({k: 'buttress' for k in [k for k in inner if k not in gul][:n_b]})
    for si_, (a, b) in enumerate(zip(xs[:-1], xs[1:])):
        zt = float(top_at(0.5 * (a + b)))
        d_all = rng.uniform(*depth)
        d_top = d_all if rng.random() < FULL_P else rng.uniform(*top_depth)
        if role.get(si_) == 'gully':
            d_all = d_top = rng.uniform(*GULLY_DEPTH_MM)
        elif role.get(si_) == 'buttress':
            d_all = d_top = rng.uniform(*spec.get('buttress', BUTTRESS_DEPTH_MM))
        if d_top < d_all:                            # no tier thinner than a rock
            d_all = max(d_all, d_top + MIN_STRIP_MM)
        z_mid = FOOT_Z_MM + rng.uniform(*TIER_Z) * (zt - FOOT_Z_MM)
        # the lower tier steps along the run (no ledge runs level for long),
        # in steps no shorter than MIN_STEP_MM, its depth changing by 0 or ≥
        # MIN_STRIP_MM — anything finer is filled by sliver rocks (organ pipes)
        k = _count(rng, TIER_STEPS)
        k = max(1, min(k, int((b - a) // MIN_STEP_MM)))
        cuts = []
        if k > 1:
            free = (b - a) - k * MIN_STEP_MM
            cuts = list(a + MIN_STEP_MM * np.arange(1, k)
                        + np.sort(rng.uniform(0, free, k - 1)))
        if zt - FOOT_Z_MM > 2 * MIN_H_MM + 3.0:      # room for a lower tier
            for a2, b2 in zip([a, *cuts], [*cuts, b]):
                sel = (A >= a2) & (A < b2)
                z2 = np.clip(z_mid + rng.uniform(-TIER_STEP_MM, TIER_STEP_MM),
                             FOOT_Z_MM + MIN_H_MM + 1.0, zt - MIN_H_MM - 1.0)
                d2 = d_all
                if d_top < d_all and rng.random() < 0.5:
                    d2 = d_all + _signed(rng, (MIN_STRIP_MM, 1.5 * MIN_STRIP_MM))
                    if d2 < d_top + MIN_STRIP_MM:
                        d2 = d_all + abs(d2 - d_all)
                T[sel & (Dp <= d2)] = z2
        sel = (A >= a) & (A < b)
        T[sel & (Dp <= d_top)] = zt
    ends = (A < END_MM) | (A > L - END_MM)
    T[ends] = FOOT_Z_MM
    T[ends & (Dp <= END_DEPTH_MM)] = top_at(A[ends & (Dp <= END_DEPTH_MM)])
    return T


def _rings(rng, spec, X, Y, W, H):
    """A free-standing mass: concentric tiers around a centre (or several,
    for needles), each tier cut into angular masses with their own radius
    and height — notched back here, standing out as a lobe there."""
    T = np.full(X.shape, FOOT_Z_MM)
    sc = min(W, H) / 70.0
    k = _count(rng, spec['centres']) if 'centres' in spec else 1
    centres = []
    for i in range(k):
        size, hgt = (1.0, 1.0) if i == 0 else (rng.uniform(*NEEDLE_SCALE),) * 2
        r_out = spec['rings'][0][0][1] * sc * size
        c = np.array([W / 2, H / 2]) + rng.uniform(-2, 2, 2)
        for _ in range(50 if i else 0):  # a cluster round the first, clear of the edge
            c0, r0 = centres[0]
            t = rng.uniform(0, 2 * np.pi)
            c = c0 + rng.uniform(*NEEDLE_SPREAD) * (r0 + r_out) * np.array([np.cos(t), np.sin(t)])
            if (r_out + 2 < c[0] < W - r_out - 2 and r_out + 2 < c[1] < H - r_out - 2
                    and all(np.hypot(*(c - c2)) > NEEDLE_SPREAD[0] * (r_out + r2)
                            for c2, r2 in centres)):
                break
        centres.append((c, r_out))
        R = np.hypot(X - c[0], Y - c[1])
        th = np.arctan2(Y - c[1], X - c[0]) % (2 * np.pi)
        circ = 2 * np.pi * r_out
        segs = np.array(_segments(rng, circ)) / max(circ, 1e-9) * 2 * np.pi
        for a, b in zip(segs[:-1], segs[1:]):
            sel = (th >= a) & (th < b)
            u = rng.random()
            f = 0.6 if u < RING_NOTCH_P else 1.2 if u < RING_NOTCH_P + RING_LOBE_P else 1.0
            for (rr, hr) in spec['rings']:             # outer → inner: inner tiers win
                r = rng.uniform(*rr) * sc * size * f * (1 + rng.uniform(-RING_JITTER, RING_JITTER))
                z = FOOT_Z_MM + rng.uniform(*hr) * spec['height'] * hgt
                if hr[0] < 1.0:
                    z += rng.uniform(-RING_STEP_MM, RING_STEP_MM)
                z = max(z, FOOT_Z_MM + MIN_H_MM)
                T[sel & (R <= r)] = np.maximum(T[sel & (R <= r)], z)
    if 'base' in spec:                              # a shared footing round them all
        (rs, hs) = spec['base']
        Rb = np.min([np.hypot(X - c[0], Y - c[1]) / (r * rng.uniform(*rs))
                     for c, r in centres], axis=0)
        Rb *= 1 + BASE_RAGGED * _wobble(rng, X.shape, 6.0)
        z = FOOT_Z_MM + max(rng.uniform(*hs) * spec['height'], MIN_H_MM)
        z = z + RING_STEP_MM * np.round(_wobble(rng, X.shape, 8.0))   # stepped, not a drum
        z = np.maximum(z, FOOT_Z_MM + MIN_H_MM)
        T = np.where(Rb <= 1.0, np.maximum(T, z), T)
    return T


def _wobble(rng, shape, sigma_mm):
    """Smooth noise over the grid, σ ≈ 1."""
    g = ndimage.gaussian_filter(rng.standard_normal(shape), sigma_mm / GRID_MM)
    return g / (g.std() + 1e-12)


# ── 2 Stack ─────────────────────────────────────────────────────────────────
@dataclass
class Placed:
    cells: np.ndarray         # bool grid: footprint
    z0: float                 # underside (lowest support)
    top: tuple                # top plane z = c + a·x + b·y
    D: float                  # size, for the finishing passes
    bed: np.ndarray = None    # the support's height under each footprint cell (NaN off it)

    def top_z(self, x, y):
        c, a, b = self.top
        return c + a * x + b * y

    @functools.cached_property
    def poly(self):           # (read only once the stack is done: cells may still grow)
        return footprint_polygon(self.cells)


def stack(rng, form, zone):
    T = form.T
    X, Y = _grid(*T.shape)
    S = np.full_like(T, FOOT_Z_MM - 1.0)
    want = T > FOOT_Z_MM
    rocks = []
    kinds = list(KINDS)
    probs = np.array([form.kind_p.get(k, KINDS[k][0]) for k in kinds])
    probs /= probs.sum()
    noise = rng.random(T.shape) * 1.5
    retry = np.zeros(T.shape, int)
    h_cliff = T.max() - FOOT_Z_MM
    last_w = 0.0
    if form.round:          # the outline's outward normal everywhere inside it
        edt = ndimage.distance_transform_edt(want) * GRID_MM
        sm = ndimage.gaussian_filter(edt, ROUND_SMOOTH_MM / GRID_MM)
        gx, gy = np.gradient(sm)
        phi = np.arctan2(-gy, -gx)
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
        lo = 1.0 + SIZE_LOW_GAIN * (0.5 - np.clip((S[si, sj] - FOOT_Z_MM)
                                                  / max(h_cliff, 1.0), 0, 1))
        w, d = (rng.uniform(*al) * form.mass * lo, rng.uniform(*dp) * form.mass * lo)
        u = (float(zone(site[0])[0]) - ZONE_FLOOR) / (1 - ZONE_FLOOR)
        sz = _lerp(ZONE_SIZE, u)              # big quiet masses, small shattered blocks
        w *= sz
        if abs(w / max(last_w, 1e-6) - 1.0) < WIDTH_APART:      # no runs of equal widths
            w *= rng.choice((1.0 - 1.5 * WIDTH_APART, 1.0 + 2.0 * WIDTH_APART))
        last_w = w
        if form.along_y is not None and form.along_y[si, sj]:
            w, d = d, w                               # the stone's long side along its band
        if form.round:
            yaw = phi[si, sj] + np.radians(ROUND_YAW_DEG) * rng.standard_normal()
        for attempt in range(4):
            # cells it may take: no higher than its bridge over the site, still short of T
            room = want & (S <= S[si, sj] + bridge + 3.0 * attempt) & (T - S > 0.3)
            if form.round:
                cells = _grow_turned(X, Y, room, site, (si, sj), w, d, yaw, edt[si, sj])
            else:
                cells = _grow(rng, room, (si, sj), w, d)
            area, side = cells.sum() * GRID_MM ** 2, _min_side(cells)
            if area >= MIN_AREA_MM2 and side >= MIN_SIDE_MM:
                break
        if area == 0:
            S[si, sj] = T[si, sj]
            continue
        if (area < MIN_AREA_MM2 or side < MIN_SIDE_MM) and rocks:
            S, ok = _merge_crumb(rocks, cells, S, X, Y, form)   # no slivers: a neighbour takes it
            if not ok and is_top and retry[cells].max() < CRUMB_RETRY:
                retry[cells] += 1             # neighbours not up yet: later
                noise[cells] += CRUMB_RETRY_MM
                continue
            if not ok:                    # a shallow one: any neighbour standing over it
                S, ok = _merge_crumb(rocks, cells, S, X, Y, form, min_h=0.3)
            if not ok and (area < THIN_AREA_MM2 or side < MIN_SIDE_MM * 0.6):
                want = want & ~cells      # left as a notch: nothing to fill, no dirt
                continue
            if ok:
                continue                  # (a long thin pocket nothing takes: a thin stone
                                          # fills it, else it is a slot through the cliff)
        if not form.round and S[cells].max() <= FOOT_Z_MM and rng.random() < JUT_P:
            cells = _jut(rng, cells, want, S)
        Sv, Xv, Yv = S[cells], X[cells], Y[cells]
        xm, ym = Xv.mean(), Yv.mean()
        # rest on the highest supports: fitted slope (capped), raised to touch
        A = np.c_[np.ones_like(Xv), Xv - xm, Yv - ym]
        coef = np.linalg.lstsq(A, Sv, rcond=None)[0] if len(Sv) > 3 else np.zeros(3)
        g = np.hypot(coef[1], coef[2])
        gmax = np.tan(np.radians(tilt))
        if g > gmax:
            coef[1:] *= gmax / g
        t2 = np.radians(TOP_TILT_DEG) * rng.standard_normal(2) * 0.6
        a, b = coef[1] + t2[0], coef[2] + t2[1]
        c = (Sv - a * (Xv - xm) - b * (Yv - ym)).max()   # bed plane at the footprint centre
        tgt = T[si, sj] - (c + a * (X[si, sj] - xm) + b * (Y[si, sj] - ym))
        lim = CAP_MIN_MM if is_top else MIN_H_MM      # what may be left above it

        def finish(h):
            if tgt - h < lim:                         # too little left above: reach T
                h = tgt + 1.0 if is_top else tgt + rng.uniform(*OVERSHOOT_MM)
            return max(h, MIN_H_MM)

        # height follows size too (more gently): wide masses stay masses, not
        # squat slabs stacked like masonry
        h = finish(tgt + 1.0 if hr is None
                   else rng.uniform(*hr) * (sz * form.mass * lo) ** ZONE_SIZE_H_EXP)
        hc = min(rng.uniform(*BLOCK_ASPECT[kind]) * _min_side(cells), tgt - lim)
        if kind != 'monolith' and rng.random() > TALL_P:
            hc = min(hc, rng.uniform(*STONE_H_SHARE) * h_cliff)
        if MIN_H_MM <= hc < h:                        # a block; the rest stacks on it
            h = hc
        # stagger: a rock stopping short of T must not end level with its
        # neighbours along their shared edge, nor with any joint of the
        # stones round it — that is how courses appear
        ring = (ndimage.binary_dilation(cells, iterations=2) & ~cells
                & (S > FOOT_Z_MM + 0.5))
        near = ndimage.binary_dilation(cells, iterations=int(COURSE_MM / GRID_MM)) & ~cells
        joints = []                                   # (cells, their top) round it
        for r in rocks:
            q = r.cells & near
            if q.sum() >= 4:
                zr = r.top_z(X[q], Y[q])
                if form.ztop is None or zr.mean() < form.ztop - 0.5:
                    joints.append((q, zr))

        def level(h):
            top = lambda M: c + h + a * (X[M] - xm) + b * (Y[M] - ym)
            if ring.any() and (np.abs(top(ring) - S[ring]) < STAGGER_MM).mean() > STAGGER_SHARE:
                return True
            return any((np.abs(top(q) - zr) < STAGGER_MM).mean() > 0.5 for q, zr in joints)
        for _ in range(6):
            if tgt - h < lim or not level(h):
                break
            h = finish(h + _signed(rng, (STAGGER_MM, 2.5 * STAGGER_MM)))
        c_top = c + h - a * xm - b * ym
        topz = c_top + a * X + b * Y
        if form.ztop is not None:
            topz = np.minimum(topz, form.ztop)
        S = np.where(cells, np.maximum(S, topz), S)
        bed = np.full(S.shape, np.nan)
        bed[cells] = Sv
        rocks.append(Placed(cells, _sunk(Sv), (c_top, a, b), float(np.median([w, d, h])), bed))
    return rocks, S


def _sunk(Sv):
    """A rock's underside on supports Sv: sunk SINK_MM into rock, not into the ground."""
    return float(Sv.min()) - (SINK_MM if Sv.min() > FOOT_Z_MM else 0.0)


def _grow(rng, ok, ij, w, d):
    """The footprint: an axis-aligned rectangle grown from the site `ij` one
    row at a time, only into `ok` cells, toward a w × d target; a side that
    ends within SNAP_MM of the pocket's edge (a neighbour or a contact plane)
    runs on to it, so the leftover pockets stay rectangular and never become
    slivers."""
    nx, ny = ok.shape
    bd = [ij[0], ij[0] + 1, ij[1], ij[1] + 1]       # i0, i1, j0, j1; side k moves bd[k]
    goal = (w / GRID_MM, d / GRID_MM)
    snap = SNAP_MM / GRID_MM

    def free(side, k=0):                            # the k-th row beyond `side` all ok
        i0, i1, j0, j1 = bd
        if side == 0:
            return i0 - 1 - k >= 0 and ok[i0 - 1 - k, j0:j1].all()
        if side == 1:
            return i1 + k < nx and ok[i1 + k, j0:j1].all()
        if side == 2:
            return j0 - 1 - k >= 0 and ok[i0:i1, j0 - 1 - k].all()
        return j1 + k < ny and ok[i0:i1, j1 + k].all()

    for limit in ('goal', 'snap'):
        for side in rng.permutation(4):
            while free(side):
                if limit == 'snap':
                    # run on only if the pocket ends within the snap distance
                    k = 0
                    while k <= snap and free(side, k):
                        k += 1
                    if k > snap:
                        break
                elif bd[2 * (side // 2) + 1] - bd[2 * (side // 2)] >= goal[side // 2]:
                    break
                bd[side] += 1 if side % 2 else -1
    cells = np.zeros_like(ok)
    cells[bd[0]:bd[1], bd[2]:bd[3]] = True
    return cells


def _min_side(cells):
    """The footprint's narrow side (0 for an empty or degenerate one)."""
    if not cells.any():
        return 0.0
    rr = footprint_polygon(cells).minimum_rotated_rectangle
    if rr.geom_type != 'Polygon' or rr.area <= 0:
        return 0.0
    xy = np.asarray(rr.exterior.coords)
    return float(np.hypot(*np.diff(xy[:3], axis=0).T).min())


def _grow_turned(X, Y, ok, site, ij, w, d, yaw, e):
    """Round forms: a w × d rectangle turned to face the outline (`yaw` =
    outward normal), its outer face on the outline when the site is near it
    (`e` = distance in), clipped to the connected `ok` cells."""
    u = (X - site[0]) * np.cos(yaw) + (Y - site[1]) * np.sin(yaw)    # outward
    v = -(X - site[0]) * np.sin(yaw) + (Y - site[1]) * np.cos(yaw)   # along
    o = min(e, 0.5 * d)
    rect = (u <= o + GRID_MM) & (u >= o - d) & (np.abs(v) <= 0.5 * w)
    lab, _ = ndimage.label(rect & ok)
    return lab == lab[ij] if lab[ij] else np.zeros_like(ok)


def _merge_crumb(rocks, cells, S, X, Y, form, min_h=0.5 * MIN_H_MM):
    """A pocket too small for a stone joins the touching stone whose top
    over it comes nearest the pocket's floor, at least `min_h` over it (its
    base drops to the floor)."""
    ring = ndimage.binary_dilation(cells, iterations=2) & ~cells
    best, bz = None, np.inf
    for r in rocks[::-1]:
        if not (r.cells & ring).any():
            continue
        z = r.top_z(X[cells], Y[cells])
        if form.ztop is not None:
            z = np.minimum(z, form.ztop)
        if z.min() > S[cells].max() + min_h and abs(z.mean() - S[cells].mean()) < bz:
            best, bz = (r, z), abs(z.mean() - S[cells].mean())
    if best is None:                       # nothing fits: leave it as a notch
        return S, False
    r, z = best
    r.cells = r.cells | cells
    r.bed[cells] = S[cells]
    r.z0 = min(r.z0, _sunk(S[cells]))
    S = S.copy()
    S[cells] = np.maximum(S[cells], z)
    return S, True


def _jut(rng, cells, want, S):
    """Extend a footprint out over bare ground on one of its open sides (a
    block standing proud of the face)."""
    ii = np.nonzero(cells.any(1))[0]
    jj = np.nonzero(cells.any(0))[0]
    i0, i1, j0, j1 = ii.min(), ii.max() + 1, jj.min(), jj.max() + 1
    k = int(round(rng.uniform(*JUT_MM) / GRID_MM))
    c = k + int(round(JUT_CLEAR_MM / GRID_MM))          # the jut and the ground beyond it
    nx, ny = cells.shape
    for side in rng.permutation(4):
        if side == 0 and j0 - c >= 0:
            sl, cl = np.s_[i0:i1, j0 - k:j0], np.s_[i0:i1, j0 - c:j0]
        elif side == 1 and j1 + c <= ny:
            sl, cl = np.s_[i0:i1, j1:j1 + k], np.s_[i0:i1, j1:j1 + c]
        elif side == 2 and i0 - c >= 0:
            sl, cl = np.s_[i0 - k:i0, j0:j1], np.s_[i0 - c:i0, j0:j1]
        elif side == 3 and i1 + c <= nx:
            sl, cl = np.s_[i1:i1 + k, j0:j1], np.s_[i1:i1 + c, j0:j1]
        else:
            continue
        if want[cl].any() or (S[cl] > FOOT_Z_MM).any():
            continue
        out = cells.copy()
        out[sl] = True
        return out
    return cells


# ── 3 Finish ────────────────────────────────────────────────────────────────
def footprint_polygon(cells):
    ii, jj = np.nonzero(cells)
    sq = [box(i * GRID_MM, j * GRID_MM, (i + 1) * GRID_MM, (j + 1) * GRID_MM)
          for i, j in zip(ii, jj)]
    # 0.45: straightens the grid staircase of a turned stone's sides (each
    # step would extrude into a narrow facet — fluting)
    poly = shapely.union_all(sq).buffer(0.01).simplify(FOOTPRINT_SIMPLIFY_MM)
    # (no arm narrower than a wall: it would stand as a fin)
    opened = poly.buffer(-0.5 * MIN_WALL_MM, join_style=2).buffer(0.5 * MIN_WALL_MM, join_style=2)
    if not opened.is_empty and opened.area > 0.5 * poly.area:
        poly = opened
    if poly.geom_type == 'MultiPolygon':
        poly = max(poly.geoms, key=lambda g: g.area)
    return shapely.geometry.Polygon(poly.exterior)


def _ccw(poly):
    """The polygon's outline, counter-clockwise (closed)."""
    return np.asarray(shapely.geometry.polygon.orient(poly, 1.0).exterior.coords)


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


@dataclass
class Column:
    """Stones stacked in one column (each sharing most of the other's
    footprint).  Its face-defining planes (dressing) and its faces' relief
    are chosen once, on the column, and cut through every stone in it, so
    its faces run on across the joints (H, N) and each joint reads as a
    groove in one mass, not as a seam between two cushions."""
    stones: list
    poly: object
    zlo: float
    zhi: float
    zone: float
    open: list               # its open sides (_open_faces)
    cuts: list = field(default_factory=list)   # (n, off, face direction): n·x ≥ off taken
    setback: dict = field(default_factory=dict)
    corner_mm: float = 1.0
    step: float = 0.0        # its faces stand back this much (COLUMN_STEP_MM)…
    back: dict = field(default_factory=dict)     # …each stone's this much more {id: mm}
    tops: int = 0            # cracks run on across its stones' bare tops
    refused: set = field(default_factory=set)    # its planes a stone refused (rock_manifold)
    carries: bool = False    # another column's stone rests on it (build)
    faces: list = field(default_factory=list)    # its open faces' relief (Face)
    reach: np.ndarray = None    # the column's (uncut) vertices: how far it reaches each way


def _components(n, pairs):
    """Each of n items' connected component (a label) under the links `pairs`."""
    par = list(range(n))

    def find(i):
        while par[i] != i:
            par[i] = par[par[i]]
            i = par[i]
        return i
    for i, j in pairs:
        par[find(i)] = find(j)
    return [find(i) for i in range(n)]


def _top_z(r, poly=None):
    """How high the stone's top reaches over `poly` (its footprint)."""
    return max(r.top_z(x, y) for x, y in _ccw(r.poly if poly is None else poly)[:-1])


def columns(rng, placed, form, S, zone):
    n = len(placed)
    A = [int(p.cells.sum()) for p in placed]
    bb = [p.poly.bounds for p in placed]
    lab = _components(n, ((i, j) for i in range(n) for j in range(i + 1, n)
                          if bb[i][0] < bb[j][2] and bb[j][0] < bb[i][2] and bb[i][1] < bb[j][3]
                          and bb[j][1] < bb[i][3]
                          and (placed[i].cells & placed[j].cells).sum()
                          >= COLUMN_SHARE * max(A[i], A[j])))
    groups = {}
    for i in range(n):
        groups.setdefault(lab[i], []).append(placed[i])
    cols = []
    for g in groups.values():
        g.sort(key=lambda r: r.z0)
        poly = shapely.union_all([r.poly for r in g]).convex_hull
        zhi = _top_z(max(g, key=_top_z), poly)    # (the highest top, not base)
        if form.ztop is not None:
            zhi = min(zhi, form.ztop)
        zlo = min(r.z0 for r in g)
        back = dict(zip(map(id, g), np.cumsum([0.0] + [rng.uniform(*STONE_STEP_MM) for _ in g[1:]])))
        cols.append(Column(g, poly, zlo, zhi, float(zone(poly.centroid.x)[0]),
                           _open_faces(poly, form, S, zlo, zhi), corner_mm=rng.uniform(*CORNER_MM),
                           setback={rim: rng.uniform(*RIM_SETBACK_MM) for rim in form.rims},
                           step=rng.uniform(*COLUMN_STEP_MM), back=back))
    return cols


def _open_faces(poly, form, S, zlo, zhi):
    """The footprint's sides (p0, p1, outward m, on the tile boundary) that
    look into the open or onto the boundary, not across a joint."""
    zs = zlo + (zhi - zlo) * np.array([0.2, 0.5, 0.8])
    xy = _ccw(poly)
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
    return faces


def _slice(rock, z):
    """The solid's plan section at height z, as shapely polygons."""
    return [shapely.Polygon(q).buffer(0) for q in rock.slice(z).to_polygons() if len(q) > 2]


def _thinness(rock, fr=(0.1, 0.3, 0.5, 0.7, 0.9)):
    """Area (mm², summed over plan slices at these height fractions) of the
    fins of a solid narrower than MIN_WALL_MM: what an opening of that width
    takes off, in pieces at least 1.5 mm long (not a corner's tip)."""
    bb = rock.bounding_box()
    t = 0.0
    for f_ in fr:
        P = _slice(rock, bb[2] + f_ * (bb[5] - bb[2]))
        if P:
            g = shapely.union_all(P)
            cut = g.difference(g.buffer(-0.5 * MIN_WALL_MM).buffer(0.5 * MIN_WALL_MM))
            for q in getattr(cut, 'geoms', [cut]):
                if q.area > 0.05 and q.length > 3.0:      # (perimeter: ≥ 1.5 mm long)
                    t += q.area
    return t


def _prism(poly, z0, h, top, form):
    """The footprint `poly` extruded h up from z0, cut down to the top plane
    z = c + a·x + b·y (`top`) and the form's top."""
    c, a, b = top
    an = np.array([-a, -b, 1.0])
    na = np.linalg.norm(an)
    man = m3d.Manifold.extrude(m3d.CrossSection([_ccw(poly)[:-1]]), h).translate([0, 0, z0])
    man = man.trim_by_plane((-an / na).tolist(), -c / na)
    if form.ztop is not None:
        man = man.trim_by_plane([0, 0, -1], -form.ztop)
    return man


class _Cutter:
    """A solid cut plane by plane: each half-space n·x ≥ off taken unless
    it would take it all, more than CLEAVE_SUPPORT_MM3 of `support` (a rock
    resting here, the core), leave a fin (_thinness up by more than
    THIN_GAIN_MM2) or (`strict`: a new plane) take more than
    CLEAVE_MAX_LOSS of it or leave less than CLEAVE_MIN_KEEP of what it was.  One taking
    under CLEAVE_MIN_MM3 is passed over (a sliver: it would pinch on float32
    STL reload).  `cuts` keeps those taken, (n, off, face direction)."""

    def __init__(self, solid, support=None):
        self.solid, self.vol0, self.thin = solid, solid.volume(), _thinness(solid)
        self.support, self.cuts = support, []

    def take(self, n, off, d=None, strict=True):
        """Take the half-space; False if refused."""
        cut = self.solid.trim_by_plane((-n).tolist(), -off)
        if cut.is_empty():
            return False
        v, v0 = cut.volume(), self.solid.volume()
        if v0 - v < CLEAVE_MIN_MM3:
            return True
        if strict and (v < (1 - CLEAVE_MAX_LOSS) * v0 or v < CLEAVE_MIN_KEEP * self.vol0):
            return False
        if (self.support is not None and not self.support.is_empty()
                and ((self.solid - cut) ^ self.support).volume() > CLEAVE_SUPPORT_MM3):
            return False                              # would undercut a rock resting here
        t = _thinness(cut)
        if t > self.thin + THIN_GAIN_MM2:
            return False                              # would leave a fin
        self.solid, self.thin = cut, t
        self.cuts.append((n, off, d))
        return True

    def take_back(self, n, p, lim, extra=0.0, d=None):
        """The plane normal to n through p, moved out (same angle) to take
        no deeper than lim (+ extra) from the solid's reach along n."""
        depth = (to_mesh(self.solid).vertices @ n).max() - n @ p
        if depth >= 0.3:
            k = np.linalg.norm(n)
            self.take(n / k, float(n @ (p + n * max(depth - lim - extra, 0.0))) / k, d)


def _plan_column(rng, c, form):
    """Plan the column's planes on its hull (the footprints' convex hull up
    to the highest top), each cut there while still convex: faces stood
    back COLUMN_STEP_MM (N: columns jut and recede) and dressed off true
    (turned a few degrees in plan and battered back, so the column reads
    wedge-shaped, not boxed) — none on a column carrying another's stone
    (a ceiling under it).  A plane cut of a convex body leaves only that
    plane, so every edge is a real change of plane (REQ-ROCK-4);
    no cut that leaves a fin is taken.  The planes that cut the hull are
    kept in c.cuts, for every stone of the column."""
    hgt = c.zhi - c.zlo
    if hgt < 2.0:
        return
    hull = _prism(c.poly, c.zlo, hgt + 1.0, max(c.stones, key=_top_z).top, form)
    if hull.is_empty():
        return
    c.reach = to_mesh(hull).vertices
    if c.carries:
        return
    hull = _Cutter(hull)
    for p0, p1, m, rim in c.open:
        L = np.hypot(*(p1 - p0))
        if rim:
            continue
        sb = min(c.step, 0.12 * np.ptp(c.reach @ m))   # (stood back: one plane, the face's own)
        hull.take_back(m, np.r_[p0, c.zlo] - 10.0 * m, 0.0, sb, d=m)
        p0, p1 = p0 - sb * m[:2], p1 - sb * m[:2]
        if L < 3.0 or rng.random() > DRESS_P:
            continue
        piv, far = (p0, p1) if rng.random() < 0.5 else (p1, p0)
        t = np.r_[(far - piv) / L, 0.0]
        psi = np.radians(min(rng.uniform(*DRESS_YAW_DEG),
                             np.degrees(np.arcsin(min(DRESS_MAX_MM / L, 1.0)))))
        nh = m * np.cos(psi) + t * np.sin(psi)            # recedes toward `far`
        be = np.radians(rng.uniform(*DRESS_BATTER_DEG))
        n = nh * np.cos(be) + Z_UP * np.sin(be)
        P = np.array([piv[0], piv[1], c.zlo + hgt * rng.uniform(0.0, 0.3)])
        hull.take_back(n, P, DRESS_MAX_MM, d=m)
    c.cuts = hull.cuts


@dataclass
class Face:
    """One open face of a column, in its own plane: u along it from p0 (unit
    t), z up; outward m.  G is the crack slots' depth (≤ 0) over a
    RELIEF_GRID_MM raster from (0, z0), F the sub-faces' setback (≤ 0);
    `cracks` the crack lines (u, z); `rim`: on the tile boundary."""
    p0: np.ndarray
    t: np.ndarray
    m: np.ndarray
    L: float
    z0: float
    G: np.ndarray
    F: np.ndarray
    cracks: list
    rim: bool = False


def _crack_net(rng, L, z0, z1, joints, want, cracks=(), starts=()):
    """A crack network over the face [0, L] × [z0, z1] (u, z), `want` mm
    more of it on `cracks`: first one from each of `starts` (a point on the
    face's end, a heading in: a crack come round the corner), then each
    started in the barest spot (a random pick, the barer the likelier) and
    grown both ways from it, mostly near vertical or near level, by random
    walks bending a little every CRACK_STEP_MM until each end meets an
    edge, a bed joint (`joints`, polylines) or another crack (a T or Y) —
    never a free end: each splits one block.  Coming closer than
    CRACK_GAP_MM to an edge, joint or crack (a sliver), or along an edge or
    joint (CRACK_ALONG: the strip between would stand as a welt), it turns
    into it and ends there.
    Refused: shorter than CRACK_MIN_MM, or meeting a crack at under
    CRACK_MEET_DEG (a lens)."""
    edge = box(0.0, z0, L, z1).boundary
    joints = [shapely.LineString(j) for j in joints]
    cracks, total = list(cracks), 0.0
    B = shapely.points(np.mgrid[0.25:L:0.5, z0 + 0.25:z1:0.5].reshape(2, -1).T)   # (bare spots)

    def walk(p, th0, up):                             # (pts from p, what it ends on)
        pts, th = [p], th0
        for _ in range(60):
            th = th0 + np.clip(th - th0 + np.radians(rng.normal(0.0, CRACK_BEND_DEG)),
                               -np.radians(CRACK_WANDER_DEG), np.radians(CRACK_WANDER_DEG))
            q = pts[-1] + (1 if up else -1) * rng.uniform(*CRACK_STEP_MM) * _heading(th)
            seg, lines = shapely.LineString([pts[-1], q]), [edge] + cracks + joints
            best = min(((c_, g) for g in lines for c_ in shapely.get_parts(seg.intersection(g))
                        if c_.geom_type == 'Point'), default=None,
                       key=lambda h: h[0].distance(shapely.Point(pts[-1])))
            if best is None:                          # (coming closer than CRACK_GAP_MM, or
                near = [g for g in lines              # along an edge or joint: on into it,
                        if g.distance(shapely.Point(pts[-1])) > CRACK_GAP_MM   # never a sliver)
                        and (g.distance(seg) < CRACK_GAP_MM
                             or g not in cracks and _along(pts[-1], q, g))]
                if not near:
                    pts.append(q)
                    continue
                g = min(near, key=seg.distance)
                a, b = shapely.shortest_line(seg, g).coords
                pts.append(np.asarray(a))
                best = (shapely.Point(b), g)
            return pts + [np.asarray(best[0].coords[0])], best[1]
        return pts, None

    for p, th in starts:
        pts, hit = walk(p + 0.05 * _heading(th), th, True)
        if hit is not None:
            cracks.append(shapely.LineString([p] + pts))
    bare = shapely.distance(B, shapely.union_all([edge] + joints + cracks))
    for _ in range(3 * int(want) + 60 if want else 0):
        w = np.where(bare > CRACK_GAP_MM, bare ** 4, 0.0)
        if total >= want or not w.any():
            break
        p = shapely.get_coordinates(B[rng.choice(len(B), p=w / w.sum())])[0]
        r = rng.random()
        th = np.radians(float(np.clip(rng.normal(0.0, CRACK_LEAN_DEG), -15.0, 15.0))
                        + (0.0 if r < CRACK_LEVEL_P else 90.0)
                        + (_signed(rng, CRACK_DIAG_DEG) if r > 1.0 - CRACK_DIAG_P else 0.0))
        (dn, hit0), (pts, hit) = walk(p, th, False), walk(p, th, True)
        if hit is None or hit0 is None:
            continue
        pts = dn[::-1] + pts[1:]
        line = shapely.LineString(pts)
        if line.length < CRACK_MIN_MM:
            continue
        meet = [(h, a, b) for h, a, b in ((hit0, pts[0], pts[1]), (hit, pts[-1], pts[-2]))
                if h is not edge and h not in joints]
        if any(abs((a - b) @ ch) > np.cos(np.radians(CRACK_MEET_DEG)) * np.hypot(*(a - b))
               * np.hypot(*ch) for h, a, b in meet for ch in [_chord(h, h.project(shapely.Point(a)))]):
            continue                                  # (meeting angle)
        cracks.append(line)
        total += line.length
        bare = np.minimum(bare, shapely.distance(B, line))
    return cracks


def _along(a, b, g):
    """Does the step a → b run along the line g: closer than CRACK_ALONG[1],
    within CRACK_ALONG[0] of its heading there?"""
    s_ = shapely.LineString([a, b])
    if s_.length < 0.2 or s_.distance(g) > CRACK_ALONG[1]:
        return False
    ch = _chord(g, g.project(s_.interpolate(0.5, normalized=True)))
    return abs((b - a) @ ch) > np.cos(np.radians(CRACK_ALONG[0])) * s_.length * np.linalg.norm(ch)


def _heading(th):
    return np.array([np.cos(th), np.sin(th)])


def _chord(line, s):
    """The line's direction at s: the chord 0.3 mm either side of it."""
    a, b = (np.asarray(line.interpolate(s + d).coords[0]) for d in (-0.3, 0.3))
    return b - a


def _plan_faces(rng, cols):
    """The block faces: the columns' open faces (the boundary's too), neighbours
    in one plane (turned under 25°, within 3 mm of it, meeting end to end:
    a column's outline facets, or the next column's face) as one face, so
    its crack network runs on across the joints between them; each column
    keeps the faces it lies on."""
    F, specs = [(c, p0, p1, rim) for c in cols for p0, p1, _, rim in c.open], []
    t_ = [(b - a) / np.hypot(*(b - a)) for _, a, b, _ in F]
    m_ = [np.array([t[1], -t[0]]) for t in t_]
    lab = _components(len(F), (
        (i, j) for i in range(len(F)) for j in range(i + 1, len(F))
        if m_[i] @ m_[j] > np.cos(np.radians(25.0)) and abs((F[j][1] - F[i][1]) @ m_[i]) < 3.0
        and max(F[i][1] @ t_[i], F[j][1] @ t_[i]) - min(F[i][2] @ t_[i], F[j][2] @ t_[i]) < 1.0))
    for g in set(lab):
        ix = [i for i in range(len(F)) if lab[i] == g]
        mem = [F[i][:3] for i in ix]
        m = sum(np.hypot(*(F[i][2] - F[i][1])) * m_[i] for i in ix)
        m /= np.linalg.norm(m)
        t = np.array([-m[1], m[0]])
        ends = np.array([q for _, a, b in mem for q in (a, b)])
        u0, u1 = (ends @ t).min(), (ends @ t).max()
        p0 = t * u0 + m * (ends @ m).max()
        z0, z1 = min(c.zlo for c, _, _ in mem), max(c.zhi for c, _, _ in mem)
        joints, area = [], 0.0
        on = {id(c): c for c, _, _ in mem}.values()
        for c in on:                   # stone tops, and the column's sides
            us = [q @ t - u0 for c2, a, b in mem if c2 is c for q in (a, b)]
            ua, ub = min(us), max(us)
            area += (ub - ua) * (c.zhi - c.zlo) * (0.6 + 0.4 * c.zone)
            us = np.linspace(ua, ub, 12)
            joints += [j for j in (np.c_[us, r.top_z(*(p0[:, None] + t[:, None] * us))]
                                   for r in c.stones if _top_z(r) < c.zhi - 0.5)
                       if z0 + 1 < j[:, 1].min() and j[:, 1].max() < z1 - 1]
            joints += [np.array([[u, c.zlo], [u, c.zhi]]) for u in (ua, ub)
                       if 0.5 < u < u1 - u0 - 0.5]
        want = min(CRACK_PER_MM2 * area, CRACK_STONE_MM * sum(len(c.stones) for c in on))
        specs.append((p0, t, u1 - u0, z0, z1, joints, want,
                      any(F[i][3] for i in ix), on))
    nets = [_crack_net(rng, *sp[2:7]) for sp in specs]
    for _ in range(4):          # a crack reaching a convex corner runs on round it
        starts = _corner_starts(rng, specs, nets)
        for i, st in starts.items():
            nets[i] = _crack_net(rng, *specs[i][2:6], 0.0, nets[i], st)
        if not starts:
            break
    for (p0, t, L, z0, z1, joints, _, rim, on), net in zip(specs, nets):
        fc = _plan_face(rng, p0, t, L, z0, z1, joints, net, rim)
        for c in on:
            c.faces.append(fc)


def _corner_starts(rng, specs, nets):
    """Where each crack ending on a face's end at a convex corner (the next
    face's end within CRACK_CORNER_MM, turning away behind this one's plane)
    should run on round it and nothing there meets it yet (within 0.6 mm):
    {face: [(start (u, z), heading)]}, as level as it came, turned in and
    CRACK_WRAP_DEG off it (no ruled line round the stone)."""
    E, out = [(i, e, sp[0] + e * sp[2] * sp[1]) for i, sp in enumerate(specs) for e in (0, 1)], {}
    for (i, ei, P), (j, ej, Q) in itertools.product(E, E):
        Lj, z0, z1 = specs[j][2:5]
        if j == i or np.hypot(*(P - Q)) > CRACK_CORNER_MM \
                or np.array([specs[i][1][1], -specs[i][1][0]]) @ ((1 - 2 * ej) * specs[j][1]) > -0.05:
            continue
        have = [q[1] for c_ in nets[j] for q in (c_.coords[0], c_.coords[-1])
                if abs(q[0] - ej * Lj) < 0.02] + [q[1] for q, _ in out.get(j, [])]
        for c_ in nets[i]:
            for a, b in ((c_.coords[-1], c_.coords[-2]), (c_.coords[0], c_.coords[1])):
                if abs(a[0] - ei * specs[i][2]) < 0.02 and z0 + 0.3 < a[1] < z1 - 0.3 \
                        and not any(abs(a[1] - h) < 0.6 for h in have):
                    d = np.subtract(a, b)
                    out.setdefault(j, []).append((np.array([ej * Lj, a[1]]), np.arctan2(
                        d[1], (1 - 2 * ej) * abs(d[0])) + np.radians(_signed(rng, CRACK_WRAP_DEG))))
                    have.append(a[1])
    return out


def _plan_face(rng, p0, t, L, z0, z1, joints, cracks, rim):
    """One block face (from p0 along t, L long, outward on its right, z0
    to z1; bed joints and column sides `joints`) split by its `cracks` (with
    the joints) into sub-faces, each a plane tilted FACET_TILT_DEG its own
    way, set back (F) so that across every crack its far side lies
    CRACK_SHIFT_MM deeper all along it (near level: the upper side, a
    shelf), N, and across a level joint the stone above at most
    JOINT_PROUD_MM proud; each crack a groove CRACK_SLOT_MM across,
    CRACK_DEPTH_MM deep (G) all along it, pinching and swelling (CRACK_PINCH)
    across; one shorter than CRACK_FINE_MM a hairline (shallower, no step)."""
    m = np.array([t[1], -t[0]])
    g = RELIEF_GRID_MM
    U, Z = np.meshgrid(np.arange(int(L / g) + 2) * g, z0 + np.arange(int((z1 - z0) / g) + 2) * g,
                       indexing='ij')
    Q = shapely.points(U.ravel(), Z.ravel())
    G, cut = np.zeros(U.size), np.zeros(U.size, bool)
    for cs in cracks:                                 # (exact distances: a raster's steps would
        dc = shapely.distance(Q, cs)                  # show as rungs down a groove)
        cut |= dc < g
        i = np.nonzero(dc < CRACK_SLOT_MM[1])[0]
        s = shapely.line_locate_point(cs, Q[i])
        f = np.clip(0.75 + 0.2 * sum(np.sin(2 * np.pi * rng.uniform(1.0, 3.0) * s / 6.0
                                            + rng.uniform(0, 2 * np.pi)) for _ in range(3)),
                    *CRACK_PINCH)                     # (pinch and swell: 2–6 mm waves)
        w = 0.5 * rng.uniform(*CRACK_SLOT_MM) * f      # (walls anti-aliased over a cell)
        D = rng.uniform(*CRACK_DEPTH_MM) * (CRACK_FINE if cs.length < CRACK_FINE_MM else 1.0)
        G[i] = np.minimum(G[i], -D * np.clip((w - dc[i]) / g + 0.5, 0.0, 1.0))
    if joints:
        cut |= shapely.distance(Q, shapely.MultiLineString([j.tolist() for j in joints])) < g
    # the sub-faces: each a plane, its high edge falling away at its tilt,
    # never further than FACET_MM[2] (a big one tilts less)…
    lab, n = ndimage.label(~cut.reshape(U.shape))
    _, (ii, jj) = ndimage.distance_transform_edt(lab == 0, return_indices=True)
    lab, ix = lab[ii, jj], np.arange(n + 1)
    th = rng.uniform(0.0, 2 * np.pi, n + 1)
    P = np.cos(th)[lab] * U + np.sin(th)[lab] * Z
    lo, hi = ndimage.minimum(P, lab, ix), ndimage.maximum(P, lab, ix)
    k = np.minimum(np.tan(np.radians(rng.uniform(*FACET_TILT_DEG, n + 1))),
                   FACET_MM[2] / np.maximum(hi - lo, 1e-3))
    T = k[lab] * (P - lo[lab])
    # …and set back FACET_MM[0:2], further across each split: its far side
    # (front, back: sub-faces either side at each point along it)
    def level(c_):
        e = np.subtract(c_.coords[-1], c_.coords[0])
        return abs(e[1]) < np.tan(np.radians(CRACK_LEVEL_DEG)) * abs(e[0])
    # (and across a bed joint the stone above at most JOINT_PROUD_MM proud)
    lines = [(c_, rng.uniform(*CRACK_SHIFT_MM)) for c_ in cracks if c_.length >= CRACK_FINE_MM] \
        + [(j, -JOINT_PROUD_MM) for j in map(shapely.LineString, joints) if level(j)]
    E = {}                                  # (level ones first: two sub-faces meeting along two
    for cs, shift in sorted(lines, key=lambda l_: level(l_[0]), reverse=True):
        s = np.arange(0.6, cs.length - 0.5, 0.5)            # (clear of what it ends on)
        x, a, b = (shapely.get_coordinates(cs.interpolate(np.clip(s + d, 0.0, cs.length)))
                   for d in (0.0, -0.2, 0.2))
        nrm = np.c_[a[:, 1] - b[:, 1], b[:, 0] - a[:, 0]]
        nrm /= np.linalg.norm(nrm, axis=1, keepdims=True)       # (its left)
        left = nrm[:, 1] > 0 if level(cs) else np.full(len(s), rng.random() < 0.5)
        side = [tuple(np.clip(np.round([(q[:, 0]) / g, (q[:, 1] - z0) / g]).astype(int).T,
                              0, np.array(U.shape) - 1).T) for q in (x - 0.35 * nrm, x + 0.35 * nrm)]
        fr, bk = np.where(left, lab[side[0]], lab[side[1]]), np.where(left, lab[side[1]], lab[side[0]])
        dT = np.where(left, T[side[0]] - T[side[1]], T[side[1]] - T[side[0]])
        for p, q in sorted(set(zip(fr, bk))):    # lines keep the first one's way)
            if p != q and (q, p) not in E:
                E[p, q] = max(E.get((p, q), -np.inf), dT[(fr == p) & (bk == q)].max() + shift)
    b = rng.uniform(*FACET_MM[:2], n + 1)
    for _ in range(3):
        for (p, q), d in E.items():
            b[q] = min(max(b[q], b[p] + d), FACET_DEEP_MM[rim])
    F = ndimage.gaussian_filter(-(b[lab] + T), FACET_SEAM_MM / g)   # (no raster stairs)
    return Face(p0, t, m, L, z0, G.reshape(U.shape), F, list(cracks), rim)


def _relief(rock, faces, core=False):
    """The stone (or the core) cut back on its column's open faces (N), in
    two passes: its surface found by rays along d from a grid of the face
    plane through its front (RELIEF_GRID_MM; the cracks' CRACK_GRID_MM),
    first cut to the sub-faces' planes down the shear (d SHEAR_DEG
    below the face's inward normal: a height field along it leans nowhere
    past the overhang limit), then the cracks sunk in straight along the
    normal (thin: their ceilings reach under REACH_MM); eased out where the
    face turns away; never closer than CRACK_WALL_MM to its back.  The
    `core` takes only the cracks on the tile boundary (there it backs the
    joints: set back further, they would open), none past the faces' ends
    nor far off their planes, and its front smoothed, not fitted a plane."""
    for fc, a in ((fc, a) for a in (np.radians(SHEAR_DEG), 0.0) for fc in faces
                  if not (a and fc.rim and core)):
        g = RELIEF_GRID_MM if a else CRACK_GRID_MM
        t = to_mesh(rock)
        q = t.vertices[:, :2] - fc.p0
        u, s_ = q @ fc.t, q @ fc.m
        if min(u.max(), fc.L) - max(u.min(), 0.0) < 2.0:
            continue
        u0, u1 = (max(u.min(), 0.0), min(u.max(), fc.L)) if core else (u.min(), u.max())
        on = (u > u0 - 1.0) & (u < u1 + 1.0)
        m3, t3 = np.r_[fc.m, 0.0], np.r_[fc.t, 0.0]
        d3 = -np.cos(a) * m3 - np.sin(a) * Z_UP
        sm = s_[on].max()
        P0 = np.r_[fc.p0, 0.0] + sm * m3                        # (its plane, through its front)
        zp = t.vertices[on, 2] + (sm - s_[on]) * np.tan(a)   # (where each ray crosses it)
        us = np.arange(u0 - 1.0, u1 + 1.0 + g, g)              # (past its ends: no flags)
        zs = np.arange(zp.min(), min(zp.max(), t.vertices[:, 2].max() + 1.5) + g, g)
        U, Z = np.meshgrid(us, zs)                              # (z, u)
        O = P0 + U.reshape(-1, 1) * t3 + Z.reshape(-1, 1) * Z_UP - d3   # (1 mm out)
        loc, ri, ti = t.ray.intersects_location(O, np.tile(d3, (len(O), 1)), multiple_hits=True)
        d = np.einsum('ij,j->i', loc - O[ri], d3)               # depth of each hit
        o = np.lexsort((d, ri))
        ri, d, ti = ri[o], d[o], ti[o]
        first = np.r_[True, ri[1:] != ri[:-1]]
        front, back = np.full((2, len(O)), np.nan)
        al, nz = np.zeros((2, len(O)))
        front[ri[first]], al[ri[first]] = d[first], t.face_normals[ti[first]] @ m3
        nz[ri[first]] = t.face_normals[ti[first], 2]
        nxt = np.nonzero(first)[0] + 1
        nxt = nxt[(nxt < len(ri)) & ~first[np.minimum(nxt, len(ri) - 1)]]
        nxt = nxt[t.face_normals[ti[nxt], 2] > -0.5]        # (out through its seat: no back)
        back[ri[nxt]] = d[nxt]
        front, back, al, nz = (a_.reshape(U.shape) for a_ in (front, back, al, nz))
        hit = ~np.isnan(front)
        # (slots on its face, and on over the chamfers and rounded edges
        # into the joints; not on its underside)
        w = np.where(hit & (nz > -0.5), _rl._smooth01((al - 0.35) / 0.3), 0.0)
        M = ndimage.binary_fill_holes(w > 0.5)
        if M.sum() < 50:
            continue
        E = ndimage.distance_transform_edt(np.pad(M, 1))[1:-1, 1:-1] * g   # (in from its edges)
        c_ = np.array([U, Z - fc.z0]) / RELIEF_GRID_MM
        G = ndimage.map_coordinates(fc.G, c_, order=1, mode='constant')   # (none off the face)
        F = ndimage.map_coordinates(fc.F, c_, order=1, mode='nearest')
        # (the sub-faces eased out to its ends: no fin on a corner between
        # two faces; the cracks run on off them; on the core, neither on
        # whatever lies far off its plane: its far side is no face)
        F = F * _rl._smooth01(np.minimum(U, fc.L - U) / FACET_END_MM)
        if core:
            off = _rl._smooth01(RELIEF_OFF_MM - np.abs(sm + np.cos(a) * (1.0 - front)))
            F, G = F * off, G * off
        # the sub-faces: planes behind the stone's face (a plane fitted to it;
        # the core's front smoothed) — all in front of one cut away flat, over
        # its edges too (no frame), all behind it (its top, a chip) left be
        _, (ii, jj) = ndimage.distance_transform_edt(~hit, return_indices=True)
        X, k = np.stack([np.ones_like(U), U, Z], -1), M & (E > 1.0)
        base = X @ np.linalg.lstsq(X[k], front[k], rcond=None)[0] if not core and k.sum() > 50 \
            else ndimage.gaussian_filter(front[ii, jj], 0.4 / g)
        room = np.clip(np.nan_to_num(back - front, nan=np.inf) - CRACK_WALL_MM, 0.0, None)
        if a:
            # (past its outline the cut runs on as at the outline: a step
            # there would leave a ledge under the stone's edge) blurred only
            # a little (crisp planes), more by its outline (no hooks at its joints)
            cut = (front - np.maximum(np.minimum(front - base + F, 0.0), -room) - CUT_SHORT_MM)[ii, jj]
            wide = np.clip(2.0 * ndimage.gaussian_filter((E < 1.0).astype(float), CUT_BLUR_MM[1] / g), 0, 1)
            fine = ndimage.gaussian_filter(cut, CUT_BLUR_MM[0] / g)
            cut = np.maximum(fine + wide * (ndimage.gaussian_filter(cut, CUT_BLUR_MM[1] / g) - fine), 0.02)
        else:                                 # (only along the cracks: nothing else touched)
            crack = np.minimum(ndimage.gaussian_filter(-w * G, CUT_BLUR_MM[0] / g), room)[ii, jj]
            cut = np.where(crack > 0.01, front[ii, jj] + crack       # (full depth: short
                           - CUT_SHORT_MM * (1.0 - _rl._smooth01(crack / 0.2)), 0.02)   # only at its rim)
        hm = _hung(cut, us, zs, 1.0)                            # (u, z, −d) ∈ … × [1 − cut, 1]
        out = rock - hm.transform(np.c_[t3, Z_UP, -d3, P0])
        if not out.is_empty():
            rock = out
    return rock


def rock_manifold(rng, r, c, form, S, support, seed, crng):
    # neighbours overlap by a hair instead of touching face to face: exact
    # tangent faces survive the union but pinch on float32 STL reload
    poly = r.poly.buffer(JOINT_OVERLAP_MM, join_style=2)
    zhi = _top_z(r, box(*poly.bounds)) + 1.0
    rock = _prism(poly, r.z0, zhi - r.z0, r.top, form)
    if rock.is_empty():
        return None
    # a column on a tile-boundary wall stands back from it a little (shrunk
    # back once finished, never cut: see _fit_tile); band ends stay flush (S4)
    # (only a stone whose whole side lies on the wall: shrinking one that
    # touches it with one arm would open its other joints)
    setback = {}
    for (ax, hi), sb in c.setback.items():
        row = (r.cells[-1] if hi else r.cells[0]) if ax == 0 else \
              (r.cells[:, -1] if hi else r.cells[:, 0])
        if row.sum() >= 0.95 * r.cells.any(axis=ax).sum() > 0:
            setback[(ax, hi)] = sb
    rock = _chamfer_corners(rock, poly, c.corner_mm)
    zb = rock.bounding_box()[2]
    V = to_mesh(rock).vertices
    zlo, ztop_r = V[:, 2].min(), V[:, 2].max()
    faces = _open_faces(r.poly, form, S, zlo, ztop_r)
    opens = [f_[2] for f_ in faces]
    cutter = _Cutter(rock, support)
    for k, (n, off, d) in enumerate(c.cuts):          # the column's planes, on its open faces
        if k not in c.refused and max([d @ m for m in opens], default=-1) > 0.9:
            # a stone standing back from the column's face takes them moved
            # back with it (flush ones: the very same planes), each stone
            # further back than the one under it (STONE_STEP_MM); one it would
            # undercut a stone resting on it by (or leave a fin), none under it takes either
            # (stones come top down): no stone stands proud of the one under it
            sh = d * min(0.0, (V @ d).max() - (c.reach @ d).max())
            if np.linalg.norm(sh) <= 0.3:
                sh = 0.0 * d
            if not cutter.take(n, off + float(n @ (sh - d * c.back.get(id(r), 0.0))), strict=False):
                c.refused.add(k)
    # its own top front edge: a 45° V under the stone it carries (the joint's
    # groove) or, on an open ledge top, a broken shoulder
    loaded = occupied(form, S, [[*r.poly.centroid.coords[0], ztop_r + 0.4]])[0]   # (a stone on it)
    if form.ztop is None or ztop_r < form.ztop - 0.5:
        for p0, p1, m, rim in faces:
            if rim:
                continue
            if loaded:                        # (one flat plane along the edge; none on
                if np.hypot(*(p1 - p0)) < JOINT_MIN_MM:   # a short one: a scoop)
                    continue
                dep = rng.uniform(*JOINT_MM)
                n, extra = (m + Z_UP) / np.sqrt(2.0), 0.5
            elif rng.random() > SHOULDER_P * c.zone:
                continue
            else:
                n = m + Z_UP * rng.uniform(0.6, 1.6)
                n, dep, extra = n / np.linalg.norm(n), rng.uniform(*SHOULDER_MM), 1.0
            q = 0.5 * (p0 + p1)
            cutter.take_back(n, np.array([q[0], q[1], r.top_z(q[0], q[1])]) - n * dep, dep, extra)
    rock = cutter.solid
    if zb > FOOT_Z_MM:                       # sits on rock: seated on the bed
        X, Y = _grid(*r.cells.shape)         # on the bed under it (not sunk under
        own = np.zeros_like(r.cells)         # its open faces where its own column's
        for s_ in c.stones:                  # stone carries it: the same cuts take both)
            if s_ is not r and s_.z0 < r.z0:
                own |= s_.cells & r.cells & (np.abs(s_.top_z(X, Y) - np.nan_to_num(r.bed)) < 0.5)
        rock = _seat(rock, r, [f_[:2] for f_ in faces], own)
        zb = float(np.nanmin(r.bed))
    # broken corners (rocklab's corner chips) and arrises (chips)
    with _rl_params(CHIP_MAX_LOSS=min(_rl.CHIP_MAX_LOSS, CHIP_MAX_MM3 / max(rock.volume(), 1e-6)),
                    EDGE_CHIP_FRAC=0.0):
        rock, _ = _rl.chip_corners_edges(rng, rock.translate([0, 0, -zb]), max(r.D, 4.0))
    # (none on the tile boundary: the next tile meets it there; nor along a
    # joint under a stone: it would undercut the seat bevel resting there)
    ctr = np.array(r.poly.centroid.coords[0])

    def keep(n1, n2, P):
        q = P[:2] + (ctr - P[:2]) / max(np.hypot(*(ctr - P[:2])), 1e-6)    # (1 mm in)
        return (any(abs(n[ax]) > 0.9 and min(P[ax], (form.W, form.H)[ax] - P[ax]) < 1.5
                    for n in (n1, n2) for ax in (0, 1))
                or abs(np.cross(n1, n2)[2]) < 0.6 * np.linalg.norm(np.cross(n1, n2))
                and P[2] > r.top_z(*P[:2]) - 1.6 and occupied(form, S, [[*q, r.top_z(*q) + 0.4]])[0])
    rock = chip_edges(crng, rock.translate([0, 0, zb]), CHIP_MM, keep)   # (seated: none below)
    rock = _relief(rock, c.faces)
    rock, k = _top_cracks(rng, rock, r, c.faces, form, S)
    c.tops += k
    return _fit_tile(_grain(rock, seed, zb), form, setback)


def _top_cracks(rng, rock, r, faces, form, S):
    """A crack reaching the top of a stone whose top is bare runs on across
    it (N; as round a corner, no crack stops at an edge with stone beyond):
    from where it crosses the top's front edge straight back (turned a
    little) until the top ends (an edge, or a stone or the dirt cap resting
    on it: a joint), a slot CRACK_SLOT_MM across, its floor CRACK_DEPTH_MM
    under the top found by rays down, open above (no ceiling).  Returns
    (rock, how many)."""
    t = to_mesh(rock)
    V, cuts = t.vertices, []
    for fc in faces:
        u, sm = (V[:, :2] - fc.p0) @ fc.t, (V[:, :2] - fc.p0) @ fc.m
        if sm.max() < -3.0 or min(u.max(), fc.L) - max(u.min(), 0.0) < 1.0:
            continue                                  # (not on this face)
        us = np.linspace(max(u.min(), 0.0), min(u.max(), fc.L), 24)
        edge = shapely.LineString(np.c_[us, r.top_z(*(fc.p0[:, None] + fc.t[:, None] * us))])
        for cs in fc.cracks:
            for q in shapely.get_parts(cs.intersection(edge)):
                if q.geom_type != 'Point':
                    continue
                th = np.radians(rng.normal(0.0, 6.0))
                d2 = -fc.m * np.cos(th) + fc.t * np.sin(th)
                d3 = np.r_[d2, 0.0]
                o = np.r_[fc.p0 + fc.t * q.x + fc.m * (sm.max() + 0.5), 0.0]
                ss = np.arange(0.0, 40.0, 0.1)
                O = o + ss[:, None] * d3 + [0.0, 0.0, V[:, 2].max() + 2.0]
                loc, ri, _ = t.ray.intersects_location(O, np.tile(-Z_UP, (len(O), 1)), multiple_hits=False)
                h = np.full(len(O), np.nan)
                h[ri] = loc[:, 2]
                ok = (h > r.top_z(*O[:, :2].T) - 2.5) & ~occupied(form, S, np.c_[O[:, :2], h + 0.4])
                i0 = int(np.argmax(ok))
                i1 = i0 + (int(np.argmin(ok[i0:])) if not ok[i0:].all() else len(ok) - i0) - 1
                if not ok[i0] or i0 > 40 or i1 - i0 < 15:
                    continue                          # (bare from its front edge on)
                D = rng.uniform(*CRACK_DEPTH_MM)
                fl = ndimage.minimum_filter1d(h[i0:i1 + 1], 7) - D
                prof = np.r_[[[0.0, fl[0]]], np.c_[ss[i0:i1 + 1], fl], [[ss[i1] + 0.3, fl[-1]]]]
                prof = np.r_[prof, [[prof[-1, 0], fl.max() + 3.0], [0.0, fl.max() + 3.0]]]
                w = rng.uniform(*CRACK_SLOT_MM)
                side = np.cross(d3, Z_UP)
                slot = m3d.Manifold.extrude(m3d.CrossSection([prof]), w)        # (s, z) ccw
                cuts.append(slot.transform(np.c_[d3, Z_UP, side, o - 0.5 * w * side]))
    if not cuts:
        return rock, 0
    out = rock - m3d.Manifold.batch_boolean(cuts, m3d.OpType.Add)
    return (rock, 0) if out.is_empty() else (out, len(cuts))


def _fit_tile(rock, form, setback):
    """Never cut a finished stone at the tile boundary: one that reaches
    past it is shrunk along that axis (about its far side) until its outermost
    point just touches it, so its rounded, grained face meets the plane at a
    few high points.  `setback` {(axis, high side): mm} holds a side further in."""
    lo, hi = np.array(rock.bounding_box()[:3]), np.array(rock.bounding_box()[3:])
    for ax, L in ((0, form.W), (1, form.H)):
        a = FIT_GAP_MM + setback.get((ax, False), 0.0)
        b = L - FIT_GAP_MM - setback.get((ax, True), 0.0)
        l, h = lo[ax], hi[ax]
        nl, nh = max(l, a), min(h, b)
        if (nl, nh) == (l, h) or nh - nl < 0.5 * (h - l):   # inside, or too far to shrink
            continue
        k = (nh - nl) / (h - l)
        sc, tr = [1.0, 1.0, 1.0], [0.0, 0.0, 0.0]
        sc[ax], tr[ax] = k, nl - k * l
        rock = rock.scale(sc).translate(tr)
    return rock


def chip_edges(rng, rock, dep_mm, keep=None):
    """Break every arris sharper than CHIP_DEG (but the underside's) as N's
    are broken: along it a row of flat chips, each a facet knocked dep_mm
    deep (drawn per chip, its plane tilted a little either way) CHIP_LEN_MM
    long, its ends running out along the edge over CHIP_RUN × its depth
    (the upper end of a steep arris's chip at 45°: a slope, never a
    ceiling), each overlapping the last, the odd stretch left sharp.  Only
    the arris: a chip reaches no further over the faces than its own facet.
    None on an arris `keep`(n1, n2, a point) holds.  Taken off at once."""
    t = _rl.to_trimesh(rock)
    if not len(t.faces):
        return rock
    fn, arris = t.face_normals, {}
    for e in np.nonzero(t.face_adjacency_angles > np.radians(CHIP_DEG))[0]:
        n1, n2 = fn[t.face_adjacency[e]]
        if min(n1[2], n2[2]) < -0.5 or n1 @ n2 < -0.98:
            continue                                   # base edges (seat, roof), knife edges
        a = t.vertices[t.face_adjacency_edges[e]]
        if keep is not None and keep(n1, n2, a[0]):
            continue
        key = (*np.round(n1, 2), *np.round(n2, 2), round(float(n1 @ a[0]), 1), round(float(n2 @ a[0]), 1))
        arris.setdefault(key, (n1, n2, []))[2].extend(a)
    chips = []
    for n1, n2, P in arris.values():
        e = np.cross(n1, n2)
        if np.linalg.norm(e) < 1e-6:
            continue
        e /= np.linalg.norm(e)
        P = np.asarray(P)
        sv = P @ e
        o, s, s1 = P[0] - e * sv[0], sv.min() - rng.uniform(0.0, 1.0), sv.max()
        nb = (n1 + n2) / np.linalg.norm(n1 + n2)
        U = [-u * np.sign(u @ (nb - n)) for n, u in ((n1, np.cross(n1, e)), (n2, np.cross(n2, e)))]
        k0, k1 = (1.0 if z_ > 0.3 else CHIP_RUN for z_ in (-e[2], e[2]))   # (its ends' run)
        while s < s1:
            ln, d = rng.uniform(*CHIP_LEN_MM), rng.uniform(*dep_mm)
            ln = max(ln, (k0 + k1) * d + 0.3)
            nc = _rl._tilted(rng, nb, 12.0)
            nc /= np.linalg.norm(nc)
            w = np.cross(e, nc)
            # (how far over each face its facet reaches: the chip's width)
            T = max(d * abs(w @ u) / max(-(nc @ u), 0.05) for u in U) + 0.3
            if rng.random() > CHIP_SKIP_P and nc[2] >= -0.3 and nc[2] - abs(e[2]) >= -1.1 and T < 4.0:
                pts = [o + e * (s + sa) + nc * r_ + w * tt for r_, lo, hi in
                       ((-d, k0 * d, ln - k1 * d), (0.0, 0.0, ln), (1.0, -k0, ln + k1))
                       for sa in (lo, hi) for tt in (-T, T)]
                chips.append(m3d.Manifold.hull_points(np.asarray(pts, dtype=np.float32)))
            s += ln * rng.uniform(0.4, 0.8)
    if not chips:
        return rock
    out = rock - m3d.Manifold.batch_boolean(chips, m3d.OpType.Add)
    return rock if out.is_empty() else out


def _cell(P, shape):
    """The grid cell (i, j) under each point (x, y), clipped to the grid."""
    return (np.clip((P[:, 0] / GRID_MM).astype(int), 0, shape[0] - 1),
            np.clip((P[:, 1] / GRID_MM).astype(int), 0, shape[1] - 1))


def _seat(rock, r, opens, own):
    """Seat a resting rock on the stones under it, each part on the support
    actually beneath it (its bed): its underside is one surface at the
    overhang limit (OVERHANG_DEG, less a margin) that starts BEVEL_MM up its
    side at the outline — the upper half of the V seam, the stone below
    rounds the lower half — crosses the bed BEVEL_MM in, and sinks on into
    the stone below further in — but only SEAT_KEEP_MM deep near an open
    face (`opens`: its sides there) over a stone of its own column (`own`
    cells), which takes the same relief and dressing: a sunk belly would
    come out there as a scoop under the seam.  Never a flat ceiling, and
    nowhere does a straight side run down into the stone below to fill its
    rounded edge (as a flat bottom at the lowest support would, wherever the
    rock bridges a taller one).  Built before the finishing passes, which
    can only carve into it."""
    lo, hi = rock.bounding_box()[2], rock.bounding_box()[5]
    bed = _seat_bed(r)
    seat_mid = float(np.nanmedian(bed[r.cells]))
    # the outline the bevel starts from: the rock at the bevel's top, and
    # wherever its sides flare out below that (else a flat ceiling there)
    rings = [g for z in (seat_mid + BEVEL_MM, lo + 0.05)
             for g in _slice(rock, float(np.clip(z, lo + 0.05, hi - 0.05)))]
    if not rings:
        return rock
    outline = shapely.union_all(rings)
    if outline.geom_type != 'Polygon':
        outline = max(outline.geoms, key=lambda g: g.area)
    # the bed under every point (nearest footprint cell)
    _, (ii, jj) = ndimage.distance_transform_edt(np.isnan(bed), return_indices=True)

    def bed_at(P):
        ij = _cell(P, bed.shape)
        return bed[ii[ij], jj[ij]]

    x0, y0, x1, y1 = np.array(outline.bounds) + [-0.6, -0.6, 0.6, 0.6]
    xs = np.arange(x0, x1 + SEAT_GRID_MM, SEAT_GRID_MM)
    ys = np.arange(y0, y1 + SEAT_GRID_MM, SEAT_GRID_MM)
    PX, PY = np.meshgrid(xs, ys)                                       # (y, x)
    # the underside is the highest of the cones (at the seat slope) hung
    # from each outline point's own seat + bevel: exactly that slope
    # everywhere (the bed's own tilt can't flatten it into a ceiling), and
    # at the outline on the local bed, where the seam shows
    ring = outline.exterior
    B = np.asarray([ring.interpolate(t).coords[0]
                    for t in np.arange(0.0, ring.length, SEAT_GRID_MM)])
    # (a narrow rock's bevel less: its V sinks SINK_MM into its bed at the
    # middle, never hangs over it)
    Q = np.c_[PX.ravel(), PY.ravel()]
    inside = shapely.contains_xy(outline, Q[:, 0], Q[:, 1])
    d = SEAT_SLOPE * shapely.distance(shapely.points(Q), ring)
    bev = float(np.clip(d[inside].max(initial=0.0) - SINK_MM, 0.2, BEVEL_MM))
    hb = bed_at(B) + bev
    # (outside the outline it keeps rising from the local bed: a crest at
    # the outline would be flattened by every grid triangle across it into a
    # ceiling strip, and a dip from a lower part's cones would hang a fin
    # down the side by the grid triangles across it)
    # …and inside never above the local bed + bevel sloping in from the outline:
    # where the bed drops inward (part of the rock over a taller stone) the
    # cones from that taller part would carve the rock away above the low bed
    local = bed_at(Q) + bev
    under = np.empty(len(Q))
    for c0 in range(0, len(Q), 2000):
        sl = slice(c0, c0 + 2000)
        dd = SEAT_SLOPE * np.linalg.norm(Q[sl, None, :] - B[None], axis=2)
        under[sl] = np.where(inside[sl], np.minimum((hb[None] - dd).max(1), local[sl] - d[sl]),
                             local[sl] + d[sl])
    if opens and own.any():
        near = shapely.distance(shapely.MultiLineString([list(map(tuple, o)) for o in opens]),
                                shapely.points(Q)) < SEAT_KEEP_MM[0]
        near &= inside & ndimage.binary_erosion(own, _disc(0.5))[_cell(Q, own.shape)]
        under = np.where(near, np.maximum(under, local - bev - SEAT_KEEP_MM[1]), under)
    under = under.reshape(PX.shape)
    zh = hi + 1.0
    keep = _hung(zh - under, xs, ys, zh)                               # z ∈ [under, zh]
    deep = float(under.min()) - 0.1
    if deep < lo:                                                      # room to sink into
        cs = rock.slice(lo + 0.05)
        rock = rock + m3d.Manifold.extrude(cs, lo + 0.06 - deep).translate([0, 0, deep])
    out = rock ^ keep
    return rock if out.is_empty() else out


def _seat_bed(r):
    """The bed a rock is seated on: its support under each footprint cell,
    every low strip narrower than SEAT_SPAN_MM bridged (a grey closing, off
    the footprint counting as high).  Draped into a narrow gap between its
    supports, or along a row of low cells at its edge, the seat would hang a
    knife-thin fin down it."""
    filled = np.where(r.cells, r.bed, np.nanmax(r.bed))
    closed = ndimage.grey_closing(filled, footprint=_disc(0.5 * SEAT_SPAN_MM), mode='constant',
                                  cval=float(np.nanmax(r.bed)))
    return np.where(r.cells, np.maximum(r.bed, closed), np.nan)


def _grain(rock, seed, z0):
    """The face grain: value-noise fBm FACE_GRAIN_MM deep, one field over
    the whole tile (`seed`), so it runs on across the joints.  Faded in
    above the seat at z0, nothing below it, nor on a face turned down (its
    underside) or facing rock across a slot, halved at sharp edges."""
    base_tm = _rl.to_trimesh(rock)
    sharp = base_tm.face_adjacency_angles > np.radians(_rl.SHARP_EDGE_DEG)
    E = base_tm.vertices[base_tm.face_adjacency_edges[sharp]]
    tm = to_mesh(rock.refine_to_length(_rl.TEX_EDGE_MM))
    P, N = tm.vertices.copy(), tm.vertex_normals
    Lap = trimesh.smoothing.laplacian_calculation(tm, equal_weight=True)
    for _ in range(4):                        # (smoothed: the cut faces' normals are noisy)
        N = Lap @ N
    N /= np.maximum(np.linalg.norm(N, axis=1, keepdims=True), 1e-9)
    gr = fbm(P, seed ^ 0x5A5A, FACE_GRAIN_LEN_MM, octaves=3)
    we = np.ones(len(P))
    if len(E):                                # (distance to points along the sharp edges)
        k = np.maximum(np.ceil(np.linalg.norm(E[:, 1] - E[:, 0], axis=1) / 0.1), 1).astype(int)
        s_ = np.concatenate([np.linspace(0.0, 1.0, n + 1) for n in k])
        Q = np.repeat(E[:, 0], k + 1, axis=0) + s_[:, None] * np.repeat(E[:, 1] - E[:, 0], k + 1, axis=0)
        we = _rl._smooth01(cKDTree(Q).query(P, distance_upper_bound=_rl.EDGE_KEEP_MM)[0]
                           / _rl.EDGE_KEEP_MM)
    w = _rl._smooth01((P[:, 2] - z0 - 0.15) / _rl.TEX_BASE_FADE_MM) * _rl._smooth01((N[:, 2] + 0.5) / 0.4)
    loc, ri, _ = base_tm.ray.intersects_location(P + 0.02 * N, N, multiple_hits=False)
    gap = np.full(len(P), np.inf)             # (none facing the rock across a slot: it
    gap[ri] = np.linalg.norm(loc - P[ri], axis=1)   # would close it, or hang a hook in it)
    w *= _rl._smooth01((gap - 0.8) / 0.8)
    V = P + (FACE_GRAIN_MM * gr * (0.5 + 0.5 * we) * w)[:, None] * N
    out = m3d.Manifold(mesh=m3d.Mesh(vert_properties=np.asarray(V, dtype=np.float32),
                                     tri_verts=np.asarray(tm.faces, dtype=np.uint32)))
    return rock if out.is_empty() else out


def _core(f, S):
    """The core: the pile's height field S set in CORE_SET_MM from every
    open face (a flat-disc erosion) and CORE_DROP_MM under every top, its
    sides on the tile boundary CORE_RIM_MM in (CORE_END_RIM_MM from a band's
    ends).  Cleaving never cuts into it (it joins the support), so it shows
    only where neighbours' rounded corners leave a gap — as the gap's floor,
    never a hole through the pile — carved like the faces it lies behind."""
    C = ndimage.grey_erosion(S, footprint=_disc(CORE_SET_MM), mode='nearest') - CORE_DROP_MM
    zb = FOOT_Z_MM - 1.5                          # under the ground
    core = _height_solid(np.maximum(C, zb + 0.05), zb, tol=0.01)
    rim = [CORE_RIM_MM if not f.rims or (ax, hi) in f.rims else CORE_END_RIM_MM
           for hi in (False, True) for ax in (0, 1)]
    return core ^ m3d.Manifold.cube([f.W - rim[0] - rim[2], f.H - rim[1] - rim[3], 200.0]).translate(
        [rim[0], rim[1], zb - 1.0])


def _hung(D, xs, ys, z):
    """The solid z − D ≤ · ≤ z over the grid xs × ys (D indexed (y, x))."""
    hm = make_heightmap_solid(D, xs[-1] - xs[0], ys[-1] - ys[0], base_h=0.0)
    return _gl.to_manifold(hm).mirror([0, 0, 1]).translate([xs[0], ys[0], z])


def _height_solid(Z, z0, tol=None):
    """The solid z0 ≤ z ≤ Z over grid cell centres (Z indexed (x, y), the
    first centre at GRID_MM / 2); `tol`: adaptive triangulation (planes and
    terraces in few triangles)."""
    nx, ny = Z.shape
    hm = make_heightmap_solid(Z.T - z0, (nx - 1) * GRID_MM, (ny - 1) * GRID_MM, base_h=0.0,
                              error_threshold=tol)
    return _gl.to_manifold(hm).translate([0.5 * GRID_MM, 0.5 * GRID_MM, z0])


def _support_cone(r):
    """The wedge under a resting rock: its footprint at its underside,
    narrowing at the seat slope all the way to a ridge — no flat bottom, so
    whatever the facets leave beneath a rock is a printable slope."""
    poly = r.poly
    lo, hi = 0.0, 0.5 * min(poly.bounds[2] - poly.bounds[0], poly.bounds[3] - poly.bounds[1])
    for _ in range(12):                                    # deepest non-empty inset
        mid = 0.5 * (lo + hi)
        inner = poly.buffer(-mid, join_style=2)
        lo, hi = (mid, hi) if (not inner.is_empty and inner.area > 1e-3) else (lo, mid)
    inner = poly.buffer(-lo, join_style=2)
    rings = [np.c_[np.asarray(poly.exterior.coords), np.full(len(poly.exterior.coords), r.z0)]]
    for g in getattr(inner, 'geoms', [inner]):
        c = np.asarray(g.exterior.coords)
        rings.append(np.c_[c, np.full(len(c), r.z0 - SEAT_SLOPE * lo)])
    return m3d.Manifold.hull_points(np.vstack(rings).astype(np.float32))


def _chamfer_corners(rock, poly, d):
    """Each convex vertical corner of the footprint cut off by a vertical
    plane across its bisector — where four rocks meet the seam opens into a
    V.  Only the corner goes (a prism), so a non-convex footprint keeps its
    arms."""
    xy = _ccw(poly)[:-1]
    bb = rock.bounding_box()
    for k in range(len(xy)):
        a, b, c = xy[k - 1], xy[k], xy[(k + 1) % len(xy)]
        e1, e2 = b - a, c - b
        l1, l2 = np.hypot(*e1), np.hypot(*e2)
        if e1[0] * e2[1] - e1[1] * e2[0] <= 0 or min(l1, l2) < 2.0:
            continue                                     # reflex or tiny: leave it
        u1, u2 = e1 / l1, e2 / l2
        m = np.array([u1[1], -u1[0]]) + np.array([u2[1], -u2[0]])
        m /= np.linalg.norm(m)                          # outward bisector
        p1 = b - u1 * min(d / max(m @ u1, 1e-6), l1)
        p2 = b + u2 * min(d / max(-(m @ u2), 1e-6), l2)
        tri = np.array([p1, p2, b + 2.0 * m])
        pts = [[*q, z] for q in tri for z in (bb[2] - 1.0, bb[5] + 1.0)]
        cut = rock - m3d.Manifold.hull_points(np.asarray(pts, dtype=np.float32))
        if not cut.is_empty():
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
    chains: list              # the length (mm) of each crack
    tops: int                 # cracks run on across bare tops


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
    cols_ = columns(rng, placed, f, S, zone)
    core = _core(f, S)
    cones = {id(r): (_support_cone(r), r.poly.bounds) for r in placed if r.z0 > FOOT_Z_MM}
    rocks, crng = [], np.random.default_rng([seed, 13])   # (chips: their own stream)
    for c in cols_:
        # (a column carrying another's stone stands where it is)
        if any(r.z0 > c.zlo + 1.0 and r.poly.intersection(c.poly).area > 1.0
               for r in placed if all(r is not s_ for s_ in c.stones)):
            c.carries, c.back = True, {}
        _plan_column(rng, c, f)
    _plan_faces(np.random.default_rng([seed, 11]), cols_)   # (its own stream: the stones stay put)
    for c in cols_:
        # what a cut must not undercut: the stones resting on this column
        # from other columns, and the core (its own stones take the same cuts)
        x0, y0, x1, y1 = c.poly.bounds
        own = {id(r) for r in c.stones}
        near = [cn for k, (cn, b) in cones.items() if k not in own and b[0] < x1 + 3
                and b[2] > x0 - 3 and b[1] < y1 + 3 and b[3] > y0 - 3]
        support = m3d.Manifold.batch_boolean(near + [core], m3d.OpType.Add)
        rocks += [m for m in (rock_manifold(rng, r, c, f, S, support, seed, crng) for r in c.stones[::-1])
                  if m is not None]
    # the core where it shows (a gully's back, behind a dressed stone, in a
    # crack: on the tile boundary it lies just behind the stones) carved like
    # the faces it lies behind, and grained: never a flat panel
    faces = list({id(fc): fc for c in cols_ for fc in c.faces}.values())
    core = _grain(_relief(core, faces, core=True), seed, FOOT_Z_MM)
    tm['rocks'] = time.perf_counter() - t1
    t2 = time.perf_counter()
    pile = m3d.Manifold.batch_boolean(rocks + [core], m3d.OpType.Add)
    tm['union'] = time.perf_counter() - t2
    t3 = time.perf_counter()
    ground, gz = foot_ground(f, S, zone, seed)
    parts = [pile, ground, *scree(rng, f, S, zone, gz)]
    tm['scree'] = time.perf_counter() - t3
    t4 = time.perf_counter()
    parts.append(_dirt_layer(f, pile, seed))
    top_ = m3d.Manifold.batch_boolean(parts, m3d.OpType.Add)
    top_ = top_ ^ m3d.Manifold.cube([W, H, 200.0])
    import clifflab as cl
    for tol in EXPORT_RETRY_TOL_MM:          # a near-tangency can open up in float32:
        try:                                # snapped to it, merged a little coarser, retried
            mesh, peg, zmax, _ = cl.attach_base(top_ if tol is None else m3d.Manifold(
                top_.simplify(tol).to_mesh()), cols, rows)
            break
        except RuntimeError:
            if tol == EXPORT_RETRY_TOL_MM[-1]:
                raise
    tm['assemble'] = time.perf_counter() - t4
    tm['total'] = time.perf_counter() - t0
    return Built(mesh, len(rocks), len(placed), peg, zmax, tm, [c_.length for fc in faces for c_ in fc.cracks],
                 sum(c.tops for c in cols_))


def foot_ground(f, S, zone, seed):
    """The dirt ground with a bank in the angle of cliff and ground: highest
    against the face, falling away concave, higher and wider in the
    shattered zones; the same at the tile ends for every seed (S4).
    Returns (solid, absolute height grid (y, x))."""
    relief = _dirt.dirt_relief(seed, f.W, f.H).relief           # (y, x)
    dist = _foot_dist(S)
    xs = (np.arange(S.shape[0]) + 0.5) * GRID_MM
    u = (zone(xs) - ZONE_FLOOR) / (1 - ZONE_FLOOR)
    e = _rl._smooth01(np.minimum(xs, f.W - xs) / (2 * END_MM))   # 0 at the ends
    hh = APRON_END[0] + e * (_lerp(APRON_H_MM, u) - APRON_END[0])
    rr = APRON_END[1] + e * (_lerp(APRON_R_MM, u) - APRON_END[1])
    bank = hh[:, None] * np.clip(1 - dist / rr[:, None], 0, 1) ** APRON_EXP
    # (round a low stone no higher than APRON_SHARE of it: a bank, not a hump over it)
    near = ndimage.grey_dilation(S, footprint=_disc(APRON_R_MM[1])) - FOOT_Z_MM
    bank = np.minimum(bank, APRON_SHARE * np.maximum(near, 0.0))
    xr = (np.arange(relief.shape[1]) + 0.5) * f.W / relief.shape[1]
    fade = _rl._smooth01(np.minimum(xr, f.W - xr) / GROUND_END_MM)   # dirt meets the next tile
    gz = FOOT_Z_MM + relief * fade[None, :] + _resample(bank.T, relief.shape)
    return _gl.to_manifold(make_heightmap_solid(gz, f.W, f.H, base_h=0.0)), gz


def _foot_dist(S):
    """Distance (mm) of every grid cell out from the rock (x, y)."""
    return ndimage.distance_transform_edt(~(S > FOOT_Z_MM + 0.5)) * GRID_MM


def scree(rng, f, S, zone, gz):
    """Fallen stones of the same rock along the foot (REQ-FOOT-1): rocklab
    stones 1–8 mm, mixed and skewed small, at the foot of every open face,
    densest against it and in the shattered zones,
    thinning outward.  Each sits on the dirt where it lands, sunk past any lift its
    tilt gives its base, so no base edge shows as a ceiling.  Grit — small
    stones half-buried — fills the bank against the face."""
    ry, rx = gz.shape

    def ground_min(x, y, r):
        i0, i1 = (int(np.clip(v / f.W * rx, 0, rx - 1)) for v in (x - r, x + r))
        j0, j1 = (int(np.clip(v / f.H * ry, 0, ry - 1)) for v in (y - r, y + r))
        return float(gz[j0:j1 + 1, i0:i1 + 1].min())

    # the foot of every open face, whatever way it looks: ground cells
    # touching rock, each with its outward direction (away from the rock)
    dist = _foot_dist(S)
    gx, gy = np.gradient(dist)
    foot = np.argwhere((dist > 0) & (dist <= 0.5))
    if not len(foot):
        return []
    P = (foot + 0.5) * GRID_MM
    O = np.c_[gx[foot[:, 0], foot[:, 1]], gy[foot[:, 0], foot[:, 1]]]
    O /= np.maximum(np.linalg.norm(O, axis=1, keepdims=True), 1e-9)
    w = 0.5 + zone(P[:, 0])
    w /= w.sum()
    per = len(foot) * GRID_MM / SQUARE_MM          # squares' worth of cliff foot
    sites = []                                    # (foot index, D, reach, block); D < 0: grit

    def pick(n):
        return rng.choice(len(foot), size=max(int(round(n)), 0), p=w)

    for i in pick(SCREE_PER_SQUARE * per):
        D = _lerp(SCREE_D_MM, rng.random() ** SCREE_D_EXP)
        sites.append((i, D, rng.exponential(SCREE_REACH_MM), False))
    for i in pick(SCREE_BLOCKS * per):            # big fallen blocks
        sites.append((i, rng.uniform(*SCREE_BLOCK_MM), rng.exponential(0.3 * SCREE_REACH_MM),
                      True))
    for i in pick(GRIT_PER_SQUARE * per):
        sites.append((i, -rng.uniform(*GRIT_D_MM), rng.uniform(0, APRON_R_MM[0]), False))
    rng.shuffle(sites)
    sites.sort(key=lambda s_: -abs(s_[1]))        # big ones first, small ones fill in
    placed, stones = [], []
    hmin = _rl.HEIGHT_MIN_MM
    with _rl_params(FAMILIES=SCREE_FAMILIES, GRAIN_AMP_MM=ROCK_GRAIN_MM, HEIGHT_MIN_MM=hmin):
        for i, D, reach, block in sites:
            grit, D = D < 0, abs(D)
            x, y = P[i] + O[i] * (D * (0.5 - SCREE_LEAN) + reach)
            if not (D / 2 + 0.3 < x < f.W - D / 2 - 0.3 and D / 2 + 0.3 < y < f.H - D / 2 - 0.3):
                continue
            if any(np.hypot(x - a, y - b) < SCREE_GAP * (D + d) for a, b, d in placed):
                continue
            _rl.HEIGHT_MIN_MM = min(hmin, 0.55 * D)
            st = _block_stone(rng, D) if block else _rl.make_rock(rng, D)[0]
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
    return stones


def _block_stone(rng, D):
    """A fallen block of the cliff: a box broken by a few planes, its edges
    chipped like the stones it fell from.  Base at z = 0."""
    dims = np.array([D, D * rng.uniform(0.55, 0.9), D * rng.uniform(0.45, 0.75)])
    st = m3d.Manifold.cube(dims.tolist()).translate([-dims[0] / 2, -dims[1] / 2, 0.0])
    for _ in range(_count(rng, SCREE_BLOCK_CUTS)):
        n = rng.standard_normal(3)
        n[2] = abs(n[2]) * 0.8
        n /= np.linalg.norm(n)
        reach = 0.5 * float(np.abs(dims * n).sum())
        cut = st.trim_by_plane((-n).tolist(), -(reach - rng.uniform(0.12, 0.3) * D))
        if not cut.is_empty() and cut.volume() > 0.6 * st.volume():
            st = cut
    st = chip_edges(rng, st, (0.05 * D, 0.12 * D))
    st, _ = _rl.chip_corners_edges(rng, st, D)
    return _rl.texture(rng, st)


def make_zones(rng, W):
    """Shattered-zone intensity along the run, ZONE_FLOOR..1: a few
    gaussian clusters, so detail gathers instead of spreading evenly."""
    k = max(1, int(round(W / SQUARE_MM * ZONES_PER_SQUARE)))
    cx, sg = rng.uniform(0, W, k), rng.uniform(*ZONE_SIGMA_MM, k)

    def zone(x):
        g = np.exp(-0.5 * ((np.atleast_1d(x)[:, None] - cx) / sg) ** 2).max(1)
        return ZONE_FLOOR + (1 - ZONE_FLOOR) * g
    return zone


def _dirt_layer(f, pile, seed):
    """Dirt lodged on the rock tops (H, N: the tops are stone, the dirt sits
    in the joints and lows), a layer resting ON the rock.  Slits narrower
    than DIRT_PLUG_MM are filled whole (a dark gap under a mm wide reads as a
    crack in the print); the lows — whatever a grey closing over
    DIRT_FILL_MM fills — take up to DIRT_FILL_DEPTH_MM; open stone a thin
    film of the dirtlab relief over the top smoothed (so the grain's peaks
    show through).  It thins to nothing only at the cliff's lips and steps
    (on the closed top: joints don't count), so it never stands above the
    rock across a step and has no walls; its underside is the rock top sunk
    DIRT_SINK_MM (nothing hangs)."""
    nx, ny = f.T.shape
    X, Y = _grid(nx, ny)
    O = np.c_[X.ravel(), Y.ravel(), np.full(X.size, 1000.0)]
    hit, ri, _ = to_mesh(pile).ray.intersects_location(
        O, np.tile([0.0, 0.0, -1.0], (len(O), 1)), multiple_hits=False)
    Z = np.full(X.size, FOOT_Z_MM - 1.0)
    Z[ri] = hit[:, 2]
    Z = Z.reshape(X.shape)                       # the rock top (x, y)
    B = ndimage.grey_erosion(Z, size=3) - DIRT_SINK_MM

    def edges(Z, step=DIRT_STEP_MM):             # off the rock, every steep slope
        gx, gy = np.gradient(ndimage.gaussian_filter(Z, 0.5 / GRID_MM), GRID_MM)
        e = (Z <= FOOT_Z_MM + 1.0) | (np.hypot(gx, gy) > np.tan(np.radians(DIRT_SLOPE_DEG)))
        Zp = np.pad(Z, 1, mode='edge')
        for di, dj in [(di, dj) for di in (-1, 0, 1) for dj in (-1, 0, 1) if di or dj]:
            e |= np.abs(Zp[1 + di:1 + di + nx, 1 + dj:1 + dj + ny] - Z) > step
        return e                                 # …and every step

    def closing(Z, r_mm):                        # (open ground beyond the tile: a gap
        return np.maximum(Z, ndimage.grey_closing(   # at the tile edge stays a gap)
            Z, footprint=_disc(r_mm), mode='constant', cval=FOOT_Z_MM - 1.0))

    Zc = closing(Z, 0.5 * DIRT_PLUG_MM[0])
    inner = ndimage.distance_transform_edt(~edges(Zc, DIRT_PLUG_MM[0])) * GRID_MM >= DIRT_PLUG_MM[0]
    Z = np.where(inner & (Zc - Z < DIRT_PLUG_MM[1]), Zc, Z)   # the top the dirt lies on
    # (a slit is filled only clear of the face and of any real step: a plug in
    # a joint's mouth is a post)
    base = np.minimum(ndimage.gaussian_filter(Z, DIRT_BLUR_MM / GRID_MM), Z)
    Zf = np.minimum(closing(Z, DIRT_FILL_MM), base + DIRT_FILL_DEPTH_MM)   # as far as it fills
    lip = edges(Zf)
    ramp = _rl._smooth01((ndimage.distance_transform_edt(~lip) - 1.5) * GRID_MM / DIRT_RAMP_MM)
    rel = ndimage.gaussian_filter(            # (anti-aliased onto the coarser grid)
        _resample(_dirt.dirt_relief(seed + 7, f.W, f.H).relief, (ny, nx)).T, 0.8)
    thick = (np.maximum(Zf - base, 0.0)
             + DIRT_FILM_MM[0] + DIRT_FILM_MM[1] * np.maximum(rel, 0.0)) * ramp
    thick *= _rl._smooth01((Z - FOOT_Z_MM - DIRT_LOW_MM) / 3.0)   # (not on a boulder at the foot)
    # none on a lip or off the rock (it is under the rock there anyway, but
    # the grid triangles down a cliff face would stand a sliver in front of it)
    D = np.where(lip, B - 0.05, base + thick)
    if not (D > Z).any():
        return m3d.Manifold()
    zb = FOOT_Z_MM - 2.0
    under = _height_solid(np.pad(B, 1, mode='edge'), zb - 1.0).translate([-GRID_MM, -GRID_MM, 0])
    return _height_solid(np.maximum(D, zb + 0.05), zb) - under


def _resample(a, shape):
    if a.shape == tuple(shape):
        return a
    return ndimage.zoom(a, (shape[0] / a.shape[0], shape[1] / a.shape[1]), order=1)


def overhang_faces(mesh, g=0.1, span_mm=3.0):
    """The faces a printer can't lay, as a slicer sees them: the mesh above
    the base sliced every LAYER_MM (vertical rays on a g mm grid), each
    layer's material farther than REACH_MM + (k − 1) × LAYER_MM × tan
    OVERHANG_DEG from the layer k below (any k up to `span_mm`): one layer
    may reach a line width out, a run of them only as far as the cone.
    Flagged: the downward faces under such material."""
    lo, hi = mesh.bounds
    X, Y = np.meshgrid(np.arange(lo[0] + 0.0137, hi[0], g), np.arange(lo[1] + 0.0091, hi[1], g),
                       indexing='ij')                 # (off the mesh's own grids: no grazing hits)
    O = np.c_[X.ravel(), Y.ravel(), np.full(X.size, lo[2] - 1.0)]
    loc, ri, _ = mesh.ray.intersects_location(O, np.tile(Z_UP, (len(O), 1)), multiple_hits=True)
    nl = int(np.ceil(hi[2] / LAYER_MM)) + 1           # (layer k: the plane (k + ½) LAYER_MM up)
    k = np.clip(np.floor(loc[:, 2] / LAYER_MM + 0.5).astype(int), 0, nl)
    occ = np.zeros((nl + 1, X.size), np.uint8)
    np.add.at(occ, (k, ri), 1)                        # (inside: an odd count of hits below)
    occ = (np.cumsum(occ, 0, dtype=np.uint8) & 1).astype(bool).reshape(nl + 1, *X.shape)[:nl]
    c = LAYER_MM * np.tan(np.radians(OVERHANG_DEG))
    far, bad = [], np.zeros_like(occ)
    for i in range(nl):
        bad[i] = occ[i] & np.any([D > REACH_MM + j * c for j, D in enumerate(far[::-1])], axis=0) \
            if far else False
        far = (far + [g * ndimage.distance_transform_edt(~occ[i])])[-int(span_mm / LAYER_MM):]
    bad = ndimage.binary_dilation(bad, np.ones((1, 3, 3), bool))
    bad[:-1] |= bad[1:]                               # (a face: under layer k or k + 1)
    P, n = mesh.triangles_center, mesh.face_normals
    ix = np.clip(((P[:, :2] - n[:, :2] * 0.5 * g - [lo[0] + 0.0137, lo[1] + 0.0091]) / g).round()
                 .astype(int), 0, np.array(X.shape) - 1)
    kz = np.clip(np.floor(P[:, 2] / LAYER_MM + 0.5).astype(int), 0, nl - 1)
    return (n[:, 2] < 0.0) & (P[:, 2] > 0.05) & bad[kz, ix[:, 0], ix[:, 1]]


def audit_overhang(mesh):
    """Area (mm²) of the faces a printer can't lay (overhang_faces)."""
    return float(mesh.area_faces[overhang_faces(mesh)].sum())



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
    ov = audit_overhang(r.mesh)
    print(f"→ {path}  {len(r.mesh.faces):,} faces, watertight={r.mesh.is_watertight}; "
          f"{r.rocks} rocks; overhang {ov:.1f} mm²; {r.tops} top cracks; "
          + ", ".join(f"{k} {v:.1f}s" for k, v in r.tm.items()))


if __name__ == '__main__':
    main()
