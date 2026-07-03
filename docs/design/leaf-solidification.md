# Leaf Solidification — Design Specification
*Rewritten 2026-07-03.  `solidify_leaf` is now a pure index-stitch closure
against a pre-computed root oval — no raycasts, no per-leaf `fix_normals`.
The original raycast-wall design (boundary-loop walk, per-vertex ray
directions, cone framework, tip bulk vertex) was retired during the
2026-07-01 performance crisis and the greedy/organic placement work; see
"Historical designs" at the end for pointers.*

*Placement — where a leaf goes and how its frame is seated — is covered in
`docs/design/leaf-placement.md`.  This document covers the leaf solid itself.*

---

## Overview

A leaf solid is built from three pieces, all sharing one vertex layout:

1. **`build_leaf_surface`** — an open trimesh (disk topology) for the visible
   top surface of the blade: ovate outline, V-crease midrib, arch/curl/lift
   longitudinal profile, dome-shaped lobes.
2. **`build_leaf_oval_offsets`** — the root oval: a flat embedded copy of the
   same vertex layout, positioned inside the parent cluster.
3. **`solidify_leaf`** — stitches surface + oval + perimeter walls into a
   watertight solid, by index, with no geometric queries.

The caller (`placement_leaf._attempt_leaf`) builds the blade and the oval in
the SAME rigid frame — shared origin, growth direction, and length by
construction — so the 1:1 stitch produces a short tapered neck everywhere.
Seating that frame against the real mesh (equal-depth oval tilt, belly-dip
drop, skew, tucks) happens before solidification and is placement's job.

Implementation: `src/dharmatiles/trees/leaf.py`.

---

## Stage 1 — Leaf Surface (`build_leaf_surface`)

`build_leaf_surface` calls `compute_leaf_geometry` to get all vertex arrays,
then assembles them into a single open `trimesh.Trimesh`.

### Vertex layout

| Block | Count | What |
|---|---|---|
| `upper_grid` (flattened) | `(N_LONG−1) × (N_LAT+1)` | Top face vertex grid, ring-major |
| `base_pt` | 1 | Attachment point (s=0) |
| `tip_pt` | 1 | Pointed tip (s=1) |

`_LEAF_N_LONG = 12`, `_LEAF_N_LAT = 10`.  There are 11 interior rings
(`N_LONG − 1`) each with 11 vertices (`N_LAT + 1`), plus 2 pole vertices =
**123 vertices** total.  `base_pt` is at index `ring_count × ring_stride`
(= 121) and `tip_pt` at 122.

This layout is load-bearing: the oval mirrors it index-for-index, and
placement code addresses canonical vertices (belly dip = tip-half midrib
column, tip = last index) directly through it.

### Face connectivity

- **Base fan** (`N_LAT` triangles): `[base_pt, ring0[j], ring0[j+1]]`.
- **Body quads** (`(N_LONG−2) × N_LAT × 2` triangles): two triangles per quad
  cell between adjacent rings.
- **Tip fan** (`N_LAT` triangles): `[tip_pt, last_ring[j+1], last_ring[j]]`.

Face normals point outward from the top surface (the crease/dome side).

### Winding cache

`fix_normals()` is called once per topology key and the corrected face array
is cached (`_FACE_CACHE`).  All subsequent leaves of the same topology reuse
the cached winding — this saves ~12k `fix_normals()` calls per tile and is
the dominant leaf-generation performance optimisation.  The cache key
includes `curl_deg` (winding differs between flat-tip and curled-tip
geometry) but NOT `lift_mm` (lift rotates all vertices uniformly).

### Shape parameters

`length_mm`, `width_mm`, `thickness_mm`, `fold_angle_deg`, `inner_curve`,
`outer_curve`, `arch_deg`, `curl_deg`, `lift_mm`, `seed`.  The organic placer
zone-blends `curl_deg` and `lift_mm` per leaf and applies its underside
end-to-end arch as a post-build vertex offset (see the placement spec).

---

## Stage 2 — Root Oval (`build_leaf_oval_offsets`)

The root oval is the embedded anchor the blade stitches down to.  It is
returned as **offsets relative to the leaf frame origin** — an
`(n_outer, 3)` array with `n_outer = 123`, the exact vertex count and index
layout of the open surface — and translated to world space by the caller.

Geometry (in the leaf frame: `n_hat` outward normal, `T_along` growth
direction, `across` lateral):

- **Half-size, bottom-aligned**: the oval spans `[L/2, L]` along `T_along` —
  the tip half of the leaf's footprint — so it is half the leaf length and
  aligned with the leaf at the TIP.  The blade's base half tapers inward to
  the oval's near end; the tip ends coincide (in s).
