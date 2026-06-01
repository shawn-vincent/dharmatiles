"""
TileScene — mutable state accumulated while building a terrain scene.

The scene holds:
  terrain_z  — float heightmap derived from the TerrainGrid (read-only after init)
  support_z  — mutable occupancy surface raised by each layer as it places geometry
  parts      — mesh list accumulated during build (cleared at export)

Configuration lives entirely in SceneConfig sub-configs; TileScene does not
hold configuration itself.

Compatibility shim
------------------
``TileConfig`` is provided as a thin alias for ``SceneConfig`` so existing
call sites continue to work during the transition.  New code should use
``SceneConfig`` directly.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import List

import numpy as np
import trimesh

from .config import SceneConfig, SurfaceConfig, GrassConfig, SolverConfig
from .terrain import (TerrainGrid, TerrainType,
                      terrain_grid_to_heightmap)


# ─────────────────────────────────────────────────────────────────────────────
# Grid coordinate helpers
# ─────────────────────────────────────────────────────────────────────────────

def make_xy_grids(surface: SurfaceConfig):
    """Return (x_grid, y_grid) world-coordinate arrays (grid_h × grid_w)."""
    iy, ix = np.mgrid[0:surface.grid_h, 0:surface.grid_w]
    return (ix * surface.cell_w).astype(float), (iy * surface.cell_h).astype(float)


# ─────────────────────────────────────────────────────────────────────────────
# Scene
# ─────────────────────────────────────────────────────────────────────────────

@dataclass
class TileScene:
    """Mutable state accumulated while building a terrain scene.

    Build with :meth:`from_config` (uses a sinusoidal stand-in terrain) or
    :meth:`from_terrain_grid` (uses a semantic TerrainGrid).

    ``terrain_z`` is fixed at construction.
    ``support_z`` grows as layers rasterise their geometry onto it.
    ``parts`` is the list of Trimesh objects to combine at export.
    """
    config:    SceneConfig
    terrain_z: np.ndarray                       # (grid_h, grid_w) — read-only
    support_z: np.ndarray                       # (grid_h, grid_w) — mutable
    parts:     List[trimesh.Trimesh] = field(default_factory=list)

    # ── Constructors ──────────────────────────────────────────────────────────

    @classmethod
    def from_config(cls, cfg: SceneConfig) -> "TileScene":
        """Initialise with a sinusoidal stand-in terrain heightmap.

        Preserves the old behaviour for scripts that have not yet been
        migrated to TerrainGrid.
        """
        terrain_z = _make_sinusoidal_terrain(cfg.surface)
        return cls(config=cfg, terrain_z=terrain_z,
                   support_z=terrain_z.copy())

    @classmethod
    def from_terrain_grid(cls, cfg: SceneConfig,
                          grid: TerrainGrid) -> "TileScene":
        """Initialise from a semantic TerrainGrid.

        Uses :func:`terrain_grid_to_heightmap` to derive the float heightmap.
        """
        terrain_z = terrain_grid_to_heightmap(grid)
        return cls(config=cfg, terrain_z=terrain_z,
                   support_z=terrain_z.copy())

    # ── Convenience properties ────────────────────────────────────────────────

    @property
    def surface(self) -> SurfaceConfig:
        return self.config.surface

    @property
    def grass(self) -> GrassConfig:
        return self.config.grass

    @property
    def solver(self) -> SolverConfig:
        return self.config.solver


# ─────────────────────────────────────────────────────────────────────────────
# Sinusoidal stand-in terrain
# ─────────────────────────────────────────────────────────────────────────────

def _make_sinusoidal_terrain(surface: SurfaceConfig,
                              amp: float = 1.0,
                              freq: float = 1.5) -> np.ndarray:
    """Build a sinusoidal test heightmap (grid_h × grid_w).

    Stand-in until the semantic TerrainGrid is wired to all entry points.
    Not part of the target architecture.
    """
    x_grid, y_grid = make_xy_grids(surface)
    u = x_grid / surface.tile_w
    v = y_grid / surface.tile_h
    envelope = np.sin(np.pi * u) * np.sin(np.pi * v)
    wave = (np.sin(2 * np.pi * freq * u) *
            np.cos(2 * np.pi * freq * v))
    return (amp * envelope * wave).astype(float)


# ─────────────────────────────────────────────────────────────────────────────
# Compatibility shim — remove after full migration
# ─────────────────────────────────────────────────────────────────────────────

def _make_compat_scene(tile_config) -> TileScene:
    """Build a TileScene from a legacy TileConfig-shaped object.

    Used by the compatibility layer in TileConfig.__new__.
    """
    raise NotImplementedError("Use SceneConfig + TileScene directly.")
