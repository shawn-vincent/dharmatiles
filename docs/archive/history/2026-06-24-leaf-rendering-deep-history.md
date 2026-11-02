# Leaf Rendering — Deep History and Analysis
*2026-06-24 — retrospective across ~139 leaf-related commits*

## Scope

This document traces every major era of individual-leaf rendering on foliage clusters,
from the first tiled grid through the branchlet detour and up to the current
Z-slice algorithm.  The goal is to establish a durable record of what was tried,
what failed, why it failed, and what the current algorithm actually does — so
future work doesn't repeat the same explorations.

---

## The Big Picture (era map)

| Era | Commits | Core approach | What ended it |
|---|---|---|---|
| 0. No leaves | `bf3cd34`…`4870126` | Foliage clumps only; smooth displaced icosphere | Wanted visual leaf texture |
| 1. Single-tip leaf | `a45f1ab`, `4f9fefa` | One leaf per clump tip; keel-based | Only one leaf per clump, not a mass |
| 2. First tiled grid | `7be6cee`, `97381f3` | Arc-parameterized rows + columns + apex cap | Polar spike artefact; apex gaps |
| 3. Branchlet design | `b38bf38`…`c55490b` | Petiole-tube-from-surface + leaf at tip | Undercut geometry; infinite complexity |
| 4. Solidify pipeline | `ebba32f`…`3ac1038` | Wallified leaves via raycast; `solidify_leaf` | Performance crisis: 20 s/tile |
| 5. Integration attempt | `b14c77d`…`de6dca5` | Arc-grid + `find_contact_angle_for_sphere` | Still slow; apex bald spot; 6 special cases |
| 6. Z-slice revolution | `afbe43e` | Horizontal plane slices of cluster mesh | — |
| 7. Z-slice fixes | `93990c7`…`605ae36` | World-Z apex cap; underside filter; analytics | — (current state) |

---

## Era 0: Foliage Clumps, No Leaves (~2026-06-15 to 2026-06-16)

**Commits:** `d4277e1`, `934f36d`, `bbcc7a4`, `4870126` and earlier cloud-tree work.

The first foliage was a noised icosphere sitting above each terminal branch node —
a "cloud" in the old terminology.  The icosphere was displaced with two noise layers:

- Fine Gaussian noise (0.05 mm cell): per-vertex grain.
- Coarse smooth noise (4 mm cell): large-scale silhouette variation.

The noise was shifted inward (subtract max so peak = 0) to prevent outward growth
beyond the smooth envelope.  No individual leaves at all — just a lumpy green blob.

**Branch embedding:** the branch ran along the bottom of the clump, offset
perpendicular-upward by a fraction of the ring radius.  The offset ramped from
0.5× at the back hemisphere to `(1 - r_wood/r_foliage)` at the tip, keeping the
branch bottom exactly flush with the clump bottom.

**Design doc considered:** `cloud-tree-foliage-leaf-embossing.md` (2026-06-16)
proposed vertex-displacement embossing: stamp leaf shapes as raised/recessed relief
directly into the foliage icosphere surface.  This would have been fast (no separate
geometry) and guaranteed printable (relief only on upward-facing normals).
**It was never implemented.** The codebase went directly to separate-geometry leaves.
This road was right but not taken.

---

## Era 1: Single-Tip Leaf (2026-06-16)

**Commits:** `a45f1ab`, `4f9fefa`.

First individual leaf geometry: one leaf placed at the tip of each foliage clump.
`build_leaf_mesh` was originally inside `cloud_mesh.py` and was extracted to
`trees/leaf.py` (4f9fefa).

**Keel:** a quarter-circle fillet keel along the leaf's longitudinal ridge.
"Replace the flawed edge-normal keel with an explicit longitudinal ridge profile:
the keel bottom edge touches the leaf tip and descends as a quarter-circle fillet."
This was already the second keel design; the first was edge-normal-based and was
described as "flawed."

