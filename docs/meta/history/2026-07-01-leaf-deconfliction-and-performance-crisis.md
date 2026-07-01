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
