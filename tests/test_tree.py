import pytest
import numpy as np

from dharmatiles.trees.bark import BarkConfig
from dharmatiles.trees.cloud_mesh import (
    _bark_centers_for_ring,
    _bark_cut,
    _bark_surface_noise,
    _foliage_bark_endpoint_maps,
    _foliage_bark_endpoint_t_by_id,
    _root_bark_lines,
    _select_bark_lines,
    build_cloud_tree_mesh,
)
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
    assert mesh.is_volume
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

    mesh, attractor_parts = build_cloud_tree_mesh(
        nodes,
        parents,
        radii,
        dirs,
        dirs,
        terrain_z=0.0,
        foliage_radius_mm=0.0,
    )

    assert attractor_parts == []
    assert np.isclose(float(mesh.vertices[:, 2].max()), 10.0 + radii[-1])
    assert mesh.is_watertight
    assert mesh.is_volume


def test_tree_mesh_warns_when_branch_angle_is_below_strict_fdm_angle() -> None:
    nodes = np.array(
        [
            [0.0, 0.0, 5.0],
            [0.0, 0.0, 0.0],
        ]
    )
    parents = np.array([-1, 0])
    radii = np.array([1.0, 0.45])
    dirs = np.tile(np.array([[0.0, 0.0, -1.0]]), (2, 1))

    with pytest.warns(RuntimeWarning, match="below strict FDM angle"):
        build_cloud_tree_mesh(
            nodes,
            parents,
            radii,
            dirs,
            dirs,
            terrain_z=0.0,
            strict_fdm_angle_deg=30.0,
        )


def test_tree_mesh_unions_continuing_and_diverging_fork() -> None:
    nodes = np.array(
        [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 6.0],
            [0.0, 0.0, 12.0],
            [4.5, 0.0, 10.0],
        ],
        dtype=float,
    )
    parents = np.array([-1, 0, 1, 1])
    radii = np.array([1.2, 0.95, 0.45, 0.45])
    in_dirs = np.array(
        [
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.75, 0.0, 0.66],
        ],
        dtype=float,
    )

    mesh, _attractor_parts = build_cloud_tree_mesh(
        nodes,
        parents,
        radii,
        in_dirs,
        in_dirs,
        terrain_z=0.0,
        foliage_radius_mm=0.0,
    )

    assert mesh.is_watertight
    assert mesh.is_volume


def test_tree_mesh_unions_three_child_fork() -> None:
    nodes = np.array(
        [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 6.0],
            [0.0, 0.0, 12.0],
            [4.0, 0.0, 10.0],
            [-3.5, 2.5, 9.5],
        ],
        dtype=float,
    )
    parents = np.array([-1, 0, 1, 1, 1])
    radii = np.array([1.35, 1.1, 0.45, 0.45, 0.45])
    in_dirs = np.array(
        [
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.7, 0.0, 0.71],
            [-0.56, 0.4, 0.72],
        ],
        dtype=float,
    )

    mesh, _attractor_parts = build_cloud_tree_mesh(
        nodes,
        parents,
        radii,
        in_dirs,
        in_dirs,
        terrain_z=0.0,
        foliage_radius_mm=0.0,
    )

    assert mesh.is_watertight
    assert mesh.is_volume


def test_bark_root_groove_count_uses_circumference_spacing() -> None:
    bark = BarkConfig(spacing_mm=1.35)
    lines = _root_bark_lines(2.0, bark, bark_seed=123)

    assert len(lines) == int(np.floor((2.0 * np.pi * 2.0) / bark.spacing_mm))


def test_default_bark_is_coarse_at_model_scale() -> None:
    bark = BarkConfig()
    lines = _root_bark_lines(2.0, bark, bark_seed=123)

    assert len(lines) == 5
    assert bark.width_mm >= 0.7
    assert bark.depth_mm >= 0.8
    assert bark.roughness_cell_mm >= bark.width_mm


def test_bark_line_selection_thins_evenly_on_small_branches() -> None:
    bark = BarkConfig(spacing_mm=1.35, min_branch_radius_mm=0.58)
    parent = _root_bark_lines(2.0, bark, bark_seed=123)

    selected = _select_bark_lines(parent, 0.75, bark)
    selected_ids = [line.line_id for line in selected]

    assert len(selected) == int(np.floor((2.0 * np.pi * 0.75) / bark.spacing_mm))
    assert selected_ids == sorted(set(selected_ids), key=selected_ids.index)
    assert _select_bark_lines(parent, 0.45, bark) == []


