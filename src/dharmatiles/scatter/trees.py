"""
Trees: a scatter thing that places procedural deciduous trees into a region.

Each tree is a unified SCA skeleton: the trunk emerges naturally as the
attractor-free path below the crown ellipsoid, then branches inside the crown.
All skeleton edges share the same scale-adaptive bark surface.

The class follows the same interface as ``Rocks``, ``Flowers``, and ``Grass``:
``scatter(scene, *, placement_mask, layer_idx)`` → list of trimesh parts.
It stamps ``terrain_support_z`` and ``obstacle_mask`` so subsequent grass
blades steer around the tree base.

Usage in a tile spec::

    from dharmatiles.scatter import Trees
    from dharmatiles.scatter.config import Uniform
    from dharmatiles.layers import Scatter

    Scatter(
        Trees(crown_base_z_mm=D[18:28], placement=Uniform(count_per_square=1)),
        Grass(species=species),
    )
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..core.config import TreeConfig
from ..core.grid import sample_grid
from ..core.tile import derive_seed
from ..dist import bounds as _bounds
from .config import Uniform
from .distribute import scatter_positions


class Trees:
    """Scatter unified deciduous trees (trunk + branches) into a region.

    Parameters
    ----------
    placement : Uniform | None
        Placement strategy.  Default: 1 tree per square.
    **tree_kwargs
        Forwarded to :class:`~dharmatiles.core.config.TreeConfig`.  Any
        ``TreeConfig`` field can be overridden here.  Example::

            Trees(crown_base_z_mm=D[20:30], r_base_mm=D[3:5])
    """

    def __init__(
        self,
        *,
        placement: Uniform | None = None,
        **tree_kwargs,
    ) -> None:
        self.cfg       = TreeConfig(**tree_kwargs)
        self.placement = placement or Uniform(count_per_square=1)

    def footprint_mm(self) -> float:
        """Exclusion radius used by scatter_positions gap checking."""
        return float(_bounds(self.cfg.r_base_mm)[1]) * (1.0 + self.cfg.flare_amp)

    def scatter(
        self,
        scene,
        *,
        placement_mask: np.ndarray | None = None,
        layer_idx:      int               = 0,
    ) -> list[trimesh.Trimesh]:
        """Build trees, stamp scene arrays, return mesh parts."""
        from ..core.color import Material, tag as _tag
        from ..trees import build_tree, stamp_tree

        surface  = scene.surface
        rng_seed = (derive_seed(surface.seed, 'trees-scatter', layer_idx)
                    ^ self.placement.seed)
        rng = np.random.default_rng(rng_seed)

        n_sq      = surface.cols * surface.rows
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

            # Independent sub-RNG per tree so trees don't share entropy
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
