"""CloudTree skeleton: SCA growth with attractor-coincident leaf nodes.

Invariants
──────────
• Every attractor is a LEAF node.  No attractor is ever a branch point.
• Every branch terminates by landing exactly on an attractor.
• Branching happens at synthetic interior nodes (never at attractor positions).

Algorithm (per branch)
──────────────────────
Each branch owns a set of attractors (and optional group labels) and a current
tip position.

Attractor groups
────────────────
When *group_labels* is provided (array of integer IDs, one per attractor), stray
detection is lifted to the group level: if any member of group G is stray, the
*whole* group G splits off together.  Spawned sub-branches (one per stray group)
drop the labels and use individual-attractor logic for their internal structure.
Owned attractors that collapse to a single group also drop labels (leaf mode).

1. Lookahead stray detection: compute next_pos = tip + main_dir * seg_len.
   Classify owned attractors (or group centroids) as primary / stray based on
   their angle from *next_pos*.  Stray attractors would be outside the split
   cone after the next step, so we branch them off *before* stepping.
2. Stray groups spawn sub-branches FROM the current synthetic tip (before the
   step), guaranteeing no sub-branch ever walks backward to reach its targets.
   Stray individuals without labels use PCA clustering as before.
3. If primary reduces to 1 attractor → terminal mode: grow intermediate nodes at
   segment_length_mm intervals, final node lands EXACTLY on the attractor.
4. Otherwise advance one segment toward the primary centroid, repeat.
5. Safety: if the step budget is exhausted before convergence, force-split the
   primary set — keep the largest group / nearest attractor, hand the rest to a
   new sub-branch from the current synthetic position.

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
    segment_length_mm: float = 1.0,
    kill_radius_mm: float | None = None,   # unused; kept for API compatibility
    min_radius_mm: float = 0.45,
    min_branch_angle_deg: float = 30.0,
    branch_split_angle_deg: float | None = None,
    target_fdm_angle_deg: float = 35.0,
    max_branches_per_step: int = 3,
    branch_exponent: float = 2.5,
    smoothing_alpha: float = 0.25,
    group_width_mm: float | None = None,
    group_height_mm: float | None = None,
    foliage_bulge_mm: float = 0.0,
    branch_split_eagerness: float = 1.0,
    branch_target: float = 0.5,
    branch_fork_balance: float = 0.0,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    """Grow a CloudTree skeleton filling *env*.

    Parameters
    ----------
    group_width_mm:
        Target XY diameter of each attractor cluster.  When provided, attractors
        are partitioned into spatial Voronoi groups so that entire groups split
        off together during branching (coarser, more architectural splits).
        ``None`` (default) disables grouping; branching uses the original
        angle-based PCA splitting.
    group_height_mm:
        Target Z height of each attractor cluster.  Defaults to *group_width_mm*
        when not specified.  Ratio group_width / group_height controls the
        ellipsoidal aspect ratio of the clusters.
    foliage_bulge_mm:
        After grouping, displace each group's attractors outward from the canopy
        surface by up to *foliage_bulge_mm*.  Attractors on the group boundary
        (nearest to another group) remain on the surface; the attractor furthest
        from the boundary receives the full displacement.  Intermediate attractors
        follow a dome profile: displacement = foliage_bulge_mm × √(2t − t²),
        where t ∈ [0, 1] is the normalised edge-distance.  Has no effect when
        ``group_width_mm`` is None (no groups) or when only one group is created.
    branch_split_eagerness:
        Controls how eagerly branches split (0.0–1.0).  1.0 is the default:
        split off stray attractors as soon as the lookahead detects them
        (effective cone = branch_split_angle_deg).  0.0 is maximally lazy:
        keep attractors in primary until they are about to go perpendicular
        (effective cone = 90°, the hard no-backtracking limit).  Intermediate
        values linearly interpolate the effective split cosine:
            split_cos_effective = branch_split_eagerness × cos(branch_split_angle_deg)
        producing fewer, longer interior branches that split closer to the tips.

    Returns
    -------
    (nodes, parents, radii, prior_dirs, attractors)
    """
    if branch_split_angle_deg is None:
        branch_split_angle_deg = min_branch_angle_deg

    pts  = _sample_cloud(env, rng, n_attraction)
    root = np.array([env.cx, env.cy, env.terrain_z], dtype=float)

    # ── Attractor grouping ────────────────────────────────────────────────────
    group_labels: np.ndarray | None = None
    if group_width_mm is not None and len(pts) >= 2:
        gh = group_height_mm if group_height_mm is not None else group_width_mm
        gh = max(gh, 1e-9)
        n_groups = _compute_n_groups(env, group_width_mm, gh)
        if n_groups >= 2:
            z_scale = group_width_mm / gh
            group_labels = _voronoi_group_attractors(pts, n_groups, rng, z_scale=z_scale)

    # ── Foliage group bulge ───────────────────────────────────────────────────
    if group_labels is not None and foliage_bulge_mm > 1e-9:
        pts = _apply_group_bulge(pts, group_labels, env, foliage_bulge_mm)

    # Step budget: generous so the branch tree can partition the full cloud.
    max_steps = max(60, int(np.ceil(env.height_mm / segment_length_mm) * 4))

    # branch_split_eagerness scales the effective split cosine linearly between
    # cos(branch_split_angle_deg) at 1.0 and 0.0 (= cos 90°) at 0.0.
    eager_cos     = float(np.cos(np.radians(branch_split_angle_deg)))
    effective_cos = eager_cos * float(np.clip(branch_split_eagerness, 0.0, 1.0))

    # Target FDM angle: the elevation above horizon we try to keep branches
    # above by splitting. Stored as a sine so the stray test can compare it
    # directly against an attractor heading's dz/len.
    min_elev_sin = float(np.sin(np.radians(target_fdm_angle_deg)))

    nodes, parents, prior_dirs = _branch_skeleton(
        root          = root,
        pts           = pts,
        seg_len       = float(segment_length_mm),
        split_cos     = effective_cos,
        min_elev_sin  = min_elev_sin,
        max_branches  = int(max_branches_per_step),
        alpha         = float(smoothing_alpha),
        max_steps     = max_steps,
        group_labels  = group_labels,
        branch_target = branch_target,
        branch_fork_balance = float(np.clip(branch_fork_balance, 0.0, 1.0)),
    )

    # Compress: drop collinear single-child nodes; they don't affect radii
    # (pipe model: r_parent = r_child when there is exactly one child) and
    # their tangent info is preserved in the per-edge in_dir / out_dir arrays.
    nodes_s, parents_s, in_dirs_s, out_dirs_s = _simplify_skeleton(
        nodes, parents, prior_dirs
    )

    radii = _compute_radii_bottom_up(parents_s, branch_exponent, min_radius_mm)

    return (
        nodes_s,
        parents_s,
        radii,
        in_dirs_s,
        out_dirs_s,
        pts,
        group_labels,   # None when no grouping; int array (n_attraction,) otherwise
    )


# ─────────────────────────────────────────────────────────────────────────────
# Pass 1: branch skeleton
# ─────────────────────────────────────────────────────────────────────────────

def _branch_skeleton(
    root:          np.ndarray,
    pts:           np.ndarray,
    seg_len:       float,
    split_cos:     float,
    min_elev_sin:  float,
    max_branches:  int,
    alpha:         float,
    max_steps:     int,
    group_labels:  np.ndarray | None = None,
    branch_target:       float = 0.5,
    branch_fork_balance: float = 0.0,
) -> tuple[list, list, list]:
    nodes:      list[np.ndarray] = [root.copy()]
    parents:    list[int]        = [-1]
    prior_dirs: list[np.ndarray] = [np.array([0.0, 0.0, 1.0])]

    if len(pts) == 0:
        return nodes, parents, prior_dirs

    # Queue items: (tip_node_idx, owned_attractors, owned_group_labels, steps_taken)
    # owned_group_labels is None for leaf branches (individual-attractor logic)
    init_labels = group_labels.copy() if group_labels is not None else None
    queue: deque[tuple[int, np.ndarray, np.ndarray | None, int]] = deque(
        [(0, pts.copy(), init_labels, 0)]
    )

    while queue:
        tip_idx, owned, labels, steps = queue.popleft()
        if len(owned) == 0:
            continue

        pos     = nodes[tip_idx]
        heading = prior_dirs[tip_idx]

        # Primary target point — lerp between lowest-z (0) and highest-z (1) attractor.
        lowest  = owned[int(np.argmin(owned[:, 2]))]
        highest = owned[int(np.argmax(owned[:, 2]))]
        target  = lowest + float(branch_target) * (highest - lowest)
        raw    = target - pos
        raw_len  = float(np.linalg.norm(raw))
        if raw_len > 1e-9:
            raw_dir  = raw / raw_len
            blended  = raw_dir * (1.0 - alpha) + heading * alpha
            b_len    = float(np.linalg.norm(blended))
            main_dir = blended / b_len if b_len > 1e-9 else raw_dir
        else:
            main_dir = heading

        # ── stray detection ────────────────────────────────────────────────
        primary        = owned
        primary_labels = labels

        if len(owned) >= 2 and max_branches > 1:
            # Lookahead stray detection: measure angles from the *next* tip
            # position (pos + main_dir * seg_len) rather than from pos.
            # We branch from the current pos before stepping, so every
            # attractor kept in primary is guaranteed within the split cone
            # from next_pos — no sub-branch ever has to walk backward.
            # This replaces the old z-passover check, which was compensating
            # for the same problem and capped doubling-back at one seg_len.
            next_pos = pos + main_dir * seg_len
            to_owned = owned - next_pos
            unit_to  = to_owned / (np.linalg.norm(to_owned, axis=1, keepdims=True) + 1e-9)
            cos_a    = np.clip(unit_to @ main_dir, -1.0, 1.0)
            # An attractor is stray if, measured from the forecasted next step,
            # reaching it would (a) leave the split cone around main_dir, OR
            # (b) require a heading below the target FDM angle (its heading's
            # vertical component is unit_to[:, 2] = sin(elevation)).
            split_stray = cos_a < split_cos
            elev_stray  = unit_to[:, 2] < min_elev_sin
            stray_mask  = split_stray | elev_stray

            if labels is not None and stray_mask.any():
                # Group-level stray: if any member of a group is stray,
                # the whole group becomes stray (they travel together).
                stray_group_ids = set(labels[stray_mask].tolist())
                stray_mask = np.isin(labels, list(stray_group_ids))

            stray          = owned[stray_mask]
            primary        = owned[~stray_mask]
            stray_labels   = labels[stray_mask]  if labels is not None else None
            primary_labels = labels[~stray_mask] if labels is not None else None

            if len(stray) > 0 and len(primary) > 0:
                # ── Compute stray clusters (needed to know K for fill) ──────
                if labels is not None:
                    stray_clusters = [
                        stray[stray_labels == gid]
                        for gid in np.unique(stray_labels)
                        if np.sum(stray_labels == gid) > 0
                    ]
                else:
                    stray_clusters = [
                        c for c in _cluster_pca(stray, pos, max_branches - 1)
                        if len(c) > 0
                    ]

                # ── branch_fork_balance redistribution ────────────────────────────
                # At fill=0 (default) each branch keeps its natural cone
                # subset.  At fill=1 all owned attractors are divided equally
                # among the K branches at this fork (1 primary + stray clusters).
                # Intermediate values lerp the primary target count.
                # In group mode: whole groups are re-assigned, not individuals.
                if branch_fork_balance > 1e-6 and stray_clusters:
                    K = 1 + len(stray_clusters)
                    if labels is None:
                        # Non-group: re-rank individuals by cos_a.
                        n_target = int(round(
                            len(primary) + (len(owned) / K - len(primary)) * branch_fork_balance
                        ))
                        n_target = max(1, min(len(owned) - 1, n_target))
                        order    = np.argsort(-cos_a)
                        primary  = owned[order[:n_target]]
                        remaining = owned[order[n_target:]]
                        stray_clusters = [
                            c for c in _cluster_pca(remaining, pos, max_branches - 1)
                            if len(c) > 0
                        ]
                        primary_labels = None
                    else:
                        # Group mode: re-rank whole groups by centroid cos_a.
                        all_gids        = np.unique(labels)
                        n_g             = len(all_gids)
                        n_primary_groups = (
                            len(np.unique(primary_labels))
                            if primary_labels is not None else 0
                        )
                        n_g_target = int(round(
                            n_primary_groups
                            + (n_g / K - n_primary_groups) * branch_fork_balance
                        ))
                        n_g_target = max(1, min(n_g - 1, n_g_target))
                        # Sort groups by centroid cos_a.
                        gid_cos = {
                            gid: float(np.mean(cos_a[labels == gid]))
                            for gid in all_gids
                        }
                        sorted_gids = sorted(all_gids, key=lambda g: -gid_cos[g])
                        primary_gids = set(sorted_gids[:n_g_target])
                        primary_mask = np.isin(labels, list(primary_gids))
                        primary        = owned[primary_mask]
                        primary_labels = labels[primary_mask]
                        stray_clusters = [
                            owned[labels == gid]
                            for gid in sorted_gids[n_g_target:]
                            if np.sum(labels == gid) > 0
                        ]

                # ── Spawn sub-branches ─────────────────────────────────────
                for cluster in stray_clusters:
                    queue.append((tip_idx, cluster, None, 0))

            if len(primary) == 0:
                # All stray: keep the largest group/cluster as primary.
                if labels is not None:
                    stray_unique = np.unique(stray_labels)
                    best_id = _largest_group_id(stray, stray_labels, stray_unique)
                    best_mask = stray_labels == best_id
                    primary        = stray[best_mask]
                    primary_labels = None   # leaf behavior from here
                    for gid in stray_unique:
                        if gid == best_id:
                            continue
                        cluster = stray[stray_labels == gid]
                        if len(cluster) > 0:
                            queue.append((tip_idx, cluster, None, 0))
                else:
                    # All stray: pick one cluster as our primary, rest as sub-branches.
                    clusters = _cluster_pca(owned, pos, 2)
                    if not clusters:
                        continue
                    primary        = clusters[0]
                    primary_labels = None
                    for extra in clusters[1:]:
                        if len(extra) > 0:
                            queue.append((tip_idx, extra, None, 0))

            # Drop labels once primary collapses to a single group (leaf mode).
            if primary_labels is not None and len(np.unique(primary_labels)) <= 1:
                primary_labels = None

        # ── terminal or continue ───────────────────────────────────────────
        force_terminal = (len(primary) == 1) or (steps >= max_steps)

        if force_terminal:
            if len(primary) > 1:
                # Safety: keep the largest/nearest attractor as our terminal target.
                if primary_labels is not None:
                    # Keep the largest group; spawn the rest.
                    unique_ids = np.unique(primary_labels)
                    best_id    = _largest_group_id(primary, primary_labels, unique_ids)
                    best_mask  = primary_labels == best_id
                    rest       = primary[~best_mask]
                    if len(rest) > 0:
                        queue.append((tip_idx, rest, None, 0))
                    primary = primary[best_mask]
                    # Now reduce to one attractor within the group
                    dists   = np.linalg.norm(primary - pos, axis=1)
                    near_i  = int(np.argmin(dists))
                    rest2   = np.delete(primary, near_i, axis=0)
                    if len(rest2) > 0:
                        queue.append((tip_idx, rest2, None, 0))
                    primary = primary[near_i : near_i + 1]
                else:
                    dists     = np.linalg.norm(primary - pos, axis=1)
                    near_i    = int(np.argmin(dists))
                    rest      = np.delete(primary, near_i, axis=0)
                    if len(rest) > 0:
                        queue.append((tip_idx, rest, None, 0))
                    primary = primary[near_i : near_i + 1]

            # Grow from synthetic tip to the single target, landing exactly on it.
            _grow_to_leaf(nodes, parents, prior_dirs,
                          tip_idx, primary[0], heading, seg_len, alpha)

        else:
            # Advance one segment toward the primary centroid.
            new_pos = pos + main_dir * seg_len
            new_idx = _add_node(nodes, parents, prior_dirs, new_pos, tip_idx, main_dir)
            queue.append((new_idx, primary, primary_labels, steps + 1))

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
# Skeleton simplification
# ─────────────────────────────────────────────────────────────────────────────

def _simplify_skeleton(
    nodes:      list | np.ndarray,
    parents:    list | np.ndarray,
    prior_dirs: list | np.ndarray,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    """Remove intermediate single-child nodes, keeping root, branch points,
    and leaf attractors.

    For each kept edge (parent → child) two tangent vectors are stored:
    - ``in_dirs[i]``  — direction *arriving* at node i (last step into it)
    - ``out_dirs[i]`` — direction *leaving* parents[i] toward node i (first step)

    These are the start and end tangents for one cubic Bézier per edge in the
    mesh, giving C1 continuity without needing to store every intermediate node.

    The pipe-model identity r_parent = r_child for a single-child node means
    radii are identical along any straight chain, so dropping those nodes leaves
    the bottom-up radius computation unchanged.
    """
    n          = len(nodes)
    nodes_arr  = np.asarray(nodes,      dtype=float)
    prior_arr  = np.asarray(prior_dirs, dtype=float)
    parents_l  = list(parents)

    # Build children list.
    children: list[list[int]] = [[] for _ in range(n)]
    for i, p in enumerate(parents_l):
        if p >= 0:
            children[p].append(i)

    # Significant nodes: root (no parent), branch points (≠1 child), leaves (0 children).
    significant: set[int] = set()
    for i in range(n):
        if parents_l[i] < 0 or len(children[i]) != 1:
            significant.add(i)

    # DFS — parents always get lower new-indices than their children (topological
    # order), which is required by _compute_radii_bottom_up.
    new_nodes:    list[np.ndarray] = []
    new_parents:  list[int]        = []
    new_in_dirs:  list[np.ndarray] = []
    new_out_dirs: list[np.ndarray] = []

    def _visit(old_idx: int, new_parent_idx: int, edge_out_dir: np.ndarray) -> None:
        new_idx = len(new_nodes)
        new_nodes.append(nodes_arr[old_idx])
        new_parents.append(new_parent_idx)
        new_in_dirs.append(prior_arr[old_idx].copy())
        new_out_dirs.append(edge_out_dir)
        for c in children[old_idx]:
            # Walk the (possibly empty) chain of non-significant nodes to find
            # the next significant node; the first step direction is prior_dirs[c].
            first_step_dir = prior_arr[c].copy()
            cur = c
            while cur not in significant:
                cur = children[cur][0]   # guaranteed exactly one child here
            _visit(cur, new_idx, first_step_dir)

    _visit(0, -1, prior_arr[0].copy())

    return (
        np.array(new_nodes,    dtype=float),
        np.array(new_parents,  dtype=int),
        np.array(new_in_dirs,  dtype=float),
        np.array(new_out_dirs, dtype=float),
    )


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
# Attractor grouping
# ─────────────────────────────────────────────────────────────────────────────

def _compute_n_groups(
    env: TreeEnvelope,
    group_width_mm: float,
    group_height_mm: float,
) -> int:
    """Estimate how many groups of target size fit across the crown surface."""
    if group_width_mm <= 0 or group_height_mm <= 0:
        return 1
    # Circumferential count at maximum crown radius
    n_around = max(1, round(2.0 * np.pi * env.crown_radius_mm / group_width_mm))
    n_tall   = max(1, round(max(0.0, env.crown_height) / group_height_mm))
    return max(2, n_around * n_tall)


def _voronoi_group_attractors(
    pts:     np.ndarray,
    n_groups: int,
    rng:     np.random.Generator,
    z_scale: float = 1.0,
) -> np.ndarray:
    """Assign each attractor to one of *n_groups* spatial Voronoi clusters.

    Uses Lloyd's algorithm (k-means) in a z-scaled space so that clusters have
    an ellipsoidal aspect ratio matching *group_width* : *group_height*.

    ``z_scale = group_width_mm / group_height_mm`` makes the clusters prefer the
    target shape: < 1 → elongated vertically, > 1 → pancake-flat.

    Returns integer group labels (0 .. n_groups-1) for each attractor.
    """
    n = len(pts)
    if n == 0 or n_groups <= 1:
        return np.zeros(n, dtype=int)
    k = min(n_groups, n)

    # Work in scaled space so distance reflects target cluster shape.
    scaled        = pts.copy()
    scaled[:, 2] *= z_scale

    # K-means++ style initialisation: pick k distinct seed points from pts.
    seed_idx = rng.choice(n, size=k, replace=False)
    seeds    = scaled[seed_idx].copy()

    # Lloyd's iterations (vectorised, n*k*3 tensors are small for typical sizes).
    for _ in range(20):
        diff   = scaled[:, np.newaxis, :] - seeds[np.newaxis, :, :]  # (n, k, 3)
        dists2 = (diff ** 2).sum(axis=2)                               # (n, k)
        labels = np.argmin(dists2, axis=1)                             # (n,)

        new_seeds = np.empty_like(seeds)
        for ki in range(k):
            members = scaled[labels == ki]
            new_seeds[ki] = members.mean(axis=0) if len(members) > 0 else seeds[ki]

        if np.allclose(seeds, new_seeds, atol=1e-6):
            break
        seeds = new_seeds

    return labels.astype(int)


def _apply_group_bulge(
    pts:          np.ndarray,
    group_labels: np.ndarray,
    env:          TreeEnvelope,
    bulge_mm:     float,
) -> np.ndarray:
    """Displace attractor groups outward from the crown envelope, dome-shaped.

    For each Voronoi group the "edge distance" of each attractor is the minimum
    3-D distance to any attractor belonging to a *different* group.  Normalising
    by the maximum edge distance in the group gives t ∈ [0, 1] (0 = boundary,
    1 = deepest interior).  The outward displacement follows a circular-arc
    (dome) profile:

        displacement = bulge_mm × √(2t − t²)

    which is zero at the boundary and *bulge_mm* at the group centre, with a
    round shoulder matching the cross-section of a hemisphere.

    Outward direction is the unit normal to the crown surface of revolution at
    each attractor position (see ``TreeEnvelope.outward_normal_at``).
    """
    unique = np.unique(group_labels)
    if len(unique) <= 1:
        return pts.copy()  # single group → no inter-group boundary to measure from

    result = pts.copy()
    for gid in unique:
        mask     = group_labels == gid
        in_group = pts[mask]     # (ng, 3)
        other    = pts[~mask]    # (no, 3)
        ng       = len(in_group)
        if ng == 0 or len(other) == 0:
            continue

        # Minimum 3-D distance from each in-group point to the nearest
        # out-of-group point → edge proximity measure.
        diffs  = in_group[:, np.newaxis, :] - other[np.newaxis, :, :]  # (ng, no, 3)
        d_edge = np.sqrt((diffs ** 2).sum(axis=2).min(axis=1))          # (ng,)

        d_max = d_edge.max()
        if d_max < 1e-9:
            continue
        t = d_edge / d_max  # 0 at boundary, 1 at interior centroid

        # Dome profile: circular-arc cross-section.
        dome = np.sqrt(np.clip(2.0 * t - t * t, 0.0, 1.0))  # (ng,)

        normals = env.outward_normal_at(in_group)                    # (ng, 3)
        result[mask] += normals * (dome * bulge_mm)[:, np.newaxis]

    return result


def _largest_group_id(
    pts:        np.ndarray,
    labels:     np.ndarray,
    unique_ids: np.ndarray,
) -> int:
    """Return the group label with the most attractors."""
    best_id    = int(unique_ids[0])
    best_count = 0
    for gid in unique_ids:
        count = int(np.sum(labels == gid))
        if count > best_count:
            best_count = count
            best_id    = int(gid)
    return best_id


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
