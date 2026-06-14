from __future__ import annotations

import numpy as np

from dharmatiles.core.config import ConstTreeConfig
from dharmatiles.core.config import SurfaceScaTreeConfig
from dharmatiles.trees.const_skeleton import (
    _crown_profile,
    grow_const_skeleton,
)
from dharmatiles.trees.surface_skeleton import grow_surface_skeleton
from dharmatiles.trees.radii import assign_radii
from dharmatiles.trees.surface import (
    _build_children,
    _extract_runs,
    _run_endpoint_tangents,
    _run_start_tangent_overrides,
)
from dharmatiles.trees.tree import build_tree


def test_crown_profile_uses_independent_top_and_bottom_shapes() -> None:
    round_bottom = _crown_profile(
        0.20,
        bottom_pointiness=0.0,
        bottom_curve=0.8,
        top_pointiness=0.8,
        top_curve=1.4,
    )
    pointed_bottom = _crown_profile(
        0.20,
        bottom_pointiness=1.0,
        bottom_curve=0.8,
        top_pointiness=0.8,
        top_curve=1.4,
    )
    round_top = _crown_profile(
        0.80,
        bottom_pointiness=0.3,
        bottom_curve=0.8,
        top_pointiness=0.0,
        top_curve=1.4,
    )
    pointed_top = _crown_profile(
        0.80,
        bottom_pointiness=0.3,
        bottom_curve=0.8,
        top_pointiness=1.0,
        top_curve=1.4,
    )

    assert round_bottom > pointed_bottom
    assert round_top > pointed_top
    assert _crown_profile(0.0, 0.3, 0.8, 0.8, 1.4) == 0.0
    assert _crown_profile(1.0, 0.3, 0.8, 0.8, 1.4) == 0.0


def test_const_skeleton_respects_height_and_elevation_floor() -> None:
    cfg = ConstTreeConfig(
        height_max_mm=24.0,
        trunk_height_mm=6.0,
        n_trunk_segs=4,
        n_levels=2,
        wander_deg=0.0,
        min_elevation_deg=45.0,
        split_count_min=2,
        split_count_max=2,
    )
    nodes, parents, arc_dists, crown_base_z = grow_const_skeleton(
        10.0, 12.0, 2.0, cfg, np.random.default_rng(123),
    )

    assert len(nodes) > 1
    assert np.all(parents[1:] >= 0)
    assert nodes[:, 2].max() <= 26.0 + 1e-6
    assert np.isclose(crown_base_z, 8.0)
    for i in range(1, len(nodes)):
        p = int(parents[i])
        assert np.isclose(
            arc_dists[i],
            arc_dists[p] + np.linalg.norm(nodes[i] - nodes[p]),
        )

    min_z = np.sin(np.radians(cfg.min_elevation_deg))
    for i in range(1, len(nodes)):
        p = int(parents[i])
        d = nodes[i] - nodes[p]
        assert d[2] / np.linalg.norm(d) >= min_z - 1e-6


def test_const_tree_radius_runs_are_constant_without_taper() -> None:
    # One-child run 0 -> 1 -> 2, then a binary split at 2.
    parents = np.array([-1, 0, 1, 2, 2], dtype=int)
    radii = assign_radii(
        parents,
        r_tip_mm=0.5,
        r_root_mm=2.0,
        exp=2.0,
        include_internal_self=False,
    )

    assert radii[1] == radii[2]
    assert np.isclose(radii[2], np.sqrt(0.5 ** 2 + 0.5 ** 2))
    assert radii[0] == 2.0


def test_branch_run_starts_continue_parent_endpoint_tangent() -> None:
    nodes = np.array([
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 2.0],
        [-1.0, 0.0, 2.0],
    ])
    parents = np.array([-1, 0, 1, 1], dtype=int)
    runs = _extract_runs(_build_children(parents))
    start_tangents = _run_start_tangent_overrides(nodes, parents, runs)

    assert np.allclose(start_tangents[1], [0.0, 0.0, 1.0])
    t0, _ = _run_endpoint_tangents(nodes[[1, 2]], start_tangents[1])

    assert np.allclose(t0, [0.0, 0.0, 1.0])


def test_const_tree_builds_mesh() -> None:
    cfg = ConstTreeConfig(
        height_max_mm=22.0,
        trunk_height_mm=6.0,
        n_trunk_segs=4,
        n_levels=2,
        split_count_min=2,
        split_count_max=2,
        r_base_mm=2.5,
        ridge_amp=0.0,
        wrinkle_amp=0.0,
    )
    mesh, height = build_tree(10.0, 10.0, 0.0, cfg, np.random.default_rng(456))

    assert len(mesh.vertices) > 0
    assert len(mesh.faces) > 0
    assert 0.0 < height <= 22.0


def test_surface_sca_tree_ends_branches_on_canopy_surface() -> None:
    cfg = SurfaceScaTreeConfig(
        height_max_mm=24.0,
        trunk_height_mm=6.0,
        n_trunk_segs=4,
        crown_radius_mm=10.0,
        top_pointiness=0.75,
        top_curve=1.4,
        bottom_pointiness=0.35,
        bottom_curve=0.8,
        surface_point_spacing_mm=5.0,
        surface_branch_segment_mm=2.0,
        surface_branch_lift_mm=0.5,
        r_base_mm=2.5,
        ridge_amp=0.0,
        wrinkle_amp=0.0,
    )
    rng = np.random.default_rng(789)
    nodes, parents, arc_dists, crown_base_z = grow_surface_skeleton(10.0, 12.0, 1.0, cfg, rng)

    assert len(nodes) > int(cfg.n_trunk_segs) + 1
    assert np.isclose(crown_base_z, 7.0)
    assert np.all(parents[1:] >= 0)

    child_count = np.zeros(len(nodes), dtype=int)
    for i in range(1, len(nodes)):
        child_count[int(parents[i])] += 1
        assert np.isclose(
            arc_dists[i],
            arc_dists[int(parents[i])] + np.linalg.norm(nodes[i] - nodes[int(parents[i])]),
        )

    leaves = np.flatnonzero(child_count == 0)
    assert len(leaves) >= 8
    assert nodes[leaves, 2].min() >= crown_base_z - 1e-6
    branchpoints = np.flatnonzero(child_count > 1)
    assert np.count_nonzero(nodes[branchpoints, 2] > crown_base_z + 2.0) >= 2

    mesh, height = build_tree(10.0, 12.0, 1.0, cfg, np.random.default_rng(789))
    assert len(mesh.vertices) > 0
    assert len(mesh.faces) > 0
    assert 0.0 < height <= 24.0 + cfg.surface_branch_lift_mm
