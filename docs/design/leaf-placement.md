# Leaf Placement — Specification
*Current as of 2026-06-24.  See `docs/meta/history/2026-06-24-leaf-rendering-deep-history.md`
for the full history of what was tried and abandoned before this approach.*

---

## What This Covers

How individual leaves are placed on foliage clusters in the tree generator.  Foliage
clusters are the bumpy green blobs at the end of each terminal branch.  Leaves are
separate 3D geometry (watertight solids) that sit on the cluster surface.

Implementation files:
- `src/dharmatiles/trees/leaf.py` — leaf geometry primitives
- `src/dharmatiles/trees/mesh.py` — placement algorithm (`_build_foliage_cluster_mesh`,
  `_emit_leaf`, `_contact_angle_for_sphere`)

---

## Leaf Geometry

A leaf is an ovate, keeled blade:

- **Outline**: teardrop, peak width at ≈ 1/3 from base, pointed tip, rounded base.
- **Cross-section**: quartic Bézier dome rising from the midrib crease.
- **Crease**: narrow tanh V-fold at the midrib.
- **Walls**: `solidify_leaf()` adds walls from the perimeter down to the cluster surface,
  plus a root cap.  The result is a watertight closed solid.

The keel (a V-ridge on the underside) exists in `build_leaf_mesh()` but is **not used**
by the foliage placement path.  The cluster placement path uses `build_leaf_surface()` +
`solidify_leaf()` only (no keel).

### Leaf parameters (all on `Tree` layer)

| Parameter | Default | Effect |
|---|---|---|
| `leaf_length_mm` | 4.5 | Leaf length from base to tip |
| `leaf_width_mm` | 3.0 | Peak leaf width (at ≈ 1/3 from base) |
| `leaf_thickness_mm` | 0.24 | Dome height at peak |
| `leaf_fold_angle_deg` | 6.0 | Midrib crease V-angle |
| `leaf_inner_curve` | 1.5 | Crease-side Bézier shoulder |
| `leaf_outer_curve` | 0.72 | Edge-side Bézier shoulder |
| `leaf_curl_deg` | 40.0 | Concave tip curl (tip curves under) |
| `leaf_lift_mm` | 3.0 | Additional tip lift above contact-angle position |
| `leaf_h_overlap` | 0.2 | Fraction of leaf width that overlaps adjacent columns |
| `leaf_v_overlap` | 0.5 | Fraction of leaf length that overlaps adjacent rows |
| `leaf_base_count` | 5 | Not used in current Z-slice path (reserved) |
| `leaf_cap_count` | 12 | Number of leaves in the world-Z apex cap |
| `leaf_angle_jitter_deg` | 24.0 | Yaw jitter range (currently disabled: `jit=0`) |
| `leaf_pos_jitter` | 0.165 | Position jitter fraction (currently disabled: `pj=0`) |

---

## Contact Angle

### What It Is

The contact angle is the rotation about the lateral axis T (perpendicular to both
the leaf growth direction and the cluster outward normal) that presses the leaf tip
just against the cluster surface.

Without the contact angle, the leaf sits flat on the surface and its walls are
nearly coplanar with the cluster surface.  The wall-embedding raycast in `solidify_leaf`
needs the perimeter vertices to be close to the surface and above it; without contact-
angle tilt, many raycasts miss the mesh and produce 0.75 mm stub walls.

### Frame After Contact-Angle Rotation

```
up_hint  = outward radial at attachment point (from cluster centroid → point)
T0       = gravity-down projected onto the tangent plane (with yaw jitter)
ca       = contact angle (radians)

tangent   = T0 * cos(ca) − up_hint * sin(ca)
up_placed = up_hint * cos(ca) + T0 * sin(ca)
```

`build_leaf_surface(base_pos, tangent, up_placed, ...)` then builds the leaf in this
tilted frame.

### Analytical Computation (`_contact_angle_for_sphere`)

The foliage cluster is locally approximated as a sphere of radius `local_r` at each
attachment point.  The contact angle satisfies:

```
tip(θ) = base + (L·cosθ + N·sinθ)·T0 + (N·cosθ − L·sinθ)·up
|tip(θ) − local_center|² = local_r²
```

After expanding (T_comp = 0 by bilateral symmetry):

```
N_comp·cosθ − L_comp·sinθ = −D² / (2·local_r)
```

where D = |v_tip − base| (tip displacement at θ=0, from `compute_leaf_geometry`).
This is `A·cosθ + B·sinθ = C`, solved analytically with `atan2`.

The computation is cached per `(cluster_radius_mm, leaf_geometry_params)`.  Most
leaves on one cluster share the same `local_r`, so the cache gives O(1) amortized
cost (~10–20 distinct radii per cluster).

