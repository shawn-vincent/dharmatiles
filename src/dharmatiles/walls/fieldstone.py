"""
Fieldstone / drystone wall — walls campaign family 3.

Same chassis as :class:`CutStoneWall` (spine → segments, courses/bays
with bond offset, quoin alternation, recessed core, seat/stamp/clip);
the UNIT and the cell field change.  Reference read
(`docs/reference/walls/README.md` drystone photo, Hirst m70): **flat
slabby stones laid on their beds** — width ≫ height on the face — in
rough courses, packed to nearly touching with thin dark shadow joints
(no mortar), big size variety with occasional throughstones spanning
two courses, and the corner built from the same stones, just squarer.

Shawn round-3 verdict on the first cut ("terrible"): stones were too
equant and egg-like, the corner kept cut-stone BRICKS, joints gaped,
sizes were uniform.  This rework:

- stones are generated as slabs (vertical support directions squashed)
  so every stone reads laid-flat, like the m70 strips;
- corners use the same fieldstone unit at high blockiness — no
  `_block_mesh` bricks anywhere in this wall;
- packing: stones overshoot their cells and are shaved back by the
  rounding (`_PACK_COMP_MM`), joints thin (`joint_mm` 0.5);
- size irregularity: bay range widened AND a `_cells` post-pass merges
  ~1/7 of cells with the cell above into a double-height throughstone,
  plus per-stone height jitter down to ~55 % of the course (small
  pinning stones between big ones).
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..scatter.stones import _blur_remesh, _relief_field, _round_edges
from .masonry import CutStoneWall, _Cell, _Seg, _RELIEF_WAVES

# ── Iteration knobs (module constants while prototyping) ─────────────────────
_BLOCKINESS       = (0.42, 0.78)  # ellipsoid↔box blend for body stones —
                                  # wide range = the reference's mix of
                                  # rounded and squarish stones; with real
                                  # gaps + hearting behind them (E10) the
                                  # rounder end no longer opens mortar
                                  # channels, so the E9 floor is relaxed
_QUOIN_BLOCKINESS = (0.65, 0.88)  # corner stones: same unit, squarer
_CAP_BLOCKINESS   = (0.55, 0.80)  # coping stones flat-topped and squarer
_LUMP             = (0.90, 1.0)   # per-point radius jitter
_DIR_JITTER       = 0.12          # support-direction jitter
_FLATTEN          = (0.50, 0.80)  # vertical squash of support directions:
                                  # stones are SLABS laid on their beds —
                                  # the round-3 "flatter stones" call.
                                  # Applied in direction space, so the hull
                                  # fills the (short) cell height while the
                                  # silhouette stays wide
_PACK_COMP_MM     = 2.1           # build oversize so rounding keeps real
                                  # curvature; exact-fit rescales to target
_FLATTEN_Y        = (0.35, 0.60)  # face squash — broad flattish face patch
                                  # (mortar diagnostic: tangent bellies covered
                                  # ~half the face; red sea between stones)
_FLATTEN_X_END    = (0.55, 0.80)  # end squash for wall-end stones only
# Stones NEVER overlap (E10, Shawn + the drystone reference photo):
# a carefully stacked pile reads through the dark outline gap around
# EVERY stone; unioned overlaps erase those outlines and the wall
# fuses into one lumpy mass.  Gaps are real (joint_mm), and the rubble
# hearting behind them is what makes a gap read as stone-filled depth
# instead of mortar.
_THROUGH_FRAC     = 0.20          # fraction of eligible cells merged with
                                  # the cell above into a throughstone
_REMESH_SIGMA     = 0.9           # aged read (stones convention)
_RUBBLE_SPACING_MM = 4.2          # hearting grid pitch over each face
_RUBBLE_FOOT      = (8.5, 11.0)   # ≥2× pitch: hull corners recede ~25 %
                                  # inside their boxes, so box-touching is
                                  # NOT enough — coverage is guaranteed
                                  # only when each stone's box overlaps
                                  # its neighbour by more than the
                                  # recession (measured 3.8 % box-level
                                  # holes at 1.3–2× pitch, E10)
_RUBBLE_H         = (5.5, 7.5)    # ≥1.3× pitch vertically, same rule
_RUBBLE_SETBACK_MM = 1.6          # rubble faces sit this far behind the
                                  # wall face planes: deep enough that the
                                  # face stones stay the wall (at 0.15 the
                                  # big-footprint rubble reached the face
                                  # plane and BURIED the coursing — E10),
                                  # shallow enough to still front the core
                                  # (reveal 2.2) through every gap


def _fieldstone_mesh(lx: float, ly: float, lz: float, blockiness: float,
                     flat_top: bool, roundover_mm: float, relief_mm: float,
                     relief_wl: tuple[float, float],
                     rng: np.random.Generator,
                     flatten_y: float = 1.0,
                     flatten_x: float = 1.0,
                     aged: bool = True) -> trimesh.Trimesh:
    """One flat-bedded stone filling the local box [0,lx]×[0,ly]×[0,lz]."""
    M = int(rng.integers(13, 18))
    i = np.arange(M) + 0.5
    phi   = np.arccos(1.0 - 2.0 * i / M)
    theta = np.pi * (1.0 + np.sqrt(5.0)) * i
    d = np.stack([np.sin(phi) * np.cos(theta),
                  np.sin(phi) * np.sin(theta), np.cos(phi)], axis=1)
    d += rng.normal(0.0, _DIR_JITTER, d.shape)
    # Squashes in direction space flatten the hull toward that axis's
    # planes.  z: flat-laid bed/top (the slab read).  y: broad flattish
    # FACE patches — the mortar-diagnostic fix: a round belly touches
    # the face plane at one point, so straight-on the wall read as
    # stones floating in mortar; a builder turns the flattest side out.
    # x: only for wall-end stones (their end is a visible face).
    d[:, 0] *= flatten_x
    d[:, 1] *= flatten_y
    d[:, 2] *= rng.uniform(*_FLATTEN)
    d /= np.linalg.norm(d, axis=1, keepdims=True) + 1e-12

    half = np.array([lx / 2.0, ly / 2.0, lz / 2.0])
    r_ell = 1.0 / np.sqrt(((d / half) ** 2).sum(axis=1))
    r_box = 1.0 / (np.abs(d) / half).max(axis=1)
    r = ((1.0 - blockiness) * r_ell + blockiness * r_box)
    r *= rng.uniform(*_LUMP, M)
    p = d * r[:, None]
    if flat_top:
        p[:, 2] = np.minimum(p[:, 2], 0.90 * half[2])
    p += half

    hull = trimesh.convex.convex_hull(p)
    v, f = np.asarray(hull.vertices), np.asarray(hull.faces)
    if roundover_mm > 0.0:
        v, f = _round_edges(v, f, roundover_mm, rng)
    body = trimesh.Trimesh(vertices=v, faces=f, process=False)
    if not aged:
        # Cheap path for rubble hearting: hull + roundover only.  The
        # rubble reads through joints, a millimetre behind the faces —
        # remesh + relief cost would be paid ~100× for background stones.
        return body
    remeshed = _blur_remesh(body, max(lx, ly, lz), _REMESH_SIGMA)
    if remeshed is not None:
        body = remeshed
        if relief_mm > 0.0:
            disp = relief_mm * _relief_field(
                body.vertices, rng, _RELIEF_WAVES, *relief_wl)
            body = trimesh.Trimesh(
                vertices=(body.vertices
                          + np.asarray(body.vertex_normals) * disp[:, None]),
                faces=body.faces, process=False)
    return body


class FieldstoneWall(CutStoneWall):
    """Direct TileLayer: a drystone fieldstone wall on a plan spine.

    Same spine convention and contracts as :class:`CutStoneWall`.
    """

    yaw_max_deg:    float = 2.5   # stones sit less true than cut blocks
    tilt_max_deg:   float = 1.2
    face_recess_mm: float = 0.45

    def __init__(self, spine, *,
                 course_mm: tuple[float, float] = (3.2, 8.8),
                 bay_mm:    tuple[float, float] = (4.5, 16.0),
                 joint_mm:  float = 0.9,
                 reveal_mm: float = 2.8,
                 roundover_mm: float | None = 0.45,
                 relief_mm:    float | None = 0.12,
                 relief_wl:    tuple[float, float] | None = (2.0, 7.0),
                 min_bond_mm:  float = 1.8,
                 **kwargs):
        super().__init__(spine, course_mm=course_mm, bay_mm=bay_mm,
                         joint_mm=joint_mm, reveal_mm=reveal_mm,
                         roundover_mm=roundover_mm, relief_mm=relief_mm,
                         relief_wl=relief_wl, min_bond_mm=min_bond_mm,
                         **kwargs)

    # ── size irregularity: throughstone merges ───────────────────────────────
    def _cells(self, segs: list[_Seg], T: float, H: float,
               rng: np.random.Generator) -> list[_Cell]:
        cells = super()._cells(segs, T, H, rng)
        # Merge some cells with a cell in the course directly above into
        # a double-height throughstone (drystone structure + the wide
        # size read).  Candidates must overlap in t and neither cell may
        # be already merged, a quoin, or the top course.
        by_course: dict[tuple[int, int], list[_Cell]] = {}
        for c in cells:
            by_course.setdefault((c.key[0], c.seg), []).append(c)
        taken: set[tuple] = set()
        merged: list[_Cell] = []
        drop:   set[tuple] = set()
        for c in cells:
            if (c.is_top or c.is_quoin or c.key in taken
                    or rng.random() > _THROUGH_FRAC):
                continue
            above = by_course.get((c.key[0] + 1, c.seg), [])
            for a in above:
                if (a.is_quoin or a.key in taken
                        or min(c.t1, a.t1) - max(c.t0, a.t0) < 3.0):
                    continue
                lo = max(c.t0, a.t0)
                hi = min(c.t1, a.t1)
                # The throughstone takes the overlap column; donor cells
                # shrink away from it so neighbours stay in place.
                merged.append(_Cell(
                    seg=c.seg, t0=lo, t1=hi,
                    end0='joint', end1='joint',
                    z0=c.z0, z1=a.z1,
                    is_top=a.is_top, is_bottom=c.is_bottom,
                    key=(c.key[0], c.seg, c.key[2], 'through'),
                ))
                taken.add(c.key)
                taken.add(a.key)
                for donor in (c, a):
                    if donor.t0 < lo - 1.0 or donor.t1 > hi + 1.0:
                        # keep the wider remainder as a shrunken cell
                        if donor.t0 < lo - 1.0:
                            donor.t1 = lo
                            donor.end1 = 'joint'
                        else:
                            donor.t0 = hi
                            donor.end0 = 'joint'
                    else:
                        drop.add(donor.key)
                break
        out = [c for c in cells if c.key not in drop]
        out.extend(merged)
        return out

    # ── rubble hearting ──────────────────────────────────────────────────────
    def _extra_parts(self, segs: list[_Seg], seat_z: float,
                     rng: np.random.Generator) -> list:
        """A sealed sheet of small rubble stones through the wall body.

        By-construction guarantee for the red-mortar diagnostic: pins
        chase voids case-by-case, but void shapes are unpredictable —
        the hearting covers every (t, z) with overlapping rubble, so
        any gap between face stones shows deeper stones, exactly like
        a real drystone wall's packed core.
        """
        T, H = self.thickness_mm, self.height_mm
        sb = _RUBBLE_SETBACK_MM
        # TWO y-layers, half-pitch staggered in t and z (E10): with one
        # layer, a face gap aligned with a rubble-rubble gap leaks a
        # sight line onto the core; the offset back layer backs every
        # front-layer gap.  Layers meet at mid-thickness (hidden).
        y_bands = [(sb, 0.55 * T), (0.45 * T, T - sb)]
        parts = []
        for seg in segs:
            nt = max(2, int(round(seg.L / _RUBBLE_SPACING_MM)) + 1)
            nz = max(2, int(round(H / _RUBBLE_SPACING_MM)) + 1)
            for layer, (y0, y1) in enumerate(y_bands):
                off = 0.5 * layer
                for i in range(nt):
                    for j in range(nz):
                        # Half-pitch stagger per row (brick bond) AND
                        # per layer.  Row centres span 0..H edge to edge
                        # (clips pull boundary stones inside).
                        tc = ((i + 0.5 * (j % 2) + off
                               + rng.uniform(-0.25, 0.25))
                              * seg.L / (nt - 1))
                        zc = ((j + off + rng.uniform(-0.25, 0.25))
                              * H / (nz - 1))
                        w = rng.uniform(*_RUBBLE_FOOT)
                        h = rng.uniform(*_RUBBLE_H)
                        t0 = np.clip(tc - w / 2.0, 0.3, seg.L - 0.3 - w)
                        z0 = np.clip(zc - h / 2.0, 0.2, H - 0.6 - h)
                        body = _fieldstone_mesh(
                            w, y1 - y0, h,
                            rng.uniform(0.25, 0.55), False,
                            0.35, 0.0, self.relief_wl, rng,
                            flatten_y=rng.uniform(0.5, 0.8), aged=False)
                        b0, b1 = body.bounds
                        tgt0 = np.array([t0, y0, z0])
                        tgt1 = np.array([t0 + w, y1, z0 + h])
                        body.apply_translation(-b0)
                        body.apply_scale((tgt1 - tgt0) / (b1 - b0))
                        body.apply_translation(tgt0)
                        m = np.eye(4)
                        m[:2, 0] = seg.d
                        m[:2, 1] = seg.n
                        m[:2, 3] = seg.a
                        m[2, 3] = seat_z
                        body.apply_transform(m)
                        parts.append(body)
        return parts

    # ── the unit ─────────────────────────────────────────────────────────────
    def _unit_mesh(self, lx: float, ly: float, lz: float, chamfer: float,
                   cell: _Cell, rng: np.random.Generator) -> trimesh.Trimesh:
        # Same fieldstone unit EVERYWHERE — corners just get squarer
        # stones (round-3: "you've left bricks in the corner").
        if cell.is_quoin:
            blockiness = rng.uniform(*_QUOIN_BLOCKINESS)
        elif cell.is_top:
            blockiness = rng.uniform(*_CAP_BLOCKINESS)
        else:
            blockiness = rng.uniform(*_BLOCKINESS)

        # Build OVERSIZE so rounding keeps real curvature, then EXACT-FIT
        # rescale to the target box.  This replaces shave compensation
        # (round-4 fix): guessing the roundover+blur shave either leaves
        # stones flush with the core (flat-slab read) or overshoots and
        # bulges them past the outer face plane, where the tile-boundary
        # clip PLANES them into flat-faced rocks (Shawn's outside find).
        # Exact fit makes every belly tangent to its face plane — round
        # faces by construction, nothing for the clip to shave.
        fy = rng.uniform(*_FLATTEN_Y)
        fx = (rng.uniform(*_FLATTEN_X_END)
              if 'face' in (cell.end0, cell.end1) else 1.0)
        c = _PACK_COMP_MM
        body = _fieldstone_mesh(lx + c, ly + c, lz + c,
                                blockiness, cell.is_top or cell.is_quoin,
                                self.roundover_mm, self.relief_mm,
                                self.relief_wl, rng,
                                flatten_y=fy, flatten_x=fx)
        # Target box IS the jointed cell (E10): stones never overlap —
        # the outline gap around every stone is the stacked read.
        b0, b1 = body.bounds
        target0 = np.array([0.0, 0.0, 0.0])
        target1 = np.array([lx, ly, lz])
        body.apply_translation(-b0)
        body.apply_scale((target1 - target0) / (b1 - b0))
        body.apply_translation(target0)
        return body
