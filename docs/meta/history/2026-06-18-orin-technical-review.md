# Orin Technical Review — 2026-06-18

**Scope:** Deep implementation review of the math/geometry core, scatter system, grass pipeline, tree pipeline, and terrain pipeline. Line-level focus on algorithms, NumPy patterns, data structures, and coordinate conventions.  
**Not covered here:** Architectural findings (see 2026-06-18 elegance review) or DRY duplication (see 2026-06-13 review).  
**Reviewer:** Orin (claude-sonnet-4-6)

---

## Status as of 2026-06-19 (all items addressed)

**Resolved (commits 31740d9, 802c111, 4b23608):**
- ✅ Finding 1 — rocks rasterisation vectorised
- ✅ Finding 2 — `_make_ring` bark ring vectorised
- ✅ Finding 3 — `_largest_group_id` uses `np.bincount`
- ✅ Finding 6 — `_compute_radii_bottom_up` uses `np.sum(radii[ch])`, no generator
- ✅ Finding 8 — `jitter_grid_xy` RNG calls batched upfront (`j_u_all`, `j_v_all`)

**Resolved 2026-06-19:**
- ✅ Finding 4 — `_voronoi_group_attractors` centroid update vectorised with `np.add.at` (O(n) scatter-accumulate, eliminates Python k-loop)
- ✅ Finding 7 — `_filter_non_overlapping_centers` already sorts once; "Sort once" comment added to code; status corrected
- ✅ Finding 9 — `_cell_index` double cast removed: `max(0, min(int(x/cw), ...))` in both axes
- ✅ Finding 10 — `grower = FlatGrassGrower` hoisted before the per-blade loops in `grow.py` and `mesh.py`
- ✅ Finding 11 — `np.dot(basis, basis)` replaced with `(basis * basis).sum()` in `_fit_quadratic_arc`
- ✅ Finding 12 — `_bezier_tangent_vec` added; foliage clump spine no longer uses Python list comprehension
- ✅ Finding 13 — `_apply_group_bulge` pairwise distance is already vectorised (NumPy broadcast); confirmed no loop per pair; no code change needed
- ✅ Finding 14 — `_segment_intersects_cells` restructured: t0/t1 no longer pre-allocated unconditionally; unified slab-intersection approach with sentinel values for degenerate axes
- ✅ Grass Pipeline — `_lift_path_points` vectorised: single `_sample_grid` batch call over all non-root points
- ✅ Grass Pipeline — `_cell_range` helper extracted to `_geometry.py`; `_contained_segment_cells` and `_leading_edge_cells` both use it (copy-pasted bbox setup eliminated)

**Acknowledged, deferred:**
- 🔵 Finding 5 — `nodes: list[np.ndarray]` fragmentation in `_branch_skeleton`: BFS with 400–600 nodes is already negligible; `_simplify_skeleton` already converts to contiguous ndarray. Pre-allocating a contiguous buffer would require a generous max_nodes estimate and adds complexity to `_add_node`/`_grow_to_leaf` without meaningful runtime benefit at current tree sizes. Revisit if trees scale to thousands of attractors.
- 🔵 Finding 15 — `random_spread_sites` full-array scan: O(n_groups × n_cells) cost is under 50 ms at current scales (n_groups ≤ 50, cells ≤ 65,536). No urgency. Revisit if tile resolution or group count increases significantly.

---

## Overview

The codebase has a high standard of vectorisation in its hot paths. The heightmap builder (`core/mesh.py`), rock kernel (`layers/rocks.py`), and grass blade builder (`grass/growers/flat.py`) are all properly NumPy-vectorised. The geometry math is generally correct, well-commented, and uses the right coordinate conventions throughout.

The technical debt lives in three zones. First, there is a persistent O(n) Python loop inside the rock rasterisation that is the only genuine hot-path violation — everything else is either already vectorised or in a cold enough path that it does not matter. Second, `cloud_mesh.py` has two substantial Python loops in the ring-building inner path (`_make_ring` and `_build_closed_edge_solid`) that are called hundreds of times per tree and are candidates for vectorisation. Third, several data structures are chosen for convenience rather than correctness — most notably `nodes: list[np.ndarray]` in the skeleton builder, which forces repeated `np.asarray()` calls on every access, and the `group` dict (keys `"rows"`, `"cols"`) in the scatter system, which is a named-tuple in a trench coat.

