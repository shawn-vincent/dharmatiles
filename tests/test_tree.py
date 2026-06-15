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
    nodes, parents, _radii, _prior_dirs, _attractors = grow_cloud_skeleton(
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
    nodes, parents, radii, prior_dirs, _attractors = grow_cloud_skeleton(
        env,
        np.random.default_rng(456),
    )
    mesh, _attractor_parts = build_cloud_tree_mesh(
        nodes,
        parents,
        radii,
        prior_dirs,
        terrain_z=env.terrain_z,
    )

    assert float(radii.min()) >= 0.42
    assert mesh.is_watertight
    assert len(mesh.vertices) > 0
