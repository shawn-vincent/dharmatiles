"""
TileScene — mutable state accumulated while building a terrain scene.

The scene holds:
  surface           — SurfaceConfig (dimensions, seed, resolution)
  terrain_z         — float heightmap; mutate via displace_terrain / set_terrain
  terrain_support_z — mutable non-vegetation support raised by terrain/rock geometry
  vegetation_support_z — mutable vegetation support, initialised from terrain support
  obstacle_mask     — bool grid marking obstacle footprints (grass steers around these)
  parts             — Trimesh objects accumulated during the pipeline

Free-function helpers
---------------------
``derive_seed(master, label, layer_idx=0)``
    Deterministic per-layer RNG seed derived from a master integer, a text
    label, and an optional layer index.  Uses blake2s so each unique
    ``(master, label, layer_idx)`` triple maps to a distinct 32-bit seed with
    no magic XOR constants in calling code.
"""
from __future__ import annotations

import hashlib
from dataclasses import dataclass, field
from typing import List

import numpy as np
import trimesh

from .config import SurfaceConfig


# ─────────────────────────────────────────────────────────────────────────────
# Seed derivation
# ─────────────────────────────────────────────────────────────────────────────

def derive_seed(master: int, label: str, layer_idx: int = 0) -> int:
    """Return a deterministic 32-bit seed for ``(master, label, layer_idx)``.

    Uses blake2s so every unique triple maps to a distinct integer with no
    ad-hoc XOR magic constants in calling code.  The result is safe to feed
    directly to ``np.random.default_rng()``.
    """
    h = hashlib.blake2s(digest_size=4)
    h.update(master.to_bytes(8, 'big', signed=False))
    h.update(label.encode())
    h.update(layer_idx.to_bytes(4, 'big', signed=False))
    return int.from_bytes(h.digest(), 'big')


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

    ``terrain_z`` is the live terrain heightmap.  Mutate it *only* through
    ``displace_terrain`` or ``set_terrain``; both methods automatically keep
    ``terrain_support_z`` in sync so layers do not need their own sync calls.
    ``terrain_support_z`` grows as terrain and rock layers rasterise geometry.
    ``vegetation_support_z`` grows as vegetation layers rasterise geometry.
    ``parts`` is the list of Trimesh objects to combine at export.
    """
    surface:   SurfaceConfig
    terrain_z: np.ndarray                       # (grid_h, grid_w) — mutable via helpers
    terrain_support_z: np.ndarray               # (grid_h, grid_w) — mutable
    vegetation_support_z: np.ndarray | None = None  # (grid_h, grid_w) — mutable
    obstacle_mask: np.ndarray | None = None      # (grid_h, grid_w) bool — True under any placed obstacle (rocks, flowers, …)
    parts:     List[trimesh.Trimesh] = field(default_factory=list)
    # region_mask: same shape as terrain_z; cell value = region index (≥0) or -ve for boundary.
    # Populated by the tile builder; None until then.
    region_mask: np.ndarray | None = None       # (grid_h, grid_w) int32 — region index per cell
    # water_surface_mm: region_index → water surface height (mm).
    # Populated by the tile builder for every region that contains a WaterLayer.
    water_surface_mm: dict = field(default_factory=dict)  # dict[int, float]

    def __post_init__(self) -> None:
        if self.vegetation_support_z is None:
            self.vegetation_support_z = self.terrain_support_z.copy()

    # ── Terrain mutation helpers (D) ──────────────────────────────────────────

    def displace_terrain(
        self,
        delta: np.ndarray,
        mask: np.ndarray | None = None,
    ) -> None:
        """Add *delta* to ``terrain_z`` and sync ``terrain_support_z``.

        If *mask* is given, only the True cells of *delta* are added.
        Layer code should prefer this over direct ``terrain_z +=`` assignment
        so the support invariant is maintained automatically.
        """
        if mask is None:
            self.terrain_z += delta
        else:
            self.terrain_z[mask] += delta[mask]
        self.terrain_support_z[:] = self.terrain_z

    def set_terrain(self, z: np.ndarray) -> None:
        """Replace ``terrain_z`` entirely and sync ``terrain_support_z``.

        *z* must have the same shape as ``terrain_z``.  Use this for
        operations that rewrite the whole heightmap (e.g. water pool shaping).
        """
        self.terrain_z[:] = z
        self.terrain_support_z[:] = self.terrain_z

    # ── Seed derivation (N) ───────────────────────────────────────────────────

    def derive_seed(self, label: str, layer_idx: int = 0) -> int:
        """Return a deterministic 32-bit RNG seed for ``(surface.seed, label, layer_idx)``.

        Delegates to the module-level :func:`derive_seed` free function so the
        same logic is available both on the scene and in helpers that receive
        only a raw integer seed.
        """
        return derive_seed(self.surface.seed, label, layer_idx)

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