### Guard

If `contact_angle >= π/2`, the leaf tangent points into the cluster surface rather
than outward.  This happens when the cluster ring radius is too small for the leaf
geometry (`D > 2R`).  Such leaves are skipped entirely.

---

## Z-Slice Placement Algorithm

The main placement algorithm slices the smooth (pre-noise) cluster mesh with
horizontal planes at regular vertical steps and places one leaf at each perimeter
sample.

### Overview

```python
row_step = leaf_length_mm * (1.0 - v_overlap)   # typically 4.5 × 0.75 = 3.375 mm
col_step = leaf_width_mm  * (1.0 - h_overlap)   # typically 3.0 × 0.80 = 2.4 mm

z_row = z_bottom
while z_row <= z_top:
    section = shaped_mesh.section(plane_origin=[0,0,z_row], plane_normal=[0,0,1])
    for polygon in section.polygons_full:
        n_col = ceil(polygon.perimeter / col_step)
        for ci in range(n_col):
            pt3d   = polygon_point_at(ci / n_col)   # on the shaped mesh surface
            outward = normalize(pt3d - mesh_centroid_3d)
            local_r = dist(pt3d, slice_centroid)
            _emit_leaf(pt3d, outward, key=(row, ci), cluster_radius_mm=local_r)
    z_row += row_step
```

### Outward Direction

`outward = normalize(pt3d − mesh_center_3d)` — the 3D centroid of the shaped mesh
to the surface point.  This is correct on:
- The cone body: points outward horizontally.
- The dome top: points upward.
- The back hemisphere: points backward/downward (filtered — see underside guard below).

### Local Radius

`local_r = dist(pt3d, section_centroid_3d)` — the 2D radius of the cross-section
polygon at this slice.  This is the actual cone/dome ring radius, not a fixed value.
It feeds the contact-angle cache key.

### Underside Guard

```python
if outward[2] < -0.1:
    continue   # skip downward-facing surface
```

The cluster underside (back hemisphere hidden by the branch) has outward vectors
pointing downward.  The contact-angle formula is invalid there — it pushes the leaf
tangent upward instead of into the surface, producing vertical spike leaves.  The
underside is also hidden by the branch and invisible.

The threshold −0.1 allows a 10% downward tolerance, keeping near-equatorial
side-facing placements while removing true underside positions.

### Instrumentation

`_emit_leaf` emits a `RuntimeWarning` when `tangent[2] > 0.707` (leaf tangent
pointing more than 45° upward after contact-angle tilt).  This catches apex and
underside regressions immediately.

---

## World-Z Apex Cap

### Why It Is Needed

Near the world-Z apex of the cluster, the horizontal cross-section shrinks to a
point.  `local_r → 0` causes `contact_angle → π/2` and the leaf is rejected by
the contact-angle guard.  The gap between the last placed row and the apex is up
to one `row_step` = 3.375 mm.

### Why `argmax(z)`, not `argmax(dot(tip_t))`

**The apex cap targets the world-Z apex vertex, not the branch-direction apex.**

For tilted branches, the branch-direction apex (`argmax(dot(tip_t))`) and the
world-Z apex (`argmax(z)`) are different vertices, separated by 4–6 mm in Z for
typical canopy angles.  Z-slices work fine at the branch-direction apex (normal
cross-section there); only the world-Z apex has the tiny-cross-section problem.

Using `argmax(dot(tip_t))` was a prior bug that produced bald spots on tilted clusters.

### Cap Algorithm

```python
apex_v_idx  = argmax(shaped.vertices[:, 2])          # world-Z apex
apex_smooth = shaped.vertices[apex_v_idx]
apex_up     = normalize(apex_smooth - mesh_center_3d)  # outward at apex
e1, e2      = two_perp(apex_up)                        # tangent plane basis

for ci in range(leaf_cap_count):
    phi    = 2π * ci / leaf_cap_count
    T0_raw = normalize(cos(phi) * e1 + sin(phi) * e2)

    # Offset base from apex by half a leaf-width so leaves don't all share one point.
    gap_angle = arcsin(clip(leaf_width_mm * 0.5 / r_tip, 0, 0.95))
    base_dir  = normalize(cos(gap_angle) * apex_up + sin(gap_angle) * T0_raw)

    # Use nearest shaped-mesh vertex in that direction (always outside the
    # noised cluster surface — analytic sphere points may lie inside).
    base_smooth = shaped.vertices[argmax(shaped.vertices @ base_dir)]
    up_hint_ci  = normalize(base_smooth - mesh_center_3d)

    # Sink to the noised cluster skin (identical to _emit_leaf).
    disp   = gaussian_noise(base_smooth) + coarse_noise(base_smooth) - noise_peak
    base_pos = base_smooth + up_hint_ci * disp

    # Apply contact angle at r_tip (the dome radius at the apex).
    ca     = _ca_cache[(r_tip, leaf_geometry...)]
    tangent   = normalize(T0_leaf * cos(ca) − up_hint_ci * sin(ca))
    up_placed = normalize(up_hint_ci * cos(ca) + T0_leaf * sin(ca))

    # Build leaf with lift_mm=0: the contact angle already drapes it against
    # the dome; lift would push the tip back upward into a spike.
    build_leaf_surface(..., lift_mm=0)
    solidify_leaf(..., parent_mesh=cluster_mesh)
```

