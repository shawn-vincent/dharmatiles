# grasslab — Sculpted Grass Tile: Requirements

*Revision 3, 2026-10-27 (matches code v2f).  Supersedes revisions 1–2; their
retired requirements and the reasons are kept in §7.*

*Visual targets: `reference/approved-reference.png` ("Sculpted Grassy
Terrain Tile Showcase", from the procedural sculpted-grass spec Shawn
supplied); Shawn's wind-lodged grass photo (long combed ribbons meandering
in S-curves, parting around eye-shaped bare patches); the Majestic
Highlands grass texture (coherent, not rigid).*

---

## 1. What this is

A standalone experiment, kept apart from the dharmatiles tile pipeline, that
writes **one STL: a standard DungeonBlocks tile of gently hilly ground
covered in sculpted grass, with a few bare-earth clearings**.

> **Build the illusion, not botany.**  At tabletop distance grass reads
> because of its **mid-scale organisation**: combed ribbons that flow,
> meander and part around obstacles, made of blades with lit crests and
> dark undercut tips.  Fine detail only decorates that structure.

### 1.1 Visual scales

| Scale | Size | What it does |
|---|---|---|
| **Flow / ribbon** | strands 15–30 mm long, 2–3 mm apart; meanders ≈ 15 mm wavelength | Combed ribbons of blades following one smooth direction field; the "flowing grass" read. |
| **Swath** | cells ≈ 11 mm apart | A height wave only: low upstream and at the sides, cresting downstream, so ribbons rise and lap over each other. |
| **Frond** | 4.4–10.5 mm long, 1.1–1.8 mm wide | Pointed, domed blades shingled along each strand; their tips are the shadow-casting lips. |
| **Groove** | ≥ 0.6 mm wide, 0.4–1.0 mm deep | Dark channels between fronds and under tips, where the base coat survives a drybrush. |
| *Leaf point* | < 1 mm | **Paint and lighting, not geometry.** |

### 1.2 Geometry is true 3D, undercuts only where designed

Undercuts exist only under frond tips (lips) and under the soil-facing
edges of bank fronds (side ledges), each with a 45° underside.  Everywhere
else the grass is a solid height field down to the ground.

### 1.3 Scope

In scope: a standard DB tile (35 mm squares, the existing socket-peg base,
the official grass height band); independent hills; the grass; bare-earth
clearings; printability by construction plus an audit.  Out of scope:
OpenLOCK, colour, seamless tiling, `.tile.py` integration (rocks live in
`rocklab/`, the combination in `meadowlab/`).  The project lives in
`grasslab/`, imports dharmatiles primitives freely, never modifies
`src/dharmatiles/`, and writes to `grasslab/out/`.

---

## 2. Requirements

Each requirement is checkable from a viewer or render (**[R]**) or from a
script or statistic (**[S]**).

### 2.1 Output and mesh

- **REQ-OUT-1 [S]** A run writes one binary STL,
  `grasslab/out/<cols>x<rows>-sculpted-grass[-flat]-s<seed>.stl`.
- **REQ-OUT-2 [S]** The mesh is one closed manifold body: watertight,
  consistent normals, no self-intersections, no internal voids.  Crumbs
  (disconnected slivers) are dropped and reported with vertex and face
  counts.
- **REQ-OUT-3 [S]** Standard DB socket-peg base
  (`dharmatiles.bases.dungeonblocks`).  The *mean* grass-top height lies in
  the official DB grass band, **15.3–16.8 mm above the piece bottom**
  (4.4–5.9 mm above the terrain datum; commit `6eb0a55`).
- **REQ-OUT-4 [S]** Footprint cols × rows DB squares, default 2 × 2.
- **REQ-OUT-5 [S]** Deterministic: the same seed and parameters give a
  byte-identical STL.
- **REQ-OUT-6 [S]** The STL is under 50 MB at 2 × 2, and the build takes
  under 60 s.

### 2.2 FDM rules (hard constraints; 0.4 mm nozzle, base-down print)

- **REQ-FDM-1 [S]** No downward-facing surface is steeper than **45°**
  from vertical, beyond micro-lips of 0.5 mm or less.
- **REQ-FDM-2 [S]** No islands: every layer is supported by the layer
  below within 45°.
