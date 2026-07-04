# Walls — Coursed Masonry Design (gen-1)

**Status: gen-1 prototype implemented (`src/dharmatiles/walls/`),
first render round E1–E4 built 2026-07-04; awaiting Shawn's MeshLab
judgment on `stl/test/walls-*.stl`.**
Interview record: `docs/meta/history/2026-07-04-walls-greenfield-requirements.md`.
Reference analysis: `docs/reference/walls/commercial-sets-analysis.md` +
`docs/reference/walls/README.md`.  Baseline judgment:
`docs/walls/wall-baseline-2026-07-04.png`.
Fourth run of the leaf/grass/rocks development method: this doc restates
the interview requirements as by-construction guarantees and picks the
mechanisms; iteration happens on named render artifacts afterwards.

## Problem

Give dharmatiles real walls: DungeonBlocks-compatible by default
(35 mm grid, ≈49.7 mm standard / ≈89 mm tall, ≈7 mm slab flush to a
tile edge), textured on **both faces, both ends, and the top** — which
the heightmap terrain cannot do (baseline probe: vertical faces are
untexturable in place).  Gen-1 ships **straight runs and corners on
flat ground** in the **irregular cut-stone** family (the core dungeon
read: DB sets 01/08/13, Hirst m50, Dragon's Rest).  The layout system
must extend to the other masonry families (small-block/brick,
fieldstone) and to ruins, openings, and organic plans without a
rewrite.

## Family build order (from the taxonomy)

1. **Gen-1: irregular cut stone** — per-unit masonry, maps directly
   onto the shipped faceted-stone primitive; the DB-compatible look.
2. Small-block coursed / brick — same solver, tighter joint stats,
   more regular bond (a parameter preset, not new machinery).
3. Fieldstone/drystone — same block-instancing, different layout
   (packing, weak coursing).
4. Natural cliff / high ground — different pipeline (organic mass +
   relief + grass tops); shares ground with the rocks
   outcrops/cliffs generation already queued.
5. Ashlar+engraving, cave rock, timber-frame, plank — later.

## Requirements → guarantee mechanisms

| # | Requirement | Mechanism (by construction) |
|---|---|---|
| R1 | DB-compatible by default, configurable | Defaults: `height_mm=49.7`, `thickness_mm=7.0`, outer face flush to the tile edge; every dimension is a parameter; `tall` preset at 89 mm; OL scale runs through the same scale machinery as everything else |
| R2 | Both faces + ends + top textured | The wall is a **separate solid**: `union(core slab, blocks)`; every block spans the full wall thickness, so its front, back, end, and top faces are real modeled surfaces — there is no untextured direction |
| R3 | Watertight, no see-through joints | Joints are **reveals, not gaps**: the core slab is the full plan footprint extruded to height, inset from every visible face by `reveal_mm`; blocks protrude past it; where blocks meet, the recessed core shows as the mortar line. Union via manifold3d (proven in bases/trees); block bodies are convex hulls (proven in stones) |
| R4 | Per-block read (the quality bar) | Each block is an individual faceted body: a jittered-box convex hull (8 corners + face points, pulled inward randomly up to `chip_mm`) with small roundover and broadband `_relief_field` micro-relief along normals — the m50 chipped-stone read, every block unique by seed |
| R5 | Courses horizontal, grid broken | Layout solver: course heights sampled from a range (top course absorbs the remainder to land exactly at H); bay (block-length) cuts sampled per course with a **minimum bond offset** from the course below (rejection on near-alignment); per-block jitter in reveal (±~0.4 mm), tiny yaw, corner chips; occasional spall-bitten block |
| R6 | Flat cap textured as block tops | The cap is the same mechanism turned up: core top at `H − reveal`, block tops at `H`, flattened (crown-flat) — a flat plane of visible block tops separated by recessed joints, by construction. No soil on the wall |
| R7 | Corners interlock (quoins) | At each corner the two runs alternate per course: odd courses run A through the corner cell and run B butts into it, even courses swap. The alternating end-grain is the quoin read at every corner in every reference |
| R8 | Butt-join at tile seams | Layout is computed in tile coordinates; any block or core crossing the tile boundary is plane-cut flush at the edge (clean vertical cut — Shawn's answer 4 says that is correct) |
| R9 | Paint pipeline works (wash + 2 drybrush tones) | Joint reveal ≥ ~0.8 mm deep and ~0.6–1.2 mm wide holds the dark base wash; block micro-relief amplitude tuned so two drybrush tones catch (acceptance judged on renders + the Hirst painting criteria) |
| R10 | Supportless FDM | Vertical faces are FDM-friendly; the only overhang is each block's bottom lip protruding `reveal` over the joint below → every block bottom gets a ≥45° chamfer to the core plane, killing the overhang by construction. Cap is flat-up. Same two-tier target/strict audit idea as trees if renders show violations |
| R11 | Wall seats into terrain; grass steers around | Base course extends ~2 mm below terrain (embed); soil skirt laps the base exactly like stones (smoothstep annulus, upward-only); footprint stamped into `terrain_support_z` + `obstacle_mask` before Grass runs (existing layer-ordering contract) |
| R12 | Perf parity | A DB-standard wall on one 35 mm edge ≈ 5–7 courses × 2–4 bays ≈ 15–25 convex blocks + one core; hull + relief per block is the same cost class as scatter stones; one manifold union per wall. Well under a second |

## Authoring surface (gen-1)

Shawn's founding sketch — *"walls are a region with dimensions the
footprint of the wall, then a texture applied"* — is the model.  For
gen-1 the footprint is authored directly as a rectilinear plan, because
DB compatibility needs millimetre-exact straight slabs flush to tile
edges, which the organic flood-fill boundary machinery cannot express:

```python
from dharmatiles.walls import CutStoneWall

Region(id='ground', selector=FloodFill(0.5, 0.5), layers=[
    SoilCarpet(),
    CutStoneWall(                    # direct tile layer, like FacetedStones
        spine=[(0, 35), (0, 0), (35, 0)],   # plan polyline, tile mm —
                                            # the OUTER face line; here:
                                            # corner wall hugging W + S edges
        thickness_mm=7.0,            # extends inward from the spine
        height_mm=49.7,              # DB standard; 89.0 = tall
        seed=7,
    ),
    Grass(species=species),          # after the wall: blades steer around it
])
```

- The **spine polyline is the outer face** in plan; thickness extends
  inward.  A spine along a tile edge gives the DB flush-to-edge slab
  exactly.  Right-angle turns are gen-1 corners.
- `CutStoneWall` implements the standard `apply(scene, *,
  placement_mask)` protocol and lives with the other direct layers.
- The region-footprint front-end (arbitrary organic plans → the same
  layout solver) is the planned generalisation, not gen-1: the solver's
  input is a plan polygon either way, so nothing is thrown away.
- Masonry knobs (all defaulted): `course_height_mm` range, `bay_mm`
  range, `joint_mm`, `reveal_mm`, `chip_mm`, `roundover_mm`,
  `min_bond_offset_mm`, `spall_fraction`.

## The block primitive

A constrained cousin of the stone primitive (same module family,
`scatter/stones.py` machinery reused, not forked):

1. The layout solver hands each block its **cell**: an axis-aligned box
   (bay length × wall thickness × course height) minus `joint_mm/2` on
   each in-plane face.
2. Support points = the cell's 8 corners + edge/face midpoints, each
   pulled inward by `U(0, chip_mm)` (corners get the biggest pulls —
   chipped arrises).  Convex hull → watertight near-cuboid with
   planar chip facets.  This is the m50 read.
3. Small `roundover_mm` (~0.2–0.4) via the existing `_round_edges`;
   broadband `_relief_field` displacement along smoothed normals for
   face micro-relief (drybrush catch); `spall_fraction` of blocks get
   one `_weather_bites` scar.
4. Bottom face chamfered ≥45° toward the core plane (R10).
5. Deterministic per `(wall seed, course, bay)` — bit-identical
   regeneration, like stones.

Blocks are **not** full `build_stone` calls — no seat rotation, lean,
burial, or blur-remesh; a cheap dedicated path that reuses the hull /
roundover / relief / bites helpers.

## Layout solver

Straight runs + corners on flat ground:

1. Split the spine into straight segments; right-angle joints become
   **corner cells** (thickness × thickness plan squares).
2. **Courses:** sample heights from `course_height_mm` (default ~6–11
   at DB scale) until the remainder < max; the top course absorbs the
   remainder so the cap lands exactly at `height_mm`.
3. **Bays per course per segment:** sample lengths from `bay_mm`
   (default ~8–18); reject any vertical joint within
   `min_bond_offset_mm` (~3) of one in the course below; end bays
   absorb remainders.  Blocks at a tile boundary are plane-cut flush
   (R8).
4. **Corners:** per course, one run claims the corner cell (its end
   block extends through it), the other butts; parity alternates per
   course (R7).
5. Emit block cells → block primitive → `union(core, blocks)` →
   one wall mesh part.

The solver is pure geometry (cells in, cells out) and is the piece the
other masonry families re-parameterise or replace: brick = small
regular cells, fieldstone = packed irregular cells, ruins = cells
deleted by a decay field.

## Terrain integration

- Flat-ground assumption (gen-1 scope): seat plane = max terrain over
  the footprint; base course extends `embed_mm` (~2) below it; body
  sliced at the tile floor plane like deep-bedded stones.
- Soil skirt annulus around the footprint, upward-only, same helper as
  stones (R11).
- `terrain_support_z` over the footprint = wall cap height (rasterised
  the same batched way stones stamp); `obstacle_mask` stamped so grass
  and future placement steer around; runs inside `apply()` so the
  layer-ordering contract is unchanged.

## Acceptance scenes (render-judged, STLs to `stl/test/`)

| Scene | What it proves |
|---|---|
| `walls-e1-straight.tile.py` — 1×1, DB-standard wall on one edge | The core read vs `RR-095-Wall` rendered side-by-side (same camera via `stl_render.py`); joint depth, block variety, cap read |
| `walls-e2-corner.tile.py` — 1×1, wall on two adjacent edges | Quoin alternation; corner cell integrity |
| `walls-e3-tall+butt.tile.py` — 2×1, tall wall crossing the tile seam | 89 mm preset; clean butt-join cut at the seam |
| `walls-e4-meadow.tile.py` — wall + grass + scatter stones | Seating, soil skirt, grass steering, the full-tile composition |
| OL variants of e1/e2 | Scale correctness |

Named experiments (E1, E2, …) with kept renders, as in the rocks
campaign; losers deleted.  Stretch after the core read is accepted:
crack overlay crossing block boundaries (`_engrave_cracks` on the
unioned surface — the DB set-13 signature).

## Experiment log

| Exp | Change | Judgment |
|---|---|---|
| E1 | First build: hull blocks + core + reveal joints | Structure right (courses/bond/cap); blocks near-square (refs are ~3:1 long bricks); faces pillowy/busy — the rocks hero-face-calm lesson applies |
| E2 | course (5.5,8.5), bay long, chip 0.55, roundover 0.22, relief 0.07 | Very close to RR-095; exposed ragged tile-seam ends (butt-join defect) |
| E3 | `flush` end type: boundary ends overshoot + plane-cut at the seam; core overshoots too (no recessed ring on the mating face) | Seam flush ✓; front joints too faint — coplanar faces melt together at glancing light |
| E4 | joint 1.2, reveal 1.3, face-recess 0.6, per-block tilt ±0.7° | Joints read; each brick takes its own tone; corner quoins alternate correctly; shipped for judgment |

Implementation notes: blocks finish through `_blur_remesh` (σ 0.7) —
`subdivide_to_size` leaves T-junctions that break the manifold union.
Open items: wall layer costs ~8 s (design target was ≪1 s; block build
is embarrassingly parallel / batchable); face incidents (chips, crack
overlay) deferred until the core read is accepted.

## Explicitly out of gen-1

Ruin states, gates/doors/windows/arches, towers, crenellations,
walkable ramparts, wall-follows-slope, interior rooms, organic
region-footprint plans, families 2+ — all sit behind the same solver
and primitive; none require breaking gen-1's contracts.
