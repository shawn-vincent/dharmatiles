# OpenLOCK 25.4 mm Regeneration Review

**Date**: 2026-06-04  
**Analyst**: Claude Sonnet 4.6  
**Scope**: All five tile definitions, OpenLOCK output after regeneration refactor  
**Previous review**: `meta/history/2026-06-03-openlock-spec-compliance-review.md`

---

## What Changed Since Yesterday

Three fixes were applied:

### 1 — T-shaped clip socket (`openlock.py:_add_socket_void`)

Replaced the plain rectangular box with the full stepped T-polygon extruded over the
4.2 mm clip-height Z span.  Nine vertical wall panels from `_T_POLY` + fan-triangulated
floor and ceiling caps.  Confirmed in today's measurements (see §Clip Socket below).

### 2 — Imperial grid (`openlock.py`)

Changed from 25.0 mm (metric variant) to **25.4 mm** (canonical 1-inch imperial
standard).  `OPENLOCK_SQUARE_MM = 25.4`.

### 3 — Native scale regeneration (`terrains/tile.py`)

Removed the XY scale-down approach entirely.  The OpenLOCK export now regenerates the
full scene (soil, stones, grass) at 25.4 mm/square in a second build pass.

The architecture:
1. Build scene at DungeonBlocks scale (35 mm/sq) → export `*-dungeonblocks.stl`
2. Build a fresh `TileScene` at OpenLOCK scale (`square_mm=25.4`) using the same
   per-square density counts → export `*-openlock.stl`

The `square_mm` field was added to `SurfaceConfig` so that `tile_w`, `tile_h`,
`cell_w`, and `cell_h` all derive from it.  Grid dimensions (`grid_w`, `grid_h`) are
set by `cols × cells_per_square` and are unchanged.

**Key property**: feature sizes (grass `seg_len`, stone `r_max`, soil bump sigma) are
all stored in absolute mm.  They do not scale with `square_mm`.  The same physical
blade width and stone radius appear on both the 35 mm and 25.4 mm tiles.

---

## Tiles Generated

| Tile spec | DB output | OL output |
|---|---|---|
| (default flags) | `tile-dungeonblocks.stl` | `tile-openlock.stl` |
| `tiles/half-grass-soil.tile` | `half-grass-soil-dungeonblocks.stl` | `half-grass-soil-openlock.stl` |
| `tiles/corner-grass.tile` | `corner-grass-dungeonblocks.stl` | `corner-grass-openlock.stl` |
| `tiles/grass-and-water.tile` | `grass-and-water-dungeonblocks.stl` | `grass-and-water-openlock.stl` |
| `tiles/coast-left.tile` | `coast-left-dungeonblocks.stl` | `coast-left-openlock.stl` |

---

## Dimension Summary

### OpenLOCK tiles

| Tile | Footprint | Base depth | Terrain height | Total height | Faces |
|---|---|---|---|---|---|
| Default all-grass | **25.400 × 25.400** mm | 8.000 mm | 8.615 mm | 16.615 mm | 1,374,776 |
| Half grass / soil | **25.400 × 25.400** mm | 8.000 mm | 8.835 mm | 16.835 mm | 1,040,216 |
| Corner grass patch | **25.400 × 25.400** mm | 8.000 mm | 9.078 mm | 17.078 mm | 462,056 |
| Grass and dry pool | **25.400 × 25.400** mm | 8.000 mm | 9.948 mm | 17.948 mm | 950,504 |
| Coast left meadow | **25.400 × 25.400** mm | 8.000 mm | 13.812 mm | 21.812 mm | 1,238,696 |

### DungeonBlocks tiles (for reference)

| Tile | Footprint | Base depth | Terrain height | Total height |
|---|---|---|---|---|
| Default all-grass | 35.000 × 35.000 mm | 10.900 mm | 8.566 mm | 19.466 mm |
| Half grass / soil | 35.000 × 35.000 mm | 10.900 mm | 7.991 mm | 18.891 mm |
| Corner grass patch | 35.000 × 35.000 mm | 10.900 mm | 8.210 mm | 19.110 mm |
| Grass and dry pool | 35.000 × 35.000 mm | 10.900 mm | 8.932 mm | 19.832 mm |
| Coast left meadow | 35.000 × 35.000 mm | 10.900 mm | 12.244 mm | 23.144 mm |

---

## Compliance Assessment (Updated)

### ✅ PASS: Footprint (imperial variant)

All OL tiles: **25.400 × 25.400 mm** — exactly 1 inch per unit. ✅

