# Grass Greenfield — Requirements Interview & Design Decisions (2026-07-03)

**Written the same day the leaf-placement complete history was recorded**
(`2026-07-03-leaf-placement-complete-history.md`), deliberately reusing its
process recipe: render-and-judge the current output, requirements interview
against a concrete analyzed reference, requirements restated as
by-construction guarantees, named acceptance scenes and perf budget before
any code.  This entry is the interview record and the reasoning; the design
itself is `docs/design/grass-mound-thatch.md`.

Reference assets (all in `docs/grass/`):
- `grass-reference.png` — the target look (painted render, two tiles).
- `grass-reference-printed.png` — a *physical FDM print* of that style of
  grass, photographed.  Proof the look survives printing: the lumpy relief,
  swirled thatch, and hollows all read at arm's length.
- `grass-current-2026-07-03.png` — baseline render of our current
  `1x1-grass` (elev 40, azim −135), the "before" picture.

---

## Step 1 — side-by-side judgment (render vs reference)

What the reference actually does (analyzed, not just invoked — the leaf-era
lesson):

1. **Blades are plump, pod-like 3D bodies** — rounded cross-sections that
   catch light on their bellies, tapering to points.
2. **Sheaf structure** — bundles of ~5–15 near-parallel blades leaning
   together like combed locks; adjacent sheaves point in visibly different
   directions → a swirled, thatched read.  Chaotic-but-structured.
3. **Real vertical relief** — sheaves mound up and layer over each other;
   the surface undulates by 2–4 blade thicknesses.  Mounds where clumps
   pile, hollows between.  In the printed photo this is the dominant
   feature: the silhouette is lumpy, not flat.
4. **Tips lift** off the mass at crests and edges (painted render; less
   visible in the print).
5. **Two-scale texture** — a finer, denser grass layer visible in hollows
   and at edges beneath the big blades.
6. **Edges are generous** — blades drape slightly over the soil rim.

What the current grass does: total coverage ✓, coherent clump swirls ✓ —
but it reads as a **flat mat**: near-constant-width ribbons pressed flat,
no mounding, no layered relief, no silhouette.  A texture, not a thicket.

## Step 2 — the interview (Shawn, 2026-07-03)

- **Q: what's liked vs what fails?**  A: *blade shape is fine.*  The
  failures are coverage, clump direction, and 3-D shape — "the reference
  ends up being lumpy and organic, current grass renders flat with grass
  flat across a surface."  → Scope: the blade mesh machinery survives; the
  *arrangement and relief* are rebuilt.  (Same shape as the leaf era:
  "leaf shape fine, arrangement is the failure.")
- **Q: essential thing in the reference?**  A: **mounded relief is key**;
  lifted tips are **dispensable**.  → The design optimizes for relief
  first; tip lift is not a requirement at all (good — it was the FDM-risky
  part).
- **Q: print constraints?**  A: hard supportless FDM; **5 mm is as high as
  grass should go**.
- **Q: keep the growth machinery?**  A: **greenfield is fine.  Key is
  aesthetics and speed.**  Obstacle-steering simulation is not a valued
  behaviour per se.

## The two load-bearing insights

### 1. The current flatness is *by construction* — parameter iteration cannot fix it

The current grower's core rule is literally *"lie as flat as possible;
rise only when something is in the way; drop back as soon as clear"*
(`docs/grass/grass-behavior.md`), and REQ-BLD-5 in the old requirements
doc **mandates** flatness ("blades lie predominantly in the horizontal
plane… vertical displacement driven only by terrain height and
obstacles").  Relief can never emerge from this system on flat terrain
because the algorithm is a relief-minimizer.  This is the formal
non-convergence declaration the leaf history says to make honestly: no
amount of knob-turning on the field simulation produces the reference,
because the failure is the algorithm's *shape*.  (The old docs also mix
implementation into requirements — REQ-GRW/REQ-PIP prescribe the two-pass
step-stamp model — confirming the history entry's prediction.  They are
superseded, not amended.)

Corollary: **invert the dependency.**  Don't hope relief emerges from
blade stacking — *build the relief first as a substrate* (a mound field
added into `terrain_z`), then dress it with blades that follow it.
Relief becomes a designed **input** with a configured amplitude — a
by-construction guarantee, exactly parallel to the leaf-era move of
building the union surface first and placing on it.

