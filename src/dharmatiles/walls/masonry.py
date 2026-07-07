"""
Coursed cut-stone wall — gen-1 prototype (walls campaign E1).

Design: docs/design/walls-coursed-masonry.md.  The wall is
``union(core slab, blocks)``: joints are REVEALS over a recessed core
(watertight, no see-through gaps — R3); every block spans the full wall
thickness so faces, ends, and top are real modeled surfaces (R2).  Each
block is a jittered-box convex hull (chipped arrises) with a small
roundover and broadband micro-relief — the Hirst-m50 chipped-stone read
(R4).  The layout solver samples course heights and bay cuts with a
minimum bond offset, and alternates corner-cell ownership per course
(quoins, R5/R7).

Spine convention: a polyline in SQUARE units (scale-invariant across
DB/OL); the polyline is the wall's OUTER face in plan and the body
extends to the LEFT of the walk direction.  A spine along a tile edge
gives the DungeonBlocks flush-to-edge slab exactly (R1).
"""
from __future__ import annotations

import warnings
from dataclasses import dataclass, field

import numpy as np
import trimesh

from ..core.color import Material, tag as _tag
from ..stone import (clip_to_box, rounded_box, rubble_stone,
                     separate_pinches, stone_relief)
from .openings import (Opening, _JOINT_FRAC, _KEYSTONE, _MIN_KEEP_MM,
                       _SILL_H_MM, _SILL_OVER_MM, arch_arc, band_extent,
                       boundary_units, build_profile, point_inside)

# ── Iteration knobs (module constants while prototyping) ─────────────────────
_FACE_RECESS_MM   = 0.60   # per-block face jitter: outer/inner/end faces sit
                           # 0..this behind their nominal plane, so block
                           # proudness over the core varies block to block
_TOP_SETTLE_MM    = 0.20   # cap blocks dip at most this below the cap plane —
                           # flat cap with per-block life, never a raw plane
_YAW_MAX_DEG      = 1.2    # tiny in-plane block rotation
_TILT_MAX_DEG     = 0.7    # tiny out-of-plane block tilt (pitch/roll): each
                           # face catches its own light tone — without it the
                           # coplanar faces melt into one plane at glancing
                           # angles and the joints stop reading
_MIN_BAY_FRAC     = 0.45   # bays shorter than this × bay_min are merged away
_SEAT_PERCENTILE  = 80.0   # seat height over the footprint (stones convention)
_CORE_ROUND_MM    = 0.6    # mortar-core edge fillet (walls AND floors):
                           # set mortar has no sharp arrises

# Opening-adjacent blocks get a SINGLE LINEAR ANGLED CUT per side that
# clears the surround units in their course band (Shawn: not a literal
# curve-tracing cut).  The cut is one extra plane in the block kernel's
# smooth-max, so the cut edges get the SAME roundover treatment as
# every other arris.  The block box extends this margin past the cut
# line so the kernel's own end plane never competes with the cut plane.
_CUT_MARGIN_MM = 1.2

# Rubble hearting (E10/E27, chassis-level since the ruins work): a
# sealed sheet of small rough stone chips through the wall body.
# Fieldstone uses it always (drystone fill); mortared families get it
# when ruined — the broken top shows packed rubble core, the
# hadrians-coursed-rubble.jpg read.
_RUBBLE_SPACING_MM = 2.8
_RUBBLE_FOOT      = (3.5, 5.5)
_RUBBLE_H         = (3.0, 4.5)
_RUBBLE_SETBACK_MM = 1.3

# Ruin state (city ruins / Hadrian's): a smooth per-segment height
# envelope breaks the top; straddling blocks survive at random (ragged
# steps); shed rubble collects at the foot.
_RUIN_DROP        = (0.20, 0.80)  # envelope drop = ruin × H × this range
_RUIN_KEEP_P      = 0.5           # P(straddling block survives)
_RUIN_WOBBLE_WL   = (45.0, 110.0) # envelope wavelengths (2 sines): LONGER
                                  # than a segment — the break line must
                                  # undulate slowly across the wall;
                                  # short wavelengths shred small-stone
                                  # families into fence pickets
_FOOT_RUBBLE_EVERY_MM = 5.5       # one foot shard per this × (1/ruin)
_FOOT_RUBBLE_OFF  = (0.8, 6.5)    # shard offset outside the faces
_FOOT_RUBBLE_FOOT = (2.2, 4.8)
_FOOT_RUBBLE_H    = (1.8, 3.4)

# Texture presets: the surface character of the blocks.  Everything else
# (layout, joints, core) is family-independent.
_TEXTURES = {
    'chipped': dict(chip_mm=0.55, roundover_mm=0.22, relief_mm=0.07,
                    relief_wl=(1.5, 6.0)),   # Hirst-m50 chipped cut stone
    'worn':    dict(chip_mm=0.30, roundover_mm=0.60, relief_mm=0.10,
                    relief_wl=(2.5, 8.0)),   # soft weathered (m24 cryptstone)
    'hewn':    dict(chip_mm=0.80, roundover_mm=0.15, relief_mm=0.16,
                    relief_wl=(0.8, 3.0)),   # rough-hewn all-over (m40)
    'dressed': dict(chip_mm=0.15, roundover_mm=0.12, relief_mm=0.03,
                    relief_wl=(2.0, 7.0)),   # near-smooth ashlar, crisp arris
}


# ── Layout ────────────────────────────────────────────────────────────────────

@dataclass
class _Seg:
    a: np.ndarray          # outer-face start (mm)
    d: np.ndarray          # unit run direction
    n: np.ndarray          # unit inward normal (left of d)
    L: float


@dataclass
class _Cell:
    """One block cell: an interval on a segment × a course."""
    seg:   int
    t0:    float
    t1:    float
    end0:  str             # 'joint' | 'face' | 'press'  (start side
    end1:  str             # along d; press = butts an opening surround)
    z0:    float           # course interval, wall-local z (0 = seat)
    z1:    float
    is_top:    bool
    is_bottom: bool
    is_quoin:  bool = False   # bay runs through a corner cell
    key:   tuple = field(default=())


def assemble_masonry(parts: list[trimesh.Trimesh], surface,
                     label: str) -> trimesh.Trimesh:
    """The masonry assembly tail shared by walls AND floors: manifold
    union of core + units, tile-boundary clip, pinch separation, the
    watertight warning, ROCK tagging.  A masonry layer is
    ``assemble_masonry(core ∪ units)`` regardless of whether the units
    pave a spine (walls) or a plan grid (floors)."""
    solid = trimesh.boolean.union(parts, engine='manifold')
    solid = CutStoneWall._clip_to_tile(solid, surface)
    separate_pinches(solid)
    if not solid.is_watertight:
        warnings.warn(f'{label} union is not watertight', RuntimeWarning)
    _tag(solid, Material.ROCK)
    return solid


def _frame(seg: _Seg, t: float = 0.0, q: float = 0.0,
           z: float = 0.0) -> np.ndarray:
    """4×4 transform from a segment-local frame (x along the run, y the
    inward normal) at run position *t*, depth *q*, world height *z*."""
    m = np.eye(4)
    m[:2, 0] = seg.d
    m[:2, 1] = seg.n
    m[:2, 3] = seg.a + seg.d * t + seg.n * q
    m[2, 3]  = z
    return m


def _segments(spine_mm: list[tuple[float, float]]) -> list[_Seg]:
    segs = []
    for a, b in zip(spine_mm[:-1], spine_mm[1:]):
        a = np.asarray(a, dtype=float)
        b = np.asarray(b, dtype=float)
        v = b - a
        L = float(np.linalg.norm(v))
        if L < 1e-6:
            raise ValueError('wall spine has a zero-length segment')
        d = v / L
        n = np.array([-d[1], d[0]])
        segs.append(_Seg(a=a, d=d, n=n, L=L))
    for s1, s2 in zip(segs[:-1], segs[1:]):
        if abs(float(s1.d @ s2.d)) > 1e-3:
            raise ValueError('gen-1 wall corners must be right angles')
    return segs


def _course_heights(height_mm: float, lo: float, hi: float,
                    rng: np.random.Generator) -> np.ndarray:
    """Varied course heights that sum EXACTLY to height_mm (cap lands at H)."""
    n = max(1, int(round(height_mm / ((lo + hi) / 2.0))))
    h = rng.uniform(lo, hi, n)
    return h * (height_mm / h.sum())


def _bay_cuts(t0: float, t1: float, lo: float, hi: float,
              prev: np.ndarray, min_bond: float,
              rng: np.random.Generator) -> np.ndarray:
    """Interior cut positions in [t0, t1] honouring the bond offset (R5)."""
    L = t1 - t0
    n = max(1, int(round(L / ((lo + hi) / 2.0))))
    w = rng.uniform(lo, hi, n)
    cuts = t0 + np.cumsum(w)[:-1] * (L / w.sum())
    # Nudge any cut that lands over a cut in the course below.
    for i, c in enumerate(cuts):
        if prev.size == 0:
            break
        j = int(np.argmin(np.abs(prev - c)))
        dv = c - prev[j]
        if abs(dv) < min_bond:
            cuts[i] = prev[j] + (min_bond if dv >= 0 else -min_bond)
    cuts = np.sort(np.clip(cuts, t0 + _MIN_BAY_FRAC * lo,
                           t1 - _MIN_BAY_FRAC * lo))
    if cuts.size == 0:
        return cuts
    # Drop cuts the nudging pushed too close together.
    keep = np.concatenate([[True], np.diff(cuts) > _MIN_BAY_FRAC * lo])
    return cuts[keep]


