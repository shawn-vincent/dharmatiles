import numpy as np

from dharmatiles.trees.cloud_skeleton import _sample_cloud
from dharmatiles.trees.envelope import TreeEnvelope


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


def test_cloud_attractors_are_on_canopy_surface() -> None:
    env = _env()
    pts = _sample_cloud(env, np.random.default_rng(123), 200)

    assert len(pts) == 200
    assert np.all(pts[:, 2] >= env.crown_base_z)
    assert np.all(pts[:, 2] <= env.crown_top_z)

    center = np.array([env.cx, env.cy])
    xy_r = np.linalg.norm(pts[:, :2] - center, axis=1)
    canopy_r = env.radius_at_z(pts[:, 2])

    assert np.allclose(xy_r, canopy_r, atol=1e-8)


def test_cloud_envelope_round_top_is_not_conical() -> None:
    round_env = _env()
    sharp_env = TreeEnvelope(
        cx=round_env.cx,
        cy=round_env.cy,
        terrain_z=round_env.terrain_z,
        height_mm=round_env.height_mm,
        trunk_height_mm=round_env.trunk_height_mm,
        crown_radius_mm=round_env.crown_radius_mm,
        top_pointiness=1.0,
        top_curve=round_env.top_curve,
        bottom_pointiness=round_env.bottom_pointiness,
        bottom_curve=round_env.bottom_curve,
    )

    near_top_t = 0.99

    assert float(round_env.radius_at_t(near_top_t)) > 3.0 * float(
        sharp_env.radius_at_t(near_top_t)
    )