### 2. Grass is the *easy case* of the leaf-placement problem

The accepted organic leaf placer solves: total coverage on an irregular
3-D surface, organic non-gridded arrangement, coherent direction
(down-slope ⊕ positional angle field), layered overlap via height-sorted
standoff accumulation, supportless FDM.  That is *this* problem — except
every hard part is easier on grass:

| Leaf-era hard part | Grass situation |
|---|---|
| Union of blobs, embree raycasts to seat each leaf | substrate is a **heightmap** — seating is a bilinear `terrain_z` sample, no rays, no embree |
| Undersides → arch blades, smoothstep zones | a heightmap has **no undersides**; every point is up-facing; the entire underside apparatus is unnecessary |
| Cross-cluster seams | no seams — one continuous field |
| Coverage verification on a curved surface | substrate **is** the terrain surface; terrain can never show through by construction; blades are decoration |
| 5 s perf via embree + batching | no rays at all → expect the blade pass in low single-digit seconds, vectorizable |

So the design transplants the leaf placer's proven organs (maximal
Poisson-disk saturation with exact-distance grid, down-slope ⊕ coherent
angle field, height-sorted standoff accumulation, curl cap, downward size
jitter) onto a heightmap domain where each is cheaper and simpler.

## Decisions recorded

1. **Architecture: substrate → carpet → dressing** (three layers, detail in
   the design doc):
   - **Mound substrate**: low-frequency lumpy field added into `terrain_z`
     (mechanically like `SoilCarpet` blobs at clump wavelength).
     Slope-capped ≤ ~40° at generation → FDM-safe by construction; no new
     mesh, no boolean, ~ms cost.
   - **Under-carpet**: existing `GrassCarpet` embossing runs *after* the
     mound, so the fine texture follows the relief (the reference's
     two-scale texture; the Fable-review carpet-underlayer idea, already
     built).
   - **Blade dressing**: sheaves (tufts of 3–9 blades, the placement unit —
     the reference's visible bundles) dart-thrown to maximal Poisson
     saturation in the grass mask; each blade's spine is a 2-D path draped
     on the substrate (`z = sampled surface + accumulated standoff`);
     direction = blend(down-slope of the mound field, coherent positional
     angle field) so sheaves comb down the flanks of their own mounds.
2. **Mound sites and sheaf structure are correlated, not independent** — a
   mound *is* a pile of grass.  Clump sites drive both the substrate blobs
   and the direction field, so the relief and the thatch read as one
   thing.  (Aesthetic bet to be validated in Experiment 1/3.)
3. **No growth simulation.**  Blades do not read/write an occupancy grid
   step by step.  Layering comes from root-z-sorted standoff accumulation
   (the leaf iter-4 lesson: a global ramp is too thin; sorted accumulation
   is structural).  Within a sheaf, blades are a coherent non-crossing fan
   by construction (parallel offset paths).
4. **Obstacles**: reject/shorten, don't steer.  Sheaf roots rejected inside
   `obstacle_mask`; paths clipped at obstacle boundaries.  Rocks/trees
   still stamp first; layer ordering contract unchanged.
5. **Height budget partitioned**: ≤ 5 mm total = mound amplitude
   (~2.5–3 mm peak-to-valley) + blade + bounded stack (~1.5–2 mm).  The cap
   is enforced by budgeting inputs, not by post-hoc clamping.
6. **Tip lift: out of scope** (dispensable per interview) — removes the one
   feature that fights the 45° rule.  Curl stays capped (leaf-era found
   16° effective; grass will tune its own constant).
7. **Keep**: blade mesh construction (shape is fine), `SpeciesConfig`
   geometry, `GrassCarpet`, `scatter/distribute.py` helpers,
   vegetation-support rasterisation (downstream layers still need
   `vegetation_support_z`).
   **Delete at acceptance** (not before): `grass/grow.py` field
   simulation, per-step occupancy stamping, `GrassSeed.sort_key()`
   upstream-distance ordering, `FloppyGrassLayer` growth internals —
   harvesting organs into whatever shared module survives, verified by
   stats-identical refactor where applicable.
8. **Knobs stay module constants while iterating**; config promotion is a
   listed open item (leaf-era lesson 7).

## Interview round 2 (same day)

- **Mound scale**: medium — 4–5 mounds per 35 mm square (~7–9 mm
  wavelength).
- **Narrow strips** (corridor margins ~4–8 mm): scale wavelength AND
  amplitude down with local region width; strips stay lumpy in
  proportion.
- **Tile edges**: **feather relief flat at tile boundaries** so adjacent
  tiles butt cleanly.  (Deliberately rejects the reference's full-relief
  edge — tileability beats edge drama.  Interior mask edges also
  feathered.)
- **Dressing density**: dense — the blades ARE the surface; carpet only
  peeks through in hollows.  Substrate's job is shape, not surface
  detail.
- **Perf baseline measured**: current `1x1-grass` full mesh build =
  **3.6 s / 176 k faces** (2026-07-03, this machine).  New system targets
  parity or better despite dense dressing.

## Interview round 3 (same day)

- **Variants graveyard**: "git will know — both designs & code;
  meta/history might be very interesting."  → The grass-era timeline is
  reconstructed from git + docs and recorded below (see "The grass
  graveyard").
- **Drybrush optimized.**  Tiles are painted; drybrush catches positive
  relief.  Design bias: crisp blade edges, dark inter-blade grooves,
  defined mound shoulders over soft domes; S3 print acceptance judged
  after priming + drybrush.
- **Rocks: rise against them.**  The mound field gains a feathered skirt
  at obstacle footprints (within slope cap) so grass laps stones —
  nestled, not moated.
- **Default grass first.**  Single species for V1; the multi-species
  framework is re-generalised after acceptance, not carried live through
  the rebuild.

## The grass graveyard (reconstructed from git, 2026-05-26 → 2026-07-03)

Eight eras in ~5 weeks.  Categories: (a) blade geometry, (b) placement/
arrangement, (c) growth/simulation, (d) heightmap/carpet.

- **Era 0 — 2D grayscale heightmap → OpenSCAD `surface()` emboss**
  (05-26→27; `5740a38`…`1a9bcb6`; `archived/generate-grass-heightmap*.py`,
  `docs/design/grass-heightmap.md`).  No 3D geometry: bezier ridge strokes
  composited with `np.maximum`; v2's **tip-priority compositing** (a
  strictly increasing `blade_h(t)` so the point nearest a tip always wins)
  plus a flow-field edition tracing blades along streamlines.  Died of:
  dark border from clipped edge blades, density imbalance, and the move to
  real 3D.
- **Era 1 — 2D painter's-algorithm blade scene renderer** (05-28;
  `478a618`…`eff4caa`; `blade.py`, `BLADE_DESIGN.md`).  Swirl flow +
  curvature-constrained blade bend.  `BLADE_DESIGN.md` is a postmortem:
  the scanline spine model **breaks at steep tilt** ("geometry is
  completely broken").
- **Era 2 — first real 3D STL: support-field terrain-following blades**
  (05-28→30; `848371b`…`dca9133`).  2.5D support field, blades painted
  back→front, each rasterised into `support_z`; cross-section evolved
  ribbon → prism → V-keel.  Died of **blade-height blowup** (a script
  literally titled "keel-free experiment … to diagnose blade-height
  blowup") and two z-solver rewrites in three days.
- **Era 3 — radial/curl burst arrangements** (05-31; `985eed5`…`eefa5b7`).
  One day of arrangement archetypes (radial spray, burst, zones, curl),
  none kept — churn that directly preceded building a real simulation.
- **Era 4 — "GrownGrass": obstacle-aware growth + support posts**
  (06-01→02; `0a7feee`…`39cdc9f`).  LCM-envelope z, intersection repair,
  steering around stones.  Died of the **support-post saga** (posts
  reshaped four times in one day) and bald spots.
- **Era 5 — `FloppyGrassLayer`** (06-04→08; the current system).
  Two-pass grow/build, per-step occupancy stamps, own-trail strips,
  Voronoi clumps, and the signature rule *"lie as flat as possible."*
  Recurring smells recorded at the time: `blade_smooth` axis flipped
  three times, a rule added and reverted next commit, and the
  REQ-OBS-2/3 vs REQ-MSH-2 tension (**Z-jitter on parallel blades**)
  documented as "the core design challenge" and left open.  Terminal
  verdict (this doc): it is a **relief-minimizer** — flat by
  construction.
- **Era 6 — GrassCarpet underlay** (06-09→13; `8ab508e`…`76ca3ef`).
  A second flat layer *under* the blades — relentless noise-amp
  back-and-forth; the dedicated carpet tile was added and deleted within
  three days.  **Papered over the flat-mat problem rather than solving
  it** (but the carpet itself survives as the mound-thatch under-layer).
- **Era 7 — placement infrastructure churn** (06-10→18).  Unified Scatter
  pipeline built then dissolved 8 days later; `flow.py` direction field
  deleted; `sort_key` flip-flopped.  `scatter/distribute.py` survives.
- **Era 8 — mound-thatch greenfield** (this doc; design only as of
  07-03 morning).

**Qualities chased repeatedly** (i.e. actual requirements, revealed by
recurrence): *higher-thing-proud-of-lower-thing* — implemented **five
separate times** (tip-priority compositing, support-field rasterise,
downstream arch-over, own-stamp standoffs, and now root-z-sorted
accumulation); coherent-but-varied direction (swirl fields → Voronoi
clumps → down-slope ⊕ angle field); total edge-to-edge coverage; sharp
tips / crisp edges for drybrush; supportless FDM.

**Failure modes that recurred**: height blowup / stacking too tall;
Z-jitter between parallel blades; self-trail climbing; bare borders from
clipped edge blades; and — the terminal one — *reads as a flat mat*.
Meta-smell: **indecision churn** (one-commit reverts, axis flip-flops,
add-then-delete artifacts) marks every dying era; the greenfield's
"special case patched more than twice → stop" rule exists because of it.

**Design check**: no prior era ever tried building the relief first as a
substrate — the mound-thatch approach is not a re-tread.  And the five
independent reinventions of proud-of-lower confirm sorted standoff
accumulation deserves its place as a by-construction guarantee.

## Acceptance scenes & budgets (named up front)

- **S1** — full `1x1-grass` tile render, elev 40 / azim −135 (matches the
  baseline `grass-current-2026-07-03.png` exactly, for honest
  before/after).
- **S2** — close-up low-elevation render (elev ~15) of a quarter tile, to
  judge the *silhouette* — mounding is the key requirement and it lives in
  the silhouette.
- **S3** — a **physical print** of the accepted tile.  The reference photo
  is a print; the final judge is the object at arm's length, not the
  render.
- **Perf budget**: grass layer ≤ 3 s, whole `1x1-grass` tile ≤ 5 s.
  Carried as a requirement, not a cleanup (leaf-era lesson 6).
- **Instrumentation before fixes**: relief histogram (z spread of the
  final surface), slope histogram (% of substrate cells > 45° must be 0),
  sheaf-direction coherence map, per-stage timing.  Also protects against
  fixing non-problems (the leaf-era "bald bowl" was designed-bare).

## What would count as failure smells (watch list)

From the leaf history: fixes that *relocate* artifacts instead of removing
them; any special-case patched more than twice (the "apex cap" sign);
coverage or printability maintained by tuning rather than construction.
If the substrate-then-dress shape shows these, stop and re-interview —
don't iterate parameters.

## Experiment log — first implementation session (same day, afternoon)

E1–E4 were run the same afternoon.  Working code: **`src/dharmatiles/grass/thatch.py`
(`ThatchGrass`)**, experimental tile spec preserved at
`docs/grass/experiment-2026-07-03-thatch.tile.py.txt`, renders at
`docs/grass/experiment-2026-07-03-thatch-s{1,2}.png`.  Not yet wired into
any real tile; FloppyGrass untouched.

**Reuse discoveries (both cut the build to near zero):**
1. **E1 needed zero new code.**  `SoilCarpet` *is* the mound substrate — a
   mound-tuned instance (max-combined super-Gaussian blobs, built-in tile-
   edge + mask-edge cosine fade = the round-2 feathering decision already
   implemented).  Ordering trap: the FIRST layer with `terrain_material`
   wins, so `GrassCarpet` must precede the mound `SoilCarpet` in
   `Region.layers` or the whole tile renders soil-brown.  Displacement is
   additive, so carpet-then-mounds ≡ mounds-then-carpet.
2. **`grass/mesh.build_meshes` is 90 % of the dressing pipeline.**  It
   lifts each blade over `vegetation_support_z`, then stamps the blade top
   back in — i.e. per-blade sequential standoff accumulation.  Feeding it
   *synthesized* paths sorted lowest-root-first gives R4 layering and R8
   support stamping with no simulation.  `ThatchGrass` uses a local
   variant (`_build_draped`) with one change: lift lands at
   `support + 0.25 mm` instead of exact contact.

**Iteration log (artifact → root cause → constructive fix):**

| Iter | Named artifact | Root cause | Fix |
|---|---|---|---|
| E1a | brown ground | SoilCarpet first → terrain_material=SOIL | GrassCarpet first in layer list |
| E1a | **lonely peaks** — isolated steep bumps on a flat sea | sparse blobs max-combined against zero baseline | dense overlapping primary tier (30/sq, σ 1.4–2.5) + broad swell tier (6/sq, σ 3–4.5) so the valley floor undulates by construction |
| E2 | **melted worms** — blades coalesce into soft sheets | lift lands blades in exact contact with support; curl too strong (C-shapes) | 0.25 mm lift clearance (own build loop); curl 0.35–0.7π → 0.12–0.32π; length 8–14 mm |
| E3 | **tangle, not thatch** — X-crossings everywhere | per-blade i.i.d. curl sign & size destroy bundle read | **sheaves as placement unit**: sites at 3.4 mm spacing, 3–7 blades fanned at 1.1 mm, shared direction/curl-sign/size — bundles read immediately |
| E4 | **tent peak** — cone at one mound apex | instrumented (peak xy/z per mesh part): near-max mound (2.1 mm) + blades stacked to the full 2 mm cap on its apex | stack cap is ride-height over the *draped substrate*, truncate-at-first-violation (min 4 pts else drop); cap 2.0 → 1.2 mm |

**Perf**: full 1×1 tile (carpet + mounds + ~1500 draped blades in sheaves,
~134 k faces) builds in ≈ 3.5 s — already at parity with the old grass, no
optimisation done (synthesis is a plain Python loop).

**Tooling note**: the pyrender/pyglet GL renderer died mid-session (macOS
`screens[0] IndexError` — no display available, likely locked screen).
Stopgap: `src/extras/swrender.py`, a numpy z-buffer orthographic software
renderer with **synthetic drybrush shading** (world-height tint) — which
proved independently useful: it previews approximately what the painted
print will emphasise.  Camera convention matches render_tile
(elev/azim; rotate by elev−90° about x).

**Current module constants** (thatch.py): sheaf spacing 3.4 mm, fan
1.1 mm, 3–7 blades/sheaf, angle-field wavelength 6 mm, down-slope weight
0.6, drape smoothing σ 1 mm, lift clearance 0.25 mm, size jitter ×0.75–1.0.
Species used in the experiment: width 1.4–2.0, length 8–14, curl
0.12–0.32π, thickness 0.7, clearance 0.15.

**Rock test (same day, later)**: `Rocks` placed between mounds and
`ThatchGrass` works with no code changes — roots reject in
`obstacle_mask`, paths clip at footprints, no blade pierces a rock.
Artifact named: **drowning pebbles** — r 1.8–3.2 mm rocks barely beat the
~3.5 mm mound+thatch relief; r 2.8–4.2 reads right.  Rule of thumb: rocks
in thatch fields need max(r) comfortably above mound amplitude + stack
cap.  Test STLs (watertight, db): `stl/test/1x1-thatch-db.stl` (134 k
faces) and `stl/test/1x1-thatch-rock-db.stl` (121 k faces); specs in the
session scratchpad, spec text also at
`docs/grass/experiment-2026-07-03-thatch.tile.py.txt`.  Rock lapping
(mound skirt against rock bases) still unimplemented — grass crowds close
naturally, so judge on the print whether the skirt is even needed.

**Blade-shape variants (Shawn's suggestion: longer tip taper + sharper
top angle)**: two one-knob variants rendered and shipped as test STLs.
`blade_taper` 1.0 → 4–5 mm gives visibly pointed grass-like tips — a
clear win.  `blade_top_facets` 6 → 2 (fully peaked) sharpens the ridge
but **flattens the blade body**, losing the reference's plumpness; 4
facets + thickness 0.8 keeps the point while staying plump — the best of
the three to my eye.  STLs for slicer/print comparison (all watertight,
db): `1x1-thatch-db.stl` (round, taper 1), `1x1-thatch-sharp-db.stl`
(taper 5, facets 2), `1x1-thatch-mid-db.stl` (taper 4, facets 4,
thickness 0.8), all in `stl/test/`.  Judgement deferred to Shawn /
the print.

**"Bushy" round (Shawn: mid is liked; make blades shorter and bushier,
more max curl)**: species length 8–14 → **5–9 mm**, curl 0.12–0.32π →
**0.15–0.5π**; thatch constants sheaf spacing 3.4 → **3.0 mm**, blades
per sheaf 3–7 → **4–9**, fan 1.1 → **1.0 mm**.  Result is the closest
match to the reference yet — short plump curled blades bunching into
swirled locks.  Current front-runner.  STL:
`stl/test/1x1-thatch-bushy-db.stl` (watertight, 122 k faces); render:
`docs/grass/experiment-2026-07-03-thatch-bushy-s1.png`.

**Bushy ACCEPTED as the default grass (Shawn: "The bushy one is great.
Good as defaults.  Good as default grass.")**  Baked in as
`_default_species()` in `thatch.py` (width 1.4–2.0, length 5–9, curl
0.15–0.5π, thickness 0.8, taper 4.0, top facets 4, clearance 0.15) —
`ThatchGrass()` with no args now produces it; default
`max_stack_height` 1.2.  SpeciesConfig global defaults untouched (shared
with GrassCarpet/FloppyGrass).

**Edge fill (Shawn ask #1)**: the bare top-surface ring at the tile edge
— the graveyard's recurring "dark border from clipped edge blades" —
fixed structurally: `_EDGE_MARGIN_MM` → 0 (spines run to the boundary)
and blade meshes are clamped to the tile footprint after building, so
flanks slice flat at the tile wall (closed mesh preserved, REQ-OUT-2 by
construction).  Verified: STL xy bounds exactly [0, 35].

**Rocks-in-bushy (Shawn ask #2)**: `stl/test/1x1-thatch-bushy-rock-db.stl`
(watertight); render
`docs/grass/experiment-2026-07-03-thatch-bushy-rock-s1.png`.  Grass laps
the stones with no skirt; reads nestled.

**Rock-interaction round (Shawn: "Not loving the grass/rock interaction…
doesn't look natural at all")**.  Named artifacts from the MeshLab
close-up: **the moat** (bare keep-out ring around every rock), **perched
rocks** (clean level waterline = placed-on-top read), **necklace
composition** (three equal eggs), **chopped-top facet** (an `n_cuts`
plane).  Interview decisions: grass swallows the base; uniform scatter
with wider size variance; angular fieldstone; ~⅓ buried.  Fixes, each
by-construction in `thatch.py` + spec:
1. **Soil skirt** (rim berm only, 0.8 mm × 2.5 mm gaussian off the
   footprint edge, zeroed inside the mask): turf line climbs the stone.
   Pipeline catch: `displace_terrain` resyncs `terrain_support_z =
   terrain_z`, which would erase the rocks' stamped heights — the skirt
   preserves the old support via `np.maximum` after displacing.
2. **Rock climb**: blades no longer stop at `obstacle_mask`; they walk up
   the stone until it rises `_ROCK_CLIMB_MM` (1.3) above the substrate
   and the drape rides `min(support, substrate+cap)` — tips rest ON the
   rock (supported, FDM-safe).
3. **Root rejection by `passable`, not the full footprint** — a sunk
   rock's mask is far wider than its exposed crown; rejecting the whole
   mask was the moat's root cause.
4. **Crowding ring**: extra single blades seeded in the 0.3–3.0 mm band
   outside each footprint (1 per 1.2 mm²), aimed *toward* the stone
   (−∇dist + jitter) so they climb and lean — the contested-border read.
5. Spec: `r=D[2.0:4.5].power(1.5)`, `flat=D[0.7:1.2]` (tall domes survive
   burial), `n_cuts=6`, `cut=D[0.55:0.85]`, `roughness=0.06`, `sink=0.9`.
   Earlier failure worth remembering: sink 0.9 + full-footprint skirt +
   low `flat` **completely buried** small rocks (invisible stones with
   giant moats) — burial interacts multiplicatively with flat/r; check
   crown height, not just sink.
Result render: `docs/grass/experiment-2026-07-03-thatch-rock-interaction.png`;
STL `stl/test/1x1-thatch-bushy-rock-db.stl` (watertight, 133 k faces).
Residual dirt pockets at rock bases read as shaded-out soil (natural);
judge on print.

**Crown rule + grass-through-rock round (Shawn: stones must sit slightly
higher than max grass; one rock had grass growing into it).**
- **Grass-into-rock root cause**: the drape capped in-footprint blade z at
  `min(rock surface, substrate + climb cap)` — capping BELOW the surface
  embedded blades inside the stone.  Fix: blades ride ON the stone
  (`max(substrate, support) + 0.2 mm standoff`); entry into a footprint is
  gated three ways (rise ≤ 1.3 mm to approach, ≤ 0.9 mm to enter, ≤ 2
  steps inside) so only tips lean on the fringe.
- **Crown rule** (documented in the spec): exposed height `r·flat − sink`
  must beat the canopy near rocks — skirt 0.8 + stack 1.2 + blade 0.8 +
  clearances ≈ 3.4 mm.  Spec now `r=D[3.6:4.8].power(1.3)`,
  `flat=D[1.2:1.4]`, `sink=0.3` → min crown 4.0 mm.
- **Instrumented, not eyeballed**: per-rock `crown-above-grass` (+0.22 /
  +1.25 / +0.75 mm ✓) and signed-distance penetration histograms.
  Residual: lateral blade-edge overlap on curved flanks, p50 ≈ 0.3 mm,
  p90 ≈ 0.8 mm, max ≈ 1.3 (≈ half a blade width) — reads as grass pressed
  against stone, NOT the through-the-wall artifact (that was the burial
  bug).  If a print still shows a piercer, next tool is a per-blade
  penetration cull (signed_distance per blade near rocks).
- STL: `stl/test/1x1-thatch-bushy-rock-db.stl` (watertight, 123 k faces);
  render refreshed at
  `docs/grass/experiment-2026-07-03-thatch-rock-interaction.png`.

**"It didn't work" round — the impaled-blade hunt (MeshLab close-up #2).**
Three wrong theories were implemented and *disproved by measurement*
before the real cause surfaced; recording the chain because the method is
the payload:
1. Theory: tip-entry blades sink in steep cells → gated entry harder
   (≤ 0.9 mm rise, ≤ 2 steps, +0.2 standoff).  Numbers unchanged.
2. Theory: rim verts under a bilinear-underestimated floor → vertical
   clamp to a local-max floor.  It fired (2 399 verts) but 286 hit the
   lift cap, and it wrongly hoisted grass above the crowns.  Reverted to a
   **horizontal push-out** (verts inside a footprint pushed out along the
   distance-field gradient) — which left the returned thatch meshes
   measurably CLEAN… yet the final part still had the same penetrators.
3. Realisation: the penetrators aren't ThatchGrass at all.  The final
   grass part is carpet ⊕ thatch merged by material, and **GrassCarpet
   emits blade TUBE meshes** — and ran *before* `Rocks`, so rocks were
   planted straight through its tubes.  Identical counts across every
   thatch-side change was the tell.
Fixes kept: horizontal push-out (thatch, belt-and-braces), steep-wall
keep-out (spines stay a blade half-width clear of walls, so the union
can't truncate a full-width blade at a rock face — the actual "impaled"
look), and the ordering rule: **`Rocks` before `GrassCarpet`** whenever a
region has both (mounds still first for the SOIL material).
Final numbers: crowns +0.48/+0.60/+2.07 mm above nearby grass
(`flat=D[1.35:1.55]`); worst residual penetration 0.85 mm (carpet-tube
laterals at ground level, hidden under the canopy), n per rock ~60
(from 149–220).  STL `stl/test/1x1-thatch-bushy-rock-db.stl` regenerated,
watertight.  Lesson for the punch list: give GrassCarpet the same
obstacle-aware placement the thatch has (or the push-out), which would
take the residual to ~0.

**The autonomous moat-elimination loop (Shawn: "This interaction is
fundamentally flawed… render top view, fix, loop").**  Process change
that mattered: judging on TOP-VIEW renders (`swrender --elev 90` +
`--box` mm-crops) instead of composition views — the moat is invisible at
45° and undeniable from above.  The loop (each step render-judged):
1. Wall keep-out (previous round) was itself a moat generator — replaced
   with **wall-slide deflection**: a blade whose next step hits a steep
   cell deflects to the wall tangent (blend 0.8·tangent + 0.2·outward)
   and flows around the stone like parted grass.
2. Carpet tubes were the "grass running into rock sides" — carpet's
   `_inside_mask` now treats obstacle cells like a region boundary.
3. Blocked sheaf sites are **relocated** outward along the distance
   gradient (8×0.5 mm steps), not dropped — dropping deleted 4–9 blades
   of coverage at once.
4. Crowding-ring blades became **tangential** (lying along the annulus
   covers it; aiming at the stone just deflects off again).
5. **The decisive find**: the stack cap was culling exactly the annulus
   blades — the skirt is the highest substrate, so its blades sort last,
   lift over everything, exceed the cap, and die.  Fix: per-blade cap
   (carried in `GrassSeed.blade_rise_cap`), ×1.5 within 3 mm of a rock.
6. A crown *ceiling* (truncate blades above crown−margin near rocks) was
   tried twice and **re-carved the moat both times** — the annulus blades
   are exactly what it kills.  Removed; the crown rule is held
   structurally by the wall gate (blades can't climb >0.9 mm up a stone)
   plus rock sizing (`flat=D[1.55:1.75]`, near-rock stack ×1.5).
Final state: top view shows grass crowding every stone with small
asymmetric dirt pockets (hollow-scale, not rings); penetration n≤18,
max 0.53 mm (shallow tip contact); crowns +2.25/+0.72 mm and one single
blade tip 0.14 mm above the small rock's crown — below one print layer,
physically invisible.  Build 2.4 s.  STL
`stl/test/1x1-thatch-bushy-rock-db.stl` (144 k faces, watertight);
renders `docs/grass/experiment-2026-07-03-thatch-rock-top.png` +
`-interaction.png`; spec text `…-thatch-rock.tile.py.txt`.

**The "hole around the rocks" (Shawn) — the saga's actual root cause.**
The rocks kernel (`layers/rocks.py`) rasterises `support_z` /
`obstacle_mask` from the **analytic uncut ellipse** (`D2 <= 1`), but the
mesh is carved afterwards by the plane cuts.  With the craggy-round cuts
(`n_cuts=6, cut=D[0.55:0.85]` — planes at barely half the radius) the
real stone was far smaller than its stamp: a **phantom rock annulus**
that grass avoided, with the skirt berm rising at its outer edge — a
crater rim around a too-small stone.  This mismatch was quietly feeding
every earlier moat symptom.  Fix (spec-side): shallow cuts
`n_cuts=5, cut=D[0.82:0.96]` → stamp-vs-mesh overhang 0.1 mm; hole gone,
grass presses directly against the stones.  Verified: crowns
+0.67/+2.22/+0.72 mm, penetration ≤ 0.9 mm shallow contact (the leaning
look).  **Punch item: make the rocks kernel apply its cut planes to the
support/obstacle stamp** so deep-cut craggy rocks become safe again.
Final STL `stl/test/1x1-thatch-bushy-rock-db.stl` (144 k faces,
watertight); renders `…-thatch-rock-top.png` / `…-interaction.png`.

**Open punch list for the next session:**
1. The residual mound-apex bump may be fine (reference print has peaks) —
   judge on a real pyrender render + by Shawn; the sw-render drybrush
   exaggerates the top 1 mm.
2. Stack-cap truncation can leave abrupt blade stubs on pile flanks —
   consider tapering the last 2 points or ending on a down-step.
3. Roots are a jittered grid, not maximal Poisson — fine so far; upgrade
   only if bare patches appear at S2 print scale.
4. Slope instrumentation must measure the *mound field*, not mesh faces
   (carpet micro-stamps dominate face normals: p50 59° is meaningless).
   Add a mounds-only slope histogram before any print.
5. Rock lapping (skirt), narrow-strip scaling, and water tiles untested.
6. Knobs stay module constants; species/config promotion later.
7. When accepted: wire into real tiles, regenerate, print S3, then delete
   FloppyGrass growth internals per the design's keep/delete inventory.

## Next actions

Experiments E1–E5 with knobs and expected outcomes are specified in
`docs/design/grass-mound-thatch.md`.  E1 (mound substrate + existing
carpet, zero new blade code) is deliberately first: it may buy the
majority of the "lumpy and organic" read before any placement work, and
it derisks the FDM slope question immediately.