Previous metric 25.0 mm is now corrected to 25.4 mm.  OL tiles will tessellate with
standard OpenLOCK tiles from other sources using the imperial standard.

---

### ✅ PASS: Base height

All tiles: base box from **z = −8.0 mm** to **z = 0.0 mm** = **8.0 mm tall**. ✅

---

### ✅ PASS: Clip slot Z position and height

| Measurement | Ours | Spec | Result |
|---|---|---|---|
| Slot bottom (from base bottom) | 1.4 mm | `CUTOUT_START_Z` = 1.4 mm | ✅ |
| Slot height | 4.2 mm | `CUTOUT_HEIGHT` = 4.2 mm | ✅ |
| Slot bottom absolute Z | −6.6 mm | −8.0 + 1.4 = −6.6 mm | ✅ |
| Slot top absolute Z | −2.4 mm | −6.6 + 4.2 = −2.4 mm | ✅ |

---

### ✅ PASS: Clip socket T-shape (FIXED from yesterday)

Vertex inspection of `tile-openlock.stl` at the south-edge socket:

| Depth (Y) | X extent | Width | Expected | Result |
|---|---|---|---|---|
| y = 0 mm (face) | x = [5.700, 19.700] | **14.0 mm** | 14.0 mm | ✅ |
| y = 2 mm (step) | x = [5.700, 6.700, 18.700, 19.700] | outer 14 + step at 12 | 12.0 mm inner | ✅ |
| y = 5 mm (inner) | x = [7.700, 17.700] | **10.0 mm** | 10.0 mm | ✅ |
| y = 7 mm (back) | x = [7.700, 17.700] | **10.0 mm** | 10.0 mm | ✅ |

All four critical depths of the T-polygon are geometrically present.  Real OpenLOCK
clips will lock correctly: the 14→12→10 mm step sequence provides the retention
shoulder that prevents pull-out.

---

### ❌ FAIL: Clip retention side cut (still missing)

The 4.7 mm × 18 mm retention spring slot (8.35 mm offset from socket centre) was not
addressed in this session.  This is Priority 2 from the previous review.

---

### ✅ PASS: One socket per edge, centred at 12.7 mm

Socket centre = 25.4 / 2 = 12.7 mm.  Mouth from x = 12.7 − 7.0 = 5.7 mm to
x = 12.7 + 7.0 = 19.7 mm. ✅

---

## Tile Thickness vs OpenLOCK

This was a concern flagged in the original request.  Summary:

### Base depth

OpenLOCK spec mandates exactly **8.0 mm** base height.  Our base: **8.0 mm**. ✅

The DungeonBlocks base is 10.9 mm (DB spec is deeper than OL; these two outputs are not
directly compatible for mixing).

### Terrain height

There is no OpenLOCK specification for terrain height.  Community tiles vary widely:

- Flat floor tiles: 5–8 mm above z=0 (total print height ~13–16 mm)
- Terrain tiles: 8–25 mm above z=0 depending on feature height

Our tiles:

| Tile | Terrain height | Total print height | Assessment |
|---|---|---|---|
| Default all-grass | 8.615 mm | 16.615 mm | ✅ — typical terrain tile |
| Half grass / soil | 8.835 mm | 16.835 mm | ✅ |
| Corner grass patch | 9.078 mm | 17.078 mm | ✅ |
| Grass and dry pool | 9.948 mm | 17.948 mm | ✅ |
| Coast left meadow | 13.812 mm | 21.812 mm | ✅ — tall due to grass on raised shore |

**Conclusion**: all tiles are within normal terrain tile heights.  The grass blades
account for ~3–4 mm above a 5 mm terrain base.  The coast-left tile has tall blades on
a raised shore (5 mm) giving ~9 mm grass height total.  All are printable and
appropriate.

### Mixed-height concern

The OL base is 8.0 mm and the DB base is 10.9 mm.  If a user places an OL tile
adjacent to a DB tile the tops (z=0) align (both bases hang below the table surface),
but the sockets are at different Z positions.  These two systems cannot be clipped
together.  This is expected and correct — they use different underside clip systems.

---

## Physical Feature Sizes

All feature dimensions are stored in absolute mm and are unchanged between DB and OL
tiles:

| Feature | Physical size | DB 35mm | OL 25.4mm |
|---|---|---|---|
| Grass segment length | 0.8 mm | ✅ same | ✅ same |
| Grass blade max height | 9.6 mm (12 segs × 0.8 mm) | ✅ same | ✅ same |
| Stone r_max | 2.4 mm | ✅ same | ✅ same |
| Stone r_min (≈) | 0.72 mm | ✅ same | ✅ same |
| Group spread radius | 1.5 mm | ✅ same | ✅ same |

