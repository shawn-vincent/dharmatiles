"""
Stone slab flooring — a cut-stone wall lying on its side.

Shawn (2026-07-06): "the floor should just be a wall laying on its
side … with different sized bricks."  ``StoneFloor`` is exactly that:
``CutStoneWall(laid_flat=True)`` whose courses and bays are pinned to
the DB square grid (``slabs_per_square`` subdivides), so one masonry
chassis generates walls AND floors:

- courses → pavement rows, bays → slabs, bond stagger → off
  (``min_bond_mm=0`` + exact-pitch ranges give grid alignment);
- wall thickness → pavement depth: slabs run full depth from the
  datum (underside overshoots and is clipped flat) to ``top_mm`` —
  7.4, the measured official interior-floor level (~6.0 reads as an
  outdoor path);
- the recessed mortar core → one sheet ``reveal_mm`` below the
  pavement top, inset from the strip's plan edges — joints floor onto
  mortar at exactly wall-joint depth;
- the wall's face proudness, texture presets, common stone relief,
  and drybrush grain all apply unchanged.

The floor REPLACES the terrain (chassis ``laid_flat`` rule): soil
under the strip drops to a 0.15 mm film, invisible inside the
full-depth slabs.  ``missing_prob`` slabs leave dark pits,
``spall_prob`` slabs get broken corners, ``crack_prob`` slabs are
engraved with the standard rock crack — the grid must be imperfect to
read real.

A DRYSTONE floor is the same idea with the fieldstone family:
``FieldstoneWall(laid_flat=True, ...)`` — crack-network flagstones
with rubble chinking in the joints.
"""
from __future__ import annotations

import numpy as np
import trimesh  # noqa: F401  (return-type annotations)

from ..stone.cracks import engrave_cracks
from .masonry import CutStoneWall, _Cell, _Seg, _block_mesh


class StoneFloor(CutStoneWall):
    """Direct TileLayer: stone slab pavement that IS the ground.

    Same texture vocabulary as the walls (``chipped`` / ``worn`` /
    ``hewn`` / ``dressed`` presets, plus the same override kwargs).
    Place after ``SoilCarpet`` and before walls that should stand on
    the pavement (they seat on ``terrain_support_z``).
    """

    height_default_mm: float = 5.0

    def __init__(self, *,
                 texture:  str = 'dressed',
                 slabs_per_square: int = 1,
                 top_mm:       float = 7.4,
                 joint_mm:     float = 0.125,
                 reveal_mm:    float = 1.3,
                 chip_mm:      float | None = None,
                 roundover_mm: float | None = None,
                 relief_mm:    float | None = None,
                 relief_wl:    tuple[float, float] | None = None,
                 missing_prob: float = 0.04,
                 spall_prob:   float = 0.10,
                 crack_prob:   float = 0.20,
                 seed:         int = 0):
        super().__init__(spine=[(0.0, 0.0), (1.0, 0.0)],  # set in apply
                         laid_flat=True,
                         thickness_mm=top_mm,
                         height_mm=1.0,                    # set in apply
                         seed=seed, texture=texture,
                         joint_mm=joint_mm, reveal_mm=reveal_mm,
                         chip_mm=chip_mm, roundover_mm=roundover_mm,
                         relief_mm=relief_mm, relief_wl=relief_wl,
                         course_mm=(1.0, 1.0), bay_mm=(1.0, 1.0),
                         min_bond_mm=0.0)
        self.face_recess_mm   = 0.0   # slabs flush in plan
        self.slabs_per_square = int(slabs_per_square)
        self.missing_prob     = missing_prob
        self.spall_prob       = spall_prob
        self.crack_prob       = crack_prob

    def apply(self, scene, *, placement_mask=None) -> list[trimesh.Trimesh]:
        surface = scene.surface
        sq = surface.square_mm
        pitch = sq / self.slabs_per_square
        # Spine along the south edge walking +x: the strip (the wall
        # body) extends north over the whole tile; courses/bays at
        # exact grid pitch with bond stagger off → slabs on the grid.
        self.spine     = [(0.0, 0.0), (float(surface.cols), 0.0)]
        self.height_mm = surface.rows * sq
        self.course_mm = (pitch, pitch)
        self.bay_mm    = (pitch, pitch)
        return super().apply(scene, placement_mask=placement_mask)

    def _place_block(self, cell: _Cell, segs: list[_Seg], seat_z: float,
                     rng: np.random.Generator) -> trimesh.Trimesh | None:
        # Missing slab: a dark pit down to the terrain film (the grid
        # must be imperfect).  Own rng salt, like BrickWall.
        mrng = np.random.default_rng(
            (self.seed * 31 + hash(cell.key)) & 0x7FFFFFFF)
        if mrng.random() < self.missing_prob:
            return None
        body = super()._place_block(cell, segs, seat_z, rng)
        if body is not None and mrng.random() < self.crack_prob:
            # The standard rock crack, walk length scaled to the slab.
            side = min(cell.t1 - cell.t0, cell.z1 - cell.z0)
            body = engrave_cracks(
                body, np.random.default_rng(
                    (self.seed * 61 + hash(cell.key)) & 0x7FFFFFFF),
                ground_z=0.0, footprint_mm=side,
                n_segs=int(np.clip(side * 0.7, 9, 26)))
        return body

    def _unit_mesh(self, lx: float, ly: float, lz: float,
                   cell: _Cell, rng: np.random.Generator) -> trimesh.Trimesh:
        chip = self.chip_mm
        if rng.random() < self.spall_prob:
            # Broken corner, capped below the mortar reveal.
            chip = min(0.30 * min(lx, ly) * 0.25, 0.30) + self.chip_mm
        # Chip pull masked flush on tile-boundary sides: the
        # inter-slab spacing is between slabs on a tile, never between
        # a slab and the tile edge (local x = run ends; local y = rows,
        # so the bottom/top rows are the strip's plan edges).
        mask = (float(cell.end0 == 'joint'), float(cell.end1 == 'joint'),
                float(not cell.is_bottom), float(not cell.is_top))
        return _block_mesh(lx, ly, lz, chip, self.roundover_mm,
                           self.relief_mm, self.relief_wl, True, rng,
                           pull_mask=mask)

    def footprint_mm(self) -> float:
        return 0.0
