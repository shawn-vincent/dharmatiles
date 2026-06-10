"""
RockPrototype and GrassPrototype: scatter-layer seed generators.

A *prototype* knows how to:
  1. Report its footprint radius (for spacing calculations).
  2. Create a fully-resolved seed from a position + direction + rng.
  3. Realise a list of pre-sorted seeds into scene geometry.

``sort_priority`` controls global placement order inside ScatterLayer:
  0 — rocks: realised first so support_z / rock_mask are ready for grass.
  1 — grass: realised after all priority-0 seeds are stamped.
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..core.config import RocksConfig, SpeciesConfig, GrassConfig as _RuntimeGrassConfig
from .config import ScatterConfig
from .seed import RockSeed


# ── RockPrototype ─────────────────────────────────────────────────────────────

class RockPrototype:
    """Generates rock seeds from a ``RocksConfig`` and realises them as meshes.

    Default ``ScatterConfig`` matches current rocks behaviour: hard count
    (``items_per_square = rocks.rocks_per_square``), no Voronoi grouping.
    Pass ``ScatterConfig(groups_per_square=N)`` to cluster rocks into N
    Voronoi groups per square — rocks then share a direction hint per group,
    though the mesh builder ignores that direction today.
    """

    sort_priority: int = 0

    def __init__(
        self,
        rocks:   RocksConfig | None  = None,
        scatter: ScatterConfig | None = None,
    ) -> None:
        self.rocks   = rocks or RocksConfig()
        self.scatter = scatter or ScatterConfig(
            items_per_square = self.rocks.rocks_per_square,
            groups_per_square = 0,
            group_dir_mode    = 'none',
        )

    def footprint_mm(self) -> float:
        """Representative footprint radius for spacing calculations (mm)."""
        return self.rocks.r_max

    def make_seed(
        self,
        x: float,
        y: float,
        group_dir: float,          # noqa: ARG002 — ignored for rocks today
        rng: np.random.Generator,
    ) -> RockSeed:
        """Sample geometry from RocksConfig; return a fully-resolved RockSeed."""
        u      = float(rng.uniform(0.0, 1.0)) ** self.rocks.size_power
        rx     = self.rocks.r_min + (self.rocks.r_max - self.rocks.r_min) * u
        ry     = rx * float(rng.uniform(self.rocks.aspect_min, 1.0))
        h_frac = float(rng.uniform(self.rocks.flat_min, self.rocks.flat_max))
        height = 0.5 * (rx + ry) * h_frac
        angle  = float(rng.uniform(0.0, np.pi))
        return RockSeed(x=x, y=y, rx=rx, ry=ry, height=height, angle=angle)

    def realize(
        self,
        seeds: list[RockSeed],
        scene,
        surface,
        *,
        layer_idx:    int                   = 0,
        verbose:      bool                  = False,
        terrain_gz_x: np.ndarray | None     = None,
        terrain_gz_y: np.ndarray | None     = None,
    ) -> list[trimesh.Trimesh]:
        """Build meshes for *seeds* (already sorted big→small) and update scene.

        Stamps each rock's top surface into ``scene.terrain_support_z`` and
        marks the footprint in ``scene.rock_mask``.
        """
        if not seeds:
            return []
        from ..layers.rocks import _build_rocks_mesh_from_seeds
        mesh = _build_rocks_mesh_from_seeds(
            seeds, self.rocks, surface,
            scene.terrain_z, scene.terrain_support_z, scene.rock_mask,
            layer_idx    = layer_idx,
            terrain_gz_x = terrain_gz_x,
            terrain_gz_y = terrain_gz_y,
        )
        return [mesh] if len(mesh.vertices) > 0 else []


# ── GrassPrototype ────────────────────────────────────────────────────────────

class GrassPrototype:
    """Generates 3D grass blades via the existing FloppyGrass pipeline.

    ``sort_priority = 1`` ensures grass seeds are planted after all
    ``RockPrototype`` seeds (priority 0) have fully updated
    ``terrain_support_z`` and ``rock_mask``.

    The ``ScatterConfig`` is stored for documentation/inspection purposes.
    Actual grass density and grouping are driven by ``SpeciesConfig``
    (``groups_per_square``, ``gap_mm``) through the existing Voronoi +
    jitter-grid pipeline inside ``GrassLayer.build()``.
    """

    sort_priority: int = 1

    def __init__(
        self,
        species: SpeciesConfig | None  = None,
        scatter: ScatterConfig | None  = None,
    ) -> None:
        self.species = species or SpeciesConfig()
        self.scatter = scatter or ScatterConfig(
            groups_per_square = self.species.groups_per_square,
            gap_mm            = self.species.gap_mm,
            group_dir_mode    = 'random',
        )

    def footprint_mm(self) -> float:
        """Representative footprint radius for spacing calculations (mm)."""
        return self.species.blade_width_max

    def realize(
        self,
        scene,
        surface,
        *,
        placement_mask:   np.ndarray | None = None,
        layer_seed:       int  | None       = None,
        verbose:          bool               = False,
        max_stack_height: float              = 2.0,
    ) -> list[trimesh.Trimesh]:
        """Grow 3D grass blades and return their mesh list.

        Delegates to ``FloppyGrassLayer.build()`` which internally handles
        Voronoi seeding, segment-by-segment growth, and vegetation support
        rasterisation.  Must be called after all ``RockPrototype`` realise
        calls are complete.
        """
        from ..grass.layer import FloppyGrassLayer

        seed = (layer_seed if layer_seed is not None
                else (surface.seed ^ 0x47524F57))
        grass_cfg = _RuntimeGrassConfig(
            species          = [self.species],
            max_stack_height = max_stack_height,
            seed             = seed,
        )
        layer = FloppyGrassLayer(grass_cfg)

        old_mask = scene.grass_mask
        if placement_mask is not None:
            scene.grass_mask = (old_mask & placement_mask
                                if old_mask is not None else placement_mask)
        try:
            return layer.build(scene, verbose=verbose)
        finally:
            scene.grass_mask = old_mask
