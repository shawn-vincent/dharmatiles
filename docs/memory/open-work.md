---
name: open-work
description: Current backlog of open work items — verified against actual code as of 2026-06-08
metadata:
  type: project
---

Verified against the live codebase on 2026-06-08. Items from older reviews that no longer
apply to the current architecture have been struck.

## Active items

### 1. Species presets (code gap)
No named preset library exists. Every `.tile` file spells out all `SpeciesConfig`
parameters explicitly. Users have no ready-made starting points (grass, rush, dead
grass, etc.). A small presets file or library of named `.tile` fragments would
make the spec format usable without reading source code.

### 2. README images / renders
`README.md` is good — has quick start, structure, spec format, philosophy. What
it lacks is any image or render of actual output. A newcomer cannot tell what
a finished tile looks like without generating one themselves.

### 3. Parallel-blade Z jitter (visual check needed)
The old `own_stamps` aliasing mechanism described in the 2026-06-06 grass requirements
review is gone from the codebase. `_sample_footprint_max` now probes only the
leading-edge cells of the next segment, not the blade's own trail. Whether
parallel blades still cause visible Z jitter needs a visual check of STL output
with two side-by-side parallel blades.

## Resolved / obsolete

- **Edge-fill blade direction** — obsolete. No "edge-fill" seeding exists in the
  current architecture. Seeds distribute via Voronoi groups; `_sort_upstream_first`
  handles ordering. The specific mechanism the old review described is gone.

- **New tile types** — 6 specs already exist: grass-only, half-grass-soil,
  grass-and-water, coast-left, corner-grass, water. Adequate coverage for current
  needs.

- **OpenLOCK retention cuts** — done (commit 88fabaa).

- **P1 rasterise half_cell fix** — superseded by `_contained_segment_cells` architecture.

- **P5 cells_per_square reduction** — rejected by project owner. Do not re-propose.

[[performance-grass]]
[[openlock-spec-compliance]]
