"""
Constructive tree skeleton growth.

Strategy: at each branching level, target tip positions are distributed
evenly across the crown cross-section using a Fibonacci (sunflower) disc
pattern.  Branches grow from the current active tips toward those target
positions, filling the canopy area evenly.

At each level the disc radius is the *cumulative* horizontal reach of all
branches grown so far from the tree centre:

    crown_r[level] = Σ (n_segs[k] * seg_len * sin(spread[k]))  for k ≤ level

The disc height is the corresponding vertical reach:

    crown_z[level] = tz + trunk_height + Σ (n_segs[k] * seg_len * cos(spread[k]))

This means the straight-line distance from any Fibonacci-disc edge target to
its parent tip is exactly ``n_segs[level] * seg_len``, so a branch grown at
the spread angle will reach its target with zero wander.  Inner targets are
closer and the branch overshoots slightly in the same direction — still an
even distribution.

Public API
----------
``grow_const_skeleton(cx, cy, tz, cfg, rng)``
    Returns ``(nodes_xyz, parents, arc_dists, crown_base_z)`` matching the
    SCA skeleton API consumed by ``trees/tree.py``.
"""
from __future__ import annotations

import numpy as np

from ..dist import sample
from .skeleton import _compute_arc_dists


_UP = np.array([0.0, 0.0, 1.0])
_GOLDEN_RATIO = (1.0 + np.sqrt(5.0)) / 2.0


# ---------------------------------------------------------------------------
# Direction helpers (shared with old implementation — keep identical)
# ---------------------------------------------------------------------------

def _normalize(v: np.ndarray, fallback: np.ndarray | None = None) -> np.ndarray:
    n = float(np.linalg.norm(v))
    if n > 1e-10:
        return v / n
    return fallback.copy() if fallback is not None else _UP.copy()


def _basis_from_dir(w: np.ndarray) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    w = _normalize(w)
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


def _wander_direction(
    base: np.ndarray,
    wander_deg: float,
    min_elevation_deg: float,
    rng: np.random.Generator,
) -> np.ndarray:
    u, v, _w = _basis_from_dir(base)
    sigma = float(np.sin(np.radians(wander_deg)))
    jitter = rng.normal(0.0, sigma, 2)
    return _clamp_elevation(base + jitter[0] * u + jitter[1] * v, min_elevation_deg)


def _append_segment(
    tip_idx: int,
    direction: np.ndarray,
    nodes: list[np.ndarray],
    dirs: list[np.ndarray],
    parents: list[int],
    seg_len_mm: float,
) -> int:
    parents.append(tip_idx)
    nodes.append(nodes[tip_idx] + direction * seg_len_mm)
    dirs.append(direction)
    return len(nodes) - 1


def _as_level_list(value, n_levels: int, name: str) -> list:
    if isinstance(value, (list, tuple)):
        if len(value) != n_levels:
            raise ValueError(f"{name} must have {n_levels} entries")
        return list(value)
    return [value] * n_levels


def _sample_level_values(value, n_levels: int, name: str, rng: np.random.Generator) -> list[float]:
    return [float(sample(v, rng)) for v in _as_level_list(value, n_levels, name)]


# ---------------------------------------------------------------------------
# Crown disc: Fibonacci (sunflower) target distribution
# ---------------------------------------------------------------------------

def _fibonacci_disc_targets(
    cx: float,
    cy: float,
    z: float,
    radius: float,
    n: int,
    phase: float,
    rng: np.random.Generator,
    jitter_frac: float = 0.25,
) -> np.ndarray:
    """Return (n, 3) positions distributed evenly in a disc of radius *radius*.

    Uses the sunflower / Fibonacci spiral (irrational angular step = 2π/φ²)
    so n points of any count are optimally spread.  A random *phase* rotates
    the whole pattern; small Gaussian jitter adds organic variation.
    """
    if n <= 0:
        return np.empty((0, 3), dtype=float)
    if n == 1:
        # Single branch: centre of disc (grows straight up from trunk)
        return np.array([[cx, cy, z]], dtype=float)

    i = np.arange(n, dtype=float)
    r = radius * np.sqrt((i + 0.5) / n)         # even-area radial spacing
    theta = 2.0 * np.pi * i / _GOLDEN_RATIO ** 2 + phase

    x = cx + r * np.cos(theta)
    y = cy + r * np.sin(theta)

    if jitter_frac > 0.0:
        avg_spacing = radius / float(np.sqrt(max(n, 1)))
        sigma = jitter_frac * avg_spacing
        x = x + rng.normal(0.0, sigma, n)
        y = y + rng.normal(0.0, sigma, n)

    return np.stack([x, y, np.full(n, z)], axis=1).astype(float)


