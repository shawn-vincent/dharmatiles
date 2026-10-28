# meadowlab — Rocks, Grass and Bare Earth Together: Requirements

*Revision 3, 2026-10-28 (matches code v1e).  Purpose (Shawn): "a tile with
some rocks across it, and then grass and bare earth in amongst the rocks —
see how these things interact."  It combines `grasslab/` and `rocklab/`,
inherits all of their requirements, and defines only the interactions.*

## Requirements

- **REQ-M-1 [S]** A run writes `meadowlab/out/<cols>x<rows>-meadow-s<seed>.stl`:
  one watertight DB tile with grasslab hills, grass and soil, and rocklab
  rocks.  Deterministic per seed (byte-identical STL).
- **REQ-M-2 [R] Rocks are part of the ground.**  Each rock sinks below the
  soil everywhere on its footprint: it is seated at a low quantile of the
  ground under it and extended downward (a plinth of its own section) to
  just below the lowest ground, so on slopes the uphill side is bedded and
  the downhill side shows no gap.  No rock floats or perches (M1).
- **REQ-M-9 [R] Rocks read as rocks.**  Every rock's top stands clearly
  above the surrounding grass: at least max(1.5 mm, 0.45 × its height),
  lifting it by up to 1.5 mm if needed.  No rock is swallowed by grass or
  buried in a slope (M5).
- **REQ-M-3 [R] Grass flows around rocks.**  The flow parts upstream of
  each rock and rejoins behind it, as it does around a bare patch
  (grasslab REQ-FL-4).  Grass never runs into a rock head-on (M4).
- **REQ-M-4 [R] Most rocks have no margin.**  Grass grows right up to the
  stone and presses against it.  Only a minority of rocks (about 35%) have
  bare earth beside them, and then on **one side** (a wake downstream or a
  scour to one side), never a ring (M2).  *(Shawn, v1b: "having each rock
  in a big bare area is kind of weird.")*
- **REQ-M-5 [R] Rocks are obstacles, not soil.**
  - No grass roots under a rock's real footprint (its own outline where
    it meets the ground).
  - Fronds heading into a rock stop pressed against its face, tucked
    0.5 mm in, and are lowered like any cut-short frond.
  - Grass beside a rock is lower (half height at the face, full height
    1.2 mm out), so it laps against the rock's foot rather than heaping up
    over it.
  - Rocks don't trigger the soil-bank behaviours (spill, side ledges);
    those belong to bare earth.
- **REQ-M-6 [S] Bare-earth target.**  `BARE_EARTH_FRAC` (20%) is the
  fraction of the tile that is bare earth **including ground under
  rocks**.  It is met by adding natural-size bare patches, never by scaling
  patch sizes (M3).  *(Shawn, v1d: "stop that scaling thing; let the 20% be
  underneath rocks too.")*
- **REQ-M-7 [S] A handful of rocks.**  6–9 placements asked for on a 2 × 2
  tile.  Grass stays dominant.
- **REQ-M-8 [S]** The grass and rock FDM rules still hold, and the audit
  reports them.

## Failure modes

| # | Failure | Looks like |
|---|---|---|
| M1 | **Pasted rocks** | rocks sitting on top of grass, or floating on slopes |
| M2 | **Moat** | bare ground around every rock, or a uniform ring (v1a/b) |
| M3 | **Bald field** | aprons or one scaled-up patch merging into large bare areas (v1a: 32% soil; v1c scaling) |
| M4 | **Ploughing** | grass running straight into rock faces |
| M5 | **Swallowed rock** | a rock showing only as holes or a sliver in the grass, or buried on its uphill side (v1d seed 3) |

## Measured on the current code (v1e, 2 × 2)

| | Seed 1 | Seed 2 | Seed 3 |
|---|---|---|---|
| Rocks | 6 placements, 6 stones, footprint 5.1%, 1 apron | 9, 18 stones, 12.6%, 3 aprons | 6, 7 stones, 4.5%, 3 aprons |
| Lowest rock top above grass | 2.4 mm (3.3 mm rock) | 2.2 mm (4.0 mm rock) | 1.5 mm (3.2 mm rock, lifted 0.6) |
| Deepest plinth | 1.0 mm | 0.6 mm | 2.0 mm |
| Bare earth incl. under rocks / visible | 20.8% / 15.7% | 21.8% / 9.2% | 21.0% / 16.5% |
| FDM backstop, overhang | +2,091 / −2,355; 12.8 mm² | +789 / −660; 4.8 mm² | +1,844 / −1,793; 13.1 mm² |

Seeds 1–6 all export as one watertight body (`stl_safe`).
