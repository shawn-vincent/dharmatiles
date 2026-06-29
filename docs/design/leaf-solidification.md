# Leaf Solidification — Design Specification
*Written 2026-06-29.  Updated 2026-06-29 with confirmed tip-pole raycast failure
analysis.  Covers the complete pipeline from open leaf surface to watertight solid
embedded in the parent foliage cluster mesh.*

Full history: `docs/meta/history/2026-06-29-leaf-jitter-and-placement-fixes.md`,
`docs/meta/history/2026-06-24-leaf-rendering-deep-history.md`.

---

## Overview

A leaf is built in two stages:

1. **`build_leaf_surface`** — produces an open trimesh (disk topology, one boundary
   loop) representing the visible top surface of the leaf blade.  The mesh has all
   visible geometry: ovate outline, V-crease midrib, arched/curled longitudinal
   profile, and dome-shaped lobes.  It is NOT watertight.

2. **`solidify_leaf`** — closes the open surface into a printable watertight solid
   by adding walls that extend from each perimeter vertex down into the parent mesh
   and a flat root cap at the buried end.

After solidification, `place_leaves_on_mesh` runs post-build quality checks (root
depth, curl burial) and discards leaves that fail.

Implementation files:
- `src/dharmatiles/trees/leaf.py` — `compute_leaf_geometry`, `build_leaf_surface`,
  `boundary_loop`, `solidify_leaf`, `LEAF_ROOT_EMBED_MM`, `LEAF_ROOT_WALL_ANGLE_DEG`
- `src/dharmatiles/trees/placement.py` — call site, post-build checks

---

## Stage 1 — Leaf Surface (`build_leaf_surface`)

`build_leaf_surface` calls `compute_leaf_geometry` to get all vertex arrays, then
assembles them into a single open `trimesh.Trimesh`.

### Vertex layout

The open surface contains:

| Block | Count | What |
|---|---|---|
| `upper_grid` (flattened) | `(N_LONG−1) × (N_LAT+1)` | Top face vertex grid, ring-major |
| `base_pt` | 1 | Attachment point (s=0) |
| `tip_pt` | 1 | Pointed tip (s=1) |

`N_LONG = 12`, `N_LAT = 10`.  There are 11 interior rings (`N_LONG − 1`) each with
11 vertices (`N_LAT + 1`), plus 2 pole vertices = **123 vertices** total.

The lower-grid (underside, used in `build_leaf_mesh` for the full blade solid) is
**not** included in the open surface — the walls added by `solidify_leaf` fill that
role.

### Face connectivity

Three groups of faces:

- **Base fan** (`N_LAT` triangles): `[base_pt, ring0[j], ring0[j+1]]` for each
  lateral column j.  base_pt fans into the innermost ring.
- **Body quads** (`(N_LONG−2) × N_LAT × 2` triangles): two triangles per quad
  cell between adjacent rings.
- **Tip fan** (`N_LAT` triangles): `[tip_pt, last_ring[j+1], last_ring[j]]`.

Face normals point outward from the top surface (the crease/dome side).

### Winding cache

`fix_normals()` is called once per topology key and the corrected face array is
stored in `_FACE_CACHE`.  All subsequent leaves of the same topology reuse the
cached winding — this saves ~12k `fix_normals()` calls per tile and is the dominant
leaf-generation performance optimisation.

The cache key includes `curl_deg` (winding differs between flat-tip and curled-tip
geometry) but NOT `lift_mm` (lift rotates all vertices uniformly without changing
which side is outward).

---

## Stage 2 — Solidification (`solidify_leaf`)

### Inputs

| Parameter | Type | Description |
|---|---|---|
| `surface` | `trimesh.Trimesh` | Open leaf surface from `build_leaf_surface` |
| `up_hint` | `(3,)` ndarray | Leaf plane outward normal (= `up_placed` from contact-angle frame) |
| `embed_mm` | float | Depth past the parent surface for each root vertex. Default `LEAF_ROOT_EMBED_MM = 0.75 mm` |
| `parent_mesh` | `trimesh.Trimesh \| None` | Foliage cluster mesh to raycast against |
| `root_wall_angle_deg` | float | Wall taper angle from leaf surface plane. Default `LEAF_ROOT_WALL_ANGLE_DEG = 90.0°` |

### Outputs

