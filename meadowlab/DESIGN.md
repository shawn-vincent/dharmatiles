# meadowlab — Design

*Revision 2, 2026-10-27 (code v1d).  Code: `meadowlab/meadowlab.py`.  It
imports `grasslab` and `rocklab` as modules and passes its own knobs as
arguments (no monkeypatching); all meadowlab knobs are its module
constants.*

## Pipeline (`main`)

1. **Two RNG streams**: grass (seed) and rocks (seed + 1000003), so rock
   changes don't reshuffle the grass and vice versa.  Stream use is
   interleaved in a fixed order: ground, flow, swaths (grass) → scatter,
   pose (rocks) → free lenses (grass) → aprons (rocks) → flow wrap, grass
   (grass).
2. **Ground, flow, swaths**: grasslab `make_ground`, `make_direction`,
   `Swaths`, unchanged.
3. **Rocks first**: `rocklab.scatter(…, ROCK_PLACEMENTS_2X2 = 6–9)` then
   `rocklab.pose`, so each rock's yaw, and therefore its real footprint,
   is fixed before any grass is built.
4. **Footprint mask** (`footprint_mask`): each rock's slice 0.45 mm above
   its base (`Manifold.slice`), rasterised onto the 0.12 mm grass grid.
5. **Soil**:
   - **Aprons** (`apron_shapes`) for 35% of rocks: a lens with half-width
     U(0.5, 0.9)·R and stretch U(1.2, 1.9), axis along the flow, centred
     U(0.6, 1.0)·R from the rock toward a side drawn within ±90° of
     downstream 60% of the time (a wake), otherwise of upstream (a scour).
   - **Free lenses**: grasslab `place_lenses` with a large pool
     (8 per square, capped by the 80 candidates and 13 mm separation),
     ranked by grasslab's low-ground score.  `fill_bare_earth` takes them
     best-first **at natural size** until (soil > 0.5) ∪ footprint ≥ 20%,
     measured on every 4th grass column (0.48 mm).  The v1c size-scaling fit
     was removed: it let one patch grow dominant.
   - `combine` builds a `grasslab.Lenses` from the first k free lenses plus
     extra shapes, sharing the free lenses' border warp.
6. **Flow** (`rock_circles`): `finish_flow` wraps the soil lenses **plus** a
   circle of radius R + 0.6 mm per rock (as a lens with equal half-length
   and half-width), so grass parts around rocks (REQ-M-3) with no soil
   involved.
7. **Grass**: `build_grass(…, blocker=footprint, tuck_mm=0.5)`: no rooting
   inside a footprint; spines stop 0.5 mm inside it, with the usual k_h
   lowering.  Then grasslab `make_field`, `fdm_backstop`, `mesh_field`.
8. **Seat rocks** (`seat_rocks`): z₀ = min(H₀ over the footprint) − 0.3 mm.
9. **Union** (`attach_base`): simplified grass ∪ rocks ∪ DB base (peg height
   from the tallest point), keep the largest body, report crumbs.

## Parameters (module constants)

| Constant | Value |
|---|---|
| `ROCK_SEED_OFFSET` | 1000003 |
| `ROCK_PLACEMENTS_2X2` | 6–9 |
| `FREE_LENS_POOL_PER_SQ` | 8.0 (drawn as U(8, 8), keeping the grass RNG stream aligned) |
| `BARE_EARTH_FRAC`, `BARE_SAMPLE_STRIDE` | 0.20, 4 |
| `APRON_PROB`, `APRON_WAKE_PROB` | 0.35, 0.6 |
| `APRON_HALF_W`, `APRON_STRETCH`, `APRON_OFFSET` | 0.5–0.9 R, 1.2–1.9, 0.6–1.0 R |
| `FOOTPRINT_Z_MM`, `TUCK_MM`, `FLOW_RING_MM`, `ROCK_SINK_MM` | 0.45, 0.5, 0.6, 0.3 |

## Differences from the parent labs (kept: changing them changes output)

- The grass voxel is always 0.12 mm (grasslab uses 0.1 mm at 1 × 1).
- The grass is simplified *before* the union with rocks and base
  (grasslab simplifies after its base union).
- Seed 1 is the case that exposed grasslab's non-manifold marching-cubes
  failure (previously a silent empty STL); grasslab's `mesh_field` now
  retries, and the tile is fine.

## Iteration log

| Iter | Artifact / trigger | Fix |
|---|---|---|
| v1a | aprons too wide and long, merging into bald areas (32% soil, M3) | apron 0.3–3.0 → 0–2.0 mm, stretch 1.3–1.8 → 1.1–1.5, margin 0.9 → 0.6; free lenses 0.6–1.0 → 0.4–0.8 per square |
| v1b | Shawn: interaction not natural; not every rock in a bare area | rocks became footprint obstacles (grass pressed against them), one-sided aprons on 35% of rocks, visible bare earth fitted to `BARE_EARTH_FRAC` (by scaling patches) |
| v1c | 10.1% visible bare earth (target 10%), 2 of 6 rocks with aprons | — |
| v1d | Shawn: "stop that scaling thing; let the 20% be underneath rocks too" | count-based fill at natural sizes; target 20% incl. under rocks → 20.6% (14.9% visible), 9 free lenses |
| — | refactor (2026-10-27) | deterministic rocks (rocklab fix); module monkeypatching replaced by arguments (`scatter(…, placements)`, `place_lenses(…, per_sq)`); `copy.copy` lens surgery replaced by `combine`; `rocklab.pose` / `Placement` reused; grasslab's `Grass` result and grass Manifold used directly |