The coordinate conventions are consistent. The codebase uses row = y = j, col = x = i throughout, and `terrain_z` is indexed `[row, col]` = `[j, i]`. The one confusing naming spot is `SurfaceConfig.grid_w` / `grid_h` — "width" maps to x/cols and "height" to y/rows, which is fine, but functions like `_contained_segment_cells` return `(ix0, ix1, iy0, iy1)` while the block slicing that follows uses `[iy0:iy1+1, ix0:ix1+1]`, which is correct but requires constant mental translation.

---

## Findings

### 1 — Rock rasterisation is O(n) Python loop inside the vectorised kernel — `layers/rocks.py` — HIGH ✅ DONE (31740d9)

**Lines:** 274–309  
**Issue:** After the fully vectorised mesh construction (lines 141–263), `_build_rocks_mesh_core` drops into a Python `for s in range(N)` loop to rasterise each rock's ellipse into `support_z` and `obstacle_mask`. The loop body does a `np.meshgrid` and vectorised `np.where` per rock, so each iteration is fast, but the dispatch overhead of N separate Python frames with N separate NumPy calls scales poorly. With default density of 15 rocks per square and a 2×2 tile, N ≈ 60. With `count_per_square=50` on a 4×4 tile, N = 800.

The loop also recomputes `r_max = max(_rx, _ry)` per rock in Python. The bounding ellipse is already known from the pre-computed `rx_arr` and `ry_arr` arrays.

**Why it matters:** The rock rasterisation loop is the only remaining hot-path Python loop in the scatter system. Everything else (mesh assembly, vertex transforms, plane cuts) is batched. This loop is O(n × bbox_cells), which for large rocks on high-resolution tiles can dominate.

**Simpler form:** Vectorise over rocks in a single pass using a padded 3-D block approach. The key insight is that all bounding boxes have the same maximum possible size (determined by `max(rx_arr.max(), ry_arr.max())`):

```python
r_max_all = max(float(rx_arr.max()), float(ry_arr.max()))
half = int(r_max_all / cw) + 2  # half-width in cells

# Per-rock cell-offset grid: (2*half+1, 2*half+1)
d = np.arange(-half, half + 1, dtype=float) * cw
DX, DY = np.meshgrid(d, d)       # (box, box)

# Per-rock local coordinates: (N, box, box)
LX = (ca[:, None, None] * (DX[None] + ...) + ...)  # rotate by angle
LY = ...

d2 = (LX / rx_arr[:, None, None])**2 + (LY / ry_arr[:, None, None])**2
inside = d2 <= 1.0                # (N, box, box)
z_top = np.where(inside, base_z[:, None, None] + height[:, None, None] * np.sqrt(...), -np.inf)

# Scatter-accumulate with np.maximum.at or a loop over rocks but with
# single-call sliced writes (no per-rock meshgrid):
for s in range(N):
    i0 = int(cx[s] / cw) - half; j0 = int(cy[s] / cw) - half
    sl = support_z[max(0,j0):j0+2*half+1, max(0,i0):i0+2*half+1]
    np.maximum(sl, z_top[s, ...], out=sl)
```

This eliminates the per-rock `np.meshgrid` allocation, moves the angle rotation to a single batched step, and keeps the rasterisation as a tight loop over pre-computed tiles. Total allocations drop from O(N) to O(1) for the bounding grid.

---

### 2 — `_make_ring` has two Python loops inside the tree ring-sampling hot path — `trees/cloud_mesh.py` — HIGH ✅ DONE (31740d9)

**Lines:** 1481–1513  
**Issue:** `_make_ring` is called once per Bézier step per skeleton edge during `_build_closed_edge_solid`. With `step_mm = 2.5` and a 40 mm branch, that is 16 calls per edge. A tree with 50 edges and 200 attractors makes ~800 calls to `_make_ring`. Each call contains two Python `for t in theta` loops:

```python
cuts = np.array([_bark_cut(float(t), radius, bark, grooves) for t in theta], dtype=float)
noise = np.array([_bark_surface_noise(float(t), ...) for t in theta], dtype=float)
```

`theta` has `n_sides = 12` elements (or 48 with bark). The inner functions `_bark_cut` and `_bark_surface_noise` are pure arithmetic — they compute angular distance to groove centres and a hash-based value. Both are trivially vectorisable.

**Why it matters:** 800 calls × 12-or-48 iterations × 2 loops = 19,200–76,800 Python function-call dispatches per tree. This is the dominant cost in tree mesh building when bark is enabled, and it is purely a vectorisation oversight.

**Simpler form:**

