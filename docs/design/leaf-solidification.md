# Leaf Solidification — Design Specification
*Written 2026-06-29.  Updated 2026-06-29 with confirmed tip-pole raycast failure
analysis.  Updated 2026-06-29 with tip bulk vertex implementation (open item 1
resolved).  Covers the complete pipeline from open leaf surface to watertight solid
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
  `boundary_loop`, `solidify_leaf`, `LEAF_TIP_VERTEX_IDX`, `LEAF_ROOT_EMBED_MM`,
  `LEAF_ROOT_WALL_ANGLE_DEG`, `_LEAF_TIP_BULK_MM`
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
| `tip_vertex_idx` | `int \| None` | Vertex index of the tip pole in `surface`. Pass `LEAF_TIP_VERTEX_IDX` to enable the tip bulk vertex. Default `None` (disabled). |
| `tip_bulk_mm` | float | Depth of the tip bulk vertex below tip_pt along its local normal. Default `_LEAF_TIP_BULK_MM = 0.5 mm`. |

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
geometry (`N_LONG=12`, `N_LAT=10`) the loop has **`NP = 2 × (N_LONG − 1) + 2 = 24`
vertices**: two lateral edges of `ring_count = 11` vertices each, plus the base_pt
and tip_pt poles.  (The lateral boundary runs along the longitudinal direction, so
it is N_LONG−1 vertices per side, not N_LAT.)

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

### Step 2b — Tip bulk vertex (optional)

When `tip_vertex_idx` is provided, a **tip bulk vertex** is inserted before the
centroid and ray-direction computations.

```python
tip_li = loop.index(tip_vertex_idx)          # position of tip_pt in the loop
lc_li  = (tip_li - 1) % NP                  # ring-10 left corner (loop position)
rc_li  = (tip_li + 1) % NP                  # ring-10 right corner (loop position)

tip_bulk_v_pos = perim[tip_li] - tip_bulk_mm * local_n[tip_li]  # 0.5 mm below tip
perim[tip_li]  = tip_bulk_v_pos              # replace tip_pt in perim in-place
# local_n[tip_li] unchanged — tip_bulk_v inherits tip_pt's outward normal
```

`perim[tip_li]` is replaced so that **every subsequent step** — centroid, inward
directions, fallback root position, and raycast origin — treats the tip bulk vertex
as if it were the tip perimeter vertex.  The net effect:

- The raycast at position `tip_li` fires from `tip_bulk_v_pos`, which is 0.5 mm
  closer to the parent mesh than the elevated `tip_pt`.  This converts the
  previously universal tip-pole miss into a hit on most leaves.
- The fallback root (used when the raycast still misses) is
  `tip_bulk_v_pos + embed_mm × ray_dirs[tip_li]`, which is now embedded 0.5 mm
  deeper than before and positioned much closer to the cluster surface.

`tip_bulk_v_pos` is added as a new explicit vertex in the solid at index
`n_surf + NP + 1`.  Two **bridge faces** and modified wall quads connect it to
the rest of the geometry (see Step 6).

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

#### Tip-pole raycast: universal miss (confirmed, now resolved)

`tip_pt` is elevated above the parent mesh surface by the combined effect of
`arch_deg`, `curl_deg`, and `lift_mm`.  Its local surface normal (area-weighted
average of the 10 tip-fan triangles) points significantly away from the parent
mesh — upward and outward from the leaf tip.  At `root_wall_angle_deg = 90°` the
ray direction is pure `−local_n_tip`, which therefore points away from the elevated
tip and does not re-intersect the parent mesh.

**Confirmed by instrumentation** (`test-leaf-placement.py`, sphere r=10 mm,
`lift_mm=3.0`, `curl_deg=40°`, no jitter): 0 of 78 tip-pole rays hit the mesh when
fired from `tip_pt`.

**Resolution — tip bulk vertex (Step 2b):** when `tip_vertex_idx` is passed to
`solidify_leaf`, the raycast for the tip slot fires from `tip_bulk_v_pos` — a point
0.5 mm below `tip_pt` along the same `−local_n_tip` direction.  Moving the origin
0.5 mm closer to the cluster surface converts the near-universal miss into a hit on
most leaves, and the fallback (used when the ray still misses) is now also 0.5 mm
closer to the mesh surface than it was before.  The visible spike artifact is
eliminated in both cases.

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

**Vertex layout of the solid (standard path, no tip bulk):**

| Block | Indices | What |
|---|---|---|
| Surface vertices | `0 … n_surf−1` | All `123` vertices from `build_leaf_surface` |
| Root ring | `n_surf … n_surf+NP−1` | One root vertex per boundary loop vertex |
| Cap centre | `n_surf+NP` | Single centroid vertex for the root cap |

**With tip bulk vertex active, one additional block is appended:**

| Block | Indices | What |
|---|---|---|
| Tip bulk vertex | `n_surf+NP+1` | `tip_bulk_v_pos` — 0.5 mm below tip_pt |

**Wall faces** (`NP × 2` triangles, forming `NP` quads):

