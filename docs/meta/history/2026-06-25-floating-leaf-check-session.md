# Floating-Leaf Check — Session Log
*2026-06-25 — status: check partially implemented, threshold question open*

---

## Context

Continuing the meridian-arc leaf-placement work from the previous session.  The
arc-based top-anchor fix and the `ca=0` clamp for apex rows were already in place
(see `2026-06-24-foliage-cluster-baldness.md` and conversation summary).  The STL
`stl/test/leaf-placement-test.stl` was already generated and being viewed in a
renderer.

---

## What We Did This Session

### 1. Added `leaf_max_dists` field and FLOATING LEAVES check to test script

`src/scripts/test-leaf-placement.py` now:

- After building each `surf` (leaf surface), calls
  `trimesh.proximity.closest_point(mesh, surf.vertices)` and stores
  `max(dists)` in `stats.leaf_max_dists`.
- Prints a per-object summary line:
  `leaf max-dist-to-mesh mm: median=X  p90=Y  max=Z  thresh=7.5  floating=N/M`
- Checks `max_dist > lift_mm + leaf_length (= 7.5 mm)` and emits FLOATING LEAVES
  artifact if any leaves exceed it.

Current state: **no leaves are flagged** at the 7.5 mm threshold.  All measured
`max_dist` values are ≤ 4.51 mm.

### 2. Discovered the base-vertex trap

`surf.vertices` always includes the base vertex at `pt3d` (on the mesh surface,
distance = 0) because `build_leaf_surface` keeps the rotation center (base) fixed
in world space.  So `min_dist` is always 0 and useless.  `max_dist` is the
meaningful metric for "how far does the leaf reach from the mesh?"

### 3. User pointed at the render — orange crown leaves at ≈45°

Image: `stl/test/leaf-placement-test.stl` rendered in viewer.  The orange (top
row) leaves of the vertical cluster (cluster A, row 7) are the ca=0 apex leaves.
They extend radially outward and upward at ~33° (the lift-rotation angle), with
the curl making the tip go even higher.  The user reported "bottom of the curl is
nowhere near the parent mesh surface."

### 4. Investigated which vertex == "bottom of curl"

Several vertex-selection strategies tried:

| Metric | Contact-angle leaves | ca=0 apex leaves | Result |
|---|---|---|---|
| `min_dist` (all verts) | 0.00 mm | 0.00 mm | always 0 — useless |
| `min_tipward_dist` (excl. base loop) | 0.00 mm | 0.29–0.37 mm | closest tipward to mesh, too close to base ring |
| `tip_dist` (farthest-from-base vert) | 0.55–3.98 mm | 3.54–4.08 mm | overlapping ranges — no clean threshold |
| `curl_region_dist` (dist-from-base > L/2) | 0.00–1.29 mm | 1.48–1.90 mm | **cleanest separation** |

**Key finding:** The `min_tipward_dist` (0.29–0.37 mm) is misleading — it finds the
first vertex just above the base boundary ring, not the visible curl.  The actual
curl region (vertices more than L/2 = 2.25 mm from the base) shows the gap clearly.

### 5. Why `curl_region_dist` is near 0 for contact-angle leaves

Contact-angle leaves are *embedded* in the cluster mesh: the contact-angle
geometry presses the leaf through the cluster surface between the base and the tip.
`trimesh.proximity.closest_point` returns unsigned distance, so a vertex *inside*
the mesh reads as ≈0 (very close to the surface from the inside).  This is
intentional: those leaves are correctly pressed against the sphere; `solidify_leaf`
then cuts the root.

### 6. State of the FLOATING LEAVES check

The check as committed uses `max_dist` (farthest vertex to mesh) with threshold
`lift_mm + leaf_length = 7.5 mm`.  This is too loose — it doesn't catch the ca=0
crown leaves (max ≈ 4 mm).

**The correct metric is `curl_region_dist` (dist-from-base > L/2), and the correct
threshold is approximately `lift_mm * 0.5 = 1.5 mm`.**

