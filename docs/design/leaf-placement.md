# Leaf Placement — Specification
*Updated 2026-06-25.  Prior algorithm (Z-slice with uniform dZ) is documented below as
"Current Algorithm (Deprecated)".  The new design is "Meridian-Arc Algorithm (Proposed)".*

*Full history: `docs/meta/history/2026-06-24-leaf-rendering-deep-history.md`,
`docs/meta/history/2026-06-24-foliage-cluster-baldness.md`,
`docs/meta/history/2026-06-25-meridian-arc-placement-design-review.md`.*

---

## What This Covers

How individual leaves are placed on foliage clusters in the tree generator.
Foliage clusters are the bumpy green blobs at the tip of each terminal branch.
Leaves are separate 3D geometry (watertight solids) sitting on the cluster surface.

Implementation files:
- `src/dharmatiles/trees/leaf.py` — leaf geometry primitives
- `src/dharmatiles/trees/mesh.py` — placement algorithm (`_build_foliage_cluster_mesh`,
  `_emit_leaf`, `_contact_angle_for_sphere`)

---

## Foliage Cluster Shape

The cluster is a deformed icosphere built from three sections:

1. **Back hemisphere** (radius `r_wood`, behind `start_pos`) — round cap hiding the
   entry stub where the branch tube enters the cluster.
2. **Cone body** (`r_wood` → `r_foliage`, swept along the Bézier spine `start_pos` →
   `tip_pos`) — tapered tube following the branch direction.  Each cross-section ring
   is offset perpendicular-upward by a smoothly-varying fraction of its radius, so the
   branch runs along the cluster bottom without protruding through the skin.
3. **Forward dome** (radius `r_foliage`, ahead of `tip_pos`) — rounded nose, offset
   upward so the branch bottom aligns with the dome equator.

Noise is applied after geometry: fine Gaussian (σ=0.05 mm) for grain, coarse smooth
(4 mm cell, ±1 mm) for silhouette variation.  Both layers are shifted inward (subtract
peak) so the smooth envelope is the outer bound and the branch always stays buried.

The cluster is **NOT axially symmetric in world space**:
- The cone body follows a curved Bézier spine, not a straight axis.
- The perpendicular-upward offset is different on each side (depends on the dot product
  of the ring normal with the world-up direction).
- For tilted branches the world-Z cross-sections are ellipses, not circles; the longer
  axis of the ellipse lies in the plane containing the branch and world-up.

This asymmetry is the fundamental reason a single global "outward direction" or a
single per-Z-level surface slope is insufficient for accurate leaf placement.

The meridian-arc placement algorithm has **no knowledge of this internal structure**.
It treats the shaped mesh as an opaque closed surface and derives everything it needs
— row positions, surface normals, local radii — directly from horizontal cross-sections
and meridian curves.  The algorithm would work identically on a pumpkin, a teardrop,
a sideways egg, or any other mesh satisfying the constraints described in the next
section.

---

## Leaf Geometry

A leaf is an ovate, keeled blade:

- **Outline**: teardrop, peak width at ≈ 1/3 from base, pointed tip, rounded base.
- **Cross-section**: quartic Bézier dome rising from the midrib crease.
- **Crease**: narrow tanh V-fold at the midrib.
- **Walls**: `solidify_leaf()` adds walls from the perimeter down to the cluster
  surface via raycast, plus a root cap.  The result is a watertight closed solid.

The keel (a V-ridge on the underside) exists in `build_leaf_mesh()` but is **not used**
by the foliage placement path.  The foliage path uses `build_leaf_surface()` +
`solidify_leaf()` only.

### Leaf parameters (all on `Tree` layer)

