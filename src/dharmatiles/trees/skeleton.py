"""
Tree skeleton growth via Space Colonization Algorithm (Runions 2007).

Starting from a single ground-level root, SCA grows upward through an
attractor-free zone — this path *is* the trunk, emerging naturally because
no attractors exist below ``crown_base_z_mm``.  Above that threshold the
skeleton fans into the crown ellipsoid, producing the branch structure.

Public API
----------
``grow_skeleton(cx, cy, tz, cfg, rng)``
    Returns ``(nodes_xyz, parents, arc_dists, crown_base_z)``
    where all arrays are (N,) / (N,3) and ``crown_base_z`` is the sampled
    height above terrain at which attractors begin.
"""
from __future__ import annotations

import numpy as np

from ..dist import sample


# ── Attractor sampling ────────────────────────────────────────────────────────

def _sample_in_ellipsoid(
    center: np.ndarray,
    rx: float,
    ry: float,
    rz: float,
    n: int,
    rng: np.random.Generator,
) -> np.ndarray:
    """Rejection-sample *n* points uniformly inside an axis-aligned ellipsoid."""
    pts: list[np.ndarray] = []
    while len(pts) < n:
        batch  = rng.uniform(-1.0, 1.0, (n * 4, 3))
        inside = (batch[:, 0] ** 2 + batch[:, 1] ** 2 + batch[:, 2] ** 2) <= 1.0
        pts.extend(batch[inside])
    arr = np.array(pts[:n])
    arr[:, 0] *= rx
    arr[:, 1] *= ry
    arr[:, 2] *= rz
    return arr + center


# ── Arc-distance computation ──────────────────────────────────────────────────

def _compute_arc_dists(
    nodes_xyz: np.ndarray,
    parents:   np.ndarray,
) -> np.ndarray:
    """Cumulative arc distance from the root to each node."""
    N    = len(nodes_xyz)
    arc  = np.zeros(N)
    for i in range(1, N):
        p = int(parents[i])
        if p >= 0:
            arc[i] = arc[p] + float(np.linalg.norm(nodes_xyz[i] - nodes_xyz[p]))
    return arc


# ── FDM elevation clamp ───────────────────────────────────────────────────────

def _clamp_elevation(g: np.ndarray, min_z: float) -> np.ndarray:
    """Return a unit vector with g[2] >= min_z (hard FDM overhang floor).

    ``min_z = sin(radians(sca_min_elevation))`` so a value of 0.707 enforces
    ≥ 45° above horizontal.  When the raw growth direction falls below the
    floor the Z component is raised to min_z and the vector is re-normalised.
    If the result would be zero (min_z ≥ 1) the vector is set to straight up.
    """
    if g[2] >= min_z:
        return g
    g    = g.copy()
    g[2] = min_z
    gn   = float(np.linalg.norm(g))
    return g / gn if gn > 1e-8 else np.array([0., 0., 1.])


# ── Core SCA loop ─────────────────────────────────────────────────────────────