Separation between groups with that threshold:
- Contact-angle leaves: curl_region_dist ≤ 1.29 mm → PASS
- ca=0 crown leaves: curl_region_dist 1.48–1.90 mm → FAIL

This has only ~0.2 mm margin.  A slightly more principled threshold would be
`lift_mm * 0.5 + 0.2 mm = 1.7 mm` or the user may want to tune it.

An even tighter curl region (dist-from-base > L * 0.7 = 3.15 mm, isolating only
the final 30% of the leaf where the curl geometry lives) was not yet tested but
might give a cleaner signal.

---

## Files Changed This Session

| File | Change |
|---|---|
| `src/scripts/test-leaf-placement.py` | Added `leaf_max_dists` to `_PlacementStats`; proximity check after `solidify_leaf`; FLOATING LEAVES artifact check (#13); diagnostic summary line |

---

## Current Test Results (before next step)

```
[PASS] Object 1 — sphere r=10          (85 leaves, 5 rows)
[PASS] Object 2 — cluster A (0° tilt)  (32 leaves, 8 rows)
[FAIL] Object 3 — cluster B (30° tilt) (37 leaves, 9 rows)
         ✗ CROSS-ROW STACKING: 1 pair
         ✗ LONG ROOTS: 1/37
[FAIL] Object 4 — cluster C (58° tilt) (56 leaves, 8 rows)
         ✗ SPARSE PHI-SECTORS: 1/12 sectors
         ✗ CROSS-ROW STACKING: 10 pairs
         ✗ LONG ROOTS: 7/56
Total issues: 5  (FAIL)
```

The FLOATING LEAVES check is present but not firing (threshold too loose).

---

## Exact Next Step

Replace the current FLOATING LEAVES implementation (which uses `max_dist` with
threshold 7.5 mm) with one that:

1. For each placed leaf, identifies the **curl-region vertices**:
   `surf.vertices` where `‖v − pt3d‖ > L/2` AND not in the base boundary loop.
2. Computes `curl_region_dist = min(closest_point(mesh, curl_region_verts).dists)`.
3. Stores it in `stats.leaf_curl_region_dists` (rename from `leaf_max_dists`).
4. Flags FLOATING LEAVES if `curl_region_dist > lift_mm * 0.5` (or a
   user-tunable constant; ~1.5 mm for lift=3 mm).

Open question: whether `L/2` or `L * 0.7` is the better curl-region cutoff.
`L * 0.7` (final 30% of leaf) isolates the visually-curled geometry more precisely
but hasn't been measured yet.

---

## Geometry Facts Established

- `build_leaf_surface` applies `lift_mm` as a **rigid rotation** around `base_pos`
  (base stays fixed, tip rises by `L * sin(arctan(lift/L))` ≈ 2.5 mm for lift=3,
  L=4.5).
- `boundary_loop(surf)` returns the base-edge vertex indices of the leaf surface.
- For ca=0 leaves: `tip_dz_above_base` is **positive** (+1.5–2.4 mm).
  For contact-angle leaves: **negative** (−1.5 to −4.6 mm).
  That sign is the cleanest binary floating indicator but doesn't map directly
  to a distance-to-mesh check.
- The `_contact_angle_for_sphere` formula is accurate for constant-radius
  spheres; on tilted clusters' inner curves, tip_dist can reach 3.98 mm
  (slightly above lift_mm=3 mm) due to approximation error.

---

## Related Files / Docs

- `docs/design/leaf-placement.md` — algorithm spec
- `src/scripts/test-leaf-placement.py` — diagnostic test (all edits here)
- `src/dharmatiles/trees/mesh.py` — production placement (`_build_foliage_cluster_mesh`, `_emit_leaf`)
- `src/dharmatiles/trees/leaf.py` — `build_leaf_surface`, `boundary_loop`, `solidify_leaf`
- Previous session: `2026-06-24-foliage-cluster-baldness.md`