| Parameter | Default | Effect |
|---|---|---|
| `leaf_length_mm` | 4.5 | Leaf length, base to tip |
| `leaf_width_mm` | 3.0 | Peak width at ≈ 1/3 from base |
| `leaf_thickness_mm` | 0.24 | Dome height at peak |
| `leaf_fold_angle_deg` | 6.0 | Midrib crease V-angle |
| `leaf_inner_curve` | 1.5 | Crease-side Bézier shoulder |
| `leaf_outer_curve` | 0.72 | Edge-side Bézier shoulder |
| `leaf_curl_deg` | 40.0 | Concave tip curl (tip curves under) |
| `leaf_lift_mm` | 3.0 | Additional tip lift above contact-angle position |
| `leaf_h_overlap` | 0.2 | Fraction of leaf width that overlaps adjacent columns |
| `leaf_v_overlap` | 0.5 | Fraction of leaf length that overlaps adjacent rows |
| `leaf_arc_meridians` | 6 | **New.** Number of meridian curves for arc-length computation |
| `leaf_arc_z_samples` | 64 | **New.** Number of fine Z levels for meridian sampling |
| `leaf_angle_jitter_deg` | 24.0 | Yaw jitter range (currently disabled: `jit=0`) |
| `leaf_pos_jitter` | 0.165 | Position jitter fraction (currently disabled: `pj=0`) |
| `leaf_cap_count` | 12 | **Deprecated by meridian-arc algorithm.** Apex cap leaves |
| `leaf_base_count` | 5 | Not used in current paths (reserved) |

---

## Contact Angle

### What It Is

The contact angle is the rotation about the lateral axis T (perpendicular to both the
leaf growth direction and the cluster outward normal) that presses the leaf tip just
against the cluster surface.

Without the contact angle, the leaf lies flat on the surface with its arch curving the
tip away from the surface.  The `solidify_leaf` wall-embedding raycast needs perimeter
vertices to be close to the surface and just above it; without contact-angle tilt, many
raycasts miss the mesh and produce stub walls.

### Frame After Contact-Angle Rotation

```
up_hint  = outward surface normal at the attachment point (see meridian section below)
T0       = gravity-down projected onto the tangent plane (with yaw jitter)
ca       = contact angle (radians)

tangent   = T0 * cos(ca) − up_hint * sin(ca)
up_placed = up_hint * cos(ca) + T0 * sin(ca)
```

`build_leaf_surface(base_pos, tangent, up_placed, ...)` builds the leaf in this frame.

### Analytical Computation (`_contact_angle_for_sphere`)

The attachment surface is locally approximated as a sphere of radius `local_r`
(the cross-section radius at the leaf's Z level and azimuthal position — derived
from the meridian cross-section, not a global fixed value).

The contact angle satisfies the tip-touching constraint:

```
|tip(ca) − cluster_center|² = local_r²
```

After expanding (T_comp ≈ 0 by bilateral leaf symmetry):

```
contact_angle = arccos(−D / (2·local_r)) − arctan(L_comp / N_comp)
```

where D = |v_tip − base| (tip displacement at ca=0, from `compute_leaf_geometry`),
and L_comp / N_comp are its projections onto the leaf longitudinal (L) and normal (N)
axes.

The computation is cached per `(local_r, leaf_geometry_params)`.

### Guard

If `contact_angle >= π/2`, the leaf tangent points into the cluster surface.
This happens when the cluster cross-section radius is smaller than the leaf geometry
allows (`D > 2·local_r`).  Such leaves are skipped.

### What the Meridian Changes

Previously, `up_hint` was computed as `normalize(pt3d − mesh_center_3d)` — the
direction from the 3D centroid of the shaped mesh to the attachment point.

This approximation is:
- Correct on a true sphere (centroid IS the sphere center).
- Wrong on the **cone body**: the centroid-to-point direction is not perpendicular to
  the cone surface.  It is biased toward the centroid, which lies along the spine, not
  along the cone's own axis.
- Roughly correct on the **dome**, where the dome center is approximately at
  `tip_pos + dome_shift * pu_tip`, which differs from `mesh_center_3d`.

