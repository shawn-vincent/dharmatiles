import numpy as np

from dharmatiles.trees.leaf import _leaf_lobe_profile, compute_leaf_geometry


def _midrib_points(*, arch_deg: float, curl_deg: float) -> np.ndarray:
    geometry = compute_leaf_geometry(
        base_pos=np.zeros(3),
        tangent=np.array([1.0, 0.0, 0.0]),
        length_mm=12.0,
        width_mm=2.0,
        thickness_mm=0.0,
        fold_angle_deg=0.0,
        arch_deg=arch_deg,
        curl_deg=curl_deg,
        up_hint=np.array([0.0, 0.0, 1.0]),
    )
    center_column = geometry.top_pts.shape[1] // 2
    return np.vstack([
        geometry.bp,
        geometry.top_pts[:, center_column, :],
        geometry.v_tip,
    ])


def test_leaf_arch_humps_before_tip_curl() -> None:
    points = _midrib_points(arch_deg=60.0, curl_deg=40.0)
    heights = points[:, 2]
    arch_peak = int(np.argmax(heights))

    assert 5 <= arch_peak <= 8
    assert heights[8] > 0.5 * heights[arch_peak]
    assert heights[-1] > 0.0


def test_leaf_arch_starts_at_the_base() -> None:
    points = _midrib_points(arch_deg=20.0, curl_deg=0.0)
    first_segment = points[1] - points[0]
    first_angle = np.degrees(np.arctan2(first_segment[2], first_segment[0]))

    assert points[1, 2] > 0.0
    assert first_angle > 10.0


def test_leaf_curl_ends_with_tip_pointed_up() -> None:
    points = _midrib_points(arch_deg=0.0, curl_deg=40.0)
    tip_delta = points[-1] - points[-2]

    assert np.allclose(points[:10, 2], 0.0)
    assert tip_delta[2] > 0.0
    assert np.degrees(np.arctan2(tip_delta[2], tip_delta[0])) > 20.0


def test_leaf_default_arch_is_nonzero_when_curl_is_disabled() -> None:
    geometry = compute_leaf_geometry(
        base_pos=np.zeros(3),
        tangent=np.array([1.0, 0.0, 0.0]),
        length_mm=12.0,
        width_mm=2.0,
        thickness_mm=0.0,
        fold_angle_deg=0.0,
        curl_deg=0.0,
        up_hint=np.array([0.0, 0.0, 1.0]),
    )

    center_column = geometry.top_pts.shape[1] // 2
    heights = geometry.top_pts[:, center_column, 2]

    assert float(heights.max()) > 0.0
    assert np.isclose(geometry.v_tip[2], 0.0)


def test_leaf_arch_spans_the_whole_leaf_before_tip_curl() -> None:
    points = _midrib_points(arch_deg=30.0, curl_deg=0.0)

    assert points[1, 2] > 0.0
    assert points[-2, 2] > 0.0
    assert points[len(points) // 2, 2] > points[1, 2]


def test_leaf_width_parameter_is_total_width() -> None:
    geometry = compute_leaf_geometry(
        base_pos=np.zeros(3),
        tangent=np.array([1.0, 0.0, 0.0]),
        length_mm=6.0,
        width_mm=4.0,
        thickness_mm=0.0,
        fold_angle_deg=0.0,
        arch_deg=0.0,
        curl_deg=0.0,
        up_hint=np.array([0.0, 0.0, 1.0]),
    )

    measured_width = np.ptp(geometry.top_pts[:, :, 1], axis=1).max()
    assert np.isclose(measured_width, 4.0)


def test_leaf_inner_and_outer_curves_shape_one_smooth_profile() -> None:
    r = np.linspace(0.0, 1.0, 1001)
    inner_heavy = _leaf_lobe_profile(r, inner_curve=0.9, outer_curve=0.1)
    outer_heavy = _leaf_lobe_profile(r, inner_curve=0.1, outer_curve=0.9)

    assert inner_heavy[0] == 0.0
    assert inner_heavy[-1] == 0.0
    assert inner_heavy[200] > outer_heavy[200]
    assert outer_heavy[800] > inner_heavy[800]

    # A single Bézier polynomial has no internal join or derivative kink.
    slope = np.gradient(inner_heavy, r)
    slope_jump = np.abs(np.diff(slope))
    assert float(slope_jump.max()) < 0.02


def test_leaf_lobe_profile_is_mirrored_at_the_crease() -> None:
    geometry = compute_leaf_geometry(
        base_pos=np.zeros(3),
        tangent=np.array([1.0, 0.0, 0.0]),
        length_mm=6.0,
        width_mm=4.0,
        thickness_mm=0.16,
        fold_angle_deg=0.0,
        inner_curve=0.8,
        outer_curve=0.2,
        arch_deg=0.0,
        curl_deg=0.0,
        up_hint=np.array([0.0, 0.0, 1.0]),
    )

    assert np.allclose(
        geometry.top_pts[:, :, 2],
        geometry.top_pts[:, ::-1, 2],
    )


def test_default_lobe_profile_is_sharp_inside_and_gradual_outside() -> None:
    geometry = compute_leaf_geometry(
        base_pos=np.zeros(3),
        tangent=np.array([1.0, 0.0, 0.0]),
        length_mm=6.0,
        width_mm=4.0,
        thickness_mm=0.16,
        arch_deg=0.0,
        curl_deg=0.0,
        up_hint=np.array([0.0, 0.0, 1.0]),
    )
    row = geometry.top_pts[len(geometry.top_pts) // 4, :, 2]
    center = len(row) // 2

    inner_rise = row[center + 1] - row[center]
    outer_rise = row[-2] - row[-1]
    assert inner_rise > outer_rise


def test_leaf_crease_fades_smoothly_over_full_length() -> None:
    geometry = compute_leaf_geometry(
        base_pos=np.zeros(3),
        tangent=np.array([1.0, 0.0, 0.0]),
        length_mm=6.0,
        width_mm=4.0,
        thickness_mm=0.0,
        arch_deg=0.0,
        curl_deg=0.0,
        up_hint=np.array([0.0, 0.0, 1.0]),
    )
    edge = geometry.top_pts[:, -1, 2]

    assert geometry.bp[2] == 0.0
    assert geometry.v_tip[2] == 0.0
    assert edge[0] < edge[1] < edge[2] < edge[3]
    assert edge[-1] < edge[-2]