`(solid, wall_face_range)`:
- `solid` — closed watertight `trimesh.Trimesh` (leaf surface + walls + root cap)
- `wall_face_range` — `range` of face indices for the wall quads only, usable for
  per-face FDM printability analysis

---

### Step 1 — Extract the boundary loop (`boundary_loop`)

The open surface has exactly one boundary loop (it is a disk, not an annulus or
more complex topology).  `boundary_loop` finds it:

1. Sort every edge's vertex pair: `edges_sorted = sort(mesh.edges, axis=1)`.
2. Find edges that appear exactly once (boundary edges — not shared between two faces).
3. Build an adjacency map and walk the chain starting from any boundary vertex.

The loop is returned as an ordered list of vertex indices.  For the default leaf
geometry (`N_LONG=12`, `N_LAT=10`) the loop has **`NP = 2 × N_LAT + 2 = 22`
vertices**: two lateral edges of 10 segments each, plus the base_pt and tip_pt
poles.

#### Boundary loop adjacency at the tip

In the tip fan, faces are `[tip_pt, last_ring[j+1], last_ring[j]]` for j in
0..N_LAT−1.  The edge `(tip_pt, last_ring[j])` appears in two adjacent tip-fan
faces for every interior j — those edges are interior to the surface.  Only the
two outermost edges are boundary:

- `(tip_pt, last_ring[0])` — left lateral corner of the last ring
- `(tip_pt, last_ring[N_LAT])` — right lateral corner of the last ring

These are the only two perimeter vertices adjacent to `tip_pt` in the boundary
loop.  They sit at s ≈ 0.917 (one ring-step from the tip), t = ±1 (leaf edges),
roughly 0.5 mm from `tip_pt` in 3D for the default leaf geometry.

---

### Step 2 — Per-vertex local surface normals

```python
local_n = surface.vertex_normals[np.array(loop)]   # (NP, 3)
```

`up_hint` (the global `up_placed` from the contact-angle frame) is the outward
normal at the base attachment point.  Due to arch, curl, and lift, the actual
leaf surface at the tip can be tilted 30–70° away from `up_hint`.  Using `up_hint`
for every vertex would produce the wrong wall angle at the tip and base where the
deviation is largest.

The fix: read pre-computed vertex normals directly from the open surface mesh.
Trimesh computes area-weighted face-normal averages, which track the curved leaf
surface closely (dot product ≈ 0.998 vs. the analytic local normal in tests).

`up_hint` (the global normal) is still used for the cap-plane projection in Step 5,
where an approximate shared plane is sufficient.

---

### Step 3 — Ray directions (wall taper)

Each root vertex is reached by casting a ray from its perimeter vertex.  The ray
direction is:

```
ray = sin(α) × (−local_n) + cos(α) × inward
```

where `α = root_wall_angle_deg` and `inward` is the unit vector from the perimeter
vertex toward the perimeter centroid, projected onto the local tangent plane:

```python
centroid   = perim.mean(axis=0)
raw_inward = centroid - perim                           # (NP, 3) vectors toward centroid
dot_ln     = einsum('ij,ij->i', raw_inward, local_n)   # project out normal component
raw_inward -= dot_ln[:, np.newaxis] * local_n           # now lies in local tangent plane
inward     = normalize(raw_inward)                      # (NP, 3)
```

#### Wall angle semantics

| `root_wall_angle_deg` | Ray direction | Effect |
|---|---|---|
| 90° | Pure `−local_n` (straight down through surface) | Wall perpendicular to leaf; root ring same shape as perimeter |
| 70° | `0.94 × (−local_n) + 0.34 × inward` | Mild inward taper; sharp corners (tip, base) converge slightly |
| 50° | `0.77 × (−local_n) + 0.64 × inward` | Noticeable taper; beefy anchoring at tip and base |
| < 50° | Increasingly inward | Root ring may collapse near-to degenerate at sharp corners |

Current default: **90°** (no taper — walls are perpendicular to the local leaf surface).

The ray origin is nudged slightly outward along `local_n` by
`_LEAF_FDM_SUPPORT_TOLERANCE_MM = 0.05 mm` before raycasting, to avoid
self-intersection when a perimeter vertex sits exactly on the parent surface.

---

### Step 4 — Raycasting and root ring placement

