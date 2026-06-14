"""
Canopy-surface endpoint skeleton growth.

This variant uses the constructive crown profile as a target surface instead of
filling crown volumes or cross-sections.  Terminal points are distributed over
the surface of that canopy, then used as SCA attractors.  Active tips compete
for visible attractors and split repeatedly through the crown until the branch
structure reaches the sampled surface.
"""
from __future__ import annotations

import numpy as np

from ..dist import sample
from .const_skeleton import (
    _UP,
    _append_node_at,
    _clamp_elevation,
    _crown_profile,
    _normalize,
    _wander_direction,
)
from .skeleton import _compute_arc_dists


_GOLDEN_ANGLE = np.pi * (3.0 - np.sqrt(5.0))


def _profile_radius(t: float, crown_radius: float, cfg) -> float:
    return crown_radius * _crown_profile(
        t,
        float(cfg.bottom_pointiness),
        float(cfg.bottom_curve),
        float(cfg.top_pointiness),
        float(cfg.top_curve),
    )


def _sample_canopy_surface_points(
    cx: float,
    cy: float,
    crown_base_z: float,
    crown_height: float,
    crown_radius: float,
    spacing_mm: float,
    cfg,
    rng: np.random.Generator,
) -> np.ndarray:
    """Return approximately evenly spaced points on the crown surface."""
    if crown_height <= 1e-8 or crown_radius <= 1e-8:
        return np.empty((0, 3), dtype=float)

    spacing = max(0.5, float(spacing_mm))
    samples = 513
    t_grid = np.linspace(0.0, 1.0, samples)
    r_grid = np.array([_profile_radius(float(t), crown_radius, cfg) for t in t_grid])
    z_grid = crown_base_z + t_grid * crown_height

    dr = np.diff(r_grid)
    dz = np.diff(z_grid)
    meridian = np.concatenate([[0.0], np.cumsum(np.sqrt(dr * dr + dz * dz))])
    total_meridian = float(meridian[-1])
    if total_meridian <= 1e-8:
        return np.empty((0, 3), dtype=float)

    phase0 = float(rng.uniform(0.0, 2.0 * np.pi))
    points: list[np.ndarray] = []

    # Half-step offset avoids putting a large ring exactly on the crown's
    # pinched endpoints while still covering the lower and upper surface.
    ring_s = np.arange(spacing * 0.5, total_meridian, spacing)
    if len(ring_s) == 0:
        ring_s = np.array([total_meridian * 0.5])

    for ring_idx, s in enumerate(ring_s):
        t = float(np.interp(s, meridian, t_grid))
        r = _profile_radius(t, crown_radius, cfg)
        z = crown_base_z + t * crown_height
        if r <= 1e-6:
            points.append(np.array([cx, cy, z], dtype=float))
            continue

        n_ring = max(1, int(round(2.0 * np.pi * r / spacing)))
        phase = phase0 + ring_idx * _GOLDEN_ANGLE
        for k in range(n_ring):
            a = phase + 2.0 * np.pi * k / n_ring
            points.append(np.array([
                cx + r * np.cos(a),
                cy + r * np.sin(a),
                z,
            ], dtype=float))

    return np.array(points, dtype=float)


def _sample_canopy_interior_points(
    trunk_tip: np.ndarray,
    surface_points: np.ndarray,
    crown_base_z: float,
    rng: np.random.Generator,
) -> np.ndarray:
    """Return one interior attractor for each canopy-surface endpoint.

    Surface-only targets make the SCA skeleton rush to the shell and leave the
    middle of the crown empty.  Pairing each endpoint with one point along the
    trunk-tip-to-endpoint ray doubles the attractor count and pulls secondary
    growth through the crown volume before terminal twigs reach the surface.
    """
    if len(surface_points) == 0:
        return np.empty((0, 3), dtype=float)

    t = rng.uniform(0.35, 0.78, len(surface_points))[:, None]
    pts = trunk_tip[None, :] + (surface_points - trunk_tip[None, :]) * t
    pts[:, 2] = np.maximum(pts[:, 2], crown_base_z)
    return pts


def _split_attractor_groups(
    dirs_n: np.ndarray,
    min_group_size: int,
    xy_std_threshold: float,
) -> list[np.ndarray]:
    """Split a tip's visible attractors into SCA child groups.

    Classic SCA grows one child per active tip.  For this miniature tree scale
    we allow a visible attractor set with clear XY spread to split into two
    child directions at the same iteration; repeated iterations then produce
    higher-order branchpoints throughout the crown.
    """
    n_local = len(dirs_n)
    if n_local < 2 * min_group_size:
        return [np.ones(n_local, dtype=bool)]

    xy_std = float(dirs_n[:, :2].std())
    if xy_std <= xy_std_threshold:
        return [np.ones(n_local, dtype=bool)]

    x_std = float(dirs_n[:, 0].std())
    y_std = float(dirs_n[:, 1].std())
    col = 0 if x_std >= y_std else 1
    split = float(np.median(dirs_n[:, col]))

    mask_a = dirs_n[:, col] >= split
    mask_b = ~mask_a
    if np.sum(mask_a) < min_group_size or np.sum(mask_b) < min_group_size:
        return [np.ones(n_local, dtype=bool)]
    return [mask_a, mask_b]