def _layout(segs: list[_Seg], thickness_mm: float, height_mm: float,
            course_mm: tuple[float, float], bay_mm: tuple[float, float],
            min_bond_mm: float, rng: np.random.Generator) -> list[_Cell]:
    """Courses × bays × quoin alternation → block cells (R5, R7).

    Wall ends are always textured ('face'): visible block ends with the
    mortar core inset behind them — including at tile boundaries, where
    two wall tiles butt with their sculpted ends (R8, Shawn's round-2
    call: "I should be able to see the ends of the bricks").
    """
    T = thickness_mm
    heights = _course_heights(height_mm, *course_mm, rng)
    z_edges = np.concatenate([[0.0], np.cumsum(heights)])
    n_joints = len(segs) - 1

    cells: list[_Cell] = []
    prev_cuts: list[np.ndarray] = [np.empty(0)] * len(segs)
    for c, (z0, z1) in enumerate(zip(z_edges[:-1], z_edges[1:])):
        is_top    = c == len(heights) - 1
        is_bottom = c == 0
        for k, seg in enumerate(segs):
            # Corner-cell ownership alternates per course (quoins, R7):
            # the owner's end block runs THROUGH the T×T corner cell; the
            # other segment butts against it.
            owns_start = k > 0 and (c + (k - 1)) % 2 != 0
            owns_end   = k < n_joints and (c + k) % 2 == 0
            t0 = 0.0 if (k == 0 or owns_start) else T
            t1 = seg.L if (k == n_joints or owns_end) else seg.L - T
            if t1 - t0 < _MIN_BAY_FRAC * bay_mm[0]:
                continue
            cuts = _bay_cuts(t0, t1, *bay_mm, prev_cuts[k], min_bond_mm, rng)
            # No cut inside an owned corner cell (+ margin): a joint at
            # t < T makes a tiny sliver quoin and the corner arris reads
            # as a broken column of pebbles (fieldstone E11; cut stone's
            # bay_min > T made this unreachable there).
            if k > 0 and owns_start:
                cuts = cuts[cuts >= T + 2.0]
            if k < n_joints and owns_end:
                cuts = cuts[cuts <= seg.L - T - 2.0]
            prev_cuts[k] = cuts
            edges = np.concatenate([[t0], cuts, [t1]])
            for b, (ta, tb) in enumerate(zip(edges[:-1], edges[1:])):
                cells.append(_Cell(
                    seg=k, t0=float(ta), t1=float(tb),
                    end0='joint' if ta > t0 or (k > 0 and not owns_start)
                         else 'face',
                    end1='joint' if tb < t1 or (k < n_joints and not owns_end)
                         else 'face',
                    z0=float(z0), z1=float(z1),
                    is_top=is_top, is_bottom=is_bottom,
                    # Both cells at a corner are quoin-class — the
                    # through-stone AND the butting stone (a rounded
                    # butting stone curves away from the arris and
                    # leaves a V-notch against the square below).
                    is_quoin=((ta == t0 and k > 0)
                              or (tb == t1 and k < n_joints)),
                    key=(c, k, b),
                ))
    return cells


# ── Block body ────────────────────────────────────────────────────────────────

def _block_mesh(lx: float, ly: float, lz: float,
                chip_mm: float, roundover_mm: float, relief_mm: float,
                relief_wl: tuple[float, float],
                is_top: bool, rng: np.random.Generator,
                pull_mask: tuple = (1.0, 1.0, 1.0, 1.0),
                taper: tuple[str, float] | None = None,
                cut_planes: list | None = None) -> trimesh.Trimesh:
    """One block in local frame [0,lx]×[0,ly]×[0,lz] (x along run, y = depth).

    Jittered-box hull: corners pulled inward (chipped arrises) around
    full-extent face centres, a chamfer ring above the base kills the
    proud-lip overhang (R10), then small roundover + broadband
    micro-relief for the drybrush catch (R4/R9).
    """
    # Jitter is drawn PER RING x PER SIDE, never per corner: each face
    # is then bounded by two parallel horizontal lines — coplanar by
    # construction.  Per-corner jitter warped every side quad, and
    # qhull triangulates a warped quad with a DIAGONAL — the straight
    # crease line across every wall block face since E1 (Shawn kept
    # finding it; floors never showed it because their visible face is
    # the top ring, which was already planar).  The block is a stack
    # of jittered planar rings: an irregular frustum with straight
    # chipped arrises and calm faces.
    # pull_mask (x0, x1, y0, y1) zeroes the chip pull per side: floor
    # slabs at a TILE BOUNDARY must run flush to it (the inter-slab
    # spacing is between slabs on a tile, not between a slab and the
    # tile edge — Shawn); draws happen regardless so rng streams don't
    # depend on the mask.
    # taper = (axis, mm): voussoir WEDGE — the x-extent grows by mm/2
    # per side from the min face to the max face of the given axis
    # ('z': between the two rings; 'y': within each ring).  Radial
    # joints on a curved ring gape at the outer radius when units are
    # rectangular; the wedge follows the ring's angular pitch.  The
    # LOCAL max side is the OUTER face by _place_posed's conventions
    # (both frames map it to the outward normal).
    t_axis, t_mm = taper if taper is not None else ('z', 0.0)
    pts = []
    def _ring(z: float, inset: float, pull: float, widen: float = 0.0):
        # Clamp per-axis: on small units (arch voussoirs) an unclamped
        # chip pull can invert the ring and degenerate the hull.
        px = min(pull, 0.22 * lx)
        py = min(pull, 0.22 * ly)
        x0 = inset + rng.uniform(0.0, px) * pull_mask[0]
        x1 = lx - inset - rng.uniform(0.0, px) * pull_mask[1]
        y0 = inset + rng.uniform(0.0, py) * pull_mask[2]
        y1 = ly - inset - rng.uniform(0.0, py) * pull_mask[3]
        # t_mm is the FULL width difference between the outer and the
        # inner face; each of the two x sides moves by t/4 per face.
        wy0 = widen - (t_mm / 4.0 if t_axis == 'y' else 0.0)
        wy1 = widen + (t_mm / 4.0 if t_axis == 'y' else 0.0)
        pts.extend([[x0 - wy0, y0, z], [x1 + wy0, y0, z],
                    [x1 + wy1, y1, z], [x0 - wy1, y1, z]])

    z_top_pull = _TOP_SETTLE_MM if is_top else 0.5 * chip_mm
    wz = t_mm / 4.0 if t_axis == 'z' else 0.0
    _ring(lz - rng.uniform(0.0, z_top_pull), 0.0, chip_mm, +wz)  # top ring
    # Bottom ring identical to the top (Shawn: bricks have the SAME
    # edge everywhere — the old bottom chamfer read as round-bottomed
    # bricks).  The R10 overhang it guarded against is the block lip
    # over the sub-mm joint recess; the official pieces have square
    # bottoms there and print fine.
    _ring(0.0, 0.0, chip_mm, -wz)                               # bottom ring

    # EXACT tessellation — no voxel grid anywhere.  Blocks went
    # through blur_remesh (binary occupancy + marching cubes) for
    # years, and MC of a binary grid stairsteps planar faces at EVERY
    # angle: grid-aligned faces band at multi-mm wavelengths, rotated
    # ones washboard at pitch/sin(angle) (Shawn's MeshLab finds, three
    # rounds).  A block is a CONVEX body we constructed analytically,
    # so sample its exact signed distance (max over hull face planes)
    # and march THAT: MC interpolates linearly along grid edges, and
    # the SDF is linear near faces, so vertices land exactly on the
    # planes — zero ripple by construction.  Extracting the isosurface
    # at +roundover IS the Minkowski roundover, replacing round_edges.
    from skimage import measure as _measure
    hull = trimesh.convex.convex_hull(np.asarray(pts))
    N = np.asarray(hull.face_normals)
    D = (N * hull.triangles[:, 0, :]).sum(axis=1)
    # Dedupe coplanar triangle planes (qhull splits every face): LSE
    # sums duplicates as +tau*ln(k) — a uniform inward shrink.
    _, uniq = np.unique(np.round(np.column_stack([N, D]), 3),
                        axis=0, return_index=True)
    N, D = N[uniq], D[uniq]
    if cut_planes:
        # Opening-fit cuts (local frame): ordinary kernel planes, so
        # the cut arris gets the same LSE roundover as every edge.
        N = np.vstack([N] + [n for n, _d in cut_planes])
        D = np.concatenate([D, [d for _n, d in cut_planes]])
    pitch0 = float(np.clip(max(lx, ly, lz) / 56.0, 0.18, 0.32))
    # Rare grid resonances (vertex-merge knife edges in the MC output)
    # make a non-volume mesh for specific (dims, pitch) pairs; a small
    # pitch nudge lands off the resonance.
    for nudge in (1.0, 1.073, 0.941):
        pitch = pitch0 * nudge
        # Smooth-max (log-sum-exp) of the plane fields: on a face one
        # plane dominates and the level-0 set IS the exact plane (zero
        # ripple); where planes meet, the LSE blend pulls the surface
        # in by ~tau*ln2 — the roundover, analytically.  tau is
        # floored at the grid pitch so the blend is resolved (a hard
        # max creases the field and MC slivers the crease into
        # non-manifold edges).
        # tau capped by the block's own size: LSE lifts the field by
        # up to tau*ln(k) where planes crowd, and a tau beyond ~a
        # quarter of the smallest half-dimension lifts EVERYTHING
        # positive — the isosurface vanishes (found on fieldstone
        # jambs: roundover ~2 on a 4mm unit).
        tau = max(min(0.7 * roundover_mm, 0.22 * min(lx, ly, lz)),
                  0.6 * pitch)
        pad = 2.5 * pitch
        lo = hull.bounds[0] - pad
        hi = hull.bounds[1] + pad
        axes = [np.arange(lo[k], hi[k] + pitch, pitch) for k in range(3)]
        G = np.stack(np.meshgrid(*axes, indexing='ij'), axis=-1)
        acc = None
        for n_i, d_i in zip(N, D):
            f = (G @ n_i - d_i) / tau
            acc = f if acc is None else np.logaddexp(acc, f)
        field = tau * acc
        mv, mf, _n, _v = _measure.marching_cubes(
            field, level=0.0, spacing=(pitch, pitch, pitch))
        body = trimesh.Trimesh(vertices=mv + lo, faces=mf, process=True)
        body.fix_normals()
        if body.is_volume:
            break
    else:
        warnings.warn('block mesh is not a volume after pitch nudges',
                      RuntimeWarning)
    if relief_mm > 0.0:
        # THE FLOOR RECIPE at block scale (Shawn: "fix the walls to
        # look like the floor"): same deep worn-recess carve the slabs
        # get — full stone_relief defaults, depth scaled by the preset
        # (relief_mm/0.10 ≈ 1 for 'worn') and capped by block size so
        # small bricks aren't eaten.  Shallow carve is WORSE than
        # none: below ~0.4 mm only the rim reads and it looks like
        # engraved scratches.
        carve = min(0.62 * relief_mm / 0.10, 0.62,
                    0.14 * min(lx, ly, lz))
        body = stone_relief(body, rng,
                            carve_mm=carve,
                            dish_mm=min(0.25, 0.06 * min(lx, ly, lz)),
                            base_fade_mm=1.2)
    return body


