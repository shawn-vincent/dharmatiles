"""Space colonization skeleton growth for envelope trees."""
from __future__ import annotations

import numpy as np

from .attractors import sample_attractors
from .envelope import TreeEnvelope


def grow_skeleton(
    env: TreeEnvelope,
    rng: np.random.Generator,
) -> tuple[np.ndarray, np.ndarray]:
    """Grow a printable tree skeleton that fills *env*."""
    nodes: list[np.ndarray] = [np.array([env.cx, env.cy, env.terrain_z], dtype=float)]
    parents: list[int] = [-1]

    height = max(env.height_mm, 1e-8)
    step_len = float(np.clip(height / 18.0, 1.0, 2.2))
    trunk_step = float(np.clip(height / 18.0, 1.2, 2.4))
    # Smaller kill radius so attractors survive long enough to guide multiple branches.
    kill_radius = 1.0 * step_len
    perception_radius = float(np.clip(0.32 * env.crown_radius_mm, 4.0, 9.0))

    trunk_tip = _grow_trunk(nodes, parents, env, trunk_step, rng)
    active = [trunk_tip]

    attractors = sample_attractors(env, rng)
    if len(attractors) == 0:
        return np.array(nodes, dtype=float), np.array(parents, dtype=int)

    max_steps = int(np.clip(round(height / step_len * 3.5), 45, 160))
    min_cluster = max(3, round(len(attractors) / 100))
    dirs = _initial_dirs(nodes, parents)

    for _ in range(max_steps):
        if len(attractors) == 0 or not active:
            break

        active_pts = np.array([nodes[i] for i in active], dtype=float)
        diff = attractors[:, None, :] - active_pts[None, :, :]
        dist = np.linalg.norm(diff, axis=2)
        alive = dist.min(axis=1) > kill_radius
        attractors = attractors[alive]
        if len(attractors) == 0:
            break

        diff = attractors[:, None, :] - active_pts[None, :, :]
        dist = np.linalg.norm(diff, axis=2)
        nearest = np.argmin(dist, axis=1)
        nearest_dist = dist[np.arange(len(attractors)), nearest]
        visible = nearest_dist <= perception_radius

        assigned: dict[int, list[np.ndarray]] = {}
        for att, local_idx, is_visible in zip(attractors, nearest, visible):
            if is_visible:
                assigned.setdefault(active[int(local_idx)], []).append(att)

        new_active: list[int] = []
        for tip_idx in active:
            pts = assigned.get(tip_idx)
            if not pts:
                continue
            clusters = _split_clusters(
                nodes[tip_idx], np.array(pts), min_cluster, rng,
                crown_base_z=env.crown_base_z, crown_height=env.crown_height,
            )
            parent_dir = dirs.get(tip_idx, np.array([0.0, 0.0, 1.0]))
            for cluster in clusters:
                d = _growth_direction(nodes[tip_idx], cluster, parent_dir, env)
                child = nodes[tip_idx] + d * step_len
                if child[2] >= env.crown_base_z:
                    child = env.project_inside(child)
                child_idx = len(nodes)
                nodes.append(child)
                parents.append(tip_idx)
                dirs[child_idx] = d
                new_active.append(child_idx)
        active = new_active

    return np.array(nodes, dtype=float), np.array(parents, dtype=int)


def _grow_trunk(
    nodes: list[np.ndarray],
    parents: list[int],
    env: TreeEnvelope,
    trunk_step: float,
    rng: np.random.Generator,
) -> int:
    target_z = env.crown_base_z
    if target_z <= env.terrain_z + 1e-8:
        return 0
    n_steps = max(1, int(np.ceil((target_z - env.terrain_z) / trunk_step)))
    phase = float(rng.uniform(0.0, 2.0 * np.pi))
    drift_dir = np.array([np.cos(phase), np.sin(phase), 0.0])
    max_drift = 0.12 * env.crown_radius_mm
    tip = 0
    for i in range(n_steps):
        f = (i + 1) / n_steps
        z = env.terrain_z + f * (target_z - env.terrain_z)
        drift = max_drift * np.sin(f * np.pi) * (0.55 + 0.25 * np.sin(phase + f * 4.0))
        pos = np.array([env.cx, env.cy, z], dtype=float) + drift_dir * drift
        parents.append(tip)
        nodes.append(pos)
        tip = len(nodes) - 1
    return tip