def _sca_grow(
    root_positions: np.ndarray,   # (N_roots, 3)
    att:            np.ndarray,   # (K, 3) — attractors
    cfg,
    rng:            np.random.Generator,
) -> tuple[np.ndarray, np.ndarray]:
    """Run SCA; return ``(nodes_xyz (N,3), parents (N,) int)``.

    Tips split when their visible attractors show sufficient XY spread
    (> ``sca_branch_xy_std``) and both clusters have >= ``sca_min_branch_att``
    members.  Every growth direction is clamped to at least
    ``sca_min_elevation`` degrees above horizontal so no segment falls below
    the FDM overhang threshold.
    """
    n_roots = len(root_positions)
    if len(att) == 0:
        return root_positions.copy(), np.full(n_roots, -1, dtype=int)

    node_xyz: list[np.ndarray] = [r.copy() for r in root_positions]
    parents:  list[int]        = [-1] * n_roots
    tips:     set[int]         = set(range(n_roots))
    tropism   = np.array([0.0, 0.0, cfg.sca_tropism], dtype=float)
    min_z     = float(np.sin(np.radians(cfg.sca_min_elevation)))

    for _step in range(cfg.sca_max_steps):
        if att.shape[0] == 0 or not tips:
            break

        nodes_arr = np.array(node_xyz)                              # (M, 3)
        diff      = att[:, None, :] - nodes_arr[None, :, :]        # (K, M, 3)
        dist2     = (diff * diff).sum(axis=-1)                      # (K, M)
        nearest   = np.argmin(dist2, axis=-1)                       # (K,)
        near_d2   = dist2[np.arange(len(att)), nearest]            # (K,)
        in_range  = near_d2 < (cfg.sca_perception_r ** 2)

        new_nodes: list[tuple[np.ndarray, int]] = []
        for tip_idx in sorted(tips):
            mask = in_range & (nearest == tip_idx)
            if not np.any(mask):
                # No attractors in perception range: grow toward the nearest
                # attractor (+ tropism).  This drives the tip through the
                # attractor-free trunk zone until it enters the crown.
                tip_pos  = nodes_arr[tip_idx]
                tip_diffs = att - tip_pos                           # (K, 3)
                tip_d2   = (tip_diffs * tip_diffs).sum(axis=-1)
                nn_idx   = int(np.argmin(tip_d2))
                direction = tip_diffs[nn_idx]
                dn        = float(np.linalg.norm(direction))
                if dn < 1e-8:
                    continue
                g  = direction / dn + tropism
                gn = float(np.linalg.norm(g))
                g  = g / gn if gn > 1e-8 else np.array([0., 0., 1.])
                g  = _clamp_elevation(g, min_z)
                new_nodes.append(
                    (tip_pos + g * cfg.sca_segment_mm, tip_idx)
                )
                continue

            dirs   = diff[mask, tip_idx, :]                         # (K_local, 3)
            norms  = np.sqrt((dirs * dirs).sum(axis=-1, keepdims=True))
            dirs_n = dirs / np.maximum(norms, 1e-8)

            # ── Branching detection ────────────────────────────────────────
            branched  = False
            n_local   = len(dirs_n)
            if n_local >= 2 * cfg.sca_min_branch_att:
                xy_std = float(dirs_n[:, :2].std())
                if xy_std > cfg.sca_branch_xy_std:
                    x_std = float(dirs_n[:, 0].std())
                    y_std = float(dirs_n[:, 1].std())
                    col   = 0 if x_std >= y_std else 1
                    split = float(np.median(dirs_n[:, col]))

                    mask_a = dirs_n[:, col] >= split
                    mask_b = ~mask_a

                    if (np.sum(mask_a) >= cfg.sca_min_branch_att
                            and np.sum(mask_b) >= cfg.sca_min_branch_att):
                        for submask in (mask_a, mask_b):
                            g  = dirs_n[submask].mean(axis=0) + tropism
                            gn = np.linalg.norm(g)
                            g  = g / gn if gn > 1e-8 else np.array([0., 0., 1.])
                            g  = _clamp_elevation(g, min_z)
                            new_nodes.append(
                                (nodes_arr[tip_idx] + g * cfg.sca_segment_mm, tip_idx)
                            )
                        branched = True

            if not branched:
                # ── Single-child growth ────────────────────────────────────
                growth = dirs.sum(axis=0) + tropism
                gn     = np.linalg.norm(growth)
                growth = growth / gn if gn > 1e-8 else np.array([0., 0., 1.])
                growth = _clamp_elevation(growth, min_z)
                new_nodes.append(
                    (nodes_arr[tip_idx] + growth * cfg.sca_segment_mm, tip_idx)
                )

        if not new_nodes:
            break

        start_idx = len(node_xyz)
        spent     = {par for _, par in new_nodes}
        tips     -= spent
        for k, (new_pos, par) in enumerate(new_nodes):
            node_xyz.append(new_pos)
            parents.append(par)
            tips.add(start_idx + k)

        # Kill attractors within kill_r of any new node
        new_arr   = np.array([p for p, _ in new_nodes])
        kill_diff = att[:, None, :] - new_arr[None, :, :]
        kill_d2   = (kill_diff * kill_diff).sum(axis=-1)
        kill_mask = np.any(kill_d2 < (cfg.sca_kill_r ** 2), axis=-1)
        att       = att[~kill_mask]

    return np.array(node_xyz), np.array(parents, dtype=int)


# ── Public API ────────────────────────────────────────────────────────────────

def grow_skeleton(
    cx:  float,
    cy:  float,
    tz:  float,
    cfg,
    rng: np.random.Generator,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, float]:
    """Grow a unified tree skeleton from a single ground-level root.

    The attractor cloud is placed entirely above ``crown_base_z_mm``, so the
    skeleton grows a nearly-vertical path (the trunk) before entering the
    crown and branching.

    Returns
    -------
    nodes_xyz : (N, 3) float
    parents   : (N,) int  — -1 for the root
    arc_dists : (N,) float — cumulative arc length from root to each node
    crown_base_z : float  — sampled height of crown bottom above terrain (mm)
    """
    crown_base_z   = float(sample(cfg.crown_base_z_mm,  rng))
    crown_rx       = float(sample(cfg.crown_rx,         rng))
    crown_ry       = float(sample(cfg.crown_ry,         rng))
    crown_rz       = float(sample(cfg.crown_rz,         rng))
    crown_offset_z = float(sample(cfg.crown_offset_z,   rng))

    # Crown centre: above the attractor exclusion zone
    crown_center = np.array([
        cx,
        cy,
        tz + crown_base_z + crown_rz * 0.3 + crown_offset_z,
    ])

    # Over-sample then filter to keep only attractors above the exclusion floor
    n_over  = cfg.n_attractors * 5
    att_raw = _sample_in_ellipsoid(crown_center, crown_rx, crown_ry, crown_rz, n_over, rng)
    att     = att_raw[att_raw[:, 2] >= tz + crown_base_z - cfg.sca_segment_mm]

    if len(att) > cfg.n_attractors:
        idx = rng.choice(len(att), cfg.n_attractors, replace=False)
        att = att[idx]

    # Single root node at ground level
    root = np.array([[cx, cy, tz]], dtype=float)

    nodes_xyz, parents = _sca_grow(root, att, cfg, rng)
    arc_dists          = _compute_arc_dists(nodes_xyz, parents)

    return nodes_xyz, parents, arc_dists, crown_base_z
