# Organic leaf placer — performance analysis (2026-07-03)

**Question:** a `1x1-grass-tree+water` tile takes ~30 s to generate. How much of
that is the organic leaf placer, and can we speed it up *dramatically* without
changing the visuals?

**Short answer:** the visible-geometry pipeline is already well optimized (no
per-leaf mesh scans, embree-only, leaves concatenated not unioned). But the
tile is still dominated by two costs, and **~two thirds of the wall clock is
recoverable without touching a single output vertex**:

| Lever | Cost today | After | Risk | Effort |
|---|---|---|---|---|
| Install `xxhash` (trimesh fast-hash dep) | — | **−14 % (−4 s)** | none | trivial (done) |
| Replace `fix_normals()` on foliage clumps | ~7 s | ~0.2 s | low, verifiable | medium |
| Stage-batch the placement embree queries | ~8 s | ~2–3 s | medium | high |

All three are geometry-preserving. Combined, they take the tile from ~29 s to a
projected **~12–14 s** with the exact same leaves.

---

## IMPLEMENTATION STATUS (updated 2026-07-03, mid-session)

Golden regression gate for `1x1-grass-tree+water` (must stay byte-identical):

```
DB: 1497 placed / 1838 roots, build-fail=155, branch-cull=186 → 527,340 v · 1,047,272 f
OL: 1479 placed / 1811 roots, build-fail=134, branch-cull=198 → 503,991 v · 1,001,006 f
```

### ✅ Lever 1 — `xxhash` dependency — DONE & shipped
Added to `pyproject.toml` `[project].dependencies` (with a comment pointing
here). Already installed in this env. Measured effect: **29.0 s → 25.0 s
(−14 %)**; embree `contains` per-call overhead 151 µs → 86 µs (cache-hash was
~half of it). No code change, no risk.

### ✅ Lever 2 — foliage-clump winding fix — DONE & validated
`trees/mesh.py`:
- New helper `_orient_outward(mesh)`: O(F) signed-volume test → reverse all
  faces if net-inward. Cheap replacement for `fix_normals()` on a mesh that is
  already winding-consistent.
- `_build_foliage_cluster_mesh`: both pole fans re-wound to AGREE with the quad
  strips (south `[0, r0+k, r0+k1]`, north `[_np, last+k1, last+k]`). Root cause
  of the old inconsistency: each fan shared its ring edge in the SAME direction
  as the adjacent strip, so ~22 faces disagreed and `fix_winding` had to flood-
  fill the whole mesh. Now `is_winding_consistent` is **True by construction**.
- Both `shaped.fix_normals()` and `result.fix_normals()` → `_orient_outward(...)`.

**Validation (3 representative clumps + full tile):**
- raw construction `is_winding_consistent = True` (was False).
- `vertex_normals` vs old `fix_normals`: max error **0.0000°**.
- vertices differ by ≤ **2.2e-16 mm** (one ULP on 13/590 verts, from float
  summation order in the normal calc — erased by float32 STL export).
- Full-tile tallies + export vertex/face counts: **IDENTICAL to golden.**
- Measured effect: **Tree layer 10.9 s → 5.48 s per scale** (clump build
  ~5.4 s → ~0.5 s). ~7 s off the tile.

### ⏳ Lever 3 — stage-batch placement embree — NOT STARTED
See the dedicated section below ("The big structural win"). Current state after
Levers 1+2: placement is ~5 s/scale (~10 s/tile), of which embree
(`contains` + rays) is the bulk. **Key enabling fact confirmed this session:**
in the organic call each leaf's `_attempt_leaf` is *fully independent* —
`neighbour_meshes=[]`, shingle standoffs are precomputed, and `avoid_meshes`
are branches (not other leaves). So the per-leaf pipeline can be restructured
into staged passes over all leaves (SoA) with **no cross-leaf dependency**;
each stage becomes one batched embree call. This is the remaining work: a
careful rewrite of the `_attempt_leaf` driver in `trees/placement_organic.py` /
`trees/placement_leaf.py`, guarded by the golden gate above (tallies +
vertex/face counts must match exactly).

**Where the ~13 embree calls/leaf come from** (batch targets, in loop order):
`_project_to_surface` (1 ray) · `_seat_oval_tilt` (≤3 × [contains(2)+ray(2)]) ·
oval containment guard (1 contains) · belly-dip seat (1 contains + 1 ray) ·
`tuck_base` (1 contains + 1 ray) · `tuck_tip` (1 contains + 1 ray) · burial
cull / `bury_lift` (1–2 contains + 1 ray).

Risk note: batching must reproduce the exact same threshold decisions per leaf
(e.g. `bury_lift` same-wall classification, tuck rotation sign selection) or the
placed/culled tallies drift. Machine-epsilon differences are acceptable (Lever 2
already introduced ~2e-16 with zero tally change), but logic differences are not.

---

## How the measurement was done

