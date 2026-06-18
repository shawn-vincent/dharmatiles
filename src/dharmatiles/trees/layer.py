"""Scatter-layer integration for Tree."""
from __future__ import annotations

from dataclasses import dataclass

import numpy as np
import trimesh

from ..core.grid import sample_grid
from ..core.tile import derive_seed
from ..dist import Sample, bounds, sample
from ..scatter.config import Uniform
from ..scatter.distribute import scatter_positions
from .bark import BarkConfig
from .envelope import CanopyEnvelope


@dataclass(frozen=True)
class TreeShape:
    height_mm: Sample[float] = 40.0
    trunk_height_mm: Sample[float] = 30.0
    canopy_radius_mm: Sample[float] = 15.0
    canopy_base_radius_mm: Sample[float] = 14.0
    top_pointiness: float = 0.0
    top_curve: float = 1.4
    bottom_pointiness: float = 0.35
    bottom_curve: float = 0.8


class Tree:
    """A printable tree grown with space-colonisation branching.

    Invariants (see skeleton.py):
    - Every attractor is a terminal node; attractors are never branch points.
    - Every branch terminates by landing exactly at an attractor.
    - Branching happens at synthetic interior nodes only.
    - Attractors are sampled across the canopy surface, so branch targets land
      on the canopy envelope rather than inside the canopy volume.
    - Branch radii are derived bottom-up (pipe model); root radius is
      calculated, not specified.
    - Segments are rendered as C1-continuous cubic Bezier tubes.
    """

    def __init__(
        self,
        *,
        height_mm: Sample[float] = 40.0,
        trunk_height_mm: Sample[float] = 30.0,
        canopy_radius_mm: Sample[float] = 15.0,
        canopy_base_radius_mm: Sample[float] = 14.0,
        top_pointiness: float = 0.0,
        top_curve: float = 1.4,
        bottom_pointiness: float = 0.35,
        bottom_curve: float = 0.8,
        placement: Uniform | None = None,
        n_attractors: int = 200,
        segment_length_mm: float = 1.0,
        branch_split_angle_deg: float = 30.0,
        target_fdm_angle_deg: float = 35.0,
        strict_fdm_angle_deg: float = 26.0,
        max_branches_per_step: int = 3,
        branch_exponent: float = 3.0,
        smoothing_alpha: float = 0.1,
        min_radius_mm: float = 1.0,
        debug_attractors: bool = False,
        group_width_mm: Sample[float] | None = 20.0,
        group_height_mm: Sample[float] | None = 20.0,
        foliage_bulge_mm: float = 6.0,
        branch_split_eagerness: float = 0.8,
        branch_target: float = 0.33,
        branch_fork_balance: float = 1.0,
        # ── Foliage clusters ──────────────────────────────────────────────
        foliage_clusters: bool = True,
        foliage_cluster_radius_mm: float = 5.5,
        foliage_cluster_length_mm: float | None = 10.5,
        # ── Leaf blades ───────────────────────────────────────────────────
        leaves: bool = True,
        leaf_base_count: int = 5,
        leaf_length_mm: float = 1.94,
        leaf_width_mm: float = 1.21,
        leaf_thickness_mm: float = 0.24,
        leaf_fold_angle_deg: float = 3.0,
        leaf_keel_tip_angle_deg: float = 45.0,
        leaf_spacing_factor: float = 1.1,
        leaf_cap_count: int = 12,
        leaf_angle_jitter_deg: float = 24.0,
        leaf_pos_jitter: float = 0.165,
        bark: BarkConfig | None = None,
    ) -> None:
        self.shape = TreeShape(
            height_mm=height_mm,
            trunk_height_mm=trunk_height_mm,
            canopy_radius_mm=canopy_radius_mm,
            canopy_base_radius_mm=canopy_base_radius_mm,
            top_pointiness=top_pointiness,
            top_curve=top_curve,
            bottom_pointiness=bottom_pointiness,
            bottom_curve=bottom_curve,
        )
        self.placement = placement or Uniform(count_per_square=1)
        self.n_attractors = int(n_attractors)
        self.segment_length_mm = float(segment_length_mm)
        self.branch_split_angle_deg = float(branch_split_angle_deg)
        self.target_fdm_angle_deg = float(target_fdm_angle_deg)
        self.strict_fdm_angle_deg = float(strict_fdm_angle_deg)
        self.max_branches_per_step = int(max_branches_per_step)
        self.branch_exponent = float(branch_exponent)
        self.smoothing_alpha = float(smoothing_alpha)
        self.min_radius_mm = float(min_radius_mm)
        self.debug_attractors  = bool(debug_attractors)
        self.group_width_mm    = group_width_mm
        self.group_height_mm   = group_height_mm
        self.foliage_bulge_mm  = float(foliage_bulge_mm)
        self.branch_split_eagerness = float(np.clip(branch_split_eagerness, 0.0, 1.0))
        self.branch_target          = float(np.clip(branch_target, 0.0, 1.0))
        self.branch_fork_balance    = float(np.clip(branch_fork_balance, 0.0, 1.0))
        # Foliage clusters
        self.foliage_clusters          = bool(foliage_clusters)
        self.foliage_cluster_radius_mm = float(foliage_cluster_radius_mm)
        self.foliage_cluster_length_mm = (
            float(foliage_cluster_length_mm) if foliage_cluster_length_mm is not None else None
        )
        # Leaf blades
        self.leaves                = bool(leaves)
        self.leaf_base_count       = int(leaf_base_count)
        self.leaf_length_mm        = float(leaf_length_mm)
        self.leaf_width_mm         = float(leaf_width_mm)
        self.leaf_thickness_mm     = float(leaf_thickness_mm)
        self.leaf_fold_angle_deg   = float(leaf_fold_angle_deg)
        self.leaf_keel_tip_angle_deg   = float(leaf_keel_tip_angle_deg)
        self.leaf_spacing_factor       = float(leaf_spacing_factor)
        self.leaf_cap_count            = int(leaf_cap_count)
        self.leaf_angle_jitter_deg     = float(leaf_angle_jitter_deg)
        self.leaf_pos_jitter           = float(leaf_pos_jitter)
        self.bark = BarkConfig() if bark is None else bark

    def footprint_mm(self) -> float:
        return float(bounds(self.shape.canopy_radius_mm)[1])

    def scatter(
        self,
        scene,
        *,
        placement_mask: np.ndarray | None = None,
        layer_idx: int = 0,
    ) -> list[trimesh.Trimesh]:
        from ..core.color import Material
        from .skeleton import grow_skeleton
        from .mesh import build_tree_mesh
        surface = scene.surface
        rng_seed = (
            derive_seed(surface.seed, "trees-scatter", layer_idx)
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
        tree_parts:  list[trimesh.Trimesh] = []
        other_parts: list[trimesh.Trimesh] = []
        for x, y, _gd in positions:
            tz = float(sample_grid(scene.terrain_z, surface, np.array([x]), np.array([y]))[0])
            tree_seed = int(rng.integers(2 ** 62))
            tree_rng = np.random.default_rng(tree_seed)
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
            nodes, parents, radii, in_dirs, out_dirs, attractors, group_labels = (
                grow_skeleton(
                    env,
                    tree_rng,
                    n_attractors=self.n_attractors,
                    segment_length_mm=self.segment_length_mm,
                    min_radius_mm=self.min_radius_mm,
                    branch_split_angle_deg=self.branch_split_angle_deg,
                    target_fdm_angle_deg=self.target_fdm_angle_deg,
                    max_branches_per_step=self.max_branches_per_step,
                    branch_exponent=self.branch_exponent,
                    smoothing_alpha=self.smoothing_alpha,
                    group_width_mm=gw,
                    group_height_mm=gh,
                    foliage_bulge_mm=self.foliage_bulge_mm,
                    branch_split_eagerness=self.branch_split_eagerness,
                    branch_target=self.branch_target,
                    branch_fork_balance=self.branch_fork_balance,
                )
            )
            if len(nodes) < 2:
                continue
            mesh, attractor_parts = build_tree_mesh(
                nodes,
                parents,
                radii,
                in_dirs,
                out_dirs,
                terrain_z=tz,
                strict_fdm_angle_deg=self.strict_fdm_angle_deg,
                foliage_cluster_radius_mm=self.foliage_cluster_radius_mm if self.foliage_clusters else 0.0,
                foliage_cluster_length_mm=self.foliage_cluster_length_mm if self.foliage_clusters else None,
                bark=self.bark,
                bark_seed=tree_seed,
                debug_attractors=attractors if self.debug_attractors else None,
                attractor_group_labels=group_labels,
                leaves=self.leaves and self.foliage_clusters,
                leaf_base_count=self.leaf_base_count,
                leaf_length_mm=self.leaf_length_mm,
                leaf_width_mm=self.leaf_width_mm,
                leaf_thickness_mm=self.leaf_thickness_mm,
                leaf_fold_angle_deg=self.leaf_fold_angle_deg,
                leaf_keel_depth_mm=self.leaf_width_mm * 0.83,
                leaf_keel_tip_angle_deg=self.leaf_keel_tip_angle_deg,
                leaf_spacing_factor=self.leaf_spacing_factor,
                leaf_cap_count=self.leaf_cap_count,
                leaf_angle_jitter_deg=self.leaf_angle_jitter_deg,
                leaf_pos_jitter=self.leaf_pos_jitter,
            )
            if len(mesh.vertices) == 0:
                continue

            tree_parts.append(mesh)
            # Attractor spheres are pre-tagged; material grouping in tile.py handles them.
            other_parts.extend(attractor_parts)
            _stamp_tree(scene, surface, x, y, env, float(radii[0]))

        if not tree_parts and not other_parts:
            return []

        result: list[trimesh.Trimesh] = []
        if tree_parts:
            combined = (trimesh.util.concatenate(tree_parts)
                        if len(tree_parts) > 1 else tree_parts[0])
            combined.metadata['material'] = Material.WOOD
            result.append(combined)

        result.extend(other_parts)
        return result

    def _sample_envelope(
        self, x: float, y: float, terrain_z: float, rng: np.random.Generator
    ) -> CanopyEnvelope:
        height = max(0.0, float(sample(self.shape.height_mm, rng)))
        trunk = float(np.clip(sample(self.shape.trunk_height_mm, rng), 0.0, height))
        canopy_radius = max(0.0, float(sample(self.shape.canopy_radius_mm, rng)))
        canopy_base_radius = max(0.0, float(sample(self.shape.canopy_base_radius_mm, rng)))
        return CanopyEnvelope(
            cx=x, cy=y, terrain_z=terrain_z,
            height_mm=height,
            trunk_height_mm=trunk,
            canopy_radius_mm=canopy_radius,
            canopy_base_radius_mm=canopy_base_radius,
            top_pointiness=self.shape.top_pointiness,
            top_curve=self.shape.top_curve,
            bottom_pointiness=self.shape.bottom_pointiness,
            bottom_curve=self.shape.bottom_curve,
        )



def _stamp_tree(scene, surface, x: float, y: float, env: CanopyEnvelope, root_radius: float) -> None:
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
