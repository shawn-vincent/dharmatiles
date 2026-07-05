"""
Stone slab flooring — pavement gridding the ground.

Shawn's ask (2026-07-05): "floor tiles (full 35mm or whatever slabs
gridding the ground)".  ``StoneFloor`` paves a region with flat stone
slabs aligned to the DB square grid — the masonry unit kernel laid
horizontal: each slab is the same chipped-box hull + roundover +
broadband relief as a wall block (``_block_mesh``), with the wall
texture presets.

Reads by construction:

- slabs align to the square grid (``slabs_per_square`` subdivides);
  the joint gaps land on grid lines, so the pavement IS the grid;
- joints are real gaps with the SOIL showing through — dirt between
  pavers;
- each slab seats on its own patch of terrain (top = local high point
  + ``proud_mm``), so pavement rolls gently with the ground;
- ``missing_prob`` slabs leave a bare dirt patch, ``spall_prob``
  slabs get a broken corner — the grid must be imperfect to read real.
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..core.color import Material, tag as _tag
from .masonry import _TEXTURES, _block_mesh

_TILT_DEG = 0.5      # per-slab tilt: each slab catches its own light


class StoneFloor:
    """Direct TileLayer: stone slab pavement over the region footprint.

    Same texture vocabulary as the walls (``chipped`` / ``worn`` /
    ``hewn`` / ``dressed`` presets, plus the same override kwargs).
    Place after ``SoilCarpet`` and before any ``Grass`` (the paved
    area stamps ``obstacle_mask`` so grass keeps off the slabs).
    """

    height_default_mm: float = 5.0

    def __init__(self, *,
                 texture:  str = 'dressed',
                 slabs_per_square: int = 1,
                 thickness_mm: float = 4.5,
                 joint_mm:     float = 1.0,
                 proud_mm:     float = 1.1,
                 chip_mm:      float | None = None,
                 roundover_mm: float | None = None,
                 relief_mm:    float | None = None,
                 relief_wl:    tuple[float, float] | None = None,
                 missing_prob: float = 0.04,
                 spall_prob:   float = 0.10,
                 seed:         int = 0):
        if texture not in _TEXTURES:
            raise ValueError(f'unknown floor texture {texture!r}; '
                             f'options: {sorted(_TEXTURES)}')
        preset = _TEXTURES[texture]
        self.texture      = texture
        self.slabs_per_square = int(slabs_per_square)
        self.thickness_mm = thickness_mm
        self.joint_mm     = joint_mm
        self.proud_mm     = proud_mm
        self.chip_mm      = preset['chip_mm'] if chip_mm is None else chip_mm
        self.roundover_mm = (preset['roundover_mm'] if roundover_mm is None
                             else roundover_mm)
        self.relief_mm    = (preset['relief_mm'] if relief_mm is None
                             else relief_mm)
        self.relief_wl    = (preset['relief_wl'] if relief_wl is None
                             else relief_wl)
        self.missing_prob = missing_prob
        self.spall_prob   = spall_prob
        self.seed         = seed

    def apply(self, scene, *, placement_mask=None) -> list[trimesh.Trimesh]:
        surface = scene.surface
        cw, gw, gh = surface.cell_w, surface.grid_w, surface.grid_h
        pitch = surface.square_mm / self.slabs_per_square
        nx = int(round(surface.tile_w / pitch))
        ny = int(round(surface.tile_h / pitch))

        parts = []
        for iy in range(ny):
            for ix in range(nx):
                brng = np.random.default_rng(
                    (self.seed * 1_000_003 + hash((ix, iy))) & 0x7FFFFFFF)
                x0, y0 = ix * pitch, iy * pitch
                cx, cy = x0 + pitch / 2.0, y0 + pitch / 2.0
                i, j = int(cx / cw), int(cy / cw)
                if not (0 <= i < gw and 0 <= j < gh):
                    continue
                if placement_mask is not None and not placement_mask[j, i]:
                    continue
                if brng.random() < self.missing_prob:
                    continue    # bare dirt patch

                # Seat on the slab's own terrain patch: the pavement
                # rolls with the ground, each slab at its local level.
                i0 = max(0, int(x0 / cw))
                i1 = min(gw, int((x0 + pitch) / cw) + 1)
                j0 = max(0, int(y0 / cw))
                j1 = min(gh, int((y0 + pitch) / cw) + 1)
                patch = scene.terrain_z[j0:j1, i0:i1]
                seat = float(np.percentile(patch, 90.0)) if patch.size \
                    else 0.0
                top = seat + self.proud_mm

                chip = self.chip_mm
                if brng.random() < self.spall_prob:
                    chip = min(0.30 * pitch * 0.25, 1.6) + self.chip_mm
                side = pitch - self.joint_mm
                body = _block_mesh(side, side, self.thickness_mm, 0.0,
                                   chip, self.roundover_mm, self.relief_mm,
                                   self.relief_wl, True, brng)
                ctr = np.array([side / 2.0, side / 2.0,
                                self.thickness_mm / 2.0])
                for axis in ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]):
                    tilt = np.radians(brng.uniform(-_TILT_DEG, _TILT_DEG))
                    body.apply_transform(
                        trimesh.transformations.rotation_matrix(
                            tilt, axis, ctr))
                body.apply_translation([
                    x0 + self.joint_mm / 2.0, y0 + self.joint_mm / 2.0,
                    top - self.thickness_mm])
                parts.append(body)

                # Stamp the slab footprint.
                sl = scene.terrain_support_z[j0:j1, i0:i1]
                np.maximum(sl, top, out=sl)
                scene.obstacle_mask[j0:j1, i0:i1] = True

        for p in parts:
            _tag(p, Material.ROCK)
        return parts

    def footprint_mm(self) -> float:
        return 0.0