def test_bark_centers_stop_before_foliage_clearance() -> None:
    bark = BarkConfig(foliage_clearance_mm=0.6)
    lines = _root_bark_lines(1.0, bark, bark_seed=123)

    before = _bark_centers_for_ring(
        lines,
        bark,
        radius=1.0,
        s=3.0,
        t=0.39,
        edge_id=1,
        bark_seed=123,
        bark_end_t=0.4,
    )
    after = _bark_centers_for_ring(
        lines,
        bark,
        radius=1.0,
        s=4.1,
        t=0.41,
        edge_id=1,
        bark_seed=123,
        bark_end_t=0.4,
    )

    assert before
    assert after == []


def test_bark_centers_twist_by_tree_height() -> None:
    bark = BarkConfig(wave_amplitude_mm=0.0, twist_rotations=1.25)
    lines = _root_bark_lines(1.0, bark, bark_seed=123)

    base = _bark_centers_for_ring(
        [lines[0]],
        bark,
        radius=1.0,
        s=0.0,
        t=0.0,
        edge_id=1,
        bark_seed=123,
        bark_end_t=1.0,
        z=5.0,
        tree_base_z=5.0,
        tree_height_mm=40.0,
    )
    top = _bark_centers_for_ring(
        [lines[0]],
        bark,
        radius=1.0,
        s=0.0,
        t=1.0,
        edge_id=1,
        bark_seed=123,
        bark_end_t=1.0,
        z=45.0,
        tree_base_z=5.0,
        tree_height_mm=40.0,
    )

    delta = (top[0][1] - base[0][1]) % (2.0 * np.pi)
    assert np.isclose(delta, (2.0 * np.pi * 1.25) % (2.0 * np.pi))


def test_bark_terminating_line_tapers_to_zero_at_endpoint() -> None:
    bark = BarkConfig()
    lines = _root_bark_lines(1.0, bark, bark_seed=123)
    terminating_id = lines[0].line_id

    before = _bark_centers_for_ring(
        lines[:2],
        bark,
        radius=1.0,
        s=4.2,
        t=0.84,
        edge_id=1,
        bark_seed=123,
        bark_end_t=1.0,
        edge_length=5.0,
        end_taper_line_ids={terminating_id},
    )
    at_end = _bark_centers_for_ring(
        lines[:2],
        bark,
        radius=1.0,
        s=5.0,
        t=1.0,
        edge_id=1,
        bark_seed=123,
        bark_end_t=1.0,
        edge_length=5.0,
        end_taper_line_ids={terminating_id},
    )

    before_by_id = {line_id: strength for line_id, _theta, strength in before}
    at_end_ids = {line_id for line_id, _theta, _strength in at_end}
    assert 0.0 < before_by_id[terminating_id] < 1.0
    assert terminating_id not in at_end_ids
    assert lines[1].line_id in at_end_ids


def test_bark_taper_strength_narrows_groove_width() -> None:
    bark = BarkConfig(width_mm=0.42, depth_mm=0.28)

    full_cut = _bark_cut(
        theta_v=0.15,
        radius=1.0,
        bark=bark,
        groove_centers=[(1, 0.0, 1.0)],
    )
    tapered_cut = _bark_cut(
        theta_v=0.15,
        radius=1.0,
        bark=bark,
        groove_centers=[(1, 0.0, 0.5)],
    )

    assert full_cut > 0.0
    assert tapered_cut == 0.0


def test_bark_surface_noise_is_deterministic_and_smaller_than_grooves() -> None:
    bark = BarkConfig(width_mm=0.42, depth_mm=0.28, roughness_amplitude_mm=0.06)
    centers = [(1, 0.0, 1.0)]

    noise = _bark_surface_noise(
        theta_v=0.8,
        radius=1.0,
        bark=bark,
        groove_centers=centers,
        s=2.0,
        edge_id=7,
        bark_seed=123,
    )
    repeat = _bark_surface_noise(
        theta_v=0.8,
        radius=1.0,
        bark=bark,
        groove_centers=centers,
        s=2.0,
        edge_id=7,
        bark_seed=123,
    )
    groove_noise = _bark_surface_noise(
        theta_v=0.0,
        radius=1.0,
        bark=bark,
        groove_centers=centers,
        s=2.0,
        edge_id=7,
        bark_seed=123,
    )

    assert noise == repeat
    assert abs(noise) <= bark.roughness_amplitude_mm * 2.0
    assert abs(noise) < bark.depth_mm
    assert groove_noise == 0.0


