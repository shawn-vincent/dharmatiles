"""
Constructive tree skeleton growth.

The constructive grower builds a predictable layered tree: a trunk run,
then repeated split-and-grow levels.  Tips at the same level grow
simultaneously and use angular-sector repulsion to steer into open space.

Public API
----------
``grow_const_skeleton(cx, cy, tz, cfg, rng)``
    Returns ``(nodes_xyz, parents, arc_dists, crown_base_z)`` matching the
    SCA skeleton API consumed by ``trees/tree.py``.
"""
from __future__ import annotations

import numpy as np

from ..dist import Sample, sample
from .skeleton import _compute_arc_dists


_UP = np.array([0.0, 0.0, 1.0])
_GOLDEN_ANGLE = np.radians(137.50776405003785)


def _normalize(v: np.ndarray, fallback: np.ndarray | None = None) -> np.ndarray:
    n = float(np.linalg.norm(v))
    if n > 1e-10:
        return v / n
    if fallback is not None:
        return fallback.copy()
    return _UP.copy()


def _basis_from_dir(direction: np.ndarray) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    w = _normalize(direction)
    ref = np.array([1.0, 0.0, 0.0])
    if abs(float(np.dot(w, ref))) > 0.9:
        ref = np.array([0.0, 1.0, 0.0])
    u = _normalize(np.cross(ref, w), np.array([0.0, 1.0, 0.0]))
    v = _normalize(np.cross(w, u), np.array([1.0, 0.0, 0.0]))
    return u, v, w


def _clamp_elevation(direction: np.ndarray, min_elevation_deg: float) -> np.ndarray:
    d = _normalize(direction)
    min_z = float(np.sin(np.radians(min_elevation_deg)))
    if d[2] >= min_z:
        return d

    xy = d[:2]
    xy_n = float(np.linalg.norm(xy))
    xy_dir = xy / xy_n if xy_n > 1e-10 else np.array([1.0, 0.0])
    xy_mag = float(np.sqrt(max(0.0, 1.0 - min_z * min_z)))
    return np.array([xy_dir[0] * xy_mag, xy_dir[1] * xy_mag, min_z])


def _direction_from_spread(
    parent_dir: np.ndarray,
    azimuth: float,
    spread_deg: float,
    min_elevation_deg: float,
) -> np.ndarray:
    u, v, w = _basis_from_dir(parent_dir)
    spread = np.radians(spread_deg)
    out = (
        np.cos(spread) * w
        + np.sin(spread) * (np.cos(azimuth) * u + np.sin(azimuth) * v)
    )
    return _clamp_elevation(out, min_elevation_deg)


def _compute_repulsion(
    tip_idx: int,
    active_tips: list[int],
    nodes: list[np.ndarray],
    seg_len_mm: float,
    z_window: float,
) -> np.ndarray:
    tip = nodes[tip_idx]
    bearings: list[float] = []
    for other_idx in active_tips:
        if other_idx == tip_idx:
            continue
        other = nodes[other_idx]
        if abs(float(other[2] - tip[2])) >= z_window * seg_len_mm:
            continue
        dx = float(other[0] - tip[0])
        dy = float(other[1] - tip[1])
        if dx * dx + dy * dy < 1e-10:
            continue
        bearings.append(float(np.arctan2(dy, dx)))

    if not bearings:
        return np.zeros(3)

    b = np.sort(np.array(bearings))
    gaps = (np.roll(b, -1) - b) % (2.0 * np.pi)
    start = float(b[int(np.argmax(gaps))])
    target = start + float(gaps.max()) * 0.5
    return np.array([np.cos(target), np.sin(target), 0.0])


def _step_direction(
    tip_idx: int,
    active_tips: list[int],
    nodes: list[np.ndarray],
    dirs: list[np.ndarray],
    seg_len_mm: float,
    cfg,
    wander_deg: float,
    rng: np.random.Generator,
) -> np.ndarray:
    base = dirs[tip_idx]
    biased = _normalize(base + cfg.upward_bias * (_UP - base), base)
    repulsion = _compute_repulsion(
        tip_idx, active_tips, nodes, seg_len_mm, cfg.repulsion_z_window,
    )
    guided = _normalize(biased + cfg.repulsion_strength * repulsion, biased)

    u, v, _w = _basis_from_dir(guided)
    sigma = float(np.sin(np.radians(wander_deg)))
    jitter = rng.normal(0.0, sigma, 2)
    wandered = guided + jitter[0] * u + jitter[1] * v
    return _clamp_elevation(wandered, cfg.min_elevation_deg)


def _as_level_list(value, n_levels: int, name: str) -> list:
    if isinstance(value, (list, tuple)):
        if len(value) != n_levels:
            raise ValueError(f"{name} must have {n_levels} entries")
        return list(value)
    return [value] * n_levels


