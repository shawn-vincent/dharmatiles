"""Tree: a direct tile layer that grows and places trees into a region."""
from __future__ import annotations

import numpy as np
import trimesh

from ..core.grid import sample_grid
from ..core.tile import derive_seed
from ..dist import Sample, bounds, sample
from ..scatter.config import Uniform
from ..scatter.distribute import scatter_positions
from .bark import BarkConfig
from .envelope import CanopyEnvelope


class Tree:
    """Grow and place space-colonisation trees directly in a region layer list.

    ``Tree`` is an independent-instance placer: each tree's skeleton is built
    without knowledge of sibling trees.  Place ``Rocks`` before ``Tree`` in
    ``Region.layers`` so trunks don't land on existing rock footprints.

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

    height_default_mm: float = 5.0

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
        debug_leaf_connectivity: bool = False,
        debug_leaf_color: bool = False,
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
        leaf_width_mm: float = 1.94 * 2.0 / 3.0,
        leaf_thickness_mm: float = 0.16,
        leaf_fold_angle_deg: float = 6.0,
        leaf_inner_curve: float = 1.5,
        leaf_outer_curve: float = 0.15,
        leaf_keel_tip_angle_deg: float = 45.0,
        leaf_spacing_factor: float = 1.1,
        leaf_cap_count: int = 12,
        leaf_angle_jitter_deg: float = 24.0,
        leaf_pos_jitter: float = 0.165,
        leaf_tilt_deg: float = 45.0,       # kept for API compat; unused with branchlets
        bark: BarkConfig | None = None,
        stamp_falloff_mm: float = 5.0,
        # ── Branchlet parameters ──────────────────────────────────────────────
        branchlet_length_mm: float = 12.0,
        branchlet_root_radius_mm: float | None = None,
        branchlet_embed_depth_mm: float = 2.5,
        branchlet_floor_angle_deg: float = 45.0,
    ) -> None:
        self.height_mm             = height_mm
        self.trunk_height_mm       = trunk_height_mm
        self.canopy_radius_mm      = canopy_radius_mm
        self.canopy_base_radius_mm = canopy_base_radius_mm
        self.top_pointiness        = top_pointiness
        self.top_curve             = top_curve
        self.bottom_pointiness     = bottom_pointiness
        self.bottom_curve          = bottom_curve
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
        self.debug_attractors        = bool(debug_attractors)
        self.debug_leaf_connectivity = bool(debug_leaf_connectivity)
        self.debug_leaf_color        = bool(debug_leaf_color)
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
        self.leaf_inner_curve      = float(leaf_inner_curve)
        self.leaf_outer_curve      = float(leaf_outer_curve)
        self.leaf_keel_tip_angle_deg   = float(leaf_keel_tip_angle_deg)
        self.leaf_spacing_factor       = float(leaf_spacing_factor)
        self.leaf_cap_count            = int(leaf_cap_count)
        self.leaf_angle_jitter_deg     = float(leaf_angle_jitter_deg)
        self.leaf_pos_jitter           = float(leaf_pos_jitter)
        self.leaf_tilt_deg             = float(leaf_tilt_deg)
        self.bark = BarkConfig() if bark is None else bark
        self.stamp_falloff_mm = float(stamp_falloff_mm)
        # Branchlet
        self.branchlet_length_mm      = float(branchlet_length_mm)
        self.branchlet_root_radius_mm = (
            None if branchlet_root_radius_mm is None
            else float(branchlet_root_radius_mm)
        )
        self.branchlet_embed_depth_mm = float(branchlet_embed_depth_mm)
        self.branchlet_floor_angle_deg = float(branchlet_floor_angle_deg)

    def footprint_mm(self) -> float:
        return float(bounds(self.canopy_radius_mm)[1])

    def scatter(
        self,
        scene,
        *,
        placement_mask: np.ndarray | None = None,
        layer_idx: int = 0,
    ) -> list[trimesh.Trimesh]:
        from ..core.color import Material
        from .skeleton import grow_skeleton
        from .mesh import build_branch_mesh
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
        branch_parts:  list[trimesh.Trimesh] = []
        foliage_parts: list[trimesh.Trimesh] = []
        other_parts:   list[trimesh.Trimesh] = []
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
            branch_mesh, foliage_mesh, attractor_parts = build_branch_mesh(
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
                leaf_inner_curve=self.leaf_inner_curve,
                leaf_outer_curve=self.leaf_outer_curve,
                leaf_keel_depth_mm=0.0,   # branchlet provides support; no keel needed
                leaf_keel_tip_angle_deg=self.leaf_keel_tip_angle_deg,
                leaf_spacing_factor=self.leaf_spacing_factor,
                leaf_cap_count=self.leaf_cap_count,
                leaf_angle_jitter_deg=self.leaf_angle_jitter_deg,
                leaf_pos_jitter=self.leaf_pos_jitter,
                leaf_tilt_deg=self.leaf_tilt_deg,
                debug_leaf_connectivity=self.debug_leaf_connectivity,
                debug_leaf_color=self.debug_leaf_color,
                branchlet_length_mm=self.branchlet_length_mm,
                branchlet_root_radius_mm=self.branchlet_root_radius_mm,
                branchlet_embed_depth_mm=self.branchlet_embed_depth_mm,
                branchlet_floor_angle_deg=self.branchlet_floor_angle_deg,
            )
            if len(branch_mesh.vertices) == 0 and len(foliage_mesh.vertices) == 0:
                continue

            if len(branch_mesh.vertices) > 0:
                branch_parts.append(branch_mesh)
            if len(foliage_mesh.vertices) > 0:
                foliage_parts.append(foliage_mesh)
            # Attractor spheres are pre-tagged; material grouping in tile.py handles them.
            other_parts.extend(attractor_parts)
            _stamp_tree(scene, surface, x, y, env, float(radii[0]),
                        falloff_mm=self.stamp_falloff_mm)

        if not branch_parts and not foliage_parts and not other_parts:
            return []

        result: list[trimesh.Trimesh] = []
        if branch_parts:
            branch_combined = (trimesh.util.concatenate(branch_parts)
                               if len(branch_parts) > 1 else branch_parts[0])
            branch_combined.metadata['material'] = Material.WOOD
            result.append(branch_combined)

        if foliage_parts:
            foliage_combined = (trimesh.util.concatenate(foliage_parts)
                                if len(foliage_parts) > 1 else foliage_parts[0])
            foliage_combined.metadata['material'] = Material.FOLIAGE
            result.append(foliage_combined)

        result.extend(other_parts)
        return result

    def apply(
        self,
        scene,
        *,
        placement_mask: np.ndarray | None = None,
    ) -> list[trimesh.Trimesh]:
        """``TileLayer`` entry point — delegates to ``scatter()``."""
        return self.scatter(scene, placement_mask=placement_mask)

    def _sample_envelope(
        self, x: float, y: float, terrain_z: float, rng: np.random.Generator
    ) -> CanopyEnvelope:
        height = max(0.0, float(sample(self.height_mm, rng)))
        trunk = float(np.clip(sample(self.trunk_height_mm, rng), 0.0, height))
        canopy_radius = max(0.0, float(sample(self.canopy_radius_mm, rng)))
        canopy_base_radius = max(0.0, float(sample(self.canopy_base_radius_mm, rng)))
        return CanopyEnvelope(
            cx=x, cy=y, terrain_z=terrain_z,
            height_mm=height,
            trunk_height_mm=trunk,
            canopy_radius_mm=canopy_radius,
            canopy_base_radius_mm=canopy_base_radius,
            top_pointiness=self.top_pointiness,
            top_curve=self.top_curve,
            bottom_pointiness=self.bottom_pointiness,
            bottom_curve=self.bottom_curve,
        )



def _stamp_tree(
    scene, surface, x: float, y: float,
    env: CanopyEnvelope, root_radius: float,
    falloff_mm: float = 5.0,
) -> None:
    """Stamp tree footprint into scene support fields.

    Two effects:

    1. **obstacle_mask** — hard no-grass circle of radius
       ``rr = max(root_radius * 1.8, 1.6)`` centred on the trunk.
       Grass is blocked entirely within this footprint.

    2. **terrain_support_z** — exponential falloff from the trunk edge:
       full tree height at ``root_radius``, decaying by a factor of ``e``
       every ``falloff_mm`` outward.  Grass blades close to the trunk can
       therefore grow tall (tufting effect); blades further away return
       naturally to their unobstructed height.

    Setting ``falloff_mm=0`` disables the falloff and reverts to a flat
    ceiling at the obstacle radius only (legacy behaviour).
    """
    rr = max(root_radius * 1.8, 1.6)          # hard obstacle radius
    cw = surface.cell_w
    # Search window: obstacle circle + 3 e-folding lengths of falloff.
    window = rr + 3.0 * falloff_mm
    j0 = max(0, int(np.floor((y - window) / cw)))
    j1 = min(surface.grid_h - 1, int(np.ceil((y + window) / cw)))
    i0 = max(0, int(np.floor((x - window) / cw)))
    i1 = min(surface.grid_w - 1, int(np.ceil((x + window) / cw)))
    jj = np.arange(j0, j1 + 1) * cw
    ii = np.arange(i0, i1 + 1) * cw
    YY, XX = np.meshgrid(jj, ii, indexing='ij')          # (rows, cols) in world coords
    dist = np.sqrt((XX - x) ** 2 + (YY - y) ** 2)        # distance from tree centre
    # Exponential falloff starts at trunk edge (dist = root_radius), decays outward.
    if falloff_mm > 0:
        dist_from_trunk = np.maximum(0.0, dist - root_radius)
        allowed_h = env.height_mm * np.exp(-dist_from_trunk / falloff_mm)
    else:
        # Legacy flat ceiling inside the obstacle circle only.
        allowed_h = np.where(dist <= rr, env.height_mm, 0.0)
    z_top = env.terrain_z + allowed_h
    sl = scene.terrain_support_z[j0:j1 + 1, i0:i1 + 1]
    np.maximum(sl, z_top, out=sl)
    # Hard obstacle mask — only within rr.
    if scene.obstacle_mask is not None:
        scene.obstacle_mask[j0:j1 + 1, i0:i1 + 1] |= (dist <= rr)
