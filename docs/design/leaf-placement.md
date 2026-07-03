# Leaf Placement — Specification
*Rewritten 2026-07-03.  The **organic union-surface placer** is the only leaf
generator.  The three earlier placers (meridian-arc, greedy lowest-first,
shoots) were deleted in commit `73e24f1`; their shared per-leaf machinery was
distilled into `placement_leaf.py`.*

*Requirements source: `docs/meta/history/2026-07-02-foliage-greenfield-requirements.md`.
Complete development history (all four placers, every iteration, why organic
won): `docs/meta/history/2026-07-03-leaf-placement-complete-history.md`.*

---

## What This Covers

How individual leaves are placed on foliage clusters in the tree generator.
Foliage clusters are the bumpy green blobs at the tip of each terminal branch.
Leaves are separate 3D geometry (watertight solids) seated on the cluster
surface.

Implementation files:
- `src/dharmatiles/trees/placement_organic.py` — the placer:
  `place_leaves_organic`, union surface, normal-aware Poisson grid, direction
  field, shingle standoffs, branch-collision cull.
- `src/dharmatiles/trees/placement_leaf.py` — shared per-leaf machinery:
  `LeafPlacementStats`, `_attempt_leaf` (the full seat → build → cull
  pipeline for one leaf), `_seat_oval_tilt`, `_leaf_frame_and_oval`, surface
  sampling/projection helpers, seat constants.
- `src/dharmatiles/trees/leaf.py` — leaf geometry primitives:
  `build_leaf_surface`, `build_leaf_oval_offsets`, `solidify_leaf`
  (see `docs/design/leaf-solidification.md`).
- `src/dharmatiles/trees/mesh.py` — the single call site: all clusters of a
  tree are placed in ONE `place_leaves_organic` call, with the branch tubes
  passed as `avoid_meshes`.

---

## Requirements (why the algorithm looks like this)

From the 2026-07-02 greenfield interview, after all three prior placers were
judged unacceptable on renders:

1. **Total coverage** — no bare patches on any surface above the FDM
   underside limit.  This was the #1 failure of every prior attempt.
2. **Organic arrangement** — no visible rows, rings, bands, or grids.
3. **Overlap with distinct heights** — overlapping leaves must sit at
   visibly different standoffs from the substrate so each reads as its own
   leaf.  Overlap density is one knob (moderate ~50% → light ~15%).
4. **Union seams** — the multi-cluster canopy is tiled as ONE surface;
   leaves flow through inside corners; no leaf buried in a neighbour.
5. **Direction** — down-slope with smooth (non-i.i.d.) ±variation.
6. **Undersides** — no discrete hanging leaves below the printability floor.
7. **Supportless FDM**, drybrush-friendly relief.
8. **Performance** — < 5 s leaf placement on a full tree (~30 clusters); no
   per-leaf mesh scans; embree only; cheap-reject before every build.

Style target: Animal Crossing (New Horizons) trees — large discrete chunky
leaves densely shingling blobby cluster masses — but with the visible banding
replaced by blue-noise placement.

---

## Algorithm Overview

```
cluster solids (real, noised)
        │  boolean union (manifold)
        ▼
union placement surface  ──►  area-weighted candidate over-generation
        │                     (~10 × area / spacing²)
        ▼
normal-aware maximal Poisson-disk dart throw  ──►  roots (+ coverage verify)
        │
        ▼
per-root: source-cluster attribution (batched contains)
          growth direction  = down-slope ⊕ positional angle field
          shingle standoff  = height-sorted accumulation over tip conflicts
          zone factor st    = smoothstep(surface normal z)
        │
        ▼
per-root _attempt_leaf:  equal-depth oval seat → rigid blade↔oval frame
          → oval containment guard → build blade (curl/lift/arch by zone)
          → printability skew → belly-dip seat → standoff lift → neck gate
          → base tuck → tip tuck → burial cull (with bury-lift) → solidify
        │
        ▼
branch-collision cull (blade inside an EXPOSED branch tube)
        │
        ▼
watertight leaf solids, attributed per source cluster + stats
```