def _initial_dirs(nodes: list[np.ndarray], parents: list[int]) -> dict[int, np.ndarray]:
    dirs: dict[int, np.ndarray] = {0: np.array([0.0, 0.0, 1.0])}
    for i in range(1, len(nodes)):
        dirs[i] = _normalize(nodes[i] - nodes[parents[i]], np.array([0.0, 0.0, 1.0]))
    return dirs


def _split_clusters(
    tip: np.ndarray,
    pts: np.ndarray,
    min_cluster: int,
    rng: np.random.Generator,
    *,
    crown_base_z: float = 0.0,
    crown_height: float = 1e8,
) -> list[np.ndarray]:
    # Suppress all splits in the lower 20% of crown height.  This keeps a
    # visible main stem growing up through the lower crown before the first
    # scaffold branches emerge, matching the look of a natural deciduous tree.
    crown_frac = (tip[2] - crown_base_z) / max(crown_height, 1e-8)
    if crown_frac < 0.20:
        return [pts]

    if len(pts) < min_cluster * 2:
        return [pts]
    dirs = pts - tip
    unit = dirs / (np.linalg.norm(dirs, axis=1, keepdims=True) + 1e-9)
    centered = unit - unit.mean(axis=0, keepdims=True)
    try:
        _u, _s, vh = np.linalg.svd(centered, full_matrices=False)
    except np.linalg.LinAlgError:
        return [pts]
    axis = vh[0]
    proj = centered @ axis
    a = pts[proj <= 0.0]
    b = pts[proj > 0.0]
    if len(a) < min_cluster or len(b) < min_cluster:
        return [pts]
    da = _normalize(a.mean(axis=0) - tip)
    db = _normalize(b.mean(axis=0) - tip)
    angle = np.degrees(np.arccos(float(np.clip(np.dot(da, db), -1.0, 1.0))))
    if angle < 25.0:
        return [pts]
    if rng.random() < 0.08:
        return [pts]
    return [a, b]


def _growth_direction(
    tip: np.ndarray,
    attractors: np.ndarray,
    parent_dir: np.ndarray,
    env: TreeEnvelope,
) -> np.ndarray:
    to_att = attractors - tip
    to_att /= np.linalg.norm(to_att, axis=1, keepdims=True) + 1e-9
    d = to_att.mean(axis=0)

    # Use crown-relative height for biases so that lower branches get
    # a stronger outward push regardless of trunk_height_mm.
    crown_frac = max(0.0, (tip[2] - env.crown_base_z) / max(env.crown_height, 1e-8))

    up = np.array([0.0, 0.0, 1.0])
    out = np.array([tip[0] - env.cx, tip[1] - env.cy, 0.0])
    out = _normalize(out, np.zeros(3))

    # Smaller upward bias (was 0.12) lets attractors guide direction more freely.
    # Stronger outward push in lower 70% of crown, fading away above that.
    # Slightly reduced parent inertia (was 0.35) for better attractor response.
    outward_strength = 0.25 * max(0.0, 0.70 - crown_frac)
    d = d + 0.07 * up + outward_strength * out + 0.30 * parent_dir
    d = _normalize(d, up)

    # 20° minimum elevation (was 35°) allows natural horizontal scaffold
    # branches in the lower crown while still satisfying FDM overhang limits
    # for thin branches at miniature scale.
    return _clamp_elevation(d, 20.0)


def _clamp_elevation(d: np.ndarray, min_deg: float) -> np.ndarray:
    d = _normalize(d)
    min_z = float(np.sin(np.radians(min_deg)))
    if d[2] >= min_z:
        return d
    xy = d[:2]
    n = float(np.linalg.norm(xy))
    xy_dir = xy / n if n > 1e-9 else np.array([1.0, 0.0])
    xy_mag = float(np.sqrt(max(0.0, 1.0 - min_z * min_z)))
    return np.array([xy_dir[0] * xy_mag, xy_dir[1] * xy_mag, min_z])


def _normalize(v: np.ndarray, fallback: np.ndarray | None = None) -> np.ndarray:
    n = float(np.linalg.norm(v))
    if n > 1e-9:
        return v / n
    if fallback is not None:
        return fallback.copy()
    return np.array([0.0, 0.0, 1.0])
