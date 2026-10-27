# meadowlab — Design

*v1, 2026-10-27.  Code: `meadowlab/meadowlab.py`.  It imports grasslab and
rocklab unchanged; all knobs are module constants in meadowlab.*

## Pipeline

1. **Two RNG streams**: grass (seed) and rocks (seed + 1000003), so
   rock changes don't reshuffle the grass and vice versa.
2. **Ground, flow and swaths**: grasslab, unchanged.
3. **Rocks first**: `rocklab.scatter` with 6–9 placements per 2 × 2.
   Each rock's yaw is fixed now (`pose_rocks`), so its real footprint
   is known before any grass is built.
4. **Footprint mask**: the rock's slice 0.45 mm above its base
   (`Manifold.slice`), rasterised onto the grass grid.
5. **Soil**:
   - **one-sided aprons** for 35% of rocks.  An apron is a lens of
     half-width 0.5–0.9 R and stretch 1.2–1.9, offset 0.6–1.0 R from the
     rock centre.  It goes downstream 60% of the time (a wake),
     otherwise upstream or sideways (a scour).
   - **Free lenses**: grasslab builds a large candidate pool, ranked by
     its usual low-ground score with 13 mm separation.  Free lenses are
     taken best-first, **at their natural size**, until
     (soil > 0.5) ∪ footprint ≥ `BARE_EARTH_FRAC`, measured on a 0.48 mm
     grid.  The v1c size-scaling fit was removed: it let one patch grow
     dominant.
6. **Flow**: `finish_flow` deflects around the soil lenses **plus** a
   circle of radius R + 0.6 mm per rock, so grass parts around rocks
   (REQ-M-3) without any soil being involved.
7. **Grass**: `build_grass(..., blocker=footprint, tuck_mm=0.5)`.  This
   is a small optional hook added to grasslab; it does nothing when
   omitted, and grasslab's own output was verified identical.
   - No rooting inside the footprint.
   - Spines stop 0.5 mm inside it, with the usual k_h lowering.
8. **Seat rocks**: z₀ = min(H₀ over the footprint) − 0.3 mm.
9. **Union**: grass (simplified) ∪ rocks ∪ DB base.

## Iteration log

| Iter | Artifact | Fix |
|---|---|---|
| v1a | aprons too wide and long, merging into bald areas (32% soil, M3) | apron 0.3–3.0 → 0–2.0 mm, stretch 1.3–1.8 → 1.1–1.5, margin 0.9 → 0.6; free lenses 0.6–1.0 → 0.4–0.8 per square |
| v1b | Shawn: interaction not natural; bare earth should be visible-only and independent; not every rock in a bare area | rocks became footprint obstacles (grass pressed against them), aprons one-sided on 35% of rocks, visible bare earth fitted to `BARE_EARTH_FRAC` |
| v1c | 10.1% visible bare earth (target 10%), 2 of 6 rocks with aprons | — |
| v1d | Shawn: "stop that scaling thing; let the 20% be underneath rocks too" | count-based fill at natural sizes; target 20% incl. under rocks → 20.6% (14.9% visible), 9 free lenses |
