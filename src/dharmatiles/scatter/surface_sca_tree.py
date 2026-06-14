"""
SurfaceScaTree: place trees whose branches terminate on the canopy surface.

The canopy is configured with the same explicit height/radius/profile fields as
``ConstTree``.  The skeleton grower samples terminal points over that surface
and uses them as SCA attractors, producing branchpoints throughout the crown.
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..core.config import SurfaceScaTreeConfig
from ..core.grid import sample_grid
from ..core.tile import derive_seed
from ..dist import bounds as _bounds
from .config import Uniform
from .distribute import scatter_positions


class SurfaceScaTree:
    """Scatter canopy-surface-attractor SCA trees into a region."""

    def __init__(
        self,
        *,
        placement: Uniform | None = None,
        **tree_kwargs,
    ) -> None:
        self.cfg = SurfaceScaTreeConfig(**tree_kwargs)
        self.placement = placement or Uniform(count_per_square=1)

    def footprint_mm(self) -> float:
        """Exclusion radius used by scatter_positions gap checking."""
        return float(_bounds(self.cfg.bark.r_base_mm)[1]) * (1.0 + self.cfg.bark.flare_amp)

    def scatter(
        self,
        scene,
        *,
        placement_mask: np.ndarray | None = None,
        layer_idx: int = 0,
    ) -> list[trimesh.Trimesh]:
        """Build trees, stamp scene arrays, return mesh parts."""
        from ..core.color import Material, tag as _tag
        from ..trees import build_tree, stamp_tree

        surface = scene.surface
        rng_seed = (
            derive_seed(surface.seed, 'surface-sca-trees-scatter', layer_idx)
            ^ self.placement.seed
        )
        rng = np.random.default_rng(rng_seed)

        n_sq = surface.cols * surface.rows
        positions = scatter_positions(
            self.placement, n_sq, self.footprint_mm(),
            placement_mask, scene, surface, rng,
        )
        if not positions:
            return []

        meshes: list[trimesh.Trimesh] = []

        for x, y, _gd in positions:
            tz = float(sample_grid(
                scene.terrain_z, surface,
                np.array([x]), np.array([y]),
            )[0])

            tree_rng = np.random.default_rng(int(rng.integers(2 ** 62)))
            mesh, height_mm = build_tree(x, y, tz, self.cfg, tree_rng)

            if len(mesh.vertices) > 0:
                _tag(mesh, Material.WOOD)
                meshes.append(mesh)

            stamp_tree(
                x, y, tz, height_mm, self.cfg,
                scene.terrain_support_z, scene.obstacle_mask, surface,
            )

        return meshes
