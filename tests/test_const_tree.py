from __future__ import annotations

import numpy as np

from dharmatiles.core.config import ConstTreeConfig
from dharmatiles.trees.const_skeleton import grow_const_skeleton
from dharmatiles.trees.radii import assign_radii
from dharmatiles.trees.tree import build_tree


def test_const_skeleton_respects_height_and_elevation_floor() -> None:
    cfg = ConstTreeConfig(
        height_max_mm=24.0,
        n_trunk_segs=4,
        n_levels=2,
        n_segs_per_level=[3, 2],
        spread_angle_deg=[35, 20],
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
    assert crown_base_z > 0.0
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


def test_const_tree_builds_mesh() -> None:
    cfg = ConstTreeConfig(
        height_max_mm=22.0,
        n_trunk_segs=4,
        n_levels=2,
        n_segs_per_level=[3, 2],
        spread_angle_deg=[32, 18],
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
