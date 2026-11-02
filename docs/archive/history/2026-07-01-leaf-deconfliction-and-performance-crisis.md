# Leaf Deconfliction — Design Session 2026-07-01

## What We Were Working On

Adding anti-intersection logic between placed leaf solids so that no two leaves
overlap or come within 0.4 mm of each other's surface.  This grew out of the
observation that the meridian-arc placement algorithm places leaves
independently of one another: each leaf is solved against the parent cluster
mesh only, with no awareness of neighbouring leaves.

---

## What We Built

### `_DECONF_CLEARANCE_MM = 0.4`

Module constant.  All intersection checks use this as the minimum required
surface-to-surface gap.

### `_leaf_intersects_placed(solid, placed_index, placed_meshes)`

Post-placement intersection test.  For each candidate leaf (pre-filtered by
R-tree bbox expanded by the clearance):

- `trimesh.proximity.closest_point(prev, solid.vertices)` → min distance < 0.4 mm → conflict
- `trimesh.proximity.closest_point(solid, prev.vertices)` → min distance < 0.4 mm → conflict

Both directions checked because thin leaves can interpenetrate without any
vertex of one being "inside" the other in the containment sense.

The placed-leaf R-tree (`ctx.placed_index`) and mesh list (`ctx.placed_meshes`)
were already wired into `_MeshCtx` and updated after every successful placement
— we just started reading them.

### Two-strategy deconfliction in `_place_leaf_slot`

Runs only when the initial solid fails the clearance test.

**Strategy 1 — Lift**: Increase `lift_mm` in 0.1 mm steps up to
`base_lift + 2 × thickness_mm`.  The oval (`inner_v`) is lift-independent and
reused unchanged.  For default `thickness_mm = 0.24`, budget = 0.48 mm
(~5 steps).  First lift that passes the clearance test wins.

**Strategy 2 — Raise**: Translate `pt3d` along `up_hint` (the outward surface
normal at the attachment point) by 0.2, 0.4, or 0.6 mm.  Oval rebuilt at the
new base; containment check skipped (small raise is acceptable even if the oval
root extends fractionally outside the parent mesh).  First raise that passes wins.

**Fallback**: If neither strategy clears the gap, the original leaf is placed
anyway.  The leaf is not discarded on collision grounds; the existing
curl-region burial check still runs independently.

---

## Why This Approach Doesn't Work

### 1. Still producing lots of intersections

The algorithm cannot clear many conflicts because:

- Lift budget is tiny (≤ 0.48 mm for a 0.24 mm thick leaf).  Leaves that
  genuinely overlap a neighbour by a full leaf-width need a qualitatively
  different response, not a fraction-of-a-mm tip nudge.
- Raise budget is also tiny (max 0.6 mm) and skips the containment check,
  so many raises produce invalid geometry.
- The fallback ("just leave it") means the common case for dense leaf packing
  is: strategy 1 fails, strategy 2 fails, leaf placed in original intersecting
  position.  No leaf is ever skipped due to an irresolvable conflict, so
  intersections accumulate.
- Neither strategy repositions the leaf azimuthally (different phi on the
  cluster surface) or in arc position (different row).  Both strategies only
  move the leaf locally, in the normal direction.  Two leaves at the same phi
  and the same arc position will always conflict regardless of lift or raise.

### 2. WAY too slow

`multi-parent-mesh-leaves.stl` generation is unacceptably slow.  The expected
cost per leaf should be low:

- Oval / root geometry is **pre-calculated** during slot collection.
- R-tree pre-filters candidates to a small set.
- The number of deconfliction attempts is bounded (≤ ~5 lift + 3 raise).

But the actual bottleneck is almost certainly `trimesh.proximity.closest_point`
called **per candidate per attempt**.  For a cluster with N leaves placed so
far and K nearby candidates per new leaf, and up to 8 deconfliction attempts,
each attempt calls `closest_point` twice.  `closest_point` builds a BVH per
call unless the mesh has a cached BVH.  Trimesh does cache the BVH on the mesh
object, but `solidify_leaf` creates a **new Trimesh object** for every
deconfliction attempt — so every `_dc_solid` starts with no BVH.

More importantly: the `placed_meshes` list is checked per-candidate, and each
previous leaf mesh is a separate Trimesh.  As the placed count grows, even with
the R-tree limiting candidates to ~5, the per-leaf cost includes:
- 2 × `closest_point` calls × up to 5 lift attempts × 3 raise attempts = 16+
  BVH queries per conflicting leaf.

And `place_leaves_on_multiple_meshes` calls `_build_meridians` (which slices
the mesh at 64 Z levels via `mesh.section`) once per cluster, which is
historically the main cost.  If multiple clusters exist this dominates.

### 3. The fundamental design mismatch

The deconfliction is **reactive**: build a leaf, detect conflict, attempt fix.
A proactive approach (don't place a leaf where it would conflict with an already
placed neighbour) would need to happen at slot-collection time, before geometry
is built.  The current slot → build → check → retry pipeline pays full geometry
cost on every failed attempt.

---

## Next Steps

### A. Profile to find actual bottleneck

Before designing a new algorithm, instrument the hot path:

```python
# Quick timing around the three expensive sections:
# 1. _build_meridians (mesh.section × 64)
# 2. solidify_leaf / build_leaf_surface per attempt
# 3. trimesh.proximity.closest_point calls
```

Use `time.perf_counter()` counters accumulated into a side dict, printed at the
end of `place_leaves_on_multiple_meshes`.

### B. Replace per-leaf Trimesh objects with a point-cloud clearance check

The `placed_meshes` list is expensive to query because each entry is a full
Trimesh.  Instead, maintain a **point cloud of placed leaf vertices** (or a
downsampled version) in a single 3-D numpy array, and use a KD-tree for nearest-
neighbour lookup.  This replaces 2× `closest_point` (BVH) with 1× KD-tree
query — much faster for small leaf meshes.

```python
# At placement start:
from scipy.spatial import cKDTree

# Replace placed_meshes with:
_placed_verts: list[np.ndarray] = []   # list of (N_i, 3) vertex arrays
_placed_kdtree: cKDTree | None = None  # rebuilt after each placement

# In _leaf_intersects_placed:
# Query the single combined KD-tree for the min distance from new leaf verts.
```

KD-tree rebuild cost grows as O(N log N) where N is total placed vertex count,
but querying is O(log N) per new vertex — dramatically better than per-mesh BVH.

### C. Rethink deconfliction as slot-level exclusion

Rather than fixing a leaf after the fact, track occupied angular arcs per row
and skip slots that fall within an occupied zone.  After placing a leaf at angle
`phi` with width `W`, mark `[phi - W/2R, phi + W/2R]` as occupied on that row
ring.  New slots that fall inside an occupied arc are skipped without ever
building the geometry.

This is O(1) per slot (angular interval query), has zero geometry cost for
skipped slots, and prevents conflicts rather than resolving them.

The gap between strategy 1 (lift) and strategy C (skip) is that lift is still
useful for the thin-overlap cases where two adjacent rows slightly overlap
vertically — a situation where skipping the slot would create a visible bald
patch but a small tip-lift resolves it cleanly.

### D. Fix the lift budget

`2 × thickness_mm` (0.48 mm) is too small to be useful for leaf-on-leaf
clearance.  A more useful budget would be `_DECONF_CLEARANCE_MM` itself (0.4 mm)
as the guaranteed tip separation, letting the contact-angle geometry determine
how much actual lift achieves that.  Alternatively, binary-search the lift value
that achieves exactly `_DECONF_CLEARANCE_MM` clearance rather than stepping
through fixed increments.

### E. Investigate why `_build_meridians` is slow

`mesh.section` internally computes mesh-plane intersections.  For 64 Z levels ×
N clusters this is significant.  Options:
- Reduce `z_samples` (currently 64) to 32 or 24 — visual difference is negligible
  for smooth cluster shapes.
- Cache meridians per `(edge_id, r_foliage, clump_length)` key since identical
  cluster parameters produce identical meridians (the cluster geometry is
  deterministic given the seed).
- Pre-compute meridians in parallel using a thread pool (GIL-free for NumPy ops).

---

---

## Corrections Made After Initial Design

### `_leaf_z_thickness` deleted — use `thickness_mm` directly

Initial draft computed a helper `_leaf_z_thickness(leaf_kw)` that called
`compute_leaf_geometry` at canonical pose to measure the Z span.  Corrected:
the leaf surface thickness is simply `leaf_kw["thickness_mm"]` — the dome
height parameter that already exists.  The helper was unnecessary.

`deconf_lift_max = 2.0 * float(leaf_kw["thickness_mm"])`.  For default
`thickness_mm = 0.24 mm` this gives a 0.48 mm budget.  The while-loop step
was changed from 0.5 mm to 0.1 mm, and the ceiling is now additive
(`_deconf_lift_base + deconf_lift_max`) so the budget is relative to whatever
`lift_mm` the leaf already has, not an absolute value.

### `_leaf_intersects_placed` — containment → proximity

Initial version used `mesh.contains(vertices)` (ray-cast, O(N·F)).  Replaced
with `trimesh.proximity.closest_point` measuring unsigned surface-to-surface
distance against the `_DECONF_CLEARANCE_MM` threshold.  The R-tree bbox query
is also expanded by the clearance so near-miss leaves are fetched as candidates
before the per-candidate proximity check.

---

## Profiling Instrumentation Added (not yet run)

Added to `placement.py`:
- `import time`
- Module-level `_PROF: dict[str, float]` and `_PROF_N: dict[str, int]`
  accumulators with helper `_pt(key, dt, n=1)`.
- Timers around: `_build_meridians` (phase 1), `_collect_row_slots` (phase 2),
  `_place_leaf_slot` total (phase 2), initial `build_leaf_surface+solidify_leaf`
  inside the slot, deconfliction block total, R-tree query inside
  `_leaf_intersects_placed`, and each `closest_point` call.
- Summary printed at end of `place_leaves_on_multiple_meshes` showing seconds,
  % of total, call count, and average ms per call.

### cProfile Results — 1x1-grass-tree tile (run completed)

**Total wall time: 1127.7 seconds (~18.8 minutes) for one tile.**

Top consumers by `tottime` (time in function body only):