The meridian algorithm computes `up_hint` from the actual surface tangent at each
attachment point (see below), giving geometrically exact normals on both the cone body
and the dome.

---

## Meridian-Arc Algorithm (Proposed)

### Algorithm Scope — Works on Any Closed Mesh

The meridian-arc algorithm is a **general surface-tiling algorithm**.  Its only input
is a closed mesh and a set of leaf geometry parameters.  It knows nothing about how
the mesh was constructed.

**Required mesh properties:**

1. **Closed (watertight).** Every edge is shared by exactly two faces.  No boundary
   edges, no holes.  Required so that every horizontal cross-section produces a
   complete, unambiguous closed polygon.

2. **Simply-connected cross-sections.** Every horizontal plane that intersects the
   mesh should produce one or more closed, simply-connected (no internal holes)
   polygons.  The algorithm iterates over all polygons in a cross-section
   (`polygons_full`), so a mesh that produces multiple disjoint blobs at some Z
   levels is handled — each blob gets its own column of leaves.

3. **Centroid inside each cross-section polygon.** The azimuthal meridian sampling
   takes, for each angle φ, the outermost perimeter point in that direction from the
   section centroid.  This requires the centroid to lie inside the polygon.  Satisfied
   automatically by any convex mesh; satisfied in practice by any mesh without severe
   concavities.  For deeply non-convex shapes (C-shapes, toroids), a fallback
   strategy of taking the outermost of multiple intersections is needed.

4. **Non-degenerate Z extent.** `z_top − z_bottom > leaf_length_mm`.  A mesh shorter
   than one leaf length cannot accommodate even a single row.

**Not required:** spherical, axially symmetric, upright, or any particular orientation.
A cluster hanging upside-down, lying on its side, or shaped like a banana satisfies
these constraints and the algorithm places leaves on it correctly.

---

### Why Uniform dZ Fails

The cluster surface is not a vertical cone.  The three sections — back hemisphere,
cone body, forward dome — have very different surface-to-vertical-extent ratios:

| Surface | dZ per unit arc | Coverage per dZ step |
|---|---|---|
| Cone body (30° branch) | moderate | moderate — roughly as expected |
| Dome shoulder | small | large — rows compressed together |
| Dome top (near apex) | very small | very large — wide bare zone |

With a fixed `row_step = leaf_length × (1 − v_overlap)` in Z, each row step covers
increasingly more surface area as the dome curves over from vertical to horizontal.
The top zone gets far fewer rows than it needs for the requested overlap, and the
last-placed row before the apex can be 5–8 mm of surface arc from the apex, well
beyond one row_step in actual surface terms.

### Meridian Curves

A **meridian** is the intersection of the smooth cluster mesh with a vertical half-plane
at a given azimuthal angle φ around the cluster's XY centroid.

Sample `N` meridians at azimuthal angles φ₀, φ₁, …, φ_{N-1} evenly spaced in [0, 2π).
For each meridian φᵢ:

1. At each of `leaf_arc_z_samples` fine Z levels from `z_bottom` to `z_top`, take a
   horizontal cross-section of the shaped mesh.
2. Find the perimeter point at azimuthal angle φᵢ (closest to the half-plane at φᵢ).
3. String these points into a polyline Mᵢ = [(x₀, y₀, z₀), (x₁, y₁, z₁), …].
4. Compute cumulative arc length along Mᵢ: s_k = Σ |Mᵢ[k] − Mᵢ[k-1]|.
5. At each point, compute the local surface tangent (T_r, T_z) in the r-z plane from
   adjacent points:
   ```
   r_k = dist2d(M[k], cluster_centroid_xy)
   (T_r, T_z) = normalize((r_{k+1} − r_{k-1}, z_{k+1} − z_{k-1}))
   ```
6. The outward surface normal in the r-z plane is:
   ```
   N_r = T_z         # rotate T by −90°
   N_z = −T_r
   ```