- **Width**: `(W/4)·sin(π·s)` per ring — half the blade's width scale.
- **Depth**: every vertex is displaced `−embed_mm · n_hat`
  (`embed_mm = _ROOT_EMBED_MM = 0.75`), putting the whole oval a fixed
  shallow depth below the surface point it was seated against.
- The two pole slots (base/tip) sit at `L/2` and `L` along `T_along`, same
  depth.

Because the oval has the surface's exact index layout, ring `i` / column `j`
of the blade connects to ring `i` / column `j` of the oval — there is no
correspondence search and no possibility of crossed walls.

The oval is *flat* in its own frame; conforming it to the curved, noised
cluster surface is done rigidly by placement (`_seat_oval_tilt` pitches the
whole frame about the oval center until both ends sit equally deep).

---

## Stage 3 — Solidification (`solidify_leaf`)

```python
solidify_leaf(surface, inner_v) -> (solid, range(0, 0))
```

`inner_v` is the world-space oval (123 vertices).  The solid is assembled
purely by index:

1. **Vertices**: `[outer 123 | inner 123]` — the blade surface then the oval.
   (Downstream code relies on this: the organic placer's branch-collision
   cull tests `solid.vertices[:len//2]`, the blade half.)
2. **Outer faces**: the surface's own faces.
3. **Inner faces**: the same face array `+ n_outer`, winding reversed — the
   oval reuses the blade's topology as its underside.
4. **Wall faces**: the perimeter loop (base_pt → left lateral edge → tip_pt →
   right lateral edge → back; `NP = 24` vertices) is stitched 1:1 to the
   corresponding oval perimeter — two triangles per perimeter edge, 48 wall
   triangles.

No raycasts, no `fix_normals`, no proximity queries — the result is
watertight by construction (`process=False`).  The returned face range is
empty, kept for API compatibility.

### Why the walls are safe without any geometric query

Everything the raycast machinery used to establish per-leaf is now
guaranteed upstream, once, by placement:

- **Walls reach real material** — the oval is embedded `_ROOT_EMBED_MM`
  below the *real noised* surface at a seated frame (equal-depth solve), and
  the oval-containment guard culls any leaf whose oval ends poke out.
- **Walls stay short** — blade and oval share a frame, so wall length is the
  analytic taper value plus whatever the skew/standoff added; the neck gate
  culls leaves whose stitch would stretch past 1.8 mm.
- **Walls print** — the printability skew slides the blade until its tip
  clears the oval tip in world z, so tip-end walls always climb upward.

---

## Constants Summary

| Constant | Value | Location | Meaning |
|---|---|---|---|
| `_LEAF_N_LONG` | 12 | `leaf.py` | Longitudinal sections (base→tip) |
| `_LEAF_N_LAT` | 10 | `leaf.py` | Lateral sections (must be even) |
| `_ROOT_EMBED_MM` | 0.75 mm | `placement_leaf.py` | Oval depth below the real surface |
| `_PROTRUSION_MM` | 0.3 mm | `placement_leaf.py` | Blade closest-vertex standoff |
| `_SKEW_TIP_MARGIN_MM` | 0.05 mm | `placement_leaf.py` | Tip-over-oval z margin |

---

## Historical designs (retired)

The 2026-06-29 version of this document specified a raycast-wall closure:
walk the open surface's boundary loop, fire a ray from each perimeter vertex
into the parent mesh (direction chosen inside a three-cone constraint
framework), and place each root vertex `embed_mm` past its hit, with a "tip
bulk vertex" special case for the tip-pole ray that never hit.  That design
solidified each leaf against the parent mesh *at solidification time* —
thousands of per-leaf raycasts, per-leaf `fix_normals`, and a family of
miss/fallback artifacts (stub walls, spike vertices, the tip-pole universal
miss).

It was replaced in two steps: the 2026-07-01 perf crisis removed
deconfliction and `fix_normals` from `solidify_leaf` (commit c828c2d), and
the greedy-placement work moved all mesh interrogation into the seating of a
rigid blade+oval frame (commits 7f9e3ce, 95d43d8), leaving solidification
purely combinatorial.

The full raycast-era spec is preserved in git history
(`git log -- docs/design/leaf-solidification.md`, versions ≤ 2026-06-29) and
its diagnostic story in
`docs/meta/history/2026-06-29-leaf-jitter-and-placement-fixes.md` and
`docs/meta/history/2026-07-01-leaf-deconfliction-and-performance-crisis.md`.
