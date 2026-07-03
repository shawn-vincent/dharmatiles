# Rocks — Faceted Stones Design

**Status: design accepted-for-prototyping; no code yet (2026-07-03).**
Interview record and reference analysis:
`docs/meta/history/2026-07-03-rocks-greenfield-requirements.md`.
Reference images and the baseline judgment scene: `docs/rocks/`.
Third run of the leaf/grass development method: this doc restates the
interview requirements as by-construction guarantees and picks the
mechanisms; iteration happens on named render artifacts afterwards.

## Problem

Replace the half-ellipsoid dome rocks ("melted gumdrops") with stones
that read **faceted and bedded** — planar faces meeting at paint-catching
edges, buried to varying depth with soil lapping the base, scattered in
groups with a dominant stone and companions — matching the reference set,
under hard supportless FDM, at DB (35 mm/sq) and OL (25.4 mm/sq) scales.
Gen-1 ships **scatter stones only**; the primitive must extend to piles,
outcrops, and cliff faces without a rewrite.  Pebble-scale output (shore
bands) is already liked and must not regress.

## Requirements → guarantee mechanisms

| # | Requirement | Mechanism (by construction) |
|---|---|---|
| R1 | Faceted read, no ellipsoid patches | Stone IS a convex hull of M sampled support points — every face is planar with a distinct normal; there is no smooth surface anywhere to leak through |
| R2 | Slab / lump / shard variety, shards up to ~2× footprint, lean | Semi-axis triple + up-axis choice + lean angle are sampled shape params of the primitive |
| R3 | Edges catch drybrush; faces read at print scale | Support points sampled with a minimum angular separation → facet size is bounded from below; M (facet budget) is bounded from above |
| R4 | Supportless FDM at all scales | Post-sample overhang audit with corrective loop: reduce lean / deepen burial until every above-ground downward face is within the printable cone or under the small-chord allowance (tree-style target/strict two-tier) |
| R5 | Bedded, not perched | **Bed-to-widest rule** (2026-07-03, from Shawn's "kind of perched" judgment): burial is measured against the stone's widest horizontal cross-section — burial ≥ 1.0 buries to/past the widest line, so every visible flank slopes outward into the soil by construction; terrain is sealed to the underside (no waterline gap) and a smoothstep skirt annulus laps the base; deep-bedded bodies are sliced at a floor plane so they never punch through the tile |
| R6 | Clustered scatter | Group-first sampling on the existing Voronoi/Grouped machinery: dominant stone at the group seat, 0–3 companions placed adjacent at touching distance with decaying size, coherent yaw; loners are 1-member groups |
| R7 | Pebble bands stay good | Same primitive at small M — verified on the shoreline acceptance scene before the old kernel is deleted |
| R8 | Crown above thatch without per-tile hand-sizing | `min_crown_mm` param drives the size/burial solver in grass regions; the manual crown-rule sizing in the two tree tiles is deleted once accepted |
| R9 | Extends to piles / outcrops / cliffs | Composition is separate from the stone primitive: piles = stacked seats, outcrops = leaning shard groups, cliffs = stratified courses — all arrangements of the same StoneSeed; nothing in the primitive assumes ground contact |
| R10 | Perf parity (rocks ≪ 1 s/tile) | Tens of stones/tile; per-stone `scipy.spatial.ConvexHull` on ≤ ~24 points is ~0.1 ms; support_z rasterisation stays the batched box scheme of the current kernel |
| R11 | Crack engraving on large stones (gen-1) | Only faces above an area threshold are engraved → pebbles/mediums untouched by construction; grooves are boolean-subtracted wedge prisms (manifold3d, robust on convex bodies) |

## The stone primitive

1. Sample M support directions quasi-uniformly on the sphere with a
   minimum angular separation (R3).  M is the **weathering knob**:
   M ≈ 8–12 → angular shard/slab read (monolith trio);
   M ≈ 16–24 with milder anisotropy → weathered cobble read
   (dirt-path pebbles).  One system, two settings (interview decision 2).
2. Radius per direction = semi-axis triple (a, b, c) × lumpiness jitter.
   Class mix sampled per stone: **slab** (short axis up), **lump**
   (equant), **shard** (long axis up, height to ~2× footprint, lean).
3. Convex hull → watertight faceted polyhedron.  Flat-shaded faces are
   the paint feature; no smoothing anywhere.
4. Yaw (group-coherent ± jitter), lean, burial applied as a rigid
   transform; then the R4 corrective loop clamps lean/burial until the
   overhang audit passes.  Bounded and deterministic per seed.

`StoneSeed` (new, `scatter/` alongside `RockSeed`) carries the fully
resolved sample: centre, semi-axes, up-axis, yaw, lean, M, jitter seed,
burial fraction.  Sorting stays big→small for stamping.

## Bedding

- Seat height = MAX terrain over the footprint disk (a soil-carpet mound
  can never entomb a stone).
- `burial` is relative to the widest horizontal cross-section (searched
  in the bottom 65 % of the stone so a leaning shard's tip doesn't win):
  1.0 = bedded exactly to the widest line.  Bodies below the tile floor
  plane are sliced off (convex ∩ half-space, still watertight).
- `terrain_support_z`: for a convex body, the top surface at (x, y) is
  the minimum over upward-facing face planes — rasterised per stone into
  the same batched bounding-box scheme the current kernel uses;
  `obstacle_mask` stamped identically (grass/tree contracts unchanged).
- Soil skirt: an annulus (~1–1.5 mm) around the footprint raises
  terrain_z with smooth falloff (~0.3–0.5 mm), only upward — soil laps
  the stone, killing the waterline gap.  Runs inside the Rocks layer, so
  layer ordering contracts are unchanged.

## Cracks (R11)

Implemented 2026-07-03 (E5), landed after six render-judged iterations —
the naive version carved invisible or artifact-reading grooves.  What
survived:

- **Per-stone cap, not per-face**: candidate faces (hull triangles are
  never coplanar, so a "facet" is a triangle) above ~8 mm² are ranked by
  `area × (0.4 + n_z)` — up-facing preferred, because tabletop stones
  are viewed from above and low cracks land at the soil line where the
  skirt swallows them.  Top 3 get ONE crack each; per-face swarms chewed
  big faces into slivers.
- **Ground filter**: faces whose centroid is below terrain + 0.5 mm are
  skipped (buried cracks wasted booleans and chewed the soil contact).
- **Horizontal bias**: cracks start within ±30° of the horizontal
  in-plane direction (stratification read).  Vertical cracks on tall
  faces read as mesh artifacts, not geology.
- **Proportions are the crack read** (Shawn's MeshLab verdict killed the
  first landed version: 1.3 mm-wide kinked slots "don't look like cracks
  AT ALL" — they read as router pockets).  A crack must be LONG and
  THIN: width 0.5 mm, depth 0.55 mm, total length ~7–11 mm.
- **Surface-projected random walk**: ~6 segments stepping 1.1–1.9 mm in
  the local tangent plane with ±24° heading jitter, each point
  reprojected onto the hull (`trimesh.proximity.closest_point`), walking
  both directions from the seed so the crack straddles it and crosses
  arrises naturally.
- **Tapered chain**: each segment is a triangular frustum; width/depth
  follow a sin^0.6 profile over the polyline — the groove fades in and
  out like a real crack instead of ending in a square notch.
- Boolean: `trimesh.boolean.difference(engine='manifold')`, one call per
  stone with all wedge segments; falls back to the uncracked stone on
  failure.
- Stamping happens BEFORE engraving — the stamp math assumes convexity,
  and grooves are too small to matter for support/obstacle fields.

Displacement-map engraving was rejected: it needs dense tessellation,
which destroys the flat-facet read.

## Placement

Public API unchanged: `Rocks(..., placement=…)` stays the layer.  A new
composition step between position sampling and seed building:

- Group seats from the existing `Grouped`/`scatter_positions` machinery
  (`Uniform` still works for plain scatter like shore pebbles).
- Per group: dominant stone at the seat; 0–3 companions at distance
  ≈ (r_dom + r_comp) × 0.9–1.1 (touching or near-touching), size decay
  ~ D[0.4:0.75]; shared yaw ± jitter.  Optional debris halo of pebbles.
- Grass-region stones get the `min_crown_mm` solver (R8).

## Keep / delete

- Keep: placement machinery (`scatter/distribute.py`), layer ordering
  contracts, support_z/obstacle stamping semantics, the `Rocks` public
  API surface.
- Old dome kernel (`layers/rocks.py` ellipsoid path) is kept until all
  four acceptance scenes are accepted, then **deleted** (method step 7)
  and the crown-rule hand-sizing in the two tree tiles removed.

## Acceptance scenes & iteration protocol

| Scene | Content | Proves |
|---|---|---|
| S1 monolith-trio | bare soil, one shard+companions group, close-up | R1 R2 R3 R6 vs `rocks-reference-monolith-trio.png` |
| S2 scatter-field | `docs/rocks/rocks-judgment.tile.py` with new stones | R1 R2 R5 R6 vs `rocks-current-2026-07-03.png` |
| S3 meadow-stones | `1x1-grass-tree` | R8 crowns above thatch, no hand-sizing |
| S4 shoreline | `1x1-water+soil` | R7 pebbles no worse; water boulder not a melted tent |

Experiments E1 (single-stone shape classes) → E2 (trio) → E3
(bedding/skirt) → E4 (cluster field) → E5 (cracks) → E6 (meadow crown)
→ E7 (shoreline), one render-judged artifact at a time, renders named
`docs/rocks/experiment-YYYY-MM-DD-stones-*.png`, knobs as module
constants until acceptance.  Perf budget: rocks layer ≪ 1 s at typical
densities, both scales.
