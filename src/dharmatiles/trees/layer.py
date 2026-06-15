"""Scatter-layer integration for CloudTree."""
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


@dataclass(frozen=True)
class TreeShape:
    height_mm: Sample[float] = 40.0
    trunk_height_mm: Sample[float] = 5.0
    crown_radius_mm: Sample[float] = 20.0
    crown_base_radius_mm: Sample[float] = 5.0
    top_pointiness: float = 0.0
    top_curve: float = 1.4
    bottom_pointiness: float = 0.35
    bottom_curve: float = 0.8


class CloudTree:
    """A printable tree grown with space-colonisation branching.

    Invariants (see cloud_skeleton.py):
    - Every attractor is a leaf node; attractors are never branch points.
    - Every branch terminates by landing exactly at an attractor.
    - Branching happens at synthetic interior nodes only.
    - Attractors are sampled across the canopy surface, so branch targets land
      on the crown envelope rather than inside the crown volume.
    - Branch radii are derived bottom-up (pipe model); root radius is
      calculated, not specified.
    - Segments are rendered as C1-continuous cubic Bezier tubes.
    """

    def __init__(
        self,
        *,
        height_mm: Sample[float] = 40.0,
        trunk_height_mm: Sample[float] = 5.0,
        crown_radius_mm: Sample[float] = 20.0,
        crown_base_radius_mm: Sample[float] = 5.0,
        top_pointiness: float = 0.0,
        top_curve: float = 1.4,
        bottom_pointiness: float = 0.35,
        bottom_curve: float = 0.8,
        placement: Uniform | None = None,
        n_attraction: int = 200,
        segment_length_mm: float = 2.0,
        branch_split_angle_deg: float = 30.0,
        max_branches_per_step: int = 3,
        branch_exponent: float = 2.5,
        smoothing_alpha: float = 0.1,
        min_radius_mm: float = 0.45,
        debug_attractors: bool = False,
        group_width_mm: Sample[float] | None = None,
        group_height_mm: Sample[float] | None = None,
    ) -> None:
        self.shape = TreeShape(
            height_mm=height_mm,
            trunk_height_mm=trunk_height_mm,
            crown_radius_mm=crown_radius_mm,
            crown_base_radius_mm=crown_base_radius_mm,
            top_pointiness=top_pointiness,
            top_curve=top_curve,
            bottom_pointiness=bottom_pointiness,
            bottom_curve=bottom_curve,
        )
        self.placement = placement or Uniform(count_per_square=1)
        self.n_attraction = int(n_attraction)
        self.segment_length_mm = float(segment_length_mm)
        self.branch_split_angle_deg = float(branch_split_angle_deg)
        self.max_branches_per_step = int(max_branches_per_step)
        self.branch_exponent = float(branch_exponent)
        self.smoothing_alpha = float(smoothing_alpha)
        self.min_radius_mm = float(min_radius_mm)
        self.debug_attractors = bool(debug_attractors)
        self.group_width_mm  = group_width_mm
        self.group_height_mm = group_height_mm

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
        from .cloud_skeleton import grow_cloud_skeleton
        from .cloud_mesh import build_cloud_tree_mesh

        surface = scene.surface
        rng_seed = (
            derive_seed(surface.seed, "cloud-trees-scatter", layer_idx)
            ^ self.placement.seed
        )
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
        wood_parts: list[trimesh.Trimesh] = []
        flower_parts: list[trimesh.Trimesh] = []
        for x, y, _gd in positions:
            tz = float(sample_grid(scene.terrain_z, surface, np.array([x]), np.array([y]))[0])
            tree_rng = np.random.default_rng(int(rng.integers(2 ** 62)))
            env = self._sample_envelope(x, y, tz, tree_rng)
            # Sample group dimensions per-tree (may be fixed floats or distributions).
            gw = (
                float(sample(self.group_width_mm, tree_rng))
                if self.group_width_mm is not None else None
            )
            gh = (
                float(sample(self.group_height_mm, tree_rng))
                if self.group_height_mm is not None else None
            )
            nodes, parents, radii, in_dirs, out_dirs, attractors = grow_cloud_skeleton(
                env,
                tree_rng,
                n_attraction=self.n_attraction,
                segment_length_mm=self.segment_length_mm,
                min_radius_mm=self.min_radius_mm,
                branch_split_angle_deg=self.branch_split_angle_deg,
                max_branches_per_step=self.max_branches_per_step,
                branch_exponent=self.branch_exponent,
                smoothing_alpha=self.smoothing_alpha,
                group_width_mm=gw,
                group_height_mm=gh,
            )
            if len(nodes) < 2:
                continue
            mesh, attractor_parts = build_cloud_tree_mesh(
                nodes,
                parents,
                radii,
                in_dirs,
                out_dirs,
                terrain_z=tz,
                debug_attractors=attractors if self.debug_attractors else None,
            )
            if len(mesh.vertices) == 0:
                continue
            wood_parts.append(mesh)
            flower_parts.extend(attractor_parts)
            _stamp_tree(scene, surface, x, y, env, float(radii[0]))

        if not wood_parts and not flower_parts:
            return []
        result: list[trimesh.Trimesh] = []
        if wood_parts:
            wood_combined = trimesh.util.concatenate(wood_parts)
            _tag(wood_combined, Material.WOOD)
            result.append(wood_combined)
        if flower_parts:
            flower_combined = trimesh.util.concatenate(flower_parts)
            _tag(flower_combined, Material.FLOWER)
            result.append(flower_combined)
        return result

    def _sample_envelope(
        self, x: float, y: float, terrain_z: float, rng: np.random.Generator
    ) -> TreeEnvelope:
        height = max(0.0, float(sample(self.shape.height_mm, rng)))
        trunk = float(np.clip(sample(self.shape.trunk_height_mm, rng), 0.0, height))
        crown_radius = max(0.0, float(sample(self.shape.crown_radius_mm, rng)))
        crown_base_radius = max(0.0, float(sample(self.shape.crown_base_radius_mm, rng)))
        return TreeEnvelope(
            cx=x, cy=y, terrain_z=terrain_z,
            height_mm=height,
            trunk_height_mm=trunk,
            crown_radius_mm=crown_radius,
            crown_base_radius_mm=crown_base_radius,
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
