# Foliage Cluster Bald Spots — Investigation Log
*2026-06-24 — issue NOT resolved at time of writing*

## Symptom

Foliage clusters (the leaf-covered ball-shapes on terminal branches) have visible
dark/bare patches on their upper surfaces when the tree is viewed from above or
from a slightly elevated angle.  The bare icosphere skin shows through in
irregularly-shaped dark patches, mostly concentrated toward the top of the
overall canopy.

Render evidence: image captured 2026-06-24 (`png/water/1x1-grass-tree+water.png`)
shows pronounced bald spots across the upper canopy.

---

## What We Know

### Cluster geometry recap

Each foliage cluster is a deformed icosphere built from three sections:

1. **Back hemisphere** — radius `r_wood` (the wood-tube radius), behind `start_pos`
2. **Cone body** — tapers from `r_wood` to `r_tip` along the Bezier spine from
   `start_pos` to `tip_pos`
3. **Forward dome** — hemisphere of radius `r_tip` forward of `tip_pos`, shifted
   perpendicularly upward by `dome_shift = r_tip - r_wood - _FOLIAGE_MAX_NOISE_MM`

The dome center is NOT at `tip_pos`; it is offset by `dome_shift * pu_tip` where
`pu_tip` = world-up projected perpendicular to `tip_t`.

### Leaf placement: Z-slice algorithm

Leaves are placed by slicing the smooth cluster mesh (`shaped`) with horizontal planes
at regular vertical steps:

```
row_step = leaf_length_mm * (1 - leaf_v_overlap)   # e.g. 4.5 * 0.75 = 3.375 mm
```

For each horizontal slice, every perimeter sample becomes a candidate leaf.  Candidates
are rejected if:

- `outward[2] < -0.1`  — outward direction (centroid→point) points downward; this
  skips the cluster underside hidden by the branch.
- `contact_angle >= π/2`  — computed from `local_r` (distance from slice centroid to
  perimeter point).  Near the world-Z apex `local_r → 0`, so this fires and rejects.
- `cluster_radius_mm < 1.0`  — early-exit inside `_emit_leaf`; triggers when
  `local_r < 1.0 mm`.

### World-Z apex vs. branch-direction apex

This is the key distinction that explains the current bald spots.

**Branch-direction apex** = vertex most aligned with `tip_t` (the branch tip tangent).
Historically the apex cap targeted this vertex.

**World-Z apex** = vertex with maximum Z coordinate = actual top of cluster in gravity.

For clusters on **tilted branches**, these are *different vertices*, often separated
by 4–6 mm in Z.  Diagnostic output from 2026-06-24:

```
cluster 17: branch_apex_z=35.40  wz_apex_z=40.00  (gap = 4.6 mm)
cluster 60: branch_apex_z=35.76  wz_apex_z=40.29  (gap = 4.5 mm)
cluster 25: branch_apex_z=35.82  wz_apex_z=40.46  (gap = 4.6 mm)
```

For **nearly-vertical** branches (`tip_t_z ≈ 1`), the two coincide:

```
cluster 11: branch_apex_z=48.11  wz_apex_z=48.34  (gap = 0.23 mm)
cluster 95: branch_apex_z=48.49  wz_apex_z=49.04  (gap = 0.55 mm)
```

### Where the Z-slice rows land (cluster 17 example)

```
z_range = [26.2, 40.0],  row_step ≈ 3.375 mm

z=39.67  placed=  0  rej_out=  0  rej_r=  0   ← section too tiny or None
z=36.29  placed= 12  ✓ last successful row
z=32.92  placed= 13  ✓
z=29.54  placed=  0  rej_out=9  ← outward check filters underside
z=26.17  placed=  0  ← back hemisphere / no section
```

The gap between the last placed row (z=36.29) and wz_apex (z=40.0) is **3.7 mm** —
nearly one full row_step — and has zero leaf coverage.