Everything below is **real wall-clock**, not cProfile (cProfile inflates the
Python-heavy hashing paths ~1.5×). The method: monkeypatch `Trimesh.fix_normals`
and the embree `contains_points` / `intersects_location` entry points to
accumulate wall time and call counts, then run the tile end-to-end.

Baseline tile (`1x1-grass-tree+water`, both DB + OL scales, incl. render/export):

```
total run:            29.06 s
fix_normals:           7.99 s  /  2107 calls
embree contains:       7.26 s  / 47996 calls   (151 us/call)
embree rays:           4.05 s  / 47861 calls   ( 85 us/call)
```

So **fix_normals (8 s) + embree (11.3 s) = 19.3 s ≈ 66 % of the tile.** The
organic placer's own `place_leaves_organic` prints ~6.7 s/scale (≈13.4 s of the
tile); the remaining tree cost is foliage-clump mesh building.

Cross-checked with cProfile (`/tmp/leafprof.out`), which additionally exposed
the *why* behind the embree per-call cost: trimesh cache-validation hashing
(`caching.hash_fallback`, 3.75 s self-time; `__hash__`/`verify` ~6 s cumulative)
runs on every mesh-property access, and `fix_winding`'s `group_rows` (7.9 s
cumulative) drives the clump cost.

---

## Where the time actually goes

### Cost 1 — `fix_normals` on foliage clumps (~7.5 s of the 8 s)

`trees/mesh.py:_build_foliage_cluster_mesh` calls `fix_normals()` **twice** per
clump (once on `shaped` to get displacement normals, once on the final
`result`). With ~64 clumps × 2 scales that is ~256 calls, on meshes up to tens
of thousands of faces — 770 k faces of winding-repair per tile.

`fix_normals()` → `fix_winding()` → `group_rows()`: an O(E log E) edge-grouping
that makes face windings mutually consistent, then a global flip so normals
point outward.

Why it's needed (I checked — my first assumption was wrong): the clump is a
structured rounded-cone sweep (pole fan → quad strips → pole fan), but as
constructed it is **not** winding-consistent:

```
is_winding_consistent (raw construction): False
signed volume: -1254  (net inward)
```

The leaf placer reads `clump.vertex_normals` for placeability
(`normals[:,2] >= threshold`) and for the noise displacement direction, so the
normals genuinely matter — we can't just skip the repair.

**But it is grossly overpriced.** On a representative 1176-face clump:

```
fix_normals():               29.5 ms/call
volume-sign flip + normals:   0.8 ms/call   (37x cheaper)
```

Only ~22 of 1176 faces are actually inconsistent — the pole fans / section
seams are wound opposite to the quad strips. A naive "orient each face outward
from the spine axis" replacement gets *close* (mean 2.8° normal error) but blows
up at the degenerate poles (15 verts >30°, up to 177°, still not consistent), so
it is **not** a safe drop-in.

**Recommendation:** fix the winding *in construction*. Get the pole-fan (and
any section-seam) face ordering to match the quad strips so
`is_winding_consistent` is `True` on build, then replace both `fix_normals()`
calls with one O(F) signed-volume orientation:

```python
mesh = trimesh.Trimesh(vertices=verts, faces=faces, process=False)
if mesh.volume < 0:                       # cheap: sum of signed tet volumes
    mesh.faces = np.ascontiguousarray(mesh.faces[:, ::-1])
# vertex_normals now recompute from consistent, outward faces
```

Vertices and faces are byte-identical to today's output; only the (invisible)
winding order and the repair *path* change. **Validation gate:** assert
`is_winding_consistent` is `True` post-construction and that
`clump.vertex_normals` matches the current `fix_normals()` result within ~1e-3
before/after. Expected saving: **~7 s/tile.**

### Cost 2 — embree queries in placement (~8–11 s)

~48 k `contains` + ~48 k `intersects_location` calls per tile — roughly **26
embree ops per leaf** across ~1838 roots × 2 scales. Per `_attempt_leaf`:

- `_seat_oval_tilt`: up to 3 iterations × (1 `contains` + 1 ray)
- oval containment guard: 1 `contains`
- belly-dip seat: 1 `contains` + 1 ray
- `tuck_base`, `tuck_tip`: 1 `contains` + 1 ray each
- burial cull / `bury_lift`: 1–2 `contains` + 1 ray
- plus `_project_to_surface` (1 ray) in the outer loop

Each call processes only **1–2 points**, so the ~85–150 µs/call is almost all
*fixed overhead* (BVH query marshalling, numpy allocation, and — see below —
trimesh cache-hash validation), not ray work. This is latency-bound, not
throughput-bound.

### Cost 3 (hidden, inside Costs 1 & 2) — trimesh cache-hash on the slow path

