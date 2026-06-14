"""
Branch skeleton generation via Space Colonization Algorithm (Runions 2007).

The algorithm grows a tree skeleton from multiple root nodes along the upper
trunk toward a cloud of attraction points seeded inside a crown ellipsoid:

1. Root nodes are taken from the top ``(1 - sca_trunk_root_frac)`` fraction
   of the trunk spine so that branches appear to emerge from the trunk, not
   only from the apex.
2. At each step, for every attraction point find its nearest skeleton node
   within *perception_r*.
3. For each tip node that has nearby attractors:
   a. If the XY spread of attraction directions exceeds *sca_branch_xy_std*
      **and** each prospective cluster has >= *sca_min_branch_att* attractors,
      the tip **splits** into two children growing in the two cluster means.
   b. Otherwise the tip grows one new node in the normalised sum of attraction
      unit-vectors, plus a vertical tropism bias.
4. Kill attractors within *kill_r* of any new node.
5. Repeat until all attractors are consumed or *max_steps* is reached.

Radii are assigned bottom-up using da Vinci's pipe model:
``r_parent² = Σ r_child²`` (leaf nodes start at *branch_r_tip_mm*).

Each skeleton edge becomes a ``_build_frustum`` truncated cone; these are
concatenated into one mesh.  Overlapping geometry at branch nodes is
acceptable for FDM slicing (the slicer union-treats closed shells).

Public API
----------
``build_branches(apex_pos, apex_dir, cx, cy, tz, height_mm, cfg, rng, trunk_spine)``
    Run SCA and return a trimesh (or None if no branches grew).
"""
from __future__ import annotations

import numpy as np
import trimesh

from ..dist import sample
from .trunk import _build_frustum


# ── Attractor sampling ────────────────────────────────────────────────────────

def _sample_in_ellipsoid(
    center: np.ndarray,
    rx: float, ry: float, rz: float,
    n: int,
    rng: np.random.Generator,
) -> np.ndarray:
    """Sample *n* points uniformly inside an ellipsoid via rejection."""
    pts: list[np.ndarray] = []
    while len(pts) < n:
        batch  = rng.uniform(-1.0, 1.0, (n * 3, 3))
        inside = (batch[:, 0] ** 2 + batch[:, 1] ** 2 + batch[:, 2] ** 2) <= 1.0
        pts.extend(batch[inside])
    arr = np.array(pts[:n])
    arr[:, 0] *= rx
    arr[:, 1] *= ry
    arr[:, 2] *= rz
    return arr + center


# ── Space colonization ────────────────────────────────────────────────────────