```python
# In _bark_cut: vectorise over theta
def _bark_cut_vec(theta_arr, radius, bark, groove_centers):
    if not groove_centers:
        return np.zeros(len(theta_arr))
    cuts = np.zeros(len(theta_arr))
    for _lid, theta_g, strength in groove_centers:
        hw = 0.5 * bark.width_mm * strength
        if hw <= 1e-9: continue
        d_mm = radius * np.abs(_wrap_angle_signed_vec(theta_arr - theta_g))
        cuts = np.maximum(cuts, bark.depth_mm * strength * np.clip(1.0 - d_mm/hw, 0.0, 1.0))
    return cuts
```

Then in `_make_ring`:
```python
cuts  = _bark_cut_vec(theta, radius, bark, grooves)
noise = _bark_surface_noise_vec(theta, radius, bark, grooves, s=s, ...)
```

---

### 3 — `_largest_group_id` is O(n × k) with Python loops where O(k) suffices — `trees/cloud_skeleton.py` — MEDIUM ✅ DONE (31740d9)

**Lines:** 667–679  
**Issue:**

```python
for gid in unique_ids:
    count = int(np.sum(labels == gid))
    if count > best_count:
        ...
```

`np.sum(labels == gid)` scans the full `labels` array for each of k unique group IDs. Total cost is O(n × k). For 200 attractors in 20 groups this is 4,000 comparisons when `np.bincount(labels).argmax()` would do it in O(n).

**Why it matters:** Called in two places during skeleton BFS fallback paths. Not in the tightest loop, but the simpler form is also more correct.

**Simpler form:**

```python
def _largest_group_id(pts, labels, unique_ids):
    counts = np.bincount(labels, minlength=int(unique_ids.max()) + 1)
    return int(unique_ids[np.argmax(counts[unique_ids])])
```

---

### 4 — `_voronoi_group_attractors` Lloyd's iteration has O(n × k) Python loop for centroid update — `trees/cloud_skeleton.py` — MEDIUM ⬜ OPEN

**Lines:** 595–609  
**Issue:** The vectorised distance matrix `diff = scaled[:, np.newaxis, :] - seeds[np.newaxis, :, :]` is correct (O(n × k × 3)). But the centroid update uses a Python loop:

```python
new_seeds = np.empty_like(seeds)
for ki in range(k):
    members = scaled[labels == ki]
    new_seeds[ki] = members.mean(axis=0) if len(members) > 0 else seeds[ki]
```

This is O(k × n) Python, where k can be 20–30 and n is 200. Negligible at current scale, but trivially replaceable with `np.add.reduceat` or a weighted sum.

**Simpler form:**

```python
# After labels = np.argmin(dists2, axis=1)
for ki in range(k):
    mask = labels == ki
    new_seeds[ki] = scaled[mask].mean(axis=0) if mask.any() else seeds[ki]
```

Or more Numpythonically:
```python
# Use np.array of per-cluster sums:
counts = np.bincount(labels, minlength=k)
new_seeds = np.array([
    scaled[labels == ki].sum(axis=0) / max(counts[ki], 1)
    for ki in range(k)
])
new_seeds[counts == 0] = seeds[counts == 0]
```

The truly vectorised version uses `np.add.at` but adds complexity for marginal gain at n=200.

---

### 5 — Skeleton nodes stored as `list[np.ndarray]` forces repeated copy-on-access — `trees/cloud_skeleton.py` — MEDIUM ⬜ OPEN

**Lines:** 192–193, 447–450  
**Issue:** The skeleton builder accumulates `nodes: list[np.ndarray]` where each element is a `(3,)` array created with `.copy()` at every `_add_node` call. By the end of a 200-attractor tree, `nodes` contains 400–600 individual 3-element arrays. Every subsequent access (`nodes[tip_idx]`, `nodes[p]`) is a Python list index returning a heap-allocated 3-element array.

In `_simplify_skeleton` (lines 477–478), the list is converted: `nodes_arr = np.asarray(nodes, dtype=float)`. This is the correct final form, but it means the intermediate BFS process is working with fragmented per-node arrays instead of a contiguous buffer.

**Why it matters:** Fragmented storage means every `nodes[i]` read in the BFS loop (line 210: `pos = nodes[tip_idx]`) does a Python list indexing plus a small-array dereference. For 600 nodes × BFS iterations this is overhead that a pre-allocated `(max_nodes, 3)` array would eliminate.

