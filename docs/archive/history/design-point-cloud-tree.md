# Point-Cloud-Partitioned Tree Growth — Design Document

## Overview

A procedural tree generator driven by a canopy-shaped attraction-point cloud.
Each branch "owns" a partition of the point cloud and grows toward its centroid.
Branching occurs when angular constraints force a split; branch thickness is
proportional to the size of the owned partition.

---

## Inputs

| Parameter | Type | Default | Description |
|---|---|---|---|
| `root_xy_mm` | (float, float) | — | Base of trunk in tile XY |
| `root_z_mm` | float | — | Height of ground at that point |
| `canopy_shape` | `CanopyShape` | — | Parameterised volume (see below) |
| `n_attraction` | int | — | Total attraction points scattered over the canopy |
| `seed` | int | 42 | RNG seed for reproducible point placement and jitter |
| `min_up_angle_deg` | float | 20° | Every segment must point at least this far above horizontal |
| `min_branch_angle_deg` | float | 30° | Minimum divergence angle between a parent segment and any child branch |
| `branch_split_angle_deg` | float | = `min_branch_angle_deg` | Points at this angle from the primary direction trigger a separate branch. Separate from `min_branch_angle_deg` to allow independent tuning. |
| `max_branches_per_step` | int | 3 | Maximum total child branches per step, **including** the primary continuation. Caps the number of separate groups that emerge from one node. |
| `segment_length_mm` | float | — | Growth step length per iteration |
| `kill_radius_mm` | float | — | Attraction points within this distance of the advancing tip are consumed. **Required invariant: `kill_radius_mm ≥ segment_length_mm`** (otherwise a step may pass through the cloud without consuming any points, leaving the branch radius unchanged indefinitely). |
| `trunk_radius_mm` | float | — | Radius of the root/trunk segment |
| `branch_exponent` | float | 2.5 | Pipe-model exponent *e*. Controls taper rate at splits: higher *e* = gentler taper (children stay thicker at each split). Absolute trunk scale is set by `trunk_radius_mm`, not this exponent. |
| `smoothing_alpha` | float | 0.25 | Blend weight toward prior heading each step (0 = pure centroid, 1 = no steering) |
| `min_radius_mm` | float | — | Terminal threshold — branches thinner than this stop growing |

---

## Canopy Shape

`CanopyShape` is a parameterised volume that can uniformly sample N 3-D points.
Initial candidates:

- **Ellipsoid** — centre, three semi-axes (good default for broadleaf)
- **Cone** — apex, base-radius, height (conifers)
- **Layered ellipsoid** — flattened at top, fuller near crown base

**Sampling method:** rejection sampling (sample from the bounding box, reject
points outside the volume). This is correct for any convex shape. Quasi-random
sequences (Halton or Sobol) are preferred over uniform random — they produce
better spatial coverage with fewer points, which improves branching uniformity.

---

## Core Data Structures

```
Node
  position: vec3
  radius:   float        ← radius of the segment ARRIVING at this node
                           (mesh builder reads parent.radius → child.radius
                            to produce a tapered edge)
  parent:   Node | None
  children: [Node]

Branch (transient, during growth)
  tip:       vec3            ← current end of the last grown segment
  radius:    float           ← radius this branch is currently growing at
  prior_dir: vec3            ← smoothed heading from previous step (init: (0,0,1))
  points:    [vec3]          ← owned attraction-point partition
  node:      Node            ← node at the tip (last emitted)
```

**Mesh semantics**: a mesh builder draws a tapered cubic Bezier tube for each
(parent, child) node pair:
- Start point: `parent.position`, start tangent: `parent.prior_dir * scale`
- End point: `child.position`, end tangent: `child.prior_dir * scale`
- Cross-section radius tapers linearly from `parent.radius` to `child.radius`

`scale` controls Bezier handle length (typically 0.4–0.6 × segment length).

C1 continuity at branch joins is guaranteed because every child's `prior_dir`
is initialised to the parent branch's `direction` at the split moment, so the
child's start tangent equals the parent's end tangent.

Each child's first node carries `radii[i]` (the post-split radius), so taper
begins immediately at the branch point.

---

## Algorithm

### Growth order: iterative breadth-first

**All active branch tips advance one step per global iteration.** This is
architecturally different from a depth-first recursive approach:

- Depth-first: one branch races to its terminal before any sibling starts.
  Siblings cannot benefit from points consumed by a leading branch.
- Breadth-first: all active branches compete for points simultaneously each
  iteration. A branch that has consumed all nearby points stops adding new
  siblings; a branch beside it that hasn't will naturally grow longer. This
  produces proportionally realistic crown shapes.

