"""
Trees: a scatter thing that places procedural deciduous trees into a region.

Each tree has:
  * A **trunk** — bent spine, elliptical cross-sections, bark texture, optional
    branch stubs.  Built by ``trees/trunk.py``.
  * An optional **branch crown** — space colonization skeleton converted to
    frustum meshes.  Built by ``trees/branches.py``.

The class follows the same interface as ``Rocks``, ``Flowers``, and ``Grass``:
``scatter(scene, *, placement_mask, layer_idx)`` → list of trimesh parts.
It also stamps ``terrain_support_z`` and ``obstacle_mask`` so subsequent grass
blades steer around the trunks.

Usage in a tile spec::

    from dharmatiles.scatter import Trees
    from dharmatiles.scatter.config import Uniform
    from dharmatiles.layers import ScatterLayer

    ScatterLayer(
        Trees(height_mm=D[25:35], placement=Uniform(count_per_square=1)),
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


def _terrain_normal(
    terrain_z: np.ndarray,
    cell_w: float,
    grid_w: int,
    grid_h: int,
    x: float,
    y: float,
) -> np.ndarray:
    """Return the unit terrain normal at world position (x, y).

    Computed from the finite-difference gradient of *terrain_z*.  The result
    is a unit vector pointing away from the terrain surface (upward side).
    """
    ix = int(round(x / cell_w))
    iy = int(round(y / cell_w))
    ix = max(1, min(grid_w - 2, ix))
    iy = max(1, min(grid_h - 2, iy))

    gx = (terrain_z[iy, ix + 1] - terrain_z[iy, ix - 1]) / (2.0 * cell_w)
    gy = (terrain_z[iy + 1, ix] - terrain_z[iy - 1, ix]) / (2.0 * cell_w)

    n  = np.array([-gx, -gy, 1.0])
    nn = np.linalg.norm(n)
    return n / nn if nn > 1e-8 else np.array([0.0, 0.0, 1.0])


class Trees:
    """Scatter deciduous tree trunks (and optional branch crowns) into a region.

    Parameters
    ----------
    placement : Uniform | None
        Placement strategy.  Default: 1 tree per square.
    **tree_kwargs
        Forwarded to :class:`~dharmatiles.core.config.TreeConfig`; any
        ``TreeConfig`` field can be overridden here.  Example::

            Trees(height_mm=D[30:45], grow_branches=False)
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
        """Build trunks + crowns, stamp scene arrays, return mesh parts.

        One ``trimesh.Trimesh`` is returned per mesh type per tree (trunk /
        branches are separate objects so they can carry different material tags).
        """
        from ..core.color import Material, tag as _tag
        from ..trees.trunk    import build_trunk, stamp_trunk
        from ..trees.branches import build_branches

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
            tz    = float(sample_grid(scene.terrain_z, surface,
                                      np.array([x]), np.array([y]))[0])
            angle = float(rng.uniform(0.0, 2.0 * np.pi))

            # Terrain normal at this position: lets the trunk grow tangent to slope
            t_normal = _terrain_normal(
                scene.terrain_z,
                surface.cell_w, surface.grid_w, surface.grid_h,
                x, y,
            )

            # Independent sub-RNG per tree so trees don't share entropy
            tree_rng = np.random.default_rng(int(rng.integers(2 ** 62)))

            trunk_mesh, apex_pos, apex_dir, height_mm, trunk_spine = build_trunk(
                x, y, tz, angle, self.cfg, tree_rng,
                terrain_normal=t_normal,
            )
            if len(trunk_mesh.vertices) > 0:
                _tag(trunk_mesh, Material.WOOD)
                meshes.append(trunk_mesh)

            if self.cfg.grow_branches:
                branch_mesh = build_branches(
                    apex_pos, apex_dir, x, y, tz, height_mm,
                    self.cfg, tree_rng,
                    trunk_spine=trunk_spine,
                )
                if branch_mesh is not None and len(branch_mesh.vertices) > 0:
                    _tag(branch_mesh, Material.WOOD)
                    meshes.append(branch_mesh)

            # Stamp obstacle footprint (trunk base circle up to full height)
            stamp_trunk(
                x, y, tz, self.cfg, height_mm,
                scene.terrain_support_z, scene.obstacle_mask, surface,
            )

        return meshes