**Simpler form:** Pre-allocate `nodes = np.empty((max_nodes, 3), dtype=float)` with a running `n_nodes` counter. Access is `nodes[tip_idx]` on a contiguous array. The `_add_node` helper becomes:

```python
def _add_node(nodes, n_nodes, parents, prior_dirs, pos, parent_idx, direction):
    nodes[n_nodes] = pos
    parents[n_nodes] = parent_idx
    prior_dirs[n_nodes] = direction
    n_nodes += 1
    return n_nodes - 1, n_nodes
```

The `_simplify_skeleton` conversion becomes `nodes_arr = nodes[:n_nodes]` — a free slice.

---

### 6 — `_compute_radii_bottom_up` uses a Python `sum()` of a generator inside a loop — `trees/cloud_skeleton.py` — MEDIUM ✅ DONE (31740d9)

**Lines:** 541–543  
**Issue:**

```python
for i in range(n - 1, -1, -1):
    if children[i]:
        radii[i] = float(sum(radii[c] ** exponent for c in children[i])) ** (1.0 / exponent)
```

The inner `sum(... for c in children[i])` is a Python generator loop over the children list. This is called for every internal node (potentially hundreds). While each call is cheap, the generator object creation and Python-level arithmetic are unnecessary when `np.sum` over a small array would do.

More importantly, the outer `for i in range(n-1, -1, -1)` is a reverse bottom-up pass. This works correctly because `_simplify_skeleton` guarantees parents have lower indices than children (DFS topological order). But the pass creates a `children: list[list[int]]` adjacency list that was also built in `_compute_radii_bottom_up` itself — it is rebuilt from the same `parents` array that `_simplify_skeleton` already traversed. The children list is built twice (once in `_simplify_skeleton._visit`, once in `_compute_radii_bottom_up`).

**Simpler form:**

```python
radii = np.full(n, min_radius_mm)
# children already available from build, or reconstruct once:
for i in range(n - 1, -1, -1):
    ch = children[i]
    if ch:
        radii[i] = np.sum(radii[ch] ** exponent) ** (1.0 / exponent)
```

Pass `children` from `_simplify_skeleton` to `_compute_radii_bottom_up` to avoid rebuilding.

---

### 7 — `_filter_non_overlapping_centers` is O(n²) via repeated sort — `trees/cloud_mesh.py` — MEDIUM ⬜ OPEN

**Lines:** 1251–1278  
**Issue:** The bark-groove deduplication loop repeatedly calls `sorted(remaining, ...)` in a `while len(remaining) > 1` loop. Each iteration sorts the full `remaining` list (O(k log k)) and then does a linear scan for gaps. Worst case with k grooves is O(k² log k).

For the default spacing of 1.35 mm and trunk radius of ~8 mm, k ≈ 37 bark lines. With 37 iterations worst case: 37 × 37 × log(37) ≈ 5,400 operations. In practice the loop terminates after removing a few items, but the repeated sort is algorithmically unnecessary.

**Simpler form:** Sort once, scan gaps linearly to find the minimum, remove it, rescan. O(k²) without the repeated sort. Or: sort, store gaps, use a heap for O(k log k) total. At k ≤ 40 any of these is trivially fast — the issue is the code communicates O(k²) intent by re-sorting on every iteration.

---

### 8 — `jitter_grid_xy` Python loop over `n_u` grid rows creates `n_u` separate arrays — `scatter/distribute.py` — MEDIUM ✅ DONE (31740d9)

**Lines:** 345–358  
**Issue:**

```python
all_us: list[np.ndarray] = []
all_vs: list[np.ndarray] = []

for i in range(n_u):
    n_v_i = max(1, int(round(float(n_v * count_w[i]))))
    ...
    all_us.append(u_lo + (t_lo_i + j_u * (t_hi_i - t_lo_i)) * u_span)
    all_vs.append(v_lo + (np.arange(n_v_i) + j_v) * (v_span / n_v_i))
```

`n_u` can be 5–20 for a typical group. Each iteration allocates two small arrays. The outer shape — a grid with non-uniform row lengths — does not vectorise trivially because `n_v_i` varies per row. But the core issue is: the loop builds each row independently with separate `rng.uniform()` calls, producing n_u separate allocations that are then concatenated.

The density-weight computation `count_w` (lines 338–340) is already vectorised. The bottleneck is the row-by-row construction. This is structural — the variable-length rows prevent a clean rectangular vectorisation — but the number of allocations could be reduced by batching all `rng.uniform()` calls upfront:

