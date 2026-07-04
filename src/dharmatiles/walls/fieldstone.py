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
_BLOCKINESS       = (0.30, 0.62)  # ellipsoid↔box blend for body stones
_QUOIN_BLOCKINESS = (0.62, 0.85)  # corner stones: same unit, squarer
_CAP_BLOCKINESS   = (0.50, 0.75)  # coping stones flat-topped and squarer
_LUMP             = (0.90, 1.0)   # per-point radius jitter
_DIR_JITTER       = 0.12          # support-direction jitter
_FLATTEN          = (0.50, 0.80)  # vertical squash of support directions:
                                  # stones are SLABS laid on their beds —
                                  # the round-3 "flatter stones" call.
                                  # Applied in direction space, so the hull
                                  # fills the (short) cell height while the
                                  # silhouette stays wide
_PACK_COMP_MM     = 2.1           # oversize before rounding; roundover +
                                  # blur-remesh shave ~1.5 mm back and the
                                  # remainder makes neighbours KISS — size
                                  # irregularity must come from the grid
                                  # (courses/bays/throughstones), never from
                                  # under-filling cells: an under-filled cell
                                  # exposes flat core, which reads "pebbles
                                  # glued on a slab" (round-3 regression)
_BULGE_MM         = 0.35          # residual vertical swell into joints
_THROUGH_FRAC     = 0.20          # fraction of eligible cells merged with
                                  # the cell above into a throughstone
_REMESH_SIGMA     = 0.9           # aged read (stones convention)


def _fieldstone_mesh(lx: float, ly: float, lz: float, blockiness: float,
                     flat_top: bool, roundover_mm: float, relief_mm: float,
                     relief_wl: tuple[float, float],
                     rng: np.random.Generator) -> trimesh.Trimesh:
    """One flat-bedded stone filling the local box [0,lx]×[0,ly]×[0,lz]."""
    M = int(rng.integers(13, 18))
    i = np.arange(M) + 0.5
    phi   = np.arccos(1.0 - 2.0 * i / M)
    theta = np.pi * (1.0 + np.sqrt(5.0)) * i
    d = np.stack([np.sin(phi) * np.cos(theta),
                  np.sin(phi) * np.sin(theta), np.cos(phi)], axis=1)
    d += rng.normal(0.0, _DIR_JITTER, d.shape)
    # Slab squash: compress vertical components so support points crowd
    # the equator — the hull gets broad bed/top faces and short sides,
    # the flat-laid read, regardless of the cell aspect.
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
                 joint_mm:  float = 0.4,
                 reveal_mm: float = 1.2,
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

        up = 0.0 if cell.is_top else rng.uniform(0.0, _BULGE_MM)
        dn = 0.0 if cell.is_bottom else rng.uniform(0.0, _BULGE_MM)
        # Compensate the rounding shave in ALL THREE axes.  y (thickness)
        # matters most: without it every stone loses ~0.7 mm per face to
        # roundover+blur and sits nearly flush with the core — the whole
        # wall reads as a flat slab with faint lumps (round-4 find; the
        # standalone strip test looked right because there was no core
        # to compare against).
        cx = _PACK_COMP_MM
        cy = _PACK_COMP_MM
        cz_up = 0.0 if cell.is_top else _PACK_COMP_MM / 2.0
        cz_dn = _PACK_COMP_MM / 2.0
        body = _fieldstone_mesh(lx + cx, ly + cy,
                                lz + up + dn + cz_up + cz_dn,
                                blockiness, cell.is_top or cell.is_quoin,
                                self.roundover_mm, self.relief_mm,
                                self.relief_wl, rng)
        body.apply_translation([-cx / 2.0, -cy / 2.0, -(dn + cz_dn)])
        return body
