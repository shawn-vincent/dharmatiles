import pytest
import numpy as np

from dharmatiles.trees.cloud_mesh import build_cloud_tree_mesh
from dharmatiles.trees.cloud_skeleton import grow_cloud_skeleton
from dharmatiles.trees.envelope import TreeEnvelope


def _env() -> TreeEnvelope:
    return TreeEnvelope(
        cx=25.0,
        cy=25.0,
        terrain_z=5.0,
        height_mm=40.0,
        trunk_height_mm=5.0,
        crown_radius_mm=18.0,
        crown_base_radius_mm=4.5,
        top_pointiness=0.0,
        top_curve=1.4,
        bottom_pointiness=0.35,
        bottom_curve=0.8,
    )


def test_tree_envelope_radius_endpoints_and_peak() -> None:
    env = _env()
    ts = np.linspace(0.0, 1.0, 257)
    rs = env.radius_at_t(ts)

    assert np.isclose(rs[0], env.crown_base_radius_mm)
    assert rs[-1] == 0.0
    assert np.isclose(float(rs.max()), env.crown_radius_mm)


def test_tree_envelope_top_and_bottom_profiles_join_smoothly() -> None:
    env = _env()
    ts = np.linspace(0.0, 1.0, 1025)
    rs = env.radius_at_t(ts)
    dr = np.gradient(rs, ts)
    peak_i = int(np.argmax(rs))

    assert 0 < peak_i < len(ts) - 1
    assert abs(float(dr[peak_i])) < 1.0


def test_tree_terminal_tips_stay_inside_crown() -> None:
    env = _env()
    nodes, parents, _radii, _in_dirs, _out_dirs, _attractors, _group_labels = grow_cloud_skeleton(
        env,
        np.random.default_rng(123),
    )

    assert len(nodes) > 20
    assert parents[0] == -1
    parent_set = {int(p) for p in parents if p >= 0}
    leaf_nodes = [nodes[i] for i in range(len(nodes)) if i not in parent_set]
    assert all(env.contains(p, margin=1e-6) for p in leaf_nodes)


def test_tree_mesh_is_watertight_and_printable() -> None:
    env = _env()
    nodes, parents, radii, in_dirs, out_dirs, _attractors, _group_labels = grow_cloud_skeleton(
        env,
        np.random.default_rng(456),
    )
    mesh, _attractor_parts = build_cloud_tree_mesh(
        nodes,
        parents,
        radii,
        in_dirs,
        out_dirs,
        terrain_z=env.terrain_z,
    )[:2]

    assert float(radii.min()) >= 0.42
    assert mesh.is_watertight
    assert len(mesh.vertices) > 0


def test_tree_mesh_renders_terminal_branch_when_leaf_clumps_disabled() -> None:
    nodes = np.array(
        [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 5.0],
            [0.0, 0.0, 10.0],
        ]
    )
    parents = np.array([-1, 0, 1])
    radii = np.array([1.0, 0.7, 0.45])
    dirs = np.tile(np.array([[0.0, 0.0, 1.0]]), (3, 1))

    mesh, _attractor_parts, foliage_parts = build_cloud_tree_mesh(
        nodes,
        parents,
        radii,
        dirs,
        dirs,
        terrain_z=0.0,
        foliage_radius_mm=0.0,
    )

    assert foliage_parts == []
    assert np.isclose(float(mesh.vertices[:, 2].max()), 10.0)


def test_tree_mesh_warns_when_branch_angle_is_below_minimum() -> None:
    nodes = np.array(
        [
            [0.0, 0.0, 5.0],
            [0.0, 0.0, 0.0],
        ]
    )
    parents = np.array([-1, 0])
    radii = np.array([1.0, 0.45])
    dirs = np.tile(np.array([[0.0, 0.0, -1.0]]), (2, 1))

    with pytest.warns(RuntimeWarning, match="branch angle below minimum"):
        build_cloud_tree_mesh(
            nodes,
            parents,
            radii,
            dirs,
            dirs,
            terrain_z=0.0,
            min_branch_angle_deg=30.0,
        )
