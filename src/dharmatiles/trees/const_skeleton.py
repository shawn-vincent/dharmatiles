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


def _append_node_at(
    tip_idx: int,
    position: np.ndarray,
    nodes: list[np.ndarray],
    dirs: list[np.ndarray],
    parents: list[int],
) -> int:
    """Place a node exactly at *position* as a child of *tip_idx*."""
    direction = _normalize(position - nodes[tip_idx], _UP)
    parents.append(tip_idx)
    nodes.append(position.copy())
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
# Angular-sector zone helpers
# ---------------------------------------------------------------------------

def _compute_parent_zones(
    parent_xy: np.ndarray,
    center: np.ndarray,
) -> list[tuple[float, float]]:
    """Return (zone_center_angle, zone_half_width) for each parent.

    Each parent owns the arc from the midpoint between it and its
    counter-clockwise predecessor to the midpoint between it and its
    successor.  With n parents equally spaced at 120° the zones are
    equal 120°-wide wedges; unequal spacing gives proportionally unequal
    zones.  A single parent owns the full circle (zone_half = π).
    """
    n = len(parent_xy)
    if n == 1:
        return [(0.0, np.pi)]

    offsets = parent_xy - center
    angles  = np.arctan2(offsets[:, 1], offsets[:, 0])
    order   = np.argsort(angles)
    sa      = angles[order]            # sorted angles

    zones_sorted: list[tuple[float, float]] = []
    for i in range(n):
        prev_a = sa[(i - 1) % n]
        curr_a = sa[i]
        next_a = sa[(i + 1) % n]

        arc_prev = (curr_a - prev_a) % (2.0 * np.pi)   # CCW arc from prev to curr
        arc_next = (next_a - curr_a) % (2.0 * np.pi)   # CCW arc from curr to next

        zone_half   = (arc_prev + arc_next) / 4.0
        zone_center = curr_a + (arc_next - arc_prev) / 4.0
        zones_sorted.append((float(zone_center), float(zone_half)))

    zones: list[tuple[float, float]] = [(0.0, 0.0)] * n
    for rank, orig in enumerate(order):
        zones[orig] = zones_sorted[rank]
    return zones


def _clamp_to_zone(
    pt:          np.ndarray,
    center:      np.ndarray,
    radius:      float,
    zone_center: float,
    zone_half:   float,
) -> np.ndarray:
    """Clamp a 2-D point to a disc-wedge: radius ≤ *radius*, angle within zone."""
    off = pt - center
    r   = float(np.linalg.norm(off))

    if r < 1e-8:
        # Exactly at centre: push to zone centre at a safe radius
        r = radius * 0.1
        return center + np.array([np.cos(zone_center), np.sin(zone_center)]) * r

    # Clamp radius
    if r > radius:
        off = off / r * radius
        r   = radius

    # Clamp angle into [zone_center - zone_half, zone_center + zone_half]
    angle = float(np.arctan2(off[1], off[0]))
    diff  = (angle - zone_center + np.pi) % (2.0 * np.pi) - np.pi   # in (-π, π]
    if abs(diff) > zone_half:
        clamped_angle = zone_center + np.sign(diff) * zone_half
        off = np.array([np.cos(clamped_angle), np.sin(clamped_angle)]) * r

    return center + off


# ---------------------------------------------------------------------------
# Crown disc: repulsion-based target distribution with per-child zones
# ---------------------------------------------------------------------------