The outer loop maintains a queue of active `Branch` objects. Each iteration
runs one growth step on every branch in the queue, collecting new child
branches into the next generation's queue.

### 1 — Initialise

```
rng     = RNG(seed)
pts     = canopy_shape.sample(n_attraction, rng)   # quasi-random or rejection
root    = Node(position=root_xyz, radius=trunk_radius_mm, parent=None)
trunk   = Branch(tip=root_xyz, radius=trunk_radius_mm, points=pts, node=root)
queue   = [trunk]
```

### 2 — Outer loop

```
while queue is not empty:
    next_queue = []
    for branch in queue:
        children = grow_one_step(branch)
        next_queue.extend(children)
    queue = next_queue
```

### 3 — `grow_one_step(branch) → [Branch]`

Returns the list of child branches to add to the next queue (empty if terminal).

```
grow_one_step(branch):
    # --- Termination check ---
    if branch.radius < min_radius_mm or branch.points is empty:
        return []

    # --- 3a: primary direction (with smoothing) ---
    centroid     = mean(branch.points)
    centroid_dir = normalize(centroid - branch.tip)
    blended      = normalize(lerp(centroid_dir, branch.prior_dir, smoothing_alpha))
    direction    = enforce_min_up(blended, min_up_angle_deg)

    # --- 3b: find points that need a separate branch ---
    # A point is "stray" if the angle from the primary direction to that point
    # exceeds branch_split_angle_deg, meaning the primary branch will not
    # naturally converge on it.
    angle_to  = [angle_between(direction, p - branch.tip) for p in branch.points]
    stray     = [p for p, a in zip(branch.points, angle_to)
                   if a > branch_split_angle_deg]
    primary_points = [p for p in branch.points if p not in stray]

    # --- 3c: cluster strays into up to (max_branches_per_step - 1) groups ---
    # Reserve one slot for the primary continuation.
    clusters = cluster_angular(stray, tip=branch.tip,
                               max_k=max_branches_per_step - 1)

    # --- 3d: advance the tip ---
    new_tip      = branch.tip + direction * segment_length_mm
    new_tip_node = Node(position=new_tip, radius=branch.radius,
                        parent=branch.node)
    branch.node.children.append(new_tip_node)

    # --- 3e: consume attraction points near new_tip ---
    primary_points = [p for p in primary_points
                      if distance(p, new_tip) > kill_radius_mm]
    clusters       = [[p for p in c if distance(p, new_tip) > kill_radius_mm]
                      for c in clusters]
    clusters       = [c for c in clusters if c]   # drop now-empty clusters

    # --- 3f: build non-empty partition groups ---
    all_groups = [g for g in ([primary_points] + clusters) if g]
    if not all_groups:
        return []    # all points were consumed; this branch terminates

    # --- 3g: compute child radii (pipe model) ---
    n_total = sum(len(g) for g in all_groups)
    radii   = pipe_radii(branch.radius, [len(g) for g in all_groups],
                         branch_exponent)

    # --- 3h: construct child branches ---
    children = []
    for group, r in zip(all_groups, radii):
        child_centroid = mean(group)
        child_dir_raw  = normalize(child_centroid - new_tip)
        child_dir      = enforce_min_up(child_dir_raw, min_up_angle_deg)
        child_dir      = enforce_min_branch_angle(direction, child_dir,
                                                  min_branch_angle_deg)
        # Re-check min_up after the branch-angle adjustment; iterate until
        # both constraints are satisfied (converges in ≤ 3 passes in practice).
        for _ in range(3):
            adjusted = enforce_min_up(child_dir, min_up_angle_deg)
            if adjusted == child_dir:
                break
            child_dir = enforce_min_branch_angle(direction, adjusted,
                                                 min_branch_angle_deg)

        child_node = Node(position=new_tip, radius=r, parent=new_tip_node)
        new_tip_node.children.append(child_node)
        # prior_dir is set to the PARENT's direction (not child_dir) so that
        # the child's Bezier start tangent matches the parent's end tangent —
        # C1 continuity at the join. Direction smoothing then curves the child
        # toward its own point cloud over subsequent steps.
        children.append(Branch(tip=new_tip, radius=r, prior_dir=direction,
                               points=group, node=child_node))

    return children
```

**Note on stray detection timing:** stray classification uses `branch.tip`
(current position) but child directions are computed relative to `new_tip`
(one segment forward). The one-segment geometry shift is acceptable for the
resolutions this algorithm targets.

