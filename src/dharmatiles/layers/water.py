"""
WaterLayer: flat water-surface placeholder.

Generates a flat coloured mesh at the water-region height.  The mesh tiles
the water region using a subsampled quad grid so the vertex count stays
manageable.

TODO (future water rendering):
  - Animated ripple / wave displacement
  - Subsurface caustic texture
  - Reflective-looking normal map
  - Depth-shaded colour gradient (shallow → deep)
  - Features placed on the slope leading to water must be oriented normal
    to the slope surface, not to the world-horizontal plane — this affects
    soil blobs, stone placement, and any future water-edge vegetation.
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..core.config import SurfaceConfig


class WaterLayer:
    """Build a flat water-surface mesh coloured blue.

    The mesh is a stub placeholder.  It sits at *height_mm* (the water
    region's terrain height) and covers all grid cells where *water_mask*
    is True, tiled in SUBSAMPLE×SUBSAMPLE blocks.

    Parameters
    ----------
    surface    : SurfaceConfig — tile dimensions and grid resolution.
    height_mm  : float — z level of the water surface in mm.
    """

    SUBSAMPLE: int = 4   # merge this many grid cells per quad edge

    def __init__(self, surface: SurfaceConfig, height_mm: float) -> None:
        self.surface   = surface
        self.height_mm = height_mm

    def build(self, water_mask: np.ndarray) -> list[trimesh.Trimesh]:
        """Return a flat mesh covering *water_mask* cells at *height_mm*.

        Returns an empty list if the mask has no True cells.
        """
        surface = self.surface
        h       = self.height_mm
        S       = self.SUBSAMPLE

        gh, gw = water_mask.shape
        verts: list[list[float]] = []
        faces: list[list[int]]   = []

        r = 0
        while r + S <= gh:
            c = 0
            while c + S <= gw:
                # Include this block only if all four corners are water
                if (water_mask[r,         c        ] and
                        water_mask[r + S - 1, c        ] and
                        water_mask[r,         c + S - 1] and
                        water_mask[r + S - 1, c + S - 1]):
                    x0, y0 = c * surface.cell_w,         r * surface.cell_h
                    x1, y1 = (c + S) * surface.cell_w,   (r + S) * surface.cell_h
                    i0 = len(verts)
                    verts += [[x0, y0, h], [x1, y0, h],
                              [x1, y1, h], [x0, y1, h]]
                    faces += [[i0, i0 + 1, i0 + 2],
                              [i0, i0 + 2, i0 + 3]]
                c += S
            r += S

        if not verts:
            return []

        mesh = trimesh.Trimesh(
            vertices = np.array(verts, dtype=float),
            faces    = np.array(faces, dtype=np.int32),
            process  = False,
        )
        mesh.fix_normals()
        return [mesh]