7. Extend to 3D along the azimuthal direction:
   ```
   outward_3d = N_r * (cos(φᵢ), sin(φᵢ), 0) + N_z * (0, 0, 1)
   ```

This gives each meridian a table of (z, arc_length, outward_normal_3d) values.

#### Why N=6?

Six meridians give 60° angular resolution before interpolation.  For a gently-curved
convex mesh, the angular variation of the surface normal between adjacent meridians is
small (typically < 30°) so linear interpolation of normals is accurate.  At N=4 the
angular gap is 90° and interpolation degrades on asymmetric or elongated meshes.  At
N=12 the benefit is marginal and the cost doubles.  N is a public parameter
(`leaf_arc_meridians`) so it can be increased for highly irregular meshes.

### Row Z Positions

**Average arc-to-Z mapping:**

For each target surface arc value `s`, find the Z level `z` by averaging over all
meridians:

```python
def avg_z_for_arc(s_target, meridians):
    z_vals = []
    for m in meridians:
        z_vals.append(np.interp(s_target, m.arc_vals, m.z_vals))
    return np.mean(z_vals)
```

**Pinned top and bottom rows:**

- **Bottom anchor** `z_bot_anchor`: one leaf-length of surface arc above the lowest
  Z level where the mesh surface is upward-facing enough to receive a leaf.

  The world-Z bottom of the mesh may include surface area whose outward normal points
  downward — the underside filter will reject all leaves placed there.  Anchoring
  from the absolute mesh bottom wastes the slot and pushes the first useful row one
  step higher than intended.  Instead, find the lowest Z where the averaged meridian
  normal crosses the upward-facing threshold, then place the bottom row one
  leaf-length of arc above that:

  ```python
  # Lowest Z where the averaged meridian normal is upward-facing enough
  z_placeable = _lowest_placeable_z(meridians, normal_z_threshold=-0.1)
  s_placeable = avg_arc_for_z(z_placeable, meridians)
  z_bot_anchor = avg_z_for_arc(s_placeable + leaf_length_mm, meridians)
  ```

  A leaf attached at `z_bot_anchor` hangs downward and covers the lowest visible
  surface of the mesh.  This is computed entirely from the meridian data — no
  knowledge of the mesh's internal structure is needed or used.

- **Top anchor** `z_top_anchor`: placed just below the world-Z apex.  Use
  `z_top = shaped.vertices[:, 2].max()` and step slightly below it to ensure the
  cross-section is non-degenerate:
  ```python
  z_top_anchor = z_top - 0.25 * leaf_length_mm
  ```

**Integer optimization:**

The surface arc between the two anchors, measured on the averaged meridian:

```python
s_bot = avg_arc_for_z(z_bot_anchor, meridians)   # inverse of avg_z_for_arc
s_top = avg_arc_for_z(z_top_anchor, meridians)
inner_arc = s_top - s_bot
row_step_target = leaf_length_mm * (1.0 - leaf_v_overlap)
N_gaps = max(1, round(inner_arc / row_step_target))
actual_row_step_arc = inner_arc / N_gaps
```

Row Z positions:

```python
row_arc_positions = [s_bot + i * actual_row_step_arc for i in range(N_gaps + 1)]
row_z_positions   = [avg_z_for_arc(s, meridians) for s in row_arc_positions]
# row_z_positions[0]  = z_bot_anchor
# row_z_positions[-1] = z_top_anchor
```

The resulting overlap will be `1 − actual_row_step_arc / leaf_length_mm`, which is the
best integer-fit approximation to the requested `leaf_v_overlap`.  For typical parameters
(v_overlap=0.5, leaf_length=4.5 mm) the error is < 0.05 on any cluster with 3+ rows.

**No apex cap needed.**  The top anchor row places leaves just below the world-Z apex;
the leaf geometry (arch + lift) covers the apex from that position.  The `leaf_cap_count`
parameter is deprecated.