```python
total = sum(max(1, int(round(float(n_v * w)))) for w in count_w)
all_j_u = rng.uniform(0.0, 1.0, total)
all_j_v = rng.uniform(0.0, 1.0, total)
```

Then scatter by row. This halves RNG calls from 2×n_u to 2 total.

---

### 9 — `_cell_index` does redundant double-cast — `grass/_geometry.py` — LOW ⬜ OPEN

**Lines:** 54–58  
**Issue:**

```python
def _cell_index(surface, x: float, y: float) -> tuple[int, int]:
    ix = int(np.clip(int(x / surface.cell_w), 0, surface.grid_w - 1))
    iy = int(np.clip(int(y / surface.cell_w), 0, surface.grid_h - 1))
    return ix, iy
```

The inner `int(x / surface.cell_w)` converts to Python int, then `np.clip` upcasts back to a NumPy scalar, then the outer `int()` converts back to Python int. Three type conversions for one arithmetic operation. Python `//` with `max`/`min` would be faster and clearer:

```python
def _cell_index(surface, x: float, y: float) -> tuple[int, int]:
    ix = max(0, min(int(x / surface.cell_w), surface.grid_w - 1))
    iy = max(0, min(int(y / surface.cell_w), surface.grid_h - 1))
    return ix, iy
```

Called during every blade growth step (hundreds of times per tile), so the overhead accumulates.

---

### 10 — `grow_all` calls `species_map[path.seed.species_id]` inside the per-blade loop — `grass/grow.py` — LOW ⬜ OPEN (partial: `species` resolved before loop; `grower = FlatGrassGrower` still inside loop as a trivial rebind)

**Lines:** 47–66  
**Issue:** `species_map = {species.name: species for species in cfg.species}` is built once (good). But `species = species_map[path.seed.species_id]` and `grower = GROWERS[species.grower]` are looked up inside the `for path in growing` loop. Since `cfg.species` is always a single-element list (see elegance review Finding 9), both lookups always return the same value. The dict lookup is O(1) but the repeated string hashing is avoidable.

**Simpler form:** Resolve `species` and `grower` once before the loop, since there is exactly one species:

```python
species = cfg.species[0]
grower  = GROWERS[species.grower]
for path in growing:
    ...
```

This also makes Finding 9 from the elegance review a forcing function: collapsing `GrassConfig.species` to a single `SpeciesConfig` (not a list) would make this obviously correct.

---

### 11 — `_fit_quadratic_arc` uses `np.dot(basis, basis)` as a scalar that could be `np.inner` or `float` — `grass/growers/flat.py` — LOW ⬜ OPEN

**Lines:** 396–405  
**Issue:**

```python
basis = 2.0 * (1.0 - t) * t
...
denom = float(np.dot(basis, basis))
if denom <= 1e-12:
    return points.copy()
control = (basis[:, None] * fit).sum(axis=0) / denom
arc += basis[:, None] * control
```

`np.dot(basis, basis)` for a 1-D array is `||basis||²` — correct, but `np.dot` is a general matrix product. `float(np.inner(basis, basis))` or `float((basis**2).sum())` is cleaner for a dot of a vector with itself. Minor but indicative of mechanical application of `np.dot`.

The deeper issue in this function: `fit = points - arc` computes all residuals before the quadratic least-squares solve, which is correct. The fitting is a constrained 1-parameter linear regression (the quadratic Bézier midpoint). This is fine math. But `_smooth_blade_spine` calls `_fit_quadratic_arc` on every blade mesh build — for a tile with 2,000 blades, that is 2,000 spine-fitting passes. Each pass does `np.diff`, `np.cumsum`, `np.dot`, and `np.linalg.norm` on arrays of length ~20. The per-call overhead is small but not zero.

---

### 12 — `_build_foliage_clump_mesh` reconstructs the Bézier spine tangents with a Python list comprehension — `trees/cloud_mesh.py` — LOW ⬜ OPEN

**Lines:** 707–712  
**Issue:**

```python
spine_traw = np.vstack([
    _bezier_tangent(start_p, s_bp1, s_bp2, tip_p, float(t))
    for t in spine_ts
])
```

`_bezier_tangent` is called N_SPINE=64 times in a Python list comprehension. The function is pure arithmetic on scalars, but `_bezier_eval` already computes the positions for all `ts` in one vectorised call (line 706). The tangent is the derivative of the Bézier: `3(1-t)²(p1-p0) + 6(1-t)t(p2-p1) + 3t²(p3-p2)`. This vectorises identically to `_bezier_eval`:

