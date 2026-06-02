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

Slope assumption (shoreline)
----------------------------
The slope strip between the grass meadow and the water pool is currently bare
soil.  No features (stones, grass, soil blobs) are placed there, so the
world-horizontal placement assumption does not produce visible artefacts at
the ≈22° slope used in the grass-and-water tile.

If features are ever placed on the slope they must be oriented to the surface
normal, not to world-Z.  The shared entry point for this is
``TileScene.terrain_normal(x, y)`` (to be implemented in core/tile.py);
see also the per-layer Slope assumption sections in soil.py, stones.py, and
grass.py.
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..core.config import SurfaceConfig


WATER_RENDER_LIFT_MM = 0.0


class WaterLayer:
    """Build a flat water-surface mesh coloured blue.

    The mesh is a stub placeholder.  It sits at *height_mm* and replaces the
    omitted terrain top quads in water cells, so previewers see only explicit
    blue water faces there.  It covers all grid cells where *water_mask* is
    True, tiled in SUBSAMPLE×SUBSAMPLE blocks.

    Parameters
    ----------
    surface    : SurfaceConfig — tile dimensions and grid resolution.
    height_mm  : float — semantic z level of the water surface in mm.
    """

    SUBSAMPLE: int = 1   # match terrain top quads so water replaces them exactly

    def __init__(self, surface: SurfaceConfig, height_mm: float,
                 render_lift_mm: float = WATER_RENDER_LIFT_MM) -> None:
        self.surface        = surface
        self.height_mm      = height_mm
        self.render_lift_mm = render_lift_mm

    def build(self, water_mask: np.ndarray) -> list[trimesh.Trimesh]:
        """Return a flat mesh covering *water_mask* cells at *height_mm*.

        Returns an empty list if the mask has no True cells.
        """
        surface = self.surface
        h       = self.height_mm + self.render_lift_mm
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