### 1. Union placement surface

The (noised) cluster solids are boolean-unioned into ONE mesh
(`trimesh.boolean.union`, manifold engine).  All placement queries —
sampling, normals, seating, containment — run against it.  Inside corners
between clusters become ordinary creases of the union; "no leaf buried in a
neighbour" is free because there are no neighbours.  This is the single move
that dissolved the cross-cluster seam problem all prior placers fought
reactively.

### 2. Coverage by normal-aware maximal Poisson-disk

Candidates are over-generated area-weighted on the union surface
(`_ORGANIC_CANDIDATE_FACTOR = 10` × area / spacing²), with smooth barycentric
vertex normals, then dart-thrown to saturation against an exact-distance
grid.  Maximality ⇒ every placeable surface point lies within one spacing
radius of an accepted root ⇒ **no bare patch wider than 2×spacing, by
construction** — coverage is guaranteed by the sampling scheme, not tuned.

**The grid is normal-aware** (`_root_blocked_n`).  A plain Euclidean disk
test fails at union seams: the two walls of a seam-V are only 1–2 mm apart
*through space*, so a root on one wall blocks candidates on the opposite wall
and every crease grows a bald band beside it.  Each root stores its surface
normal; a candidate conflicts only with roots whose normal agrees
(`dot > _ROOT_BLOCK_COS = 0.26`, ≈ 75°).  Opposite V-walls never block each
other; same-wall neighbours always do.

**Spacing is THE overlap knob**: `spacing = _ORGANIC_SPACING_FRAC × leaf_width`
(0.45 = moderate AC-style shingle, ~0.8 = light touch).

**Coverage verification** runs every placement: fresh samples on the union
surface (one per `_ORGANIC_VERIFY_RES_MM²`), each of which must be covered by
a root under the same normal-aware rule; the `uncovered-test-pts` count is
printed in the placement summary line.

Surfaces with normal z below `_ORGANIC_PLACEABLE_NORMAL_Z = −0.75` (deep
undersides) are excluded — designed bare, per requirement 6.

### 3. Anchor re-projection (roots ≠ anchors)

The build machinery anchors the root oval's tip-half at its anchor point, so
the blade spans 0.75·L up-slope but only 0.25·L down-slope of it — a 75/25
bias that leaves bare bands wherever "up-slope" exits a face (e.g. over a
crest).  Each root's anchor is therefore re-projected 0.25·L down-slope
(`_project_to_surface`, ray drop from a 1 mm lift) so the blade covers ±L/2
around the ACTUAL root and Poisson maximality over roots translates into
visual coverage.  The drop budget is tight (~1 mm past the offset): a
projection that falls further has crossed a crease onto a different wall, and
anchoring there produces long-neck extrusions.

### 4. Direction field

