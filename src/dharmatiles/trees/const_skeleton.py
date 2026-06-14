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

from dataclasses import dataclass

import numpy as np

from ..dist import Sample, sample
from .skeleton import _compute_arc_dists


_UP = np.array([0.0, 0.0, 1.0])
_GOLDEN_ANGLE = np.radians(137.50776405003785)
_FORK_CLEARANCE_IGNORE_T = 0.35


@dataclass(frozen=True)
class _Candidate:
    slot_idx: int
    tip_idx: int
    direction: np.ndarray
    end_pos: np.ndarray
    score: float


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


def _segment_segment_distance(
    a0: np.ndarray,
    a1: np.ndarray,
    b0: np.ndarray,
    b1: np.ndarray,
) -> float:
    """Shortest distance between two 3-D line segments."""
    u = a1 - a0
    v = b1 - b0
    w = a0 - b0
    aa = float(np.dot(u, u))
    bb = float(np.dot(v, v))
    cc = float(np.dot(u, v))
    dd = float(np.dot(u, w))
    ee = float(np.dot(v, w))
    denom = aa * bb - cc * cc

    s_num = denom
    t_num = denom
    s_den = denom
    t_den = denom

    if aa <= 1e-12 and bb <= 1e-12:
        return float(np.linalg.norm(a0 - b0))
    if aa <= 1e-12:
        t = np.clip(ee / bb, 0.0, 1.0)
        return float(np.linalg.norm(a0 - (b0 + t * v)))
    if bb <= 1e-12:
        s = np.clip(-dd / aa, 0.0, 1.0)
        return float(np.linalg.norm((a0 + s * u) - b0))

    if denom < 1e-12:
        s_num = 0.0
        s_den = 1.0
        t_num = ee
        t_den = bb
    else:
        s_num = cc * ee - bb * dd
        t_num = aa * ee - cc * dd
        if s_num < 0.0:
            s_num = 0.0
            t_num = ee
            t_den = bb
        elif s_num > s_den:
            s_num = s_den
            t_num = ee + cc
            t_den = bb

    if t_num < 0.0:
        t_num = 0.0
        if -dd < 0.0:
            s_num = 0.0
        elif -dd > aa:
            s_num = s_den
        else:
            s_num = -dd
            s_den = aa
    elif t_num > t_den:
        t_num = t_den
        if -dd + cc < 0.0:
            s_num = 0.0
        elif -dd + cc > aa:
            s_num = s_den
        else:
            s_num = -dd + cc
            s_den = aa

    s = 0.0 if abs(s_num) < 1e-12 else s_num / s_den
    t = 0.0 if abs(t_num) < 1e-12 else t_num / t_den
    closest = w + s * u - t * v
    return float(np.linalg.norm(closest))


def _segment_has_space(
    start_idx: int,
    end_pos: np.ndarray,
    nodes: list[np.ndarray],
    parents: list[int],
    clearance_mm: float,
) -> bool:
    if clearance_mm <= 0.0:
        return True

    start_pos = nodes[start_idx]
    start_parent_idx = int(parents[start_idx]) if start_idx < len(parents) else -1
    for end_idx in range(1, len(nodes)):
        other_start_idx = int(parents[end_idx])
        if other_start_idx < 0:
            continue
        if start_idx in (other_start_idx, end_idx):
            continue
        if start_parent_idx >= 0 and start_parent_idx == other_start_idx:
            continue
        dist = _segment_segment_distance(
            start_pos, end_pos, nodes[other_start_idx], nodes[end_idx],
        )
        if dist < clearance_mm:
            return False
    return True


def _candidate_has_existing_space(
    tip_idx: int,
    end_pos: np.ndarray,
    nodes: list[np.ndarray],
    parents: list[int],
    z_limit: float,
    clearance_mm: float,
) -> bool:
    return (
        end_pos[2] <= z_limit
        and _segment_has_space(tip_idx, end_pos, nodes, parents, clearance_mm)
    )