**Fallback (no raycast):**
```python
root = perim + embed_mm * ray_dirs   # (NP, 3)
```
Each root vertex is simply `embed_mm` along the ray from the perimeter vertex.
Used when `parent_mesh is None` or a ray misses.

**With parent mesh:**
```python
locs, ray_idx, _ = parent_mesh.ray.intersects_location(
    ray_origins = perim + TOL * local_n,
    ray_directions = ray_dirs,
    multiple_hits = True,
)
for each ray ri:
    hits_on_ri = locs[ray_idx == ri]
    nearest_valid = hits[dist <= _LEAF_ROOT_MAX_HIT_MM]
    root[ri] = nearest_valid + embed_mm * ray_dirs[ri]
```

`_LEAF_ROOT_MAX_HIT_MM = 10.0 mm` — hits further than this are discarded and fall
back to the angled-offset default.  This prevents spike vertices when a perimeter
vertex ends up slightly inside the parent mesh (e.g. from aggressive pos_jitter),
causing the ray to pass through the interior and hit the far side.

The root vertex is placed `embed_mm` **past** the first valid hit, not at the hit.
This guarantees a positive burial depth regardless of surface curvature.

`embed_mm` default: `LEAF_ROOT_EMBED_MM = 0.75 mm`.

#### Tip-pole raycast: universal miss (confirmed)

`tip_pt` is elevated above the parent mesh surface by the combined effect of
`arch_deg`, `curl_deg`, and `lift_mm`.  Its local surface normal (area-weighted
average of the 10 tip-fan triangles) points significantly away from the parent
mesh — upward and outward from the leaf tip.  At `root_wall_angle_deg = 90°` the
ray direction is pure `−local_n_tip`, which therefore points away from the elevated
tip and does not re-intersect the parent mesh.

**Confirmed by instrumentation** (`test-leaf-placement.py`, sphere r=10 mm,
`lift_mm=3.0`, `curl_deg=40°`, no jitter): 0 of 78 tip-pole rays hit the mesh.

Consequence: `root[tip_pt]` is always placed by the fallback — exactly
`embed_mm = 0.75 mm` along `−local_n_tip` from `tip_pt`, floating outside (or
barely at) the parent mesh surface.  This creates a visible vertex ("V") ~0.75 mm
below the tip in every solidified leaf, sitting outside the parent mesh rather
than embedded inside it.

The subsequent cap fan triangle connects V to `cap_center`.  Because `cap_center`
is projected onto the z-plane of `root[0]` (a vertex near the leaf base, much
lower in Z than the tip), the edge V → `cap_center` forms a visible downward spike
extending from V further into the cluster interior.

This is the primary visual artifact in rendered foliage: a small spike visible at
every leaf tip, pointing down from V through the cluster surface.  The sphere test
passes because the fallback depth (0.75 mm) is well under the 4 mm "long roots"
threshold — the artifact is silent from the test's perspective but visible in the mesh.

---

### Step 5 — Cap centre projection

The root ring is non-planar (each vertex was driven a different distance into a
curved surface).  A naive centroid for the cap fan might land outside the ring.
The cap centre is projected onto an approximate plane defined by `up_hint` (global
normal) passing through `root[0]`:

```python
raw_center = root.mean(axis=0)
center = raw_center - dot(raw_center - root[0], n) * n
```

This ensures the cap centre is approximately coplanar with the root ring regardless
of how much the ring curves.

---

### Step 6 — Wall and cap construction

**Vertex layout of the solid:**

| Block | Indices | What |
|---|---|---|
| Surface vertices | `0 … n_surf−1` | All `123` vertices from `build_leaf_surface` |
| Root ring | `n_surf … n_surf+NP−1` | One root vertex per boundary loop vertex |
| Cap centre | `n_surf+NP` | Single centroid vertex for the root cap |

**Wall faces** (`NP × 2` triangles, forming `NP` quads):
```
for i in 0..NP:
    j = (i+1) % NP
    a, b = loop[i], loop[j]     # adjacent perimeter vertices (surface)
    d, c = root[i], root[j]     # corresponding root ring vertices
    wall faces: [a, b, c] and [a, c, d]
```

**Root cap** (`NP` triangles — centroid fan):
```
for i in 0..NP:
    [cap_centre, root[(i+1)%NP], root[i]]
```

The winding of wall and cap faces is corrected by `_mesh_with_fixed_normals` using
the same cache mechanism as the open surface.

