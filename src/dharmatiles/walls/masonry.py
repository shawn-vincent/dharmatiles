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
from ..scatter.stones import _blur_remesh, _relief_field, _round_edges

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
_CHAMFER_FRAC     = 1.0    # bottom chamfer depth = reveal × this (R10: the
                           # block's proud lip never overhangs the joint below)
_REMESH_SIGMA     = 0.7    # blur-remesh sigma: below the stones' 0.9 "fresh"
                           # setting so chip facets and chamfers stay crisp
_RELIEF_WL_MM     = (1.5, 6.0)   # broadband relief wavelengths (log-uniform)
_RELIEF_WAVES     = 24
_MIN_BAY_FRAC     = 0.45   # bays shorter than this × bay_min are merged away
_SEAT_PERCENTILE  = 80.0   # seat height over the footprint (stones convention)
_FLUSH_OVERSHOOT_MM = 0.8  # tile-boundary ends overshoot by this and are
                           # plane-cut flush at the seam (butt-join, R8)
_BOUNDARY_EPS_MM  = 0.05   # wall end counts as on-boundary within this


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
    key:   tuple = field(default=())


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


def _end_on_tile_boundary(p: np.ndarray, out_dir: np.ndarray,
                          surface) -> bool:
    """True when the wall end plane at ``p`` (outward normal ``out_dir``)
    coincides with a tile boundary plane — the seam of an adjacent tile."""
    for axis, extent in ((0, surface.tile_w), (1, surface.tile_h)):
        if abs(abs(out_dir[axis]) - 1.0) > 1e-3:
            continue
        if (abs(p[axis]) < _BOUNDARY_EPS_MM
                or abs(p[axis] - extent) < _BOUNDARY_EPS_MM):
            return True
    return False


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
            min_bond_mm: float, rng: np.random.Generator,
            flush_start: bool = False, flush_end: bool = False,
            ) -> list[_Cell]:
    """Courses × bays × quoin alternation → block cells (R5, R7).

    ``flush_start`` / ``flush_end``: the wall end lies on a tile
    boundary — blocks there overshoot and are plane-cut flush (R8)
    instead of getting a textured end face.
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
            prev_cuts[k] = cuts
            edges = np.concatenate([[t0], cuts, [t1]])
            for b, (ta, tb) in enumerate(zip(edges[:-1], edges[1:])):
                if ta > t0 or (k > 0 and not owns_start):
                    end0 = 'joint'
                else:
                    end0 = 'flush' if (k == 0 and flush_start) else 'face'
                if tb < t1 or (k < n_joints and not owns_end):
                    end1 = 'joint'
                else:
                    end1 = ('flush' if (k == n_joints and flush_end)
                            else 'face')
                cells.append(_Cell(
                    seg=k, t0=float(ta), t1=float(tb),
                    end0=end0, end1=end1,
                    z0=float(z0), z1=float(z1),
                    is_top=is_top, is_bottom=is_bottom,
                    key=(c, k, b),
                ))
    return cells


# ── Block body ────────────────────────────────────────────────────────────────

def _block_mesh(lx: float, ly: float, lz: float, chamfer: float,
                chip_mm: float, roundover_mm: float, relief_mm: float,
                is_top: bool, rng: np.random.Generator) -> trimesh.Trimesh:
    """One block in local frame [0,lx]×[0,ly]×[0,lz] (x along run, y = depth).

    Jittered-box hull: corners pulled inward (chipped arrises) around
    full-extent face centres, a chamfer ring above the base kills the
    proud-lip overhang (R10), then small roundover + broadband
    micro-relief for the drybrush catch (R4/R9).
    """
    ch = min(chamfer, 0.45 * lz)
    xy = np.array([[0.0, 0.0], [lx, 0.0], [lx, ly], [0.0, ly]])
    ctr = np.array([lx / 2.0, ly / 2.0])

    pts = []
    def _ring(z: float, inset: float, pull: float):
        for p in xy:
            q = p + np.sign(ctr - p) * inset
            q = q + np.sign(ctr - q) * rng.uniform(0.0, pull, 2)
            pts.append([q[0], q[1], z])

    z_top_pull = _TOP_SETTLE_MM if is_top else 0.5 * chip_mm
    _ring(lz - rng.uniform(0.0, z_top_pull), 0.0, chip_mm)      # top corners
    _ring(rng.uniform(0.6, 1.0) * ch, 0.0, chip_mm)             # chamfer ring
    if ch > 0.0:
        _ring(0.0, ch, 0.4 * chip_mm)                           # inset base
    # Full-extent face centres keep each face plane out at the cell wall:
    # chipped corners then read as facets INSIDE the face, not shrinkage.
    pts.extend([[lx / 2.0, ly / 2.0, lz],
                [lx / 2.0, 0.0, lz / 2.0], [lx / 2.0, ly, lz / 2.0],
                [0.0, ly / 2.0, lz / 2.0], [lx, ly / 2.0, lz / 2.0]])

    hull = trimesh.convex.convex_hull(np.asarray(pts))
    v, f = np.asarray(hull.vertices), np.asarray(hull.faces)
    if roundover_mm > 0.0:
        v, f = _round_edges(v, f, roundover_mm, rng)
    body = trimesh.Trimesh(vertices=v, faces=f, process=False)
    # Uniform watertight remesh before displacement — subdivide_to_size
    # leaves T-junctions (non-conforming), which breaks the manifold
    # union.  _blur_remesh is the pipeline's stable-mesh primitive.
    remeshed = _blur_remesh(body, max(lx, ly, lz), _REMESH_SIGMA)
    if remeshed is not None:
        body = remeshed
        if relief_mm > 0.0:
            disp = relief_mm * _relief_field(
                body.vertices, rng, _RELIEF_WAVES, *_RELIEF_WL_MM)
            body = trimesh.Trimesh(
                vertices=(body.vertices
                          + np.asarray(body.vertex_normals) * disp[:, None]),
                faces=body.faces, process=False)
    return body


# ── Layer ─────────────────────────────────────────────────────────────────────

class CutStoneWall:
    """Direct TileLayer: a coursed cut-stone wall on a plan spine.

    ``spine``: polyline in SQUARE units — the OUTER face line in plan;
    the body extends to the LEFT of the walk direction.  Walk a tile-edge
    spine with the tile interior on your left for the DB flush slab.

    Stamps ``terrain_support_z`` and ``obstacle_mask`` over the footprint
    (same contracts as Rocks/FacetedStones) so grass steers around.
    """

    height_default_mm: float = 5.0

    def __init__(self, spine: list[tuple[float, float]], *,
                 thickness_mm: float = 7.0,
                 height_mm:    float = 49.7,
                 seed:         int   = 0,
                 course_mm:    tuple[float, float] = (5.5, 8.5),
                 bay_mm:       tuple[float, float] = (11.0, 20.0),
                 joint_mm:     float = 1.2,
                 reveal_mm:    float = 1.3,
                 chip_mm:      float = 0.55,
                 roundover_mm: float = 0.22,
                 relief_mm:    float = 0.07,
                 min_bond_mm:  float = 3.0,
                 embed_mm:     float = 2.5):
        if len(spine) < 2:
            raise ValueError('wall spine needs at least two points')
        self.spine        = [tuple(map(float, p)) for p in spine]
        self.thickness_mm = thickness_mm
        self.height_mm    = height_mm
        self.seed         = seed
        self.course_mm    = course_mm
        self.bay_mm       = bay_mm
        self.joint_mm     = joint_mm
        self.reveal_mm    = reveal_mm
        self.chip_mm      = chip_mm
        self.roundover_mm = roundover_mm
        self.relief_mm    = relief_mm
        self.min_bond_mm  = min_bond_mm
        self.embed_mm     = embed_mm

    # ── build ────────────────────────────────────────────────────────────────
    def apply(self, scene, *, placement_mask=None) -> list[trimesh.Trimesh]:
        surface = scene.surface
        sq = surface.square_mm
        segs = _segments([(x * sq, y * sq) for x, y in self.spine])
        rng  = np.random.default_rng(self.seed)
        T, H = self.thickness_mm, self.height_mm

        seat_z = self._seat_z(scene, segs)
        f0 = _end_on_tile_boundary(segs[0].a, -segs[0].d, surface)
        f1 = _end_on_tile_boundary(segs[-1].a + segs[-1].d * segs[-1].L,
                                   segs[-1].d, surface)
        cells  = _layout(segs, T, H, self.course_mm, self.bay_mm,
                         self.min_bond_mm, rng,
                         flush_start=f0, flush_end=f1)

        parts = self._core_boxes(segs, seat_z, f0, f1)
        for cell in cells:
            parts.append(self._place_block(cell, segs, seat_z, rng))

        wall = trimesh.boolean.union(parts, engine='manifold')
        wall = self._clip_to_tile(wall, surface)
        if not wall.is_watertight:
            warnings.warn('CutStoneWall union is not watertight',
                          RuntimeWarning)

        self._stamp(scene, segs, seat_z + H)
        _tag(wall, Material.ROCK)
        return [wall]

    # ── pieces ───────────────────────────────────────────────────────────────
    def _core_boxes(self, segs: list[_Seg], seat_z: float,
                    flush_start: bool, flush_end: bool,
                    ) -> list[trimesh.Trimesh]:
        """Recessed core: full footprint inset by reveal from every visible
        face (both wall faces, free ends, corner outer planes, and the
        top); the bottom runs to the embed depth (R3)."""
        rv, T = self.reveal_mm, self.thickness_mm
        z0, z1 = seat_z - self.embed_mm, seat_z + self.height_mm - rv
        boxes = []

        def _box(seg: _Seg, t0: float, t1: float,
                 q0: float, q1: float) -> trimesh.Trimesh:
            ex = np.array([t1 - t0, q1 - q0, z1 - z0])
            b = trimesh.creation.box(extents=ex)
            b.apply_translation(ex / 2.0)
            m = np.eye(4)
            m[:2, 0] = seg.d
            m[:2, 1] = seg.n
            m[:2, 3] = seg.a + seg.d * t0 + seg.n * q0
            m[2, 3]  = z0
            b.apply_transform(m)
            return b

        # A start/end inset of ``rv`` is right at BOTH free ends and
        # joints: at a joint, the neighbouring segment's outer face plane
        # coincides with this segment's t=0 / t=L plane, so the inset
        # recesses the core from that face too; the two segment cores
        # overlap inside the corner cell, keeping the union connected.
        # At a tile-boundary end the core overshoots instead — the seam
        # cut leaves a full flat cross-section, no recessed ring (R8).
        last = len(segs) - 1
        for k, seg in enumerate(segs):
            c0 = -_FLUSH_OVERSHOOT_MM if (k == 0 and flush_start) else rv
            c1 = (seg.L + _FLUSH_OVERSHOOT_MM
                  if (k == last and flush_end) else seg.L - rv)
            boxes.append(_box(seg, c0, c1, rv, T - rv))
        return boxes

    def _place_block(self, cell: _Cell, segs: list[_Seg], seat_z: float,
                     rng: np.random.Generator) -> trimesh.Trimesh:
        seg = segs[cell.seg]
        j2  = self.joint_mm / 2.0

        def _side(t: float, end: str, sgn: float) -> float:
            if end == 'joint':
                return t + sgn * j2
            if end == 'flush':
                return t - sgn * _FLUSH_OVERSHOOT_MM
            return t + sgn * rng.uniform(0.0, _FACE_RECESS_MM)

        x0 = _side(cell.t0, cell.end0, +1.0)
        x1 = _side(cell.t1, cell.end1, -1.0)
        y0 = rng.uniform(0.0, _FACE_RECESS_MM)
        y1 = self.thickness_mm - rng.uniform(0.0, _FACE_RECESS_MM)
        z0 = seat_z + (cell.z0 + j2 if not cell.is_bottom
                       else -self.embed_mm)
        z1 = seat_z + (cell.z1 - (0.0 if cell.is_top else j2))

        chamfer = 0.0 if cell.is_bottom else _CHAMFER_FRAC * self.reveal_mm
        brng = np.random.default_rng(
            (self.seed * 1_000_003 + hash(cell.key)) & 0x7FFFFFFF)
        body = _block_mesh(x1 - x0, y1 - y0, z1 - z0, chamfer,
                           self.chip_mm, self.roundover_mm, self.relief_mm,
                           cell.is_top, brng)

        ctr = np.array([(x1 - x0) / 2.0, (y1 - y0) / 2.0, (z1 - z0) / 2.0])
        yaw = np.radians(brng.uniform(-_YAW_MAX_DEG, _YAW_MAX_DEG))
        body.apply_transform(trimesh.transformations.rotation_matrix(
            yaw, [0.0, 0.0, 1.0], ctr))
        for axis in ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]):
            tilt = np.radians(brng.uniform(-_TILT_MAX_DEG, _TILT_MAX_DEG))
            body.apply_transform(trimesh.transformations.rotation_matrix(
                tilt, axis, ctr))

        m = np.eye(4)
        m[:2, 0] = seg.d
        m[:2, 1] = seg.n
        m[:2, 3] = seg.a + seg.d * x0 + seg.n * y0
        m[2, 3]  = z0
        body.apply_transform(m)
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
        inside, (i0, i1, j0, j1) = self._footprint(scene, segs)
        patch = scene.terrain_z[j0:j1, i0:i1]
        if not inside.any():
            return float(patch.max())
        return float(np.percentile(patch[inside], _SEAT_PERCENTILE))

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
        clipped = trimesh.boolean.intersection([wall, box], engine='manifold')
        if len(clipped.faces) > 0 and clipped.is_watertight:
            return clipped
        warnings.warn('wall tile-boundary clip failed; left unclipped',
                      RuntimeWarning)
        return wall