**Leaf frame:** `build_leaf_mesh(base_pos, tangent, up_hint, ...)`.  The `up_hint`
controls which way the leaf's top face points.  For the single-tip leaf it was set
to the outward dome normal.

**Why it ended:** one leaf per clump looks like a single petal, not a mass of
foliage.

---

## Era 2: First Tiled Grid (2026-06-17)

**Commits:** `7be6cee` ("Tile leaf coverage across clump surface"), `9682835`, `5716551`,
`97381f3` ("Add near-apex leaf ring").

**Algorithm (arc-parameterized):**
- Parameterize the foliage cluster surface by arc length along the branch direction.
- At each arc position sample the latitude `phi` (angle from the branch pole).
- `row_step` = `leaf_length × (1 - v_overlap)`.
- At each row, sample `n_col = ceil(circumference / col_step)` evenly-spaced leaves.
- Droop formula: `T0` = gravity projected onto the tangent plane; `ltan = T0 * cos(dip) - surface_normal * sin(dip)`.
- Apex cap: `leaf_cap_count` leaves fanned out from the branch tip.

**Critical perf win:** switched from `boolean.union` to `concatenate` for leaf assembly.
At 3400+ leaves per tile, boolean union dominated runtime by 16 s+.  Concatenating
independent shells is essentially free.  This decision persists to today.

**Tuning commits:** `9682835` increased position jitter 10%; `5716551` tightened
spacing, reduced droop, enlarged blades.  These were aesthetic tunings.

**Polar spike problem (97381f3):** near the dome pole, the gravity-projected tangent
`dp → 0`, so `ltan ≈ up`.  Leaves near the pole pointed straight up like spikes
rather than draping sideways.  The fix was a special case: pass a horizontal spread
tangent (outward from the tip axis) for the near-apex ring.  The near-apex ring
was added at `phi ≈ 81°` to fill the gap before the cap.

**Cap count tuned:** `leaf_cap_count` 3 → 12.

---

## Era 3: Branchlet Design (2026-06-20)

**Commits:** `b38bf38`, `d803a1c`, `6fc4162`, `5ba5e2c`, `b1a6731`, `9d963fb`,
`e229d60`, `6fd45a9`, `48df7d2`, `1d303b5`, `908f911`, `6ad6fdf`, `ab61464`,
`649a875`, `9906b77`, `4ec6d87`, `0fc46f7`, `3270015`, `ad19d1c`, `556d094`,
`c8f7c61`, `9ec01c3`, `294ba6a`, `c55490b`.

This is the longest sub-story.  ~24 commits over a sustained push to build
"leaf branchlets" — a petiole-like stem connecting each attachment point on the
foliage clump surface to a leaf blade at the tip.

### What the branchlet was

```
foliage clump surface  →  [loft tube]  →  leaf blade
     (anchor)             (stem)          (teardrop)
```

The loft is a tapered tube swept from a circular cross-section at the root (radius
= branchlet_root_radius) to the leaf perimeter at the tip.

Design docs: `tree-leaf-branchlets.md` and `tree-branchlet-growth-algorithm.md`.
These are thorough design documents, still in the repo.

### Why it seemed right

A branchlet decouples leaf blade orientation from the cluster surface orientation.
The blade can face world-up even when the attachment surface faces sideways or
slightly down.  The petiole conversation in `2026-06-19-leaf-placement-fdm-design-space.md`
(recorded mid-session) shows the reasoning: because the cluster surface is FDM-
printable everywhere, every attachment point has `surface_normal.z ≥ sin(26°) ≈ 0.44`.
A stem exiting along the surface normal is therefore always printable at the root.

### Why it failed

**The downward bend creates an undercut.**  The conversation in
`2026-06-19-leaf-placement-fdm-design-space.md` ends exactly at this question:
"If something comes out and heads down, then that will create an undercut. Right?"
— but the session ran out of tokens before answering.

A branchlet that grows outward and then curves downward under gravity does create
an undercut on the underside of the curve.  A branchlet that stays straight but
points sideways is printable.  A branchlet that curves toward horizontal and then
tilts slightly down is printable as a cantilever only within the FDM floor angle.

