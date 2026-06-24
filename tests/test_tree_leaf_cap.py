import numpy as np

from dharmatiles.trees.mesh import _structured_cap_leaf_angles


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