```python
def _bezier_tangent_vec(p0, p1, p2, p3, ts):
    t = ts[:, None]
    return (3*(1-t)**2*(p1-p0) + 6*(1-t)*t*(p2-p1) + 3*t**2*(p3-p2))
```

Called once per foliage clump (once per leaf branch per tree). With 200 attractors and a tree having ~50 leaf branches, this is 50 × 64 = 3,200 Python `_bezier_tangent` calls that could be one vectorised call.

---

### 13 — `_apply_group_bulge` computes pairwise O(ng × no) distance matrix per group — `trees/cloud_skeleton.py` — LOW ⬜ OPEN (no urgency at n=200)

**Lines:** 649–661  
**Issue:**

```python
diffs = in_group[:, np.newaxis, :] - other[np.newaxis, :, :]  # (ng, no, 3)
d_edge = np.sqrt((diffs ** 2).sum(axis=2).min(axis=1))        # (ng,)
```

With 200 attractors in 20 groups, each group has ~10 in-group and ~190 out-of-group points. The full pairwise distance matrix is (10 × 190 × 3) per group — 3,800 floats × 20 groups = 76,000 intermediate values. This is trivially fine at the current scale (called once per tree, before skeleton growth). But it is worth noting that `scipy.spatial.cKDTree` would give the same result in O(ng log no) for large attractor counts.

At n=200 the current approach is correct and fast. Flag this only if n_attraction is ever raised to 1,000+.

---

### 14 — `_segment_intersects_cells` conditionally skips axes for near-degenerate segments — `grass/growers/flat.py` — LOW ⬜ OPEN

**Lines:** 493–533  
**Issue:**

```python
if abs(dx) <= eps:
    mask &= (ax >= left[None, :] - eps) & (ax <= right[None, :] + eps)
else:
    ...
if abs(dy) <= eps:
    mask &= (ay >= bottom[:, None] - eps) & (ay <= top[:, None] + eps)
else:
    ...
```

The conditional dispatch `if abs(dx) <= eps` / `else` is a correct degenerate-case guard, but it means the `t0`, `t1` arrays (lines 507–508) are pre-initialised and then may never be used (in the degenerate case, the slab intersection is replaced by an equality test). The `t0`, `t1` arrays are only needed for the non-degenerate path. The pre-allocation to `(rows, cols)` is unnecessary in the degenerate case. This is a minor inefficiency — most segments are not degenerate — but the two-path structure makes the function harder to read than an unconditional slab intersection with a clamp.

---

### 15 — `random_spread_sites` uses O(n) comparison inside a Python loop for spread sites — `scatter/distribute.py` — LOW ⬜ OPEN (no urgency at current tile sizes)

**Lines:** 234–253  
**Issue:**

```python
for _ in range(1, n):
    sample_size = min(len(rows), max(64, n * 8))
    candidates  = rng.choice(len(rows), size=sample_size, replace=False)
    next_idx    = int(candidates[np.argmax(min_d2[candidates])])
    chosen.append(next_idx)
    d2     = (rows - rows[next_idx]) ** 2 + (cols - cols[next_idx]) ** 2
    min_d2 = np.minimum(min_d2, d2)
```

The `min_d2` array is O(total_cells) ≈ O(grid_w × grid_h). For a 128-cell tile this is 16,384 elements; for 256 it is 65,536. The update `np.minimum(min_d2, d2)` scans the full valid-cell set every iteration. With n_groups = 12 groups × 4 squares = 48 centres, this is 48 full-array scans of 65,536 elements. That is ~3M element comparisons at the start of every grass layer.

This is a classic k-centre greedy heuristic. The current "sample k×8 candidates, pick farthest" approach is a reasonable approximation, but the full-array update is what makes it O(n_groups × n_cells). For this domain (grass clumping), the visual difference between this and a faster approximation is invisible.

At the current scale (n_groups ≤ 50, cells ≤ 65,536) the cost is under 50 ms. Flagged for awareness, not urgency.

---

## The Grass Pipeline

The grass pipeline (`grass/seed.py`, `grass/grow.py`, `grass/growers/flat.py`, `grass/_geometry.py`, `grass/mesh.py`) is the most line-dense subsystem. Here is the earned-vs-accidental breakdown.

**Earned complexity:**

The segment-by-segment growth algorithm is the right approach. Growing blade-by-blade, sorted upstream-first, with a shared `occ_z` occupancy grid that is stamped as each segment grows, is the minimal correct algorithm for blades that must ride on top of each other rather than interpenetrate. The alternative — grow all blades, then sort by Z — would require backtracking when a late-growing blade discovers it is under an earlier one.