| tottime | cumtime | ncalls | function |
|---------|---------|--------|----------|
| 161.2s | 191.2s | 13,297,225 | `rtree index.py:intersection` |
| 145.3s | 164.3s | 1,098,690,100 | `rtree index.py:_get_ids` (internal) |
| 137.6s | 164.7s | 72,961 | `trimesh triangles.py:closest_point` |
| 98.9s | 435.0s | 72,961 | `trimesh proximity.py:nearby_faces` |
| 64.0s | 730.8s | 72,961 | `trimesh proximity.py:closest_point` ← our call |
| 40.8s | 163.6s | 5,757,585 | `trimesh grouping.py:group_rows` |
| 21.9s | 268.1s | 12,133 | `trimesh repair.py:fix_winding` ← fix_normals |
| 8.2s | 610.2s | 11,500 | `placement.py:_leaf_intersects_placed` ← our fn |

### What the Numbers Mean

**`_leaf_intersects_placed` cumtime: 610s = 54% of total.**
Called 11,500 times, averaging 53ms each.

**`trimesh.proximity.closest_point` called 72,961 times.**
72,961 / 11,500 ≈ 6.3 closest_point calls per `_leaf_intersects_placed` call
= ~3 R-tree candidate leaves per check × 2 directions.  Each call costs
~730 / 72,961 ≈ **10ms cumulative** (BVH build + triangle intersection).

**R-tree `intersection` called 13,297,225 times (161s tottime).**
We make at most one R-tree bbox query per `_leaf_intersects_placed` call
(11,500 calls × 1 = 11,500 direct queries).  The remaining 13M are internal
R-tree node visits — as the placed-leaf index grows to hundreds of entries,
each spatial query traverses more internal nodes.  The R-tree is becoming
the dominant overhead as leaves accumulate.

**`repair.py:fix_winding` called 12,133 times (268s cumtime).**
`solidify_leaf` calls `solid.fix_normals()` unconditionally.  Every
deconfliction attempt (`_dc_solid`) also calls it.  12,133 calls ≈ number
of initial leaf builds + deconfliction rebuilds.  `fix_normals` triggers
`group_rows` (40s) and `hashable_rows` (30s) — pure overhead for computing
face-winding on a mesh whose winding is already pre-determined.

**`ray_triangle` called only 3,706 times** — the oval `mesh.contains` check
and curl-region `mesh.contains` are NOT the bottleneck.

### Root Causes Confirmed

1. **`closest_point` is called on every placed leaf against every nearby
   candidate, for every deconfliction attempt.** 72,961 calls × 10ms = 730s.
   The BVH is rebuilt on every call because every `_dc_solid` is a fresh
   `trimesh.Trimesh` with no cached BVH.

2. **The R-tree grows unboundedly** and each query gets more expensive as the
   placed count rises.  13M internal node visits for 11,500 outer queries =
   1,156 internal visits per query on average — the tree is deep/wide.

3. **`fix_normals` on every solidification** is pure waste.  The face winding
   in `solidify_leaf` is deterministic and pre-correct; `fix_normals` does
   expensive graph-based winding propagation for no benefit.

### Inline timing instrumentation

Also added to `placement.py` (not yet run separately — inline timing will give
per-section ms breakdown on next run):

```bash
python -m dharmatiles.terrains.tile \
    --tile "src/tiles/ground/1x1-grass-tree.tile.py" --quiet
```

Prints a table of seconds / % / ncalls / avg-ms for: `phase1.meridians`,
`phase2.collect_slots`, `phase2.place_slot`, `slot.initial_build`,
`slot.deconf_total`, `intersect.rtree`, `intersect.closest_point`,
`deconf.triggered`, `deconf.s2_attempted`, `deconf.unresolved`.

---

## State of the Code

All deconfliction changes are in `src/dharmatiles/trees/placement.py`:
- `_DECONF_CLEARANCE_MM` constant (line ~53)
- `_leaf_intersects_placed()` helper (after `_collect_row_slots`)
- Deconfliction block in `_place_leaf_slot` (after initial solid build)
- `_deconf_lift_max` computed once in `place_leaves_on_multiple_meshes`

The existing `placed_index` R-tree and `placed_meshes` list (on `_MeshCtx`)
are populated correctly and survive for the lifetime of a placement run.
The infrastructure is sound; the specific intersection-test and retry strategy
is what needs replacement.

---

## Inline Timing: `test-multi-parent-mesh-leaves.py` (run 2026-07-01)

The inline `_PROF` instrumentation added to `placement.py` was exercised on the
dedicated two-cluster test.  Setup: cluster A (vertical, r_foliage=5.5mm, tip at
z=22), cluster B (55° curved spine, tip at z=23, 6mm away).  Parameters: 0
h_overlap, 0 v_overlap (densest possible packing); `z_samples=64`; `n_meridians=6`.

### Output summary

```
── leaf placement timing ──────────────────────────────────────────
  phase2.place_slot         24.191s   99.4%  n=   65  avg= 372.17ms
  slot.deconf_total         18.928s   77.7%  n=   48  avg= 394.33ms
  intersect.closest_point   14.884s   61.1%  n= 1106  avg=  13.46ms
  slot.initial_build         1.782s    7.3%  n=   48  avg=  37.13ms
  phase1.meridians           0.131s    0.5%  n=    2  avg=  65.49ms
  phase2.collect_slots       0.024s    0.1%  n=    9  avg=   2.72ms
  intersect.rtree            0.008s    0.0%  n=  348  avg=   0.02ms
  deconf.triggered           0.000s    0.0%  n=   43  avg=   0.00ms
  deconf.s2_attempted        0.000s    0.0%  n=   43  avg=   0.00ms
  deconf.unresolved          0.000s    0.0%  n=   42  avg=   0.00ms
  TOTAL                     24.348s
```

Placement result: 48 total leaves (25 on A, 23 on B).

### What the numbers reveal

**`slot.deconf_total` = 18.9s (77.7% of total) for only 48 leaves.**
Every deconfliction attempt costs an average of 394ms.  At this rate, a single
foliage cluster at tree scale (~200 leaves) would spend 79 seconds in
deconfliction alone.

**`intersect.closest_point` called 1,106 times = 23.0 calls per placed leaf.**
The deconf block triggers 43 times (89.6% of all 48 placed leaves).  Each
triggered leaf tries strategy 1 (up to 5 lift steps, each calling
`_leaf_intersects_placed` once, which calls `closest_point` twice per R-tree
candidate) then strategy 2 (3 raise steps, same pattern).  With ~3.5 R-tree
candidates per query: `1106 / 43 ≈ 25.7` closest_point calls per triggered leaf
— consistent with 5 lift attempts × 2 directions × ~2.5 candidates + 3 raise
attempts × 2 directions × ~2.5 candidates ≈ 40, adjusted down for early-exit
(returning True as soon as one candidate confirms).

**`deconf.unresolved` = 42 out of 43 triggered = 97.7% failure rate.**
Strategy 1 and strategy 2 together resolve **one** leaf out of 43 conflicts.
The deconfliction code is doing enormous work to achieve essentially nothing.

**`deconf.triggered` = 43 out of 48 attempts = 89.6% of all leaves conflict
on first placement.**
With `h_overlap=0` and `v_overlap=0` (the hardest test case) and a 0.4mm
clearance requirement, virtually every leaf placed after the first few conflicts
with an already-placed neighbour.  This is the expected regime for a densely
packed cluster: leaf bodies on adjacent rows overlap in 3D, and the
no-overlap parameter only governs column spacing (perimeter step), not 3D solid
clearance.

**Phase 1 meridians = 65ms per cluster (0.5% of total).**
`_build_meridians` is NOT the bottleneck for this test.  The earlier cProfile
run on the full 1x1 tile (18.8 minutes total) saw meridian cost subsumed by the
massive deconfliction overhead.  The worry about `mesh.section` cost at 64 z-levels
is moot until deconfliction is fixed.

**R-tree cost = negligible (0.02ms per query).**
Despite the cProfile run showing 161s in `rtree.intersection`, the per-call cost
is tiny.  The 13M "internal node visits" in cProfile were index traversal
counted in the C extension, not 13M Python-level calls.  With only 48 placed
leaves the R-tree is shallow and fast; at 500+ leaves per cluster the tree
deepens and per-query cost grows, but it is not the primary driver today.

### Key insight: 0-overlap test exposes the worst case

In production, `h_overlap ≈ 0.2` and `v_overlap ≈ 0.5` reduce the leaf count
per cluster by ≈40% and spread leaves further apart, making the deconfliction
trigger less often.  The no-overlap test (`h_overlap=0, v_overlap=0`) produces
the maximum number of leaves at the densest possible packing — exactly where
deconfliction is triggered the most and resolves the least.  The 89.6% trigger
rate is the ceiling; production rates are probably 30–60%.  But even at 30%,
the 394ms avg per triggered leaf is unacceptable.

### Why `slot.initial_build` (37ms) is a secondary problem

`build_leaf_surface` + `solidify_leaf` for the initial placement costs 37ms
per leaf.  `solidify_leaf` calls `fix_normals()` which triggers
`group_rows`+`hashable_rows` on every call — 3 calls per leaf (initial + any
deconfliction rebuilds).  For 48 leaves × 3 rebuilds × 37ms ≈ 5.3s — already
significant, but dwarfed by the deconfliction overhead.  At scale (200 leaves)
this becomes 22s of `fix_normals` alone.  Removing `fix_normals` from
`solidify_leaf` (the face winding is constructed correctly by design and needs
no correction) would cut `slot.initial_build` to an estimated 5–10ms.

---

## What to Fix First (priority order after this profiling run)

### 1. Kill the deconfliction code entirely — restore baseline

The current deconfliction resolves 1 in 43 conflicts (2.3%) while burning 78%
of total runtime.  It is worse than useless: it guarantees near-full geometry
cost for every conflict without actually resolving it (42/43 land at
`deconf.unresolved` and the leaf is placed in its original intersecting
position anyway).

**Immediate action**: rip out `_leaf_intersects_placed`, the deconfliction
block in `_place_leaf_slot`, and the R-tree insert/update in the success path.
Remove `placed_index` and `placed_meshes` from `_MeshCtx`.  Restore the
pre-deconfliction latency of ~1.4s for this test.

This is not giving up on deconfliction — it is clearing the wreckage before
building the right approach.

### 2. Remove `fix_normals` from `solidify_leaf` hot path