# ── Layer ─────────────────────────────────────────────────────────────────────

class CutStoneWall:
    """Direct TileLayer: a coursed cut-stone wall on a plan spine.

    ``spine``: polyline in SQUARE units — the OUTER face line in plan;
    the body extends to the LEFT of the walk direction.  Walk a tile-edge
    spine with the tile interior on your left for the DB flush slab.

    Stamps ``terrain_support_z`` and ``obstacle_mask`` over the footprint
    (same contracts as Rocks/FacetedStones) so grass steers around.

    **This class is also the masonry CHASSIS** — everything that is
    family-independent about a per-unit wall: spine → segments, the
    courses × bays × quoins layout, the recessed core, seat/stamp/clip,
    the manifold union, and the export guarantees.  A wall FAMILY is
    this chassis plus a unit kernel, via the subclass hooks:

    - ``_cells``       — post-process the layout (merges, splits)
    - ``_stone_box``   — the unit's box within its jointed cell
    - ``_unit_mesh``   — the solid that fills one cell (THE family axis)
    - ``_place_block`` — full per-cell override when the unit isn't
      box-shaped (fieldstone's crack-outline sphere-morph)
    - ``_core_boxes``  — core extent (fieldstone stops it below the cap)
    - ``_extra_parts`` — additional solids (fieldstone rubble hearting)

    ``FieldstoneWall`` is the worked example; a brick family is this
    chassis with a small-format unit kernel; a cliff face is the same
    chassis at geological scale (bedding strata = courses).  Shape and
    finish primitives for unit kernels live in ``dharmatiles.stone``.
    """

    height_default_mm: float = 5.0
    yaw_max_deg:    float = _YAW_MAX_DEG
    tilt_max_deg:   float = _TILT_MAX_DEG
    face_recess_mm: float = _FACE_RECESS_MM

    def __init__(self, spine: list[tuple[float, float]], *,
                 thickness_mm: float = 7.0,
                 height_mm:    float | None = None,
                 top_mm:       float = 33.1,   # official wall-top
                                                # elevation above the
                                                # datum (49.7 piece bbox
                                                # − 16.6 tall base).  The
                                                # default wall hits this
                                                # TOP regardless of what
                                                # it seats on — soil,
                                                # grass, or pavement — so
                                                # finished heights always
                                                # rank with the official
                                                # pieces (tall ≈ 72.3).
                                                # An explicit height_mm
                                                # overrides with a
                                                # seat-relative extent.
                 seed:         int   = 0,
                 texture:      str   = 'chipped',
                 course_mm:    tuple[float, float] = (5.5, 8.5),
                 bay_mm:       tuple[float, float] = (11.0, 20.0),
                 joint_mm:     float = 0.11,  # halved twice 2026-07-06 (Shawn)
                 reveal_mm:    float = 1.3,
                 chip_mm:      float | None = None,
                 roundover_mm: float | None = None,
                 relief_mm:    float | None = None,
                 relief_wl:    tuple[float, float] | None = None,
                 min_bond_mm:  float = 3.0,
                 embed_mm:     float = 2.5,
                 crenellated:  bool = False,
                 merlon_mm:    tuple[float, float] = (8.0, 11.0),
                 crenel_mm:    tuple[float, float] = (5.5, 7.5),
                 crenel_depth_mm: float = 11.0,
                 ruin:         float = 0.0,
                 laid_flat:    bool = False,
                 openings:     list | None = None):
        if len(spine) < 2:
            raise ValueError('wall spine needs at least two points')
        if texture not in _TEXTURES:
            raise ValueError(f'unknown wall texture {texture!r}; '
                             f'options: {sorted(_TEXTURES)}')
        preset = _TEXTURES[texture]
        self.spine        = [tuple(map(float, p)) for p in spine]
        self.thickness_mm = thickness_mm
        self.height_mm    = height_mm
        self.top_mm       = top_mm
        self.seed         = seed
        self.texture      = texture
        self.course_mm    = course_mm
        self.bay_mm       = bay_mm
        self.joint_mm     = joint_mm
        self.reveal_mm    = reveal_mm
        self.chip_mm      = preset['chip_mm'] if chip_mm is None else chip_mm
        self.roundover_mm = (preset['roundover_mm'] if roundover_mm is None
                             else roundover_mm)
        self.relief_mm    = (preset['relief_mm'] if relief_mm is None
                             else relief_mm)
        self.relief_wl    = (preset['relief_wl'] if relief_wl is None
                             else relief_wl)
        self.min_bond_mm  = min_bond_mm
        self.embed_mm     = embed_mm
        self.crenellated  = crenellated
        self.merlon_mm    = merlon_mm
        self.crenel_mm    = crenel_mm
        self.crenel_depth_mm = crenel_depth_mm
        self.ruin         = float(np.clip(ruin, 0.0, 1.0))
        # A FLOOR IS A WALL LYING ON ITS SIDE (Shawn): with laid_flat
        # the wall's HEIGHT axis becomes the second plan axis (courses
        # = pavement rows, bays = slabs, bond = row stagger), and its
        # THICKNESS becomes the pavement depth with the OUTER face up
        # (face proudness = per-stone pavement height).  height_mm is
        # then the paved strip's plan depth and must be explicit.
        self.laid_flat    = laid_flat
        self.openings     = list(openings or [])
        self._op_profiles: list = []   # [(seg_i, P, op, tc)] per build
        self._merlons: list[list[tuple[float, float]]] = []   # per segment
        self._crenel_z = None      # wall-local z of the crenel floor
        self._ruin_env = None      # per-segment envelope callables

    #: families with a drystone interior build the hearting always;
    #: mortared families get it when ruined (broken top shows rubble).
    hearting: bool = False

    #: opening-surround sizing (families override): voussoir width,
    #: voussoir ring depth, jamb depth, jamb block height range.
    surround_vw:   float = 4.6
    surround_ring: float = 4.2
    surround_jd:   float = 4.2
    surround_jh:   tuple = (4.5, 7.0)
    #: opening-surround finish — INDEPENDENT of the body texture: the
    #: surround is dressed work in every family (design: "structurally
    #: distinct SURROUND").  Surround units are 2–5 mm; the body's chip
    #: budget (up to 0.22×dim per side) and roundover shrank them into
    #: gapped pebbles (the E13 fieldstone bead-chain arch).
    surround_chip: float = 0.15
    surround_ro:   float = 0.18
    #: unit width as a fraction of the pitch along the boundary / jamb
    #: stack: < 1 leaves a mortar joint, > 1 presses units into each
    #: other (drystone — the union fuses the contact).
    surround_frac: float = _JOINT_FRAC
    #: surround units stand proud of both wall faces (refs 05–07: the
    #: surround is a distinct order, not flush bond).
    surround_proud_mm: float = 0.30
    #: how far the fitted bond presses INTO the surround units.  The
    #: opening-adjacent blocks overshoot into the surround and are
    #: boolean-cut against the units dilated by (joint_mm − press):
    #: 0 for mortared families (the bond meets the surround at an
    #: ordinary thin mortar joint, the fitted face hugging the curve);
    #: drystone overrides — stones must TOUCH, so the cut leaves them
    #: interpenetrating and the union fuses the contact.
    surround_bond_press: float = 0.0

    def _lay(self, seg: _Seg) -> np.ndarray:
        """Flat-mode frame for meshes built in raw wall coordinates
        (t, q, z_wall): t stays along the run, z_wall becomes the
        second plan axis (seg.n), and q becomes DEPTH — the outer face
        (q = 0) lands on top at z = thickness."""
        mm = np.eye(4)
        mm[:3, 0] = [seg.d[0], seg.d[1], 0.0]
        mm[:3, 1] = [0.0, 0.0, -1.0]
        mm[:3, 2] = [seg.n[0], seg.n[1], 0.0]
        mm[:2, 3] = seg.a
        mm[2, 3]  = self.thickness_mm
        return mm

    # ── build ────────────────────────────────────────────────────────────────
    def apply(self, scene, *, placement_mask=None) -> list[trimesh.Trimesh]:
        surface = scene.surface
        sq = surface.square_mm
        self._sq = sq
        segs = _segments([(x * sq, y * sq) for x, y in self.spine])
        rng  = np.random.default_rng(self.seed)

        if self.laid_flat and self.height_mm is None:
            raise ValueError('laid_flat needs an explicit height_mm '
                             '(the paved strip depth in mm)')
        seat_z = self._seat_z(scene, segs)
        if self.height_mm is None:
            # Top-anchored (default): the finished top lands at top_mm
            # above the datum whatever the seat surface is.
            self.height_mm = max(self.top_mm - seat_z, 6.0)
        T, H = self.thickness_mm, self.height_mm
        cells  = self._cells(segs, T, H, rng)
        cells, posed = self._apply_openings(cells, segs, rng)
        if self.ruin > 0.0:
            # Surround cells live in `posed` and are exempt: a ruined
            # wall keeps its surviving arch.
            cells = self._ruin_cells(cells, segs, H, rng)

        parts = self._core_boxes(segs, seat_z)
        parts = self._cut_openings_from_core(parts, segs, seat_z)
        for pc in posed:
            parts.append(self._place_posed(pc, segs, seat_z))
        for cell in cells:
            block = self._place_block(cell, segs, seat_z, rng)
            if block is not None:    # degenerate cell (fieldstone inset)
                parts.append(block)
        parts.extend(self._extra_parts(segs, seat_z, rng))

        leaves = self._leaf_parts(segs, seat_z, surface)

        wall = assemble_masonry(parts, surface, 'CutStoneWall')
        if self.laid_flat:
            # Ground the pavement: stones/core overshoot below the
            # datum for solid seating; one clip flattens the bottom.
            box = trimesh.creation.box(
                extents=[surface.tile_w, surface.tile_h, T + 20.0],
                transform=trimesh.transformations.translation_matrix(
                    [surface.tile_w / 2.0, surface.tile_h / 2.0,
                     (T + 20.0) / 2.0]))
            wall = clip_to_box(wall, box, 'floor bottom')
            # The boolean returns a fresh mesh with metadata stripped —
            # untagged parts default to SOIL in the material grouping.
            _tag(wall, Material.ROCK)

        self._stamp(scene, segs, seat_z + (T if self.laid_flat else H))
        return [wall] + leaves

    # ── pieces ───────────────────────────────────────────────────────────────
    def _cells(self, segs: list[_Seg], T: float, H: float,
               rng: np.random.Generator) -> list[_Cell]:
        """Block cells for the wall; subclass hook — FieldstoneWall
        post-processes these (throughstone merges)."""
        cells = _layout(segs, T, H, self.course_mm, self.bay_mm,
                        self.min_bond_mm, rng)
        if self.crenellated:
            cells = self._crenellate(cells, segs, T, H, rng)
        return cells

    def _crenellate(self, cells: list[_Cell], segs: list[_Seg], T: float,
                    H: float, rng: np.random.Generator) -> list[_Cell]:
        """Merlon/crenel parapet (city-wall-crenellated.jpg): the crenel
        floor lands on the course boundary nearest ``H − crenel_depth``;
        parapet cells above it survive only inside merlon intervals,
        re-cut to the merlon edges (flanks become textured 'face' ends).
        Merlons are forced at segment ends so every corner is a merlon;
        interior widths are scaled to fit each segment exactly."""
        z_edges = sorted({round(c.z1, 6) for c in cells})
        below = [z for z in z_edges if z < H - 1e-6]
        if not below:
            return cells
        zf = min(below, key=lambda z: abs(z - (H - self.crenel_depth_mm)))
        self._crenel_z = zf

        wm_avg = float(np.mean(self.merlon_mm))
        wc_avg = float(np.mean(self.crenel_mm))
        self._merlons = []
        for seg in segs:
            n_c = max(1, int(round((seg.L - wm_avg) / (wm_avg + wc_avg))))
            wm = rng.uniform(*self.merlon_mm, n_c + 1)
            wc = rng.uniform(*self.crenel_mm, n_c)
            scale = seg.L / (wm.sum() + wc.sum())
            wm *= scale
            wc *= scale
            merlons, t = [], 0.0
            for i in range(n_c + 1):
                merlons.append((t, t + wm[i]))
                t += wm[i] + (wc[i] if i < n_c else 0.0)
            self._merlons.append(merlons)

        out: list[_Cell] = []
        for c in cells:
            if c.z0 < zf - 1e-6:
                out.append(c)
                continue
            # Parapet course: keep only the merlon-interval overlaps.
            for m, (m0, m1) in enumerate(self._merlons[c.seg]):
                lo, hi = max(c.t0, m0), min(c.t1, m1)
                if hi - lo < 1.5:
                    continue
                cell = _Cell(
                    seg=c.seg, t0=lo, t1=hi,
                    end0='face' if lo > c.t0 + 1e-6 else c.end0,
                    end1='face' if hi < c.t1 - 1e-6 else c.end1,
                    z0=c.z0, z1=c.z1,
                    is_top=c.is_top, is_bottom=False,
                    is_quoin=c.is_quoin,
                    key=c.key + (m,),
                )
                out.append(cell)
        return out

    def _unit_mesh(self, lx: float, ly: float, lz: float,
                   cell: _Cell, rng: np.random.Generator) -> trimesh.Trimesh:
        """One masonry unit in the local cell frame; subclass hook —
        FieldstoneWall swaps this for lumpy rounded stones."""
        return _block_mesh(lx, ly, lz, self.chip_mm,
                           self.roundover_mm, self.relief_mm,
                           self.relief_wl, cell.is_top, rng,
                           cut_planes=getattr(cell, 'cut_planes_local',
                                              None))

    # ── openings: doors, windows, oculi, hatches ────────────────────────────
    def _cut_openings_from_core(self, parts, segs, seat_z):
        """Subtract each opening's passage prism (dilated by the
        reveal) from the mortar core boxes: the core edge stays hidden
        behind the surround ring (ring depth > reveal), and jamb
        reveals are real masonry, never core plane."""
        if not self._op_profiles or not parts:
            return parts
        import shapely.geometry as sgeom
        T = self.thickness_mm
        cutters = []
        for seg_i, P, op, _tc in self._op_profiles:
            seg = segs[seg_i]
            poly = sgeom.Polygon(P).buffer(self.reveal_mm, quad_segs=6)
            prism = trimesh.creation.extrude_polygon(poly, height=T + 6.0)
            mm = np.eye(4)
            if self.laid_flat:
                mm[:3, 0] = [seg.d[0], seg.d[1], 0.0]
                mm[:3, 1] = [seg.n[0], seg.n[1], 0.0]
                mm[:3, 2] = [0.0, 0.0, -1.0]
                mm[:2, 3] = seg.a
                mm[2, 3]  = T + 3.0
            else:
                mm[:3, 0] = [seg.d[0], seg.d[1], 0.0]
                mm[:3, 1] = [0.0, 0.0, 1.0]
                mm[:3, 2] = [seg.n[0], seg.n[1], 0.0]
                mm[:2, 3] = seg.a + seg.n * (-3.0)
                mm[2, 3]  = seat_z
            prism.apply_transform(mm)
            cutters.append(prism)
        out = []
        for b in parts:
            try:
                cut = trimesh.boolean.difference([b] + cutters,
                                                 engine='manifold')
                out.append(cut if len(cut.faces) else b)
            except Exception:                       # noqa: BLE001
                out.append(b)
        return out


    def _apply_openings(self, cells: list[_Cell], segs: list[_Seg],
                        rng: np.random.Generator):
        """Exclude wall cells inside each opening profile and build the
        posed SURROUND cells (docs/design/walls-doors.md): jamb stacks
        on near-vertical boundary, radial voussoirs elsewhere, lintel /
        sill slabs for 'auto' profiles.  Surrounds may rise above the
        wall top (low walls imply tall walls).  Returns
        (trimmed_cells, posed_cells)."""
        self._op_profiles = []
        if not self.openings:
            return cells, []
        arcs = np.concatenate([[0.0], np.cumsum([s_.L for s_ in segs])])
        posed: list[_Cell] = []
        for oi, op in enumerate(self.openings):
            a_mm = op.at * self._sq
            seg_i = int(np.searchsorted(arcs[1:], a_mm + 1e-6))
            seg_i = min(seg_i, len(segs) - 1)
            tc = a_mm - arcs[seg_i]
            P = build_profile(op, tc)
            self._op_profiles.append((seg_i, P, op, tc))
            # 1. surround FIRST — the bond is trimmed against the
            #    actual surround units, not the profile.
            op_cells = self._surround_cells(op, P, seg_i, tc, rng, oi)
            posed += op_cells
            # Each unit's rotated rectangle in the wall's (t, z) plane
            # (matching _place_posed's rotation conventions).
            quads = []
            for u in op_cells:
                t, z, ang = u.pose
                w, d = u.pdims
                ca, sa = np.cos(ang), np.sin(ang)
                quads.append(np.array(
                    [[t + dx * ca + dz * sa, z - dx * sa + dz * ca]
                     for dx, dz in ((-w / 2, -d / 2), (w / 2, -d / 2),
                                    (w / 2, d / 2), (-w / 2, d / 2))]))
            # The forbidden region a fitted block must clear: the
            # units dilated by (joint − press) — thin mortar line on
            # mortared families, pressed interpenetration on drystone
            # — plus the passage dilated by the reveal.
            import shapely.geometry as sgeom
            from shapely.ops import unary_union
            delta = self.joint_mm - self.surround_bond_press
            region = unary_union(
                [sgeom.Polygon(q).buffer(delta, join_style=2)
                 for q in quads]
                + [sgeom.Polygon(P).buffer(self.reveal_mm,
                                           quad_segs=6)])
            # 2. exclusion: per course band, the bond runs to the
            #    surround units actually IN that band (+ the normal
            #    'press'/joint treatment at the cut).  The bond tooths
            #    into the quoin-alternating jamb courses and follows
            #    the arch extrados — the gap to the surround is an
            #    ordinary joint, never an exposed mortar wedge.
            import dataclasses as _dc

            def _rep(c, **kw):
                nc = _dc.replace(c, **kw)
                for a in ('cut0', 'cut1', 'cut_z0', 'cut_z1',
                          'side_bands', 'coping'):
                    if hasattr(c, a):
                        setattr(nc, a, getattr(c, a))
                return nc

            def _cut_line_h(c, side):
                """Horizontal counterpart of _cut_line: the support
                line UNDER a surround hanging into the band from
                above (side 'T' → block keeps full width, cut from
                the top: the stones beneath a ring/arch bottom) or
                OVER one rising from below ('B': the bond flowing
                over a keystone).  Returns (z@t0, z@t1) or None."""
                ts = np.linspace(c.t0 + 0.05, c.t1 - 0.05, 9)
                pts = []
                for t in ts:
                    col = region.intersection(sgeom.LineString(
                        [(t, c.z0 - 0.5), (t, c.z1 + 0.5)]))
                    if col.is_empty:
                        continue
                    cb = col.bounds
                    pts.append((t, cb[1] if side == 'T' else cb[3]))
                if not pts:
                    return None
                ta = np.array([p[0] for p in pts])
                za = np.array([p[1] for p in pts])
                if len(pts) == 1:
                    m, k = 0.0, float(za[0])
                else:
                    m, k = np.polyfit(ta, za, 1)
                res = za - (m * ta + k)
                k += float(res.min() if side == 'T' else res.max())
                return (float(m * c.t0 + k), float(m * c.t1 + k))

            def _columns(c):
                """Classify how the surround region blocks the cell:
                per t-column, does it span the full band height, hang
                from the top, or rise from the bottom?"""
                ts = np.linspace(max(c.t0, lo) + 0.05,
                                 min(c.t1, hi) - 0.05, 7)
                kinds = set()
                for t in ts:
                    col = region.intersection(sgeom.LineString(
                        [(t, c.z0 - 0.5), (t, c.z1 + 0.5)]))
                    if col.is_empty:
                        continue
                    cb = col.bounds
                    at_top = cb[3] >= c.z1 - 0.05
                    at_bot = cb[1] <= c.z0 + 0.05
                    if at_top and at_bot:
                        kinds.add('full')
                    elif at_top:
                        kinds.add('top')
                    elif at_bot:
                        kinds.add('bottom')
                    else:
                        kinds.add('full')   # island: no single cut fits
                return kinds

            def _cut_line(z0, z1, side):
                """A single linear angled cut clearing the surround
                within the band (Shawn: one straight cut per side, not
                a literal curve trace): sample the region's extent per
                z, least-squares a line, shift it until every sample
                clears.  Returns (t@z0, t@z1) or None."""
                zs = np.linspace(z0 + 0.02, z1 - 0.02, 9)
                b0, _zl, b1, _zh = region.bounds
                pts = []
                for z in zs:
                    row = region.intersection(
                        sgeom.LineString([(b0 - 1.0, z), (b1 + 1.0, z)]))
                    if row.is_empty:
                        continue
                    rb = row.bounds
                    pts.append((z, rb[0] if side == 'L' else rb[2]))
                if not pts:
                    return None
                za = np.array([p[0] for p in pts])
                ta = np.array([p[1] for p in pts])
                if len(pts) == 1:
                    m, k = 0.0, float(ta[0])
                else:
                    m, k = np.polyfit(za, ta, 1)
                res = ta - (m * za + k)
                k += float(res.min() if side == 'L' else res.max())
                return (float(m * z0 + k), float(m * z1 + k))

            out = []
            absorb = []   # (z0, edge_t, side, target, line): a remnant
            #             too narrow to keep is ABSORBED by its course
            #             neighbour, which extends across the vanished
            #             head joint to the surround — a mason's cut
            #             unit, never a column of exposed core (the
            #             E15 flat mortar band beside the brick door).
            for c in cells:
                if c.seg != seg_i:
                    out.append(c)
                    continue
                exts = [e for q in quads
                        if (e := band_extent(q, c.z0, c.z1)) is not None]
                if not exts:
                    out.append(c)
                    continue
                lo = min(e[0] for e in exts)
                hi = max(e[1] for e in exts)
                if c.t1 <= lo + 1e-6 or c.t0 >= hi - 1e-6:
                    out.append(c)
                    continue
                kinds = _columns(c)
                if not kinds:
                    out.append(c)
                    continue
                lrem = lo - c.t0
                rrem = c.t1 - hi
                if kinds == {'top'} or kinds == {'bottom'}:
                    # The surround only dips into the band from one
                    # horizontal side: candidate — the block keeps its
                    # FULL width with a single angled cut on its top
                    # (under a ring/arch bottom) or bottom (over a
                    # keystone).  Taken only when it KEEPS MORE STONE
                    # than the side cut (a jamb grazing one end of the
                    # cell classifies 'bottom' too, but there a side
                    # cut saves the whole brick — the E15 missing
                    # bottom brick).
                    side = 'T' if kinds == {'top'} else 'B'
                    line = _cut_line_h(c, side)
                    if line is not None:
                        if side == 'T':
                            pen = c.z1 - min(line)
                            keep = max(line) - c.z0
                        else:
                            pen = max(line) - c.z0
                            keep = c.z1 - min(line)
                        if pen < 0.35:      # graze: fuses invisibly
                            out.append(c)
                            continue
                        bh = c.z1 - c.z0
                        area_h = ((c.t1 - c.t0) * keep
                                  if keep >= _MIN_KEEP_MM else 0.0)
                        area_s = (lrem if lrem >= _MIN_KEEP_MM
                                  else 0.0) * bh \
                            + (rrem if rrem >= _MIN_KEEP_MM
                               else 0.0) * bh
                        if area_h >= area_s:
                            if area_h <= 0.0:
                                continue    # nothing substantial left
                            nc = _rep(c, key=c.key + (705, oi))
                            if side == 'T':
                                nc.cut_z1 = line
                            else:
                                nc.cut_z0 = line
                            out.append(nc)
                            continue
                        # else: fall through to the side cut
                if lrem > 1e-6:
                    line = _cut_line(c.z0, c.z1, 'L')
                    t1 = (max(line) + _CUT_MARGIN_MM) if line else lo
                    if lrem >= _MIN_KEEP_MM:
                        nc = _rep(c, t1=t1, end1='press',
                                  key=c.key + (701, oi))
                        nc.cut1 = line
                        out.append(nc)
                    else:
                        absorb.append((c.z0, c.t0, 'end1', t1, line))
                if rrem > 1e-6:
                    line = _cut_line(c.z0, c.z1, 'R')
                    t0 = (min(line) - _CUT_MARGIN_MM) if line else hi
                    if rrem >= _MIN_KEEP_MM:
                        nc = _rep(c, t0=t0, end0='press',
                                  key=c.key + (702, oi))
                        nc.cut0 = line
                        out.append(nc)
                    else:
                        absorb.append((c.z0, c.t1, 'end0', t0, line))
            cells = out
            for z0, edge, side, target, line in absorb:
                for i, c in enumerate(cells):
                    if c.seg != seg_i or abs(c.z0 - z0) > 1e-6:
                        continue
                    if side == 'end1' and abs(c.t1 - edge) < 1e-6:
                        nc = _rep(c, t1=target, end1='press',
                                  key=c.key + (703, oi))
                        nc.cut1 = line
                        cells[i] = nc
                        break
                    if side == 'end0' and abs(c.t0 - edge) < 1e-6:
                        nc = _rep(c, t0=target, end0='press',
                                  key=c.key + (704, oi))
                        nc.cut0 = line
                        cells[i] = nc
                        break
        return cells, posed

    def _posed_cell(self, seg_i, oi, tag, t, z, w, d, ang,
                    split=None, taper=0.0) -> list[_Cell]:
        """Posed surround cell(s); with ``split`` (slot system) the
        unit becomes front+back pair leaving the leaf channel between."""
        g = getattr(self, '_slot_gap', 0.0) if split else 0.0
        T = self.thickness_mm
        p = self.surround_proud_mm
        qs = [(-p, T + p)] if not split else \
             [(-p, (T - g) / 2.0), ((T + g) / 2.0, T + p)]
        outs = []
        for qi, (q0, q1) in enumerate(qs):
            c = _Cell(seg=seg_i, t0=t - w / 2.0, t1=t + w / 2.0,
                      end0='face', end1='face', z0=z - d / 2.0,
                      z1=z + d / 2.0, is_top=False, is_bottom=False,
                      key=(801, oi, tag, qi))
            c.pose = (t, z, ang)
            c.pdims = (w, d)
            c.qspan = (q0, q1)
            c.taper = taper
            outs.append(c)
        return outs

    def _surround_cells(self, op, P, seg_i, tc, rng, oi) -> list[_Cell]:
        vw, ring = self.surround_vw, self.surround_ring
        jd, jh = self.surround_jd, self.surround_jh
        frac = self.surround_frac
        split = op.slot
        out: list[_Cell] = []
        w2 = op.width_mm / 2.0
        if op.profile == 'auto':
            rise = 0.0 if op.head == 'lintel' else min(
                op.rise_mm if op.rise_mm is not None else w2,
                w2, op.head_mm - op.sill_mm)
            z_sp = op.head_mm - rise
            # jamb stacks (quoin-style: alternating depth)
            n_j = max(1, int(round((z_sp - op.sill_mm) / np.mean(jh))))
            hs = rng.uniform(*jh, n_j)
            hs *= (z_sp - op.sill_mm) / hs.sum()
            z = op.sill_mm
            for k, h in enumerate(hs):
                d_k = jd * (1.0 if k % 2 == 0 else 0.78)
                # A door's bottom jamb blocks root below the seat like
                # the wall's own bottom course (standing walls only —
                # laid flat, "down" would be a plan direction).
                emb = (2.0 if k == 0 and op.sill_mm < 0.5
                       and not self.laid_flat else 0.0)
                for sgn, tag in ((-1, 10), (+1, 20)):
                    t = tc + sgn * (w2 + d_k / 2.0)
                    out += self._posed_cell(
                        seg_i, oi, tag * 100 + k, t,
                        z - emb + (h + emb) / 2.0, d_k,
                        (h + emb) * min(frac, 1.03), 0.0, split)
                z += h
            if op.head == 'lintel':
                lh = 4.5
                out += self._posed_cell(
                    seg_i, oi, 30, tc, op.head_mm + lh / 2.0,
                    op.width_mm + 2.0 * jd + 1.0, lh, 0.0, split)
            else:
                # voussoirs along the arc ONLY (open polyline: the end
                # units land exactly on the jamb tops at the
                # springings); odd count = one true keystone.
                units = boundary_units(arch_arc(op, tc), vw,
                                       closed=False, offset=ring / 2.0,
                                       force_odd=True)
                mid = (len(units) - 1) // 2
                for k, (p, n, ang, step, dth) in enumerate(units):
                    scale = _KEYSTONE if k == mid else 1.0
                    # scaled units grow OUTWARD only: the keystone
                    # soffit stays ON the arc — nothing hangs below
                    # the arch head (FDM printability).
                    pos = p + n * ((scale - 1.0) * ring / 2.0)
                    out += self._posed_cell(
                        seg_i, oi, 40 + k, pos[0], pos[1],
                        step * frac * scale, ring * scale,
                        ang, split,
                        taper=min(dth * ring * scale, 0.5 * step))
            if op.sill_mm > 0.5:
                out += self._posed_cell(
                    seg_i, oi, 50, tc, op.sill_mm - _SILL_H_MM / 2.0,
                    op.width_mm + 2.0 * jd + 2.0 * _SILL_OVER_MM,
                    _SILL_H_MM, 0.0, None)
        else:
            # circle / custom polygon: generic boundary lining — every
            # unit a voussoir rotated to the local normal (a circle has
            # no verticals: full ring — oculus / well).
            units = boundary_units(P, vw, closed=True, offset=ring / 2.0)
            apex = max(u[0][1] for u in units)
            for k, (p, n, ang, step, dth) in enumerate(units):
                scale = _KEYSTONE if abs(p[1] - apex) < step * 0.6 else 1.0
                pos = p + n * ((scale - 1.0) * ring / 2.0)   # outward only
                out += self._posed_cell(
                    seg_i, oi, 60 + k, pos[0], pos[1],
                    step * frac * scale, ring * scale, ang, split,
                    taper=min(dth * ring * scale, 0.5 * step))
        return out

    def _place_posed(self, cell: _Cell, segs: list[_Seg],
                     seat_z: float) -> trimesh.Trimesh:
        """Place one surround unit: an ordinary block kernel rotated to
        the cell's pose angle — voussoirs are just rotated blocks."""
        seg = segs[cell.seg]
        brng = np.random.default_rng(
            (self.seed * 1_000_003 + hash(cell.key)) & 0x7FFFFFFF)
        w, d = cell.pdims
        t, z, ang = cell.pose
        q0, q1 = cell.qspan
        ro = self.surround_ro
        chip = self.surround_chip
        rel = self.relief_mm if isinstance(self.relief_mm, (int, float)) \
            and self.relief_mm is not None else 0.10
        # The kernel's LSE blend pulls every face in by ~tau*ln2 — the
        # roundover.  On wall-scale blocks that reads as the joint; on
        # 3–5 mm surround units it is a 25–30 % loss per dimension
        # (fieldstone jambs rendered as floating pebbles).  Oversize
        # the in-plane dims by the analytic shrink so the faces land
        # on their nominal boxes and the units nearly touch.
        g = 0.0
        for _ in range(3):      # tau depends on the grown dims; converges
            dims = (w + g, q1 - q0, d + g)
            pitch = float(np.clip(max(dims) / 56.0, 0.18, 0.32))
            tau = max(min(0.7 * ro, 0.22 * min(dims)), 0.6 * pitch)
            g = 2.0 * tau * np.log(2.0)
        w += g
        d += g
        taper = getattr(cell, 'taper', 0.0)
        if self.laid_flat:
            body = _block_mesh(w, d, q1 - q0, chip, ro, rel,
                               self.relief_wl, False, brng,
                               taper=('y', taper))
            body.apply_translation([-w / 2.0, -d / 2.0, -(q1 - q0) / 2.0])
            body.apply_transform(trimesh.transformations.rotation_matrix(
                -ang, [0, 0, 1]))
            mm = np.eye(4)
            mm[:3, 0] = [seg.d[0], seg.d[1], 0.0]
            mm[:3, 1] = [seg.n[0], seg.n[1], 0.0]
            mm[:3, 2] = [0.0, 0.0, 1.0]
            mm[:2, 3] = seg.a + seg.d * t + seg.n * z
            mm[2, 3]  = self.thickness_mm - (q0 + q1) / 2.0
            body.apply_transform(mm)
            return body
        body = _block_mesh(w, q1 - q0, d, chip, ro, rel,
                           self.relief_wl, False, brng,
                           taper=('z', taper))
        body.apply_translation([-w / 2.0, -(q1 - q0) / 2.0, -d / 2.0])
        body.apply_transform(trimesh.transformations.rotation_matrix(
            ang, [0, 1, 0]))
        mm = np.eye(4)
        mm[:2, 0] = seg.d
        mm[:2, 1] = seg.n
        mm[:2, 3] = seg.a + seg.d * t + seg.n * ((q0 + q1) / 2.0)
        mm[2, 3]  = seat_z + z
        body.apply_transform(mm)
        return body

    def _leaf_parts(self, segs: list[_Seg], seat_z: float,
                    surface) -> list[trimesh.Trimesh]:
        """Integrated leaves (design stage O5): each opening with a
        ``leaf`` gets a separate WOOD-tagged solid (ROCK for bars)
        fitted to its PROFILE — an arched doorway gets an arch-top
        door, a circle a round grille/lid.  The leaf embeds into the
        jamb reveals and surround all round, so the export union
        fuses it to the masonry; it is NOT part of the wall's own
        union (it keeps its material group)."""
        from .leaf import build_leaf
        out = []
        for li, (seg_i, P, op, tc) in enumerate(self._op_profiles):
            if op.leaf is None:
                continue
            leaf = op.leaf
            lrng = np.random.default_rng(
                (self.seed * 9_369_319 + 71 * li + leaf.seed)
                & 0x7FFFFFFF)
            P = np.asarray(P, dtype=float)
            xmin = float(P[:, 0].min())
            zmin = float(P[:, 1].min())
            body = build_leaf(leaf, P - [xmin, zmin], lrng)
            # leaf plane mid-thickness on a wall; just under the
            # walking surface on a floor (flush trapdoor lid).
            q0 = 0.35 if self.laid_flat else \
                (self.thickness_mm - leaf.thickness_mm) / 2.0
            seg = segs[seg_i]
            body.apply_translation([xmin, q0, zmin])
            body.apply_transform(self._lay(seg) if self.laid_flat
                                 else _frame(seg, z=seat_z))
            body = self._clip_to_tile(body, surface)
            _tag(body, leaf.material)
            out.append(body)
        return out

    def _ruin_cells(self, cells: list[_Cell], segs: list[_Seg], H: float,
                    rng: np.random.Generator) -> list[_Cell]:
        """Break the top along a smooth per-segment height envelope
        (hadrians-coursed-rubble.jpg): blocks above it go, blocks
        straddling it survive at random — the ragged stepped ruin
        line.  Nothing reads as a dressed cap any more."""
        self._ruin_env = []
        for seg in segs:
            wl = rng.uniform(*_RUIN_WOBBLE_WL, 2)
            ph = rng.uniform(0.0, 2.0 * np.pi, 2)

            def env(t, wl=wl, ph=ph):
                n01 = 0.5 + sum(np.sin(2.0 * np.pi * t / w + p)
                                for w, p in zip(wl, ph)) / 4.0
                lo, hi = _RUIN_DROP
                return H * (1.0 - self.ruin * (lo + (hi - lo) * n01))
            self._ruin_env.append(env)

        out: list[_Cell] = []
        for c in cells:
            e = float(self._ruin_env[c.seg]((c.t0 + c.t1) / 2.0))
            if c.z0 >= e:
                continue
            if c.z1 > e and rng.random() > _RUIN_KEEP_P:
                continue
            c.is_top = False
            out.append(c)
        return out

    def _extra_parts(self, segs: list[_Seg], seat_z: float,
                     rng: np.random.Generator) -> list[trimesh.Trimesh]:
        """Rubble hearting (always for drystone families, on ruin for
        mortared ones) + shed rubble at the foot of a ruin."""
        parts = []
        if self.hearting or self.ruin > 0.0:
            parts += self._hearting_parts(segs, seat_z, rng)
        if self.ruin > 0.0 and not self.laid_flat:
            parts += self._foot_rubble(segs, seat_z, rng)
        return parts

    def _heart_cap(self, seg_i: int, t: float) -> float:
        """Wall-local z the hearting may fill to at (seg, t); families
        tighten this (fieldstone keeps it under the coping/top course)."""
        if self._ruin_env is not None:
            return float(self._ruin_env[seg_i](t))
        return self.height_mm

    def _hearting_parts(self, segs: list[_Seg], seat_z: float,
                        rng: np.random.Generator) -> list[trimesh.Trimesh]:
        """A sealed sheet of small rough stone chips through the wall
        body (E10/E27): every crack or break shows packed rubble, never
        a bare core plane.  Two y-layers, half-pitch staggered in t and
        z; setback keeps it behind the visible faces AND the segment
        END planes (free ends and the corner arris)."""
        T, H = self.thickness_mm, self.height_mm
        sb = _RUBBLE_SETBACK_MM
        y_bands = [(sb, 0.55 * T), (0.45 * T, T - sb)]
        parts = []
        for k, seg in enumerate(segs):
            nt = max(2, int(round(seg.L / _RUBBLE_SPACING_MM)) + 1)
            nz = max(2, int(round(H / _RUBBLE_SPACING_MM)) + 1)
            for layer, (yb0, yb1) in enumerate(y_bands):
                off = 0.5 * layer
                for i in range(nt):
                    for j in range(nz):
                        tc = ((i + 0.5 * (j % 2) + off
                               + rng.uniform(-0.25, 0.25))
                              * seg.L / (nt - 1))
                        zc = ((j + off + rng.uniform(-0.25, 0.25))
                              * H / (nz - 1))
                        w = rng.uniform(*_RUBBLE_FOOT)
                        h = rng.uniform(*_RUBBLE_H)
                        t0 = np.clip(tc - w / 2.0, sb, seg.L - sb - w)
                        cap = self._heart_cap(k, float(tc))
                        z0 = np.clip(zc - h / 2.0, 0.2, cap - 0.3 - h)
                        if z0 < 0.2 - 1e-9:
                            continue
                        # Keep chips out of the passage: test the box
                        # as PLACED (the z clamp can drag a chip whose
                        # sampled centre was below the sill up into the
                        # opening).
                        if any(si == k and any(
                                point_inside(P, tp, zp)
                                for tp in (t0, t0 + w / 2.0, t0 + w)
                                for zp in (z0, z0 + h / 2.0, z0 + h))
                               for si, P, *_ in self._op_profiles):
                            continue
                        body = rubble_stone(w, yb1 - yb0, h, rng)
                        b0, b1 = body.bounds
                        tgt0 = np.array([t0, yb0, z0])
                        tgt1 = np.array([t0 + w, yb1, z0 + h])
                        body.apply_translation(-b0)
                        body.apply_scale((tgt1 - tgt0) / (b1 - b0))
                        body.apply_translation(tgt0)
                        body.apply_transform(
                            self._lay(seg) if self.laid_flat
                            else _frame(seg, z=seat_z))
                        parts.append(body)
        return parts

    def _foot_rubble(self, segs: list[_Seg], seat_z: float,
                     rng: np.random.Generator) -> list[trimesh.Trimesh]:
        """Shards shed at the base of a ruined wall, both sides,
        embedded ~40 % into the ground.  Anything outside the tile is
        trimmed by the boundary clip like every other wall part."""
        T = self.thickness_mm
        parts = []
        for seg in segs:
            n = int(self.ruin * seg.L / _FOOT_RUBBLE_EVERY_MM)
            for _ in range(n):
                t = rng.uniform(2.0, seg.L - 2.0)
                off = rng.uniform(*_FOOT_RUBBLE_OFF)
                q = -off if rng.random() < 0.5 else T + off
                w = rng.uniform(*_FOOT_RUBBLE_FOOT)
                h = rng.uniform(*_FOOT_RUBBLE_H)
                body = rubble_stone(w, w * rng.uniform(0.7, 1.0), h, rng)
                b0, _b1 = body.bounds
                body.apply_translation(-b0 + np.array(
                    [t - w / 2.0, q - w / 2.0, -0.4 * h]))
                body.apply_transform(_frame(seg, z=seat_z))
                parts.append(body)
        return parts

    def _core_boxes(self, segs: list[_Seg], seat_z: float,
                    ) -> list[trimesh.Trimesh]:
        """Recessed core: full footprint inset by reveal from every visible
        face (both wall faces, free ends, corner outer planes, and the
        top); the bottom runs to the embed depth (R3)."""
        rv, T = self.reveal_mm, self.thickness_mm
        if self.laid_flat:
            # Mortar sheet under the pavement: recessed rv below the
            # top (same joint-floor depth as standing walls), inset rv
            # from the strip's plan edges, overshooting the datum for
            # the bottom clip.
            boxes = []
            for seg in segs:
                ex = np.array([seg.L - 2.0 * rv, (T + 0.5) - rv,
                               self.height_mm - 2.0 * rv])
                b = rounded_box(ex, _CORE_ROUND_MM)
                b.apply_translation(ex / 2.0 + np.array([rv, rv, rv]))
                b.apply_transform(self._lay(seg))
                boxes.append(b)
            return boxes
        embed = getattr(self, '_embed_eff', self.embed_mm)
        z0, z1 = seat_z - embed, seat_z + self.height_mm - rv
        boxes = []

        def _box(seg: _Seg, t0: float, t1: float, q0: float, q1: float,
                 z_lo: float | None = None,
                 z_hi: float | None = None) -> trimesh.Trimesh:
            za = z0 if z_lo is None else z_lo
            zb = z1 if z_hi is None else z_hi
            ex = np.array([t1 - t0, q1 - q0, zb - za])
            # Rounded core edges: wherever a joint looks onto the
            # mortar plane, a sharp box edge reads as a machined
            # insert, not set mortar.
            b = rounded_box(ex, _CORE_ROUND_MM)
            b.apply_translation(ex / 2.0)
            b.apply_transform(_frame(seg, t0, q0, za))
            return b

        # A start/end inset of ``rv`` is right at BOTH free ends and
        # joints: at a joint, the neighbouring segment's outer face plane
        # coincides with this segment's t=0 / t=L plane, so the inset
        # recesses the core from that face too; the two segment cores
        # overlap inside the corner cell, keeping the union connected.
        if self._ruin_env is not None:
            # Ruined wall: the mortar core stays below the LOWEST point
            # of the break envelope; the exposed band above shows the
            # rubble hearting, never a mortar plane.
            for k, seg in enumerate(segs):
                env = self._ruin_env[k]
                emin = min(float(env(t))
                           for t in np.linspace(0.0, seg.L, 25))
                boxes.append(_box(seg, rv, seg.L - rv, rv, T - rv,
                                  z_hi=seat_z + emin - rv))
            return boxes
        if self.crenellated and self._crenel_z is not None:
            # Base core stops below the crenel floor (which must read
            # as block tops, not mortar); each merlon gets a mini-core
            # inset from its flanks, overlapping down into the base
            # core so the union stays connected.
            zc = seat_z + self._crenel_z
            for seg, merlons in zip(segs, self._merlons):
                boxes.append(_box(seg, rv, seg.L - rv, rv, T - rv,
                                  z_hi=zc - rv))
                for m0, m1 in merlons:
                    boxes.append(_box(seg, max(m0 + rv, rv),
                                      min(m1 - rv, seg.L - rv),
                                      rv, T - rv, z_lo=zc - 2.0))
            return boxes
        for seg in segs:
            boxes.append(_box(seg, rv, seg.L - rv, rv, T - rv))
        return boxes

    def _stone_box(self, cell: _Cell, seat_z: float,
                   rng: np.random.Generator) -> tuple:
        """Unit box (x0,x1,y0,y1,z0,z1) inside the jointed cell; subclass
        hook — FieldstoneWall jitters per-stone face recession here."""
        j2 = self.joint_mm / 2.0
        fr = self.face_recess_mm
        # 'press' ends (opening trim) sit like joints: the cut line
        # already includes any surround_bond_press, so the unit runs
        # tight to it — never the free-end recess draw.
        x0 = cell.t0 + (rng.uniform(0.0, fr) if cell.end0 == 'face'
                        else j2)
        x1 = cell.t1 - (rng.uniform(0.0, fr) if cell.end1 == 'face'
                        else j2)
        y0 = rng.uniform(0.0, fr)
        y1 = self.thickness_mm - rng.uniform(0.0, fr)
        if self.laid_flat:
            # q = 0 is the pavement TOP (proud jitter stays); the
            # inner face overshoots below the datum and is clipped
            # flat — every slab is grounded, full depth.
            y1 = self.thickness_mm + 1.0
        z0 = seat_z + (cell.z0 + j2 if not cell.is_bottom
                       else -getattr(self, '_embed_eff', self.embed_mm))
        z1 = seat_z + (cell.z1 - (0.0 if cell.is_top else j2))
        return x0, x1, y0, y1, z0, z1

    def _place_block(self, cell: _Cell, segs: list[_Seg], seat_z: float,
                     rng: np.random.Generator) -> trimesh.Trimesh:
        seg = segs[cell.seg]
        x0, x1, y0, y1, z0, z1 = self._stone_box(cell, seat_z, rng)

        brng = np.random.default_rng(
            (self.seed * 1_000_003 + hash(cell.key)) & 0x7FFFFFFF)
        if self.laid_flat:
            # Swapped dims so the unit kernel's local z (its "up",
            # where the top-ring / relief conventions live) is the
            # pavement depth q.
            lx, ly, lz = x1 - x0, z1 - z0, y1 - y0
        else:
            lx, ly, lz = x1 - x0, y1 - y0, z1 - z0
        # Opening-fit cut lines → kernel planes in the block's local
        # frame (the along-height axis is local z standing, local y
        # laid flat).  One straight angled plane per cut side —
        # vertical-ish side cuts against jambs/rings, horizontal-ish
        # cuts under a ring bottom (cut_z1) or over a keystone
        # (cut_z0).
        cuts = []
        span = ly if self.laid_flat else lz
        zw0 = z0 - seat_z
        for attr in ('cut0', 'cut1', 'cut_z0', 'cut_z1'):
            line = getattr(cell, attr, None)
            if line is None or span <= 1e-6:
                continue
            if attr in ('cut0', 'cut1'):
                tA, tB = line          # t at cell.z0 / cell.z1
                m = (tB - tA) / max(cell.z1 - cell.z0, 1e-9)
                xA = tA + m * (zw0 - cell.z0) - x0
                xB = xA + m * span
                p, q = np.array([xA, 0.0]), np.array([xB, span])
                want = (0, +1.0 if attr == 'cut1' else -1.0)
            else:
                zA, zB = line          # z at cell.t0 / cell.t1
                m = (zB - zA) / max(cell.t1 - cell.t0, 1e-9)
                hA = zA + m * (x0 - cell.t0) - zw0
                hB = hA + m * lx
                p, q = np.array([0.0, hA]), np.array([lx, hB])
                want = (1, +1.0 if attr == 'cut_z1' else -1.0)
            d = q - p
            n2 = np.array([-d[1], d[0]])
            if n2[want[0]] * want[1] < 0:
                n2 = -n2
            n2 /= np.linalg.norm(n2)
            n3 = (np.array([n2[0], n2[1], 0.0]) if self.laid_flat
                  else np.array([n2[0], 0.0, n2[1]]))
            cuts.append((n3, float(n2 @ p)))
        cell.cut_planes_local = cuts or None
        body = self._unit_mesh(lx, ly, lz, cell, brng)

        ctr = np.array([lx / 2.0, ly / 2.0, lz / 2.0])
        yaw = np.radians(brng.uniform(-self.yaw_max_deg, self.yaw_max_deg))
        body.apply_transform(trimesh.transformations.rotation_matrix(
            yaw, [0.0, 0.0, 1.0], ctr))
        for axis in ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]):
            tilt = np.radians(brng.uniform(-self.tilt_max_deg,
                                           self.tilt_max_deg))
            body.apply_transform(trimesh.transformations.rotation_matrix(
                tilt, axis, ctr))

        if self.laid_flat:
            # local x → run, local y → second plan axis (wall z),
            # local z → up; block top (local lz, the q0 face) lands at
            # thickness − y0.
            mm = np.eye(4)
            mm[:3, 0] = [seg.d[0], seg.d[1], 0.0]
            mm[:3, 1] = [seg.n[0], seg.n[1], 0.0]
            mm[:3, 2] = [0.0, 0.0, 1.0]
            mm[:2, 3] = seg.a + seg.d * x0 + seg.n * z0
            mm[2, 3]  = self.thickness_mm - y1
            body.apply_transform(mm)
        else:
            body.apply_transform(_frame(seg, x0, y0, z0))
        return body

    # ── terrain ──────────────────────────────────────────────────────────────
    def _footprint(self, scene, segs: list[_Seg]) -> tuple:
        """Bool grid mask of the wall plan (strips + corner cells)."""
        surface = scene.surface
        cw, gw, gh = surface.cell_w, surface.grid_w, surface.grid_h
        # Laid flat, the strip's plan width is the wall HEIGHT.
        T = self.height_mm if self.laid_flat else self.thickness_mm
        pts = np.vstack([[seg.a, seg.a + seg.d * seg.L,
                          seg.a + seg.n * T,
                          seg.a + seg.d * seg.L + seg.n * T]
                         for seg in segs])
        i0 = max(0, int(pts[:, 0].min() / cw) - 1)
        i1 = min(gw, int(pts[:, 0].max() / cw) + 2)
        j0 = max(0, int(pts[:, 1].min() / cw) - 1)
        j1 = min(gh, int(pts[:, 1].max() / cw) + 2)
        X, Y = np.meshgrid(np.arange(i0, i1) * cw, np.arange(j0, j1) * cw)
        inside = np.zeros(X.shape, dtype=bool)
        for seg in segs:
            t = (X - seg.a[0]) * seg.d[0] + (Y - seg.a[1]) * seg.d[1]
            q = (X - seg.a[0]) * seg.n[0] + (Y - seg.a[1]) * seg.n[1]
            inside |= (t >= 0.0) & (t <= seg.L) & (q >= 0.0) & (q <= T)
        return inside, (i0, i1, j0, j1)

    def _seat_z(self, scene, segs: list[_Seg]) -> float:
        """Seat on whatever surface is UNDER the wall — pavement
        included (Shawn: walls sit ON the floor, not buried beneath
        it).  When the footprint is mostly paved (support well above
        the soil), the wall stands on the slab tops with a token
        0.25 mm fusion embed; on bare soil it keeps its normal burial
        (fine per Shawn)."""
        inside, (i0, i1, j0, j1) = self._footprint(scene, segs)
        ter = scene.terrain_z[j0:j1, i0:i1]
        if self.laid_flat:
            # The pavement IS the terrain (StoneFloor rule): drop the
            # soil under the strip to a thin film — NOT zero, the
            # heightmap solid degenerates (base_h = 0) — and pave from
            # the datum.
            ter[inside] = np.minimum(ter[inside], 0.15)
            self._embed_eff = 0.0
            return 0.0
        sup = scene.terrain_support_z[j0:j1, i0:i1]
        surf = np.where(np.isfinite(sup), np.maximum(ter, sup), ter)
        if not inside.any():
            return float(surf.max())
        paved = float(np.percentile((surf - ter)[inside], 60.0)) > 0.5
        self._embed_eff = 0.25 if paved else self.embed_mm
        return float(np.percentile(surf[inside], _SEAT_PERCENTILE))

    def _stamp(self, scene, segs: list[_Seg], cap_z: float) -> None:
        inside, (i0, i1, j0, j1) = self._footprint(scene, segs)
        sl = scene.terrain_support_z[j0:j1, i0:i1]
        np.maximum(sl, np.where(inside, cap_z, -np.inf), out=sl)
        scene.obstacle_mask[j0:j1, i0:i1] |= inside

    @staticmethod
    def _clip_to_tile(wall: trimesh.Trimesh, surface) -> trimesh.Trimesh:
        """Butt-join (R8): plane-cut flush at the tile boundary."""
        w, h = surface.tile_w, surface.tile_h
        lo = wall.bounds[0]
        hi = wall.bounds[1]
        if (lo[0] >= -1e-6 and lo[1] >= -1e-6
                and hi[0] <= w + 1e-6 and hi[1] <= h + 1e-6):
            return wall
        zpad = (hi[2] - lo[2]) + 20.0
        box = trimesh.creation.box(
            extents=[w, h, zpad],
            transform=trimesh.transformations.translation_matrix(
                [w / 2.0, h / 2.0, lo[2] - 10.0 + zpad / 2.0]))
        return clip_to_box(wall, box, 'wall tile boundary')