`xxhash` was **not installed**, so trimesh fell back to `blake2b`
(`caching.hash_fallback`) for the cache-validation hash it runs on *every*
geometry-property access. This tax is paid inside `fix_winding` (repeated
`mesh.edges`/`mesh.faces` access) and inside every embree call (`mesh.bounds`,
`mesh.triangles` for the intersector).

---

## The free win: install `xxhash`

`xxhash` is trimesh's optional fast-hash acceleration. Installing it and
re-measuring (identical output):

```
total run:        25.02 s        (was 29.06 s  →  -4.0 s, -14%)
fix_normals:       7.81 s        (≈ unchanged — group_rows is numpy work, not hashing)
embree contains:   4.14 s / 47996 calls (86 us)   (was 151 us — nearly halved)
embree rays:       3.76 s / 47861 calls (79 us)   (was  85 us)
```

Two takeaways:

1. **embree per-call overhead was ~half cache-hashing.** `contains` dropped
   151 → 86 µs with no code change — the wrapper was spending as much time
   validating the mesh cache as casting rays.
2. `fix_normals` barely moved, confirming its cost is `group_rows`' own numpy
   sort/unique work (Cost 1's fix is what addresses it), not the hash.

**Action:** add `xxhash` to the project dependencies (`pyproject.toml` /
`setup.py`). It is pure upside and already installed in this environment.

---

## The big structural win: stage-batch the placement embree

After `xxhash` + the clump fix, embree (~8 s) is the top remaining cost, and
it's all per-call overhead from doing one leaf at a time in a Python loop.

The placement pipeline is a **sequential dependency chain within a leaf**
(seat → build → skew → belly-seat → tuck → cull) but **embarrassingly parallel
across leaves at each stage.** Restructure from array-of-structs (loop over
leaves, each doing its own tiny embree calls) to **struct-of-arrays** (run one
stage for *all* live leaves, one batched embree call, drop the failures, next
stage):

- Stage 1 — `_seat_oval_tilt`: iterate the Newton step across all N roots at
  once. Iteration 1 = one `contains(2N pts)` + one ray-cast(2N); iteration 2
  only on the not-yet-converged subset; ≤3 iterations. Collapses ~3 N tiny
  calls into ~6 big ones.
- Stage 2 — oval containment guard: one `contains` over all seated ovals.
- Stage 3 — build surfaces (pure numpy, already fast; vectorizable further).
- Stage 4 — belly-dip / tuck / cull: each is one `contains` + one ray per live
  leaf → one batched pair per stage.

The per-leaf **math is unchanged** (same seat solve, same skew, same tucks) —
only the *dispatch* changes, so the output geometry is identical. Batched embree
amortizes the ~80 µs fixed overhead across thousands of points, so ~48 k calls
→ ~dozens. Expected: placement embree ~8 s → **~2–3 s**.

This is the highest-effort item (a careful rewrite of `_attempt_leaf`'s driver
into staged passes, keeping a "live mask" of surviving leaves) and the one with
real regression risk, so it wants a golden-output diff test (vertex/face counts
+ placed/culled tallies per clump must match exactly).

### Cheaper embree half-measure (optional)

Without the full SoA rewrite, several `contains` calls exist only to pick a ray
direction (`dip_inside = mesh.contains(dip)` → cast ±normal). Casting the ray in
**both** directions once and inferring inside/outside from the hits removes ~one
`contains` (86 µs) per belly-dip/tuck — order ~2 s/tile. Fiddly and per-leaf;
lower value than batching, listed for completeness.

---

## What is *not* worth doing (visual cost)

- **Fewer clump faces / coarser tessellation** — changes the silhouette. Off
  limits (the whole point is the visuals).
- **Fewer leaves / larger spacing** — directly changes coverage/look.
- **Skipping the second (OL) scale** — halves wall clock but drops a real
  output; fine for *iteration* (`--tile` one scale) but not for batch.
- **Unioning leaves** — already correctly avoided; leaves are concatenated
  watertight shells (slicers union at slice time).

---

## Recommended order of work

1. **`xxhash` dependency** — done locally; add to project deps. −14 %, zero risk.
2. **Foliage-clump winding fix** — construction-consistent winding + O(F)
   volume-sign orient; drop `fix_winding`. ~−7 s, low risk, fully verifiable.
3. **Stage-batch placement embree** — SoA rewrite of the `_attempt_leaf` driver.
   ~−5 s, guarded by a golden-output test.

Projected result: **~29 s → ~12–14 s per tile, identical geometry.**

## Reference — file map

- `trees/placement_organic.py` — `place_leaves_organic`: union, Poisson roots,
  direction field, shingle standoffs, per-leaf build loop.
- `trees/placement_leaf.py` — `_attempt_leaf` (the per-leaf embree chain),
  `_seat_oval_tilt`, `_project_to_surface`, `_points_inside_any`.
- `trees/mesh.py` — `_build_foliage_cluster_mesh` (the `fix_normals` cost) and
  `build_branch_mesh` (calls the placer on all clumps together).
