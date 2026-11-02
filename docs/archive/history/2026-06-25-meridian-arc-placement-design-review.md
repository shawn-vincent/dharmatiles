# Meridian-Arc Leaf Placement — Design Review
*2026-06-25 — review of the proposed algorithm in `docs/design/leaf-placement.md`.
Covers edge cases, efficiency traps, elegance opportunities, and things not
visible from inside the conversation that produced the design.*

---

## Edge Cases

### 1. The world-Z apex might be on the CONE BODY, not the dome

The design doc pins the top row near `z_top = shaped.vertices[:, 2].max()`.  For
steeply-tilted branches, the world-Z maximum is NOT on the forward dome — it is on the
upper side of the cone body, which slopes upward steeply enough that its highest point
exceeds the dome's world-Z extent.

This matters for the top-anchor placement and for the contact angle formula.  The cone
body has zero Gaussian curvature (it is locally flat), so `_contact_angle_for_sphere`
with `local_r` = the cross-section radius OVER-estimates the contact angle there.  On a
flat surface, the correct contact angle is the angle needed to press the arched leaf
tip back to the flat plane, not to a sphere.

Pragmatic fix: test whether the `z_top` vertex is on the dome or cone by checking
whether it was produced in the `mn` (north/dome) branch of the cluster builder.  If
it is on the cone, reduce the contact angle slightly.  Or just document that the
over-estimate (ca slightly too large → leaf slightly more tilted) is tolerable.

### 2. Very short clusters (cluster_length ≈ 0)

When `foliage_cluster_length_mm` is very small or the branch is nearly a point,
the Bézier spine degenerates to a point, `spine_d → 0`, and `cone_arc_p → r_tip −
r_base` (just the radius change, no length).  The meridian polylines are all nearly
vertical.  The arc-to-Z mapping is nearly linear and the algorithm still works.

But: the bottom anchor is at arc = `leaf_length_mm` from the cluster bottom.  If the
entire cluster arc (bottom to top) is shorter than `leaf_length_mm`, the bottom anchor
exceeds the cluster height and `avg_z_for_arc` returns a clamped (incorrect) value.

Guard needed:
```python
if total_cluster_arc < leaf_length_mm:
    # cluster too small for any leaves — skip placement entirely
    return
```

This is already approximately handled by `cluster_radius_mm < 1.0` early-exit in
`_emit_leaf`, but an explicit arc check is cleaner.

### 3. Non-convex cross-section polygons and ambiguous azimuthal intersection

The shaped mesh (smooth cluster) is convex in most configurations.  But for tight
Bézier bends (branch loops back on itself), the back hemisphere can protrude through
the cone body's XY footprint, producing a non-convex or even disconnected cross-section
polygon at certain Z levels.

The meridian sampling step — "find the perimeter point at azimuthal angle φᵢ" — is
ambiguous on a non-convex polygon: a ray from the centroid at angle φᵢ may intersect
the perimeter at two or more points.

Fix: always take the **outermost** intersection (maximum r from section centroid).
This is the physical cluster surface.  Inner intersections are geometric artefacts of
the non-convex shape.

### 4. Meridian at the φ=0 seam (angular wraparound)

The azimuthal interpolation wraps at 2π.  A leaf whose φ_leaf is slightly less than 0
(i.e. −0.01 rad) should bracket between meridian N-1 and meridian 0, not between
meridian −1 (undefined) and meridian 0.

Fix: normalize phi_leaf to [0, 2π) before lookup.  Use modular arithmetic for the
bracket index: `i = int(phi_leaf / (2π / N)) % N`.

### 5. Z-level outside meridian range

