# meadowlab — Rocks, Grass and Bare Earth Together: Requirements

*Written 2026-10-27, v1.  Purpose (Shawn): "a tile with some rocks across
it, and then grass and bare earth in amongst the rocks — see how these
things interact."  This combines `grasslab/` and `rocklab/`.  It inherits
all of their requirements and only defines the interactions.*

## Requirements

- **REQ-M-1 [S]** A run writes `meadowlab/out/<cols>x<rows>-meadow-s<seed>.stl`:
  one watertight DB tile with grasslab hills, grass and soil, and rocklab
  rocks.
- **REQ-M-2 [R] Rocks are part of the ground.**  Rocks are seated into
  the hills, with the base just below the lowest soil under the
  footprint.  On slopes the uphill side is buried, as if the ground
  formed around the rock.  No rock floats or perches.
- **REQ-M-3 [R] Grass flows around rocks.**  The flow parts upstream of
  each rock and rejoins behind it, the same way it wraps around a bare
  patch (REQ-FL-4).  Grass doesn't run into a rock head-on.
- **REQ-M-4 [R] Most rocks have no margin.**  Grass grows right up to the
  stone and presses against it.
  - Only a minority of rocks (about 35%) have bare earth beside them, and
    then on **one side** (a wake downstream or a scour to one side),
    never a full ring around the rock.
  - *(Shawn, v1b: "having each rock in a big bare area is kind of
    weird.")*
- **REQ-M-5 [R] Rocks are obstacles, not soil.**
  - No grass roots under a rock's real footprint, which is the rock's
    own outline where it meets the ground.
  - Fronds heading into a rock stop pressed against its face, tucked
    0.5 mm in, and are lowered like any cut-short frond.
  - Rocks don't trigger the soil-bank behaviours (spill, side ledges);
    those belong to bare earth.
- **REQ-M-5a [S] Bare earth target.**  `BARE_EARTH_FRAC` (default 20%)
  is the fraction of the tile that is bare earth **including ground
  under rocks**.  It is met by adding natural-size bare patches, never
  by scaling patch sizes.  *(Shawn, v1c: "stop that scaling thing; let
  the 20% be underneath rocks too.")*
- **REQ-M-6 [S] A handful of rocks.**  6–9 placements on a 2 × 2 tile.
  Grass stays dominant.
- **REQ-M-7 [S]** The grass and rock FDM rules still hold, and the audit
  reports them.

## Failure modes

| # | Failure | Looks like |
|---|---|---|
| M1 | **Pasted rocks** | rocks sitting on top of grass, or floating on slopes |
| M2 | **Moat** | bare ground around every rock, or a uniform ring (seen in v1a/b) |
| M3 | **Bald field** | rock aprons merging into large bare areas (seen in v1a: 32% soil) |
| M4 | **Ploughing** | grass running straight into rock faces |
