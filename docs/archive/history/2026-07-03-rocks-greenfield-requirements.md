# Rocks Greenfield — Requirements Interview & Reference Analysis (2026-07-03)

Third application of the development method that cracked leaves
(`2026-07-03-leaf-placement-complete-history.md`) and grass
(`2026-07-03-grass-greenfield-requirements.md`): render-and-judge the
current output, requirements interview against concrete analyzed
references, requirements restated as by-construction guarantees, named
acceptance scenes and a perf budget before any code.  This entry is the
interview record; the design doc comes next (does not exist yet).

Reference assets (all in `docs/rocks/`):
- `rocks-current-2026-07-03.png` — baseline judgment scene (bare soil,
  boulder/medium/pebble bands of the current kernel); spec saved as
  `rocks-judgment.tile.py`, close-up in `…-boulder.png`.
- `rocks-reference-monolith-trio.png` — **primary shape reference.**
  Painted tabletop stones: tall shard + two companions.
- `rocks-reference-outcrops-and-scatter.png` — resin/FDM outcrop shards
  (The Pilgrim) + low-poly "asteroid scatter" rock set.
- `rocks-reference-dirtpath-pebbles.png` — **primary bedding/context
  reference.** Commercial dirt-path tiles with half-buried cobble groups.
- `rocks-reference-big-rock-outcrop.png` — tile-scale outcrop, fluted
  cliff sides, plateau top with engraved cracks + debris.
- `rocks-reference-waterfall-piles.png` — boulder piles as walls, pebble
  beach lines, stones in water.
- `rocks-reference-stairs-grotto.png` — stratified cliff faces, rock
  stairs, grotto arch.
- `rocks-reference-high-waterfall-cliff.png` — tall cliff columns capped
  with boulder piles; grey + brown two-tone rock.

---

## Step 1 — side-by-side judgment (current output vs references)

Current kernel (`layers/rocks.py`): half-ellipsoid + 3–5 plane cuts +
2% vertex noise + slope alignment + fixed 0.1–0.3 mm sink, i.i.d.
uniform placement.