### Geometry complexity

The commits in this era are almost all geometry fixes, not features:

- `294ba6a` — flip tip ring 180° so pointed end faces down-gravity.
- `c55490b` — require 100% leaf exposure outside parent mesh.
- `9906b77`…`556d094` — 6 commits on detecting/fixing undercut walls ("purple face detection").
- `ab61464` — shorten loft to attachment surface (not some computed length).
- `6ad6fdf` — solve undercut depth per vertex.
- `908f911` — fix thin lip detection.
- `5ba5e2c` — collapse loft to 2 rings (was N intermediate rings, wasteful).
- `b1a6731` — Orin "greenfield strip": 294 → 212 lines, remove all coupling to leaf internals.

The code was getting leaner in each pass but the fundamental geometry problem —
can you attach a leaf-tipped tube to an arbitrary surface and guarantee FDM
printability? — was never fully resolved.

### What remains

The branchlet code path was abandoned for foliage.  But the geometry work from
this era produced `solidify_leaf()` — the function that wall-extrudes a leaf
surface down to a parent mesh via raycasting.  That function still runs in
production on every leaf today.

The loft-tube itself was not re-used.  `leaf.py` still contains `place_leaf_on_mesh()`
and related helpers from this era but they are not called by the production path.

---

## Era 4: Solidify Pipeline — `solidify_leaf` and placement helpers (2026-06-22 early)

**Commits:** `ebba32f`, `1299b3e`, `97d0f2f`, `63b825c` (bug fixes), `32439e3`,
`fa743eb` (code review doc).

**What changed:**

`solidify_leaf(leaf_surf, up_hint, parent_mesh)`: given a leaf *surface* mesh
(no walls yet), walks the boundary loop, raycasts each perimeter vertex along
`-up_hint` into the parent mesh, places the root vertex `embed_mm = 0.75mm` past
the hit, and builds quad walls from perimeter to root.  Result: a closed watertight
leaf solid that is physically attached to the parent.

`leaf_placement_from_surface(mesh, pos)`: snaps a requested position to the nearest
surface point, returns `(base_pos, tangent, up_hint)` using interpolated vertex
normals.

`find_max_dip_for_sphere`: binary-searches for the maximum tilt angle before any
leaf vertex penetrates the sphere.  Correct at the time, but sphere-only.

**Code review (2026-06-22):** The `fa743eb` commit documents a 7-angle multi-agent
review.  Found 4 bugs (one HIGH severity: bisection bracket logic inverted, returned
0 instead of max-valid; one MEDIUM: centroid cap falls outside non-planar root ring;
etc.).  All 4 fixed in `63b825c`.  Six open items left, including dead code, private
name imports, and the performance risk of `_color_leaf_walls_by_fdm` in a production
module.

---

## Era 5: Integration Attempt — Arc Grid + Contact Angle (2026-06-22 to 2026-06-23)

**Commits:** `b14c77d` (solidified debug leaves used in tree foliage), `4578ea5`,
`4d79356`, `583dccf`, `5678405`, `c851fd8`, `3ac1038`, `e0c9af2`, `4a4c382`,
`b25b1ff`, `de6dca5` (flat bottom row + apex fix), `25073f0` (leaf-count diagnostic),
`69238c8`, `31e5ff4`.

### Contact angle

The key problem: when `solidify_leaf` raycasts perimeter vertices toward the parent
mesh, it needs those vertices to actually *hit* the mesh.  If the leaf sits flat
on the surface (no tilt), the perimeter vertices are near-coplanar with the surface
and many raycasts miss → stub walls only 0.75mm deep instead of reaching the surface.

**Solution:** tilt the leaf frame inward so the leaf tip presses against the cluster
sphere.  The angle needed is the "contact angle."  Two approaches were tried:

- `_find_contact_angle_for_mesh()`: runs `parent_mesh.contains()` 16× per leaf on
  the 5120-face icosphere.  Result: never completed (>10 min for 3400 leaves).