`solidify_leaf` constructs its triangles with correct winding by construction.
`fix_normals()` (which calls `group_rows`, `hashable_rows`, `repair.fix_winding`)
is a repair pass for malformed meshes — not needed here.  Removing it saves
~30ms per `solidify_leaf` call, reducing `slot.initial_build` from 37ms to ~7ms.

This is a fast, safe win independent of any deconfliction redesign.

### 3. Implement slot-level arc exclusion (Option C from Next Steps)

Rather than building a leaf and checking it against neighbours, track occupied
angular arcs per row after each placement.  A new leaf slot is skipped before
any geometry is built if its azimuthal angle falls inside an already-occupied
arc.

Cost model: O(1) per slot, zero geometry cost for skipped slots, resolves
conflicts before they become geometry.  The arc is computed analytically from
`n_col` (the leaf count per row perimeter) and `leaf_width_mm`.

This replaces both the R-tree and the `closest_point` loop with a single angle
comparison.

### 4. Address `slot.initial_build` at 37ms if still needed after #1–#3

After removing deconfliction and `fix_normals`, the estimated per-leaf cost
drops from 372ms to ~12ms.  For a 200-leaf cluster that is 2.4s — acceptable.
Further optimisation (BVH caching, fewer contact-angle iterations) can be done
if profiling still shows a bottleneck.

---

## Revised Understanding of Performance Budget

| Operation | Current | After #1 | After #2 | Target |
|---|---|---|---|---|
| phase1.meridians | 65ms/cluster | 65ms | 65ms | 65ms |
| slot.initial_build | 37ms/leaf | 37ms | ~7ms | ~7ms |
| slot.deconf_total | 394ms/leaf | 0ms | 0ms | 0ms |
| phase2.place_slot total | 372ms/leaf | ~40ms | ~10ms | ~10ms |
| TOTAL for 48 leaves | 24.3s | ~2s | ~0.5s | ~0.5s |

At ~7ms per leaf, 200 leaves per cluster × 10 clusters per tile = 14s total
placement time — acceptable for an offline generation pipeline.

---

## Fix Applied: Deconfliction Removed + `fix_normals` Removed (2026-07-01)

### Changes made

**`src/dharmatiles/trees/placement.py`**
- Removed `import rtree.index as _rtree_idx`
- Removed `_DECONF_CLEARANCE_MM` constant
- Removed `_leaf_intersects_placed()` function
- Removed `scene_proximity`, `placed_index`, `placed_meshes` fields from `_MeshCtx`
- Removed `deconf_lift_max` parameter from `_place_leaf_slot`
- Removed entire deconfliction block from `_place_leaf_slot` (~57 lines)
- Removed R-tree insert in the success path
- Removed `_scene_proximity`, `_rtree_prop`, `_placed_index`, `_placed_meshes` setup from `place_leaves_on_multiple_meshes`
- Removed `_deconf_lift_max` computation

**`src/dharmatiles/trees/leaf.py`**
- Removed `solid.fix_normals()` from `solidify_leaf` — face winding is constructed correctly by the triangle-building code; no repair pass is needed

### Profiling results after fix

```
── leaf placement timing ──────────────────────────────────────────
  phase2.place_slot         4.820s   96.8%  n=   65  avg=  74.16ms
  slot.initial_build        1.184s   23.8%  n=   48  avg=  24.66ms
  phase1.meridians          0.132s    2.7%  n=    2  avg=  65.99ms
  phase2.collect_slots      0.026s    0.5%  n=    9  avg=   2.88ms
  TOTAL                     4.979s
```

**4.9× speedup**: 24.3s → 4.98s for the same 48 leaves on 2 clusters.

### What moved where

| Metric | Before | After | Δ |
|---|---|---|---|
| TOTAL (48 leaves) | 24.3s | 4.98s | −19.3s (−79%) |
| `slot.deconf_total` | 18.9s (78%) | 0s | eliminated |
| `slot.initial_build` per leaf | 37ms | 25ms | −12ms (−32%) |
| `phase2.place_slot` per leaf | 372ms | 74ms | −298ms (−80%) |
| Leaf count | 48 | 48 | unchanged |
| Geometry (v/f) | 13,465 / 26,544 | 13,465 / 26,544 | identical |

The 12ms saving in `slot.initial_build` is from removing `fix_normals` (which previously called `group_rows` + `hashable_rows` + `repair.fix_winding` on every solidification).

### Remaining bottleneck

`slot.initial_build` at 24.7ms/leaf is now the dominant cost.  It covers
`build_leaf_surface` + `solidify_leaf` (surface mesh construction, contact-angle
candidates, oval geometry).  At this rate:

- 200 leaves/cluster × 4 clusters/tile × 24.7ms = 19.8s initial build
- Plus `_contact_angle_for_mesh` bisection (8 iterations × proximity queries per leaf)
  — this is inside `_place_leaf_slot` but NOT in `slot.initial_build` (which only
  covers the post-contact-angle surface+solid build)

The full `phase2.place_slot` avg is 74ms/leaf, so the contact-angle + frame
calculation costs the remaining 74 − 25 = 49ms/leaf.  That is the next target
if further speed is needed; for now 74ms/leaf × ~800 leaves/tile ≈ 60s total
placement is workable.

---

## Brown-Cluster Bald Spot + Uneven Row Gaps (2026-07-01, later session)

### Symptoms reported

On the multi-parent-mesh test's **brown cluster (B — 55° tilt, tip at (6,2))**:

1. **Missing leaves in the 2nd-from-top row**, a "significant gap near the end"
   (the windward, +x tip direction).
2. **Inconsistent row gaps**: the apex→2nd-row gap was a visible gap while the
   2nd→3rd-row gap overlapped.  With 0 % overlap the user expected rows to touch
   or the slack to distribute evenly.

### Instrumentation added

- `_DEBUG_CAPTURE` / `_DEBUG_RECORDS` opt-in hook in `placement.py`: appends a
  per-candidate record (outcome, base position, growth tangent, contact angle,
  and for build errors the failure reason + oval protrusion depth) at each
  outcome site in `_place_leaf_slot`.  No effect on geometry when disabled.
- `src/scripts/_instrument_multiparent_gaps.py`: reproduces cluster B alone,
  reports per-row surface-arc positions + gaps, and dumps the per-candidate
  outcome table.

### Root cause — both symptoms are one bug in `_compute_row_z_positions`

The meridian-arc row placer picks row positions at **uniform surface-arc steps**,
then converts arc→z with `_avg_z_for_arc` while the anchors `s_bot`/`s_top` are
found with `_avg_arc_for_z`.  These two averages are **not mutual inverses**:
each averages over whichever meridians cover the query level, and near the apex
of a *tilted* cluster fewer meridians reach each level.  So the arc→z→arc
round-trip is lossy, worst just below the apex.

Measured (cluster B, before fix), re-derived arc of the 4 realised rows:

| row | z | arc s | Δarc(prev) |
|---|---|---|---|
| 0 | 21.49 | 10.71 | — |
| 1 | 25.04 | 14.92 | 4.21 |
| 2 | 27.97 | 18.22 | **3.30** |
| 3 (apex, pinned) | 30.59 | 23.79 | **5.57** |

Row 2 landed ~0.95 arc-units too low → gap to the pinned apex ballooned to 5.57
(overlap below shrank to 3.30).  The old `z_top_sample` **pin** on the last row
was a prior workaround for this same bias — it fixed the apex *z* but not the
*spacing* to the row below, which is exactly the gap the user saw.

The row-2 **bald spot was a downstream symptom**: at the mis-placed z=27.97 the
windward cross-section curved away fast enough that the straight embedded leaf
oval poked outside the mesh (`mesh.contains` fail → `build_error`), rejecting the
two windward leaves (ci=0 protrude 0.751 mm, ci=10 protrude 0.014 mm).

### Fix — consistent arc(z) inversion

Replace the `_avg_z_for_arc` + apex-pin with a single monotone averaged arc(z)
profile that is **inverted** for the arc→z step:

```python
z_prof = np.linspace(z_bot_anchor, z_top_sample, 400)
s_prof = np.maximum.accumulate([_avg_arc_for_z(z, meridians) for z in z_prof])
row_zs = [np.interp(s, s_prof, z_prof) for s in row_arc]
row_zs[-1] = z_top_sample   # exact apex pin (interp already ≈ this)
```

Now the arc↔z round-trip is exact by construction; `s_top` maps back to
`z_top_sample`, so the apex stays pinned without a special case.

### Result (cluster B, after fix)

| row | z | arc s | Δarc(prev) |
|---|---|---|---|
| 0 | 20.98 | 9.94 | — |
| 1 | 24.69 | 14.56 | **4.61** |
| 2 | 28.79 | 19.17 | **4.61** |
| 3 (apex) | 30.59 | 23.79 | **4.61** |

Perfectly even arc spacing.  Row 2 rose to z=28.79 where the cross-section fits
the oval — **row 2 now places 10/10 leaves, zero build errors**.  Both reported
symptoms resolved by the single row-spacing fix; the bald spot did not need a
separate oval-tolerance change.

Residual sub-print oval rejections remained in rows 1 and 3 (protrusions
0.06–0.54 mm) — not user-reported, but they stacked into a leeward brown stripe
visible in the side render.  Addressed by the second fix below.

### Second fix — oval-protrusion tolerance (leeward/windward stripes)

The embedded leaf **oval** (root, pushed `embed_mm = 0.75` into the surface) was
gated by a strict `mesh.contains(inner_v).all()` — any single protruding vertex
→ `build_error`.  On a convex/tilted cluster the straight oval overshoots where
the surface curves away, poking a few tip-end vertices out by a fraction of a
millimetre (e.g. 0.063 mm — a 63-micron poke rejecting a whole leaf).

Replaced the binary test with a tolerance keyed to the embed depth: reject only
when the deepest protruding vertex clears `_OVAL_PROTRUSION_TOL_MM = 0.75` mm
(= the embed depth).  Below that, the root's outer skin is still at/inside the
surface, so the leaf plugs in and the blade hides it.  Genuinely-outside cases
(bottom-row downward overhangs at 1.4–2.2 mm) are still rejected.

### Combined result (cluster B, both fixes)

