"""
Worn brick / small-block wall — walls campaign family 2.

Reference: ``docs/reference/walls/brick-worn.jpg`` — running bond of
small near-uniform units, ERODED mortar joints (deep dark recesses —
negative relief is the whole read), spalled bricks with rounded worn
arrises breaking the grid, and the occasional missing brick showing
the recessed mortar plane behind.  "The grid is *imperfect*, that's
the whole read" (reference README).

On the masonry chassis this family is almost pure configuration:

- small tight course/bay ranges → near-regular units whose residual
  sampling jitter (and the min-bond stagger) IS the broken grid;
- ``joint``/``reveal`` tuned for eroded mortar: joints read deep and
  slightly wide, the core showing as the mortar plane;
- the unit kernel is the shared ``_block_mesh`` with worn-brick
  finish (small chip, visible roundover, fine relief);
- ``spall_prob`` bricks re-roll with a big chip budget — a broken,
  crumbling unit; ``missing_prob`` cells place NO unit at all, the
  chassis' None-guard leaving a dark socket floored by the core.
"""
from __future__ import annotations

import numpy as np
import trimesh

from .masonry import CutStoneWall, _Cell, _Seg, _block_mesh


class BrickWall(CutStoneWall):
    """Direct TileLayer: a worn running-bond brick wall on a plan spine.

    Same spine convention and contracts as :class:`CutStoneWall`.
    ``missing_prob`` — chance a brick is absent (dark socket showing
    the recessed mortar core; never in the bottom/top course, at
    quoins, or at textured wall ends).  ``spall_prob`` — chance a
    brick is spalled (heavily chipped broken unit).
    """

    surround_vw:   float = 2.1     # bricks-on-end rowlock arch
    surround_ring: float = 3.6
    surround_jd:   float = 3.4
    surround_jh:   tuple = (2.6, 3.2)
    surround_chip: float = 0.18    # worn brick arrises, mortar joints
    surround_ro:   float = 0.30
    surround_frac: float = 0.94

    def __init__(self, spine, *,
                 course_mm: tuple[float, float] = (2.5, 3.0),
                 bay_mm:    tuple[float, float] = (5.0, 6.6),
                 joint_mm:  float = 0.125,  # halved twice 2026-07-06
                                            # (Shawn); floors match
                 reveal_mm: float = 1.0,
                 min_bond_mm: float = 2.0,
                 texture:   str = 'worn',
                 chip_mm:      float | None = 0.25,
                 roundover_mm: float | None = 0.35,
                 relief_mm:    float | None = 0.08,
                 relief_wl:    tuple[float, float] | None = (1.2, 4.0),
                 missing_prob: float = 0.04,
                 spall_prob:   float = 0.10,
                 **kwargs):
        super().__init__(spine, course_mm=course_mm, bay_mm=bay_mm,
                         joint_mm=joint_mm, reveal_mm=reveal_mm,
                         min_bond_mm=min_bond_mm, texture=texture,
                         chip_mm=chip_mm, roundover_mm=roundover_mm,
                         relief_mm=relief_mm, relief_wl=relief_wl,
                         **kwargs)
        self.missing_prob = missing_prob
        self.spall_prob   = spall_prob

    def _place_block(self, cell: _Cell, segs: list[_Seg], seat_z: float,
                     rng: np.random.Generator) -> trimesh.Trimesh | None:
        # Missing brick: a dark socket floored by the recessed core.
        # Keep the bottom course (seat), the cap, quoins, and textured
        # wall ends intact; own rng salt so the roll doesn't correlate
        # with the brick's shape stream.
        mrng = np.random.default_rng(
            (self.seed * 31 + hash(cell.key)) & 0x7FFFFFFF)
        eligible = (not cell.is_bottom and not cell.is_top
                    and not cell.is_quoin
                    and cell.end0 == 'joint' and cell.end1 == 'joint'
                    and cell.cut_z0 is None and cell.cut_z1 is None)
        if eligible and mrng.random() < self.missing_prob:
            return None
        return super()._place_block(cell, segs, seat_z, rng)

    def _unit_mesh(self, lx: float, ly: float, lz: float,
                   cell: _Cell, rng: np.random.Generator) -> trimesh.Trimesh:
        chip = self.chip_mm
        if not cell.is_quoin and rng.random() < self.spall_prob:
            # Spalled unit: the chip budget jumps to a third of the
            # brick — broken rounded arrises, a crumbling face.
            chip = min(0.35 * min(lx, lz), 1.2)
        return _block_mesh(lx, ly, lz, chip,
                           self.roundover_mm, self.relief_mm,
                           self.relief_wl, cell.is_top, rng,
                           cut_planes=cell.cut_planes_local)


class RegularBrickWall(CutStoneWall):
    """Direct TileLayer: a REGULAR running-bond brick wall — uniform
    course heights, uniform-length bricks, a clean half-brick stagger.
    The tidy counterpart to the worn :class:`BrickWall`: same engine,
    the ``bond='running'`` layout variant plus a near-flush dressed
    unit (barely any wobble or face recession, so the courses read
    dead level).  Same spine convention and contracts as
    :class:`CutStoneWall`.

    ``brick_l_mm`` / ``brick_h_mm`` are the exact brick length and
    course height (end bricks become closers).  There are no spalled
    or missing units — for a broken/ruined regular wall, pass the
    chassis ``ruin=`` knob.
    """

    # Near-flush, dead-level units: the layout is already uniform, so
    # keep only a whisper of wobble/recession for the drybrush catch.
    yaw_max_deg:    float = 0.25
    tilt_max_deg:   float = 0.15
    face_recess_mm: float = 0.12

    # brick-scale dressed surround (matches the BrickWall rowlock arch)
    surround_vw:   float = 2.1
    surround_ring: float = 3.6
    surround_jd:   float = 3.4
    surround_jh:   tuple = (2.6, 3.2)
    surround_chip: float = 0.15
    surround_ro:   float = 0.22
    surround_frac: float = 0.94

    def __init__(self, spine, *,
                 brick_l_mm: float = 8.5,
                 brick_h_mm: float = 3.2,
                 joint_mm:   float = 0.45,   # clean uniform mortar line
                 reveal_mm:  float = 1.0,
                 texture:    str = 'dressed',
                 **kwargs):
        super().__init__(spine, bond='running',
                         course_mm=(brick_h_mm, brick_h_mm),
                         bay_mm=(brick_l_mm, brick_l_mm),
                         joint_mm=joint_mm, reveal_mm=reveal_mm,
                         texture=texture, **kwargs)
