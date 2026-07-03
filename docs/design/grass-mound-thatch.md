# Grass — Mound-Thatch Design

**Status: first working prototype (2026-07-03 afternoon).**
`ThatchGrass` lives in `src/dharmatiles/grass/thatch.py`; experiment log +
iteration table in the history entry; renders in
`docs/grass/experiment-2026-07-03-thatch-s{1,2}.png`.  E1–E4 validated:
substrate relief, sheaf bundles, lift-clearance layering, stack cap.
Full tile builds in ~3.5 s (parity with old grass, unoptimised).
Supersedes `docs/grass/requirements.md`, `docs/grass/design.md`,
`docs/grass/grass-behavior.md`, `docs/floppy-grass-algorithm.md` (all of
which describe the flat field-simulation grass and mix requirements with
implementation).  Interview record and rationale:
`docs/meta/history/2026-07-03-grass-greenfield-requirements.md`.
Reference images: `docs/grass/grass-reference.png` (target look),
`docs/grass/grass-reference-printed.png` (FDM print proof),
`docs/grass/grass-current-2026-07-03.png` (baseline "before").

## Problem

Replace the flat field-simulation grass with grass that reads **lumpy and
organic** — mounded relief, swirled sheaves of plump blades — matching the
reference, under hard supportless FDM (0.4 mm nozzle, ≤ ~45° overhang) and
a 5 mm height ceiling, at DB (35 mm/sq) and OL (25.4 mm/sq) scales.
Blade *shape* is fine and is kept; arrangement, coverage structure, and
3-D relief are rebuilt.  Key priorities from the interview: **aesthetics
and speed**.

## Requirements (testable; each with its by-construction guarantee)

| # | Requirement | Guarantee mechanism |
|---|---|---|
| R1 | Mounded relief: surface undulates at clump wavelength with configured amplitude (~2.5–3 mm p-t-v) | Relief is an **input**: a mound field added into `terrain_z` before any blade exists. Cannot fail to appear. |
| R2 | Total coverage: no raw terrain visible in the grass region | The mound substrate **is** the terrain surface, textured by the carpet; blades are decoration on top. Vacuously total. |
| R3 | Organic, non-gridded arrangement; visible sheaves; adjacent sheaves differ in direction | Maximal Poisson-disk sheaf placement (no rows possible); direction from a smooth low-frequency field (no i.i.d. salt) blended with down-slope. |
| R4 | Overlapping blades read layered, each proud of the one below | Root-z-sorted standoff **accumulation** (leaf iter-4 mechanism): a higher-rooted blade can never sit under a lower-rooted one, structurally. |
| R5 | Supportless FDM everywhere | Substrate is a heightmap (no undersides exist) with generation-time slope cap ≤ 40°; blades are draped near-surface-parallel with capped curl and no tip lift. Violations are unproducible, not culled. |
| R6 | Total height ≤ 5 mm above region base | Budget partition enforced on inputs: mound amplitude + blade thickness × max stack depth ≤ 5 mm. |
| R7 | Blades stay inside tile XY and grass mask; don't pierce rocks/trees | Roots rejected outside mask / inside `obstacle_mask`; paths clipped at mask/obstacle boundary (shorten, don't steer). |
| R8 | Downstream stacking still works | Final blade tops rasterised into `vegetation_support_z` (existing machinery kept). |
| R9 | Perf: at least parity with today — current `1x1-grass` full build measured at **3.6 s** (2026-07-03); target grass layer ≤ 3 s, tile ≤ 5 s, linear in area | No growth simulation, no rays, no boolean: z-seating is bilinear heightmap sampling; blades batch-built and concatenated. Dense dressing raises blade count — watch this in E5. |
| R10 | Deterministic under fixed seed; correct at DB and OL scales | All dimensions in mm; RNG threaded from `SurfaceConfig.seed` as today. |

Non-goals: lifted tips (dispensable per interview); blade-level obstacle
*steering*; cross-tile mound continuity (tiles are separate objects; the
current grass has the same property); colour; multi-species in V1 — **one
great default grass first** (interview round 3), the species framework is
re-generalised after acceptance, not carried live through the rebuild.