- **REQ-FDM-3 [S] Gaps are wedges, never slots.**  An undercut opens from
  its root at 45° or wider.  Parallel-walled air gaps thinner than 0.6 mm
  are forbidden (slicers fuse them; paint can't reach them).
- **REQ-FDM-4 [S]** Free features (lips, ledges) are ≥ 0.9 mm in XY and
  ≥ 0.6 mm thick in z.  No needles.
- **REQ-FDM-5 [S]** Every feature is fused to the mass: no hinges, no
  pinches.
- **REQ-FDM-6 [S]** Tile edges are solid; how grass *ends* at the rim is
  REQ-EDGE-3.
- **REQ-FDM-7 [S]** Grass stays ≤ 3.5 mm above the local ground, budgeted
  on the inputs, never clamped afterwards.
- **REQ-FDM-8 [S]** An automated audit (overhang area, crumbs, backstop
  changes) runs on every output and reports any violation.

### 2.3 Hills

- **REQ-HILL-1** The ground H₀ is generated independently of the grass and
  reads as rolling ground with the grass removed.
- **REQ-HILL-2 [R]** Broad and irregular: landforms 15–40 mm across, no
  grid, no repeated bumps, no high-frequency noise.
- **REQ-HILL-3 [S] Rises only.**  *(Shawn.)*  The perimeter level is the
  lowest ground on the tile: hills rise from flat plains at edge height,
  about 40% of the interior stays plain, nothing dips below the edge.
- **REQ-HILL-4 [S]** At least 60% of the area has slope ≤ 12°; no slope
  exceeds 34°.
- **REQ-HILL-5 [S]** The ground is feathered to a constant edge height.
- **REQ-HILL-6** The grass seats on H₀ and follows its curvature.

### 2.4 Flow (the defining feature)

- **REQ-FL-1 [R] Fronds follow the flow.**  Every frond's spine is a
  streamline of one direction field, with no per-frond bend.  Neighbours
  stay parallel along their length; curvature belongs to the flow.
- **REQ-FL-2 [R] Meandering, varied flow.**  Gentle S-curves (wavelength
  about 15 mm, ±10–30°) that are **coherent across the flow** (neighbouring
  streamlines wave in phase, like combed hair), with phase and amplitude
  drifting so waves are never regular.  Several flow regions, not one
  sweep: a 2 × 2 tile shows at least three dominant directions.
- **REQ-FL-3 [R] Ribbons.**  Fronds gather into strands about 2–3 mm
  apart, each 15–30 mm long, made of fronds shingled along one streamline.
  Dark seams run between strands.  Strands cross only at shallow angles; a
  few strays (about 5%) cross more steeply.
- **REQ-FL-4 [R] Flow wraps around bare soil.**  The field bends
  tangentially around each clearing: grass parts at the upstream end and
  rejoins downstream, like water around a stone.
- **REQ-FL-5 [R] No crowns or vortices.**  The field is singularity-free
  except the deliberate partings and rejoins at clearings: no starbursts
  or spirals at any scale.
- **REQ-FL-6 [R] Banks read as piles of blades.**  *(Shawn: "a big pile of
  grass leans up right against [a clearing]… undercuts at the bottom of
  the grass blade so there's clear ridges down the side.")*
  - Where grass faces a clearing, each blade's soil-facing edge overhangs
    the body below as a ledge with a **45° chamfered undercut**, so a stack
    of blades shows ridge lines with dark shadows down the bank face.  A
    vertical bank is fine; a smooth featureless one is not (F25).
  - Bank grass varies: fronds wander in heading, height and spill, some
    cross at an angle, and how strongly the flow hugs each clearing
    varies, so the bank line meanders.
  - Every ledge obeys REQ-FDM-1…4 by construction.

### 2.5 Swath height wave

- **REQ-SW-1 [S] Total coverage.**  Every point belongs to exactly one
  swath cell, so the height wave has no gaps.
- **REQ-SW-2 [R] Wave profile.**  A swath is low upstream and at its
  sides and crests downstream; lateral neighbours meet low, so there are
  **no cliffs or terraces between side-by-side ribbons**.
- **REQ-SW-3 [R] Shingled leading edge.**  Fronds crossing from a high
  swath end into the next swath's low start stay high, casting the
  tile's dominant shadow lines.
- **REQ-SW-4 [R]** Swath borders are never straight lines, polygons or
  repeated shapes, and never visible as steps (REQ-EDGE-4).

### 2.6 Fronds

- **REQ-FR-1 [R]** Fronds are elongated along the flow, 4.4–10.5 mm,
  longer toward each swath's crest.  Planform is widest about one third
  along and tapers to a blunt point (tip half-width ≥ 0.45 mm).
- **REQ-FR-2 [R] Shingling.**  Each tip lies over the next frond's base
  downstream; overlap always reads in that direction.
- **REQ-FR-3 [R] Domed bodies.**  Rounded-dome cross-section that catches
  a drybrush; no fins, ropes or tubes.  A frond emerges flush at its base
  and arches down toward its tip; no frond is a pod sitting on top.
- **REQ-FR-4 [R] Lifted tips.**  The last part of each frond (≤ 2 mm) is a
  lip with a 45° chamfered underside, casting a small shadow downstream
  and fusing where the surface below is high.
- **REQ-FR-5 [R] Variation without repetition.**  Length, width, height
  and small heading offsets vary; no two neighbours are identical copies
  (F7).

### 2.7 Edges: grass ends as grass

*From Shawn's review of v1e: "the grass is just being uniformly chopped
down like it was all made of clay."*

- **REQ-EDGE-1 The grass volume is made of fronds.**  No grass height
  exists except as part of some frond's body: no free-standing envelope
  or mass layer, no mask or scale applied to the grass as a whole.
- **REQ-EDGE-2 [R] Soil banks** are the outlines of the last fronds (tips
  and sides at varied heights, dark gaps between, lapping slightly onto
  the soil), never carved walls or a hole in a volume (F19).
- **REQ-EDGE-3 [R] Tile rim.**  Fronds are shortened (never sliced) so
  tips stop just inside the wall; fronds rooted at the rim keep their
  rounded bases; no bevel or height ramp toward the edge.
- **REQ-EDGE-4 [R] Swath borders** inside the grass are overlapping frond
  lobes, never steps.
- **REQ-EDGE-5 [R]** Fronds cut short are also lower, so a fringe steps
  down instead of ending in tall stubs.
- **REQ-EDGE-6 [R]** Fronds stopped by soil end softly (no "sawn log"
  faces at a cut tip, no cliff at a base facing soil).  This applies to
  bank fronds only: rounding every frond's ends made fish scales (F7).
- **REQ-EDGE-7 [R] Grass flows to the rim, not away from it.**  Near the
  edge, the flow points *at* or runs *alongside* the wall, never away
  (F21).  The steering fades in over 9–15 mm and blends into the interior
  with no seam; flow already heading outward leans further out near the
  rim; long stretches of rim turn the same way, so the flow does not part
  along the wall; at the very rim nothing heads inward past parallel.
- **REQ-EDGE-8 [R] Irregular rim endings.**  Tip insets and steering
  strength vary along the rim; never a ruled line of tips (F23).

### 2.8 Bare soil

- **REQ-SOIL-1 [R] Lens clearings.**  Bare patches are **eye-shaped
  openings elongated along the local flow**, with slightly irregular
  edges, preferring low ground; never puddles or amoebas (F16).
- **REQ-SOIL-2 [S] A few.**  About 1.3–1.8 clearings per square, at least
  13 mm apart and 7 mm from the rim, covering about 10% of the tile.
  Grass stays dominant.
- **REQ-SOIL-3 [R] Banks, not ramps.**  The soil mask decides only *where
  fronds may root*; it never cuts the grass.  Each frond laps 0.8–3.2 mm
  into the soil (skewed short); about 4% are wisps reaching 4 mm.
- **REQ-SOIL-4 [R] Subordinate texture.**  The soil sits about 0.4 mm
  below the surrounding ground with no outlining step, low roughness
  (about 0.08 mm), and **sparse, angular pebbles** (0.45–1.2 mm radius;
  low domes cut by 7–10 facets; no flat tops, prism walls, bubbles or
  clusters; F17, F20).  A few grass sprigs grow in the soil.  All of it is
  height field, printable by construction.

### 2.9 Grooves and surface quality

- **REQ-GR-1 [R]** Dark grooves between adjacent fronds along the flow
  and under every lip, ≥ 0.6 mm wide at half depth, 0.4–1.0 mm deep.
- **REQ-GR-2 [R]** No voxel terracing or stair-stepping on any surface.
- **REQ-GR-3 [R]** Groove edges are crisp, not blurred into soft hills
  (F10).

### 2.10 Parameters and diagnostics

- **REQ-PAR-1** Knobs are named, bounded module constants grouped by
  stage, with a seed.
- **REQ-DIAG-1 [S]** Each run prints mesh, hills, soil, grass-height, DB
  band, FDM audit and timing statistics.
- **REQ-DIAG-2** Before any STL is handed over, Claude checks it with
  quick monochrome renders (`--check`), as an internal filter only.
  Shawn's eye in a viewer is the judge.

---

## 3. Failure modes (named anti-goals)

| # | Failure | Looks like | Seen in |
|---|---|---|---|
| F1 | **Hair** | long, parallel, even strands | spec iteration 2 |
| F2 | **Moss** | short rounded lumps, no direction | spec iteration 3 |
| F3 | **Tufts / crowns** | rosettes, radial clusters | Era 3 |
| F4 | **Shag carpet** | uniform everything | Era 6 |
| F5 | **Wood grain** | long grooves spanning the tile | — |
| F6 | **Worms / rope** | uniform tubes, random headings | 2026-07-03 baseline |
| F7 | **Stamped repetition / fish scales** | identical fronds or regular lobes (feathery itself is fine) | ThatchGrass bushy; v1h global soft ends |
| F8 | **Fingerprints / swirls** | loops, spirals | Era 1 |
| F9 | **Crumpled foil** | relief everywhere, no mid-scale structure | grasslab v0 |
| F10 | **Flat or melted** | no silhouette; grooves blurred out | Era 5; ThatchGrass E2 |
| F11 | **Fragile literal grass** | needles, floating tips | — |
| F12 | **Tent peaks** | stacks spiking above neighbours | Era 2; ThatchGrass E4 |
| F13 | **Z-jitter** | neighbours at random heights | Era 5; grasslab v0 |
| F14 | **Terracing** | voxel stair-steps, side-by-side swath cliffs | grasslab v0 |
| F15 | **Ragged rim** | stubs along the tile edge | grasslab v0 |
| F16 | **Puddles** | amoeba bare blobs with a trench rim, unrelated to the grass | grasslab v1c/d |
| F17 | **Bubbles** | round, clustered dome pebbles | grasslab v1c/d |
| F18 | **Cracks** | hairline soil slits along seams | grasslab v1e |
| F19 | **Clay cut** | grass ending as a carved wall, bevel or hole | grasslab v1e |
| F20 | **Soil debris** | boxy pebbles, prism chips, a dashed step outlining each patch | grasslab v1g |
| F21 | **Wall sprouts** | base at the tile edge, tip pointing inward | grasslab v1h |
| F22 | **Rigid sticks** | straight fronds, no coherent arcing | grasslab v1i |
| F23 | **Ruled rim** | tips lined up at a constant inset | grasslab v1i |
| F24 | **Commas** | short independently bent fronds, no continuous flow lines | grasslab v1k |
| F25 | **Smooth bank wall** | a clearing edged by one featureless vertical face | grasslab v2d |

**Pre-paint test:** the bare grey print must read as grass.

---

## 4. Acceptance scenes

| Scene | Content | Passes when |
|---|---|---|
| **S0: hills only** | 2 × 2, grass off | Reads as natural ground; REQ-HILL-3/4 hold. |
| **S1: flat study** | 1 × 1, flat ground (`--flat`) | Ribbons, fronds, lips and grooves read clearly.  Judged first, before hill or full-tile tuning. |
| **S2: approved tile** | 2 × 2 with hills, seed 1 | All of §2 holds, no F-mode is visible. |
| **S3: seed sweep** | S2 at seeds 1–6 | At least 5 of 6 pass; none looks stamped. |
| **S4: print** | S2 printed, primed, drybrushed | Tip and bank shadows survive, grooves stay dark, nothing breaks. |

---

## 5. Open questions

1. Should fronds carry a midrib or sub-grooves for extra texture, or does
   the frond-to-frond groove suffice?  Judge on S1.
2. Final dimensions come from the S4 print.

---

## 6. Measured on the current code (seed 1 / seed 2, 2 × 2)

| Check | Seed 1 | Seed 2 |
|---|---|---|
| Ground (REQ-HILL) | 3.40–6.58 mm, max slope 34.0°, 78% ≤ 12° | 3.40–6.56 mm, 34.0°, 90% |
| Soil (REQ-SOIL-2) | 6 lenses, 9.6%, 26 pebbles | 6 lenses, 10.0%, 30 pebbles |
| Grass above ground (REQ-FDM-7) | mean 1.38, p95 2.45, max 3.23 mm | 1.43 / 2.37 / 3.44 mm |
| Mean grass top (REQ-OUT-3) | 5.34 mm (OK) | 5.19 mm (OK) |
| FDM backstop / overhang | +1,086 / −807 voxels; 5.8 mm² (0.07%) | +931 / −1,169; 8.1 mm² (0.10%) |
| Mesh, size, time | 247k verts, 24.7 MB, ≈ 15 s | 254k verts, 25.4 MB, ≈ 15 s |

---

## 7. Lessons from previous attempts

### 7.1 dharmatiles grass, Eras 0–8

Recorded in `docs/meta/history/2026-07-03-grass-greenfield-requirements.md`
and `docs/design/grass-mound-thatch.md`.  Recurring needs: proud-of-lower
layering (reinvented five times), coherent but varied direction, designed
(not emergent) depth.  Recurring failures: height blowup, Z-jitter, bare
borders, a flat mat, indecision churn.  Here each need is a
by-construction guarantee: shingling (REQ-FR-2), a singularity-free field
(REQ-FL-5), total coverage (REQ-SW-1), the height budget on the inputs
(REQ-FDM-7).

### 7.2 grasslab v0: why it failed (crumpled foil, F9)

v0 built ~1,800 small lens blades in random order inside a binary voxel
volume.  It was printable and watertight, and read as crumpled foil.

- **Built bottom-up**: no mid-scale structure was required, so none
  appeared.  → Specify top-down, by visual scale.
- **Undercuts appeared incidentally** on every blade side, and the 45°
  filter chamfered them into grit.  → Design undercuts; only at lips.
- **Random stacking order** gave Z-jitter; **binary voxels plus blur** gave
  terracing; **clipped blades** left a ragged rim.
- **Process**: the 2 × 2 tile was built before S1 was looked at, and the
  STL was handed over unseen.  → S1 first; self-check renders always.

### 7.3 Retired requirements and rejected approaches (keep the lessons)

| Retired / rejected | Why | Replaced by |
|---|---|---|
| Swaths as the defining feature, fronds fanned ±12° off each swath axis (rev 2) | Read as clumps once compared with lodged grass; the flow, not the cell, should carry direction | Streamline fronds in strands (REQ-FL-1/3); swaths kept as a height wave only |
| Coherent **sickle** curvature field (v1j–v1k, REQ-FR-7) | Each frond bent on its own: "clumps of commas" (F24) | Curvature belongs to the flow's in-phase meanders (REQ-FL-2) |
| Per-frond rim steering (v1i–v1j) | Per-point side flips made a starburst (+1 source) | Steering applied to the field as a smoothed scalar rotation (REQ-EDGE-7) |
| Free-standing swath envelope with rim height ease and soil cut (v1–v1e) | Grass "chopped like clay": rim bevel, holes, border steps (F19) | Grass made of fronds; boundaries are frond outlines (REQ-EDGE-1…5) |
| Soft ends on **every** frond (v1h-a/b) | Whole tile turned into fish scales (F7) | Soft ends for soil-stopped bank fronds only (REQ-EDGE-6) |
| Noise-warped ellipse puddles with ramp and trench rim (v1c/d) | "Terrible": puddles, bubbles (F16/F17) | Seam lanes (v1e), then lens clearings (v2a) |
| Soil along **swath seams** (v1e–v1k) | Independent of the flow; the photo shows grass parting around clearings | Flow-aligned lenses the flow wraps (REQ-SOIL-1, REQ-FL-4) |
| Prism pebbles (flat tops, 45–60° walls) and a hard dip step (v1g) | Boxy debris and a dashed outline (F20) | Faceted low domes; blurred dip mask |
| Hard sign(b) side choice for lens partings | Long herringbone seam upstream of every lens | tanh side push |
| Hills with a soft floor that could dip below the edge | Shawn: hills should rise from a consistent perimeter | Rises only (REQ-HILL-3) |
| A single sweeping flow (θ σ 25 mm, ±35°) | "Not just one big sweep" | σ 15 mm, ±60° plus meanders |