def _candidates_are_compatible(
    a: _Candidate,
    b: _Candidate,
    nodes: list[np.ndarray],
    parents: list[int],
    clearance_mm: float,
) -> bool:
    if clearance_mm <= 0.0:
        return True

    a_parent = int(parents[a.tip_idx]) if a.tip_idx < len(parents) else -1
    b_parent = int(parents[b.tip_idx]) if b.tip_idx < len(parents) else -1
    if a.tip_idx == b.tip_idx:
        return float(np.linalg.norm(a.end_pos - b.end_pos)) >= clearance_mm

    same_parent_fork = a_parent >= 0 and a_parent == b_parent
    if same_parent_fork:
        a_start = nodes[a.tip_idx]
        b_start = nodes[b.tip_idx]
        a_clear_start = a_start + (a.end_pos - a_start) * _FORK_CLEARANCE_IGNORE_T
        b_clear_start = b_start + (b.end_pos - b_start) * _FORK_CLEARANCE_IGNORE_T
        dist = _segment_segment_distance(
            a_clear_start, a.end_pos, b_clear_start, b.end_pos,
        )
        return dist >= clearance_mm

    dist = _segment_segment_distance(
        nodes[a.tip_idx], a.end_pos, nodes[b.tip_idx], b.end_pos,
    )
    return dist >= clearance_mm


def _score_candidate(
    tip_idx: int,
    direction: np.ndarray,
    end_pos: np.ndarray,
    active_tips: list[int],
    nodes: list[np.ndarray],
    dirs: list[np.ndarray],
    repulsion: np.ndarray,
    seg_len_mm: float,
) -> float:
    momentum = float(np.dot(direction, dirs[tip_idx]))
    upward = float(direction[2])
    has_repulsion = np.linalg.norm(repulsion) > 1e-8
    open_dir = float(np.dot(direction, repulsion)) if has_repulsion else 0.0

    min_tip_dist = seg_len_mm
    for other_idx in active_tips:
        if other_idx == tip_idx:
            continue
        dist = float(np.linalg.norm(end_pos - nodes[other_idx]))
        min_tip_dist = min(min_tip_dist, dist)

    return (
        momentum
        + 0.35 * upward
        + 0.45 * open_dir
        + 0.20 * (min_tip_dist / seg_len_mm)
    )


def _select_batch(
    candidates_by_slot: list[list[_Candidate]],
    nodes: list[np.ndarray],
    parents: list[int],
    clearance_mm: float,
) -> list[_Candidate]:
    selected: list[_Candidate] = []
    ordered_slots = sorted(
        range(len(candidates_by_slot)),
        key=lambda i: (len(candidates_by_slot[i]), i),
    )

    for slot_idx in ordered_slots:
        slot_candidates = sorted(
            candidates_by_slot[slot_idx],
            key=lambda c: c.score,
            reverse=True,
        )
        for cand in slot_candidates:
            if all(_candidates_are_compatible(cand, prev, nodes, parents, clearance_mm)
                   for prev in selected):
                selected.append(cand)
                break
    return sorted(selected, key=lambda c: c.slot_idx)


def _append_batch(
    selected: list[_Candidate],
    nodes: list[np.ndarray],
    dirs: list[np.ndarray],
    parents: list[int],
) -> list[int]:
    new_indices: list[int] = []
    for cand in selected:
        parents.append(cand.tip_idx)
        nodes.append(cand.end_pos)
        dirs.append(cand.direction)
        new_indices.append(len(nodes) - 1)
    return new_indices


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


