"""
Fieldstone / drystone wall — walls campaign family 3.

Same chassis as :class:`CutStoneWall` (spine → segments, courses/bays
with bond offset, quoin alternation, recessed core, seat/stamp/clip);
only the UNIT changes, per the design doc ("same block-instancing,
different layout").  Reference read (`docs/reference/walls/README.md`,
Hirst m70): rough courses of rounded/angular stones with big size
variety, DEEP shadow joints (no mortar — the recessed core reads as
darkness between stones), squared quoins at corners, and larger
flat-topped coping stones capping the wall.

Each stone is the convex hull of Fibonacci-sampled support points on a
blend between the cell's inscribed ellipsoid and the cell box —
``blockiness`` 0 = ovoid river stone, 1 = squared block — with per-point
lumpiness, so one wall mixes rounded and angular stones the way a real
drystone wall does.  Stones bulge vertically past their course band
into the deep joints (grid broken); corner cells keep the chipped-box
quoin (real drystone corners are built from squared stones).
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..scatter.stones import _blur_remesh, _relief_field, _round_edges
from .masonry import CutStoneWall, _Cell, _block_mesh, _RELIEF_WAVES

# ── Iteration knobs (module constants while prototyping) ─────────────────────
# Packing rules the read: inscribed ellipsoids fill only ~half the cell
# and the wall read "pebbles stuck on a slab" (E6 find) — stones must be
# blocky enough to reach their cell walls, with the blend rounding only
# the corners.
_BLOCKINESS       = (0.38, 0.75)  # ellipsoid↔box blend range for body stones
_CAP_BLOCKINESS   = (0.55, 0.80)  # coping stones are squarer and flat-topped
_LUMP             = (0.93, 1.0)   # per-point radius jitter
_DIR_JITTER       = 0.12          # support-direction jitter
_BULGE_MM         = 0.45          # stones may swell past their course band
                                  # into the deep joints (up and down)
_QUOIN_CHAMFER_MM = 0.9           # cap on the quoin bottom chamfer: the
                                  # inherited reveal-sized chamfer (1.7)
                                  # stacked into chevrons up the arris
_PACK_COMP_MM     = 1.6           # oversize each stone body by this before
                                  # rounding: roundover + blur-remesh shave
                                  # ~this much total, which reopened the
                                  # joints into flat core expanses (E6 find —
                                  # packed stones, thin dark joints, is the
                                  # whole drystone read)
_CAP_CLAMP_FRAC   = 0.90          # coping top points clamp to this × half-
                                  # height → flat cap facet, textured stone top
_REMESH_SIGMA     = 0.9           # aged read (stones convention), softer than
                                  # the cut-block 0.7


def _fieldstone_mesh(lx: float, ly: float, lz: float, blockiness: float,
                     flat_top: bool, roundover_mm: float, relief_mm: float,
                     relief_wl: tuple[float, float],
                     rng: np.random.Generator) -> trimesh.Trimesh:
    """One lumpy stone filling the local box [0,lx]×[0,ly]×[0,lz]."""
    M = int(rng.integers(12, 17))
    i = np.arange(M) + 0.5
    phi   = np.arccos(1.0 - 2.0 * i / M)
    theta = np.pi * (1.0 + np.sqrt(5.0)) * i
    d = np.stack([np.sin(phi) * np.cos(theta),
                  np.sin(phi) * np.sin(theta), np.cos(phi)], axis=1)
    d += rng.normal(0.0, _DIR_JITTER, d.shape)
    d /= np.linalg.norm(d, axis=1, keepdims=True) + 1e-12

    half = np.array([lx / 2.0, ly / 2.0, lz / 2.0])
    r_ell = 1.0 / np.sqrt(((d / half) ** 2).sum(axis=1))
    r_box = 1.0 / (np.abs(d) / half).max(axis=1)
    r = ((1.0 - blockiness) * r_ell + blockiness * r_box)
    r *= rng.uniform(*_LUMP, M)
    p = d * r[:, None]
    if flat_top:
        p[:, 2] = np.minimum(p[:, 2], _CAP_CLAMP_FRAC * half[2])
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

    yaw_max_deg:  float = 2.5     # stones sit less true than cut blocks
    tilt_max_deg: float = 1.2

    def __init__(self, spine, *,
                 course_mm: tuple[float, float] = (3.8, 7.5),
                 bay_mm:    tuple[float, float] = (5.5, 13.0),
                 joint_mm:  float = 0.6,
                 reveal_mm: float = 1.5,
                 roundover_mm: float | None = 0.5,
                 relief_mm:    float | None = 0.12,
                 relief_wl:    tuple[float, float] | None = (2.0, 7.0),
                 min_bond_mm:  float = 2.0,
                 **kwargs):
        super().__init__(spine, course_mm=course_mm, bay_mm=bay_mm,
                         joint_mm=joint_mm, reveal_mm=reveal_mm,
                         roundover_mm=roundover_mm, relief_mm=relief_mm,
                         relief_wl=relief_wl, min_bond_mm=min_bond_mm,
                         **kwargs)

    def _unit_mesh(self, lx: float, ly: float, lz: float, chamfer: float,
                   cell: _Cell, rng: np.random.Generator) -> trimesh.Trimesh:
        if cell.is_quoin:
            # Squared quoin stones at corners — how drystone is really
            # built, and the corner keeps its crisp alternating arris.
            # Built with the cut-stone 'chipped' params: the fieldstone
            # roundover blunted every quoin into a soft nose set deep
            # behind the adjacent run's face (E6 chevron artifact).
            return _block_mesh(lx, ly, lz, min(chamfer, _QUOIN_CHAMFER_MM),
                               0.55, 0.22, 0.07, (1.5, 6.0),
                               cell.is_top, rng)

        flat_top = cell.is_top
        blockiness = rng.uniform(*(_CAP_BLOCKINESS if flat_top
                                   else _BLOCKINESS))
        # Vertical bulge into the deep joints breaks the course grid;
        # the cap plane and the embedded base stay respected.
        up = 0.0 if cell.is_top else rng.uniform(0.0, _BULGE_MM)
        dn = (0.0 if cell.is_bottom else rng.uniform(0.0, _BULGE_MM))
        # Pack compensation: build oversize, let rounding shave it back
        # to the cell — the top course only grows downward (cap plane).
        cx = _PACK_COMP_MM
        cz_up = 0.0 if cell.is_top else _PACK_COMP_MM / 2.0
        cz_dn = _PACK_COMP_MM / 2.0
        body = _fieldstone_mesh(lx + cx, ly, lz + up + dn + cz_up + cz_dn,
                                blockiness, flat_top,
                                self.roundover_mm, self.relief_mm,
                                self.relief_wl, rng)
        body.apply_translation([-cx / 2.0, 0.0, -(dn + cz_dn)])
        return body
