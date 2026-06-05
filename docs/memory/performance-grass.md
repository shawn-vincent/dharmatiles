---
name: performance-grass
description: Grass generation performance — fix_normals was the bottleneck; one-line fix gave 9.6× speedup
metadata:
  type: project
---

Tile generation was dominated (98.7%) by `mesh.fix_normals()` called once per blade mesh (1,573 calls total).

**Fixed (2026-06-04)**: For `cross_section='circle'` (the default), the tube winding is already outward-correct except for the TOP CAP. Top cap winding was `[rl+i, rl+(i+1)%n, v_tip]` (inward normal). Fix: `[v_tip, rl+(i+1)%n, rl+i]`. After this, `fix_normals()` is skipped entirely for circle tubes. **90× speedup per tube mesh, 9.6× on grass phase, 8× overall (80s → 10s).**

Triangle/diamond cross-sections still call fix_normals (their side faces are inconsistently wound by construction).

**Post-fix profile** (inside grass):
- `rasterise_into_support`: 61% — new bottleneck
- `sample_grid` in growth loop: 15%
- `_smooth_path` (CubicSpline): 9%
- `build_tube_mesh`: 7% (down from 97%)

**Remaining quick wins** (not yet implemented):
- **P1** `grid.py`: `half_cell = 0.5 × min(cw,ch)` → `1.5 × min(cw,ch)` → ~3× rasterise speedup
- **P3** `config.py`: `cells_per_square = 256` → `128` → ~4× heightmap+rasterise speedup, zero print-quality impact (nozzle = 0.4mm >> cell = 0.2mm)

See `meta/history/2026-06-04-performance-review.md` for full analysis and projections.

[[openlock-spec-compliance]]