def _split_tips_batch(
    active_tips: list[int],
    nodes: list[np.ndarray],
    dirs: list[np.ndarray],
    parents: list[int],
    child_counts: list[int],
    spread_deg: float,
    seg_len_mm: float,
    z_limit: float,
    cfg,
    rng: np.random.Generator,
) -> list[int]:
    candidates_by_slot: list[list[_Candidate]] = []
    slot_idx = 0

    for tip_idx, n_children in zip(active_tips, child_counts):
        phase = float(rng.uniform(0.0, 2.0 * np.pi))
        parent_dir = dirs[tip_idx]
        repulsion = _compute_repulsion(
            tip_idx, active_tips, nodes, seg_len_mm, cfg.repulsion_z_window,
        )
        for child_i in range(n_children):
            child_spread = spread_deg
            if cfg.dominant_branch and child_i == 0:
                child_spread *= cfg.dominant_angle_factor

            slot_candidates: list[_Candidate] = []
            for attempt in range(int(cfg.space_retry_count) + 1):
                child_dir = _direction_from_spread(
                    parent_dir,
                    phase + (child_i + attempt) * _GOLDEN_ANGLE,
                    child_spread,
                    cfg.min_elevation_deg,
                )
                end_pos = nodes[tip_idx] + child_dir * seg_len_mm
                if not _candidate_has_existing_space(
                    tip_idx, end_pos, nodes, parents, z_limit, cfg.space_clearance_mm,
                ):
                    continue
                score = _score_candidate(
                    tip_idx, child_dir, end_pos, active_tips, nodes, dirs,
                    repulsion, seg_len_mm,
                )
                slot_candidates.append(
                    _Candidate(slot_idx, tip_idx, child_dir, end_pos, score)
                )
            candidates_by_slot.append(slot_candidates)
            slot_idx += 1

    return _append_batch(
        _select_batch(candidates_by_slot, nodes, parents, cfg.space_clearance_mm),
        nodes, dirs, parents,
    )


def _grow_step_batch(
    active_tips: list[int],
    nodes: list[np.ndarray],
    dirs: list[np.ndarray],
    parents: list[int],
    seg_len_mm: float,
    z_limit: float,
    cfg,
    wander_deg: float,
    rng: np.random.Generator,
) -> list[int]:
    candidates_by_slot: list[list[_Candidate]] = []
    for slot_idx, tip_idx in enumerate(active_tips):
        repulsion = _compute_repulsion(
            tip_idx, active_tips, nodes, seg_len_mm, cfg.repulsion_z_window,
        )
        slot_candidates: list[_Candidate] = []
        for _attempt in range(int(cfg.space_retry_count) + 1):
            new_dir = _step_direction(
                tip_idx, active_tips, nodes, dirs, seg_len_mm, cfg, wander_deg, rng,
            )
            end_pos = nodes[tip_idx] + new_dir * seg_len_mm
            if not _candidate_has_existing_space(
                tip_idx, end_pos, nodes, parents, z_limit, cfg.space_clearance_mm,
            ):
                continue
            score = _score_candidate(
                tip_idx, new_dir, end_pos, active_tips, nodes, dirs,
                repulsion, seg_len_mm,
            )
            slot_candidates.append(
                _Candidate(slot_idx, tip_idx, new_dir, end_pos, score)
            )
        candidates_by_slot.append(slot_candidates)

    return _append_batch(
        _select_batch(candidates_by_slot, nodes, parents, cfg.space_clearance_mm),
        nodes, dirs, parents,
    )


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
            active = _grow_step_batch(
                active, nodes, dirs, parents, seg_len_mm, z_limit, cfg, wander, rng,
            )
            if not active:
                break
        return active

    active_tips = grow_run(active_tips, int(cfg.n_trunk_segs))

    for level in range(cfg.n_levels):
        child_counts = [
            max(1, int(rng.integers(cfg.split_count_min, cfg.split_count_max + 1)))
            for _tip_idx in active_tips
        ]
        post_split = _split_tips_batch(
            active_tips, nodes, dirs, parents, child_counts,
            spread_angles[level], seg_len_mm, z_limit, cfg, rng,
        )
        active_tips = grow_run(post_split, max(0, n_segs_per_level[level] - 1))
        if not active_tips:
            break

    nodes_xyz = np.array(nodes, dtype=float)
    parents_arr = np.array(parents, dtype=int)
    arc_dists = _compute_arc_dists(nodes_xyz, parents_arr)
    crown_base_z = max(0.1, float(cfg.n_trunk_segs) * seg_len_mm)
    return nodes_xyz, parents_arr, arc_dists, crown_base_z