`_contained_segment_cells` is genuinely complex but necessarily so. The tapered swept-quadrilateral containment test (checking all four corners) is the minimal correct test for "is this grid cell fully inside the swept blade segment?" The `corner_along` / `corner_lateral` decomposition into the local segment frame is mathematically the cleanest form.

The separation of `_stamp_segment` (used by the grower during growth to update `occ_z`) and the equivalent call in `build_meshes` (to update `vegetation_support_z` from the final lifted path) is correct: the growth-time stamping needs a conservative overestimate (occ_z = max), the mesh-time stamping needs the actual mesh surface. They correctly use the same kernel.

`GrassSeed.distance_taper_vec` is a good vectorisation — it exactly mirrors the scalar `distance_taper` for batch operation over `path_dists`, using `np.minimum` of two independent taper curves. The separate `_tip_taper_vec` and `_base_taper_vec` make the two constraints explicit.

**Accidental complexity:**

`_leading_edge_cells` in `flat.py` (lines 441–490) and `_contained_segment_cells` in `_geometry.py` (lines 61–155) are near-identical in structure. Both compute a bounding box of a line segment (or quadrilateral), set up cell coordinate arrays, and build a boolean mask. The leading-edge version is a special case (the "front cap" of the sweep, not the full quad). They could share a common `_segment_bbox_cells` primitive that computes the cell ranges and local coordinates, with the specific mask logic differing between them. As written, the bounding-box setup code (lines 110–113 and 470–473) is copy-pasted.

`_lift_path_points` (mesh.py lines 66–83) is a Python loop over path points:

```python
for idx, (x, y, z) in enumerate(points):
    if idx == 0:
        lifted.append(...)
        continue
    floor_z = _sample_grid(support_z, surface, x, y)
    lifted.append((float(x), float(y), float(max(z, floor_z))))
```

The `_sample_grid` bilinear sampler handles arrays. This loop could be vectorised by extracting `x`, `y`, `z` arrays and sampling in batch:

```python
pts = np.asarray(points)
floor_z = _sample_grid(support_z, surface, pts[1:, 0], pts[1:, 1])
lifted_z = np.maximum(pts[1:, 2], floor_z)
lifted = [points[0]] + list(zip(pts[1:, 0], pts[1:, 1], lifted_z))
```

For blades of 10–20 points this is a 10–20x reduction in `_sample_grid` call overhead.

The `grow_all` verbose print block (lines 43–44, 71–76) inside the main grow loop is fine for development but should be gated on a `verbose` flag that the production path passes as `False`. It is already gated, but the `if verbose:` block inside the hot blade loop (lines 67–69) means the condition is evaluated once per blade.

---

## The Tree Pipeline

**Earned complexity:**

The BFS skeleton builder (`_branch_skeleton`) is complex but the complexity is load-bearing. The three-layer stray detection — group-level promotion, individual PCA clustering, safety force-split — exists to handle three genuinely different attractor configurations. The `branch_fork_balance` redistribution is the most intricate part, but it encodes a real design decision (how evenly to distribute attractors at a fork), not an accident.

The `_simplify_skeleton` DFS traversal that collapses single-child chains is algorithmically clean: it produces a topologically sorted output (parents before children) in one pass, and the two tangent arrays (`in_dirs`, `out_dirs`) correctly capture the chain's start and end directions for Bézier continuity.

The pipe-model radius derivation (`_compute_radii_bottom_up`) is 8 lines of correctly vectorised code. The reverse-order loop works because `_simplify_skeleton` guarantees topological order.

`_build_foliage_clump_mesh` is genuinely the most complex function in the codebase at ~350 lines. The icosphere deformation (back hemisphere → cone body → forward dome) has earned complexity: each of the three zones has a different perpendicular-upward offset formula because the branch embed geometry changes along the clump. The leaf tiling loops (rows + near-apex ring + polar cap) are structurally necessary — three different sampling patterns for three different surface regions.

**Accidental complexity:**