Growth direction is steepest-descent (world-down projected onto the tangent
plane; radial-outward fallback at the apex, `_growth_tangent`), rotated by a
smooth positional angle field (`_direction_field_angle`): three positional
sines with hash-derived phases, wavelength `_ORGANIC_DIR_WAVELEN_MM = 7`,
peak deviation `_ORGANIC_DIR_VAR_DEG = 15°`.  Neighbouring leaves deviate
*together* — coherent variation, not i.i.d. jitter, not combed.  15° (down
from the requirement's ±25°) because divergent neighbours were the main
source of blade sheets slicing through each other.

### 5. Blade zones — continuous blend down the canopy

A smoothstep zone factor `st` runs over the surface-normal z from 1 on
clearly-upward faces (`nz ≥ _ORGANIC_ZONE_HI = +0.30`) to 0 on undersides
(`nz ≤ _ORGANIC_ZONE_LO = −0.45`):

| Property | scales with | upward face (st=1) | underside (st=0) |
|---|---|---|---|
| curl | st | min(leaf_curl_deg, 32°) | 0 |
| tip lift | st | 1.2 mm | 0 |
| shingle standoff | st | full | 0 |
| tip-clearance ceiling | st | 0.02 + 2.4 mm | touching (0.02 mm) |
| end-to-end arch | 1−st | 0 | 0.8 mm |
| printability skew | st < 0.5 skips | applied | skipped |

The pitched blade (upward faces) presents its face with an exaggerated
upturned tip at the crown; the flush blade (undersides) is a pure end-to-end
**arch** — a parabolic 4·s·(1−s) rise, both ends touching/tucked into the
clump.  An arch can never re-enter the surface between its endpoints (curl
always could, burying mid-blades in convex underside lobes), and a blade
lying on the substrate inherits its printability, so no skew is needed.
Because everything scales with `st`, curl and tip height fade *gradually*
down the tree, reaching the fully surface-embedded arch blade exactly where
overhangs would begin.

### 6. Overlap layering — height-sorted shingle standoff

`_shingle_standoffs` processes leaves bottom-up by root z; each leaf stands
`_ORGANIC_SHINGLE_STEP_MM = 0.3` above the tallest already-processed leaf it
conflicts with, capped at `_ORGANIC_SHINGLE_CAP_MM = 1.2` (+0.05 mm jitter).
At the cap, ties are resolved by the tip lift (a tip always clears a base).
A higher-rooted leaf can never sit under a lower-rooted one — the
upper-over-lower guarantee is by construction, not statistical.

**Conflicts are point-end only**: a conflict exists when one leaf's TIP comes
within `_ORGANIC_TIP_CONFLICT_MM = 1.5` of the other leaf's base-end, centre,
or tip.  Base-to-base adjacency is harmless nestling and must NOT propagate
height, or the escalation runs away and every stitch neck stretches (the
"long-neck chains" of iteration 4/6).

The standoff lifts the finished blade along the seat normal AFTER the
belly-dip seat — the root oval stays plugged; the stitch walls stretch.

### 7. Per-leaf build (`_attempt_leaf` in placement_leaf.py)

The full pipeline for one leaf, shared constants at top of module
(`_ROOT_EMBED_MM = 0.75`, `_PROTRUSION_MM = 0.3`, `_SKEW_TIP_MARGIN_MM = 0.05`):

1. **Equal-depth oval seat** (`_seat_oval_tilt`): the rigid root oval
   (half-size, bottom-aligned — spans [L/2, L] of the leaf frame) is centered
   at `candidate − embed·n̂` and pitched about its own center by an iterated
   split-the-difference Newton step (≤3 iterations) until both ends sit
   equally deep below the real noised surface, measured by embree rays.  The
   mesh is only ever asked "how deep is this point?", never "where should
   this point be?".  Failure (ray miss / tilt > 60°) falls back to a FLAT
   seat for the organic placer (`seat_fallback_flat`) — a coverage hole is
   worse than an imperfect seat.
2. **Rigid frame** (`_leaf_frame_and_oval`): blade and oval are built in the
   same frame — shared origin, direction, and length by construction — so the
   1:1 index stitch in `solidify_leaf` yields a short tapered neck
   everywhere.  Placing the oval is the primary act; the blade comes along
   for the ride.
3. **Oval containment guard**: both oval ends must be inside the union.
4. **Build blade** (`build_leaf_surface`) with the zone-blended kwargs, then
   apply the end-to-end **arch** ((1−st) × 0.8 mm parabolic offset along the
   seat normal) for flush blades.
5. **Printability skew** (pitched blades only): slide the whole blade
   in-plane toward the base until its tip clears the oval tip in world z by
   `_SKEW_TIP_MARGIN_MM` — otherwise the tip-end stitch walls overhang
   downward (FDM-unprintable).  Culled if the slide exceeds
   `_ORGANIC_MAX_SKEW_FRAC = 0.35 × L`.
6. **Adaptive belly-dip seat**: locate the blade's canonical closest-approach
   vertex (tip-half midrib vertex or tip with smallest normal displacement)
   and translate the blade along ±normal so it sits exactly `_PROTRUSION_MM`
   off the real surface (one contains probe + one ray).  Positive drops are
   capped by the tip's remaining z-clearance so the skew guarantee holds.
7. **Standoff lift** (the shingle layer, § 6).
8. **Neck gate**: reject if `hypot(skew, net normal standoff)` exceeds
   `_ORGANIC_MAX_NECK_MM = 1.8` — stretched stitch walls read as chimneys/fans
   ("long-rooted leaves").
9. **Base tuck** (`tuck_base`): pitch the blade about its belly dip until the
   BASE end sits slightly embedded (target −0.2 mm).  The base was each
   blade's high point; a proud base pokes over an upper leaf's low tip
   regardless of standoff order.  An embedded base can never overlie
   anything, and anchors better.
10. **Tip tuck** (`tuck_tip`): pitch about the anchored base until the tip
    floats no more than the zone-blended ceiling (`0.02 + st × 2.4 mm`).  On
    undersides this pulls the tip down to touching — kills FDM
    floating-island tips; both blade ends are rooted, nothing hangs.
11. **Burial cull with bury-lift** (skipped for tuck_tip blades, whose ends
    are intentionally buried): tip + lowest-curl-vertex probes must be
    outside the mesh.  Per-probe classification: same-wall burial (exit
    normal ≈ seat normal, e.g. a tip curling under a dome crown) lifts out by
    measured depth, capped at 0.8 mm with the remainder kept tucked;
    oblique-wall burial (union-seam inside corner, exit normal > 50° off) is
    the desired tuck — kept; unmeasurable contains() grazes are kept.
12. **Solidify** (`solidify_leaf`) — outer blade + mirrored oval + perimeter
    wall stitch = watertight solid.

### 8. Branch-collision cull

`avoid_meshes` (the wood tubes, passed by the `build_branch_mesh` call site):
a leaf is culled when any blade-surface vertex lies inside a branch tube AND
outside the canopy union — the visible-skewer case.  Intersections with
branches running INSIDE the canopy are invisible and kept: culling them holed
the skin over every under-canopy branch (~20 % of all leaves on the test
tree; the exposed-only rule culls the handful at wood–canopy junctions).

### 9. Attribution, stats, determinism

Leaves are attributed to the source cluster whose solid their root sits on
(batched `contains` per cluster — per-leaf attribution was the full-tree hot
spot).  Each cluster gets a `LeafPlacementStats`; the summary line prints
placed/roots, spacing, build-fail, branch-cull, and uncovered-test-pts.
Everything is hash-seeded (`_hash01`) — placement is deterministic per tree
seed.

---

## Constants Reference

Organic module constants (`placement_organic.py`; promote to `Tree(...)`
config when the look settles):

| Constant | Value | Meaning |
|---|---|---|
| `_ORGANIC_SPACING_FRAC` | 0.45 | Root spacing / leaf width — THE overlap knob |
| `_ORGANIC_CANDIDATE_FACTOR` | 10.0 | Candidate over-generation for maximality |
| `_ORGANIC_ZONE_HI / _ZONE_LO` | +0.30 / −0.45 | smoothstep zone bounds on normal z |
| `_ORGANIC_PLACEABLE_NORMAL_Z` | −0.75 | below → bare underside |
| `_ORGANIC_TIP_LIFT_MM` | 1.2 | pitched-blade tip lift (crown effect, doubled 43a2d51) |
| `_ORGANIC_PITCH_CURL_DEG` | 32.0 | curl cap for pitched blades (doubled 43a2d51) |
| `_ORGANIC_TIP_CEIL_RANGE_MM` | 2.4 | tip-clearance ceiling range (doubled 43a2d51) |
| `_ORGANIC_FLUSH_ARCH_MM` | 0.8 | underside arch mid-span rise |
| `_ORGANIC_DIR_VAR_DEG / _WAVELEN_MM` | 15° / 7 mm | direction field |
| `_ORGANIC_SIZE_JITTER` | 0.2 | per-leaf downward-only size jitter |
| `_ORGANIC_SHINGLE_STEP/CAP_MM` | 0.3 / 1.2 | height-sorted standoff |
| `_ORGANIC_TIP_CONFLICT_MM` | 1.5 | point-end conflict radius |
| `_ORGANIC_MAX_NECK_MM` | 1.8 | stitch-wall neck gate |
| `_ORGANIC_MAX_SKEW_FRAC` | 0.35 | printability-skew cull (× L) |
| `_ROOT_BLOCK_COS` | 0.26 | normal-agreement threshold (≈75°) for the Poisson grid |
| `_ORGANIC_VERIFY_RES_MM` | 0.9 | coverage-verification sample density |

Seat constants (`placement_leaf.py`):

| Constant | Value | Meaning |
|---|---|---|
| `_ROOT_EMBED_MM` | 0.75 | oval embed below the real noised surface |
| `_PROTRUSION_MM` | 0.3 | blade closest-vertex standoff off the surface |
| `_SKEW_TIP_MARGIN_MM` | 0.05 | tip-over-oval z margin |
| `_PROJECT_LIFT_MM` | 1.0 | re-projection ray lift |

`Tree(...)` leaf parameters (layer.py):

| Parameter | Default | Effect |
|---|---|---|
| `leaves` | True | enable leaf placement (requires `foliage_clusters`) |
| `leaf_length_mm` | 4.5 | leaf length, base to tip |
| `leaf_width_mm` | 3.0 | peak width |
| `leaf_thickness_mm` | 0.24 | dome height |
| `leaf_fold_angle_deg` | 6.0 | midrib crease V-angle |
| `leaf_inner_curve / leaf_outer_curve` | 1.5 / 0.72 | Bézier shoulders |
| `leaf_curl_deg` | 40.0 | tip curl — capped at 32° by the placer, scaled by zone |
| `debug_leaf_color` | False | per-leaf debug palette colouring |

(The retired placers' parameters — `leaf_placement`, `leaf_lift_mm`,
`leaf_h_overlap`, `leaf_v_overlap`, `leaf_angle_jitter_deg`,
`leaf_pos_jitter`, `leaf_arc_meridians`, `leaf_arc_z_samples` — were removed
in 73e24f1.)

---

## Known Open Items

1. **Knob promotion** — the organic module constants (spacing/overlap above
   all) are still module-level; promote the settled ones to `Tree(...)`
   config.
2. **Underside relief (Phase C of the greenfield plan)** — the designed-bare
   below-floor zone could take a leaf-silhouette relief stamped into the
   cluster surface (GrassCarpet-style).  Unexplored.
3. **Sheet solidification (parked)** — replacing the oval+wall stitch with a
   thin sheet + root tab (`_ORGANIC_SHEET_*` constants, unwired) if the
   "pillow" read of the wall skirts ever matters again.
4. **Leaf instancing** — the leaf solid is fully rigid; a prefab library +
   rigid transforms would collapse per-leaf build cost if placement ever
   needs to be faster (from the Fable design review; not currently needed at
   ~6 s/tree).

---

## What Was Tried and Abandoned

Short version — the full story with root-cause analysis is in
`docs/meta/history/2026-07-03-leaf-placement-complete-history.md`:

- **Embossed surface relief** — leaf shapes as displacement.  "Looks like
  scales" (may return as the *underside* treatment, item 2 above).
- **Branchlet/petiole stems** — 24+ commits; FDM undercuts.
- **Z-slice rows with uniform dZ + apex cap** — bald zones near the dome
  apex; patched six times, never solved.
- **Meridian-arc rows** (`placement.py`, deleted) — correct arc-length row
  spacing and true surface normals; still a (φ,z) parameterization forced
  onto irregular unioned blobs; slow (~26 s/tree); judged ugly on render.
- **Greedy lowest-first accretion** (`placement_greedy.py`, deleted) —
  z-sorted dart throw; birthplace of the equal-depth oval seat, the rigid
  blade↔oval frame, the printability skew, and the graze translation, all of
  which survive in `placement_leaf.py`.  Coverage holes and pod-like reads
  persisted; judged ugly.
- **Shoots** (`placement_shoots.py`, deleted) — sprigs of 3–7 leaves in
  herringbone; birthplace of the exact-distance root grid and the adaptive
  belly-dip seat.  Judged ugly.