### The apex-cap timeline (commit history)

Multiple rounds of apex-cap work have been done, each fixing one thing while
re-introducing or missing another:

| Commit | What changed | What it broke / missed |
|---|---|---|
| `de6dca5` | First apex cap (flat bottom row + r_tip contact angle) | |
| `25073f0` | Leaf-count diagnostic; apex cap v2 | |
| `69238c8` | Improve top coverage | |
| `31e5ff4` | Updated leaf cap mesh | |
| `afbe43e` | Replaced ad-hoc cap with Z-slice algorithm | Apex gap not fully understood |
| `93990c7` | Fixed bald apex and blade-on-edge leaves | |
| `00f3b92` | Fixed bald apex, improved contact-angle accuracy | Changed cap target to `argmax(dot(tip_t))` — **wrong axis** |
| `605ae36` | Added upward-pointing leaf warning; added `outward[2] < -0.1` underside filter | |
| today | Changed cap target to `argmax(z)` (world-Z apex) | **not yet verified as fixed** |

### Why `argmax(dot(tip_t))` was wrong

The previous apex cap found the vertex most aligned with the branch tip direction.
For tilted branches, this vertex is *below* the world-Z maximum by several mm.
Z-slices *do* work fine at that height (local_r is normal-sized there), so the
branch-apex cap was doubling up on covered area while leaving the actual gravity-top
uncovered.

### The `outward[2] < -0.1` filter (added 2026-06-24, commit 605ae36)

Added to prevent upward-pointing leaves on the cluster underside.  The filter
is logically correct — `outward = (pt3d - mesh_center_3d) / |...|` points
downward on the underside — but it interacts with the apex coverage problem:
lower-half Z-slice rows are now completely filtered, leaving only 2–3 rows per
cluster placing leaves.  For a typical cluster spanning 14 mm in Z with row_step
3.375 mm that's only rows at the top ~7 mm, which is fine if the apex cap correctly
caps the top.  The filter is not the root cause of bald spots.

---

## Current State (2026-06-24 end of session)

Applied fix: changed the apex cap vertex from `argmax(dot(tip_t))` to `argmax(z)`.
A render was produced showing good top coverage, but **the fix has not been
battle-tested**:

- Only one tile rendered.
- No comparison across different `canopy_radius_mm`, `branch_target`,
  `foliage_cluster_length_mm` parameter combinations.
- The `outward[2] < -0.1` filter may still hide lower-body coverage gaps that
  aren't visible from a naïve top-down render.
- The `gap_mm = leaf_width * 0.5` offset (≈15.8° from apex) may still leave a
  tiny uncovered ring right at the world-Z apex.  At r_tip=5.5 this is a cap of
  ~0.2 mm height, probably sub-visual at print scale.

---

## Open Questions / Next Steps

1. **Verify the fix is durable** across more tiles and parameter settings.
2. **Check gap at apex**: with `gap_angle ≈ 16°`, leaves start 1.5 mm from the
   apex vertex.  At print scale (0.2 mm layers) this may be invisible, but
   worth rendering from directly overhead.
3. **Lower-body coverage**: only 2–3 Z-slice rows place leaves per cluster.
   Rows in the lower half are killed by `outward[2] < -0.1`.  For short
   cluster lengths this may mean sparse coverage on the sides.  Consider
   whether the filter threshold (-0.1) is too aggressive for lower-side
   surfaces that should have leaves (they are not truly "underside").
4. **`_FOLIAGE_MAX_NOISE_MM` and `dome_shift`**: for short clusters
   (`r_base ≈ r_tip`), `dome_shift ≈ 0` and the whole cluster sits very
   close to the branch centerline.  The world-Z apex and branch apex may
   again diverge in unexpected ways.
5. **Diagnostic scaffolding to keep**: the per-row diagnostic code was useful;
   consider adding it behind an env-var flag rather than removing it entirely.