**Aesthetic bias: drybrush-optimized** (interview round 3).  Tiles are
painted; the tabletop read depends on drybrush catching positive relief.
Design consequences: preserve **crisp blade edges and inter-blade
grooves** (don't over-smooth seams between blades — the grooves are where
the base coat stays dark); substrate super-Gaussian exponent tuned toward
defined shoulders rather than soft domes; the S3 print acceptance is
judged **after priming + drybrush**, not on raw plastic.

## Architecture — three layers

```
(rocks/trees have stamped obstacle_mask, terrain_support_z as today)
        │
        ▼
 1. MOUND SUBSTRATE   mound field += terrain_z   (heightmap op, ~ms)
        │
        ▼
 2. UNDER-CARPET      existing GrassCarpet embossing, now following relief
        │
        ▼
 3. BLADE DRESSING    sheaves placed to Poisson saturation, blades draped
                      on the substrate, standoff-accumulated, batch-meshed
```

### 1. Mound substrate

A low-frequency lumpy field added into `terrain_z` inside the grass mask
(mechanically a sibling of `SoilCarpet`'s super-Gaussian blob sum — reuse
that kernel at clump wavelength, amplitude ~2.5 mm, exponent tuned for
soft domes).

**Rock lapping** (interview round 3): after `Rocks`/`Tree` stamp, the
mound field gains a feathered *skirt* term rising against obstacle
footprints (within the slope cap), so grass laps the stones and they read
nestled rather than ringed by a bare moat.  Blade roots stay out of
`obstacle_mask` but may sit close, on the skirt.

Interview round 2 (2026-07-03) pinned three knobs:
- **Wavelength: 4–5 mounds per 35 mm square** (~7–9 mm blob spacing) —
  distinct countable lumps, clearly undulating silhouette.
- **Narrow regions scale down**: where the local grass-region width can't
  fit a full mound, wavelength *and* amplitude shrink proportionally with
  local region width (corridor margins stay lumpy in miniature, never
  clipped slices of full-size mounds).
- **Feather flat at tile edges**: relief eases to the region's base level
  near the tile boundary so adjacent tiles meet cleanly at butt joints.
  Also feathered to zero at interior mask edges (~2 mm) so region
  boundaries stay clean.  (Explicit interview decision — the reference's
  full-relief edge was considered and rejected in favour of tileability.)

- **Slope cap by construction**: amplitude/spacing chosen so max gradient
  ≤ tan(40°); verified by an every-run slope histogram (must report 0
  cells over 45°).
- **Clump sites are shared** with layer 3: the same site list drives blob
  centres and the sheaf direction field, so mounds read as *piles of
  grass*, not noise under grass.

### 2. Under-carpet

`GrassCarpet` unchanged, ordered after the mound so its blade stamps
follow the relief.  Provides the reference's fine second scale in hollows
and between sheaves.  (Possible later knob: slightly denser stamps in
hollows; not V1.)

### 3. Blade dressing

**Density: dense — the blades ARE the surface** (interview round 2): big
blades cover nearly everything; the carpet texture only peeks through in
hollows.  The substrate under a dense dressing is mostly hidden, so its
job is *shape*, not surface detail.

**Placement unit = sheaf** (tuft of 3–9 blades; the reference's visible
bundles).  Sheaf roots dart-thrown to **maximal Poisson-disk saturation**
over the grass mask using the exact-distance grid from
`scatter/distribute.py` / the leaf placer — coverage saturation by
construction, no rows.  Roots rejected in `obstacle_mask`.

**Direction** per sheaf: `blend(w · downslope(mound field at root),
(1−w) · coherent positional angle field)` — the leaf placer's field,
2-D-simplified.  Down-slope combing off a sheaf's own mound is the
swirl-around-lumps read; `w` is a module constant to tune (start 0.6).
Within a sheaf: shared direction + curl, per-blade jitter (small), blades
fanned as **parallel offset paths → non-crossing by construction**.

**Draping**: each blade spine is a 2-D path (length/width/curl sampled
from `SpeciesConfig` as today, ±20 % downward size jitter — leaf iter-3
lesson).  `z(s) = max(substrate over the local footprint) + clearance +
standoff`, sampled bilinearly; root embedded below the surface as today.
No rise cap needed — the substrate is slope-capped.