def _sca_grow(
    root_positions:     np.ndarray,   # (N_roots, 3) — starting nodes (all parent = -1)
    n_attractors:       int,
    crown_center:       np.ndarray,
    crown_rx:           float,
    crown_ry:           float,
    crown_rz:           float,
    segment_mm:         float,
    perception_r:       float,
    kill_r:             float,
    max_steps:          int,
    tropism:            float,
    branch_xy_std:      float,        # XY direction std-dev threshold for branching
    min_branch_att:     int,          # min attractors per cluster to allow split
    rng:                np.random.Generator,
) -> tuple[np.ndarray, np.ndarray]:
    """Run SCA; return ``(nodes_xyz (N,3), parents (N,) int)``.

    *root_positions* are the initial nodes (all with parent = -1).  Typically
    these are a subset of the trunk spine so branches appear to emerge from
    the trunk at multiple heights.

    Growth directions are clamped to non-negative Z (FDM safety).

    When a tip's visible attractors show sufficient XY spread (std > *branch_xy_std*)
    **and** each half has >= *min_branch_att* members, the tip splits into two
    children, one per cluster.  Otherwise a single child is grown toward the
    average direction.
    """
    n_roots = len(root_positions)
    att = _sample_in_ellipsoid(crown_center, crown_rx, crown_ry, crown_rz,
                                n_attractors, rng)
    # Discard attractors below the lowest root (crown shouldn't be underground)
    min_root_z = float(root_positions[:, 2].min())
    att = att[att[:, 2] >= min_root_z - segment_mm]
    if len(att) == 0:
        return root_positions.copy(), np.full(n_roots, -1, dtype=int)

    node_xyz: list[np.ndarray] = [r.copy() for r in root_positions]
    parents:  list[int]        = [-1] * n_roots
    tips:     set[int]         = set(range(n_roots))
    tropism_vec = np.array([0.0, 0.0, tropism], dtype=float)

    for _step in range(max_steps):
        if not att.shape[0] or not tips:
            break

        nodes_arr = np.array(node_xyz)   # (M, 3)
        att_arr   = att                  # (K, 3)

        # Distance² from each attractor to each node: (K, M)
        diff   = att_arr[:, None, :] - nodes_arr[None, :, :]   # (K, M, 3)
        dist2  = (diff * diff).sum(axis=-1)                     # (K, M)

        # Each attractor picks its nearest node
        nearest      = np.argmin(dist2, axis=-1)               # (K,)
        nearest_d2   = dist2[np.arange(len(att_arr)), nearest] # (K,)
        in_range     = nearest_d2 < (perception_r ** 2)

        if not np.any(in_range):
            break

        # Grow each tip that has at least one influencing attractor
        new_nodes: list[tuple[np.ndarray, int]] = []
        for tip_idx in sorted(tips):
            mask = in_range & (nearest == tip_idx)
            if not np.any(mask):
                continue

            dirs  = diff[mask, tip_idx, :]                      # (K_local, 3)
            norms = np.sqrt((dirs * dirs).sum(axis=-1, keepdims=True))
            dirs_n = dirs / np.maximum(norms, 1e-8)

            # ── Branching detection ────────────────────────────────────────
            # Split when the XY spread of attraction directions is large,
            # using the direction of maximum XY variance to partition clusters.
            branched = False
            n_local = len(dirs_n)
            if n_local >= 2 * min_branch_att:
                xy_std = float(dirs_n[:, :2].std())
                if xy_std > branch_xy_std:
                    # Choose split axis: X or Y — whichever has more variance
                    x_std = float(dirs_n[:, 0].std())
                    y_std = float(dirs_n[:, 1].std())
                    col   = 0 if x_std >= y_std else 1
                    split = float(np.median(dirs_n[:, col]))

                    mask_a = dirs_n[:, col] >= split
                    mask_b = ~mask_a

                    if np.sum(mask_a) >= min_branch_att and np.sum(mask_b) >= min_branch_att:
                        for submask in (mask_a, mask_b):
                            g = dirs_n[submask].mean(axis=0) + tropism_vec
                            gn = np.linalg.norm(g)
                            g  = g / gn if gn > 1e-8 else np.array([0.0, 0.0, 1.0])
                            if g[2] < 0.0:
                                g[2] = 0.0
                                gn   = np.linalg.norm(g)
                                g    = g / gn if gn > 1e-8 else np.array([0.0, 0.0, 1.0])
                            new_nodes.append((nodes_arr[tip_idx] + g * segment_mm, tip_idx))
                        branched = True

            if not branched:
                # ── Standard single-child growth ───────────────────────────
                growth = dirs.sum(axis=0) + tropism_vec
                gn     = np.linalg.norm(growth)
                if gn < 1e-8:
                    growth = np.array([0.0, 0.0, 1.0])
                else:
                    growth /= gn

                # FDM constraint: no downward growth
                if growth[2] < 0.0:
                    growth[2] = 0.0
                    gn = np.linalg.norm(growth)
                    if gn < 1e-8:
                        growth = np.array([0.0, 0.0, 1.0])
                    else:
                        growth /= gn

                new_nodes.append((nodes_arr[tip_idx] + growth * segment_mm, tip_idx))

        if not new_nodes:
            break

        # Register new nodes; retire spent tips; promote new ones
        start_idx = len(node_xyz)
        spent     = {par for _, par in new_nodes}
        tips     -= spent
        for k, (new_pos, par) in enumerate(new_nodes):
            node_xyz.append(new_pos)
            parents.append(par)
            tips.add(start_idx + k)

        # Kill attractors near newly added nodes
        new_arr   = np.array([p for p, _ in new_nodes])          # (K_new, 3)
        kill_diff = att_arr[:, None, :] - new_arr[None, :, :]   # (K, K_new, 3)
        kill_d2   = (kill_diff * kill_diff).sum(axis=-1)
        kill_mask = np.any(kill_d2 < (kill_r ** 2), axis=-1)
        att       = att[~kill_mask]

    return np.array(node_xyz), np.array(parents, dtype=int)


# ── Radius assignment ─────────────────────────────────────────────────────────

def _assign_radii(
    parents:      np.ndarray,
    r_tip_mm:     float,
) -> np.ndarray:
    """Bottom-up da Vinci pipe model: r_parent² = Σ r_child².

    Leaf nodes (no children) get *r_tip_mm*.  Radii are accumulated from
    leaves to root so the root radius reflects the total pipe cross-section.
    Multiple root nodes (parent = -1) are each accumulated independently.
    """
    N      = len(parents)
    radii  = np.full(N, r_tip_mm, dtype=float)

    # Bottom-up: accumulate from leaves toward root.
    # Nodes are added in BFS order, so reversing gives leaves first.
    for i in range(N - 1, 0, -1):
        p = int(parents[i])
        if p >= 0:
            radii[p] = np.sqrt(radii[p] ** 2 + radii[i] ** 2)

    return radii


