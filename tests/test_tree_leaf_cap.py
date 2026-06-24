import numpy as np
import trimesh

from dharmatiles.core.color import Material
from dharmatiles.trees.mesh import _build_foliage_cluster_mesh
from dharmatiles.trees.mesh import _structured_cap_leaf_angles
from dharmatiles.trees.mesh import _structured_world_top_leaf_angles
from dharmatiles.trees.mesh import build_branch_mesh


def _sphere_points(angles: list[tuple[int, float, float]], radius: float) -> np.ndarray:
    pts = []
    for _idx, phi, theta in angles:
        rr = radius * np.cos(phi)
        pts.append([
            radius * np.sin(phi),
            rr * np.cos(theta),
            rr * np.sin(theta),
        ])
    return np.asarray(pts, dtype=float)


def _top_view_leaf_coverage(
    clump: trimesh.Trimesh,
    leaves: list[trimesh.Trimesh],
    *,
    samples: int = 50,
) -> tuple[float, int]:
    leaf_mesh = trimesh.util.concatenate(leaves)
    mins, maxs = clump.bounds
    xs = np.linspace(mins[0] - 0.5, maxs[0] + 0.5, samples)
    ys = np.linspace(mins[1] - 0.5, maxs[1] + 0.5, samples)
    xx, yy = np.meshgrid(xs, ys)
    origins = np.column_stack([
        xx.ravel(),
        yy.ravel(),
        np.full(xx.size, maxs[2] + 20.0),
    ])
    directions = np.tile(np.array([0.0, 0.0, -1.0]), (len(origins), 1))

    hits = []
    for mesh in (clump, leaf_mesh):
        z_hits = np.full(len(origins), np.nan)
        locations, ray_indices, _tri_indices = mesh.ray.intersects_location(
            origins, directions, multiple_hits=True,
        )
        for ray_i, z in zip(ray_indices, locations[:, 2]):
            if np.isnan(z_hits[ray_i]) or z > z_hits[ray_i]:
                z_hits[ray_i] = z
        hits.append(z_hits)

    clump_z, leaf_z = hits
    clump_mask = ~np.isnan(clump_z)
    leaf_visible = clump_mask & ~np.isnan(leaf_z) & (leaf_z >= clump_z - 0.05)
    bare = clump_mask & ~leaf_visible

    bare_grid = bare.reshape(xx.shape)
    seen = np.zeros_like(bare_grid, dtype=bool)
    largest_bare = 0
    for row in range(bare_grid.shape[0]):
        for col in range(bare_grid.shape[1]):
            if not bare_grid[row, col] or seen[row, col]:
                continue
            stack = [(row, col)]
            seen[row, col] = True
            component = 0
            while stack:
                y, x = stack.pop()
                component += 1
                for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    ny, nx = y + dy, x + dx
                    if (
                        0 <= ny < bare_grid.shape[0]
                        and 0 <= nx < bare_grid.shape[1]
                        and bare_grid[ny, nx]
                        and not seen[ny, nx]
                    ):
                        seen[ny, nx] = True
                        stack.append((ny, nx))
            largest_bare = max(largest_bare, component)

    return float(leaf_visible.sum() / clump_mask.sum()), largest_bare


def test_leaf_cap_positions_are_structured_not_random_clusters() -> None:
    radius = 5.5
    angles = _structured_cap_leaf_angles(
        12,
        pos_jitter=0.075,
        row_step=4.5 * (1.0 - 0.25),
        col_step=3.0 * (1.0 - 0.10),
        r_tip=radius,
        bark_seed=123,
        edge_id=7,
    )

    assert len(angles) == 12

    points = _sphere_points(angles, radius)
    deltas = points[:, None, :] - points[None, :, :]
    distances = np.linalg.norm(deltas, axis=2)
    distances[distances == 0.0] = np.inf

    assert float(distances.min()) > 0.75


def test_world_top_leaf_angles_follow_world_up_crest() -> None:
    angles = _structured_world_top_leaf_angles(
        12,
        arc_lo=1.0,
        arc_hi=9.0,
        cone_end_arc=6.0,
        south_arc=0.5,
        cone_arc_p=5.5,
        north_arc=3.0,
        r_tip=5.5,
        row_step=3.375,
        col_step=2.7,
        pos_jitter=0.075,
        bark_seed=123,
        edge_id=7,
        top_theta_at_arc=lambda _arc: (np.pi / 2.0, 1.0),
        z_at_arc_theta=lambda arc, theta: arc + 0.5 * np.sin(theta),
    )

    assert len(angles) == 12
    assert len({round(arc, 3) for _idx, arc, _theta in angles}) > 1

    wrapped_delta = [
        abs(np.arctan2(np.sin(theta - np.pi / 2.0), np.cos(theta - np.pi / 2.0)))
        for _idx, _arc, theta in angles
    ]
    assert max(wrapped_delta) < 1.25


def test_foliage_cluster_adds_dome_tip_and_world_top_leaves() -> None:
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
        leaf_base_count=5,
        leaf_length_mm=4.5,
        leaf_width_mm=3.0,
        leaf_curl_deg=20.0,
        leaf_lift_mm=2.5,
        leaf_h_overlap=0.1,
        leaf_v_overlap=0.25,
        leaf_cap_count=12,
        leaf_angle_jitter_deg=5.0,
        leaf_pos_jitter=0.075,
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
        leaf_base_count=5,
        leaf_length_mm=4.5,
        leaf_width_mm=3.0,
        leaf_curl_deg=20.0,
        leaf_lift_mm=2.5,
        leaf_h_overlap=0.1,
        leaf_v_overlap=0.25,
        leaf_cap_count=12,
        leaf_angle_jitter_deg=5.0,
        leaf_pos_jitter=0.075,
    )

    assert len(vertical_leaves) >= 240
    assert len(tilted_leaves) >= 240

    vertical_coverage, vertical_bare = _top_view_leaf_coverage(vertical_clump, vertical_leaves)
    tilted_coverage, tilted_bare = _top_view_leaf_coverage(tilted_clump, tilted_leaves)

    assert vertical_coverage >= 0.90
    assert tilted_coverage >= 0.93
    assert vertical_bare <= 100
    assert tilted_bare <= 100


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
        leaf_base_count=5,
        leaf_length_mm=4.5,
        leaf_width_mm=3.0,
        leaf_curl_deg=20.0,
        leaf_lift_mm=2.5,
        leaf_h_overlap=0.1,
        leaf_v_overlap=0.25,
        leaf_cap_count=12,
        leaf_angle_jitter_deg=5.0,
        leaf_pos_jitter=0.075,
        bark_seed=123,
    )

    assert len(branch_mesh.vertices) > 0
    assert len(foliage_mesh.vertices) > 0
    assert len(leaf_mesh.vertices) > 0
    assert attractor_parts == []
    assert foliage_mesh.metadata["material"] == Material.FOLIAGE
    assert leaf_mesh.metadata["material"] == Material.LEAF