def _sample_level_values(
    value: Sample[float] | list[Sample[float]],
    n_levels: int,
    name: str,
    rng: np.random.Generator,
) -> list[float]:
    return [float(sample(v, rng)) for v in _as_level_list(value, n_levels, name)]


def _split_tip(
    tip_idx: int,
    nodes: list[np.ndarray],
    dirs: list[np.ndarray],
    parents: list[int],
    n_children: int,
    spread_deg: float,
    seg_len_mm: float,
    z_limit: float,
    cfg,
    rng: np.random.Generator,
) -> list[int]:
    phase = float(rng.uniform(0.0, 2.0 * np.pi))
    parent_dir = dirs[tip_idx]
    child_indices: list[int] = []

    for i in range(n_children):
        child_spread = spread_deg
        if cfg.dominant_branch and i == 0:
            child_spread *= cfg.dominant_angle_factor
        child_dir = _direction_from_spread(
            parent_dir,
            phase + i * _GOLDEN_ANGLE,
            child_spread,
            cfg.min_elevation_deg,
        )
        child_pos = nodes[tip_idx] + child_dir * seg_len_mm
        if child_pos[2] > z_limit:
            continue
        parents.append(tip_idx)
        nodes.append(child_pos)
        dirs.append(child_dir)
        child_indices.append(len(nodes) - 1)

    return child_indices


def grow_const_skeleton(
    cx: float,
    cy: float,
    tz: float,
    cfg,
    rng: np.random.Generator,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, float]:
    """Grow a constructive skeleton from a single ground-level root."""
    height_max = float(sample(cfg.height_max_mm, rng))
    if height_max <= 0.0:
        root = np.array([[cx, cy, tz]], dtype=float)
        parents = np.array([-1], dtype=int)
        return root, parents, np.zeros(1), 0.0

    n_segs_per_level = [int(v) for v in _as_level_list(
        cfg.n_segs_per_level, cfg.n_levels, "n_segs_per_level",
    )]
    if cfg.n_levels < 0:
        raise ValueError("n_levels must be non-negative")
    if cfg.n_trunk_segs < 0 or any(v < 0 for v in n_segs_per_level):
        raise ValueError("segment counts must be non-negative")

    total_segments = int(cfg.n_trunk_segs + sum(n_segs_per_level))
    if total_segments <= 0:
        total_segments = 1
    seg_len_mm = height_max / total_segments
    spread_angles = _sample_level_values(
        cfg.spread_angle_deg, cfg.n_levels, "spread_angle_deg", rng,
    )
    lean = float(sample(cfg.initial_lean_deg, rng))
    wander = float(sample(cfg.wander_deg, rng))

    lean_az = float(rng.uniform(0.0, 2.0 * np.pi))
    root_dir = _direction_from_spread(_UP, lean_az, lean, cfg.min_elevation_deg)

    nodes: list[np.ndarray] = [np.array([cx, cy, tz], dtype=float)]
    parents: list[int] = [-1]
    dirs: list[np.ndarray] = [root_dir]
    active_tips = [0]
    z_limit = tz + height_max

    def grow_run(tips: list[int], n_steps: int) -> list[int]:
        active = tips
        for _ in range(n_steps):
            next_tips: list[int] = []
            for tip_idx in active:
                new_dir = _step_direction(
                    tip_idx, active, nodes, dirs, seg_len_mm, cfg, wander, rng,
                )
                new_pos = nodes[tip_idx] + new_dir * seg_len_mm
                if new_pos[2] > z_limit:
                    continue
                parents.append(tip_idx)
                nodes.append(new_pos)
                dirs.append(new_dir)
                next_tips.append(len(nodes) - 1)
            active = next_tips
            if not active:
                break
        return active

    active_tips = grow_run(active_tips, int(cfg.n_trunk_segs))

    for level in range(cfg.n_levels):
        post_split: list[int] = []
        for tip_idx in active_tips:
            n_children = int(rng.integers(cfg.split_count_min, cfg.split_count_max + 1))
            n_children = max(1, n_children)
            post_split.extend(
                _split_tip(
                    tip_idx, nodes, dirs, parents, n_children,
                    spread_angles[level], seg_len_mm, z_limit, cfg, rng,
                )
            )
        active_tips = grow_run(post_split, max(0, n_segs_per_level[level] - 1))
        if not active_tips:
            break

    nodes_xyz = np.array(nodes, dtype=float)
    parents_arr = np.array(parents, dtype=int)
    arc_dists = _compute_arc_dists(nodes_xyz, parents_arr)
    crown_base_z = max(0.1, float(cfg.n_trunk_segs) * seg_len_mm)
    return nodes_xyz, parents_arr, arc_dists, crown_base_z