# ---------------------------------------------------------------------------
# Nearest-neighbour target assignment (balanced by desired child counts)
# ---------------------------------------------------------------------------

def _assign_targets_to_parents(
    targets_xy: np.ndarray,
    parent_xy: np.ndarray,
    desired_counts: list[int],
) -> list[list[int]]:
    """Assign each target to the nearest parent that still has capacity.

    *desired_counts[p]* is the number of children parent *p* must receive.
    Targets are processed in ascending distance order (greedy), so each
    parent claims the closest available targets first.
    """
    n_targets = len(targets_xy)
    n_parents = len(parent_xy)
    if n_targets == 0 or n_parents == 0:
        return [[] for _ in range(n_parents)]

    # All (dist², target, parent) pairs, sorted ascending
    pairs: list[tuple[float, int, int]] = []
    for t in range(n_targets):
        for p in range(n_parents):
            dx = float(targets_xy[t, 0] - parent_xy[p, 0])
            dy = float(targets_xy[t, 1] - parent_xy[p, 1])
            pairs.append((dx * dx + dy * dy, t, p))
    pairs.sort()

    assignments: list[list[int]] = [[] for _ in range(n_parents)]
    remaining = list(desired_counts)
    taken = [False] * n_targets

    for _d2, t, p in pairs:
        if taken[t] or remaining[p] <= 0:
            continue
        assignments[p].append(t)
        taken[t] = True
        remaining[p] -= 1

    return assignments


# ---------------------------------------------------------------------------
# Branch growth: each segment aims at the start→target direction + wander
# ---------------------------------------------------------------------------

def _grow_toward_target(
    start_idx: int,
    target: np.ndarray,
    n_segs: int,
    seg_len_mm: float,
    wander_deg: float,
    min_elevation_deg: float,
    nodes: list[np.ndarray],
    dirs: list[np.ndarray],
    parents: list[int],
    rng: np.random.Generator,
) -> int:
    """Grow *n_segs* segments from *nodes[start_idx]* toward *target*.

    The base direction (start → target straight line) is fixed for the whole
    branch run; only per-segment wander noise varies.  This keeps the branch
    aimed at its target without orbiting or overshooting.

    Returns the index of the final node grown.
    """
    base_dir = _clamp_elevation(
        _normalize(target - nodes[start_idx]), min_elevation_deg,
    )
    current = start_idx
    for _ in range(n_segs):
        direction = _wander_direction(base_dir, wander_deg, min_elevation_deg, rng)
        current = _append_segment(current, direction, nodes, dirs, parents, seg_len_mm)
    return current


# ---------------------------------------------------------------------------
# Public entry point
# ---------------------------------------------------------------------------