| row | before | after Issue-2 fix | after both fixes |
|---|---|---|---|
| 0 (bottom, downward — intended bald) | 0/3 | 0/3 | 0/3 |
| 1 | 9/11 | 6/10 | **10/10** |
| 2 (2nd-from-top — reported) | 9/11 | **10/10** | **10/10** |
| 3 (apex) | 5/9 | 5/7 | **7/7** |

Cluster B: 21 → 27 leaves; fully covered except the intended bald bottom row.
Cluster A (vertical, symmetric) unaffected by the arc fix and improved from
25 → 33 leaves via the oval tolerance.  Top/perspective/side renders confirm the
apex rosette now flows into the ring below with no gap, and the 2nd-from-top row
is solid.

The FLOATING-LEAVES artifact count rose (recovered leaves on overhang faces have
curl tips that lift up to ~1.85 mm) — a pre-existing cosmetic check, not a bald
spot; trading a bald spot for a slightly-lifted tip is a net coverage win.

### Files touched

- `mesh.py::_compute_row_z_positions` — consistent arc(z) inversion (Issue 2).
- `placement.py` — `_OVAL_PROTRUSION_TOL_MM` constant + tolerant oval check
  (Issue 1 residual).

Investigation-only scaffolding (a `_DEBUG_CAPTURE` per-candidate hook in
`placement.py` and `src/scripts/_instrument_multiparent_gaps.py`) was added to
diagnose the failures and **removed after the fixes landed**; the durable
per-row placed/attempted counts in the test's artifact report cover regressions.

---

## Current Priority: Re-add Deconfliction via Slot-Level Arc Exclusion (2026-07-01)

The row-spacing and oval-tolerance fixes are landed.  The next item is bringing
leaf-surface deconfliction back — but via the **right approach** (Option C from
the Next Steps section above), not the reactive `closest_point` approach that
was removed.

### Rationale

The removed approach resolved 1/43 conflicts (2.3%) while burning 78% of total
runtime.  The right approach is proactive, not reactive:

- **O(1) per slot** — angular interval lookup, no BVH, no proximity queries.
- **Zero geometry cost for skipped slots** — no `build_leaf_surface` call at all.
- **Prevents conflicts** before geometry is built, instead of reacting after.

### Implementation file

`src/dharmatiles/trees/placement.py`

### Step-by-step spec

**Step 1 — `_OccupiedArcs` structure in `_MeshCtx`**

Add a `dict[int, list[tuple[float, float]]]` field (row index → list of
`(phi_lo, phi_hi)` occupied angular intervals) to `_MeshCtx`.  Initialise to
`defaultdict(list)`.  Intervals are in radians; the list for each row is kept
sorted and merged so lookups stay O(n_intervals) with a small constant.

**Step 2 — Arc check in `_place_leaf_slot` before geometry**

Before calling `build_leaf_surface`, compute the slot's azimuth `phi` (it is
already available as the column angle on the row ring).  Query the row's arc
list: a slot is occupied if any interval `(lo, hi)` satisfies
`lo ≤ phi ≤ hi` — with wraparound handling for intervals that straddle 2π
(split them into `[lo, 2π)` and `[0, hi − 2π]` on insert; query both).  If
occupied, skip — return without building any geometry, increment a
`stats.arc_skipped` counter.

**Step 3 — Arc insert on successful placement**

After a leaf is successfully placed, compute `half_w = (leaf_width_mm / 2) /
row_ring_radius`, where `row_ring_radius` is the XY radius of the cluster cross-
section at the leaf's row z (already computed during slot collection — reuse it,
don't recompute).  Insert `(phi − half_w, phi + half_w)` into the row's arc
list.  After inserting, merge any overlapping intervals to keep the list short.

**Step 4 — Retain lift for cross-row vertical overlap only**

The existing lift strategy (increase `lift_mm` in small steps) is useful when
two leaves on adjacent rows are at non-conflicting azimuths but still slightly
interpenetrate in 3D due to vertical overlap.  Keep it, but gate it on a cheap
height-range overlap test: compute the z-extent of the candidate leaf (from
`pt3d` + blade height) and check whether any already-placed leaf on the
neighbouring rows has an overlapping z-range AND an azimuth within `2 × half_w`
of the candidate's.  Only enter the lift loop if both conditions hold.  This
avoids triggering lift on the common (non-overlapping) case.

**Step 5 — Hard constraints**

- Do NOT re-introduce `trimesh.proximity.closest_point`, R-tree intersection,
  or `placed_meshes`.  They were removed for cause and are not coming back.
- Do NOT add `fix_normals()` calls — removed from `solidify_leaf` for
  performance reasons.
- After this change, a 48-leaf two-cluster run must complete in under 10s
  (baseline without deconfliction: ~5s).  The arc check budget per slot is
  effectively zero; total overhead should be < 1s on top of baseline.
- The wraparound case must be handled: a leaf at phi ≈ 0 and a leaf at
  phi ≈ 2π on the same row must be detected as conflicting.

---

## SUPERSEDED — Reframed from Exclusion to Imbrication (2026-07-01, design review)

The "slot-level arc exclusion" plan above is **abandoned**.  A design review
(walking the actual spacing/geometry code) established that it solves the wrong
problem on the wrong axis, and — more importantly — that the *goal itself* was
mis-stated.  What follows supersedes everything from "Current Priority" onward.

### Why arc-exclusion was aimed at the wrong axis

Within-row and cross-row spacing are **already analytic and deterministic**:

- `placement.py:919` — `col_step = max(W*(1-h_overlap), 1e-3)` → within-row
  column spacing is exactly one leaf-width (edges touch at `h_overlap=0`).
- `mesh.py:1028` — `row_step_target = L*(1-v_overlap)` → cross-row surface-arc
  spacing is likewise fixed.