# ── Skeleton → mesh ───────────────────────────────────────────────────────────

def _skeleton_to_mesh(
    nodes_xyz: np.ndarray,
    parents:   np.ndarray,
    radii:     np.ndarray,
    az_segs:   int,
    min_r:     float,
) -> trimesh.Trimesh:
    """Convert skeleton edges to a concatenated frustum mesh.

    Edges whose *max* radius is below *min_r* are skipped (below FDM
    printable minimum).  Geometry at branch junctions overlaps; slicers
    treat overlapping closed shells as a union, which is correct.

    Nodes with parent = -1 are roots; they appear as parents of edges but
    have no incoming edge themselves.
    """
    parts: list[trimesh.Trimesh] = []
    N = len(nodes_xyz)
    for i in range(1, N):
        p   = int(parents[i])
        if p < 0:
            continue
        r0  = float(radii[p])
        r1  = float(radii[i])
        if max(r0, r1) < min_r:
            continue
        frust = _build_frustum(nodes_xyz[p], nodes_xyz[i], r0, r1, az_segs)
        if len(frust.vertices) > 0:
            parts.append(frust)

    if not parts:
        return trimesh.Trimesh(process=False)
    return trimesh.util.concatenate(parts)


# ── Public entry point ────────────────────────────────────────────────────────

def build_branches(
    apex_pos:   np.ndarray,
    apex_dir:   np.ndarray,
    cx:         float,
    cy:         float,
    tz:         float,
    height_mm:  float,
    cfg,
    rng:        np.random.Generator,
    trunk_spine: np.ndarray | None = None,
) -> trimesh.Trimesh | None:
    """Grow and mesh the branch crown above *apex_pos*.

    Returns a trimesh or ``None`` if no branches grew.

    SCA root nodes are taken from the top ``(1 - cfg.sca_trunk_root_frac)``
    fraction of *trunk_spine* so branches appear to emerge from the trunk at
    multiple heights rather than only from the apex.  When *trunk_spine* is
    None, only the apex is used as the root.

    Crown ellipsoid centre:
      ``(cx, cy, apex_pos.z + crown_rz*0.3 + crown_offset_z)``
    """
    if not cfg.grow_branches:
        return None

    crown_rx       = float(sample(cfg.crown_rx,       rng))
    crown_ry       = float(sample(cfg.crown_ry,       rng))
    crown_rz       = float(sample(cfg.crown_rz,       rng))
    crown_offset_z = float(sample(cfg.crown_offset_z, rng))

    crown_center = np.array([
        apex_pos[0],
        apex_pos[1],
        apex_pos[2] + crown_rz * 0.3 + crown_offset_z,
    ])

    # ── Root positions: upper trunk spine nodes ────────────────────────────
    if trunk_spine is not None and len(trunk_spine) >= 3:
        n      = len(trunk_spine)
        # Use the top (1 - trunk_root_frac) of the spine as SCA roots,
        # e.g. frac=0.60 → top 40% (last 40% of spine points).
        start  = max(0, int(n * cfg.sca_trunk_root_frac))
        roots  = trunk_spine[start:]          # (N_roots, 3)
    else:
        roots = apex_pos.reshape(1, 3)        # fallback: apex only

    nodes_xyz, parents = _sca_grow(
        root_positions   = roots,
        n_attractors     = cfg.n_attractors,
        crown_center     = crown_center,
        crown_rx         = crown_rx,
        crown_ry         = crown_ry,
        crown_rz         = crown_rz,
        segment_mm       = cfg.sca_segment_mm,
        perception_r     = cfg.sca_perception_r,
        kill_r           = cfg.sca_kill_r,
        max_steps        = cfg.sca_max_steps,
        tropism          = cfg.sca_tropism,
        branch_xy_std    = cfg.sca_branch_xy_std,
        min_branch_att   = cfg.sca_min_branch_att,
        rng              = rng,
    )

    if len(nodes_xyz) <= len(roots):
        return None

    radii = _assign_radii(parents, cfg.branch_r_tip_mm)
    mesh  = _skeleton_to_mesh(
        nodes_xyz, parents, radii,
        az_segs = cfg.branch_az_segs,
        min_r   = cfg.branch_min_r_mm,
    )

    if len(mesh.vertices) == 0:
        return None

    mesh.fix_normals()
    return mesh