def grow_const_skeleton(
    cx: float,
    cy: float,
    tz: float,
    cfg,
    rng: np.random.Generator,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, float]:
    """Grow a constructive skeleton from a single ground-level root.

    Algorithm
    ---------
    1. Derive *seg_len_mm* from *height_max_mm* / total-segment-count so the
       tree reaches the requested height when growing straight up.
    2. Grow *n_trunk_segs* bare-trunk segments upward with wander.
    3. For each branching level:
       a. Sample the number of children each active tip will produce.
       b. Place that many target positions using a Fibonacci disc centred at
          (cx, cy) — the crown cross-section at the height and radius each
          branch can reach in *n_segs_per_level[level]* steps.
       c. Assign targets to parents by nearest-XY greedy matching.
       d. Grow *n_segs_per_level[level]* segments from each parent toward its
          assigned targets.
       e. The final nodes of those paths become the next active tips.

    Returns
    -------
    nodes_xyz, parents_arr, arc_dists, crown_base_z
        Same interface as the SCA skeleton; consumed by trees/tree.py.
    """
    height_max = float(sample(cfg.height_max_mm, rng))
    if height_max <= 0.0:
        root = np.array([[cx, cy, tz]], dtype=float)
        return root, np.array([-1], dtype=int), np.zeros(1), 0.0

    n_segs_per_level = [int(v) for v in _as_level_list(
        cfg.n_segs_per_level, cfg.n_levels, "n_segs_per_level",
    )]
    total_segments = int(cfg.n_trunk_segs) + sum(n_segs_per_level)
    seg_len_mm = height_max / max(1, total_segments)

    spread_angles = _sample_level_values(
        cfg.spread_angle_deg, cfg.n_levels, "spread_angle_deg", rng,
    )
    wander = float(sample(cfg.wander_deg, rng))
    lean   = float(sample(cfg.initial_lean_deg, rng))

    # ── Crown disc geometry (one disc per branching level) ────────────────────
    #
    # For level k, branches grow n_segs_per_level[k] steps at spread_angles[k]
    # from vertical.  The target disc sits at the expected tip position:
    #
    #   horizontal reach from parent:  n_segs * seg_len * sin(spread)
    #   vertical reach from parent:    n_segs * seg_len * cos(spread)
    #
    # crown_r is cumulative (parents at crown_r[k-1], tips at crown_r[k]).
    # The geometry ensures that the distance from an edge parent to its
    # furthest-assigned target equals exactly n_segs * seg_len.
    crown_z: list[float] = []
    crown_r: list[float] = []
    z_acc = tz + int(cfg.n_trunk_segs) * seg_len_mm
    r_acc = 0.0
    for level in range(cfg.n_levels):
        dl = n_segs_per_level[level] * seg_len_mm
        theta = float(np.radians(spread_angles[level]))
        r_acc += dl * float(np.sin(theta))
        z_acc += dl * float(np.cos(theta))
        crown_r.append(r_acc)
        crown_z.append(z_acc)

    # ── Initialise skeleton ───────────────────────────────────────────────────
    lean_rad = float(np.radians(lean))
    lean_az  = float(rng.uniform(0.0, 2.0 * np.pi))
    root_dir = np.array([
        np.sin(lean_rad) * np.cos(lean_az),
        np.sin(lean_rad) * np.sin(lean_az),
        np.cos(lean_rad),
    ]) if lean_rad > 1e-6 else _UP.copy()

    nodes:   list[np.ndarray] = [np.array([cx, cy, tz], dtype=float)]
    parents: list[int]        = [-1]
    dirs:    list[np.ndarray] = [root_dir]

    # ── Trunk ─────────────────────────────────────────────────────────────────
    trunk_tip = 0
    for _ in range(int(cfg.n_trunk_segs)):
        d = _wander_direction(dirs[trunk_tip], wander, cfg.min_elevation_deg, rng)
        trunk_tip = _append_segment(trunk_tip, d, nodes, dirs, parents, seg_len_mm)

    active_tips: list[int] = [trunk_tip]

    # ── Level-by-level branching ──────────────────────────────────────────────
    # Phase advances by an irrational fraction each level so consecutive crown
    # discs are not rotationally aligned.
    crown_phase = float(rng.uniform(0.0, 2.0 * np.pi))

    for level in range(cfg.n_levels):
        n_segs = n_segs_per_level[level]
        if n_segs <= 0 or not active_tips:
            break

        # Each active tip forks into 2–split_count_max children
        n_children = [
            max(1, int(rng.integers(cfg.split_count_min, cfg.split_count_max + 1)))
            for _ in active_tips
        ]
        n_total = sum(n_children)

        # Distribute targets evenly in the crown cross-section at this level
        targets = _fibonacci_disc_targets(
            cx, cy, crown_z[level], crown_r[level], n_total, crown_phase, rng,
        )

        # Assign targets to their nearest parent (capacity-bounded)
        parent_xy = np.array(
            [[nodes[i][0], nodes[i][1]] for i in active_tips], dtype=float,
        )
        assignments = _assign_targets_to_parents(
            targets[:, :2], parent_xy, n_children,
        )

        # Grow branches toward targets; collect the new tips
        new_tips: list[int] = []
        for slot, tip_idx in enumerate(active_tips):
            for t_idx in assignments[slot]:
                final = _grow_toward_target(
                    tip_idx, targets[t_idx], n_segs, seg_len_mm,
                    wander, cfg.min_elevation_deg, nodes, dirs, parents, rng,
                )
                new_tips.append(final)

        active_tips = new_tips
        crown_phase += 2.0 * np.pi / _GOLDEN_RATIO   # irrational advance
        if not active_tips:
            break

    # ── Assemble output ───────────────────────────────────────────────────────
    nodes_xyz   = np.array(nodes, dtype=float)
    parents_arr = np.array(parents, dtype=int)
    arc_dists   = _compute_arc_dists(nodes_xyz, parents_arr)
    crown_base_z = float(tz + max(0.1, int(cfg.n_trunk_segs) * seg_len_mm))
    return nodes_xyz, parents_arr, arc_dists, crown_base_z