---

## Solid Vertex Counts

For the default topology (`N_LONG=12`, `N_LAT=10`, `NP=22`):

| Part | Vertices | Faces |
|---|---|---|
| Open surface | 123 | 120 |
| Root ring | 22 | — |
| Cap centre | 1 | — |
| **Total solid** | **146** | **120 (surf) + 44 (walls) + 22 (cap) = 186** |

---

## Post-Build Quality Checks (in `place_leaves_on_mesh`)

After `solidify_leaf` returns, the placement loop runs two checks and discards
leaves that fail either.

### Root depth measurement

```python
perim_v    = surf.vertices[loop]                    # (NP, 3) surface perimeter
root_v     = solid.vertices[n_surf : n_surf+NP]     # (NP, 3) root ring
root_depth = max(norm(root_v[i] - perim_v[i]) for i in 0..NP)
```

`root_depth` is the longest root-wall segment — i.e. how far the most deeply
buried perimeter vertex was driven into the parent mesh.  It is stored in
`LeafPlacementStats.root_depths` and used by the test script to flag
suspiciously long roots (check: any root > 3 mm indicates a likely runaway
raycast).

### Curl-region burial check

```python
base_dists_v = norm(surf.vertices - pt3d, axis=1)
curl_mask    = base_dists_v > (L / 2.0)     # vertices more than L/2 from base
curl_verts   = surf.vertices[curl_mask]

inside   = mesh.contains(curl_verts)        # True = vertex inside parent mesh
_burial_d = max(closest_point_dist[inside])
```

The "curl region" is the distal half of the leaf (distance from base > L/2).  This
is where the arch curls upward toward the tip; if this region is buried inside the
parent mesh, the leaf is visually wrong and physically impossible to print.

Leaves where `_burial_d > _PREBURIED_DEPTH_MM` are discarded.
`_PREBURIED_DEPTH_MM = 0.25 mm` — this tolerates tiny discretisation artefacts
while catching leaves that `_contact_angle_for_mesh` failed to un-bury (typically
apex-row leaves placed flat at `contact_angle = 0` because no valid angle existed).

The "float distance" (`_float_d`) is also measured (max distance from parent surface
for exterior curl vertices) and stored for diagnostic purposes, but does NOT cause
discards.

### Why actual vertices, not contact candidates

The post-build burial check uses the full solid vertices (`solidify_leaf` output)
rather than the contact candidates used by `_contact_angle_for_mesh`.  Using actual
vertices avoids false positives on steep bottom-row leaves where the canonical-frame
contact candidates over-report burial due to coordinate-system mismatch between the
canonical sphere frame and the actual mesh surface.

---

## Constants Summary

| Constant | Value | Location | Meaning |
|---|---|---|---|
| `LEAF_ROOT_EMBED_MM` | 0.75 mm | `leaf.py` | Depth past parent surface for each root vertex |
| `LEAF_ROOT_WALL_ANGLE_DEG` | 90.0° | `leaf.py` | Wall taper angle (90° = perpendicular, no taper) |
| `_LEAF_ROOT_MAX_HIT_MM` | 10.0 mm | `leaf.py` | Max accepted raycast hit distance |
| `_LEAF_FDM_SUPPORT_TOLERANCE_MM` | 0.05 mm | `leaf.py` | Ray origin offset to avoid self-intersection |
| `_PREBURIED_DEPTH_MM` | 0.25 mm | `placement.py` | Max tolerated curl-region burial before discard |
| `_FLOOR_TOL_MM` | 0.1 mm | `placement.py` | Tolerance for tip-below-mesh-floor discard |
| `N_LONG` | 12 | `leaf.py` | Longitudinal sections (base→tip) |
| `N_LAT` | 10 | `leaf.py` | Lateral sections across the leaf |

---

## Known Open Items

### 1. Ray direction is wrong at the tip pole — causes universal miss and visible artifact

**This is the most important open item.**

At `root_wall_angle_deg = 90°` the ray is pure `−local_n`, which tracks the
*leaf surface geometry* (arch, curl, lift) rather than the *parent mesh geometry*.
At the tip, where lift and curl have rotated the surface normal far from the cluster
surface normal, the ray does not point toward the parent mesh at all.

