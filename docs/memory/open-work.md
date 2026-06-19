---
name: open-work
description: Current backlog of open work items — verified against actual code as of 2026-06-08
metadata:
  type: project
---

Verified against the live codebase on 2026-06-19.

## Active items

### 1. README images / renders
`README.md` is good — has quick start, structure, spec format, philosophy. What
it lacks is any image or render of actual output. A newcomer cannot tell what
a finished tile looks like without generating one themselves.

### 2. Strategic Recs 6 & 8 (future)
- **Rec 6** — `TileScene.placed_solids`: union of all placed 3D solids for mesh
  queries; enables leaf placement, grass tufting near trunks, canopy avoidance.
- **Rec 8** — Leaf placement via `trimesh.sample.sample_surface()` (depends on Rec 6).

## Resolved / obsolete

- **Species presets** — subsumed by Strategic Rec 4 (tile template / region library).
  Named `SpeciesConfig` instances (grass, rush, dead grass, etc.) will live in
  `src/tiles/shared/` alongside region factories when Rec 4 is implemented.

- **Edge-fill blade direction** — obsolete. No "edge-fill" seeding exists in the
  current architecture. Seeds distribute via Voronoi groups; `_sort_upstream_first`
  handles ordering. The specific mechanism the old review described is gone.

- **New tile types** — 5 specs already exist: grass, soil+grass, water+grass,
  soil+grass-corner, water. Adequate coverage for current needs.

- **Parallel-blade Z jitter** — fixed. `own_stamps` mechanism removed; `_sample_footprint_max`
  probes only leading-edge cells via `_leading_edge_cells`. Visually confirmed fixed 2026-06-08.

- **OpenLOCK retention cuts** — done (commit 88fabaa).

- **P1 rasterise half_cell fix** — superseded by `_contained_segment_cells` architecture.

- **P5 cells_per_square reduction** — rejected by project owner. Do not re-propose.

[[performance-grass]]
[[openlock-spec-compliance]]