- `find_contact_angle_for_sphere()`: binary search, 48 iterations per leaf, using
  the sphere-approximation.  Result: ~20 s/tile.

The analytical closed-form solution (`_contact_angle_for_sphere` with cache) was
designed in this era (see `2026-06-24-leaf-wall-contact-angle-optimization.md`)
but was not yet implemented.

### Naming fix: "dip" → "contact angle"

`3ac1038` renamed the "dip" parameter to "contact angle" throughout.  This
clarification mattered: "dip" implied the leaf was drooping downward; "contact
angle" made clear it was a rotation to press the leaf tip against the sphere —
the tip could end up pointing in any direction depending on the local normal.

### Arc grid special cases accumulated

The main-grid + near-apex-ring + world-top + apex-cap structure had grown to
four distinct placement passes, each with their own tangent-calculation.  The
apex coverage was particularly fragile: the branch-direction apex and world-Z apex
were conflated.  The leaf-count diagnostic commit (`25073f0`) was added to track
how many leaves landed per cluster.

### Apex bald spot (first appearance)

`de6dca5` fixes the apex bald spot for the first time: "near-apex and most cap
leaves pass `r_tip` as the contact-angle sphere radius instead of the local ring
radius.  The old code used `rr = r_tip * cos(phi)`, which drops below the 1mm skip
guard at phi > 79° — silently discarding every near-apex and most cap leaves."

This is the first documented apex problem.  It was fixed, then broken again, then
fixed again across the next 5 commits.

---

## Era 6: Z-Slice Revolution (2026-06-23)

**Commit:** `afbe43e` — "trees: replace leaf placement with Z-slice algorithm"

Removed ~212 lines of arc-parameterized code and all its special cases.

### New algorithm

```python
z_row = z_bottom
while z_row <= z_top:
    section = shaped.section(plane_origin=[0, 0, z_row], plane_normal=[0, 0, 1])
    for poly in section.polygons_full:
        perim = poly.length
        n_col = ceil(perim / col_step)
        for ci in range(n_col):
            pt2d = poly.exterior.interpolate(ci / n_col, normalized=True)
            pt3d = xform @ pt2d
            outward = normalize(pt3d - mesh_center_3d)   # 3D radial
            _emit_leaf(pt3d, outward, (row_idx, ci), cluster_radius_mm=local_r)
    z_row += row_step
```

**Why this is better than the arc parameterization:**

1. **No special cases at poles.** Near the apex, the horizontal cross-section
   simply gets smaller — fewer column samples, until `local_r → 0` where the
   contact-angle guard rejects it naturally.  The near-apex ring and polar-spike
   fix were both eliminated.

2. **Correct 3D outward direction.** The arc parameterization estimated the
   outward direction from the parametric tangent plane.  The Z-slice uses
   `centroid → surface_point` in 3D, which is correct for both the cone body
   (points outward horizontally) and the dome (points upward).  This eliminated
   "blade-on-edge" artefacts where leaves were placed with their flat face
   pointing forward instead of up.

3. **Geometry-driven, not formula-driven.** The shaped mesh (post-noise) is the
   ground truth.  The Z-slice sees exactly what the mesh looks like, including
   the perpendicular-upward offset of the dome center.

4. **Row/column steps are in physical mm.** No `phi`, no `arc_position`.  A leaf
   `row_step` = `leaf_length × (1 - v_overlap)` = 4.5 × 0.75 = 3.375 mm.  Every
   contributor immediately understands what the parameters mean.

**What the Z-slice can't see on its own:** the world-Z apex.  Near the apex,
`local_r → 0`, contact angle → π/2, leaf is rejected.  The gap between the last
placed row and the apex is up to one `row_step` = 3.375 mm.  This required a
supplemental apex cap (below).

**Simplifications made for debugging:** `jit = 0.0, pj = 0.0` — angle and position
jitter were disabled to make the placement debuggable.  **These were never
re-enabled.**  The current production output has fully deterministic, unjittered
leaf placement.

