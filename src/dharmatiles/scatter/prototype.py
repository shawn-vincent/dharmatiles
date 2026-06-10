"""
Rocks and Grass: the things you scatter inside a ``ScatterLayer``.

Each class has one method, ``scatter(scene, *, placement_mask)``, which
samples positions, sorts seeds, builds meshes, and stamps the relevant
scene support fields.  No seed/realise split — one call does it all.
"""
from __future__ import annotations

import dataclasses

import numpy as np
import trimesh

from ..core.config import (RocksConfig, SpeciesConfig,
                           GrassConfig as _RuntimeGrassConfig)
from .config import ScatterConfig
from .seed import RockSeed
from .distribute import scatter_positions


_ROCK_FIELDS    = {f.name for f in dataclasses.fields(RocksConfig)}
_SPECIES_FIELDS = {f.name for f in dataclasses.fields(SpeciesConfig)}


# ── Rocks ─────────────────────────────────────────────────────────────────────

class Rocks:
    """One pass of half-ellipsoid rocks to scatter into a region.

    Flat kwargs build a ``RocksConfig``.  Optional ``scatter=ScatterConfig(...)``
    overrides the default (count-based, no Voronoi grouping).
    """

    def __init__(self, *, scatter: ScatterConfig | None = None, **rocks_kwargs):
        unknown = set(rocks_kwargs) - _ROCK_FIELDS
        if unknown:
            raise TypeError(f"Rocks: unknown kwargs {sorted(unknown)!r}")
        self.rocks = RocksConfig(**rocks_kwargs)
        self.scatter_cfg = scatter or ScatterConfig(
            items_per_square  = self.rocks.rocks_per_square,
            groups_per_square = 0,
            group_dir_mode    = 'none',
        )

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
        surface = scene.config.surface
        rng_seed = (surface.seed
                    ^ 0x726F636B          # "rock"
                    ^ self.scatter_cfg.seed
                    ^ (layer_idx * 65537))
        rng = np.random.default_rng(rng_seed)

        # scatter_positions reads scene.grass_mask via scaled_voronoi_group_count;
        # temporarily set it to this layer's mask so density scaling is correct.
        old_grass_mask = scene.grass_mask
        if placement_mask is not None:
            scene.grass_mask = placement_mask
        try:
            n_sq      = surface.cols * surface.rows
            positions = scatter_positions(
                self.scatter_cfg, n_sq, self.footprint_mm(),
                placement_mask, scene, surface, rng,
            )
        finally:
            scene.grass_mask = old_grass_mask

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

    Pass a ``SpeciesConfig`` (sharable with a companion ``GrassCarpetLayer``)
    plus optional flat overrides to specialise this instance.
    """

    def __init__(
        self,
        species: SpeciesConfig | None = None,
        *,
        scatter: ScatterConfig | None = None,
        max_stack_height: float = 2.0,
        **species_overrides,
    ):
        unknown = set(species_overrides) - _SPECIES_FIELDS
        if unknown:
            raise TypeError(f"Grass: unknown kwargs {sorted(unknown)!r}")
        base = species or SpeciesConfig()
        self.species = (dataclasses.replace(base, **species_overrides)
                        if species_overrides else base)
        self.scatter_cfg = scatter or ScatterConfig(
            groups_per_square = self.species.groups_per_square,
            gap_mm            = self.species.gap_mm,
            group_dir_mode    = 'random',
        )
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

        surface = scene.config.surface
        seed    = (surface.seed
                   ^ 0x47524F57          # "GROW"
                   ^ self.scatter_cfg.seed
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

        old_grass_mask = scene.grass_mask
        if placement_mask is not None:
            scene.grass_mask = (old_grass_mask & placement_mask
                                if old_grass_mask is not None else placement_mask)
        try:
            return layer.build(scene, verbose=verbose)
        finally:
            scene.grass_mask = old_grass_mask