The result (confirmed by instrumentation — see Step 4 above): every tip-pole
raycast misses on every leaf, on every parent mesh.  The fallback places `root[tip_pt]`
floating 0.75 mm outside the parent mesh, producing a visible spike artifact at
every leaf tip.

The same mechanism causes **long roots on clusters** for perimeter vertices near
the tip (the last-ring lateral corners), whose local normals are also rotated away
from the cluster surface.  Those rays hit the cluster but at large angles, often
traversing the full interior and striking the far wall at 6–9 mm.

**What the fix needs to do**: the ray direction at the tip (and near-tip perimeter
vertices) must aim at the actual parent mesh surface rather than follow
`−local_n_tip`.  Candidate approaches:

- **Nearest-surface direction**: replace `−local_n` with the direction from the
  perimeter vertex toward its closest point on the parent mesh.  This is
  geometrically exact and works for any parent shape.
- **Global up_hint direction**: use `−up_hint` (the cluster surface normal at the
  base attachment point, passed to `solidify_leaf`) for all perimeter vertices
  instead of per-vertex leaf normals.  Simpler, correct in the flat-to-moderately-
  tilted case, wrong when the leaf is heavily contact-angle tilted.
- **Blend**: interpolate between `−local_n` (correct for mid-leaf perimeter
  vertices, which sit on or near the surface) and the nearest-surface direction
  (correct for the tip), weighted by distance from the base.

The parameter `root_wall_angle_deg` is fully wired and the formula already supports
non-90° values — the fix likely lives in how the *reference direction* for the ray
is chosen, not in the angle arithmetic itself.

### 2. Root cap is a flat centroid fan

The root cap is a simple centroid fan (one central vertex, `NP` triangles radiating
out).  This produces a non-planar cap when the root ring curves significantly (e.g.
at large contact angles on small-radius clusters).  Additionally, `cap_center` is
anchored to the z-plane of `root[0]` (a vertex near the leaf base); for leaves where
`root[tip_pt]` is much higher in Z than `root[0]` (which is universal given the
tip-pole miss), the cap triangle at the tip spans a large Z range, creating the
downward spike from V described in Step 4.  A proper planar cap projected onto the
least-squares-fit plane of the root ring would reduce both problems.

### 3. Raycasting is done per-vertex with `multiple_hits=True`

`parent_mesh.ray.intersects_location` is called once for all `NP` perimeter
vertices simultaneously with `multiple_hits=True`.  For a 22-vertex boundary loop
and a ~2000-face foliage cluster mesh this is fast.  For much denser parent meshes
(> 50k faces) a BVH-accelerated query (trimesh's `RayMeshIntersector`) would be
preferable but the current approach is adequate.

### 4. `wall_face_range` is returned but not used in production

`solidify_leaf` returns a `range` identifying the wall faces for optional per-face
FDM printability analysis.  Nothing currently consumes this range in the tile
generation pipeline.  It was added when a future "flag steep wall angles" check was
planned; that check was never implemented.

---

## Interaction With Contact Angle

`solidify_leaf` receives `up_hint = up_placed` from the placement frame.  This is
the leaf surface normal at the base attachment point AFTER the contact-angle
rotation has been applied:

```
up_placed = normalize(up_hint_surface × cos(ca) + T0 × sin(ca))
```

At `ca = 0` (flat leaf, no tilt), `up_placed = up_hint_surface` and the ray goes
straight into the cluster.  At `ca > 0`, the entire leaf is tilted and `up_placed`
tilts with it.  The root wall follows `up_placed`, not the original surface normal,
so the wall enters the cluster at the same tilt as the leaf — the root geometry is
always consistent with the visible blade.

`lift_mm` is applied inside `compute_leaf_geometry` as a rigid rotation of the entire
vertex grid around the lateral axis through the base.  It is already baked into the
vertex positions when `solidify_leaf` is called.  `solidify_leaf` has no special
knowledge of lift — it treats the lifted geometry the same as unlifted geometry.

This is the root cause of the tip-pole raycast failure: `lift_mm` elevates `tip_pt`
above the parent mesh surface and rotates its local normal away from the mesh, but
`solidify_leaf` uses that local normal as the ray direction without any awareness
that the tip is no longer near the surface.  The ray fires in the wrong direction
and misses.  The mid-leaf perimeter vertices are much closer to the parent surface
and their normals are much less rotated, so their raycasts largely succeed.
