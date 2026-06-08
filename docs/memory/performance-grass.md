---
name: performance-grass
description: Grass generation performance — current baseline 4.6 s for 1×1 tile (both systems)
metadata:
  type: project
---

## History

**fix_normals bottleneck (fixed 2026-06-04)**: Tile generation was dominated (98.7%) by
`mesh.fix_normals()` called once per blade mesh. Root cause: top cap winding was inverted
for the `circle` cross-section. One-line fix gave **9.6× speedup on grass, 8× overall
(80 s → 10 s)**.

**P1 rasterise (resolved by architecture change)**: The 2026-06-04 review identified a
`half_cell = 0.5 × min(cw,ch)` step-loop in the rasteriser making 121 K `np.meshgrid`
calls. This code no longer exists — the rasteriser was rewritten as `_contained_segment_cells`
+ `_stamp_segment` in `grass/_geometry.py`, which identifies swept-quadrilateral cells
directly with no spine-walking loop. The P1 fix is superseded.

**Blade vectorisation (2026-06-08)**: `_build_blade_mesh` face construction and
`_make_ring_verts` ring construction vectorised via NumPy broadcasting.
`distance_taper_vec` replaces per-point Python `math.*` loop.

## Current baseline (2026-06-08)

`generate-tile-stl --spec src/tiles/grass-and-water.tile` (grass + water + soil + stones,
both DB and OL passes, 256 cells/square):

**4.6 s wall clock** — within the REQ-PRF-1 target of < 5 s for a 1×1 tile.

## P5 — cells_per_square reduction (rejected)

Reducing `cells_per_square` from 256 → 128 was considered but rejected by the project owner.
Do not re-propose this. The 256-cell grid is intentional and should be treated as fixed.

See `meta/history/2026-06-04-performance-review.md` for full original analysis.

[[openlock-spec-compliance]]
