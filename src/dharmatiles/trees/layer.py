"""Scatter-layer integration for envelope SCA trees."""
from __future__ import annotations

from dataclasses import dataclass

import numpy as np
import trimesh

from ..core.grid import sample_grid
from ..core.tile import derive_seed
from ..dist import Sample, bounds, sample
from ..scatter.config import Uniform
from ..scatter.distribute import scatter_positions
from .envelope import TreeEnvelope
from .mesh import build_tree_mesh
from .radii import assign_radii
from .skeleton import grow_skeleton


@dataclass(frozen=True)
class TreeShape:
    height_mm: Sample[float] = 40.0
    trunk_height_mm: Sample[float] = 5.0
    crown_radius_mm: Sample[float] = 20.0
    top_pointiness: float = 0.0
    top_curve: float = 1.4
    bottom_pointiness: float = 0.35
    bottom_curve: float = 0.8


class Tree:
    """A printable natural-looking tree grown with space colonization."""

    def __init__(
        self,
        *,
        height_mm: Sample[float] = 40.0,
        trunk_height_mm: Sample[float] = 5.0,
        crown_radius_mm: Sample[float] = 20.0,
        top_pointiness: float = 0.0,
        top_curve: float = 1.4,
        bottom_pointiness: float = 0.35,
        bottom_curve: float = 0.8,
        placement: Uniform | None = None,
    ) -> None:
        self.shape = TreeShape(
            height_mm=height_mm,
            trunk_height_mm=trunk_height_mm,
            crown_radius_mm=crown_radius_mm,
            top_pointiness=top_pointiness,
            top_curve=top_curve,
            bottom_pointiness=bottom_pointiness,
            bottom_curve=bottom_curve,
        )
        self.placement = placement or Uniform(count_per_square=1)

    def footprint_mm(self) -> float:
        return float(bounds(self.shape.crown_radius_mm)[1])

    def scatter(
        self,
        scene,
        *,
        placement_mask: np.ndarray | None = None,
        layer_idx: int = 0,
    ) -> list[trimesh.Trimesh]:
        from ..core.color import Material, tag as _tag

        surface = scene.surface
        rng_seed = derive_seed(surface.seed, "envelope-trees-scatter", layer_idx) ^ self.placement.seed
        rng = np.random.default_rng(rng_seed)

        positions = scatter_positions(
            self.placement,
            surface.cols * surface.rows,
            self.footprint_mm(),
            placement_mask,
            scene,
            surface,
            rng,
        )
        meshes: list[trimesh.Trimesh] = []
        for x, y, _gd in positions:
            tz = float(sample_grid(scene.terrain_z, surface, np.array([x]), np.array([y]))[0])
            tree_rng = np.random.default_rng(int(rng.integers(2 ** 62)))
            env = self._sample_envelope(x, y, tz, tree_rng)
            nodes, parents = grow_skeleton(env, tree_rng)
            if len(nodes) < 2:
                continue
            radii = assign_radii(nodes, parents, env.terrain_z, env.height_mm)
            mesh = build_tree_mesh(
                nodes,
                parents,
                radii,
                terrain_z=env.terrain_z,
                trunk_height_mm=env.trunk_height_mm,
            )
            if len(mesh.vertices) == 0:
                continue
            _tag(mesh, Material.WOOD)
            meshes.append(mesh)
            _stamp_tree(scene, surface, x, y, env, radii[0])

        if not meshes:
            return []
        combined = trimesh.util.concatenate(meshes)
        _tag(combined, Material.WOOD)
        return [combined]

    def _sample_envelope(self, x: float, y: float, terrain_z: float, rng: np.random.Generator) -> TreeEnvelope:
        height = max(0.0, float(sample(self.shape.height_mm, rng)))
        trunk = float(np.clip(sample(self.shape.trunk_height_mm, rng), 0.0, height))
        crown_radius = max(0.0, float(sample(self.shape.crown_radius_mm, rng)))
        return TreeEnvelope(
            cx=x,
            cy=y,
            terrain_z=terrain_z,
            height_mm=height,
            trunk_height_mm=trunk,
            crown_radius_mm=crown_radius,
            top_pointiness=self.shape.top_pointiness,
            top_curve=self.shape.top_curve,
            bottom_pointiness=self.shape.bottom_pointiness,
            bottom_curve=self.shape.bottom_curve,
        )


def _stamp_tree(scene, surface, x: float, y: float, env: TreeEnvelope, root_radius: float) -> None:
    """Block the tree base so later grass grows around it."""
    rr = max(root_radius * 1.8, 1.6)
    cw = surface.cell_w
    j0 = max(0, int(np.floor((y - rr) / cw)))
    j1 = min(surface.grid_h - 1, int(np.ceil((y + rr) / cw)))
    i0 = max(0, int(np.floor((x - rr) / cw)))
    i1 = min(surface.grid_w - 1, int(np.ceil((x + rr) / cw)))
    for j in range(j0, j1 + 1):
        yy = j * cw
        for i in range(i0, i1 + 1):
            xx = i * cw
            if (xx - x) ** 2 + (yy - y) ** 2 <= rr ** 2:
                scene.terrain_support_z[j, i] = max(scene.terrain_support_z[j, i], env.terrain_z + env.height_mm)
                if scene.obstacle_mask is not None:
                    scene.obstacle_mask[j, i] = True
