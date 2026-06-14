from __future__ import annotations

import numpy as np

from dharmatiles.core.config import ConstTreeConfig
from dharmatiles.trees.const_skeleton import (
    _Candidate,
    _candidates_are_compatible,
    _segment_has_space,
    _segment_segment_distance,
    grow_const_skeleton,
)
from dharmatiles.trees.radii import assign_radii
from dharmatiles.trees.tree import build_tree


def _assert_nonincident_segments_clear(
    nodes: np.ndarray,
    parents: np.ndarray,
    clearance_mm: float,
) -> None:
    edges = [(int(p), i) for i, p in enumerate(parents) if p >= 0]
    for edge_i, (a0_idx, a1_idx) in enumerate(edges):
        for b0_idx, b1_idx in edges[edge_i + 1:]:
            if len({a0_idx, a1_idx, b0_idx, b1_idx}) < 4:
                continue
            if parents[a0_idx] >= 0 and parents[a0_idx] == b0_idx:
                continue
            if parents[b0_idx] >= 0 and parents[b0_idx] == a0_idx:
                continue
            dist = _segment_segment_distance(
                nodes[a0_idx], nodes[a1_idx], nodes[b0_idx], nodes[b1_idx],
            )
            assert dist >= clearance_mm - 1e-6


def test_const_skeleton_rejects_segments_that_cross_occupied_space() -> None:
    nodes = [
        np.array([0.0, 0.0, 0.0]),
        np.array([1.0, 1.0, 0.0]),
        np.array([0.0, 1.0, 0.0]),
    ]
    parents = [-1, 0, 1]

    assert not _segment_has_space(
        2, np.array([1.0, 0.0, 0.0]), nodes, parents, clearance_mm=0.1,
    )
    assert _segment_has_space(
        2, np.array([0.0, 2.0, 0.0]), nodes, parents, clearance_mm=0.1,
    )


def test_const_skeleton_allows_same_fork_candidates_to_diverge() -> None:
    nodes = [
        np.array([0.0, 0.0, 0.0]),
        np.array([-0.2, 0.0, 1.0]),
        np.array([0.2, 0.0, 1.0]),
    ]
    parents = [-1, 0, 0]
    left = _Candidate(
        slot_idx=0,
        tip_idx=1,
        direction=np.array([-1.0, 0.0, 0.0]),
        end_pos=np.array([-1.0, 0.0, 1.3]),
        score=1.0,
    )
    right = _Candidate(
        slot_idx=1,
        tip_idx=2,
        direction=np.array([1.0, 0.0, 0.0]),
        end_pos=np.array([1.0, 0.0, 1.3]),
        score=1.0,
    )
    crowded = _Candidate(
        slot_idx=2,
        tip_idx=2,
        direction=np.array([-1.0, 0.0, 0.0]),
        end_pos=np.array([-0.4, 0.0, 1.3]),
        score=1.0,
    )

    assert _candidates_are_compatible(left, right, nodes, parents, clearance_mm=0.75)
    assert not _candidates_are_compatible(left, crowded, nodes, parents, clearance_mm=0.75)


def test_const_skeleton_binary_split_uses_opposed_child_directions() -> None:
    cfg = ConstTreeConfig(
        height_max_mm=10.0,
        n_trunk_segs=0,
        n_levels=1,
        n_segs_per_level=[1],
        spread_angle_deg=[35.0],
        initial_lean_deg=0.0,
        wander_deg=0.0,
        split_count_min=2,
        split_count_max=2,
        space_clearance_mm=0.0,
    )
    nodes, parents, _arc_dists, _crown_base_z = grow_const_skeleton(
        0.0, 0.0, 0.0, cfg, np.random.default_rng(7),
    )

    child_idxs = np.flatnonzero(parents == 0)
    assert len(child_idxs) == 2
    child_dirs = [nodes[i] - nodes[0] for i in child_idxs]
    child_dirs = [d / np.linalg.norm(d) for d in child_dirs]
    angle = np.degrees(np.arccos(np.clip(np.dot(child_dirs[0], child_dirs[1]), -1.0, 1.0)))

    assert np.isclose(angle, 70.0, atol=1e-6)


def test_const_skeleton_respects_height_and_elevation_floor() -> None:
    cfg = ConstTreeConfig(
        height_max_mm=24.0,
        n_trunk_segs=4,
        n_levels=2,
        n_segs_per_level=[3, 2],
        spread_angle_deg=[35, 20],
        wander_deg=0.0,
        min_elevation_deg=45.0,
        space_clearance_mm=0.5,
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

    _assert_nonincident_segments_clear(nodes, parents, cfg.space_clearance_mm)


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
