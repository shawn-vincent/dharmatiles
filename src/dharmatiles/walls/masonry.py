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
    end0:  str             # 'joint' | 'face'  (start side along d)
    end1:  str
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
                pull_mask: tuple = (1.0, 1.0, 1.0, 1.0)) -> trimesh.Trimesh:
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
    pts = []
    def _ring(z: float, inset: float, pull: float):
        x0 = inset + rng.uniform(0.0, pull) * pull_mask[0]
        x1 = lx - inset - rng.uniform(0.0, pull) * pull_mask[1]
        y0 = inset + rng.uniform(0.0, pull) * pull_mask[2]
        y1 = ly - inset - rng.uniform(0.0, pull) * pull_mask[3]
        pts.extend([[x0, y0, z], [x1, y0, z],
                    [x1, y1, z], [x0, y1, z]])

    z_top_pull = _TOP_SETTLE_MM if is_top else 0.5 * chip_mm
    _ring(lz - rng.uniform(0.0, z_top_pull), 0.0, chip_mm)      # top ring
    # Bottom ring identical to the top (Shawn: bricks have the SAME
    # edge everywhere — the old bottom chamfer read as round-bottomed
    # bricks).  The R10 overhang it guarded against is the block lip
    # over the sub-mm joint recess; the official pieces have square
    # bottoms there and print fine.
    _ring(0.0, 0.0, chip_mm)                                    # bottom ring

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
    pitch = float(np.clip(max(lx, ly, lz) / 56.0, 0.18, 0.32))
    # Smooth-max (log-sum-exp) of the plane fields: on a face one
    # plane dominates and the level-0 set IS the exact plane (zero
    # ripple); where planes meet, the LSE blend pulls the surface in
    # by ~tau*ln2 — the roundover, analytically.  tau is floored at
    # the grid pitch so the blend is resolved (a hard max creases the
    # field and MC slivers the crease into non-manifold edges).
    tau = max(0.7 * roundover_mm, 0.6 * pitch)
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
                 ruin:         float = 0.0):
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
        self._merlons: list[list[tuple[float, float]]] = []   # per segment
        self._crenel_z = None      # wall-local z of the crenel floor
        self._ruin_env = None      # per-segment envelope callables

    #: families with a drystone interior build the hearting always;
    #: mortared families get it when ruined (broken top shows rubble).
    hearting: bool = False

    # ── build ────────────────────────────────────────────────────────────────
    def apply(self, scene, *, placement_mask=None) -> list[trimesh.Trimesh]:
        surface = scene.surface
        sq = surface.square_mm
        segs = _segments([(x * sq, y * sq) for x, y in self.spine])
        rng  = np.random.default_rng(self.seed)

        seat_z = self._seat_z(scene, segs)
        if self.height_mm is None:
            # Top-anchored (default): the finished top lands at top_mm
            # above the datum whatever the seat surface is.
            self.height_mm = max(self.top_mm - seat_z, 6.0)
        T, H = self.thickness_mm, self.height_mm
        cells  = self._cells(segs, T, H, rng)
        if self.ruin > 0.0:
            cells = self._ruin_cells(cells, segs, H, rng)

        parts = self._core_boxes(segs, seat_z)
        for cell in cells:
            block = self._place_block(cell, segs, seat_z, rng)
            if block is not None:    # degenerate cell (fieldstone inset)
                parts.append(block)
        parts.extend(self._extra_parts(segs, seat_z, rng))

        wall = assemble_masonry(parts, surface, 'CutStoneWall')

        self._stamp(scene, segs, seat_z + H)
        return [wall]

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
                           self.relief_wl, cell.is_top, rng)

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
        if self.ruin > 0.0:
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
                        body = rubble_stone(w, yb1 - yb0, h, rng)
                        b0, b1 = body.bounds
                        tgt0 = np.array([t0, yb0, z0])
                        tgt1 = np.array([t0 + w, yb1, z0 + h])
                        body.apply_translation(-b0)
                        body.apply_scale((tgt1 - tgt0) / (b1 - b0))
                        body.apply_translation(tgt0)
                        body.apply_transform(_frame(seg, z=seat_z))
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
        x0 = cell.t0 + (j2 if cell.end0 == 'joint'
                        else rng.uniform(0.0, fr))
        x1 = cell.t1 - (j2 if cell.end1 == 'joint'
                        else rng.uniform(0.0, fr))
        y0 = rng.uniform(0.0, fr)
        y1 = self.thickness_mm - rng.uniform(0.0, fr)
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
        body = self._unit_mesh(x1 - x0, y1 - y0, z1 - z0, cell, brng)

        ctr = np.array([(x1 - x0) / 2.0, (y1 - y0) / 2.0, (z1 - z0) / 2.0])
        yaw = np.radians(brng.uniform(-self.yaw_max_deg, self.yaw_max_deg))
        body.apply_transform(trimesh.transformations.rotation_matrix(
            yaw, [0.0, 0.0, 1.0], ctr))
        for axis in ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]):
            tilt = np.radians(brng.uniform(-self.tilt_max_deg,
                                           self.tilt_max_deg))
            body.apply_transform(trimesh.transformations.rotation_matrix(
                tilt, axis, ctr))

        body.apply_transform(_frame(seg, x0, y0, z0))
        return body

    # ── terrain ──────────────────────────────────────────────────────────────
    def _footprint(self, scene, segs: list[_Seg]) -> tuple:
        """Bool grid mask of the wall plan (strips + corner cells)."""
        surface = scene.surface
        cw, gw, gh = surface.cell_w, surface.grid_w, surface.grid_h
        T = self.thickness_mm
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