### Per-Leaf Surface Normal (Azimuthal Interpolation)

For each attachment point `pt3d` on the cross-section perimeter:

1. Compute the azimuthal angle of the attachment point relative to the cluster XY centroid:
   ```python
   phi_leaf = atan2(pt3d[1] - centroid_xy[1], pt3d[0] - centroid_xy[0])
   ```

2. Find the two bracketing meridians: φᵢ ≤ φ_leaf < φᵢ₊₁ (wrapping at 2π).

3. At the current row's Z level, interpolate between the two meridian normals:
   ```python
   n_i   = meridian_i.normal_at(z_row)
   n_ip1 = meridian_ip1.normal_at(z_row)
   w     = (phi_leaf - phi_i) / (phi_ip1 - phi_i)
   up_hint = normalize(lerp(n_i, n_ip1, w))   # lerp + normalize ≈ slerp for small angles
   ```

4. Use `up_hint` for contact-angle computation and leaf placement.
   ```python
   local_r = dist(pt3d, section_centroid_3d)   # same as current
   ca = _contact_angle_for_sphere(local_r, ...)
   tangent   = normalize(T0 * cos(ca) - up_hint * sin(ca))
   up_placed = normalize(up_hint * cos(ca) + T0 * sin(ca))
   ```

### Underside Filtering

Any surface whose outward normal points sufficiently downward cannot hold a leaf —
the contact-angle formula inverts and the leaf tangent ends up pointing upward into
the mesh.  The guard:

```python
if up_hint[2] < -0.1:
    continue   # skip downward-facing surface normals
```

…applies uniformly to every attachment point regardless of where on the mesh it sits.

The bottom anchor placement (`z_placeable + leaf_length_mm`) already starts above the
lowest upward-facing surface, so most underside positions are excluded before the
placement loop runs.  The per-leaf guard is a safety net for positions that slip
through (e.g. a non-convex indentation whose section centroid produces a misleading
normal direction for a specific azimuth).

The threshold −0.1 permits surfaces tilted up to ~96° from vertical (nearly
horizontal underside) to receive leaves.  Tighten toward 0.0 to restrict leaves to
more upward-facing surfaces; loosen toward −0.3 to allow leaves on steeper undersides.

### Placement Loop (Full)

```python
meridians = _build_meridians(shaped, N=leaf_arc_meridians, Z_samples=leaf_arc_z_samples)
row_z_positions = _compute_row_z_positions(meridians, leaf_length_mm, leaf_v_overlap,
                                           z_top, z_bottom)

for z_row in row_z_positions:
    section = shaped.section(plane_origin=[0, 0, z_row], plane_normal=[0, 0, 1])
    if section is None:
        continue
    path2d, xform = section.to_planar()
    for poly in path2d.polygons_full:
        n_col = ceil(poly.length / col_step)
        centroid_3d = xform @ [*poly.centroid.coords[0], 0, 1]
        for ci in range(n_col):
            pt2 = poly.exterior.interpolate(ci / n_col, normalized=True)
            pt3d = (xform @ [pt2.x, pt2.y, 0, 1])[:3]
            phi_leaf = atan2(pt3d[1] - cx, pt3d[0] - cx)
            up_hint  = _interpolate_meridian_normal(meridians, phi_leaf, z_row)
            if up_hint[2] < -0.1:
                continue
            local_r = dist(pt3d, centroid_3d[:3])
            _emit_leaf(pt3d, up_hint, key=(row_idx, ci), cluster_radius_mm=local_r)
```

---

## Current Algorithm (Deprecated)

*Retained here for reference.  The meridian-arc algorithm replaces this.*

### Z-Slice with Uniform dZ

