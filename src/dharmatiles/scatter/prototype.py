"""
Rocks and Grass: the things you scatter inside a ``Scatter`` layer.

Each class has one method, ``scatter(scene, *, placement_mask)``, which
samples positions, sorts seeds, builds meshes, and stamps the relevant
scene support fields.  No seed/realise split — one call does it all.
"""
from __future__ import annotations

import numpy as np
import trimesh

import dataclasses

from ..core.config import (RocksConfig, SpeciesConfig,
                           GrassConfig as _RuntimeGrassConfig)
from .config import Uniform, Grouped
from .seed import RockSeed
from .distribute import scatter_positions


_ROCK_FIELDS = {f.name for f in dataclasses.fields(RocksConfig)}


# ── Rocks ─────────────────────────────────────────────────────────────────────

class Rocks:
    """One pass of half-ellipsoid rocks to scatter into a region.

    Flat kwargs set rock geometry (see ``RocksConfig``).
    Pass ``placement=Uniform(count_per_square=N)`` to control density;
    default is 15 rocks per square.
    """

    def __init__(self, *, placement: Uniform | None = None, **rocks_kwargs):
        unknown = set(rocks_kwargs) - _ROCK_FIELDS
        if unknown:
            raise TypeError(f"Rocks: unknown kwargs {sorted(unknown)!r}")
        self.rocks = RocksConfig(**rocks_kwargs)
        self.placement = placement or Uniform(count_per_square=15)

    def footprint_mm(self) -> float:
        return self.rocks.r_max

    def _make_seed(self, x: float, y: float,
                   rng: np.random.Generator) -> RockSeed:
        u      = float(rng.uniform(0.0, 1.0)) ** self.rocks.size_power
        rx     = self.rocks.r_min + (self.rocks.r_max - self.rocks.r_min) * u
        ry     = rx * float(rng.uniform(self.rocks.aspect_min, 1.0))
        h_frac = float(rng.uniform(self.rocks.flat_min, self.rocks.flat_max))
        height = 0.5 * (rx + ry) * h_frac
        angle  = float(rng.uniform(0.0, np.pi))
        return RockSeed(x=x, y=y, rx=rx, ry=ry, height=height, angle=angle)

    def scatter(
        self,
        scene,
        *,
        placement_mask: np.ndarray | None = None,
        layer_idx:      int               = 0,
    ) -> list[trimesh.Trimesh]:
        """Sample positions, build seeds (sorted big→small), return mesh parts.

        Stamps ``scene.terrain_support_z`` and ``scene.rock_mask`` in place.
        """
        surface = scene.surface
        rng_seed = (surface.seed
                    ^ 0x726F636B          # "rock"
                    ^ self.placement.seed
                    ^ (layer_idx * 65537))
        rng = np.random.default_rng(rng_seed)

        n_sq      = surface.cols * surface.rows
        positions = scatter_positions(
            self.placement, n_sq, self.footprint_mm(),
            placement_mask, scene, surface, rng,
        )

        seeds = [self._make_seed(x, y, rng) for x, y, _gd in positions]
        seeds.sort(key=lambda s: s.sort_key())
        if not seeds:
            return []

        # Pre-compute terrain gradient for slope-aligned rock rotation.
        cw         = surface.cell_w
        terrain_gz_x = np.gradient(scene.terrain_z, axis=1) / cw
        terrain_gz_y = np.gradient(scene.terrain_z, axis=0) / cw

        from ..layers.rocks import _build_rocks_mesh_from_seeds
        mesh = _build_rocks_mesh_from_seeds(
            seeds, self.rocks, surface,
            scene.terrain_z, scene.terrain_support_z, scene.rock_mask,
            layer_idx    = layer_idx,
            terrain_gz_x = terrain_gz_x,
            terrain_gz_y = terrain_gz_y,
        )
        return [mesh] if len(mesh.vertices) > 0 else []


# ── Grass ─────────────────────────────────────────────────────────────────────

class Grass:
    """One species of 3D blades to scatter into a region.

    Pass a ``SpeciesConfig`` (sharable with a companion ``GrassCarpet``)
    to specify blade geometry.  Pass ``placement=Grouped(...)`` to control
    grouping and density; default is 3 Voronoi groups per square.
    """

    def __init__(
        self,
        species: SpeciesConfig | None = None,
        *,
        placement: Grouped | None = None,
        max_stack_height: float = 2.0,
    ):
        self.species = species or SpeciesConfig()
        self.placement = placement or Grouped()
        self.max_stack_height = max_stack_height

    def footprint_mm(self) -> float:
        return self.species.blade_width_max

    def scatter(
        self,
        scene,
        *,
        placement_mask: np.ndarray | None = None,
        layer_idx:      int               = 0,
        verbose:        bool              = False,
    ) -> list[trimesh.Trimesh]:
        """Plant + grow blades inside *placement_mask*, return mesh parts.

        Reads ``scene.terrain_support_z`` (populated by any prior Rocks)
        and stamps ``scene.vegetation_support_z`` as blades grow.
        """
        from ..grass.layer import FloppyGrassLayer

        surface = scene.surface
        seed    = (surface.seed
                   ^ 0x47524F57          # "GROW"
                   ^ self.placement.seed
                   ^ (layer_idx * 65537))
        grass_cfg = _RuntimeGrassConfig(
            species          = [self.species],
            max_stack_height = self.max_stack_height,
            seed             = seed,
        )
        layer = FloppyGrassLayer(grass_cfg)

        # Lift vegetation_support_z to include rock tops (and any other
        # terrain support stamped by prior layers) so the grower's
        # leading-edge sampler and the seed-depth check both see them.
        np.maximum(scene.vegetation_support_z, scene.terrain_support_z,
                   out=scene.vegetation_support_z)

        return layer.build(scene, verbose=verbose, placement_mask=placement_mask,
                           placement=self.placement)
