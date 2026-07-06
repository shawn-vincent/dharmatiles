"""
Stone slab flooring — pavement gridding the ground.

Shawn (2026-07-05): "floor tiles (full 35mm or whatever slabs gridding
the ground)" … "the slab should go right to the bottom of the terrain"
… "None of the floor terrain should be on top of soil.  The floor IS
the terrain, replaces the soil."

``StoneFloor`` paves a region with flat stone slabs aligned to the DB
square grid — the masonry unit kernel laid horizontal (``_block_mesh``
with the wall texture presets).  The floor REPLACES the terrain:

- every paved pitch cell (including missing-slab cells) has its
  terrain LOWERED to a thin dirt bed (``bed_mm``) — no soil under or
  around the pavement surface;
- slabs run FULL DEPTH, from the tile bottom (z = 0) to the pavement
  level (``top_mm`` ± jitter) — no undercut can exist, by construction.
  The default 7.4 mm is the OFFICIAL interior-floor standard: measured
  across the DungeonBlocks library (docs/reference/walls/
  commercial-sets-analysis.md §ground-heights), interior stone floors
  sit 17.8–18.8 mm above the piece bottom = 6.9–7.9 mm above our
  terrain datum (base = 5.7 peg + 5.2 flare = 10.9).  Outdoor paths
  are lower (~6.0): pass ``top_mm=6.0`` for a garden path;
- joints land on the grid lines and read as razor-thin dark cracks
  (0.25 mm nominal, thinner than brick joints) flooring onto the
  MORTAR CORE at wall-joint depth (``reveal_mm``): one recessed sheet
  welds the slabs into a single connected solid, the wall rule turned
  horizontal; a ``missing_prob`` slab leaves a sunken dirt pit;
- ``spall_prob`` slabs get broken corners; ``crack_prob`` slabs are
  engraved with the standard rock crack (``stone/cracks.py``) — the
  grid must be imperfect to read real.
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..stone import stone_relief
from ..stone.cracks import engrave_cracks
from .masonry import _TEXTURES, _block_mesh, assemble_masonry

_TILT_DEG = 0.5      # per-slab tilt: each slab catches its own light
_TOP_JITTER_MM = 0.3  # per-slab pavement-level jitter


class StoneFloor:
    """Direct TileLayer: stone slab pavement that IS the ground.

    Same texture vocabulary as the walls (``chipped`` / ``worn`` /
    ``hewn`` / ``dressed`` presets, plus the same override kwargs).
    Place after ``SoilCarpet`` (the pavement flattens the soil under
    itself) and before any ``Grass`` (the paved area stamps
    ``obstacle_mask``).
    """

    height_default_mm: float = 5.0

    def __init__(self, *,
                 texture:  str = 'dressed',
                 slabs_per_square: int = 1,
                 top_mm:       float = 7.4,
                 bed_mm:       float = 1.2,
                 joint_mm:     float = 0.25,  # razor-thin crack —
                                              # thinner than the brick
                                              # joints (0.5)
                 reveal_mm:    float = 1.3,   # mortar core recessed
                                              # this far below the
                                              # pavement top — joints
                                              # floor onto mortar at
                                              # the same depth as the
                                              # wall joints
                 chip_mm:      float | None = None,
                 roundover_mm: float | None = None,
                 relief_mm:    float | None = None,
                 relief_wl:    tuple[float, float] | None = None,
                 missing_prob: float = 0.04,
                 spall_prob:   float = 0.10,
                 crack_prob:   float = 0.20,
                 seed:         int = 0):
        if texture not in _TEXTURES:
            raise ValueError(f'unknown floor texture {texture!r}; '
                             f'options: {sorted(_TEXTURES)}')
        preset = _TEXTURES[texture]
        self.texture      = texture
        self.slabs_per_square = int(slabs_per_square)
        self.top_mm       = top_mm
        self.bed_mm       = bed_mm
        self.joint_mm     = joint_mm
        self.reveal_mm    = reveal_mm
        self.chip_mm      = preset['chip_mm'] if chip_mm is None else chip_mm
        self.roundover_mm = (preset['roundover_mm'] if roundover_mm is None
                             else roundover_mm)
        self.relief_mm    = (preset['relief_mm'] if relief_mm is None
                             else relief_mm)
        self.relief_wl    = (preset['relief_wl'] if relief_wl is None
                             else relief_wl)
        self.missing_prob = missing_prob
        self.spall_prob   = spall_prob
        self.crack_prob   = crack_prob
        self.seed         = seed

    def apply(self, scene, *, placement_mask=None) -> list[trimesh.Trimesh]:
        surface = scene.surface
        cw, gw, gh = surface.cell_w, surface.grid_w, surface.grid_h
        pitch = surface.square_mm / self.slabs_per_square
        nx = int(round(surface.tile_w / pitch))
        ny = int(round(surface.tile_h / pitch))

        parts = []
        placed = np.zeros((ny, nx), dtype=bool)
        tops = np.zeros((ny, nx))
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

                # The floor IS the terrain: drop this pitch cell (slab
                # or dirt pit alike) to the thin dirt bed — the soil
                # never sits at pavement level anywhere paved.
                i0 = max(0, int(x0 / cw))
                i1 = min(gw, int(np.ceil((x0 + pitch) / cw)))
                j0 = max(0, int(y0 / cw))
                j1 = min(gh, int(np.ceil((y0 + pitch) / cw)))
                patch = scene.terrain_z[j0:j1, i0:i1]
                if brng.random() < self.missing_prob:
                    # Sunken dirt pit: keep the dirt bed visible.
                    np.minimum(patch, self.bed_mm, out=patch)
                    continue
                # Under a slab there is NO soil at all: the terrain
                # drops to the datum (z = 0), where the tile's
                # structural slab still runs base_h deep below it —
                # the same convention as water pool floors.  At tile
                # edges you see ONLY the slab side.
                np.minimum(patch, 0.0, out=patch)

                top = self.top_mm + float(
                    brng.uniform(-_TOP_JITTER_MM, _TOP_JITTER_MM))
                placed[iy, ix] = True
                tops[iy, ix] = top
                chip = self.chip_mm
                if brng.random() < self.spall_prob:
                    chip = min(0.30 * pitch * 0.25, 1.6) + self.chip_mm
                # Joints inset the slab EXCEPT at tile boundaries:
                # floor tiles butt slab-to-slab like walls do (R8) —
                # only floor block at the tile edge, and the seam
                # between two tiles reads as its own grid line.
                jx0 = 0.0 if ix == 0 else self.joint_mm / 2.0
                jx1 = 0.0 if ix == nx - 1 else self.joint_mm / 2.0
                jy0 = 0.0 if iy == 0 else self.joint_mm / 2.0
                jy1 = 0.0 if iy == ny - 1 else self.joint_mm / 2.0
                side_x = pitch - jx0 - jx1
                side_y = pitch - jy0 - jy1
                side = max(side_x, side_y)
                # Full-depth slab: tile bottom to pavement level.
                body = _block_mesh(side_x, side_y, top, 0.0,
                                   chip, self.roundover_mm, self.relief_mm,
                                   self.relief_wl, True, brng)
                # The common stone relief (stone/finish.py, design:
                # docs/design/stone-surface-texture.md): a calm
                # plateau carved downward — worn recesses + gentle
                # dish, matching the measured official floors.  The
                # plane-wave drybrush pass read as corduroy/chop here
                # and is gone.  Clamp the underside back to the tile
                # bottom afterwards.
                body = body.subdivide()
                p0 = np.asarray(body.vertices)
                v = stone_relief(body, brng)
                # Fade the carve out near the slab base: a deep side
                # recess at the foot exposes the terrain film at tile
                # edges (orange flecks).
                fade = np.clip(p0[:, 2] / 1.5, 0.0, 1.0)[:, None]
                v = p0 + (v - p0) * fade
                v[:, 2] = np.maximum(v[:, 2], 0.0)
                body = trimesh.Trimesh(vertices=v, faces=body.faces,
                                       process=False)
                ctr = np.array([side_x / 2.0, side_y / 2.0, top / 2.0])
                for axis in ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]):
                    tilt = np.radians(brng.uniform(-_TILT_DEG, _TILT_DEG))
                    body.apply_transform(
                        trimesh.transformations.rotation_matrix(
                            tilt, axis, ctr))
                body.apply_translation([x0 + jx0, y0 + jy0, 0.0])
                if brng.random() < self.crack_prob:
                    # The standard rock crack, walk length scaled to
                    # the slab so it crosses a good fraction of it.
                    body = engrave_cracks(
                        body, np.random.default_rng(
                            (self.seed * 61 + hash((ix, iy)))
                            & 0x7FFFFFFF),
                        ground_z=0.0, footprint_mm=side,
                        n_segs=int(np.clip(side * 0.7, 9, 26)))
                parts.append(body)

                # Stamp the slab footprint.
                sl = scene.terrain_support_z[j0:j1, i0:i1]
                np.maximum(sl, top, out=sl)
                scene.obstacle_mask[j0:j1, i0:i1] = True

        if not parts:
            return []
        # Mortar core (Shawn: "for structural reasons, have some mortar
        # inside the floor slabs … the same way we do for walls"): one
        # sheet per placed cell spanning the FULL pitch (so neighbours'
        # sheets merge across the joints and the pavement unions into
        # one connected solid), recessed reveal_mm below the pavement
        # top — the razor joints floor onto mortar at wall-joint depth
        # — and inset reveal_mm from every visible side (tile
        # boundaries, unpaved cells, dirt pits), exactly the wall rule.
        rv = self.reveal_mm
        for iy in range(ny):
            for ix in range(nx):
                if not placed[iy, ix]:
                    continue
                x0, y0 = ix * pitch, iy * pitch
                cx0 = x0 + (rv if ix == 0 or not placed[iy, ix - 1]
                            else 0.0)
                cx1 = x0 + pitch - (rv if ix == nx - 1
                                    or not placed[iy, ix + 1] else 0.0)
                cy0 = y0 + (rv if iy == 0 or not placed[iy - 1, ix]
                            else 0.0)
                cy1 = y0 + pitch - (rv if iy == ny - 1
                                    or not placed[iy + 1, ix] else 0.0)
                zc = tops[iy, ix] - rv
                box = trimesh.creation.box(
                    extents=[cx1 - cx0, cy1 - cy0, zc])
                box.apply_translation([(cx0 + cx1) / 2.0,
                                       (cy0 + cy1) / 2.0, zc / 2.0])
                parts.append(box)

        return [assemble_masonry(parts, surface, 'StoneFloor')]

    def footprint_mm(self) -> float:
        return 0.0