---

## Era 7: Z-Slice Fixes (2026-06-24)

**Commits:** `93990c7`, `00f3b92`, `605ae36`.

### Fix 1: `93990c7` — "fix bald apex and blade-on-edge leaves"

The Z-slice works for the cone body and dome sides, but not near the world-Z apex
where the horizontal cross-section shrinks to a point.  Added an explicit apex cap:
find the vertex most aligned with `tip_t` (branch direction), fan cap leaves from it.

**Problem:** for tilted branches, the branch-direction apex is NOT at world-Z max.
Diagnostic from this session:
```
cluster 17: branch_apex_z=35.40  wz_apex_z=40.00  (gap = 4.6 mm)
```
The cap was covering the wrong point.

### Fix 2: `00f3b92` — "fix bald apex and improve foliage contact-angle accuracy"

Changed cap target from `argmax(dot(tip_t))` to `argmax(z)` (world-Z apex).
But wait — this commit message claims it was changed; the foliage-cluster-baldness
history doc (written at the same time) says the change was `argmax(dot(tip_t))` →
**wrong** and that `argmax(z)` was the pending fix.  Cross-referencing both docs:

The correct understanding is:
- `93990c7` first used **branch-direction apex** (`argmax(dot(tip_t))`).
- `00f3b92` attempted to improve contact-angle accuracy but left the wrong apex.
- `605ae36` (below) is where the apex was definitively changed to world-Z.

(Note: the timing of when exactly `argmax(z)` was introduced vs. `argmax(dot(tip_t))`
is somewhat tangled in the commit message text vs. the baldness doc.  The key fact
is that `605ae36` is the commit that finally stabilized the world-Z apex approach.)

### Fix 3: `605ae36` — "fix upward-pointing leaves on foliage clusters"

Three changes in one commit:
1. **Apex cap contact angle** — the apex cap was calling `build_leaf_surface` with
   `tangent=T0` (horizontal) and `up_hint=apex_up` (vertical), then `lift_mm` curved
   the tip upward, producing vertical spikes.  Now applies the same contact-angle tilt
   (`_ca_cache` keyed on `r_tip`) before building the leaf.

2. **Underside filter** — `outward[2] < -0.1` skips any Z-slice leaf whose radial
   direction points downward.  The cluster underside is hidden by the branch and its
   outward radial points down, so contact-angle tilt pushes the tangent upward instead
   of into the sphere — producing spikes.  Filter eliminates these.

3. **Instrumentation** — `_emit_leaf` now emits `RuntimeWarning` when `tangent[2] > 0.707`
   (post-tilt leaf pointing more than 45° upward).

Side effect: face count dropped ~88k (516k → 428k) by removing hidden underside leaves.

### Analytical contact angle + cache

Also landed in this era (implementation in mesh.py):

`_contact_angle_for_sphere(radius, length_mm, width_mm, curl_deg, lift_mm)` —
closed-form solution to the tip-touching constraint:

```
N_comp·cosθ − L_comp·sinθ = −(L_comp² + N_comp²) / (2R)
```

Solved as `A·cosθ + B·sinθ = C`.  Result: ~10–20 calls per cluster instead of
~3400 binary searches.  Cache keyed on `(cluster_radius_mm, leaf_geometry_params)`.

Performance table (from session notes):

| Approach | Time (db scale) |
|---|---|
| No contact angle (stub walls) | ~1s |
| `place_leaf_on_mesh` (contains()) | never completed |
| `find_contact_angle_for_sphere` (48-iter binary search) | ~20s |
| `_contact_angle_for_sphere` analytical + cache | estimated ~3–5s |

---

## Current State of leaf.py (2026-06-24)

`leaf.py` is 1554 lines.  The public API:

