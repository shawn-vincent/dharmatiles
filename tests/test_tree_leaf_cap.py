import numpy as np

from dharmatiles.core.color import Material
from dharmatiles.trees.mesh import _build_foliage_cluster_mesh
from dharmatiles.trees.mesh import build_branch_mesh


def test_foliage_cluster_adds_leaves_with_apex_coverage() -> None:
    vertical_clump, vertical_leaves = _build_foliage_cluster_mesh(
        tip_pos=np.array([0.0, 0.0, 10.5]),
        tip_tangent=np.array([0.0, 0.0, 1.0]),
        start_pos=np.array([0.0, 0.0, 0.0]),
        start_tangent=np.array([0.0, 0.0, 1.0]),
        r_wood=0.45,
        r_foliage=5.5,
        clump_length_mm=10.5,
        edge_id=7,
        bark_seed=123,
        leaves=True,
        leaf_length_mm=4.5,
        leaf_width_mm=3.0,
        leaf_curl_deg=20.0,
        leaf_lift_mm=2.5,
        leaf_h_overlap=0.1,
        leaf_v_overlap=0.25,
    )
    tilted_clump, tilted_leaves = _build_foliage_cluster_mesh(
        tip_pos=np.array([6.0, 0.0, 8.5]),
        tip_tangent=np.array([0.65, 0.0, 0.76]),
        start_pos=np.array([0.0, 0.0, 0.0]),
        start_tangent=np.array([0.4, 0.0, 0.92]),
        r_wood=0.45,
        r_foliage=5.5,
        clump_length_mm=10.5,
        edge_id=7,
        bark_seed=123,
        leaves=True,
        leaf_length_mm=4.5,
        leaf_width_mm=3.0,
        leaf_curl_deg=20.0,
        leaf_lift_mm=2.5,
        leaf_h_overlap=0.1,
        leaf_v_overlap=0.25,
    )

    assert len(vertical_leaves) >= 15
    assert len(tilted_leaves) >= 15


def test_foliage_clusters_and_leaves_use_separate_materials() -> None:
    nodes = np.array(
        [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 10.5],
        ],
        dtype=float,
    )
    parents = np.array([-1, 0])
    radii = np.array([0.8, 0.45])
    dirs = np.tile(np.array([[0.0, 0.0, 1.0]]), (2, 1))

    branch_mesh, foliage_mesh, leaf_mesh, attractor_parts = build_branch_mesh(
        nodes,
        parents,
        radii,
        dirs,
        dirs,
        terrain_z=0.0,
        foliage_cluster_radius_mm=5.5,
        foliage_cluster_length_mm=10.5,
        leaves=True,
        leaf_length_mm=4.5,
        leaf_width_mm=3.0,
        leaf_curl_deg=20.0,
        leaf_lift_mm=2.5,
        leaf_h_overlap=0.1,
        leaf_v_overlap=0.25,
        bark_seed=123,
    )

    assert len(branch_mesh.vertices) > 0
    assert len(foliage_mesh.vertices) > 0
    assert len(leaf_mesh.vertices) > 0
    assert attractor_parts == []
    assert foliage_mesh.metadata["material"] == Material.FOLIAGE
    assert leaf_mesh.metadata["material"] == Material.LEAF