The meridian curves cover `z_bottom` to `z_top` of the shaped mesh.  The row anchor
positions are computed from these bounds.  But if a cross-section at a row Z returns
`None` (trimesh finds no intersection — this can happen if the Z level is exactly at a
vertex's Z coordinate, causing a degenerate plane-vertex touch), the row is silently
skipped.

The current code already handles this with `if section is None: continue`.  With the
meridian algorithm, a skipped top-anchor row would leave the apex uncovered — the very
problem we're solving.  Add a fallback: if the top-anchor section is None, try
`z_top_anchor − 0.1 mm` and `z_top_anchor − 0.2 mm` before giving up.

### 6. r_wood > r_foliage (inverted cone)

This should not happen by construction (`foliage_cluster_radius_mm` is the TIP radius
and is always set larger than the wood tube radius in practice), but there is no
explicit guard.  If it did happen, the cone body would NARROW from base to tip, and the
`factor_tip` computation:

```python
factor_tip = max(0.0, 1.0 − (r_base + _FOLIAGE_MAX_NOISE_MM) / max(r_tip, 1e-6))
```

…would be negative, clamped to 0, producing a cylinder instead of a narrowing cone.
The meridian algorithm would produce outward normals pointing slightly INWARD on the
narrowing section (the outward normal flips sign when `dr/ds < 0`).

Guard: `assert r_foliage >= r_wood` in `_build_foliage_cluster_mesh`, or document
the constraint clearly.

### 7. Nearly-vertical branch where pu_tip ≈ 0

The perpendicular-upward offset uses:
```python
pu_tip = _pu_unit_scalar(tip_t)   # world-up perp to tip_t
```

When `tip_t = [0, 0, 1]` (perfectly vertical), `pu_tip` is the zero vector.  The
current code divides by `max(n, 1e-6)`, returning zero — no upward shift.  For a
perfectly-vertical branch the cluster is symmetric and the upward shift is unnecessary,
so this is correct behavior.

But the meridian algorithm computes the cluster XY centroid from `shaped.vertices.mean()`.
For a zero-shift cluster, this centroid is on the branch axis.  The azimuthal angles
φᵢ then fan out from this axis symmetrically — correct.  No issue.

### 8. Leaf lift interacting with the top-anchor row

The top-anchor row places leaves just below the world-Z apex.  Each leaf has
`leaf_lift_mm > 0` (default 3.0 mm), which rotates the leaf upward relative to the
contact-angle position.  On the top-anchor row, the lift may push leaf tips ABOVE the
cluster apex, producing tiny spikes that protrude beyond the cluster's silhouette.

This is the same reason the apex cap used `lift_mm = 0` in the old algorithm.  For the
top-anchor row specifically, consider reducing `lift_mm` — possibly to 0, or to a
fraction of the full value, ramped by proximity to the apex.  A simple approach:

```python
proximity_to_apex = max(0.0, 1.0 − (z_top − z_row) / leaf_length_mm)
eff_lift = leaf_lift_mm * (1.0 − proximity_to_apex)
```

---

## Efficiency Traps

### 1. `shaped.section()` called O(Z_samples) times for meridians

The meridian-building step samples `leaf_arc_z_samples` (default 64) Z levels.  Each
call to `shaped.section()` triggers a BVH query against the shaped mesh (1280 faces at
ico_subdiv=3).  The per-call cost is small but not zero.

For the current Z-slice algorithm the loop calls `section()` roughly 4–8 times per
cluster.  The meridian algorithm adds 64 more calls for the arc computation.  For a
tree with 50 clusters that is 50 × 64 = 3200 section calls, each hitting a 1280-face
BVH.

**Better**: compute all 64 cross-sections in a single vectorized pass by intersecting
the mesh with 64 parallel planes at once.  `trimesh.intersections.mesh_multiplane` (or
equivalent) can do this in O(faces + N_planes) rather than O(N_planes × BVH_cost).
If this isn't available directly, pre-sort the mesh faces by Z extent and sweep once.

Even simpler: the cluster has a known ANALYTIC parametric form (south cap + cone body +
north dome).  The arc length and surface normal at any (Z, φ) can be computed
analytically without any mesh query at all.  For the smooth mesh (which is what the
meridian algorithm uses), this would replace all 64 section calls with closed-form
evaluation.  Noise is handled separately at placement time (same as current), not at
meridian-building time.

### 2. `np.interp` called once per leaf per meridian interpolation

The per-leaf normal interpolation calls `meridian.normal_at(z_row)` for two meridians
per leaf.  If `normal_at` is implemented as `np.interp(z_row, z_vals, normals)`, it
is O(log Z_samples) per call (binary search).  With 100 leaves per cluster and 2
meridians per leaf, that is 200 log(64) ≈ 1200 operations — negligible.

But if it is accidentally implemented as a Python loop over `z_vals`, it becomes O(200
× 64) = O(12800) iterations.  **Use `np.interp`.**

### 3. `solidify_leaf` raycast dominates — do not add more geometry

The dominant cost per leaf is the multi-ray BVH raycast in `solidify_leaf` (each of
~20 perimeter vertices fires a ray into the cluster mesh).  This does not change with
the meridian algorithm.

Do not be tempted to pre-build a higher-resolution cluster mesh for "better raycasting."
The 1280-face ico mesh is already adequate for embedding.  Adding meridian-arc
computation is free in comparison to the raycast cost.

### 4. Meridian data structure: avoid per-leaf object allocations

The six meridians need to be accessible during the placement loop.  If the meridian
data is stored as six separate Python objects with per-call method invocations, the
interpreter overhead for 100 leaves × 6 meridians = 600 calls adds up.

**Better**: store the meridians as pre-interpolated NumPy arrays indexed by Z level.
Pre-interpolate each meridian's normal onto the same fine Z grid, so the placement loop
does only array indexing + weighted sum, no per-leaf `np.interp` calls.

```python
# Shape: (N_meridians, N_z_fine, 3) — normals pre-interpolated onto a common Z grid
meridian_normals = np.stack([...], axis=0)
phi_meridians    = np.linspace(0, 2*np.pi, N, endpoint=False)
```

Lookup becomes a pure NumPy operation:
```python
i = int(phi_leaf / (2*np.pi / N)) % N
j = np.searchsorted(z_grid, z_row)
w = (phi_leaf - phi_meridians[i]) / (2*np.pi / N)
up_hint = normalize((1-w) * meridian_normals[i, j] + w * meridian_normals[(i+1)%N, j])
```

### 5. Re-running arc-to-Z average for each row Z query

If `avg_z_for_arc(s, meridians)` is called N_rows times and each call does
`np.interp` × N_meridians, the total is O(N_rows × N_meridians × log(Z_samples)).
For typical values (5 rows × 6 meridians × log(64)) this is trivial.

But if N_rows were to grow large (e.g. a cluster with 20+ rows for very small leaves),
pre-computing the averaged arc-to-Z curve once and doing a single `np.interp` per row
is cleaner and faster.

---

## Elegance Opportunities

### 1. Bottom anchor from first placeable Z, not world-Z bottom

The original design anchored the bottom row at arc distance = `leaf_length_mm` from
the mesh's world-Z bottom.  But the world-Z bottom of the mesh may include surface
area where all leaves are rejected by the underside filter (`up_hint[2] < -0.1`).
The bottom anchor lands in this dead zone — the row exists but places zero leaves —
and the first actual placed row ends up one row-step higher than intended.

The meridian data itself provides the correct reference: find the lowest Z level
where the averaged meridian normal crosses the upward-facing threshold, then anchor
one leaf-length of arc above that.  No knowledge of the mesh's internal structure
is needed.

```python
z_placeable = _lowest_placeable_z(meridians, normal_z_threshold=-0.1)
s_placeable = avg_arc_for_z(z_placeable, meridians)
z_bot_anchor = avg_z_for_arc(s_placeable + leaf_length_mm, meridians)
```

This is a pure function of the meridian data and works for any mesh geometry.

*Note: an earlier version of this review described the fix in terms of the foliage
cluster's "back hemisphere seam."  That framing was wrong — the algorithm has no
knowledge of the mesh's internal construction.  The correct fix is the general one
above, derived entirely from the meridian normals.*

### 2. The `leaf_cap_count` parameter can be removed entirely

The apex cap is deprecated.  The parameter `leaf_cap_count` should be:
- Set to 0 by default, immediately, to prevent the old cap from firing.
- Removed from `Tree` layer parameters entirely after the meridian algorithm is
  confirmed working.
- Its slot in the parameter table should say `(removed — use leaf_arc_meridians)`.

Keep the deprecation note in the design doc and in a `# DEPRECATED` comment in the
code for one release cycle, then delete.

### 3. Meridian normals subsume `outward` throughout `_emit_leaf`

Currently `_emit_leaf` takes `radial` (the centroid-to-point outward direction) and
uses it as `up_hint` for contact angle computation AND for noise-sinking:

```python
disp_base = gaussian_noise(base_smooth) + coarse_noise(base_smooth)
base_pos = base_smooth + up_hint * (disp_base - noise_peak)
```

The noise sink moves the base along `up_hint`.  With the meridian normal as `up_hint`,
this sinking direction is now the actual surface normal (not the centroid approximation).
This improves noise sinking on the cone flanks where the centroid direction is
significantly off.

This means the `radial` parameter to `_emit_leaf` can be eliminated; the function
receives `up_hint` directly from the meridian interpolation.  Fewer parameters, cleaner
interface.

### 4. Arc-averaging and row-placement can be pure functions with no mesh dependency

The functions:
- `_build_meridian(shaped, phi, z_samples)` → `(z_vals, arc_vals, normal_vals)`
- `_compute_row_z_positions(meridians, leaf_length, v_overlap, z_top)` → `[z_row, ...]`

…have no dependencies on `leaf_length_mm`, `leaf_width_mm`, leaf geometry, or the
contact angle.  They depend ONLY on the shaped mesh and the arc parameters.  This means
they can be computed once per cluster and reused across all leaf parameter variations
during visual tuning.

If the mesh and arc parameters don't change between two tuning runs, the meridians don't
need to be recomputed.  This is a natural cache boundary.

### 5. The row-count formula gives the natural parameterization

```python
N_gaps = round(inner_arc / row_step_target)
actual_overlap = 1.0 − (inner_arc / N_gaps) / leaf_length_mm
```

This is a much more honest number than `leaf_v_overlap`.  The user specifies a target;
the algorithm delivers the closest integer-feasible overlap.  Log `actual_overlap` in
the debug output:

```
[LEAF] cluster 17: target_overlap=0.50 actual_overlap=0.497 N_rows=5
```

This immediately makes it clear when a cluster is "too small to fit the requested
overlap" (actual deviates significantly from target) vs. "good fit."

### 6. The two anchor positions define the contact-angle RANGE, not a single value

At the bottom anchor (cone body, lower area), `local_r` is small (narrow cone base)
and the contact angle is moderate.  At the top anchor (dome apex, small cross-section),
`local_r` is large (r_tip), and the contact angle is the smallest (large sphere →
leaves lie more flat).

This gradient — contact angle decreasing from bottom to top — means leaves near the
base tilt more aggressively and leaves near the apex lie more flat.  This is
aesthetically desirable (it mirrors how leaves grow on real spherical canopies: more
drooping at the bottom, more flat at the top).  Worth noting in the design doc as an
emergent aesthetic property.

---

## Things Not Visible From Inside the Conversation

### 1. The leaf tip pointing UPWARD warning may fire more on the cone body

The `tangent[2] > 0.707` RuntimeWarning catches leaves whose growth direction points
strongly upward.  With the meridian normal replacing centroid-to-point, leaves on the
cone body's steep flanks get a more upward-tilted `up_hint`.  The contact-angle formula
then produces a `tangent` that is rotated further from vertical.

If the cone body is steep (branch nearly horizontal) AND the leaf is long relative to
the local cross-section radius, the contact angle can approach π/2 and the leaf
tangent flips toward +Z.  The warning guard (`ca >= π/2: skip`) catches this, but the
transition is abrupt: a leaf is placed, then the next leaf 0.1 mm further down is
skipped with no visual continuity.

Smoother fix: instead of a binary skip at `ca >= π/2`, fade `eff_leaf_length` as
`ca → π/2` (shorter leaves can tolerate steeper surfaces).  This gives a graceful
degradation rather than a coverage cliff.

### 2. Meridian arc averaging masks a real quantity: coverage width variance

The average arc-to-Z mapping throws away information about HOW MUCH the meridians
diverge from each other.  A cluster where meridians differ by 2% is well-approximated
by the average; one where they differ by 40% (a nearly-horizontal branch) has rows
that are a poor fit for some azimuths.

Consider emitting the per-cluster variance:
```
[LEAF] cluster 17: arc_variance = 0.32  (meridians spread ±16% of mean arc)
```

High variance (> 0.5) is a signal that the averaged Z positions are a poor fit for
some meridians.  In that case, switch to per-azimuth row placement: instead of one
set of row Z values for all azimuths, compute separate sets for each meridian and
interpolate the row Z position per leaf as well.  This is a more expensive but more
correct approach for extreme tilt cases.

### 3. The `solidify_leaf` raycast fires into the NOISED cluster mesh, not the smooth one

The noised mesh has ±1 mm coarse noise superimposed.  The raycast from each leaf
perimeter vertex along its `ray_dir` into the noised mesh can return multiple hits
if the noise creates deep concavities.  The current code takes the NEAREST hit within
`_LEAF_ROOT_MAX_HIT_MM = 10.0 mm`.

With the meridian algorithm, `up_hint` is from the SMOOTH mesh.  The raycasting is
into the NOISED mesh.  At a deep noise valley, the ray might hit the far wall of the
valley rather than the near wall, embedding the root TOO DEEP.  The
`_LEAF_ROOT_MAX_HIT_MM` guard partially protects against this (hits > 10 mm are
discarded), but a hit at, say, 3 mm into a noise valley might still be "valid" and
produce a badly-embedded root.

Consider filtering hits by comparing the hit normal to the expected normal:
```python
# Reject hits where the mesh normal at the hit point is pointing the wrong way
if dot(hit_normal, ray_dir) > -0.2:
    continue   # hit the back wall of a noise concavity — skip
```

### 4. The column spacing `n_col = ceil(perimeter / col_step)` varies per row

For the top anchor row (small cross-section, small perimeter), `n_col` may be 2 or 3.
For a mid-cluster row (large perimeter), `n_col` may be 15–20.  This variation is
correct but means the placement is NOT uniform in leaf density per unit area — rows
near the top have fewer leaves per mm² of surface than rows near the bottom (larger
cross-section = more leaves per row but same row step = similar density).

Actually this IS correct for tile printing: the goal is uniform VISUAL density when
viewed from above, and a larger cross-section at a given Z level corresponds to a
larger amount of visible surface area at that Z.  But it means the total leaf count
varies significantly with cluster shape, and a `leaf_base_count` "guaranteed minimum
per row" might be useful for very small top-anchor cross-sections that produce only
2–3 leaves.

### 5. The `_ca_cache` is local to `_build_foliage_cluster_mesh` — it resets per cluster

The contact-angle cache is currently initialized fresh for each cluster:
```python
_ca_cache: dict[tuple, float] = {}
```

For a tree with 50 clusters all using the same leaf geometry and similar `local_r`
values, this means each cluster recomputes the same set of contact angles.  If
`local_r` values cluster tightly (they do — all clusters have `r_tip` in roughly the
same range), the cache is recomputed ~50 times with ~10–20 entries each.

The analytical `_contact_angle_for_sphere` is cheap (one `compute_leaf_geometry` call
+ arithmetic), but for large trees it adds up.  Elevate `_ca_cache` to module level,
keyed by `(local_r, leaf_geometry_tuple)`.  Across a full tree the cache hits
dramatically.  This is currently a TODO but becomes more important once the meridian
algorithm produces per-leaf `local_r` values that vary more continuously.

### 6. There is no test for the averaging correctness

The correctness spec (from the systematic-algorithm-development protocol) says:
> "Every cluster must have at least one leaf within `leaf_length_mm` of the world-Z apex vertex."

The meridian-arc algorithm should satisfy this by construction (top anchor is pinned
within 0.25 × `leaf_length_mm` of the apex).  But the `avg_z_for_arc` function is
a numerical inversion that can fail if the meridian polylines have coarse resolution
(few Z_samples) or if the shaped mesh produces degenerate cross-sections at certain Z
values.

Add a post-placement assertion in debug mode:
```python
if _DEBUG_LEAVES:
    leaf_z_values = [leaf.base_pos[2] for leaf in placed_leaves]
    gap_to_apex = z_top - max(leaf_z_values, default=z_bottom)
    if gap_to_apex > leaf_length_mm:
        print(f"[LEAF] FAIL cluster {i}: apex gap = {gap_to_apex:.2f} mm > {leaf_length_mm:.2f}")
    else:
        print(f"[LEAF] PASS cluster {i}: apex gap = {gap_to_apex:.2f} mm")
```

This makes the meridian algorithm provably compliant with the correctness spec and
catches numerical failures in the arc inversion before they reach the user.
