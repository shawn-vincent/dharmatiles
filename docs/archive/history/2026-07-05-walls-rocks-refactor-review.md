# Walls + Rocks Deep Review & Refactor Plan — 2026-07-05

Baseline tag: `walls-fieldstone-e25`.  Shawn's brief: *"clean things up
that we don't need to render the demos we've been working with for
walls (both fieldstone and brick) and standalone rocks.  DRY and
generalize all mechanisms so they reuse the same configurability to go
from rock to fieldstone to brick/worked stone.  Simplicity > normality
> elegance > robustness > performant.  Aesthetics must not change
materially.  Reuse of code/mechanisms/capabilities is paramount."*

Scope: `scatter/stones.py` (1 337 lines), `walls/masonry.py` (535),
`walls/fieldstone.py` (695), `scatter/prototype.py` (Rocks adapter),
and the demo scenes in `docs/rocks/` + `docs/walls/`.  Grass, trees,
water, flowers are OUT of scope.

## 1. What renders the demos today

```
Rocks (scatter/prototype)  ──┐  legacy adapter → StoneSpec list
StoneField (scatter/stones) ─┼→ _build_and_stamp → build_stone
FacetedStones (")           ─┘      │  (hull → seat → weather bites →
                                    │   round_edges → warp → blur_remesh
                                    │   → Taubin → AGED RELIEF PASS →
                                    │   overhang audit loop)
                                    └→ _engrave_cracks → _stamp_stone

CutStoneWall (walls/masonry)   layout (courses×bays×quoins) → core
   │                           boxes → _block_mesh per cell (jittered
   │                           box hull + round_edges + blur_remesh +
   │                           relief_field) → union → clip → pinches
   └── FieldstoneWall          same chassis; overrides _cells (through-
       (walls/fieldstone)      stones, splits), _place_block (crack-
                               network outline → sphere-morph loft →
                               _stone_texture AGED RELIEF PASS),
                               _extra_parts (rubble hearting =
                               fibonacci lumpy hulls)
```

The **masonry chassis is already family-independent** (layout, core,
rubble hook, union, tile clip, pinch separation, seat/stamp).  The
family axis is the *unit kernel*: what solid fills one cell.  That is
exactly the axis rock→fieldstone→brick generalization should live on.

## 2. Duplication inventory (ranked by payoff)

**D1 — The aged-surface relief pass exists three times.**
1. `stones.py` aged path (~lines 696–764): Taubin-relaxed copy →
   smoothed normals + curvature damp `1/(1+(cd/0.35)²)`; broad
   undulation `0.35·relief_field(6, foot/4.5, foot/1.6, spectral=1.0)`;
   grain `0.7·relief_field(16, 0.6, 3.2, 0.7)`; 3-wave patchy envelope
   floor 0.2; hero-face damping; displace along smoothed normals.
2. `stones.py` fresh path (~lines 611–626): same relax/damp machinery,
   grain only, half amplitude.
3. `fieldstone.py _stone_texture` (606–647): an explicit hand-mirror of
   (1) at wall-stone scale — same envelope, same damp constant, grain
   floor lifted to 0.9 mm, mm floors on amplitudes, no hero face.

Three copies of the project's signature "stone read".  Any future tuning
(and the cliff family) needs it in one place with parameters.

**D2 — Lumpy Fibonacci hull ×2.**  `stones._support_points` (jittered
golden-angle directions × ellipsoid radii × lump, egg taper, crown
flat) vs `fieldstone._rubble_mesh` (same direction sampling + lump,
plus a blockiness ellipsoid↔box blend).  One point-cloud primitive with
`egg / crown / blockiness` parameters serves stones, rubble — and
future brick rubble fill / ruin scatter.

**D3 — Shared primitives have no home.**  `walls/*` imports
`_blur_remesh`, `_relief_field`, `_round_edges` from `scatter/stones`
by private name.  The reuse already happened; the module boundary never
caught up.  These are stone-making primitives, not scatter-placement
code.

**D4 — Segment frame transform ×4.**  The 4×4 `[d n | a]` block is
built by hand in `masonry._core_boxes._box`, `masonry._place_block`,
`fieldstone._place_block`, `fieldstone._extra_parts`.

**D5 — Deterministic per-feature RNG ×3 styles.**
`(seed·1_000_003 + hash(key)) & 0x7FFFFFFF` (masonry + fieldstone
blocks), `hash((seed,)+key)` (fieldstone shared curves), `seed ^ 0xCAFE`
constants (stones, 5 sites).  `core/tile.derive_seed` already exists as
the project-normal way to do this.

**D6 — Box-clip-with-fallback ×2.**  `masonry._clip_to_tile` and the
stones floor clip (`_build_and_stamp`) are the same
intersection→watertight-check→warn-and-keep pattern.

**D7 — Float32 STL round-trip check ×2.**  Stones re-quantize vertices
to float32 and re-`process` to prove the mesh survives export (bites,
cracks).  The wall pipeline solves the same failure with
`_separate_pinches`.  Same concern, two idioms, no shared helper.

## 3. Dead / stale findings

- `FieldstoneWall(relief_wl=…)` is accepted, stored, and never read —
  `_stone_texture` hardcodes its wavelengths.  Remove.
- `CLAUDE.md` architecture table still lists `layers/rocks.py`
  (deleted 2026-07-04), doesn't list `walls/` or `scatter/stones.py`,
  and the `Rocks` docs describe the deleted dome kernel.
