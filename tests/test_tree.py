import numpy as np

from dharmatiles.trees.envelope import TreeEnvelope
from dharmatiles.trees.mesh import build_tree_mesh
from dharmatiles.trees.radii import assign_radii
from dharmatiles.trees.skeleton import grow_skeleton


def _env() -> TreeEnvelope:
    return TreeEnvelope(
        cx=25.0,
        cy=25.0,
        terrain_z=5.0,
        height_mm=40.0,
        trunk_height_mm=5.0,
        crown_radius_mm=18.0,
        top_pointiness=0.0,
        top_curve=1.4,
        bottom_pointiness=0.35,
        bottom_curve=0.8,
    )


def test_tree_envelope_radius_endpoints_and_peak() -> None:
    env = _env()
    ts = np.linspace(0.0, 1.0, 257)
    rs = env.radius_at_t(ts)

    assert rs[0] == 0.0
    assert rs[-1] == 0.0
    assert np.isclose(float(rs.max()), env.crown_radius_mm)


def test_tree_skeleton_stays_inside_crown_after_trunk() -> None:
    env = _env()
    nodes, parents = grow_skeleton(env, np.random.default_rng(123))

    assert len(nodes) > 20
    assert parents[0] == -1
    crown_nodes = nodes[nodes[:, 2] >= env.crown_base_z - 1e-6]
    assert all(env.contains(p, margin=1e-6) for p in crown_nodes)


def test_tree_mesh_is_watertight_and_printable() -> None:
    env = _env()
    nodes, parents = grow_skeleton(env, np.random.default_rng(456))
    radii = assign_radii(nodes, parents, env.terrain_z, env.height_mm)
    mesh = build_tree_mesh(
        nodes,
        parents,
        radii,
        terrain_z=env.terrain_z,
        trunk_height_mm=env.trunk_height_mm,
    )

    assert float(radii.min()) >= 0.42
    assert mesh.is_watertight
    assert len(mesh.vertices) > 0