---

## Supporting Subroutines

### `enforce_min_up(dir, min_up_deg)`

If the elevation angle of `dir` is below `min_up_deg`, rotate `dir` upward
within its azimuth plane until the constraint is satisfied. Returns the
adjusted unit vector.

### `enforce_min_branch_angle(parent_dir, child_dir, min_angle_deg)`

If the angle between `parent_dir` and `child_dir` is less than `min_angle_deg`,
rotate `child_dir` away from `parent_dir` in the plane containing both, until
the divergence equals `min_angle_deg`. Returns the adjusted unit vector.

**Constraint interaction:** applying `enforce_min_up` then
`enforce_min_branch_angle` can push a direction below the horizontal minimum
again. Step 3h therefore re-applies `enforce_min_up` after the branch-angle
adjustment and iterates until stable (typically 1–2 extra passes).

### `cluster_angular(points, tip, max_k)`

Groups `points` into at most `max_k` directional clusters as seen from `tip`.
Compute `directions = [normalize(p - tip) for p in points]`, then cluster on
the unit sphere.

Recommended first implementation: split into two clusters along the axis of
greatest angular spread (PCA on the direction vectors). Returns a list of
point lists (one per cluster).

### `pipe_radii(r_parent, counts, e)`

```
n_total = sum(counts)
return [r_parent * (n / n_total) ** (1 / e) for n in counts]
```

This is **exact**: `sum(r_i^e) = r_parent^e * sum(n_i / n_total) = r_parent^e`. ✓

---

## Termination

A branch terminates (returns `[]` from `grow_one_step`) when **any** of:

- `branch.radius < min_radius_mm`
- `branch.points` is empty on entry
- All points consumed in step 3e and `all_groups` is empty after filtering

**Convergence guarantee:** the invariant `kill_radius_mm ≥ segment_length_mm`
ensures at least one point is consumed each step when any points remain within
the forward hemisphere. Combined with the pipe-model radius reduction whenever
points are consumed, every branch eventually reaches `min_radius_mm` or runs
out of points.

The outer loop is iterative (not recursive), so Python's recursion limit is not
a concern.

---

## Output

A tree is a node graph rooted at the `root` node. Each `Node` has:
- `position: vec3`
- `radius: float` — radius of the segment arriving at this node
- `parent: Node | None`
- `children: [Node]`

The mesh builder walks each `(parent, child)` edge and emits a swept-cylinder
or Bezier tube tapering from `parent.radius` to `child.radius`.

---

## Parameters to Tune

| Parameter | Effect if increased |
|---|---|
| `n_attraction` | Denser, more branchy canopy; more decision points |
| `min_up_angle_deg` | Steeper growth; removes drooping branches |
| `min_branch_angle_deg` | Wider splits; more open, airy crowns |
| `branch_split_angle_deg` | Higher = fewer branches triggered; more columnar growth |
| `segment_length_mm` | Longer segments; fewer decision points; coarser crown shape |
| `kill_radius_mm` | More aggressive consumption; fewer terminal twigs |
| `branch_exponent` | Gentler taper at each split; branches stay thicker longer |
| `max_branches_per_step` | More simultaneous splits; bushier |

---

## Open Questions

These must be resolved before implementation begins. Each will be discussed in
order.

1. **Clustering strategy** — ✅ PCA angular split. Find the axis of greatest
   angular spread among stray direction vectors, split at the midpoint into
   two clusters. A third group (if `max_branches_per_step = 3`) folds into
   the nearest PCA cluster.

2. **Direction smoothing** — ✅ Blend centroid direction with prior heading each
   step: `direction = lerp(centroid_dir, prior_dir, α)`, `α` tunable (default
   0.25). Adds `prior_dir: vec3` to `Branch` (initialised to `(0,0,1)` for
   trunk). Stray detection and child direction computation both use the smoothed
   direction, not the raw centroid direction.

3. **Segment curvature** — ✅ Cubic Bezier tubes. The mesh builder constructs
   each edge as a cubic Bezier: start tangent = `parent.prior_dir`, end tangent
   = `child.prior_dir`. C1 continuity at branch joins is achieved by
   initialising each child's `prior_dir` to the **parent's `direction`** at the
   moment of the split (not `child_dir`). The child's direction smoothing then
   curves it away from the parent heading toward its own point cloud over
   subsequent steps — matching biological branch geometry naturally.

4. **Stochastic jitter** — deferred. Evaluate crown shape first.

5. **Gravity sag** — deferred. Evaluate crown shape first.
