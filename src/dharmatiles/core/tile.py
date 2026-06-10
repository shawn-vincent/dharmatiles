"""
TileScene — mutable state accumulated while building a terrain scene.

The scene holds:
  surface           — SurfaceConfig (dimensions, seed, resolution)
  terrain_z         — float heightmap (read-only after init)
  terrain_support_z — mutable non-vegetation support raised by terrain/rock geometry
  vegetation_support_z — mutable vegetation support, initialised from terrain support
  rock_mask         — bool grid marking rock footprints (grass steers around these)
  parts             — Trimesh objects accumulated during the pipeline
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import List

import numpy as np
import trimesh

from .config import SurfaceConfig


# ─────────────────────────────────────────────────────────────────────────────
# Grid coordinate helpers
# ─────────────────────────────────────────────────────────────────────────────

def make_xy_grids(surface: SurfaceConfig):
    """Return (x_grid, y_grid) world-coordinate arrays (grid_h × grid_w)."""
    iy, ix = np.mgrid[0:surface.grid_h, 0:surface.grid_w]
    return (ix * surface.cell_w).astype(float), (iy * surface.cell_w).astype(float)


# ─────────────────────────────────────────────────────────────────────────────
# Scene
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class TileScene:
    """Mutable state accumulated while building a terrain scene.

    ``terrain_z`` is read-only after construction — no layer or mesh helper
    may mutate it.
    ``terrain_support_z`` grows as terrain and rock layers rasterise geometry.
    ``vegetation_support_z`` grows as vegetation layers rasterise geometry.
    ``parts`` is the list of Trimesh objects to combine at export.
    """
    surface:   SurfaceConfig
    terrain_z: np.ndarray                       # (grid_h, grid_w) — read-only
    terrain_support_z: np.ndarray               # (grid_h, grid_w) — mutable
    vegetation_support_z: np.ndarray | None = None  # (grid_h, grid_w) — mutable
    rock_mask: np.ndarray | None = None         # (grid_h, grid_w) bool — True under a rock
    parts:     List[trimesh.Trimesh] = field(default_factory=list)

    def __post_init__(self) -> None:
        if self.vegetation_support_z is None:
            self.vegetation_support_z = self.terrain_support_z.copy()

    # ── Slope-normal helpers (future) ─────────────────────────────────────────
    #
    # ASSUMPTION (current): all geometry layers (soil, stones, grass) treat the
    # terrain surface as locally horizontal.  Heights and orientations are given
    # in world coordinates: +Z is "up", offsets are added along world-Z, and
    # objects are placed upright regardless of the local slope angle.
    #
    # This is correct for the flat grass zone and the flat water pool.  The
    # only slope in the water+grass tile (≈22° over a 5 mm strip) is bare
    # soil with no placed features, so the visual error is negligible there.
    #
    # When slope-aware placement is needed the entry point is:
    #
    #   def terrain_normal(self, x_mm: float, y_mm: float) -> np.ndarray:
    #       """Unit surface normal at world position (x_mm, y_mm).
    #
    #       Derived from the central-difference gradient of terrain_z:
    #           n = normalize([-dz/dx, -dz/dy, 1])
    #       where dz/dx and dz/dy are in mm/mm (dimensionless slope).
    #       Returns world-Z unit vector [0, 0, 1] when terrain_z is flat.
    #       """
    #       cw = self.surface.cell_w
    #       # ... bilinear sample of gradient ...
    #
    # Callers that need updating when this is implemented:
    #   - _build_rocks_mesh_core           → rotate rock local-Z to terrain_normal
    #   - FloppyGrassLayer (blade origin)  → sink along normal, not world-Z
    #   - FloppyGrassLayer (rise_cap check)→ compare Δ along normal, not abs Δz
    #   - _make_support_post               → measure z_top clearance along normal
    #   - SoilCarpet._accumulate_blob → displace bump along normal
