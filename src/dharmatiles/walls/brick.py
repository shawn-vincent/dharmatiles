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

    def __init__(self, spine, *,
                 course_mm: tuple[float, float] = (2.5, 3.0),
                 bay_mm:    tuple[float, float] = (5.0, 6.6),
                 joint_mm:  float = 0.5,
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
                    and cell.end0 != 'face' and cell.end1 != 'face')
        if eligible and mrng.random() < self.missing_prob:
            return None
        return super()._place_block(cell, segs, seat_z, rng)

    def _unit_mesh(self, lx: float, ly: float, lz: float, chamfer: float,
                   cell: _Cell, rng: np.random.Generator) -> trimesh.Trimesh:
        chip = self.chip_mm
        if not cell.is_quoin and rng.random() < self.spall_prob:
            # Spalled unit: the chip budget jumps to a third of the
            # brick — broken rounded arrises, a crumbling face.
            chip = min(0.35 * min(lx, lz), 1.2)
        return _block_mesh(lx, ly, lz, chamfer, chip,
                           self.roundover_mm, self.relief_mm,
                           self.relief_wl, cell.is_top, rng)
