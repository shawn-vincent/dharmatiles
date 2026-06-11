"""Base systems: DungeonBlocks, OpenLOCK, BareSystem.

Each class describes how to scale a tile and attach its base for one
output target.  Pass instances to ``Tile(systems=[...])``.

The default (applied when ``Tile.systems`` is empty)::

    [DungeonBlocks(), OpenLOCK()]

Custom examples::

    Tile(..., systems=[DungeonBlocks()])        # DB only, no OL
    Tile(..., systems=[BareSystem()])           # plain terrain mesh
    Tile(..., systems=[OpenLOCK(square_mm=25.0)])  # metric OL
"""
from __future__ import annotations

import dataclasses
import pathlib

import numpy as np
import trimesh

from .core.config import BaseConfig, SurfaceConfig


class DungeonBlocks:
    """DungeonBlocks socket-peg base, built at the spec's declared square_mm.

    ``peg_height=None`` → auto-select short/tall based on terrain height.
    """

    suffix   = "db"
    dir_name = "dungeonblocks"

    def __init__(self, *, peg_height: float | None = None):
        self.peg_height = peg_height

    def surface_for(self, base_surface: SurfaceConfig) -> SurfaceConfig:
        """Return the surface to build at — DB uses the spec's native scale."""
        return base_surface

    def export(self, tile_mesh: trimesh.Trimesh, surface: SurfaceConfig,
               terrain_z: np.ndarray, output_path: pathlib.Path) -> trimesh.Trimesh:
        from .bases import dungeonblocks
        base_cfg = dataclasses.replace(BaseConfig(), peg_height=self.peg_height)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        return dungeonblocks.export(tile_mesh, surface, base_cfg, terrain_z, output_path)

    def __repr__(self) -> str:
        return f"DungeonBlocks(peg_height={self.peg_height!r})"


class OpenLOCK:
    """OpenLOCK T-slot base, rebuilt at ``square_mm`` (default 25.4 mm/sq).

    ``peg_height=None`` → auto-select.
    """

    suffix   = "ol"
    dir_name = "openlock"

    def __init__(self, *, square_mm: float = 25.4, peg_height: float | None = None):
        self.square_mm  = square_mm
        self.peg_height = peg_height

    def surface_for(self, base_surface: SurfaceConfig) -> SurfaceConfig:
        """Return the surface scaled to this system's square_mm."""
        return dataclasses.replace(base_surface, square_mm=self.square_mm)

    def export(self, tile_mesh: trimesh.Trimesh, surface: SurfaceConfig,
               terrain_z: np.ndarray, output_path: pathlib.Path) -> trimesh.Trimesh:
        from .bases import openlock
        base_cfg = dataclasses.replace(BaseConfig(), peg_height=self.peg_height)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        return openlock.export(tile_mesh, surface, base_cfg, terrain_z, output_path)

    def __repr__(self) -> str:
        return f"OpenLOCK(square_mm={self.square_mm!r}, peg_height={self.peg_height!r})"


class BareSystem:
    """No base — export plain terrain mesh (useful for custom integration).

    The suffix is used both in the filename and as the systems dict key.
    """

    dir_name = "bare"

    def __init__(self, *, suffix: str = "bare"):
        self.suffix = suffix

    def surface_for(self, base_surface: SurfaceConfig) -> SurfaceConfig:
        return base_surface

    def export(self, tile_mesh: trimesh.Trimesh, surface: SurfaceConfig,
               terrain_z: np.ndarray, output_path: pathlib.Path) -> trimesh.Trimesh:
        output_path.parent.mkdir(parents=True, exist_ok=True)
        tile_mesh.export(str(output_path))
        return tile_mesh

    def __repr__(self) -> str:
        return f"BareSystem(suffix={self.suffix!r})"
