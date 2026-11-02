# Leaf Placement Phi Distribution — Session 2026-06-28

## What We Were Working On

Improving the azimuthal distribution of leaves within a row in `place_leaves_on_mesh`
(`src/dharmatiles/trees/placement.py`), and hardening the duplicate-detection logic
in `src/scripts/test-leaf-placement.py`.

---

## Changes Made This Session (all committed to `main`)

### 1. Duplicate detection: midpoint distance instead of base distance

`_same_row_duplicate_indices()` in `test-leaf-placement.py` now measures pairwise
distance at the **leaf midpoint** (`base + L/2 * tangent`) rather than at the base
position.  Leaves that fan from a shared apex base in different directions have
well-separated midpoints and are no longer falsely flagged.  True polygon-iterated-twice
duplicates (same phi AND same tangent) still have coincident midpoints.

The old phi-angle gate (`Δphi < 5°`) was removed — the midpoint metric makes it
unnecessary.

### 2. Duplicate leaf red-colouring

`_mark_error_leaves()` now also colours same-row duplicate leaves red (via the shared
`_same_row_duplicate_indices()` helper).  Previously only long-root / floating / buried /
upward-tangent leaves were red.

### 3. Phi angle distribution: belly edge-normal approach (WORK IN PROGRESS — BROKEN)

We were trying to improve the azimuthal spread of leaves at the cluster apex (Object 2,
row 4 z=35.2), where 6 leaves fan from one tiny base point.  The root cause is that
the foliage cluster has a **D-section belly cross-section** whose outward normals don't
cover the full 360°, so some phi targets collapse to the same edge.

**Three approaches tried, none working well:**

| Approach | Min gap (apex row) | Status |
|---|---|---|
| Original uniform phi (0°,60°,…) | 29° | Baseline — best so far |
| Arc-length on belly polygon | 22° | Regression |
| Edge-normal matching (fixed targets) | 19° | Worse regression |
| Edge-normal + largest-gap start offset | 0.6° | Catastrophically broken |

**Current state of `placement.py`**: the "largest-gap start offset + edge-normal
matching" code is in place but produces terrible results.  **Must be reverted or
replaced before shipping.**

---

## Root Cause Analysis (incomplete)

At the apex row (z=35.2), the row cross-section is a tiny polygon fragment (~0.3 mm
perimeter).  All base positions come from the fallback arc-length interpolation on this
tiny polygon, so they're essentially at the same 3D point.  The **tangent direction is
therefore determined almost entirely by `up_hint`** (the belly surface normal).

The belly at z=34.6 has:
- perim ≈ 16.7 mm, centroid ≈ (40,-0.056)
- It is a D-section: outward normals only span ~298°, leaving a ~62° flat-face gap

The 0.6° min-gap failure suggests the edge-normal formula may have a **sign/winding
error** causing targets to point inward.  The Shapely polygon from `to_2D()` may be CW
rather than CCW, which would flip the formula.  **This needs to be verified first.**

---

## Recommended Next Steps

### Step 1 — Revert placement to uniform phi (safe baseline)
```python
_phi_list = [2.0 * math.pi * ci / n_col for ci in range(n_col)]
```
Remove all belly-polygon phi computation. Uniform phi gave the best results (29° min
gap) and avoids the winding-direction bug.

### Step 2 — Verify belly polygon winding
Add a debug print: `print(bpoly.exterior.is_ccw)` or check signed area.  The Shapely
`exterior.is_ccw` property tells you if the polygon is CCW.  If it is CW, the outward
normal formula should use `atan2(dx, -dy)` instead of `atan2(-dx, dy)`.

### Step 3 — Debug edge normals directly
```python
# Run this on the cluster A belly polygon
bcoords = np.array(bpoly.exterior.coords)[:-1, :2]
bnext = np.roll(bcoords, -1, axis=0)
bedges = bnext - bcoords
bnormals = np.arctan2(-bedges[:,0], bedges[:,1])  # try both signs
print("edge normal range:", np.degrees(bnormals.min()), "to", np.degrees(bnormals.max()))
# Plot bnormals histogram to see coverage
```

### Step 4 — Once winding is confirmed, try the gap-start approach again
The algorithm is correct in principle:
1. Sort edge normals
2. Find the largest gap (= flat face, ~62°)
3. Start distributing n_col targets from just after the gap end
4. Map each target to the nearest belly edge by normal angle
5. Use that edge midpoint as belly phi

### Step 5 — Fall back to uniform phi for circular bellies
For rows where the belly is nearly circular (max_gap < 30° or so), keep uniform phi
to avoid regressions on the sphere.

---

## Test Command
```bash
python src/scripts/test-leaf-placement.py 2>&1 | grep -E "^\[|✗|✓|Total|rows:"
```

Expected clean state (before phi work):
```
[PASS] Object 1 — sphere r=10   ✓ No artifacts detected
[FAIL] Object 2 — cluster A (0° tilt)   ✗ LONG ROOTS, ✗ FLOATING LEAVES (no duplicates)
[FAIL] Object 3 — cluster B (30° tilt)  ✗ LONG ROOTS, ✗ FLOATING, ✗ UPWARD TANGENTS
[FAIL] Object 4 — cluster C (58° tilt)  ✗ 1 SAME-ROW DUPLICATE, ✗ LONG ROOTS, etc.
Total issues: 10 (if phi work reverted cleanly)
```

---

## Key Files

| File | Role |
|---|---|
| `src/dharmatiles/trees/placement.py` | `place_leaves_on_mesh` — phi angle computation is at ~line 485 in the `for poly in path2d.polygons_full:` loop, after `n_col` is computed |
| `src/scripts/test-leaf-placement.py` | `_same_row_duplicate_indices()`, `_mark_error_leaves()`, `_check_artifacts()` |
| `stl/test/leaf-placement-test.stl` | Output STL (not regenerated this session) |
