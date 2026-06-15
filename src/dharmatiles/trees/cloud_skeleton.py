"""CloudTree skeleton: SCA growth with attractor-coincident leaf nodes.

Invariants
──────────
• Every attractor is a LEAF node.  No attractor is ever a branch point.
• Every branch terminates by landing exactly on an attractor.
• Branching happens at synthetic interior nodes (never at attractor positions).

Algorithm (per branch)
──────────────────────
Each branch owns a set of attractors and a current tip position.

1. Classify owned attractors as primary (within split_angle of main_dir) or
   stray (outside).  Stray clusters spawn sub-branches FROM the current
   synthetic tip, then leave this branch's owned set.

2. If primary reduces to 1 attractor → terminal mode: grow intermediate nodes
   at segment_length_mm intervals, final node lands EXACTLY on the attractor.

3. Otherwise advance one segment toward the primary centroid, repeat.

4. Safety: if the step budget is exhausted before convergence, force-split the
   primary set — keep the nearest attractor as this branch's terminal target,
   hand the rest to a new sub-branch from the current synthetic position.

Radii
─────
Computed bottom-up after the skeleton is complete.  Tips get min_radius_mm,
internal nodes get (Σ r_child^e)^(1/e).  Root radius is fully derived.
"""
from __future__ import annotations

from collections import deque

import numpy as np

from .envelope import TreeEnvelope