| Function | Production use? | Notes |
|---|---|---|
| `compute_leaf_geometry` | Yes | Core geometry: curves, keel, rings |
| `build_leaf_surface` | Yes (via `_emit_leaf`) | Leaf surface mesh, no walls |
| `build_leaf_mesh` | Rarely (debug) | Leaf + keel, no walls |
| `solidify_leaf` | Yes | Walls + root embedding into parent mesh |
| `leaf_placement_from_surface` | No | Used in branchlet era only |
| `build_leaf_on_surface` | No | Wrapper for above |
| `find_contact_angle` | No | General-purpose, not called in production |
| `find_contact_angle_for_sphere` | No | Replaced by analytical function in mesh.py |
| `find_max_dip_for_sphere` | No | Branchlet era |
| `boundary_loop` | Yes (solidify_leaf) | Internal |
| `place_leaf_on_sphere` | No | Debug only |
| `_find_contact_angle_for_mesh` | No | Branchlet era, too slow |
| `place_leaf_on_mesh` | No | Branchlet era, too slow |

Approximately 400–500 lines are unused in the production path.  They are leftover
from the branchlet era.

---

## The Two Roads Not Taken

### 1. Embossed surface relief (`cloud-tree-foliage-leaf-embossing.md`, 2026-06-16)

Leaf shapes as *displacement on the foliage icosphere surface*.  Raised body,
recessed outline groove, raised midrib — all just vertex displacements.

**Why it would have been better in some ways:**
- Guaranteed printable: only raise surfaces where normal is safe.
- No separate geometry: zero extra faces.
- No contact-angle problem: no separate leaf mesh, nothing to embed.
- Clean overlap: later-priority leaves just take their displacement.

**Why it was not pursued:**
- Doesn't read as individual leaves; looks more like embossed scales.
- No curl, droop, or 3D overhang — fully flat on the clump surface.
- Icosphere subdivision would need to be higher for fine feature resolution.
- The instinct to have *real* protruding leaves was stronger.

### 2. Petiole/branchlet stems (`tree-leaf-branchlets.md`, 2026-06-20)

Tiny tubes growing from the cluster surface to hold leaves.  The biologically
accurate model.

**Why it would have been better:**
- Leaf blade orientation fully decoupled from surface orientation.
- Natural drooping appearance.
- Clean embedding: the stem root is embedded, the blade floats free.

**Why it failed:**
- A stem that bends downward creates an undercut.
- 24+ commits and still not working reliably.
- The complexity of the undercut detection was increasing, not decreasing.
- The "greenfield strip" commit (`b1a6731`) reduced the code but not the problem.

---

## Recurring Problems and Their Root Causes

### 1. The apex bald spot (recurred 5+ times)

**Root cause:** "apex" has two meanings:
- Branch-direction apex: vertex with `argmax(dot(tip_t))`.  The forward-most
  point of the cluster along the branch direction.
- World-Z apex: vertex with `argmax(z)`.  The highest point of the cluster
  in gravity.

For nearly-vertical branches, these coincide.  For tilted branches, they can
be 4–6 mm apart in Z.  Z-slices work fine at the branch-direction apex (normal
local_r there, cross-section is full-sized), but the world-Z apex has `local_r → 0`
and is rejected by the contact-angle guard.

**Every time the apex cap targeted the branch-direction apex, the fix was wrong.**
Detected only when a tilted branch rendered a bald spot.

**The permanent fix:** always target `argmax(z)` for the apex cap.  The branch-
direction apex does not need a cap because Z-slices cover it.

### 2. Contact angle performance (recurred across 2 eras)

**Root cause:** the contact angle must be computed per-leaf (varies with cluster
radius), and the only implementation was a 48-iteration binary search.  At 3400
leaves/tile this was 48 × 3400 = 163,200 iterations.

The analytical solution was clear once the math was written out: a single closed-
form formula + dict cache.  Should have been written before the binary search was
ever applied to production code.

### 3. Upward-pointing leaves (recurred in Z-slice era)

**Root cause:** the contact-angle formula assumes `outward[2] ≥ 0`.  When the
cluster underside has `outward[2] < 0`, the contact-angle rotation pushes the
tangent *upward* instead of into the sphere.  The `outward[2] < -0.1` filter
was the correct fix; it should have been the first thing added when the Z-slice
was introduced.

