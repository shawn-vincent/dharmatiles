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


def _end_profile(u: float, pointiness: float, curve: float) -> float:
    """Relative radius moving inward from a crown endpoint.

    ``u`` is 0 at the endpoint and 1 at the widest part.  ``pointiness``
    blends between a round quarter-arc and a strict linear taper; ``curve``
    controls how quickly that endpoint reaches full width.
    """
    u = float(np.clip(u, 0.0, 1.0))
    p = float(np.clip(pointiness, 0.0, 1.0))
    c = max(0.01, float(curve))

    linear = u ** c
    round_arc = np.sin(0.5 * np.pi * u) ** c
    return float((1.0 - p) * round_arc + p * linear)


def _crown_profile(
    t: float,
    bottom_pointiness: float,
    bottom_curve: float,
    top_pointiness: float,
    top_curve: float,
) -> float:
    """Normalised crown-radius profile for t in [0, 1].

    The bottom and top endpoint profiles meet by taking the smaller envelope,
    then a dense normalisation pass makes ``crown_radius_mm`` the actual maximum
    width regardless of asymmetric endpoint settings.
    """
    if t <= 0.0 or t >= 1.0:
        return 0.0
    bottom = _end_profile(t, bottom_pointiness, bottom_curve)
    top = _end_profile(1.0 - t, top_pointiness, top_curve)
    raw = min(bottom, top)

    samples = np.linspace(0.0, 1.0, 257)
    vals = np.minimum(
        [_end_profile(s, bottom_pointiness, bottom_curve) for s in samples],
        [_end_profile(1.0 - s, top_pointiness, top_curve) for s in samples],
    )
    raw_peak = float(np.max(vals))
    if raw_peak < 1e-12:
        return 0.0
    return float(raw / raw_peak)


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
    1. Split total height into an explicit bare trunk and the remaining crown.
       If ``trunk_height_mm`` is unset, the legacy ``crown_height_fraction``
       derives the trunk height.  Trunk grows ``n_trunk_segs`` wandering
       segments.
    2. Crown levels are evenly spaced in height.  The target disc radius at
       each level is read from the top/bottom pointiness+curve profile scaled
       by *crown_radius_mm*, where t is the normalised height within the crown.
    3. For each branching level, parents fork and aim at repulsion-spread
       targets on the level's crown disc.  *branch_stagger* spreads the
       branching events across the level's vertical range.

    Returns
    -------
    nodes_xyz, parents_arr, arc_dists, crown_base_z
        Same interface as the SCA skeleton; consumed by trees/tree.py.
    """
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
    crown_height  = max(0.0, height_max - trunk_height)
    seg_len_mm    = trunk_height / max(1, int(cfg.n_trunk_segs))
    crown_base_z  = tz + trunk_height

    wander = float(sample(cfg.wander_deg, rng))
    lean   = float(sample(cfg.initial_lean_deg, rng))

    # ── Crown disc geometry: one disc per branching level ────────────────────
    # Heights are evenly spaced within the crown; radii come from the smooth
    # beta-distribution profile so the shape is purely a function of height.
    crown_z: list[float] = []
    crown_r: list[float] = []
    for level in range(cfg.n_levels):
        t = (level + 1.0) / cfg.n_levels
        crown_z.append(crown_base_z + t * crown_height)
        crown_r.append(crown_radius * _crown_profile(
            t,
            float(cfg.bottom_pointiness),
            float(cfg.bottom_curve),
            float(cfg.top_pointiness),
            float(cfg.top_curve),
        ))

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
    branch_stagger = float(getattr(cfg, 'branch_stagger', 0.0))

    # ── Level-by-level branching ──────────────────────────────────────────────
    for level in range(cfg.n_levels):
        if not active_tips:
            break

        n_parents = len(active_tips)

        # Target disc: always the full crown disc for this level, computed once.
        # This is independent of stagger — the area children are placed in
        # depends only on the tree geometry at this level, not on when each
        # parent happens to branch.
        n_children = [
            max(1, int(rng.integers(cfg.split_count_min, cfg.split_count_max + 1)))
            for _ in active_tips
        ]
        n_total = sum(n_children)

        parent_xy = np.array(
            [[nodes[tip_idx][0], nodes[tip_idx][1]] for tip_idx in active_tips],
            dtype=float,
        )
        center2d = np.array([cx, cy], dtype=float)
        parent_zones = _compute_parent_zones(parent_xy, center2d)

        child_zones = [
            parent_zones[slot]
            for slot, count in enumerate(n_children)
            for _ in range(count)
        ]
        seed_xy = np.array(
            [nodes[tip_idx][:2] for tip_idx, count in zip(active_tips, n_children)
             for _ in range(count)],
            dtype=float,
        )
        targets = _repulsion_disc_targets(
            cx, cy, crown_z[level], crown_r[level], n_total, seed_xy, rng,
            zones=child_zones,
        )

        # Target assignment: fixed by construction (zone → parent slot)
        assignments: list[list[int]] = [[] for _ in range(n_parents)]
        k = 0
        for slot, count in enumerate(n_children):
            assignments[slot] = list(range(k, k + count))
            k += count

        # Stagger: spread branch events across the level's vertical range.
        # n_groups=1 (stagger=0) reproduces the original simultaneous branch.
        n_groups = max(1, round(branch_stagger * n_parents))
        slot_order = rng.permutation(n_parents)
        group_of_slot = np.zeros(n_parents, dtype=int)
        for i, slot in enumerate(slot_order):
            group_of_slot[slot] = int(i * n_groups / n_parents)

        z_base = crown_z[level - 1] if level > 0 else crown_base_z
        current_tips = list(active_tips)
        new_tips: list[int] = []

        for g in range(n_groups):
            # Sub-level height at which this group's parents branch
            z_g = z_base + (g + 1.0) / n_groups * (crown_z[level] - z_base)

            branching_slots = [s for s in range(n_parents) if group_of_slot[s] == g]
            continuing_slots = [s for s in range(n_parents) if group_of_slot[s] > g]

            # Advance non-branching parents straight up to z_g so they are
            # at the right height when their own group is reached
            for s in continuing_slots:
                tip_idx = current_tips[s]
                adv_pos = np.array(
                    [nodes[tip_idx][0], nodes[tip_idx][1], z_g], dtype=float,
                )
                current_tips[s] = _append_node_at(tip_idx, adv_pos, nodes, dirs, parents)

            # Branch: each branching parent jumps straight to its pre-computed
            # target on the full level crown disc (same targets as stagger=0)
            for slot in branching_slots:
                for t_idx in assignments[slot]:
                    final = _append_node_at(
                        current_tips[slot], targets[t_idx], nodes, dirs, parents,
                    )
                    new_tips.append(final)

        active_tips = new_tips
        if not active_tips:
            break

    # ── Assemble output ───────────────────────────────────────────────────────
    nodes_xyz    = np.array(nodes, dtype=float)
    parents_arr  = np.array(parents, dtype=int)
    arc_dists    = _compute_arc_dists(nodes_xyz, parents_arr)
    return nodes_xyz, parents_arr, arc_dists, float(max(tz + 0.1, crown_base_z))