def grow_cloud_skeleton(
    env: TreeEnvelope,
    rng: np.random.Generator,
    *,
    n_attraction: int = 200,
    segment_length_mm: float = 2.0,
    kill_radius_mm: float | None = None,   # unused; kept for API compatibility
    min_radius_mm: float = 0.45,
    min_branch_angle_deg: float = 30.0,
    branch_split_angle_deg: float | None = None,
    max_branches_per_step: int = 3,
    branch_exponent: float = 2.5,
    smoothing_alpha: float = 0.25,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    """Grow a CloudTree skeleton filling *env*.

    Returns (nodes, parents, radii, prior_dirs, attractors).
    """
    if branch_split_angle_deg is None:
        branch_split_angle_deg = min_branch_angle_deg

    pts  = _sample_cloud(env, rng, n_attraction)
    root = np.array([env.cx, env.cy, env.terrain_z], dtype=float)

    # Step budget: generous so the trunk + crown branching both complete.
    max_steps = max(60, int(np.ceil(env.height_mm / segment_length_mm) * 4))

    nodes, parents, prior_dirs = _branch_skeleton(
        root         = root,
        pts          = pts,
        seg_len      = float(segment_length_mm),
        split_cos    = float(np.cos(np.radians(branch_split_angle_deg))),
        max_branches = int(max_branches_per_step),
        alpha        = float(smoothing_alpha),
        crown_base_z = float(env.crown_base_z),
        max_steps    = max_steps,
    )

    radii = _compute_radii_bottom_up(parents, branch_exponent, min_radius_mm)

    return (
        np.array(nodes,      dtype=float),
        np.array(parents,    dtype=int),
        radii,
        np.array(prior_dirs, dtype=float),
        pts,
    )


# ─────────────────────────────────────────────────────────────────────────────
# Pass 1: branch skeleton
# ─────────────────────────────────────────────────────────────────────────────

def _branch_skeleton(
    root:         np.ndarray,
    pts:          np.ndarray,
    seg_len:      float,
    split_cos:    float,
    max_branches: int,
    alpha:        float,
    crown_base_z: float,
    max_steps:    int,
) -> tuple[list, list, list]:
    nodes:      list[np.ndarray] = [root.copy()]
    parents:    list[int]        = [-1]
    prior_dirs: list[np.ndarray] = [np.array([0.0, 0.0, 1.0])]

    if len(pts) == 0:
        return nodes, parents, prior_dirs

    # Queue items: (tip_node_idx, owned_attractors, steps_taken)
    queue: deque[tuple[int, np.ndarray, int]] = deque([(0, pts.copy(), 0)])

    while queue:
        tip_idx, owned, steps = queue.popleft()
        if len(owned) == 0:
            continue

        pos     = nodes[tip_idx]
        heading = prior_dirs[tip_idx]

        # Primary direction toward centroid of owned attractors.
        centroid = owned.mean(axis=0)
        raw      = centroid - pos
        raw_len  = float(np.linalg.norm(raw))
        if raw_len > 1e-9:
            raw_dir  = raw / raw_len
            blended  = raw_dir * (1.0 - alpha) + heading * alpha
            b_len    = float(np.linalg.norm(blended))
            main_dir = blended / b_len if b_len > 1e-9 else raw_dir
        else:
            main_dir = heading

        # ── stray detection (crown zone only) ──────────────────────────────
        primary = owned
        if pos[2] >= crown_base_z and len(owned) >= 2 and max_branches > 1:
            to_owned = owned - pos
            unit_to  = to_owned / (np.linalg.norm(to_owned, axis=1, keepdims=True) + 1e-9)
            cos_a    = np.clip(unit_to @ main_dir, -1.0, 1.0)
            # Proactive z-passover: before each step, split off any attractor
            # that would be left on the wrong side in z.  Works in both
            # directions so sub-branches moving downward don't leave upper
            # members behind either.
            next_z = pos[2] + float(main_dir[2]) * seg_len
            if main_dir[2] > 1e-6:        # going up → split anything below next_z
                passover = owned[:, 2] < next_z
            elif main_dir[2] < -1e-6:     # going down → split anything above next_z
                passover = owned[:, 2] > next_z
            else:                          # horizontal → split anything already below
                passover = owned[:, 2] < pos[2]
            stray_mask = (cos_a < split_cos) | passover
            stray      = owned[stray_mask]
            primary    = owned[~stray_mask]

            if len(stray) > 0 and len(primary) > 0:
                # Spawn stray sub-branches from the CURRENT SYNTHETIC position.
                for cluster in _cluster_pca(stray, pos, max_branches - 1):
                    if len(cluster) > 0:
                        queue.append((tip_idx, cluster, 0))

            if len(primary) == 0:
                # All stray: pick one cluster as our primary, rest as sub-branches.
                clusters = _cluster_pca(owned, pos, 2)
                if not clusters:
                    continue
                primary = clusters[0]
                for extra in clusters[1:]:
                    if len(extra) > 0:
                        queue.append((tip_idx, extra, 0))

        # ── terminal or continue ───────────────────────────────────────────
        force_terminal = (len(primary) == 1) or (steps >= max_steps)

        if force_terminal:
            if len(primary) > 1:
                # Safety: keep the nearest attractor as our terminal target.
                dists     = np.linalg.norm(primary - pos, axis=1)
                near_i    = int(np.argmin(dists))
                rest      = np.delete(primary, near_i, axis=0)
                if len(rest) > 0:
                    queue.append((tip_idx, rest, 0))
                primary = primary[near_i : near_i + 1]

            # Grow from synthetic tip to the single target, landing exactly on it.
            _grow_to_leaf(nodes, parents, prior_dirs,
                          tip_idx, primary[0], heading, seg_len, alpha)

        else:
            # Advance one segment toward the primary centroid.
            new_pos = pos + main_dir * seg_len
            new_idx = _add_node(nodes, parents, prior_dirs, new_pos, tip_idx, main_dir)
            queue.append((new_idx, primary, steps + 1))

    return nodes, parents, prior_dirs


def _grow_to_leaf(
    nodes:      list,
    parents:    list,
    prior_dirs: list,
    start_idx:  int,
    target:     np.ndarray,
    heading:    np.ndarray,
    seg_len:    float,
    alpha:      float,
) -> None:
    """Add intermediate synthetic nodes then a final node exactly at *target*."""
    pos        = nodes[start_idx]
    to_t       = target - pos
    dist_total = float(np.linalg.norm(to_t))
    if dist_total < 1e-9:
        return

    dir_to_target = to_t / dist_total
    cur_idx = start_idx
    cur_pos = pos.copy()
    # Start steering directly toward the target so intermediate nodes never
    # walk the wrong way (e.g. upward when the target is below).
    cur_dir = dir_to_target.copy()
    covered = 0.0

    while dist_total - covered > seg_len + 1e-9:
        arrival_dir = cur_dir
        step_pos    = cur_pos + cur_dir * seg_len
        covered    += seg_len
        cur_idx     = _add_node(nodes, parents, prior_dirs, step_pos, cur_idx, arrival_dir)
        cur_pos     = step_pos
        rem         = target - step_pos
        rem_len     = float(np.linalg.norm(rem))
        if rem_len > 1e-9:
            steer   = rem / rem_len
            nd      = steer * (1.0 - alpha) + cur_dir * alpha
            nd_len  = float(np.linalg.norm(nd))
            cur_dir = nd / nd_len if nd_len > 1e-9 else steer

    _add_node(nodes, parents, prior_dirs, target, cur_idx, dir_to_target)


def _add_node(nodes, parents, prior_dirs, pos, parent_idx, direction):
    idx = len(nodes)
    nodes.append(pos.copy())
    parents.append(parent_idx)
    prior_dirs.append(direction.copy())
    return idx


# ─────────────────────────────────────────────────────────────────────────────
# Pass 2: bottom-up pipe-model radii
# ─────────────────────────────────────────────────────────────────────────────

def _compute_radii_bottom_up(
    parents:       list[int] | np.ndarray,
    exponent:      float,
    min_radius_mm: float,
) -> np.ndarray:
    n        = len(parents)
    children: list[list[int]] = [[] for _ in range(n)]
    for i, p in enumerate(parents):
        if p >= 0:
            children[p].append(i)

    radii = np.full(n, min_radius_mm, dtype=float)
    for i in range(n - 1, -1, -1):
        if children[i]:
            radii[i] = float(sum(radii[c] ** exponent for c in children[i])) ** (1.0 / exponent)
    return radii


# ─────────────────────────────────────────────────────────────────────────────
# Shared helpers
# ─────────────────────────────────────────────────────────────────────────────

def _sample_cloud(env: TreeEnvelope, rng: np.random.Generator, n: int) -> np.ndarray:
    """Sample attractors with even coverage over the crown surface."""
    if env.crown_height <= 1e-8 or env.crown_radius_mm <= 1e-8:
        return np.empty((0, 3), dtype=float)
    n = max(0, int(n))
    if n == 0:
        return np.empty((0, 3), dtype=float)

    # Surface area for a surface of revolution: dA = 2*pi*r*sqrt(1+(dr/dz)^2) dz.
    samples = max(257, n * 8)
    ts = np.linspace(0.0, 1.0, samples)
    zs = env.crown_base_z + ts * env.crown_height
    rs = np.asarray(env.radius_at_t(ts), dtype=float)
    dr_dz = np.gradient(rs, zs, edge_order=2)
    density = 2.0 * np.pi * rs * np.sqrt(1.0 + dr_dz * dr_dz)
    density[~np.isfinite(density)] = 0.0
    density = np.maximum(density, 0.0)

    cumulative = np.zeros_like(ts)
    cumulative[1:] = np.cumsum(0.5 * (density[:-1] + density[1:]) * np.diff(zs))
    total_area = float(cumulative[-1])
    if total_area <= 1e-9:
        return np.empty((0, 3), dtype=float)

    area_targets = (np.arange(n, dtype=float) + rng.random(n)) / n * total_area
    z = np.interp(area_targets, cumulative, zs)
    r = np.asarray(env.radius_at_z(z), dtype=float)

    theta_step = np.pi * (3.0 - np.sqrt(5.0))
    theta = np.arange(n, dtype=float) * theta_step + rng.uniform(0.0, 2.0 * np.pi)
    theta += rng.uniform(-0.5, 0.5, n) * theta_step

    return np.column_stack([
        env.cx + r * np.cos(theta),
        env.cy + r * np.sin(theta),
        z,
    ])


def _cluster_pca(pts: np.ndarray, origin: np.ndarray, max_k: int) -> list[np.ndarray]:
    if max_k <= 0 or len(pts) == 0:
        return []
    if max_k == 1 or len(pts) < 2:
        return [pts]
    dirs     = pts - origin
    unit     = dirs / (np.linalg.norm(dirs, axis=1, keepdims=True) + 1e-9)
    centered = unit - unit.mean(axis=0)
    try:
        _, _, vh = np.linalg.svd(centered, full_matrices=False)
    except np.linalg.LinAlgError:
        return [pts]
    proj = centered @ vh[0]
    return [c for c in [pts[proj <= 0.0], pts[proj > 0.0]] if len(c) > 0][:max_k]