- `scatter/prototype.Rocks` still accepts `n_cuts/cut/roughness/sink`
  (documented as ignored) — fine as a compat shim, but the class
  docstring is the only place that says so.
- No other dead code found on these paths: crack engraving, spall
  bites, seam, crown_flat, hero-face protection are all exercised by
  the `docs/rocks/` scenes; every masonry hook has a subclass user.

## 4. The generalization axis

All three families make the SAME thing: a stone body = **shape**
(faceted hull | jittered-box hull | outline sphere-morph loft) +
**finish** (roundover, blur-remesh/Taubin, aged relief pass) +
**guarantees** (watertight, printable, STL-survivable).  Today each
family hardwires its own column of that matrix:

| | scatter rock | fieldstone unit | cut-stone unit |
|---|---|---|---|
| shape | fibonacci hull (egg/crown) | crack outline → sphere-morph | jittered box hull |
| roundover | `round_edges` per-corner | 2-D outline buffer | `round_edges` |
| relief | aged pass (copy 1+2) | aged pass (copy 3) | `relief_field` only |
| remesh | `blur_remesh` σ1.2 | loft-native | `blur_remesh` σ0.7 |

After the refactor the finish column is ONE parameterized mechanism,
the shape column is shared primitives, and a wall family = chassis +
unit kernel + finish config.  Cross-breeding (a brick wall of worn
pebble-morphed blocks; a fieldstone wall of near-rectangular slabs à la
ref-03/ref-04 Cotswold) becomes configuration, not new code.

## 5. Reference-photo awareness (what must stay expressible)

`docs/reference/walls/README.md` + `fieldstone/README.md` +
`docs/design/rocks-faceted-stones.md` name the eventual targets:
brick with eroded joints and spalled/missing units, coursed-slab
fieldstone (thin flat units, ref-03/04/07), cliff faces (stratified
bedding = the same coursed chassis at geological scale), crenellations,
ruin states shedding scatter stones.  The refactor must not close any
of these doors: unit-kernel-as-axis and shared finish config OPEN them
(a missing brick is a cell that renders no unit; a cliff is a chassis
variant; ruin rubble is the shared lumpy-hull primitive).  Nothing in
the plan below removes a capability the references need.

## 6. Staged plan

Aesthetics guard: after every stage regenerate the demo set (below),
render, and compare against the `walls-fieldstone-e25` baseline renders;
verify watertightness on every STL.  RNG-draw *order* changes are
allowed to shift individual bumps (slight changes OK) but the character
must hold.  One commit per stage.

- **Stage 1 — a home for stone primitives (mechanical, zero behavior
  change).**  New package `src/dharmatiles/stone/`: move
  `relief_field`, `blur_remesh`, `round_edges` (public names) plus
  shared helpers `clip_to_box`, `survives_stl32`, `separate_pinches`,
  and a `_frame` segment-frame helper in `walls/masonry.py` (D4).
  `scatter/stones.py` and `walls/*` import from it.  D5 (RNG
  derivation styles) is deliberately LEFT ALONE: unifying it onto
  `derive_seed` would reroll every stone in every shipped tile for
  zero functional gain — the existing int-tuple hashes are already
  process-stable.  Verification bar: byte-identical STLs.
- **Stage 2 — one aged-surface pass.**  `stone/finish.py:
  aged_relief(mesh, rng, foot, *, broad_amp, grain_amp, grain_wl,
  env_floor, hero_face=None, …)` replacing D1's three copies; stones
  aged, stones fresh-grain, fieldstone `_stone_texture` all call it.
  Delete the dead `relief_wl` fieldstone param.
- **Stage 3 — one lumpy-hull primitive.**  `stone/shape.py:
  lumpy_hull_points(rng, n, half_extents, *, lumpiness, dir_jitter,
  egg, crown_flat, blockiness)` replacing `_support_points` internals
  and `_rubble_mesh` sampling; fold D6/D7 helpers into their call
  sites.
- **Stage 4 — unit kernel + shared configurability + export
  robustness.**  Document the chassis subclass hooks as the family
  axis; promote the look-defining fieldstone constants to ctor
  parameters (`wobble_amp_mm`, `head_overlap_mm`, `bed_overlap_mm`,
  `proud_mm`, `bed_flat_exp` — defaults = the approved E25 look); new
  `docs/walls/walls-e7-variants.tile.py` demo (slab / default / cobble
  fieldstone + dressed cut stone on one tile) added to the demo set;
  and `separate_pinches` runs on the final concatenated mesh in
  `_attach_and_export` — the three baseline non-watertight demo scenes
  were cross-group pinches (stone against sealed terrain) that no
  per-layer fix could see.
- **Stage 5 — docs + final demo set.**  CLAUDE.md architecture table
  (add `walls/` + `stone/`, fix `scatter/stones.py`, drop
  `layers/rocks.py`), design-doc notes, regenerate everything.

## 7. Demo set (regenerated per stage → `stl/test/`)

| Scene | Exercises |
|---|---|
| `docs/rocks/weathering-sweep.tile.py` | roundover axis fresh→cobble |
| `docs/rocks/stone-showcase.tile.py` | class/size/crack/scar range |
| `docs/rocks/stone-field.tile.py` | sampled cluster placement |
| `docs/walls/walls-e5-textures.tile.py` | all 4 cut-stone presets |
| `docs/walls/walls-e2-corner.tile.py` | cut-stone corner/quoins |
| `docs/walls/walls-e6-fieldstone.tile.py` | fieldstone E25 corner |
| `docs/walls/walls-e4-meadow.tile.py` | wall + stones + grass integration |