```python
row_step = leaf_length_mm * (1.0 - v_overlap)
z_row = z_bottom
while z_row <= z_top:
    section = shaped.section(...)
    for poly in polygons:
        outward = normalize(pt3d - mesh_center_3d)   # centroid-to-point approximation
        local_r = dist(pt3d, slice_centroid)
        _emit_leaf(pt3d, outward, ...)
    z_row += row_step
```

**Failure mode:** fixed dZ ≠ fixed surface arc.  Near the world-Z apex, the dome
surface is nearly horizontal, so each dZ step covers much more surface area than a
step on the cone body.  The last few rows before the apex are spread far apart in
surface terms, and the zone within `~2 × row_step` of the apex gets zero coverage
from Z-slices.

### Apex Cap (Deprecated)

The apex cap was a special-case patch to cover the world-Z apex zone that Z-slices
could not reach.  It placed `leaf_cap_count` leaves fanning outward from the apex
vertex with `lift_mm = 0` (critical: lift would spike leaves upward).

The cap was correct in spirit but brittle:
- It used `argmax(dot(tip_t))` (branch-direction apex) before 2026-06-24, missing
  the world-Z apex on tilted branches by 4–6 mm.
- The `gap_mm` offset from the apex to avoid all leaves sharing one point was computed
  analytically, not from the shaped mesh, and could place bases inside the noised mesh.
- A `contact_angle >= π/2` fallback silently used `ca=0` instead of skipping.

The meridian-arc algorithm eliminates the apex cap by pinning a proper row near the
world-Z apex from the outset.

---

## Examples

### Example 1: Nearly Vertical Branch (tip_t ≈ [0, 0, 1])

The cluster is roughly symmetric around world-Z.  All six meridians have nearly
identical arc-to-Z curves.  The averaged mapping is very close to any individual
meridian.  Row Z positions are distributed as:

```
z_top    = 48.3 mm   z_top_anchor = 47.2 mm  (top row pinned here)
z_bottom = 30.1 mm   z_bot_anchor = 34.6 mm  (bottom row at arc = 4.5mm up)

inner_arc ≈ 20 mm,  row_step_target ≈ 2.25 mm  →  N_gaps = 9
row_z ≈ [34.6, 36.8, 39.0, 41.2, 43.4, 45.0, 46.3, 47.0, 47.2]
```

The rows compress toward the top because the dome surface curves over — but they now
correctly track surface arc distance rather than vertical distance.  Coverage is
uniform from bottom to apex.

Surface normals from the six meridians are nearly identical at each Z level (cluster
is symmetric), so interpolation has no visible effect.  The result is similar to the
current algorithm except for the apex zone.

### Example 2: Moderate Tilt (tip_t ≈ [0.5, 0, 0.87], 30° from vertical)

The cluster leans 30°.  The world-Z range is similar, but the cluster is no longer
symmetric:

- The upper-front side (toward +X) has a steeper cone surface.  Its meridian has
  more arc per unit Z than the back side.
- The world-Z apex is NOT the branch tip; it is on the dome on the upper-front side,
  approximately 4–5 mm in Z above the branch-direction apex.

**Row Z positions**: the averaged meridian has a slower arc-per-Z rise at the dome
top than for the vertical case.  N_gaps may be one fewer (the dome extends less in Z
and is covered by fewer rows).  The top anchor is correctly placed at the world-Z apex
of the asymmetric dome, not at the branch tip.

**Surface normals**: the front-side meridians have normals tilted more upward; the
back-side normals tilt more backward.  A leaf placed on the front face gets `up_hint`
tilted upward (correct — the front dome surface faces up-and-front).  A leaf on the
back of the cone gets `up_hint` angled backward (correct — the cone back surface faces
back-and-outward).  These differ by 20–30° from the centroid-to-point approximation.

### Example 3: Steep Tilt (tip_t ≈ [0.85, 0, 0.53], 58° from vertical)

The branch is nearly horizontal.  The cluster is elliptical in world-Z cross-section
with an aspect ratio of roughly 1.6.  The world-Z range is much smaller than the
branch-direction range.