**Critical:** apex-cap leaves use `lift_mm=0`.  The contact angle alone drapes
the leaf against the dome.  Any `lift_mm > 0` would push the tip back upward,
producing vertical spikes at the apex.

---

## Foliage Cluster Shape

The cluster is not a sphere.  It is a deformed icosphere with three sections:

1. **Back hemisphere** (radius `r_wood`, behind `start_pos`): round cap hiding
   the stub where the branch enters the cluster.
2. **Cone body** (`r_wood` → `r_foliage` along the Bezier spine `start_pos` →
   `tip_pos`): tapered tube following the branch direction.
3. **Forward dome** (radius `r_foliage`, ahead of `tip_pos`): rounded nose.

The cone body and dome are offset perpendicular-upward so the branch runs along
the cluster bottom without protruding through the skin.

Noise is applied after geometry: fine Gaussian (0.05 mm cell) for grain, coarse
smooth (4 mm cell) for silhouette variation.  Both are shifted inward (subtract
max) so the smooth envelope is the outer bound.

The Z-slice algorithm works on the **smooth, pre-noise** cluster mesh (`shaped`)
to get stable cross-section polygons.  Each leaf base is then sunk to the noised
surface by evaluating both noise layers at the base position and offsetting along
`up_hint`.

---

## Known Open Items

### 1. Jitter disabled

```python
jit = 0.0   # leaf_angle_jitter_deg
pj  = 0.0   # leaf_pos_jitter
```

These were disabled for visual debugging and never re-enabled.  The placement is
fully deterministic.  To re-enable: set `jit = float(leaf_angle_jitter_deg)` and
restore `pj = float(leaf_pos_jitter)` in `_build_foliage_cluster_mesh`.  Verify
the RuntimeWarning for `tangent[2] > 0.707` does not fire on any cluster after
re-enabling.

### 2. Lower-body coverage

The `outward[2] < -0.1` filter removes all rows in the lower half of the cluster
(below the equatorial band).  Typically 3–4 rows place leaves per cluster (the
top ~12 mm of a 14 mm cluster).  The lower-mid body is bare.

This is currently intentional (the underside is hidden by the branch and the filter
prevents spike leaves), but has not been explicitly evaluated for visible side-gaps
in different parameter settings.

### 3. Leaf size fixed across cluster sizes

`leaf_length_mm` and `leaf_width_mm` are fixed per tree, regardless of cluster size.
On very small clusters (`r_tip < 2.5 mm`), `contact_angle → π/2` and leaves are
skipped.  On large clusters, the fixed leaf size looks sparse.  Consider scaling
leaf geometry proportionally to `r_tip`.

### 4. Apex-cap `contact_angle >= π/2` fallback

```python
if apex_ca >= np.pi / 2:
    apex_ca = 0.0   # silently falls back to flat placement
```

A flat apex leaf (`ca = 0`) with `lift_mm = 0` grows horizontally from the apex —
not a spike, but not correct either.  Should skip the cap entirely when the formula
is invalid rather than silently falling back.

---

## What Was Tried and Abandoned

- **Embossed surface relief** — leaf shapes as vertex displacement on the icosphere.
  Never implemented.  Would be faster and print-safe but looks like scales, not leaves.
- **Branchlet/petiole stems** — tiny tubes growing from the cluster, leaf at the tip.
  24+ commits; abandoned because a stem that bends downward creates an FDM undercut.
- **Arc-parameterized placement** — rows/columns indexed by arc position on the
  cluster surface.  Replaced by the Z-slice algorithm because the outward direction
  was wrong at the dome top, producing blade-on-edge artefacts, and the pole required
  a special case that was brittle.
- **Binary-search contact angle** — `find_contact_angle_for_sphere` at 48 iterations
  per leaf.  Replaced by the analytical closed-form + cache.

See `docs/meta/history/2026-06-24-leaf-rendering-deep-history.md` for full analysis.