### 4. Wall stubs instead of full walls (one occurrence)

**Root cause:** `solidify_leaf` embeds walls by raycasting from perimeter vertices
along `-up_hint`.  If the leaf sits flat on the surface without contact-angle tilt,
many raycasts miss the parent mesh → stub walls 0.75mm deep.  The contact-angle
computation exists specifically to tilt the leaf so raycasts succeed.  The connection
was not obvious: "contact angle" sounded like a feature of where the leaf tip lands,
not a prerequisite for wall embedding.

---

## What Is Actually Good About the Current Implementation

1. **Z-slice is fundamentally correct.** Slicing the shaped mesh horizontally and
   sampling its perimeter gives the right placement positions and the right outward
   directions, with no parameterization errors.  The approach was a genuine insight.

2. **Analytical contact angle.** The closed-form solution is exact within the sphere
   approximation.  The cache (keyed on radius + leaf geometry) gives O(1) amortized
   cost.  Performance is now acceptable.

3. **solidify_leaf raycasts are reliable.** Given a contact-angle-tilted leaf, the
   perimeter vertices are close to the cluster surface and all raycasts hit.  The
   wall geometry correctly extends from the leaf perimeter to the cluster skin.

4. **Underside filter is conservative and correct.** `outward[2] < -0.1` is a
   10% downward tolerance — leaves near-horizontal are still placed.  The filter
   applies only to truly downward-facing surfaces (the hidden underside behind the
   branch).

5. **The apex cap (world-Z argmax) is now correct.** It targets the right vertex,
   computes the contact angle from `r_tip` (the full dome radius, correct for apex
   where `local_r → 0`), and applies `lift_mm=0` so no upward lift curls the tip
   upward after tilt.

---

## What Is Open / Not Yet Resolved

### A. Jitter disabled

`jit = 0.0, pj = 0.0` was set for debug visibility.  The comment in the code says
"Deterministic placement — jitter disabled for clean visual debugging."  For a
printed tile this is probably fine — the leaves are small enough that regularity
doesn't stand out — but it was not a deliberate aesthetic choice; it was left off
because re-enabling it hadn't been verified as safe after all the Z-slice changes.

**Expected state:** re-enable `jit = leaf_angle_jitter_deg` and `pj = leaf_pos_jitter`
and verify there are no new spike artefacts (the RuntimeWarning instrumentation
will catch them).

### B. Lower-body coverage

The `outward[2] < -0.1` filter kills all Z-slice rows in the lower half of the cluster.
For a typical cluster spanning 14mm in Z with `row_step = 3.375mm`, only 3–4 rows
place leaves (the top ~13mm).  The lower-mid body of the cluster is bare.  This may
be acceptable (leaves only on visible surfaces, hidden underside bare) but has not
been explicitly evaluated.

### C. leaf.py dead code

~400–500 lines unused in production.  No risk, but maintenance burden.  The branchlet-
era functions (`place_leaf_on_mesh`, `_find_contact_angle_for_mesh`, `find_max_dip_for_sphere`,
`place_leaf_on_sphere`, `leaf_placement_from_surface`, `build_leaf_on_surface`) can be
removed or moved to `src/scripts/`.

### D. apex_ca fallback

```python
if apex_ca >= np.pi / 2:
    apex_ca = 0.0
```

This silently falls back to flat placement (no tilt) rather than skipping the apex
cap.  A cap leaf at `apex_ca = 0` will have spike-tendency if `lift_mm > 0`, but
the apex cap already sets `lift_mm=0`, so this is probably harmless.  Worth removing
the fallback and skipping the cap entirely when the formula is invalid.

### E. `group_width_mm` foliage cluster variation