Critical differences from the vertical case:

- The world-Z apex may be on the CONE BODY, not the forward dome.  The cone body
  slopes upward steeply enough that its uppermost point exceeds the dome's world-Z
  maximum.
- The back hemisphere is partially above the cone body's equator.  The bottom anchor
  (at arc = `leaf_length_mm`) may land in the back hemisphere zone; the underside
  filter `up_hint[2] < -0.1` correctly excludes leaves that would face downward there.
- The meridians on the upper side are much shorter in arc (steeper surface) than those
  on the lower side (gentler slope toward underside).  The averaging reduces this to a
  moderate arc length, and the actual row spacing is a compromise.

For very steep clusters (>70° from vertical) the leaf coverage is intrinsically
limited: only the upper 90–120° of arc is above the underside threshold, and the number
of rows is small.  This is physically correct — a nearly-horizontal branch has a mostly
downward-facing cluster that a viewer sees from the side, not the top.

---

## Known Open Items

### 1. Jitter disabled

```python
jit = 0.0   # leaf_angle_jitter_deg
pj  = 0.0   # leaf_pos_jitter
```

Disabled for visual debugging, never re-enabled.  Re-enabling: restore
`jit = float(leaf_angle_jitter_deg)` and `pj = float(leaf_pos_jitter)`.
Verify the `tangent[2] > 0.707` RuntimeWarning does not fire after re-enabling.

### 2. Leaf size not scaled to cluster size

`leaf_length_mm` and `leaf_width_mm` are fixed per tree regardless of cluster size.
On very small clusters (`r_tip < 2.5 mm`) `contact_angle → π/2` and all leaves are
skipped, leaving a bare ball.  Consider scaling leaf geometry proportionally to
`r_tip` so small clusters still get some coverage.

### 3. Contact-angle formula assumes local sphere

The formula `contact_angle = arccos(−D / 2R) − arctan(L_comp / N_comp)` uses the
cross-section radius as a local sphere radius.  This is accurate on the dome (locally
spherical) and over-estimates the contact angle on the flat cone body (which has
infinite curvature radius).  A future improvement: use the meridian's second-derivative
`d²r/ds²` to compute the actual local surface curvature and feed it into the contact
angle formula.

### 4. Meridian sampling near the back-hemisphere/cone seam

The back hemisphere connects to the cone body at `start_pos`.  The surface normal
changes direction sharply at this seam (hemisphere is curved; cone is flat).  A
meridian polyline crossing this seam at a very acute angle may produce a spurious
tangent spike.  Smoothing the tangent with a 3-point weighted average would suppress
this.

### 5. Diagnostic flag

`DHARMATILES_DEBUG_LEAVES=1` should emit per-cluster row placement diagnostics
per the protocol in `docs/meta/history/2026-06-24-systematic-algorithm-development.md`.
This flag is not yet implemented for the meridian-arc algorithm.

---

## What Was Tried and Abandoned

- **Embossed surface relief** — leaf shapes as vertex displacement on the icosphere.
  Would be faster and print-safe but looks like scales.
- **Branchlet/petiole stems** — tiny tubes growing from the cluster, leaf at the tip.
  24+ commits; abandoned because a stem that bends downward creates an FDM undercut.
- **Arc-parameterized placement (first attempt)** — rows/columns indexed by arc
  position on the cluster surface.  Replaced by Z-slice because the outward direction
  was wrong at the dome top, producing blade-on-edge artefacts, and the pole was
  brittle.
- **Binary-search contact angle** — `find_contact_angle_for_sphere` at 48 iterations
  per leaf.  Replaced by the analytical closed-form + cache.
- **Z-slice with uniform dZ (current code)** — correct for cylinders, wrong for the
  dome.  Produces bald zones near the world-Z apex.  Patched six times with an apex-cap
  special case that never fully solved the problem.  Replaced by meridian-arc.