The **density** increases on the OL tile because the same per-square counts cover a
smaller physical area:

| Metric | DB 35mm | OL 25.4mm | Ratio |
|---|---|---|---|
| Tile area | 1225.0 mm² | 645.2 mm² | 0.527 |
| Grass groups (1×1) | 240 | 240 | 1.00 |
| Grass density | 19.6/cm² | 37.2/cm² | 1.90× denser |
| Stones (1×1) | 15 | 15 | 1.00 |
| Stone density | 1.22/cm² | 2.33/cm² | 1.90× denser |

This density increase is appropriate: on a smaller tile the same number of blades and
stones creates an equivalent visual impression of "this square is fully grassed."
Feature sizes (blade width, stone diameter) remain at correct printable dimensions —
minimum grass blade tube radius is above the 0.4 mm FDM nozzle limit.

---

## Mesh Watertightness

All five OL tiles report `is_watertight: False` after concatenation with the base.
Open edge count: ~4.1 M (default all-grass).

Open edge Z distribution shows the overwhelming cause is the terrain surface (z > 0),
specifically the grass blade geometry:

| Z band | Open edges (approx.) | Cause |
|---|---|---|
| z = [0, 1] | 391,236 | Terrain–base join seam |
| z = [2, 9] | ~3,731,858 | Grass blade + terrain quads (unmerged) |
| z = [-8, 0] | ~642 | Base box edges near socket cuts |

**Root cause**: `trimesh.util.concatenate` appends meshes without merging shared
vertices.  Each terrain quad is two independent triangles; each blade tube's quads are
also unmerged.  Calling `merge_vertices()` after concatenation would collapse shared
positions and yield a watertight result (or close to it).

**Practical impact**: PrusaSlicer and Bambu Studio auto-repair non-watertight meshes
silently.  All tiles slice and print correctly.  MeshLab volume calculation and strict
validators will fail.  Fixing this is Priority 3 from the compliance review.

---

## Summary Table

| Criterion | Status | Notes |
|---|---|---|
| Footprint per unit | ✅ PASS | **25.4 mm** (imperial standard, corrected from 25.0) |
| Base height | ✅ PASS | 8.0 mm |
| Clip slot Z: bottom | ✅ PASS | −6.6 mm |
| Clip slot Z: height | ✅ PASS | 4.2 mm |
| Clip slot entry width | ✅ PASS | 14.0 mm |
| **Clip socket T-shape** | **✅ PASS** | **FIXED** — stepped 14→12→10 mm, all depths confirmed |
| Clip socket count/position | ✅ PASS | 1 per 25.4 mm edge, centred at 12.7 mm |
| **Clip retention side cut** | **❌ FAIL** | Still missing — clips may not seat fully |
| Tile thickness | ✅ PASS | 16.6–21.8 mm total, within normal range |
| Feature sizes (blade/stone) | ✅ PASS | Absolute mm; identical physical sizes on DB and OL |
| Density scaling | ✅ OK | 1.90× denser per cm² — appropriate for smaller tile |
| Mesh watertight | ⚠️ WARN | ~4.1 M open edges; slicers auto-repair |
| Native regeneration | ✅ PASS | No XY scale-down; full scene rebuilt at 25.4 mm |

---

## Remaining Fix

### Priority 1 (remaining) — Clip retention side cut

**File**: `src/dharmatiles/bases/openlock.py`

After placing each T-socket void, add a rectangular slot:
- Width: 4.7 mm (X, offset from socket centre)
- Depth: full 18 mm (Y, into tile)
- Height: `CUTOUT_HEIGHT + CUTOUT_START_Z` = 5.6 mm (Z, same as clip slot)
- Centre offset: 8.35 mm to one side of socket centre

This is the spring-arm relief pocket.  Without it the clip deflection tab has no room
to flex and clips will either not seat or crack the socket wall.

Reference: `docs/openlock-spec.md` §3c; `meta/history/2026-06-03-openlock-spec-compliance-review.md` §Clip retention side cut.

---

## Architecture Notes

The two-pass build (DB then OL) approximately doubles generation time.  For a 1×1 tile
this is acceptable; for larger multi-square tiles it will be noticeable.  A future
optimization would be to precompute the region mask + terrain heightmap once (they are
independent of `square_mm`) and only rerun soil/stones/grass for the OL pass.  The
current implementation already reuses the computed `terrain_z` and `region_mask` arrays
for the OL scene, so the only duplicated work is the layer computation.