Standard path:
```
for i in 0..NP:
    j = (i+1) % NP
    a, b = loop[i], loop[j]     # adjacent perimeter vertices (surface indices)
    d, c = root[i], root[j]     # corresponding root ring vertices
    wall faces: [a, b, c] and [a, c, d]
```

With tip bulk vertex, the two quads adjacent to the tip are rerouted:
```
for i in 0..NP:
    j = (i+1) % NP
    if i == lc_li:   a, b = loop[lc_li], tip_bulk_v_idx   # left: ring-10 → tip_bulk_v
    elif i == tip_li: a, b = tip_bulk_v_idx, loop[rc_li]  # right: tip_bulk_v → ring-10
    else:             a, b = loop[i], loop[j]              # all other quads unchanged
    d, c = root[i], root[j]
    wall faces: [a, b, c] and [a, c, d]
```

**Bridge faces** (tip bulk only, `2` triangles):

Two triangles connect `tip_pt` and the two ring-10 corners down to `tip_bulk_v`,
sealing the gap left by rerouting the adjacent wall quads:
```
[tip_pt, loop[lc_li], tip_bulk_v_idx]
[tip_pt, tip_bulk_v_idx, loop[rc_li]]
```

In the assembled face list, bridge faces come immediately after the surface faces
(before the wall faces), so `wall_face_range` is offset accordingly.

**Root cap** (`NP` triangles — centroid fan):
```
for i in 0..NP:
    [cap_centre, root[(i+1)%NP], root[i]]
```

The winding of wall, bridge, and cap faces is corrected by `_mesh_with_fixed_normals`
using the same cache mechanism as the open surface.  The cache key includes whether
the tip bulk vertex is active so that tip-bulk and standard solids have separate
cached winding arrays.

---

## Solid Vertex Counts

For the default topology (`N_LONG=12`, `N_LAT=10`):

`NP = 2 × (N_LONG − 1) + 2 = 2 × 11 + 2 = 24`

Surface face count: `N_LAT` (base fan) + `(N_LONG−2) × N_LAT × 2` (body) + `N_LAT` (tip fan)
= 10 + 10×10×2 + 10 = **220 faces**.

| Part | Vertices | Faces |
|---|---|---|
| Open surface | 123 | 220 |
| Root ring | 24 | — |
| Cap centre | 1 | — |
| **Total solid (standard)** | **148** | **220 (surf) + 48 (walls) + 24 (cap) = 292** |
| Tip bulk vertex | +1 | +2 (bridge) |
| **Total solid (tip bulk active)** | **149** | **294** |

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
| `LEAF_TIP_VERTEX_IDX` | 122 | `leaf.py` | Index of tip_pt in the open surface (`(N_LONG−1)×(N_LAT+1)+1`) |
| `LEAF_ROOT_EMBED_MM` | 0.75 mm | `leaf.py` | Depth past parent surface for each root vertex |
| `LEAF_ROOT_WALL_ANGLE_DEG` | 90.0° | `leaf.py` | Wall taper angle (90° = perpendicular, no taper) |
| `_LEAF_TIP_BULK_MM` | 0.5 mm | `leaf.py` | Depth of tip bulk vertex below tip_pt |
| `_LEAF_ROOT_MAX_HIT_MM` | 10.0 mm | `leaf.py` | Max accepted raycast hit distance |
| `_LEAF_FDM_SUPPORT_TOLERANCE_MM` | 0.05 mm | `leaf.py` | Ray origin offset to avoid self-intersection |
| `_PREBURIED_DEPTH_MM` | 0.25 mm | `placement.py` | Max tolerated curl-region burial before discard |
| `_FLOOR_TOL_MM` | 0.1 mm | `placement.py` | Tolerance for tip-below-mesh-floor discard |
| `N_LONG` | 12 | `leaf.py` | Longitudinal sections (base→tip) |
| `N_LAT` | 10 | `leaf.py` | Lateral sections across the leaf |

---

## Ray Direction Design — Cone Framework

*Added 2026-06-29.*

The ray direction problem — choosing how each perimeter wall vertex fires into the
parent mesh — can be understood as finding a direction inside the intersection of
three cones on the unit sphere.  Each cone corresponds to a distinct physical
constraint.

### The three cones

| Cone | Axis | Half-angle | Type |
|---|---|---|---|
| **Leaf surface** | `−local_n` at the vertex | `90° − min_junction_angle` | **Hard constraint** — ray must be close enough to perpendicular to the leaf surface or the wall junction has knife-edge thickness. |
| **FDM printability** | `[0, 0, −1]` (world down) | ~60° (practical limit) | **Hard constraint** — wall must point within the FDM overhang limit of straight down or it needs supports. |
| **Parent mesh** | Inward face normal at nearest surface point | ~90° (wide) | **Soft constraint** — aim toward the mesh; empirically nearly a hemisphere so rarely binding. |

**Undercut is a desire, not a constraint.**  The `inward` direction (from the
perimeter vertex toward the perimeter centroid, projected onto the tangent plane)
defines which side of `−local_n` produces undercut vs. overcut.  It is not a
separate cone — it is a preference for which half of the leaf-surface cone to
occupy.  It should be satisfied when possible and silently dropped when the cone
intersection forces the ray to the overcut side.

