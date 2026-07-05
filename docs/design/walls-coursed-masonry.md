# Walls — Coursed Masonry Design (gen-1)

**Status (2026-07-05): cut-stone family shipped-quality per Shawn's
round-2/round-5 verdicts; fieldstone family rebuilt as crack-network
tessellation (E12, approved) and iterated E13–E25 under Shawn's
direction — current state E25 (perched top rocks, bed+head overlap,
pebble morph, rocks texture) awaiting judgment on
`docs/walls/renders/walls-fieldstone-{front,corner,top}.png`.
Fieldstone reference set + geometric truths:
`docs/reference/walls/fieldstone/README.md`.**
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
| R8 | Butt-join at tile seams | Wall ends are ALWAYS textured — visible block ends with the mortar core inset behind them, at tile boundaries too; two wall tiles butt with their sculpted ends, like the commercial pieces do. (Round-2 revision: E3's flush plane-cut ends were built first; Shawn: "I should be able to see the ends of the bricks, and the mortar should be inset there as well.") A safety plane-cut still trims anything poking past the tile bounds |
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
| — | **Shawn round-2 verdict**: "incredible… beautiful" + five fixes | (2) ends textured, not flush-cut; (3) grass must not run up the wall face; (4) thinner mortar, bricks nearly touching; (5) raised shapes mid-face/top = hull plateau from full-extent face-centre points; (6) explore texture options |
| E5 | Ends always 'face' (flush machinery deleted); joint 0.45; face centres pulled in ~0.5×chip (plateau gone by construction); grass `_CLIFF_CLAMP_MM` in thatch.py (support taller than 4 mm above substrate is cliff-class — drape/lift see at most the clamp); texture presets `chipped/worn/hewn/dressed` + walls-e5-textures comparison scene | Shawn: "this looks great" |
| E6 | **FieldstoneWall** (family 3, `walls/fieldstone.py`): same chassis, unit = hull of Fibonacci points on an ellipsoid↔box blend (`blockiness`), lumpiness, vertical bulge into deep joints, flat coping cap, squared quoins. Three sub-iterations: (a) inscribed ellipsoids read "pebbles stuck on a slab" → blockiness up, joints tighter; (b) roundover+blur shave ~1.5 mm and reopen the joints → `_PACK_COMP_MM` oversizes each stone before rounding; (c) rounded BUTTING stones curve away from the corner arris leaving V-notches → both cells at a corner are quoin-class (squared, chipped-preset params) | **Shawn: "terrible"** — flatter stones; stones (not bricks) in the corner; nearly touching; more irregular sizes |
| E7 | Fieldstone rework per round-3: (a) slab squash of support directions (`_FLATTEN`) — stones are flat-laid slabs; (b) corner quoins are the same fieldstone unit at high blockiness, `_block_mesh` bricks removed from this wall; (c) size irregularity from the GRID (wider course/bay ranges + `_cells` post-pass merging ~1/5 of cells into double-height throughstones), never from under-filling — an under-filled cell exposes flat core = "pebbles glued on a slab" (re-found); (d) packing: `_PACK_COMP_MM` 2.1 applied in ALL THREE axes — y (thickness) compensation was missing, so roundover+blur left every stone nearly flush with the core and the wall read as a flat slab with faint lumps (the standalone strip test missed it: no core to compare against); joint 0.4 | **Shawn**: outside has flat-faced rocks; fill every space (more small rocks); full rock faces on top/sides; mortar mostly invisible |
| E8 | (a) **Exact-fit rescale replaces shave compensation**: build oversize (rounding keeps curvature), then rescale each stone to its target box — bellies tangent to face planes by construction; the flat outside faces were the tile-boundary clip planing off stones whose compensated bulge overshot the outer plane (guessing the shave is unstable in both directions); (b) **pinning stones**: small stones at joint × course-line crossings (`_PIN_PROB` 0.55) fill the star voids four rounded shoulders open, plus flush cap-joint pins; (c) reveal 1.6 — with tangent packing the core shows only deep in crevices | **Shawn: "not enough"** — render mortar RED, top+4 sides, iterate until it reads STACKED FIELDSTONES, not fieldstones in mortar |
| E9 | **Red-mortar diagnostic** (`src/extras/wall_mortar_diag.py`: core red, stones grey, no union, top+4 sides, red-pixel fraction printed; below-soil clipped). Round 1 measured 17 % red on faces — a rounded stone touches its face plane at ONE TANGENT POINT, so straight-on the wall was stones floating in mortar. Fixes, in causal order: (a) `_FLATTEN_Y` face squash — builders turn the flattest side out; broad face patches (17→9 %); (b) `_OVERLAP_MM` past joint midlines — stacked stones TOUCH, union merges contacts into bedding creases; blockiness floor 0.55 — below ~0.5 the hull silhouette recedes between corners and shared edges open into channels (9→4.6 %); (c) through-flank pins + `_PIN_PROB` 1.0 (4.6→3.5 %); (d) **rubble hearting** (`_extra_parts` hook): the by-construction end-state — pins chase voids case-by-case but void shapes are unpredictable; a half-pitch-staggered overlapping sheet of cheap rubble stones (hull+roundover only, no remesh) through the wall body makes every void a window onto deeper stones, never onto the core plane (3.5→**0.4–0.8 %**, and the leftover flecks read as deep-void shadow) | **Shawn: "rocks can't overlap — fused rocks, not a carefully stacked pile; look at the reference"** |
| E10 | **No overlap anywhere** — the E9 overlap erased the dark outline around every stone, which IS the stacked read (reference photo). Stones exact-fit their jointed cells (joint 0.9 real gaps); overlap/bulge deleted; pins deleted (they straddle crossings → would fuse; hearting owns gap depth). Hearting became the sealed layer: footprint ≥2× pitch (hull corners recede ~25 % in their boxes — box-touching left 3.8 % measured holes), two y-layers staggered half-pitch in t AND z, setback 1.6 vs core reveal 2.8 (at setback 0.15 the rubble BURIED the coursing; at 2.0 vs reveal 2.2 oblique rays leaked past rubble edges). Red 0.00–0.06 % all five views. Instrument lesson: the flat-shaded diagnostic under-reads gap-depth contrast (no shadows) — over-tuning against it (wider joints, domed faces) mushed the real-light read; final judgment on the pyrender PNG + MeshLab | **Shawn: still not the reference** — the exact-fit rule makes every belly tangent to the SAME plane, so the wall face is geometrically flat and reads as texture embossed on a slab; "everything is up for grabs" |
| E11 | Per-stone recession + gravity pockets + chinkers on the cell-hull chassis (never judged — superseded mid-round). Kept findings: (a) rubble hearting must honour the face setback in **t** too — segment END planes are visible faces (free ends, corner arris), and 0.3 mm-clipped rubble read as a broken pebble column down the arris; (b) no bay cut inside an owned corner cell (+2 mm margin) or a sliver quoin breaks the corner; (c) int key tags, not strings — `_place_block` hashes `cell.key` and str hashes vary per process | superseded by round-8 redirect |
| E12 | **Approach reset (Shawn round-8: "distinct natural stones, fitting together with tight cracks … stones fitting into all the space; find 10 reference images and look carefully").**  Reference set collected → `docs/reference/walls/fieldstone/README.md`; the images agree: adjacent stones SHARE edge geometry (parallel edges, bulge-fits-notch), stone covers ≥90 % of the face, cracks are thin (~0.3–0.7 mm at scale) and uniform.  Independent per-cell hulls can never produce that complementarity — root cause of E6–E11.  Rebuilt **crack-network-first**: courses/bays give the crack topology; each bed line gets a shared wobble, each head joint a shared slant (both sides evaluate the same cached curve → complementary by construction); stone face outline = its bounded polygon inset joint/2 (shapely) with rounded corners; solid = outline lofted through the thickness with a gentle belly; per-stone tone from belly-apex drift + proudness, NO whole-stone rotation (it would open wedges).  Finishing: subdivide → broadband relief → Taubin; **no blur-remesh** (near-planar faces are marching cubes' terracing case — rendered as radial/banded artifacts) and **no centroid-fan caps** (skinny triangles streak under smooth shading) — caps are Delaunay over ring + interior grid; ring keeps shapely's corner vertices (`segmentize`, not uniform resample — under-sampled 0.42 mm corner arcs banded).  Red-mortar 0.00–0.03 % all five views; ~5 s build (no remesh) | **Shawn: "that's great. I love it"** + one change → E13 |
| E13 | **Shorter stones** (Shawn: more horizontal cracks): courses 2.8–7.5 (was 3.4–9.6), h-split prob 0.28 at ≥5.5 mm (was 0.18 at ≥6.5), bed wobble amp per sine 0.14–0.32 (was 0.18–0.40 — the thinnest split stones drop to ~2.5 mm and two independent beds must never cross).  Throughstone merges kept at 0.20 for size contrast.  Red 0.00–0.02 % | Shawn: ✓ + next tweak |
| E14 | **Hairline cracks** (Shawn: nonzero but ~0.1 mm): `joint_mm` 0.5 → 0.1.  The physical gap closes to 0.1 mm at mid-depth; the VISIBLE crack stays a dark V-groove because the belly taper opens each stone ~0.2–0.5 mm at the face plane.  Relief (±0.10) can exceed the 0.05/side inset at mid-depth → occasional invisible belly kisses, which the union fuses (real stones touch; prints fine).  Red 0.0000 all five views | Shawn: "pretty good" + next tweak |
| E15 | **Just the cracks and the roundovers** (Shawn: drop the face pillowing): belly/apex-drift deleted; stones are straight extruded prisms; every edge rounded — outline corners by the 2D buffer, face↔side edges by a circular-arc inset over the first/last `roundover_mm` of depth (cosine-spaced arc stations); `relief_mm` default 0 (the plane-wave fan-streak artifact went with it); per-stone tone now carried by proudness alone.  Half the mesh (110 k faces per corner tile).  Red 0.0000 all five views | Shawn: ✓ + next tweak |
| E16 | **Big random roundovers** (Shawn: way more, randomised): `roundover_mm` is now a per-stone RANGE, default 0.9–2.0 mm, applied to both the 2D outline corners and the face↔side arc; ring normals smoothed 2 passes (raw vertex-normal insets fold at concave drift jogs at these radii); cap-triangulation grid clearance follows the roundover.  Per-stone cap `_ROUND_SIZE_CAP` = 0.22 × the outline's smaller dimension — at 0.38 the roundover ate the faces of thin stones and they read as dark slots.  Red 0.0000 all five views | Shawn: "pretty good" + next step |
| E17 | **Full pebble morph** (Shawn: "an isosphere morphed to fit the outline/depth of the cracked region … an irregular stone instead of a block with rounded corners").  Sphere topology per stone: longitude follows the crack-bounded outline (the EQUATOR ring is the outline at mid-depth — silhouette, cracks, and dimensions unchanged); latitude sweeps a superellipse meridian `s=(1−|sin lat|^a)^b` from face pole to face pole, exponents drawn per pole (asymmetric bulge), poles drifting off-centre.  A true ellipse meridian (first cut) read as MELTED dough — all curvature at the silhouette, equator-line contact, cracks became soft wide valleys; the fuller superellipse (a 2.0–3.2, b 0.30–0.45) keeps a gently domed face and drops fast near the silhouette so cracks stay narrow and dark.  Delaunay cap machinery deleted (sphere poles are small fans on curved domes — clean under smooth shading); cap stones no longer crown-flat (R6 relaxed for this family: rounded cobble tops).  Red 0.0000 all five views | Shawn: ✓ + next tweak |
| E18 | **Zero gap** (Shawn: with the pebble morph the cracks are the curved edges themselves): `joint_mm` 0.1 → 0.0 — neighbouring stones TOUCH at their shared equator lines; the crack is a V-groove with no gap at its root, drawn entirely by the two pole-ward curvatures meeting.  Manifold union handles the line contacts cleanly (bed contacts micro-overlap from independent polyline sampling of the shared curve; head joints share exact segments); no watertightness warnings.  Red 0.0000 all five views | Shawn: ✓ + next tweak |
| E19 | **Flat beds, shorter courses** (Shawn: piling flat stones, not curved; fewer tall cross-row stones; +20 % horizontal cracks): the morph goes anisotropic — z-scale = s^p with p 0.22–0.42 per stone (`_BED_FLAT_EXP`), so bed surfaces stay near-flat planes over most of the depth and only turn down at the face poles; courses 2.3–6.2 (−20 %); throughstone merges capped at 9.5 mm total height (`_THROUGH_MAX_MM`).  Red 0.0000 all five views | Shawn: "pretty good!" + next step |
| E20 | **Scatter-stones surface texture** (Shawn: apply our existing rock texture): `_stone_texture` ports the shipped stones.py aged pass at wall-stone scale — broad organic undulation (6 waves, foot/4.5…foot/1.6, spectral 1.0; auto amplitude clip(0.03·foot, 0.10, 0.30)) + granular drybrush tooth (16 waves 0.9–3.2 mm, clip(0.012·foot, 0.10, 0.25)×0.5), × patchy 3-wave envelope (floor 0.2), displaced along Taubin-smoothed normals with curvature damping — which also protects the crack roots, exactly where the zero-gap stones touch.  `relief_mm=None` → auto; 0 disables; a number overrides the undulation amplitude (base-ctor preset substitution bypassed post-super).  Red 0.0000 all five views | Shawn: ✓ + next tweak |
| E21 | **Rougher, irregular, wandery** (Shawn): texture amplitudes up (undulation clip(0.045·foot, 0.14, 0.45), grain clip(0.016·foot, 0.12, 0.30)×0.6); head joints BOW ±1.1 mm at midpoint (shared parabola, zero at the beds — both neighbours evaluate the same curve); bed wobble amp 0.16–0.38 at shorter wavelengths 11–32 (course min raised 2.3 → 2.6 to keep the thinnest waists chink-class); per-stone INWARD-only outline noise (2 sinusoids over the perimeter) makes silhouettes irregular and cracks open/close along their run without touching the no-overlap guarantee | folded into E22 |
| E22 | **Rounder, less gap + watertight fix** (Shawn): roundover 1.3–2.6 (size cap 0.26), morph shoulders slightly fuller (b 0.33–0.48); outline noise reduced to 0.04–0.16 mm **with a floor** — E21's zero-noise minima left surfaces exactly tangent, and one tangency survived the union as a single non-manifold edge after float32 STL rounding; the hairline floor keeps every contact transversal.  Exported STL watertight again (0 open, 0 non-manifold).  Red 0.0000–0.0001 | Shawn: ✓ + next tweak |
| E23 | **Slightly rounder beds + the real pinch fix**: `_BED_FLAT_EXP` 0.22–0.42 → 0.30–0.52 ("a bit" rounder tops/bottoms).  The non-manifold edge returned — root cause understood: zero-gap stones + texture displacement leave a near-zero clearance somewhere every build; the mesh stays INDEX-manifold so re-canonicalising through manifold is a no-op, but STL reload merges vertices by POSITION and collapses the pinch.  `_separate_pinches` (masonry chassis): after the union, kD-tree finds coincident vertex pairs that aren't topologically adjacent and nudges each 0.15 µm inward along its own normal — every contact stays separated past the float32 grid.  Exported STL watertight, 0 non-manifold | Shawn: ✓ + next tweak |
| E24 | **Head-joint overlap** (Shawn: reduce horizontal gaps quite a bit more — let stones overlap so visible mortar reduces; top row untouched): each stone extends past its head-joint lines by U(0.45, 0.95) mm per side (capped at 0.25× stone width), the union fusing side-by-side neighbours into a pressed crease.  Deliberately relaxes the E10 no-overlap rule for head joints only — beds stay shared curves, and with the pebble morph the fused contact reads as stones pressed together, not the E9 melted mass.  `is_top` cells exempt.  Watertight, red 0.0000–0.0001 | Shawn: ✓ + next tweak |
| E25 | **Perched top rocks + bed overlap** (Shawn: remove mortar between head stones — rocks just sitting on top; +20 % horizontal cracks; why are end rocks sheared?; more face-protrusion variance; rounder top/bottom with overlap like the head joints): (1) top course = perched rocks — the core stops just BELOW the top-course bed (`_core_boxes` override via `_cap_z0`; rubble clipped under it too), so gaps between cap stones look onto the stones beneath, never mortar; the cap stones are attached by the bed overlap pressed up from the course below.  (2) Beds get the E24 overlap treatment: stones dilate vertically U(0.35, 0.80) mm past their bed lines (cap 0.22× height; `is_top` exempt, bottom course keeps its seat), fusing courses into pressed contacts.  (3) Courses 2.2–5.2 (−20 %).  (4) End-rock shearing diagnosed: texture displacement pushed end-stone surfaces past the tile boundary and `_clip_to_tile` planed them flat — 'face' ends now recede 0.7 mm (`_END_MARGIN_MM`), leaving full rounded stones at tile edges.  (5) Proud faces U(0.10, 0.70) with a 15 % deep tail to 1.10 (`_PROUD_*`).  (6) `_BED_FLAT_EXP` 0.40–0.65 (rounder beds — safe now that beds overlap).  Pinch nudge made asymmetric (0.15/0.30 µm) + iterated — parallel-normal pairs moved in lockstep and stayed coincident.  Watertight (0 open, 0 non-manifold), red 0.0000 all five views | pending Shawn |

Implementation notes: cut-stone blocks finish through `_blur_remesh`
(σ 0.7) — `subdivide_to_size` leaves T-junctions that break the
manifold union; fieldstone (E12) finishes loft-native (subdivide →
relief → Taubin) because the remesh terraces its near-planar faces.
Open items: cut-stone wall layer costs ~8 s (design target was ≪1 s;
block build is embarrassingly parallel / batchable; fieldstone is ~5 s
already); faint residual cap-triangulation streaks on a few fieldstone
faces at grazing light; face incidents (chips, crack overlay) deferred
until the core read is accepted.

## Explicitly out of gen-1

Ruin states, gates/doors/windows/arches, towers, crenellations,
walkable ramparts, wall-follows-slope, interior rooms, organic
region-footprint plans, families 2+ — all sit behind the same solver
and primitive; none require breaking gen-1's contracts.