def test_bark_continues_onto_first_foliage_segment_with_random_endpoints() -> None:
    bark = BarkConfig(min_branch_radius_mm=0.1)
    lines = _root_bark_lines(1.0, bark, bark_seed=123)[:3]
    base_ts = np.array([0.0, 0.5, 0.75, 1.0])
    end_t_by_id = _foliage_bark_endpoint_t_by_id(
        lines,
        base_ts,
        0.5,
        edge_id=7,
        bark_seed=123,
    )
    assert end_t_by_id is not None
    assert len(set(end_t_by_id.values())) == len(lines)
    assert all(0.5 < end_t < 0.75 for end_t in end_t_by_id.values())

    ts = np.array(sorted(set([*base_ts, *end_t_by_id.values()])))
    arc_s = ts * 8.0
    end_s_by_id, taper_start_s_by_id = _foliage_bark_endpoint_maps(
        end_t_by_id,
        ts,
        arc_s,
        0.5,
    )

    assert end_s_by_id is not None
    assert taper_start_s_by_id is not None
    assert all(4.0 < end_s < 6.0 for end_s in end_s_by_id.values())
    assert set(taper_start_s_by_id.values()) == {4.0}

    first_id = lines[0].line_id
    before_end = _bark_centers_for_ring(
        [lines[0]],
        bark,
        radius=1.0,
        s=(4.0 + end_s_by_id[first_id]) * 0.5,
        t=0.6,
        edge_id=7,
        bark_seed=123,
        bark_end_t=1.0,
        edge_length=8.0,
        line_end_s_by_id=end_s_by_id,
        line_taper_start_s_by_id=taper_start_s_by_id,
    )
    after_end = _bark_centers_for_ring(
        [lines[0]],
        bark,
        radius=1.0,
        s=end_s_by_id[first_id] + 0.01,
        t=0.7,
        edge_id=7,
        bark_seed=123,
        bark_end_t=1.0,
        edge_length=8.0,
        line_end_s_by_id=end_s_by_id,
        line_taper_start_s_by_id=taper_start_s_by_id,
    )

    assert 0.0 < before_end[0][2] < 1.0
    assert after_end == []


def test_tree_mesh_with_bark_extending_onto_foliage_leaf_is_printable() -> None:
    nodes = np.array(
        [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 8.0],
        ],
        dtype=float,
    )
    parents = np.array([-1, 0])
    radii = np.array([1.2, 0.6])
    dirs = np.tile(np.array([[0.0, 0.0, 1.0]]), (2, 1))

    mesh, _attractor_parts = build_cloud_tree_mesh(
        nodes,
        parents,
        radii,
        dirs,
        dirs,
        terrain_z=0.0,
        foliage_radius_mm=2.0,
        leaf_clump_length_mm=None,
        bark=BarkConfig(min_branch_radius_mm=0.1),
        bark_seed=99,
    )

    assert mesh.is_watertight
    assert mesh.is_volume


def test_tree_mesh_with_bark_unions_three_child_fork() -> None:
    nodes = np.array(
        [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 6.0],
            [0.0, 0.0, 12.0],
            [4.0, 0.0, 10.0],
            [-3.5, 2.5, 9.5],
        ],
        dtype=float,
    )
    parents = np.array([-1, 0, 1, 1, 1])
    radii = np.array([1.35, 1.1, 0.45, 0.45, 0.45])
    in_dirs = np.array(
        [
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.7, 0.0, 0.71],
            [-0.56, 0.4, 0.72],
        ],
        dtype=float,
    )

    mesh, _attractor_parts = build_cloud_tree_mesh(
        nodes,
        parents,
        radii,
        in_dirs,
        in_dirs,
        terrain_z=0.0,
        foliage_radius_mm=0.0,
        bark=BarkConfig(),
        bark_seed=99,
    )

    assert mesh.is_watertight
    assert mesh.is_volume
