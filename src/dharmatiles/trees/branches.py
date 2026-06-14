"""
Branch skeleton generation via Space Colonization Algorithm (Runions 2007).

The algorithm grows a tree skeleton from the trunk apex toward a cloud of
attraction points seeded inside a crown ellipsoid:

1. Seed ``n_attractors`` points uniformly in the crown ellipsoid.
2. At each step, for every attraction point find its nearest skeleton node
   within *perception_r*.
3. For each tip node that has nearby attractors: grow one new node one
   *segment_mm* in the normalised sum of attraction unit-vectors, plus a
   vertical tropism bias (FDM-safe upward lean).
4. Kill attractors within *kill_r* of any new node.
5. Repeat until all attractors are consumed or *max_steps* is reached.

Radii are assigned bottom-up using da Vinci's pipe model:
``r_parent² = Σ r_child²`` (leaf nodes start at *branch_r_tip_mm*).

Each skeleton edge becomes a ``_build_frustum`` truncated cone; these are
concatenated into one mesh.  Overlapping geometry at branch nodes is
acceptable for FDM slicing (the slicer union-treats closed shells).

Public API
----------
``build_branches(apex_pos, apex_dir, cx, cy, tz, height_mm, cfg, rng)``
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
    root_pos:       np.ndarray,
    n_attractors:   int,
    crown_center:   np.ndarray,
    crown_rx:       float,
    crown_ry:       float,
    crown_rz:       float,
    segment_mm:     float,
    perception_r:   float,
    kill_r:         float,
    max_steps:      int,
    tropism:        float,
    rng:            np.random.Generator,
) -> tuple[np.ndarray, np.ndarray]:
    """Run SCA; return ``(nodes_xyz (N,3), parents (N,) int)``.

    *root_pos* is node 0 (parent = -1).  All growth directions are clamped to
    have a non-negative Z component so no branch points downward (FDM safety).
    """
    att = _sample_in_ellipsoid(crown_center, crown_rx, crown_ry, crown_rz,
                                n_attractors, rng)
    # Discard attractors below the root (crown shouldn't reach underground)
    att = att[att[:, 2] >= root_pos[2] - segment_mm]
    if len(att) == 0:
        return np.array([root_pos]), np.array([-1], dtype=int)

    node_xyz: list[np.ndarray] = [root_pos.copy()]
    parents:  list[int]        = [-1]
    tips:     set[int]         = {0}
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

            dirs = diff[mask, tip_idx, :]                       # (K_local, 3)
            norms = np.sqrt((dirs * dirs).sum(axis=-1, keepdims=True))
            dirs  = dirs / np.maximum(norms, 1e-8)

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

            new_pos = nodes_arr[tip_idx] + growth * segment_mm
            new_nodes.append((new_pos, tip_idx))

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
    apex_pos:  np.ndarray,
    apex_dir:  np.ndarray,
    cx:        float,
    cy:        float,
    tz:        float,
    height_mm: float,
    cfg,
    rng:       np.random.Generator,
) -> trimesh.Trimesh | None:
    """Grow and mesh the branch crown above *apex_pos*.

    Returns a trimesh or ``None`` if no branches grew (e.g. all attractors
    are below the trunk apex).

    Crown ellipsoid centre:
      ``(cx, cy, apex_pos.z + crown_rz*0.3 + crown_offset_z)``
    This places the crown overlapping the apex and extending upward, which
    gives natural branching from just below the trunk tip.
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

    nodes_xyz, parents = _sca_grow(
        root_pos     = apex_pos,
        n_attractors = cfg.n_attractors,
        crown_center = crown_center,
        crown_rx     = crown_rx,
        crown_ry     = crown_ry,
        crown_rz     = crown_rz,
        segment_mm   = cfg.sca_segment_mm,
        perception_r = cfg.sca_perception_r,
        kill_r       = cfg.sca_kill_r,
        max_steps    = cfg.sca_max_steps,
        tropism      = cfg.sca_tropism,
        rng          = rng,
    )

    if len(nodes_xyz) <= 1:
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