def _repulsion_disc_targets(
    cx:         float,
    cy:         float,
    z:          float,
    radius:     float,
    n:          int,
    seed_xy:    np.ndarray,
    rng:        np.random.Generator,
    zones:      list[tuple[float, float]] | None = None,
    max_iters:  int = 400,
) -> np.ndarray:
    """Place n points in a disc by repulsion from seed positions.

    Each point starts at its row in *seed_xy* (the parent tip's XY).
    Points repel each other via position-based constraint projection and
    are hard-clamped to their disc-wedge zone after every iteration.
    The loop exits as soon as no pair violates the minimum spacing —
    minimum total displacement.

    *zones* is a list of (zone_center, zone_half) per child.  ``None``
    means all children share the full circle (single-parent case).

    Returns (n, 3) with the given z coordinate.
    """
    if n <= 0:
        return np.empty((0, 3), dtype=float)

    center = np.array([cx, cy], dtype=float)
    full_circle = zones is None

    if n == 1:
        pt = seed_xy[0].copy().astype(float)
        zc, zh = (0.0, np.pi) if full_circle else zones[0]
        pt = _clamp_to_zone(pt, center, radius, zc, zh)
        return np.array([[pt[0], pt[1], z]], dtype=float)

    # target spacing: optimal packing estimate
    target_spacing = radius * 1.8 / float(np.sqrt(n))
    ts2 = target_spacing * target_spacing

    # Initialise: clamp each seed to its zone
    pts = seed_xy[:n].copy().astype(float)
    for k in range(n):
        zc, zh = (0.0, np.pi) if full_circle else zones[k]
        pts[k] = _clamp_to_zone(pts[k], center, radius, zc, zh)

    # Tiny jitter breaks exact coincidence
    pts += rng.normal(0.0, radius * 5e-4, pts.shape)

    for _ in range(max_iters):
        any_violated = False

        for i in range(n):
            for j in range(i + 1, n):
                diff = pts[i] - pts[j]
                d2   = float(diff[0] * diff[0] + diff[1] * diff[1])
                if d2 >= ts2:
                    continue
                any_violated = True
                d = float(np.sqrt(d2))
                if d < 1e-10:
                    ang  = float(rng.uniform(0.0, 2.0 * np.pi))
                    diff = np.array([np.cos(ang), np.sin(ang)])
                    d    = 1.0
                direction = diff / d
                push      = (target_spacing - d) * 0.5
                pts[i] += direction * push
                pts[j] -= direction * push

        # Hard clamp every point to its zone
        for k in range(n):
            zc, zh = (0.0, np.pi) if full_circle else zones[k]
            pts[k] = _clamp_to_zone(pts[k], center, radius, zc, zh)

        if not any_violated:
            break

    return np.stack([pts[:, 0], pts[:, 1], np.full(n, z)], axis=1).astype(float)


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

        # Compute one angular zone per parent so children from different
        # parents can never cross: each zone is a disc wedge whose angular
        # boundaries bisect the arcs between adjacent parent angles.
        parent_xy = np.array(
            [[nodes[tip_idx][0], nodes[tip_idx][1]] for tip_idx in active_tips],
            dtype=float,
        )
        center2d = np.array([cx, cy], dtype=float)
        parent_zones = _compute_parent_zones(parent_xy, center2d)

        # Each child inherits its parent's zone
        child_zones = [
            parent_zones[slot]
            for slot, count in enumerate(n_children)
            for _ in range(count)
        ]

        # Seed each child at its parent's XY, then spread by repulsion
        seed_xy = np.array(
            [nodes[tip_idx][:2] for tip_idx, count in zip(active_tips, n_children)
             for _ in range(count)],
            dtype=float,
        )
        targets = _repulsion_disc_targets(
            cx, cy, crown_z[level], crown_r[level], n_total, seed_xy, rng,
            zones=child_zones,
        )

        # Assignments are fixed by construction — target k belongs to whichever
        # parent seeded it; no re-assignment needed (zones enforce non-crossing).
        assignments: list[list[int]] = [[] for _ in range(len(active_tips))]
        k = 0
        for slot, count in enumerate(n_children):
            assignments[slot] = list(range(k, k + count))
            k += count

        # Place one node directly at each target — the surface renderer
        # curves each single-segment edge with a Hermite cubic.
        new_tips: list[int] = []
        for slot, tip_idx in enumerate(active_tips):
            for t_idx in assignments[slot]:
                final = _append_node_at(tip_idx, targets[t_idx], nodes, dirs, parents)
                new_tips.append(final)

        active_tips = new_tips
        if not active_tips:
            break

    # ── Assemble output ───────────────────────────────────────────────────────
    nodes_xyz   = np.array(nodes, dtype=float)
    parents_arr = np.array(parents, dtype=int)
    arc_dists   = _compute_arc_dists(nodes_xyz, parents_arr)
    crown_base_z = float(tz + max(0.1, int(cfg.n_trunk_segs) * seg_len_mm))
    return nodes_xyz, parents_arr, arc_dists, crown_base_z