Named defects (Claude's read, confirmed by Shawn: "the current domes
suck"):

1. **Domes, not stones.** The ellipsoid base form always reads through;
   cuts remove volume but the silhouette stays convex and round.  The
   big water boulder reads as a melted tent.
2. **No facets to catch light.** Continuous shading gradient; nothing
   for a drybrush (or the renderer) to pick out.
3. **Perched, not bedded.** Constant sub-mm sink; stones sit *on* the
   soil like dropped candy.  No skirt, no burial variation.
4. **No scatter structure.** i.i.d. uniform placement; no groups, no
   big-stone-with-companions, no debris.  Same defect grass had before
   sheaves.
5. **Pebbles are fine.** At 1–2 mm the dome read disappears; shore
   pebble bands are accepted.  The failure grows with stone size.

Non-convergence smell present: crown-rule commits (cd2e9ed, fc50321)
sized rocks *around* the drowning problem per tile; `n_cuts`/`cut`/
`roughness` have been nudged repeatedly without changing the round read.

## What the references actually do (analyzed, not just invoked)

1. **Faceted polyhedral bodies.** Every stone — angular shard or
   weathered cobble — is a set of planar-ish faces meeting at crisp
   edges.  The *edges* are the paint feature: drybrush catches them,
   washes pool in the valleys between faces.  Even the "rounded"
   dirt-path cobbles are lumpy asymmetric polyhedra with crease lines,
   never smooth domes.
2. **Silhouette variety with vertical reach.** Slabs, equant lumps, and
   tall shards up to ~2–3× taller than their footprint, often leaning.
   The monolith trio's drama is the aspect contrast within one group.
3. **Group composition.** Stones come as a dominant stone + smaller
   companions touching or near-touching, size-ordered, plus loners and
   debris halos.  Piles are many stones stacked with individual
   identity preserved.
4. **Bedded in the ground.** Half-buried pebbles; soil/grass laps
   against stone bases; no waterline gap.  Burial fraction varies
   per stone.
5. **Crack/crevice engraving** on large faces (big-rock outcrop,
   cliffs) — negative detail lines that washes can pick up.  Appears at
   outcrop scale, not on small scatter stones.
6. **Big architecture is stratified/stacked.** Cliffs and outcrops read
   as courses/tiers with plateau tops, debris at the foot, grass tufts
   in crevices.  (Future scope, but the stone primitive must not
   dead-end short of it.)

## Interview answers (Shawn, 2026-07-03)

- Judgment: confirmed — the domes suck.
- Scope: **placement AND shape AND bedding.**  Ultimately wants
  **outcrops, piles, cliff faces** (later generations, but requirements
  must anticipate them).
- Paint-catching is an **explicit requirement**.
- **FDM no-supports** is hard.
- Perf: **parity with today** (rocks are currently nearly free).

## Requirements (testable; guarantee mechanism candidates in third column)

| # | Requirement | By-construction candidate |
|---|---|---|
| R1 | Every stone reads as a faceted polyhedron: planar faces meeting at crisp edges; no ellipsoid patch visible at any size above pebble scale | Stones ARE polyhedra by construction (bounded plane-cut hull from all directions), not textured domes |
| R2 | Silhouette variety: slab / equant lump / tall shard within one system; shard height up to ~2× footprint; yaw + lean variation | Aspect + lean are sampled shape params of the primitive, not post-hoc scaling |
| R3 | Edges catch paint: adjacent faces meet at dihedrals strong enough to drybrush (and to shade distinctly in renders); faces large enough to read at print scale | Minimum facet size / bounded facet count baked into the cut sampler |
| R4 | FDM no-supports: every downward-facing facet within the printable overhang cone, at every scale | Post-generation overhang audit + corrective clamp, same spirit as the tree `strict_fdm_angle_deg` backstop |
| R5 | Bedded, not perched: per-stone burial fraction varies (roughly 15–60 % of height); soil laps against the base (skirt), no waterline gap | Burial is a sampled param; skirt stamped into terrain_z around the footprint (thatch already proves the skirt mechanism) |
| R6 | Clustered placement: dominant stone + size-ordered companions + loners; optional debris halo; reads like the monolith trio | Group-first sampling (existing Voronoi/Grouped machinery): sample groups, then members within a group with size decay |
| R7 | Keep what's liked: shore/pebble bands at 1–2 mm stay at least as good as today | Pebble scale is the same primitive with facet count floor — verify on the shoreline acceptance scene |
| R8 | Crown rule by construction in grass regions: exposed crown above the ~3.4 mm thatch canopy parameterised once, not hand-sized per tile | Min-crown param drives the size/burial sampler; delete the per-tile manual sizing |
| R9 | Extensible primitive: the same stone + composition machinery must be expressible as piles (stacked), outcrops (leaning shard clusters), cliff faces (stratified) without a rewrite | Architecture review question at design time; gen-1 ships scatter only |
| R10 | Perf parity with today (rocks are a rounding error in tile build time; stay that way — budget ≤ ~1 s added per tile at DB scale, typical density) | Keep the vectorised batch spirit: generate all stones' geometry in NumPy passes |
| R11 | Large stones carry crack/crevice engraving — negative lines a wash can pool in (gen-1, per Shawn) | Engrave only faces above a size threshold, so pebbles/mediums are untouched by construction |

## Named acceptance scenes

| Scene | Content | What it proves |
|---|---|---|
| S1 monolith-trio | Bare soil, ONE group: tall shard + two companions, close-up render | R1 R2 R3 R6 — the money shot vs `rocks-reference-monolith-trio.png` |
| S2 scatter-field | `docs/rocks/rocks-judgment.tile.py` re-rendered with new rocks | R1 R2 R5 R6 side-by-side vs `rocks-current-2026-07-03.png` |
| S3 meadow-stones | `1x1-grass-tree` meadow | R8 crowns above thatch without manual sizing |
| S4 shoreline | `1x1-water+soil` | R7 pebble bands + a water boulder that isn't a melted tent |

Perf budget: full-tile build parity with today; the rocks layer itself
stays ≪ 1 s at typical densities, both scales.

## Scope decisions (Shawn, 2026-07-03, post-interview)

1. **Gen-1 = scatter rocks only** (shape + bedding + clusters).
   Piles/outcrops/cliffs are later generations; the design must prove
   the primitive extends to them (R9) but gen-1 does not ship them.
2. **One primitive with a weathering knob.**  Angular faceted
   polyhedron is the base form; a weathering parameter sweeps
   continuously to the cobble read.  Monolith trio and dirt-path
   pebbles are two settings of one system, not two systems.
3. **Crack engraving IS in gen-1** (R11) — Shawn overrode the deferral
   recommendation: wash-paintability on the biggest scatter stones
   matters now.  Small/medium stones stay untouched.

Design doc: `docs/design/rocks-faceted-stones.md`.