A fourth candidate — **leaf centroid** (`inward`) — is not a cone.  It is the
preferred in-plane direction within the leaf-surface cone, relevant only after the
three cones above are satisfied.

### Why the current implementation is limited

`solidify_leaf` has one degree of freedom: `root_wall_angle_deg`.  The formula
`ray = sin(α)(−local_n) + cos(α)(inward)` moves along a single arc between
`−local_n` and `inward`.  This conflates junction thickness (a hard constraint)
with undercut direction (a desire) — they cannot be tuned independently.  The FDM
and parent-mesh constraints are not represented at all; the ray direction is
determined entirely by the leaf's own geometry, which is why the tip-pole ray
points away from the parent mesh when the leaf is lifted.

### Empirical findings (2026-06-29, n=169 placed leaves)

Measured on the standard test scene (sphere r=10 mm + three foliage clusters),
cone half-angles leaf=60°, FDM=60°, mesh=90°, 1000 sphere samples per leaf:

- **Mesh cone is not binding.**  Empirical probe (300 rays/leaf) shows the mesh
  subtends 20–30% of the full sphere from each tip, with a half-angle of 49–71°
  (p25–p75) from the nearest-point normal — effectively a hemisphere.  Using a
  fixed 90° half-angle is a safe approximation.

- **22/169 tips (13%) have an empty triple intersection.**  All 22 have the parent
  mesh facing upward at the tip vertex (inward normal z-component > 0, median 0.6).
  These are leaves on the underside or steep-upward face of a foliage cluster.

- **19/22 empty cases are resolved by dropping the undercut desire.**  The three
  cones do overlap for those leaves, but the overlap lies entirely on the overcut
  side.  A wall on an upward-facing mesh surface cannot simultaneously tuck under
  the leaf and remain FDM-printable; it must flare outward instead.

- **3/22 are geometrically hard.**  The FDM and mesh cones are genuinely
  incompatible (FDM∩mesh angle > 150°) — the mesh face is pointing nearly
  straight up.  No wall direction is simultaneously into the mesh and printable.
  Accept a support-requiring wall or treat these as unfixable.

### Relaxation priority

When the triple intersection is empty, relax in this order:

1. **Drop undercut** — allow the ray to be on the overcut side.  Resolves 19/22
   cases at no printability cost.
2. **Relax FDM angle** — widen the FDM cone past 60° toward 75–80°.  Trades
   some printability for anchor reliability on steep surfaces.
3. **Accept the mesh-normal direction** — fire straight into the mesh regardless
   of FDM angle.  Wall will need supports but leaf is anchored.

### Toward a better ray

An improved ray direction would:
1. Query the parent mesh for the nearest surface point and its inward normal
   (cone 3 axis) — a single BVH nearest-point lookup, ~6× faster than the
   current 24-ray batch raycast, yielding the axis directly.
2. Find the direction in the triple-cone intersection that best satisfies
   the undercut desire, relaxing constraints in priority order when empty.
3. Fire a single well-aimed ray in that direction to find the actual root depth.

This decouples axis selection (geometry-driven, cheap) from root depth measurement
(still needs one ray), and makes all three physical concerns independently tunable.

---

## Known Open Items

### 1. ~~Tip-pole raycast: universal miss~~ — RESOLVED

**Resolved 2026-06-29 by the tip bulk vertex (Step 2b).**

The root cause was that `tip_pt` is elevated above the parent mesh by lift+curl,
so its local normal points away from the mesh and the 90° ray misses universally.
The fix moves the raycast origin 0.5 mm closer to the mesh along the same direction,
converting the miss to a hit on most leaves.  See Step 2b and Step 4 above.

### 2. Root cap is a flat centroid fan

The root cap is a simple centroid fan (one central vertex, `NP` triangles radiating
out).  This produces a non-planar cap when the root ring curves significantly (e.g.
at large contact angles on small-radius clusters).  `cap_center` is projected onto
the plane of `up_hint` through `root[0]`; when the root ring is strongly non-planar
this projection may place the centroid near the ring boundary, producing thin cap
triangles.

Note: the downward spike from `root[tip_pt]` described in earlier analysis is
substantially improved by the tip bulk vertex (the tip root is now much closer to
the cluster surface), but the fundamental flat-centroid-fan issue for non-planar
rings remains.  A proper planar cap projected onto the least-squares-fit plane of
the root ring would be the correct fix.

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

This was the root cause of the tip-pole raycast failure: `lift_mm` elevates `tip_pt`
above the parent mesh surface and rotates its local normal away from the mesh, so
the ray fired from `tip_pt` pointed away from the cluster and missed.  The tip bulk
vertex (Step 2b) resolves this by moving the raycast origin 0.5 mm closer to the
mesh, within range of a successful hit.  `solidify_leaf` still has no explicit
knowledge of lift — the correction is purely spatial (repositioning the origin)
rather than directional (changing the ray vector).