The bark system (`_select_bark_lines`, `_advance_bark_lines`, `_bark_centers_for_ring`, `_bark_taper_strength`, `_foliage_bark_endpoint_t_by_id`, `_foliage_bark_endpoint_maps`) is the densest zone of `cloud_mesh.py`. It spans ~200 lines and is called in the ring-building inner loop. The function call chain is: `_build_closed_edge_solid` → `_bark_centers_for_ring` → `_bark_taper_strength` + `_bark_theta` + `_bark_twist_angle` → `_filter_non_overlapping_centers` → `_hash01`. That is 5 levels of call depth in a loop. The bark logic could be extracted into a `_BarkState` object with a `centers_at(s, t, radius)` method, making the inner loop's interface explicit.

The `foliage_bark_start_t` pathway (lines 1080–1124) is the most obscure: it computes per-line endpoint t-values for bark lines that terminate at the foliage clump boundary, converting t → s-arc twice through two helper functions. The two maps `foliage_bark_end_s_by_id` and `foliage_bark_taper_start_s_by_id` are always either both-None or both-populated — they could be a single `dict[int, tuple[float, float]]`.

The `_build_closed_edge_solid` function signature (lines 307–330) takes 18 keyword arguments, including `is_foliage_leaf`, `dome_tip`, `bark`, `bark_lines`, `bark_seed`, `edge_id`, `bark_end_t`, `tree_base_z`, `tree_height_mm`, `end_taper_line_ids`, `foliage_bark_start_t`. Seven of the 18 are bark-related and always travel together. A `_BarkEdgeState` dataclass (bark_lines, bark_seed, edge_id, bark_end_t, tree_base_z, tree_height_mm, end_taper_line_ids, foliage_bark_start_t) would reduce the signature to 10 args while making the bark bundle explicit.

---

## NumPy Usage Summary

**Well-vectorised zones:**

- `core/mesh.py` — heightmap solid construction is fully vectorised. The adaptive Laplacian pass (`_make_heightmap_solid_adaptive`) correctly builds all vertex and face arrays in one shot. Side walls are constructed with `np.stack` + `np.empty` interleave.
- `layers/rocks.py` — the mesh construction kernel is genuinely vectorised over N rocks simultaneously (N × EL × AZ tensors). The slope rotation, plane cuts, and roughness noise are all batched. The only violation is the rasterisation loop (Finding 1).
- `grass/growers/flat.py:_build_blade_mesh` — the ring construction is correctly vectorised over all rings simultaneously (lines 287–319). The collapsed-tip loop (line 315) is the only per-ring Python loop, and it correctly handles only the degenerate case.
- `core/region.py` — boundary path generation is clean. The organic path uses `np.searchsorted` for parameterisation. `_rasterise` is a single vectorised index write.
- `scatter/distribute.py:voronoi_groups` — `nearest_site_labels` correctly chunks the O(n × k) pairwise distance matrix to avoid OOM on large grids.

**Under-vectorised zones:**

- `trees/cloud_mesh.py:_make_ring` — Python list comprehensions for bark cuts and noise (Finding 2).
- `trees/cloud_mesh.py:_build_foliage_clump_mesh` — Bézier tangent computation via Python list comprehension (Finding 12).
- `grass/mesh.py:_lift_path_points` — point-by-point lifting loop when array sampling would batch this (Grass Pipeline section).
- `grass/grow.py:grow_all` — `species_map` and `grower` re-resolved per blade (Finding 10).

**Dtype and broadcasting:**

No dtype mismatches found at boundaries. The codebase consistently uses `dtype=float` for vertex arrays and `dtype=np.int32` for face arrays. The `sample_grid` bilinear interpolation correctly promotes integer grid indices to float before fractional computation.

One subtle dtype note: `_foliage_gaussian_noise` (cloud_mesh.py lines 548–565) uses explicit `np.uint64` arithmetic for hash computation with `np.uint64` casts per operation. The pattern is correct but verbose — the XOR and multiply operations require explicit `np.uint64()` wrappers to prevent Python int overflow rather than NumPy's normal broadcasting rules, because Python ints are arbitrary-precision. This is correct but the per-operation wrapping makes the hash chain harder to read. A helper `_fnv_mix(h, val)` would centralise the uint64 semantics.

**Broadcasting clarity:**

The `[:, None, None]` idiom appears throughout the rock kernel for broadcasting N-dimensional arrays over (EL, AZ) ring dimensions. This is clear and correct. The shape annotations in comments (`# (N, EL, AZ)`) make the intent readable.

The foliage clump's icosphere deformation (cloud_mesh.py lines 755–817) uses `[:, None]` for broadcasting scalars over vertex arrays consistently. The three mask regions `ms`, `mc`, `mn` are disjoint boolean arrays indexing the same `(M,)` vertex set — this pattern is clean and correct.