def _grow_surface_sca(
    trunk_tip: int,
    attractors: np.ndarray,
    terminal_points: np.ndarray,
    seg_len_mm: float,
    min_elevation_deg: float,
    nodes: list[np.ndarray],
    dirs: list[np.ndarray],
    parents: list[int],
    cfg,
) -> None:
    """Grow a real SCA skeleton from *trunk_tip* toward surface attractors."""
    if len(attractors) == 0:
        return

    att = attractors.copy()
    tips: set[int] = {trunk_tip}
    segment = max(0.5, float(seg_len_mm))
    spacing = max(segment, float(getattr(cfg, "surface_point_spacing_mm", 4.5)))
    perception_cfg = getattr(cfg, "surface_sca_perception_mm", None)
    kill_cfg = getattr(cfg, "surface_sca_kill_mm", None)
    perception = float(perception_cfg if perception_cfg is not None else spacing * 1.8)
    kill_r = float(kill_cfg if kill_cfg is not None else max(segment * 1.25, spacing * 0.35))
    max_steps = int(getattr(cfg, "surface_sca_max_steps", 160))
    tropism = np.array(
        [0.0, 0.0, float(getattr(cfg, "surface_sca_tropism", 0.12))],
        dtype=float,
    )
    xy_std_threshold = float(getattr(cfg, "surface_sca_branch_xy_std", 0.26))
    min_branch_points = max(1, int(getattr(cfg, "surface_sca_min_branch_points", 3)))
    target_bias = max(0.0, float(getattr(cfg, "surface_sca_target_bias", 0.55)))
    tangent_bias = max(0.0, float(getattr(cfg, "surface_sca_tangent_bias", 0.30)))
    trunk_tip_pos = nodes[trunk_tip].copy()

    for _ in range(max_steps):
        if len(att) == 0 or not tips:
            break

        tip_list = sorted(tips)
        tip_arr = np.array([nodes[i] for i in tip_list], dtype=float)
        tip_kill_diff = att[:, None, :] - tip_arr[None, :, :]
        tip_kill_d2 = (tip_kill_diff * tip_kill_diff).sum(axis=-1)
        tip_nearest = np.argmin(tip_kill_d2, axis=1)
        tip_killed = np.min(tip_kill_d2, axis=1) <= kill_r * kill_r
        if np.any(tip_killed):
            for att_idx in np.flatnonzero(tip_killed):
                parent_idx = tip_list[int(tip_nearest[att_idx])]
                target = att[int(att_idx)]
                is_terminal = (
                    len(terminal_points) > 0
                    and np.any(np.linalg.norm(terminal_points - target[None, :], axis=1) < 1e-6)
                )
                if is_terminal and np.linalg.norm(target - nodes[parent_idx]) > 1e-8:
                    _append_node_at(parent_idx, target, nodes, dirs, parents)
            att = att[~tip_killed]
            if len(att) == 0:
                break

        tip_list = sorted(tips)
        tip_arr = np.array([nodes[i] for i in tip_list], dtype=float)
        diff = att[:, None, :] - tip_arr[None, :, :]
        dist2 = (diff * diff).sum(axis=-1)
        nearest_slot = np.argmin(dist2, axis=1)
        nearest_d2 = dist2[np.arange(len(att)), nearest_slot]
        in_range = nearest_d2 <= perception * perception

        new_nodes: list[tuple[np.ndarray, int]] = []

        for slot, tip_idx in enumerate(tip_list):
            local_mask = in_range & (nearest_slot == slot)
            tip_pos = nodes[tip_idx]

            if not np.any(local_mask):
                # Reach into the attractor-free gap until this tip can see the
                # canopy surface.  This is how the bare trunk transitions into
                # crown growth without special fan-out code.
                tip_diffs = att - tip_pos
                tip_d2 = (tip_diffs * tip_diffs).sum(axis=1)
                target = att[int(np.argmin(tip_d2))]
                direct_dir = _normalize(target - tip_pos, _UP)
                crown_dir = _normalize(target - trunk_tip_pos, direct_dir)
                direction = _normalize(
                    direct_dir + target_bias * crown_dir + tangent_bias * dirs[tip_idx] + tropism,
                    _UP,
                )
                direction = _clamp_elevation(direction, 0.0)
                new_nodes.append((tip_pos + direction * segment, tip_idx))
                continue

            local_dirs = att[local_mask] - tip_pos
            norms = np.sqrt((local_dirs * local_dirs).sum(axis=1, keepdims=True))
            dirs_n = local_dirs / np.maximum(norms, 1e-8)

            for group_mask in _split_attractor_groups(
                dirs_n, min_branch_points, xy_std_threshold,
            ):
                group_targets = att[local_mask][group_mask]
                target = group_targets.mean(axis=0)
                crown_dir = _normalize(target - trunk_tip_pos, dirs[tip_idx])
                growth = (
                    dirs_n[group_mask].mean(axis=0)
                    + target_bias * crown_dir
                    + tangent_bias * dirs[tip_idx]
                    + tropism
                )
                direction = _normalize(growth, _UP)
                direction = _clamp_elevation(direction, 0.0)
                new_nodes.append((tip_pos + direction * segment, tip_idx))

        if not new_nodes:
            break

        tips -= {parent_idx for _, parent_idx in new_nodes}
        start_idx = len(nodes)
        new_indices: list[int] = []
        for offset, (pos, parent_idx) in enumerate(new_nodes):
            parents.append(parent_idx)
            nodes.append(pos)
            dirs.append(_normalize(pos - nodes[parent_idx], _UP))
            new_idx = start_idx + offset
            tips.add(new_idx)
            new_indices.append(new_idx)

        new_arr = np.array([nodes[i] for i in new_indices], dtype=float)
        kill_diff = att[:, None, :] - new_arr[None, :, :]
        kill_d2 = (kill_diff * kill_diff).sum(axis=-1)
        nearest_new = np.argmin(kill_d2, axis=1)
        killed = np.min(kill_d2, axis=1) <= kill_r * kill_r

        if np.any(killed):
            killed_indices = np.flatnonzero(killed)
            for att_idx in killed_indices:
                parent_idx = new_indices[int(nearest_new[att_idx])]
                target = att[int(att_idx)]
                is_terminal = (
                    len(terminal_points) > 0
                    and np.any(np.linalg.norm(terminal_points - target[None, :], axis=1) < 1e-6)
                )
                if is_terminal and np.linalg.norm(target - nodes[parent_idx]) > 1e-8:
                    _append_node_at(parent_idx, target, nodes, dirs, parents)
            att = att[~killed]