So a **per-row** occupied-arc list (the plan's core structure) merely re-derives
what `col_step`/`n_col` already guarantees, and — because two leaves on
*different rows* land in *different* arc lists — it is structurally blind to the
**cross-row** overlap that the profiling section itself named as the dominant
conflict ("leaf bodies on adjacent rows overlap in 3D").  The plan's Step 4 also
quietly re-admitted the very `lift` strategy that resolved 1/43 conflicts.

The 89.6% "conflict" rate was additionally a red herring: the removed test used
a 0.4 mm clearance requirement *stricter than the touch-packing the spacing
produces*, so it flagged the intended packing as failure.

### The goal, restated (this is the real pivot)

Deconfliction is **not** about preventing overlap, and **not** a surface-gap
requirement.  The user wants *more* overlap, not less.  The actual goal is
**imbrication (shingling)**: let leaf footprints overlap freely, but stagger
overlapping leaves along the surface normal so they **stack into layers** like
real overlapping leaves, instead of sitting coplanar and phasing through one
another.  Criterion = "no visible skewering," not "≥ X mm of air."

This kills the arc-exclusion plan outright (its whole job is to *reject*
overlap) and retires the 0.4 mm clearance target permanently.

### What survives, repurposed: the `(phi, s)` occupancy map → a *layer* index

The one good idea from the old Next Steps — an unwrapped-surface occupancy map
in `(phi, s)` coordinates (`s` = surface arc-length, already computed via
`_avg_arc_for_z`; `phi` already in hand per slot) — survives, but its output
changes from a boolean *skip* to a **shingle-layer index**:

1. Maintain a coarse occupancy grid in `(phi, s)`, shared across **all** rows
   (not per-row).  Each cell stores a small bitmask of occupied normal-offset
   **layers**.
2. Per new leaf, before building: OR the bitmasks of the cells its `W×L`
   footprint covers; pick the **lowest free layer** (greedy graph colouring).
   Lowest-free (not max+1) bounds the layer count to the *local* overlap
   multiplicity (~2–3), so dense regions don't run away.
3. `layer → δ` standoff, `Δ ≈ thickness_mm` (~0.24–0.3 mm).
4. Write that layer's bit into every covered cell.

Cost: O(cells covered) per leaf.  No BVH, no `closest_point`, no R-tree, no
reactive retry.  Fully proactive and deterministic.  Cross-row skewering is
handled because a blade's footprint spans multiple rows' worth of `s`.

### The actuator — corrected twice during review

**First (wrong) idea:** use `lift_mm`.  Rejected: `lift_mm` is a *rotation*
about the base (`leaf.py:414`, `lift_angle = arctan2(lift_mm, length_mm)`), tip
tilts up, **base stays put** — it changes pose, not depth, and leaves the base
region coplanar.

**Second (wrong) idea:** rigidly translate the whole leaf (`pt3d += δ·up_hint`,
oval and blade together) — the old removed "Strategy 2 / raise".  Rejected: it
lifts the root oval out of the parent mesh, detaching the leaf (the old code
tellingly "skipped the containment check"), forcing an awkward inward-only
budget bounded by `embed_mm`.

**Correct idea (user's):** we are *constructing* the leaf — decouple blade depth
from the root.  Keep the **root oval plugged at the surface** (embed as today),
and build the **blade surface offset outward by δ** along the normal.
`solidify_leaf` supports this for free: it skins blade-rim → oval-rim with **1:1
index correspondence** (`leaf.py:897–903`) and assumes only matching vertex
count/topology (`:864`), *not* coincident rims.  A δ offset simply makes the
connecting neck taller; the solid stays watertight.

Concretely, at the build site (`placement.py:765`/`:787`):

```python
surf, geom = build_leaf_surface(base_pos = pt3d + delta * up_hint, ...)  # blade offset out
inner_v    = _oval_off + pt3d[np.newaxis]                                # oval stays plugged
```

The pose frame (`tangent`, `up_placed`) is untouched → elevation and growth
direction are identical; **only the standoff changes.**

### Consequences of the correct actuator

- **No embed budget, no float-check risk.**  The root never moves.  Layers go
  purely **outward**, which is also the natural shingle direction (later/outer
  leaves stand further off and overlie earlier ones).
- **`col_step`/`row_step` become pure overlap-density knobs** — dial them *down*
  to overlap more; they no longer carry any deconfliction duty.
- **The one thing to watch is now the neck**, not the root: large δ makes a tall
  base riser ("leaf on a stalk") and its wall skims the mesh on concave regions.
  Keep `Δ ≈ thickness_mm` and let the bounded layer count cap total standoff at
  ~1 mm — enough to separate surfaces visually while still reading as leaves
  lying against each other.

### Retained hard constraints (still in force)

- Do NOT re-introduce `trimesh.proximity.closest_point`, R-tree intersection, or
  `placed_meshes`.
- Do NOT add `fix_normals()` calls.
- Proactive only — decide the layer *before* building geometry; never build,
  test, retry.

### Next action

Prototype against the two-cluster test: `(phi, s)` layer map + greedy lowest-free
layer + blade standoff via offset `base_pos`, with `col_step`/`row_step` dialed
down so leaves genuinely overlap.  Eyeball whether the shingling reads as real
overlapping foliage before generalising to tree scale.

---

## IMPLEMENTED — Imbrication (shingling) via (phi, s) layer map (2026-07-01)

Built exactly the actuator/layer-map design above.  All changes in
`src/dharmatiles/trees/placement.py`; no `closest_point`, R-tree, `placed_meshes`,
or `fix_normals` reintroduced.  Fully proactive (layer chosen before geometry).

### What was added

- Module constants `_SHINGLE_*`: `PHI_CELLS=180` (2° angular grid),
  `S_CELL_FRAC=0.25` (meridional cell = L/4), `MAX_LAYERS=4`, `DELTA_MM=0.30`
  (per-layer outward standoff ≈ leaf thickness), `FOOTPRINT_SCALE=0.80`.
- `_shingle_cells(phi, s, ring_r, W, L)` → grid cells a leaf's W×L footprint
  covers; `_shingle_pick_layer(occ, cells)` → lowest free layer (greedy
  colouring, clamped to `MAX_LAYERS-1`); `_shingle_write(occ, cells, layer)`.
- `_MeshCtx.occ`: `dict[(phi_bin, s_bin) → layer bitmask]`, shared across all
  rows of one mesh, persists for the placement run.
- `_LeafSlot.s_row`: meridional arc-length at the row (from `_avg_arc_for_z`),
  the shared `s` coordinate.
- `LeafPlacementStats.shingle_layers`: per-leaf layer, for inspection.

### Actuator (as designed)

At the build site, blade offset out, root oval left plugged:
```python
base_blade = pt3d + layer * _SHINGLE_DELTA_MM * up_hint   # blade stands off
surf, _ = build_leaf_surface(base_pos=base_blade, tangent=tangent, up_hint=up_placed, ...)
inner_v = _oval_off + pt3d[np.newaxis]                     # oval stays at surface
```
Pose frame (`tangent`, `up_placed`) untouched → elevation/growth identical, only
the standoff changes.  `solidify_leaf` just builds a taller connecting neck; all
61 test leaves remain watertight.  Grid written only on successful placement.

### Two corrections found by eyeballing the first render

The user flagged the **brown cluster apex leaves raised way more than needed** —
tall necks where the dome already crests.  Two root causes, both fixed:

1. **Apex phi blow-up.** `half_phi = (W/2)/ring_r` with `ring_r` = *base* ring
   radius → near the apex `ring_r → 0`, so a single leaf claimed the whole phi
   ring and every apex leaf was forced onto its own layer (0,1,2,3 → 0.9 mm
   stalks).  Fix: measure `phi`/`ring_r` at the leaf **mid-length**
   (`pt3d + 0.5·L·tangent`), where the body actually sits and fans outward to a
   larger radius, and cap `half_phi` at π/2.  Apex leaves that are azimuthally
   separated now stay on layer 0.
2. **Touch-packing over-triggered.** At `h_overlap=v_overlap=0` leaves merely
   *touch* (spaced exactly W/L apart) yet shared a boundary grid cell → bumped to
   higher layers.  Fix: `_SHINGLE_FOOTPRINT_SCALE=0.80` shrinks the footprint to
   the leaf's overlapping *core*, so only genuine overlap (> 20 % of a dimension)
   escalates a layer.

### Result (two-cluster touch-packed test, `h=v=0`)

- Runtime **5.9 s** (well under the 10 s budget; baseline ~5 s).
- Layer histograms: A `{0:9, 1:8, 2:7, 3:10}`, B `{0:13, 1:10, 2:4}` — max
  standoff 0.6–0.9 mm, within the ~1 mm design target.
- Apex rosette on B now lies flat (top render) instead of on stalks; per-cluster
  placed counts and watertightness unchanged.

`col_step`/`row_step` are now pure overlap-density knobs (dial `h_overlap`/
`v_overlap` up to overlap more); shingling engages automatically when leaves
genuinely overlap.  Next: generalise to tree scale and tune `DELTA_MM`/overlap
on a full tile.

---

## Cross-Cluster Intersection — Options Review + Option A Prototype (2026-07-01)

The imbrication work above solves **within-cluster** leaf overlap.  A separate
failure remains: when **two foliage clusters intersect** (the multi-parent-mesh
test's A + B), leaves are placed on each cluster independently, so the shingle
map (`_MeshCtx.occ`) — being **per-mesh** — is blind across clusters.

### The "intersection" is really three distinct failures

1. **Buried-base leaves** — a leaf whose root oval sits on the part of A's
   surface that is *inside* B.  These are the "half cut off" ones: base plugs
   into A, blade phases out through B's skin.  Should not exist at all.
2. **Cross-cluster skewering** — one leaf from A, one from B, both on genuinely
   exposed surface near the seam, phasing through each other coplanar.  Exactly
   the intra-cluster problem the `(phi, s)` shingle map already solves — but the
   map is per-`_MeshCtx`, so A and B can't see each other's layers.
3. **Inward-pointing leaves** — leaves on A's surface that faces *into* B.  Even
   after removing the truly-buried ones (#1), a leaf on the still-exposed inner
   flank of the seam grows toward B's body instead of out of the combined
   silhouette.

Every coordinate the placer uses (`phi` = ray from *this* cluster's 2-D
centroid via `_polygon_point_at_phi`; `s` = arc along *this* cluster's
meridians; `occ`) is defined per single axis per blob, so the pipeline
structurally cannot see any of the three.

### Options considered (with tradeoffs)

- **A — Cull only (cheapest).** Keep independent per-cluster placement; before
  building a slot, test whether its base is inside any *other* cluster's solid;
  skip if so.  Fixes #1 fully and most of #3 as a side effect; **misses #2**
  (survivors still skewer).  No change to the meridian model.
- **B — Cull + shared world-space shingle occupancy.** A, plus replace per-mesh
  `occ` with one occupancy structure in world space (two clusters don't share a
  `(phi,s)` frame).  Fixes #1 + #2 + most of #3.  Loses the cheap 2-D on-surface
  grid; layering across differently-oriented surfaces is approximate.
- **C — Union to one shell, place once (the "single parent mesh" idea).** CSG-
  union (manifold3d already a dep) → one outer surface, one `occ`.  Fixes #1/#2
  for free, #3 on convex parts.  **Catch:** the union is non-star-convex —
  `_polygon_point_at_phi` casts from ONE centroid per z-slice, but the union's
  cross-section is two disjoint loops (below merge) or a concave peanut (at the
  merge).  Requires rewriting meridians for multi-loop / non-convex sections
  AND handling the concave crease (two walls facing each other — #3 relocates
  into the crease rather than vanishing).  Largest effort.
- **D — Union for *classification*, per-cluster for *placement* (hybrid).** Use
  CSG only to answer "is this slot on the true outer shell?" (robust cull),
  keep per-cluster placement on the exposed parts, pair with B's shared
  occupancy for #2.  Same coverage as B with a cleaner cull; no meridian
  rewrite.

### Decision

**Recommended end-state: D, not C.**  C trades a bounded problem (two overlapping
blobs) for an open-ended one (a general non-convex surface placer with concave-
crease handling).  D keeps what the model is good at (outward-shingled leaves on
a convex-ish blob) and borrows CSG only for the exposed-surface predicate.

**Immediate step (this session): prototype Option A** — cull buried bases only —
to *see* how much of the artifact is just #1 before investing in shared
occupancy (B/D).  Open question the render should answer: is a bald, recessed
seam acceptable (real stylized foliage has a shadowed valley where clumps meet)?
If yes, #3's hard residue disappears and C loses its only edge over D.

### Option A prototype — implementation

All in `src/dharmatiles/trees/placement.py`:

- `_CROSS_CLUSTER_BURY_TOL_MM` module constant (mm a base may sit *inside*
  another cluster before it is culled).
- `_MeshCtx.other_prox: list[ProximityQuery]` — the other clusters' proximity
  queries, populated after all contexts are built in
  `place_leaves_on_multiple_meshes`.
- `LeafPlacementStats.skipped_cross_buried` counter.
- In `_place_leaf_slot`, right after the base `pt3d` is computed (before any
  geometry build), query each `other_prox.signed_distance([pt3d])`; if the base
  is inside another cluster by more than the tolerance, increment the counter
  and return `(0, 0)` (culled — not counted as attempted).

`signed_distance` sign convention in trimesh: **inside = positive**.  Reuses the
`ProximityQuery`/BVH already built per mesh; no new `contains` ray engine, no
`closest_point`, no R-tree — consistent with the standing hard constraints.

The multi-parent-mesh test (`src/scripts/test-multi-parent-mesh-leaves.py`)
prints the new `xbury=` count per cluster in its skip line.  Render the three
views and eyeball whether culling buried bases alone reads acceptably.

### Correction — base-only cull was invisible; must sample the whole midrib

First render showed **no visible change** (`xbury=4/5`, 9 leaves).  Reason: a
**base-inside** test only catches leaves whose *root* is buried — and those are
exactly the "leaves rendered inside the structure (fine)" category the user
already accepted (deep inside, invisible).  The **offensive** leaves ("half cut
off", intersecting) have their base on the cluster's *exposed* surface and their
**blade** crossing into the neighbour — the base test misses them entirely.

Fix: moved the cull to *after* the growth `tangent` is known and sample points
along the **midrib** (`pt3d + f·L·tangent` for `f ∈ {0, 0.5, 1.0}`); cull if ANY
sample lands inside a neighbouring solid.  `signed_distance` takes the 3 points
in one call per neighbour; still no BVH rebuild, no `closest_point`.

Result on the two-cluster test: `xbury=12` on A, `xbury=8` on B (20 culled vs 9
before).  The seam tangle — leaves jutting through the other cluster's skin — is
gone, replaced by exposed cluster body (bald recessed seam), runtime unchanged
(~4.9 s).  This is the intended Option A signature; whether the bald seam reads
acceptably (vs. wanting the seam filled + shingled, i.e. Option B/D) is the
open decision.

### State of the code (end of session)

Option A (blade-sample cull) is **landed and working** on the branch:

- `src/dharmatiles/trees/placement.py`
  - `_CROSS_CLUSTER_BURY_TOL_MM = 0.0` constant (tolerance a midrib sample may
    sit inside a neighbour before culling).
  - `_MeshCtx.other_prox: list` — populated after all contexts are built with
    the other clusters' `ProximityQuery` objects (empty for single-cluster runs,
    so those are unaffected).
  - `LeafPlacementStats.skipped_cross_buried` counter.
  - Midrib-sample cull in `_place_leaf_slot`, placed **after** `tangent`/
    `up_placed` are computed and **before** the shingle/geometry build.  The
    earlier base-only block (right after `pt3d`) was removed.
- `src/scripts/test-multi-parent-mesh-leaves.py` — prints `xbury=` per cluster.

No `closest_point`, R-tree, `placed_meshes`, or `fix_normals` reintroduced.

### Tuning knobs left in place (for the pending decision)

- `_CROSS_CLUSTER_BURY_TOL_MM` — raise > 0 to let grazing blades survive
  (narrower bald band); lower < 0 to cull more aggressively.
- Midrib sample fractions `{0, 0.5, 1.0}` — add points for a stricter test.

### Next action (awaiting user)

Decide from the render whether the bald recessed seam is acceptable:
- **Accept** → Option A is the fix; Option C (union / single parent mesh) loses
  its only advantage over the lighter D and can be dropped.
- **Want seam filled + shingled** → proceed to shared cross-cluster occupancy
  (Option B/D): keep this cull, add a world-space shingle-layer map shared
  across clusters so surviving seam leaves stack instead of skewer.

---

## DECISION — Build B (world-space shared shingle occupancy). D dropped. (2026-07-01)

The goal is **filling the inner corners** where clusters meet — leaves in the
seam that *shingle* instead of skewer — **not** the bald recessed seam that
Option A alone produces. Real trees have **many** overlapping clusters, so the
cross-cluster seam is pervasive, not a two-blob special case. Decision: **build
Option B**. D is dropped, for a concrete reason worth recording:

### Why D buys nothing over B (even at many clusters)

D's pitch is "CSG-union the clusters so the cull tests against the true outer
shell." But the buried cull is `signed_distance` with **inside = positive**, and
**a point is inside the union iff it is inside at least one member cluster**
(union = OR). So the Option A midrib test we already have — *"is this sample
inside ANY neighbour?"* — is **mathematically identical** to *"inside the CSG
union?"*, at 2 clusters or 200. CSG only gives something extra if you want the
union's *surface* to place on (that is Option C, the meridian rewrite). For
culling, D pays for a manifold3d union and returns an answer B already has for
free. No CSG anywhere in the plan.

### What "many clusters" actually changes

1. **Occupancy must be world-space, not `(phi, s)`.** The current shingle grid is
   keyed in per-cluster `(phi, s)` surface coordinates — you cannot share one
   `(phi, s)` frame across many arbitrarily-oriented clusters. Replace it with a
   single **world-space voxel grid** shared across all clusters.
2. **The cull is O(neighbours) per slot.** Fine for the two-cluster test; at tree
   scale, **spatially prune** `other_prox` to clusters whose bounding spheres
   overlap the current one. This is a scaling optimisation, **not** required for
   correctness — defer until a mid-size test shows it matters.

### The B delta over A (what to build)

Option A survivors near the seam still **skewer** each other because `occ` is
per-`_MeshCtx` — cluster A and cluster B can't see each other's layers. B makes
the shingle occupancy **one world-space grid shared by every cluster**, so a
seam leaf from A and a seam leaf from B get assigned **different normal-offset
layers** and stack. That is the entire mechanism that lets the corner fill
without re-introducing skewering.

### Current code (grounding for the implementer)

All in `src/dharmatiles/trees/placement.py`:

- **Shingle grid, per-mesh (to be shared):** `_MeshCtx.occ` — line ~456,
  `dict[(phi_bin, s_bin) → layer bitmask]`, default per-context.
- **Cell helpers:** `_shingle_cells(phi, s, ring_r, W, L)` (lines ~95–123)
  computes `(phi, s)` cells; `_shingle_pick_layer(occ, cells)` (lines ~126–140)
  and `_shingle_write(occ, cells, layer)` (lines ~143–147) are **key-agnostic** —
  they take a `cells` list + a dict, so they work unchanged with world-space keys.
- **Build site:** `_place_leaf_slot`, lines ~874–894 compute `phi_occ`,
  `ring_r`, call `_shingle_cells`, `_shingle_pick_layer`, set
  `base_blade = pt3d + layer·_SHINGLE_DELTA_MM·up_hint`. Line ~976 writes the
  grid **only on successful placement**. Keep all of this; swap only the cell
  computation.
- **Cull (keep as-is):** lines ~865–872 (midrib `signed_distance` vs
  `ctx.other_prox`). `other_prox` wired at lines ~1130–1134.
- **Context setup:** lines ~1085–1135 build one `_MeshCtx` per mesh.

Constants at lines ~72–92: `_SHINGLE_PHI_CELLS`, `_SHINGLE_S_CELL_FRAC`,
`_SHINGLE_MAX_LAYERS=4`, `_SHINGLE_DELTA_MM=0.30`, `_SHINGLE_FOOTPRINT_SCALE=0.80`.

### Step-by-step implementation plan

**Step 1 — Add `_SHINGLE_WORLD_CELL_MM` constant.** World voxel edge length.
Start at `L * _SHINGLE_S_CELL_FRAC`-equivalent — i.e. ~¼ leaf length, a few mm.
Coarse enough that overlapping leaves' footprints land in shared voxels; fine
enough to separate genuinely disjoint leaves. Tune on the render.

**Step 2 — New helper `_shingle_world_cells(base, tangent, lat, W, L, cell)`.**
Returns the set of world voxel keys `(ix, iy, iz)` a leaf's `W×L` footprint
covers. Sample the leaf mid-surface as a small grid (e.g. 3 across × 5 along)
over the rectangle spanned by `tangent` (length `L·FOOTPRINT_SCALE`) and `lat`
(width `W·FOOTPRINT_SCALE`), centred so the footprint core matches the old
behaviour. Voxelise each sample: `key = (floor(x/cell), floor(y/cell),
floor(z/cell))`; dedupe. **Compute from `pt3d` (the surface point, delta = 0)**,
NOT from `base_blade` — two overlapping leaves must map to the same voxels
regardless of which layer they land on, so the footprint reference depth is
always the surface. `lat = normalize(cross(up_hint, tangent))` (already computed
as `_lat_ov` at line ~922 — reuse the same vector).

**Step 3 — Share one `occ` dict across all contexts.** In
`place_leaves_on_multiple_meshes`, after the context loop (near line ~1134,
alongside the `other_prox` wiring), create `occ_shared: dict = {}` and assign
`ctx.occ = occ_shared` for every context. (Simplest; `_MeshCtx.occ` keeps its
default for single-context callers that build contexts directly.) The grid then
persists across the whole multi-cluster placement run.

**Step 4 — Swap the cell computation at the build site.** Replace lines ~884–891
(the `_mid`/`phi_occ`/`ring_r`/`_shingle_cells` block) with:
```python
_lat = np.cross(up_hint, tangent)
_lat /= max(float(np.linalg.norm(_lat)), 1e-9)
_cells = _shingle_world_cells(pt3d, tangent, _lat, W, L, _SHINGLE_WORLD_CELL_MM)
```
Leave `_shingle_pick_layer(ctx.occ, _cells)`, `_delta`, `base_blade`, and the
`_shingle_write(ctx.occ, _cells, _layer)` at line ~976 untouched — they are
key-agnostic and now operate on the shared world grid. `slot.s_row` (line ~480)
and `phi_occ` become unused for occupancy; `s_row` may stay for stats or be
removed. `_shingle_cells` (the `(phi,s)` version) can be deleted once nothing
references it.

**Step 5 — Layer semantics note (expected approximation).** A layer index is a
scalar; the actual standoff direction `up_hint` differs per cluster. So "layer 1"
on A stands off along A's normal, "layer 1" on B along B's normal — the offsets
aren't coplanar. That's fine and expected (the review flagged B's cross-surface
layering as "approximate"): deconfliction only needs two overlapping leaves to
get **different** layers so they sit at different depths. Greedy lowest-free
across the shared voxels guarantees that regardless of normal direction.

### Hard constraints (still in force)

- No `trimesh.proximity.closest_point` for deconfliction, no R-tree, no
  `placed_meshes`, no `fix_normals()`. (The `closest_point` calls at lines ~936
  and ~952 are the pre-existing oval-protrusion and curl float/bury checks —
  leave them; they are not deconfliction.)
- Proactive only: layer chosen **before** geometry is built; never build → test →
  retry.
- Keep Option A's midrib cull. B *adds* shared occupancy; it does not replace the
  cull.

### Validation

- **Develop on the two-cluster test** `src/scripts/test-multi-parent-mesh-leaves.py`
  (~5 s; a full tile is ~18 min). Every cross-cluster pathology shows up in the
  A+B pair. Confirm watertightness and per-cluster placed counts are unchanged
  and that seam leaves now stack (inspect `stats.shingle_layers` histogram — expect
  non-zero layers appearing on seam leaves that previously all sat on layer 0 or
  were culled).
- **Then a mid-size test (~6–8 overlapping clusters)** to shake out world-voxel
  keying and (if added) the bounding-sphere pruning before touching a full tree.
- **Tuning knobs:** `_SHINGLE_WORLD_CELL_MM` (voxel size — overlap sensitivity),
  `_SHINGLE_DELTA_MM` (per-layer standoff), `_SHINGLE_MAX_LAYERS` (standoff cap),
  `_CROSS_CLUSTER_BURY_TOL_MM` (raise > 0 to let more grazing seam leaves survive
  for the shingle map to stack, narrowing the bald band).

### Open question for the first render

With the cull still at `_CROSS_CLUSTER_BURY_TOL_MM = 0.0`, B lets survivors
shingle but the aggressively-culled band may still read as too bald. If so, the
follow-up is to **loosen the cull** (raise the tolerance / drop a midrib sample)
so more seam leaves survive into the now-safe shared shingle map — the cull and
the occupancy are the two dials that together set how full the corner reads.

---

## UPDATE — Profiling + tip-z ordering fix (2026-07-01)

User verdict: leaf placement/layout looks good and must not be substantially
changed, but the latest work is **way too slow**, and some placed leaves have
visible blade-surface tip `z` lower than the embedded root-oval tip `z`.

### Profiling results

Focused two-cluster script, placement-only cProfile:

- `place_leaves_on_multiple_meshes`: ~6.16 s under cProfile.
- `_place_leaf_slot`: ~5.88 s.
- `_contact_angle_for_mesh`: ~3.59 s via `ProximityQuery.on_surface` /
  `closest_point`.
- Cross-cluster `signed_distance` cull: only ~0.07 s on the two-cluster case.
- Shingle occupancy: effectively free (~0.004–0.007 s).

Conclusion: the shared world occupancy is not the runtime problem.  The focused
test is dominated by the pre-existing contact-angle BVH solve; many-cluster
tiles also need cull-neighbour pruning because otherwise the midrib cull scales
as slots × clusters.

### Rejected experiments

- Local-face contact solve using `slot.local_faces`: slower (~6.9 s) and changed
  coverage (`22/22` → `20/17`).  Rejected.
- Reduced contact candidate sets (3 or 5 lateral columns): faster (~2.8–3.1 s
  focused) but changed coverage (`22/22` → `19/16`).  Rejected.

### Landed fixes

- Kept the full contact candidate set to preserve layout.
- Reduced contact-angle bisection refinement from 8 to 5 iterations after the
  bracket is found.  Focused test still places `22/22` leaves; `contact` point
  evals drop from ~118k to ~92k and focused total from ~4.6 s to ~4.1 s in the
  instrumented run.
- Added bounding-sphere pruning for `ctx.other_prox`, expanded by one leaf
  length.  This preserves culling for any cluster a blade could reach, while
  avoiding all-pairs neighbour checks on larger trees.
- Added timing counters:
  - `slot.contact_angle`
  - `contact.bvh_on_surface`
  - `slot.cross_bury_cull`
  - `slot.shingle`
  - `slot.oval_contains`
  - `slot.oval_proximity`
  - `slot.curl_check`
  - `phase1.cross_prune`
- Added final geometry invariant for the blade/root tip z ordering:
  `_TIP_Z_CLEARANCE_MM = 0.02`.  The original surface is still used for the
  existing accept/reject curl-burial check, then the accepted blade surface is
  raised in world `+Z` just enough that visible surface tip z is at least
  `root_oval_tip_z + 0.02`.  The root oval stays plugged at `pt3d`.
- `LeafPlacementStats` now records `tip_z_clearances` and `tip_z_lifts`; the
  focused script prints min/median/max clearance and correction counts.

### Current validation

`python -u src/scripts/test-multi-parent-mesh-leaves.py /private/tmp/multi-parent-iter5.stl`

- Placed counts unchanged from the accepted layout: A `22`, B `22`.
- Tip clearance fixed: min `0.02 mm` on both clusters.
- Focused timing: total placement ~`4.15 s`; contact solve ~`2.22 s`.
- Known artifact-report failures remain the pre-existing visual diagnostics:
  bald bottom row on A and floating curl warnings.

### Latest goal — fail fast on inverted solid tips

User reported both problems continuing on the full tile path and asked for a
hard assertion after each leaf solid is constructed.  Added a post-construction
assertion immediately after the accepted blade-surface lift:

```python
surface_tip_z = solid.vertices[_tip_idx, 2]
root_tip_z = solid.vertices[len(surf.vertices) + _tip_idx, 2]
assert surface_tip_z > root_tip_z
```

This runs after `solidify_leaf(...)`, after the pre-burial curl check, and after
any `_TIP_Z_CLEARANCE_MM` correction has been applied to the visible blade
surface.  Failure reports surface/root z, clearance, row, column, and shingle
layer.

Validation on `src/tiles/water/1x1-grass-tree+water.tile.py`:

- `python -m compileall -q src/dharmatiles/trees/placement.py` passes.
- `python -m cProfile -o /private/tmp/1x1-grass-tree-water.prof -m dharmatiles.terrains.tile --tile src/tiles/water/1x1-grass-tree+water.tile.py`
  ran through the DB export and into OL generation with no assertion printed,
  but exited with process code `-1` before cProfile flushed a `.prof` file.
- `python -m cProfile -o /private/tmp/1x1-grass-tree-water-db.prof -m dharmatiles.terrains.tile --tile src/tiles/water/1x1-grass-tree+water.tile.py --formats db`
  generated `stl/db/water/1x1-grass-tree+water-db.stl` with no assertion
  printed, then exited with code `-1` before cProfile flushed.
- A wrapper that dumped cProfile in `finally` also exited with code `-1`
  without running `finally`, so the process termination is not a normal Python
  exception or `SystemExit`.
- Added `dharmatiles.terrains.tile --no-png` to skip post-build PNG thumbnail
  rendering and catalog writing.  Re-running the same DB tile with:

  ```bash
  python -m cProfile -o /private/tmp/1x1-grass-tree-water-db-no-png.prof \
    -m dharmatiles.terrains.tile \
    --tile src/tiles/water/1x1-grass-tree+water.tile.py \
    --formats db --no-png
  ```

  exits normally with code `0`, writes the `.prof` file, and still produces no
  tip-z assertion.  This isolates the previous `-1` to the post-export
  PNG/render path, almost certainly the native `pyrender`/offscreen renderer
  teardown path rather than Python control flow.

### Successful full-tile cProfile details (`--no-png`)

Profile file: `/private/tmp/1x1-grass-tree-water-db-no-png.prof`

Command:

```bash
python -m cProfile -o /private/tmp/1x1-grass-tree-water-db-no-png.prof \
  -m dharmatiles.terrains.tile \
  --tile src/tiles/water/1x1-grass-tree+water.tile.py \
  --formats db --no-png
```

Result:

- Exit code: `0`
- Total cProfile runtime: `222.390 s`
- Function calls: `359,762,754` total (`359,518,373` primitive)
- Tile output: `stl/db/water/1x1-grass-tree+water-db.stl`
- 3MF assembly: completed, `1` file
- Tip-z assertion: no failures

Top cumulative stack:

| function | calls | cumtime |
|---|---:|---:|
| `tile.py:1278(main)` | 1 | `221.796 s` |
| `tile.py:1088(_build_tile)` | 1 | `219.010 s` |
| `tile.py:510(build_tile_from_spec)` | 1 | `219.010 s` |
| `tile.py:345(_build_tile_content)` | 1 | `218.534 s` |
| `layer.py:282(apply)` / `layer.py:142(scatter)` | 1 | `218.003 s` |
| `mesh.py:55(build_branch_mesh)` | 1 | `217.981 s` |
| `mesh.py:1107(_build_foliage_cluster_mesh)` | 64 | `217.257 s` |
| `placement.py:1304(place_leaves_on_mesh)` | 64 | `208.350 s` |
| `placement.py:1048(place_leaves_on_multiple_meshes)` | 64 | `208.348 s` |
| `placement.py:668(_place_leaf_slot)` | 2,904 | `199.007 s` |

Placement-specific profile:

| function | calls | cumtime | note |
|---|---:|---:|---|
| `_place_leaf_slot` | 2,904 | `199.007 s` | dominant leaf-placement loop |
| `_contact_angle_for_mesh` | 2,554 | `100.130 s` | contact solve for accepted/build-attempt slots |
| `_max_inside` | 33,356 | `99.966 s` | repeated contact-angle inside-depth evaluations |
| `_collect_row_slots` | 290 | `1.330 s` | row/slot enumeration is not a hotspot |
| `_shingle_world_cells` | 2,554 | `0.186 s` | world shingle footprint is negligible |
| `_shingle_pick_layer` | 2,554 | `0.013 s` | layer choice is negligible |
| `_shingle_write` | 1,991 | `0.010 s` | occupancy writes are negligible |
| `_pt` instrumentation | 40,039 | `0.039 s` | timing instrumentation overhead is negligible |

Trimesh/proximity hot paths:

| function | calls | cumtime | why it matters |
|---|---:|---:|---|
| `proximity.py:121(closest_point)` | 34,035 | `110.707 s` | includes contact solve + oval/curl proximity |
| `proximity.py:314(on_surface)` | 30,899 | `101.772 s` | contact-angle candidate projection |
| `base.py:3148(contains)` | 4,546 | `82.791 s` | embedded oval containment gate |
| `ray_triangle.py:156(contains_points)` | 4,546 | `82.788 s` | contains implementation |
| `ray_triangle.py:177(ray_triangle_id)` | 3,982 | `81.409 s` | ray/triangle work behind contains |
| `proximity.py:25(nearby_faces)` | 34,035 | `77.663 s` | R-tree face lookup for closest-point |
| `index.py:755(intersection)` | 4,358,772 | `60.156 s` | spatial-index query underneath proximity/ray checks |

Interpretation:

- The runtime is the tree leaf placement path, not STL export, 3MF assembly,
  world-space shingling, or the new tip-z assertion.
- The largest optimization targets are the contact-angle `on_surface` loop and
  the embedded-oval `mesh.contains` ray path.
- The shared world shingle system is not responsible for the slowdown: its
  total cProfile cost is under `0.25 s` across the full DB tile.

The live placement timers on the full DB tile consistently show the same hot
spots as the focused profile: `slot.contact_angle` / `contact.bvh_on_surface`,
then `slot.oval_contains` and `slot.initial_build`.  `slot.shingle` remains
negligible, usually around `0.004–0.007 s` per cluster.

---

## RESOLVED — 8× speedup: embreex + analytic contact angle + normal pull-away (2026-07-01)

The "glacially slow" `1x1-grass-tree+water` tile is fixed.  Full DB tile wall
time: **~222 s → 28 s (~8×)**, in two independent, stacking changes.  No change
to the deconfliction/shingle model; the placement *layout* is preserved to
within the accepted tolerance (per-cluster placed counts equal-or-better).

### Fix 1 — install `embreex` (the missing fast ray backend)

Profiling had already isolated two hot ops in `_place_leaf_slot` against the
~1280-face foliage-cluster mesh: the contact-angle `on_surface` solve (~100 s)
and the oval-containment gate `mesh.contains` (~82 s).  The 82 s was **pure
waste**: `embreex` was not installed, so trimesh silently fell back to its
pure-Python `ray_triangle` engine (confirmed: `type(m.ray).__module__ ==
'trimesh.ray.ray_triangle'`).  Benchmarked on a 1280-face cluster-sized mesh,
`mesh.contains` on ~120 points went **18 ms → 0.20 ms (~90×)** with embreex.

- Installed `embreex 4.4.0` (macOS arm64 wheel available) and **added it to
  `pyproject.toml` dependencies** with a comment, so a fresh `pip install -e .`
  never regresses to the slow ray engine.
- `slot.oval_contains` dropped from ~82 s to ~0.01 s; full DB tile **222 s →
  102 s**.  Zero output change — embree returns identical inside/outside.
- Ruled out empirically: a **vectorized brute-force** closest-point/winding
  replacement is ~5× *slower* than trimesh's rtree-pruned proximity at 1280
  faces (materializing N×F arrays is memory-bound).  Do not retry it.

### Fix 2 — analytic contact angle + normal pull-away (kills the 100 s solve)

The remaining bottleneck was `_contact_angle_for_mesh`: ~9 `on_surface` evals
per leaf (measured: histogram 7–11), each over a ~140-point candidate cloud,
bisecting for the lean angle.  embree does **not** help it (proximity is
rtree+triangle math, not rays).

**Key reframe (user's):** the contact solve was doing *two* jobs at once —
orient the leaf AND keep it from penetrating the parent mesh — and the
non-penetration job is what forced sub-degree precision (hence the iteration).
Decouple them:

1. **Lean → closed form.**  `_contact_angle_analytic(dL, dN, T0, up_hint, m)`
   solves the belly-dip grazing angle against a locally-planar surface.  Rotating
   the belly-dip displacement `(dL, dN)` about the base by θ (the *same* rotation
   the old mesh solver used), its inside-depth is `A·cosθ + B·sinθ` with
   `A = dL·t + dN·n`, `B = dN·t − dL·n` (`t = T0·m`, `n = up_hint·m`); set it to
   the base inside-depth `D0` and solve — one `acos`, no iteration.  The
   belly-dip `(dL, dN)` is the shape-only binding point (`_leaf_belly_dip`,
   mirrors `_contact_angle_for_sphere`'s `argmin(d_normal)` over the tip-half
   midrib), computed once per placement run and threaded into every slot.
2. **Non-penetration → normal pull-away.**  After the leaf solid is built, the
   blade surface's curl-region penetration is measured (signed depth via closest-
   triangle normal — same convention as the old solver) and the blade is
   **translated outward along `up_hint`** by `penetration + _PULL_CLEARANCE_MM`
   until it clears.  This is a *translation*, distinct from `lift` (a rotation
   about the base) and from the shingle standoff (which stacks overlaps).  The
   root oval stays plugged at `pt3d`; `solidify_leaf` just builds a taller neck —
   the exact mechanism the shingle offset already relied on.

Because pull-away absorbs any residual penetration, the closed-form lean need
not be exact.  Two consequences, both acceptable / positive:

- **Coverage rises.**  Leaves the old path *rejected* as preburied are now pulled
  out and placed instead — `skipped_preburied` went to **0** on the two-cluster
  test; per-cluster counts A `22 → 23`, B `22 → 23`.
- **Slightly more standoff on curved regions.**  With the base normal alone the
  lean under-shoots curvature, inflating float; measuring the surface normal at
  the belly-dip point (`_ANALYTIC_MEASURE_BELLY = True`, 1 extra `on_surface`
  per leaf, still ~0.3 ms) recovers curvature.  Final float is comparable to the
  embree baseline (B lower on every count; A median 1.25 vs 1.39 mm, with one
  3.98 mm outlier near the A∩B seam).  The render reads the same shingled-cluster
  silhouette with marginally more visible standoff.

### Implementation (`src/dharmatiles/trees/placement.py`)

- Constants `_ANALYTIC_CONTACT = True`, `_ANALYTIC_MEASURE_BELLY = True`,
  `_PULL_CLEARANCE_MM = 0.05`, `_PULL_MAX_MM = 3.0` (penetration beyond this is
  still rejected as preburied — genuinely stuck inside).
- New `_leaf_belly_dip(**leaf_kw) → (dL, dN)` and `_contact_angle_analytic(...)`.
- `_place_leaf_slot` gains a `belly_dip` param; the `_contact_angle_for_mesh`
  call is now behind `if _ANALYTIC_CONTACT` (legacy iterative path kept as the
  `else` branch for A/B — deletable once the analytic path is confirmed on more
  tiles, along with the now-legacy-only `contact_candidates`/`ca_cache`).
- Curl float/bury block rewritten: one `proximity.on_surface(surf.vertices)`
  measures signed penetration; curl-region max drives the pull-away; tip-z
  ordering (`_TIP_Z_CLEARANCE_MM`) is applied *after* the pull.  New
  `LeafPlacementStats.pull_aways` per-leaf record.

### Results

| stage | full DB tile | focused 2-cluster placement | contact solve |
|---|---:|---:|---|
| original (pure-Python rays, iterative) | ~222 s | ~24 s → ~5 s (embree) | ~9 evals/leaf, dominant |
| + embreex | 102 s | ~5 s | unchanged |
| **+ analytic + pull-away** | **28 s** | **0.45 s** | **~0 (0.01 ms/leaf)** |

Full DB tile exits 0, no tip-z assertion failures, 1.40 M faces (vs 1.32 M —
more leaves placed).  `NOT watertight` is the pre-existing state (union of
thousands of separate leaf solids), unrelated to this change.  Hard constraints
still honoured: no `closest_point`/R-tree deconfliction, no `fix_normals`.

### Open items

- The legacy `_contact_angle_for_mesh` path + `contact_candidates`/`ca_cache`
  are dead under `_ANALYTIC_CONTACT = True`; delete once confirmed on more tiles.
- One float outlier (~4 mm) near the A∩B seam — pull-away over-reacting to deep
  curl penetration on the concave seam; tighten later if it reads badly at tree
  scale.
- Next remaining hot spot is `slot.curl_check` (the single pull-away
  `on_surface`, ~2.8 ms/leaf) and `slot.initial_build`; both are now the floor.

---

## Cross-cluster culling finally wired up (2026-07-01, later)

User saw "a fair number of intersecting leaves (cross mesh)" on the real 1×1 tree
tile and asked to cull them.

### Root cause — the whole Option A/B system was dead on real tiles

`_build_foliage_cluster_mesh` (mesh.py) called `place_leaves_on_mesh` **once per
cluster, in isolation**, so every cluster's `other_prox`/`other_meshes` neighbour
list was empty.  The cross-cluster cull *and* the shared world-space shingle
occupancy — everything the earlier "Option A / Option B" sections built — had
therefore **never run in production**; only the two-cluster test (which passes
both meshes to one call) ever exercised them.  That is why nothing culled the
intersections.

### Fix 1 — place all foliage clusters together

`build_branch_mesh` (mesh.py) now builds every foliage cluster mesh with
`leaves=False`, collects them into `foliage_clumps`, and after the build loop
makes **one** `place_leaves_on_multiple_meshes` call over all clusters (per-cluster
seeds/labels).  Verified: batched placement reproduces the per-cluster leaf count
exactly (2156 leaves, 1.399 M faces with culling disabled) — so batching itself
is neutral; only the cull changes output.  Placement order is global-z batched and
the shingle `occ` is shared, but non-overlapping clusters occupy disjoint world
voxels so their layout is unchanged; only overlapping seams reorganise.

### Fix 2 — make the cull correct and fast

Three problems with the cull as inherited, all fixed:

1. **`signed_distance` was too slow.**  With neighbours now non-empty everywhere,
   the O(leaves × neighbours) `ProximityQuery.signed_distance` (rtree closest-point)
   blew the tile past 2 min.  Switched the inside test to embree
   `mesh.contains` (embreex, ~15× faster; needs only inside/outside) in a new
   `_inside_neighbour(other_meshes, pts, near_pt, reach, min_frac)` helper, with a
   cheap per-neighbour AABB reject keyed on the leaf base so far clusters are
   skipped.  Full tile back to ~28 s.
2. **The pre-build midrib/base cull was catastrophically over-aggressive.**  It
   culled a leaf if its base/centerline sat inside any neighbour — but in a dense
   canopy a cluster's own surface is *usually* inside its neighbours, so it nuked
   almost everything (2156 → ~200 leaves).  **Removed entirely** (this is the same
   base-inside test the Option A notes had already deprecated once).
3. **Cull now runs once, on the final geometry, by fraction.**  A single pass over
   the built + pulled-away blade surface culls the leaf when
   `≥ _CROSS_CLUSTER_BLADE_INSIDE_FRAC` (0.30) of its blade vertices lie inside any
   one neighbour (fraction, not count, so it is independent of leaf tessellation).
   This removes skewering leaves (blade straddling a neighbour's skin) and
   fully-buried invisible leaves, while keeping leaves that merely graze a seam.

### Result

- Full DB tile: **2156 → 721 leaves**, faces **1.40 M → 0.70 M** (~half — a real
  print/slice win), runtime unchanged (~28 s).
- Side-by-side renders (cull off vs frac 0.30) are visually near-identical: the
  ~1435 culled leaves were the hidden interior/overlap ones; the visible rosette
  canopy is preserved, intersections cleaned.
- Two-cluster test: A 29 / B 25 with 13 seam leaves culled (gentler than the old
  count test, which gave 20/20).
- `_CROSS_CLUSTER_BLADE_INSIDE_FRAC` is the tuning dial: **lower = cull more**
  grazing/partial intersections; higher = keep more.  0.30 is a conservative
  default that provably preserves the silhouette.

### Files

- `mesh.py::build_branch_mesh` — two-phase (collect clumps → one batched
  placement call); `_build_foliage_cluster_mesh` now always called `leaves=False`
  (its inline placement path is dead but retained).
- `placement.py` — `_inside_neighbour` (embree contains + AABB reject);
  `_CROSS_CLUSTER_BLADE_INSIDE_FRAC`; `_MeshCtx.other_prox` renamed
  `other_meshes` (stores meshes, not ProximityQueries — the cull uses `.contains`);
  pre-build midrib cull deleted; single fraction-based blade cull after pull-away.