**Layering**: process sheaves sorted by root z (low first); each blade's
standoff accumulates over already-placed overlapping blades' recorded
tops (cheap 2-D grid of placed-top z, written per blade after seating —
one vectorised rasterise per blade, no simulation loop).  Stack depth
capped (~2 blade thicknesses) to hold R6; a blade that would exceed the
cap shortens or skips rather than climbing.

**Mesh**: existing blade grower/mesh machinery (`grass/mesh.py`,
`growers/flat.py`) fed with draped spine paths; all blades batch-built
and concatenated (never unioned per-blade); tops rasterised into
`vegetation_support_z`.

## Experiments (render-judged, one at a time, on S1/S2 scenes)

Acceptance scenes and budgets (fixed up front): **S1** full-tile render
elev 40/azim −135 (exact match to the baseline PNG); **S2** close-up
elev ~15 quarter-tile for silhouette; **S3** physical print at
acceptance.  Perf measured every run.

- **E1 — substrate only.**  Mound field + existing carpet + existing
  *current* 3-D grass untouched on top.  Zero new blade code.  Judge: does
  the tile already read "lumpy and organic" in S2's silhouette?  Knobs:
  amplitude, blob spacing, super-Gaussian exponent, feather width.
  Also the FDM derisk: print-slope histogram.  *Hypothesis: E1 buys the
  majority of the look.*
- **E2 — draped blades, no sheaves.**  Replace the simulation with drape:
  uniform Poisson singles, direction field only, no standoff accumulation
  (constant clearance).  Judge coverage rhythm and direction coherence vs
  the mat look; measure speed floor.
- **E3 — sheaves + shared clump sites + down-slope blend.**  The full
  aesthetic bet (mound=pile correlation).  Judge: do sheaves comb over
  their own mounds; do adjacent sheaves differentiate; artichoke/rows
  smells.
- **E4 — standoff accumulation + stack cap.**  Judge layered-overlap read
  (R4) at S2; check height histogram against the 5 mm budget (R6).
- **E5 — perf pass.**  Vectorise the drape sampling; verify R9 on a 2×2
  (linear scaling).  Only after the look settles (leaf-era ordering:
  constraints carried throughout, deep optimisation last).

Iteration protocol (from the leaf history): name artifacts in plain
language, instrument before fixing, fix constructively (change what *can*
happen), judge on the same scenes, one iteration at a time.  Failure
smells that trigger a stop-and-reinterview instead of a parameter nudge:
fixes that relocate artifacts; any special case patched more than twice.

## Keep / delete inventory

Keep: blade mesh construction + growers (shape approved), `SpeciesConfig`,
`GrassCarpet`, `scatter/distribute.py`, vegetation-support rasterisation,
layer-ordering contract (`Rocks`/`Tree` before `Grass`).
Delete **at acceptance, not before**: `grass/grow.py` simulation loop,
per-step occupancy stamping, `GrassSeed.sort_key()` upstream ordering,
`FloppyGrassLayer` growth internals — organs harvested, one generator
left standing (leaf-era lesson 8).  Old grass docs in `docs/grass/` and
`docs/floppy-grass-algorithm.md` are superseded by this file.

## Risks & open questions

- **Mound=pile correlation could look samey** (every mound combed the same
  way).  Mitigation: `w` blend knob; per-clump angle field already varies.
  E3 decides.
- **Draped blades without simulation may visibly interpenetrate** where
  sheaves cross.  Mitigation: R4 accumulation handles vertical order; if
  side-by-side shear still offends, add a cheap root-spacing rule —
  do NOT reintroduce per-step simulation.
- **Plumpness**: current cross-section approved, but sparse placement may
  read thin; width/thickness scale is a species knob, not new geometry.
- **Boundary drape** (reference blades overhang the soil rim) conflicts
  with the tile-footprint invariant; V1 keeps the hard clip, feathered
  mound keeps edges clean.  Revisit only if S3 print looks shaved.
- **Knobs stay module constants** until the look settles; config promotion
  is an open item.