The tree can have many different-sized foliage clusters depending on `group_width_mm`
and `foliage_bulge_mm`.  The leaf parameters are fixed per tree (`leaf_length_mm`,
`leaf_width_mm`, etc.).  On very small clusters (`r_tip < 2.5mm`), the contact angle
hits `≥ π/2` and leaves are skipped.  On very large clusters, the fixed leaf size
looks sparse.  Scaling leaf size with cluster radius would give a more consistent
appearance across clusters.

---

## Appendix: Commit Timeline (leaf-related only)

```
2026-06-16  a45f1ab  Add quarter-circle keel to cloud-tree leaves
2026-06-16  4f9fefa  Extract leaf.py; fix foliage clump offset
2026-06-17  7be6cee  Tile leaf coverage across clump surface; vectorise
2026-06-17  9682835  Increase leaf_pos_jitter 10%
2026-06-17  5716551  Tune leaf defaults
2026-06-17  97381f3  Add near-apex leaf ring; fix polar spike
2026-06-20  b38bf38  Add printable leaf branchlets (+ 23 branchlet-era commits)
  …(24 commits through c55490b, all branchlet work)…
2026-06-22  ebba32f  leaf: add leaf_placement_from_surface + build_leaf_on_surface
2026-06-22  1299b3e  leaf: add solidify_leaf + color_leaf_walls_by_fdm
2026-06-22  97d0f2f  leaf: add find_max_dip_for_sphere
2026-06-22  63b825c  leaf: fix 4 bugs from code review
2026-06-22  32439e3  leaf: fix 6 open items from 2026-06-22 code review
2026-06-22  fa743eb  docs: leaf placement debug pipeline code review
2026-06-22  3ac1038  leaf: rename dip → contact angle throughout
2026-06-22  e0c9af2  leaf: add place_leaf_on_sphere; simplify debug scripts
2026-06-22  4a4c382  leaf: replace lift z-offset with whole-leaf rotation
2026-06-22  b25b1ff  leaf: replace FDM tip heuristic with raycast find_tip_root
2026-06-22  c851fd8  leaf: identify tip by fixed vertex index, not lowest-Z
2026-06-23  b14c77d  Use solidified debug leaves for tree foliage
2026-06-23  4578ea5  Speed up debug leaf STL generation
2026-06-23  de6dca5  leaves: flat bottom row; fix apex bald spot with r_tip contact angle
2026-06-23  25073f0  trees: fix apex over-coverage + leaf-count diagnostic
2026-06-23  69238c8  Improve tree foliage top coverage
2026-06-23  31e5ff4  Update tree leaf cap mesh
2026-06-23  afbe43e  trees: replace leaf placement with Z-slice algorithm  ← REVOLUTION
2026-06-24  93990c7  trees: fix bald apex and blade-on-edge leaves
2026-06-24  00f3b92  trees: fix bald apex and improve foliage contact-angle accuracy
2026-06-24  605ae36  trees: fix upward-pointing leaves on foliage clusters  ← CURRENT
```

---

## Lessons for Future Leaf Work

1. **The Z-slice approach is the right substrate.** Don't replace it.  Any future
   improvement (jitter, density variation, lower-body leaves) should be *added to*
   the Z-slice, not an alternative to it.

2. **Branchlets/petioles are a dead end for foliage clusters at this scale.**
   The FDM constraint plus the cluster surface geometry means any stem that droops
   creates an undercut.  The contact-angle-tilted direct placement is the right model.

3. **The world-Z apex, not the branch apex, is the hard-to-cover point.** Always
   use `argmax(z)` for the apex cap.  Document this in code comments.

4. **Contact angle is a function of (radius, leaf_geometry) — cache it.**
   Do not binary-search it per-leaf.  Compute it analytically or via a cheap
   per-call function, always keyed on radius.

5. **Instrumentation should have been first.**  The `RuntimeWarning` on `tangent[2] > 0.707`
   was added in commit 605ae36 — after the Z-slice was working.  It should have been
   the very first thing added when the Z-slice was written, so every subsequent
   parameter tweak could be immediately tested.

6. **Re-enable jitter before declaring the algorithm done.**  The current output
   is deterministic.  That's a debugging state, not a production state.
