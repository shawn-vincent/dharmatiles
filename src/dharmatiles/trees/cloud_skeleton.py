"""CloudTree skeleton growth: point-cloud-partitioned breadth-first algorithm."""
from __future__ import annotations

from dataclasses import dataclass

import numpy as np

from .envelope import TreeEnvelope


@dataclass
class _CloudParams:
    segment_length_mm: float
    kill_radius_mm: float
    trunk_radius_mm: float
    min_radius_mm: float
    min_up_sin: float        # sin(min_up_angle_deg)
    min_branch_cos: float    # cos(min_branch_angle_deg)
    branch_split_cos: float  # cos(branch_split_angle_deg)
    max_branches_per_step: int
    branch_exponent: float
    smoothing_alpha: float
    crown_base_z: float      # no stray detection below this height
    max_steps_per_branch: int  # safety cap; prevents infinite growth in empty space


@dataclass
class _CloudBranch:
    tip_idx: int
    radius: float
    prior_dir: np.ndarray   # smoothed heading; also Bezier start tangent for next node
    points: np.ndarray      # (N, 3) owned attraction points
    steps_left: int = 0     # remaining steps before forced termination


def grow_cloud_skeleton(
    env: TreeEnvelope,
    rng: np.random.Generator,
    *,
    n_attraction: int = 200,
    segment_length_mm: float = 2.0,
    kill_radius_mm: float | None = None,
    trunk_radius_mm: float = 2.0,
    min_radius_mm: float = 0.45,
    min_up_angle_deg: float = 20.0,
    min_branch_angle_deg: float = 30.0,
    branch_split_angle_deg: float | None = None,
    max_branches_per_step: int = 3,
    branch_exponent: float = 2.5,
    smoothing_alpha: float = 0.25,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    """Grow a CloudTree skeleton filling *env*.

    Returns five arrays:
        nodes       (N, 3) — node positions
        parents     (N,)   — parent index, -1 for root
        radii       (N,)   — radius of the arriving segment at each node
        prior_dirs  (N, 3) — smoothed heading arriving at each node (Bezier tangent)
        attractors  (M, 3) — the sampled attraction-point cloud (for debug visualisation)
    """
    if kill_radius_mm is None:
        kill_radius_mm = segment_length_mm
    if branch_split_angle_deg is None:
        branch_split_angle_deg = min_branch_angle_deg

    # Each branch is capped at a generous multiple of the tree height in steps.
    # This prevents infinite growth when the tip advances through empty space
    # (no cloud points within kill_radius) and no points are consumed.
    max_steps = max(40, int(np.ceil(env.height_mm / segment_length_mm) * 4))

    params = _CloudParams(
        segment_length_mm=float(segment_length_mm),
        kill_radius_mm=float(kill_radius_mm),
        trunk_radius_mm=float(trunk_radius_mm),
        min_radius_mm=float(min_radius_mm),
        min_up_sin=float(np.sin(np.radians(min_up_angle_deg))),
        min_branch_cos=float(np.cos(np.radians(min_branch_angle_deg))),
        branch_split_cos=float(np.cos(np.radians(branch_split_angle_deg))),
        max_branches_per_step=int(max_branches_per_step),
        branch_exponent=float(branch_exponent),
        smoothing_alpha=float(smoothing_alpha),
        crown_base_z=float(env.crown_base_z),
        max_steps_per_branch=max_steps,
    )

    pts = _sample_cloud(env, rng, n_attraction)

    root_pos = np.array([env.cx, env.cy, env.terrain_z], dtype=float)
    up = np.array([0.0, 0.0, 1.0], dtype=float)

    # Parallel node arrays built during growth.
    positions: list[np.ndarray] = [root_pos]
    par_list:  list[int]        = [-1]
    radii:     list[float]      = [float(trunk_radius_mm)]
    p_dirs:    list[np.ndarray] = [up.copy()]

    if len(pts) == 0:
        return _pack(positions, par_list, radii, p_dirs, pts)

    queue: list[_CloudBranch] = [
        _CloudBranch(tip_idx=0, radius=float(trunk_radius_mm),
                     prior_dir=up.copy(), points=pts,
                     steps_left=max_steps)
    ]

    while queue:
        next_queue: list[_CloudBranch] = []
        for branch in queue:
            children = _grow_one_step(branch, positions, par_list, radii, p_dirs, params)
            next_queue.extend(children)
        queue = next_queue

    _rescue_uncovered(pts, positions, par_list, radii, p_dirs, params)

    return _pack(positions, par_list, radii, p_dirs, pts)


# ─────────────────────────────────────────────────────────────────────────────
# Core growth step
# ─────────────────────────────────────────────────────────────────────────────

def _grow_one_step(
    branch: _CloudBranch,
    positions: list[np.ndarray],
    par_list: list[int],
    radii: list[float],
    p_dirs: list[np.ndarray],
    p: _CloudParams,
) -> list[_CloudBranch]:
    if branch.radius < p.min_radius_mm or len(branch.points) == 0 or branch.steps_left <= 0:
        return []
    steps_left = branch.steps_left - 1

    tip = positions[branch.tip_idx]

    # 3a — primary direction with momentum smoothing
    centroid = branch.points.mean(axis=0)

    # Overshoot guard: if the centroid has fallen below the tip (we've grown
    # past our cloud and min_up prevents turning back), stop here.
    if tip[2] > p.crown_base_z and centroid[2] < tip[2]:
        return []

    raw = centroid - tip
    raw_len = float(np.linalg.norm(raw))
    centroid_dir = (raw / raw_len) if raw_len > 1e-9 else branch.prior_dir.copy()

    a = p.smoothing_alpha
    blended = centroid_dir * (1.0 - a) + branch.prior_dir * a
    b_len = float(np.linalg.norm(blended))
    blended = (blended / b_len) if b_len > 1e-9 else up_vec()
    direction = _enforce_min_up(blended, p.min_up_sin)

    # 3b — stray detection (only above crown base)
    if tip[2] >= p.crown_base_z:
        to_pts = branch.points - tip
        norms = np.linalg.norm(to_pts, axis=1, keepdims=True)
        unit_to = to_pts / (norms + 1e-9)
        cos_a = np.clip(unit_to @ direction, -1.0, 1.0)
        stray_mask = cos_a < p.branch_split_cos
        stray = branch.points[stray_mask]
        primary = branch.points[~stray_mask]
    else:
        stray = np.empty((0, 3), dtype=float)
        primary = branch.points

    # 3c — cluster strays via PCA angular split
    max_k = p.max_branches_per_step - 1
    clusters = _cluster_pca(stray, tip, max_k) if len(stray) > 0 else []

    # 3d — advance tip; emit new node at pre-split radius
    new_tip = tip + direction * p.segment_length_mm
    new_tip_idx = len(positions)
    positions.append(new_tip)
    par_list.append(branch.tip_idx)
    radii.append(branch.radius)
    p_dirs.append(direction.copy())

    # 3e — consume attraction points near new_tip
    primary = _kill(primary, new_tip, p.kill_radius_mm)
    clusters = [kc for kc in (_kill(c, new_tip, p.kill_radius_mm) for c in clusters)
                if len(kc) > 0]

    # 3f — drop empty groups
    all_groups = [g for g in ([primary] + clusters) if len(g) > 0]
    if not all_groups:
        return []

    # 3g — pipe-model child radii (exact: sum(r_i^e) = r_parent^e)
    n_total = sum(len(g) for g in all_groups)
    child_radii = [
        branch.radius * (len(g) / n_total) ** (1.0 / p.branch_exponent)
        for g in all_groups
    ]

    # 3h — construct child branches
    children: list[_CloudBranch] = []
    for group, r in zip(all_groups, child_radii):
        r = max(r, p.min_radius_mm)  # floor instead of skip — keeps all point groups alive
        g_cen = group.mean(axis=0)
        cdr = g_cen - new_tip
        cdr_len = float(np.linalg.norm(cdr))
        child_dir = (cdr / cdr_len) if cdr_len > 1e-9 else direction.copy()
        child_dir = _enforce_min_up(child_dir, p.min_up_sin)
        child_dir = _enforce_min_branch_angle(direction, child_dir, p.min_branch_cos)
        # Re-apply min_up after branch-angle push; iterate until stable (≤ 3 passes).
        for _ in range(3):
            adj = _enforce_min_up(child_dir, p.min_up_sin)
            if np.allclose(adj, child_dir, atol=1e-6):
                break
            child_dir = _enforce_min_branch_angle(direction, adj, p.min_branch_cos)

        # prior_dir initialised to parent's direction → C1 Bezier continuity at join.
        children.append(_CloudBranch(
            tip_idx=new_tip_idx,
            radius=r,
            prior_dir=direction.copy(),
            points=group,
            steps_left=steps_left,
        ))

    return children


# ─────────────────────────────────────────────────────────────────────────────
# Helpers
# ─────────────────────────────────────────────────────────────────────────────

def up_vec() -> np.ndarray:
    return np.array([0.0, 0.0, 1.0], dtype=float)


def _pack(
    positions: list[np.ndarray],
    par_list: list[int],
    radii: list[float],
    p_dirs: list[np.ndarray],
    attractors: np.ndarray,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    return (
        np.array(positions, dtype=float),
        np.array(par_list, dtype=int),
        np.array(radii, dtype=float),
        np.array(p_dirs, dtype=float),
        attractors,
    )


def _kill(pts: np.ndarray, tip: np.ndarray, kill_r: float) -> np.ndarray:
    if len(pts) == 0:
        return pts
    return pts[np.linalg.norm(pts - tip, axis=1) > kill_r]


def _cluster_pca(stray: np.ndarray, tip: np.ndarray, max_k: int) -> list[np.ndarray]:
    """Split stray points into ≤ max_k angular clusters via PCA on direction vectors."""
    if max_k <= 0 or len(stray) == 0:
        return []
    if max_k == 1 or len(stray) < 2:
        return [stray]
    dirs = stray - tip
    unit = dirs / (np.linalg.norm(dirs, axis=1, keepdims=True) + 1e-9)
    centered = unit - unit.mean(axis=0)
    try:
        _, _, vh = np.linalg.svd(centered, full_matrices=False)
    except np.linalg.LinAlgError:
        return [stray]
    proj = centered @ vh[0]
    a = stray[proj <= 0.0]
    b = stray[proj > 0.0]
    clusters = [c for c in [a, b] if len(c) > 0]
    return clusters[:max_k]


def _rescue_uncovered(
    pts: np.ndarray,
    positions: list[np.ndarray],
    par_list: list[int],
    radii: list[float],
    p_dirs: list[np.ndarray],
    params: _CloudParams,
) -> None:
    """Grow straight branches from nearest nodes to any still-uncovered attractor points."""
    if len(pts) == 0 or len(positions) == 0:
        return
    for pt in pts:
        nodes = np.array(positions, dtype=float)
        dists = np.linalg.norm(nodes - pt, axis=1)
        if float(dists.min()) <= params.kill_radius_mm:
            continue
        nearest_idx = int(dists.argmin())
        current_idx = nearest_idx
        pos = nodes[nearest_idx].copy()
        for _ in range(params.max_steps_per_branch):
            to_pt = pt - pos
            dist = float(np.linalg.norm(to_pt))
            if dist <= params.kill_radius_mm:
                break
            direction = to_pt / dist
            step_size = min(params.segment_length_mm, dist)
            pos = pos + direction * step_size
            new_idx = len(positions)
            positions.append(pos.copy())
            par_list.append(current_idx)
            radii.append(params.min_radius_mm)
            p_dirs.append(direction.copy())
            current_idx = new_idx


def _sample_cloud(env: TreeEnvelope, rng: np.random.Generator, n: int) -> np.ndarray:
    """Uniform rejection sampling inside the crown envelope."""
    if env.crown_height <= 1e-8 or env.crown_radius_mm <= 1e-8:
        return np.empty((0, 3), dtype=float)
    points: list[list[float]] = []
    max_attempts = n * 25
    attempts = 0
    while len(points) < n and attempts < max_attempts:
        attempts += 1
        t = float(rng.uniform(0.0, 1.0))
        r_max = float(env.radius_at_t(t))
        if r_max <= 1e-8:
            continue
        theta = float(rng.uniform(0.0, 2.0 * np.pi))
        rho = float(np.sqrt(rng.uniform(0.0, 1.0))) * r_max
        z = env.crown_base_z + t * env.crown_height
        points.append([env.cx + rho * np.cos(theta), env.cy + rho * np.sin(theta), z])
    return np.array(points, dtype=float) if points else np.empty((0, 3), dtype=float)


def _enforce_min_up(d: np.ndarray, min_z: float) -> np.ndarray:
    """Clamp d to have at least min_z vertical component, preserving azimuth."""
    if float(d[2]) >= min_z:
        return d
    xy = d[:2]
    n = float(np.linalg.norm(xy))
    if n < 1e-9:
        return up_vec()
    xy_mag = float(np.sqrt(max(0.0, 1.0 - min_z * min_z)))
    return np.array([xy[0] / n * xy_mag, xy[1] / n * xy_mag, min_z], dtype=float)


def _enforce_min_branch_angle(
    parent_dir: np.ndarray,
    child_dir: np.ndarray,
    min_cos: float,
) -> np.ndarray:
    """Rotate child_dir away from parent_dir until their angle >= min_branch_angle.

    min_cos = cos(min_branch_angle_deg).  Adjustment triggers when the
    current angle is too small (cos_a > min_cos).
    """
    cos_a = float(np.clip(np.dot(parent_dir, child_dir), -1.0, 1.0))
    if cos_a <= min_cos:
        return child_dir  # already divergent enough
    # Component of child_dir perpendicular to parent_dir.
    perp = child_dir - cos_a * parent_dir
    perp_len = float(np.linalg.norm(perp))
    if perp_len < 1e-9:
        # Parallel — pick an arbitrary perpendicular.
        ref = np.array([1.0, 0.0, 0.0])
        if abs(float(np.dot(parent_dir, ref))) > 0.9:
            ref = np.array([0.0, 1.0, 0.0])
        perp = np.cross(parent_dir, ref)
        perp_len = float(np.linalg.norm(perp))
        if perp_len < 1e-9:
            return child_dir
    perp_unit = perp / perp_len
    # Place child exactly at the minimum angle, on the same side as original child_dir.
    sin_target = float(np.sqrt(max(0.0, 1.0 - min_cos * min_cos)))
    result = min_cos * parent_dir + sin_target * perp_unit
    r_len = float(np.linalg.norm(result))
    return result / r_len if r_len > 1e-9 else child_dir