def grow_surface_skeleton(
    cx: float,
    cy: float,
    tz: float,
    cfg,
    rng: np.random.Generator,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, float]:
    """Grow a trunk and an SCA crown attracted to canopy surface points."""
    height_max = float(sample(cfg.height_max_mm, rng))
    if height_max <= 0.0:
        root = np.array([[cx, cy, tz]], dtype=float)
        return root, np.array([-1], dtype=int), np.zeros(1), 0.0

    crown_radius = float(sample(cfg.crown_radius_mm, rng))
    if getattr(cfg, "trunk_height_mm", None) is None:
        trunk_height = height_max * (1.0 - float(cfg.crown_height_fraction))
    else:
        trunk_height = float(sample(cfg.trunk_height_mm, rng))
    trunk_height = float(np.clip(trunk_height, 0.0, height_max))
    crown_height = max(0.0, height_max - trunk_height)
    crown_base_z = tz + trunk_height

    n_trunk_segs = max(1, int(cfg.n_trunk_segs))
    trunk_seg_len = trunk_height / n_trunk_segs if trunk_height > 0.0 else 0.0
    branch_seg_len = max(0.5, float(getattr(cfg, "surface_branch_segment_mm", 2.0)))
    spacing = float(getattr(cfg, "surface_point_spacing_mm", 4.5))

    wander = float(sample(cfg.wander_deg, rng))
    lean = float(sample(cfg.initial_lean_deg, rng))
    lean_rad = float(np.radians(lean))
    lean_az = float(rng.uniform(0.0, 2.0 * np.pi))
    root_dir = np.array([
        np.sin(lean_rad) * np.cos(lean_az),
        np.sin(lean_rad) * np.sin(lean_az),
        np.cos(lean_rad),
    ]) if lean_rad > 1e-6 else _UP.copy()

    nodes: list[np.ndarray] = [np.array([cx, cy, tz], dtype=float)]
    parents: list[int] = [-1]
    dirs: list[np.ndarray] = [root_dir]

    trunk_tip = 0
    for _ in range(n_trunk_segs):
        if trunk_seg_len <= 1e-8:
            break
        d = _wander_direction(dirs[trunk_tip], wander, cfg.min_elevation_deg, rng)
        parents.append(trunk_tip)
        nodes.append(nodes[trunk_tip] + d * trunk_seg_len)
        dirs.append(d)
        trunk_tip = len(nodes) - 1

    endpoints = _sample_canopy_surface_points(
        cx, cy, crown_base_z, crown_height, crown_radius, spacing, cfg, rng,
    )
    if len(endpoints) == 0:
        nodes_xyz = np.array(nodes, dtype=float)
        parents_arr = np.array(parents, dtype=int)
        return nodes_xyz, parents_arr, _compute_arc_dists(nodes_xyz, parents_arr), crown_base_z

    interior = _sample_canopy_interior_points(nodes[trunk_tip], endpoints, crown_base_z, rng)
    attractors = np.vstack([endpoints, interior])
    rng.shuffle(attractors, axis=0)

    _grow_surface_sca(
        trunk_tip, attractors, endpoints, branch_seg_len, cfg.min_elevation_deg,
        nodes, dirs, parents, cfg,
    )

    nodes_xyz = np.array(nodes, dtype=float)
    parents_arr = np.array(parents, dtype=int)
    arc_dists = _compute_arc_dists(nodes_xyz, parents_arr)
    return nodes_xyz, parents_arr, arc_dists, float(max(tz + 0.1, crown_base_z))
