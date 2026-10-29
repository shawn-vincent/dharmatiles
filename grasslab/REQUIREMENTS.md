# grasslab — Sculpted Grass Tile: Requirements

*Revision 4, 2026-10-29 (matches code v3: stroke blades).  Supersedes
revisions 1–3; their retired requirements and the reasons are kept in §7.*

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
| **Blade** | 2.8–7.2 mm long, 0.9–1.8 mm wide, 1.5–2.0 mm tall | Separate pointed blades standing on the dirt, shingled along each strand; each tip overhangs open air. |
| **Gap** | the dirt between blades, the air under each tip | Dark ground and shadows, where the base coat survives a drybrush. |
| *Leaf point* | < 1 mm | **Paint and lighting, not geometry.** |

### 1.2 Geometry is defined in the printer's units

*(Shawn, 2026-10-29: "make it more honest about the shape, so that we can
refine THAT rather than a theoretical ideal.")*  A blade is a stack of
0.2 mm layers, each a stroke a whole number of 0.45 mm lines wide; what is
modelled is what an FDM slicer draws.  For resin the same shape is
smoothed (the layer steps melted); its outline, keel and points are kept.
Undercuts are part of every blade: its upper layers reach further toward
the tip than the keel below.

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
  consistent normals, no self-intersections, no internal voids (voids
  sealed under overlapping blades are filled).  Crumbs are dropped and
  reported.
- **REQ-OUT-3 [S]** Standard DB socket-peg base
  (`dharmatiles.bases.dungeonblocks`).  The *mean* grass-top height lies in
  the official DB grass band, **15.3–16.8 mm above the piece bottom**
  (4.4–5.9 mm above the terrain datum; commit `6eb0a55`).
- **REQ-OUT-4 [S]** Footprint cols × rows DB squares, default 2 × 2.
- **REQ-OUT-5 [S]** Deterministic: the same seed and parameters give a
  byte-identical STL.
- **REQ-OUT-6 [S]** Resin resolution: blade surfaces within ≈ 20 µm of
  their smooth definition.  The 2 × 2 STL stays under 100 MB and the build
  under 2 minutes.

### 2.2 Blade construction (print-native, then smoothed)

- **REQ-B-1 [S] Whole layers and lines.**  Before smoothing, every blade
  is a stack of `LAYER_MM` (0.2) layers, each a stroke whose width is a
  whole number of `LINE_W_MM` (0.45) lines; nothing narrower than one
  line.  Opening each layer by just under half a line keeps ≥ 99% of it.
- **REQ-B-2 [R] Cross-section.**  A keel at the bottom (≥ 2 lines), widest
  in the middle, a single-line ridge on top.  Width grows ≤ 1 line per
  layer going up (≈ 45°).
- **REQ-B-3 [R] Profile.**  A convex arch: the blade rises from the ground
  at its root and levels out toward the tip; no straight ramp, no pad.
- **REQ-B-4 [R] Raked bow.**  Only the top layer reaches the tip; each
  layer below ends `RAKE_MM` (0.2) shorter, so the tip overhangs open air
  to the last layer.
- **REQ-B-5 [R] Points.**  Every horizontal section of a blade ends in a
  point at both ends (strokes step down a line at a time to one line's
  round end).  No blunt front face.
- **REQ-B-6 [R] Smooth.**  The STL blade is the stroke stack smoothed: no
  layer steps or ribs, outline, keel and points kept.
- **REQ-B-7 [S]** Every blade is fused to the ground (embedded 0.3 mm);
  no islands, no hinges.  Grass stays ≤ 3.5 mm above the local ground.
- **REQ-B-8 [S]** An audit (overhang area, crumbs) runs on every output;
  `blades_demo.py` writes single blades, smoothed or as the raw stroke
  stack, for inspection.

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
  - Where grass faces a clearing, the last blades stand separately on the
    dirt, each with its own keel, raked bow and undercut tip (REQ-B-*), so
    the bank is a pile of blades with shadows between them, never a smooth
    wall (F25).
  - Bank grass varies: fronds wander in heading, height and spill, some
    cross at an angle, and how strongly the flow hugs each clearing
    varies, so the bank line meanders.

### 2.5 Swaths

- **REQ-SW-1 [S] Total coverage.**  Every point belongs to exactly one
  swath cell.  Swaths group strands and lengthen fronds toward each cell's
  downstream end; they no longer carry a height wave (there is no turf
  mass to carry it).

### 2.6 Blades in the field

- **REQ-FR-1 [R]** Blades are elongated along the flow, 2.8–7.2 mm,
  longer toward each swath's downstream end.
- **REQ-FR-2 [R] Shingling.**  Each tip lies over or beside the next
  blade's base downstream; overlap always reads in that direction.
- **REQ-FR-3 [R] Blades on dirt.**  *(Shawn, 2026-10-29: "no turf
  mass.")*  Every bit of grass is a blade; the dirt shows between them.
  No pad, skirt or filler under or between blades.
- **REQ-FR-5 [R] Variation without repetition.**  Length, width (in
  lines), height and small heading offsets vary; no two neighbours are
  identical copies (F7).

### 2.7 Edges: grass ends as grass

*From Shawn's review of v1e: "the grass is just being uniformly chopped
down like it was all made of clay."*

- **REQ-EDGE-1 The grass is made of blades.**  No grass height exists
  except as part of some blade: no free-standing envelope or mass layer,
  no mask or scale applied to the grass as a whole.
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
- **REQ-EDGE-7 [R] Grass flows to the rim, not away from it.**  Near the
  edge, the flow points *at* or runs *alongside* the wall, never away
  (F21).  The steering fades in over 9–15 mm and blends into the interior
  with no seam (F26); flow already heading outward leans further out near the
  rim; long stretches of rim turn the same way (one side per stretch,
  corners continuing the adjacent edge), so the flow does not part along
  the wall; where two stretches meet, the flow comes square to the wall and
  ends there: a short parting, never a long feathered V or a starburst; at
  the very rim nothing heads inward past parallel.
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
- **REQ-SOIL-4 [R] Dirt everywhere exposed.**  Bare patches and the
  ground between blades carry the shared dirt texture
  (`dirtlab.dirt_relief`).  Patches sit about 0.4 mm below the
  surrounding ground with no outlining step.  A few sprigs grow in the
  soil.

### 2.9 Surface quality

- **REQ-GR-1 [R]** Dark gaps between neighbouring blades and air under
  every tip.
- **REQ-GR-2 [R]** No voxel terracing, layer steps or ribs on any blade
  (REQ-B-6).

### 2.10 Parameters and diagnostics

- **REQ-PAR-1** Knobs are named, bounded module constants grouped by
  stage, with a seed.
- **REQ-DIAG-1 [S]** Each run prints mesh, hills, soil, grass-height, DB
  band, overhang audit and timing statistics.
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
| F26 | **Rim seam** | flow meeting itself head-on along the wall: a long feathered V, or a starburst at a corner | meadowlab sweep (v2f) |

**Pre-paint test:** the bare grey print must read as grass.

---

## 4. Acceptance scenes

| Scene | Content | Passes when |
|---|---|---|
| **S0: hills only** | 2 × 2, grass off | Reads as natural ground; REQ-HILL-3/4 hold. |
| **S1: flat study** | 1 × 1, flat ground (`--flat`) | Ribbons, blades, undercut tips and gaps read clearly.  Judged first, before hill or full-tile tuning. |
| **S2: approved tile** | 2 × 2 with hills, seed 1 | All of §2 holds, no F-mode is visible. |
| **S3: seed sweep** | S2 at seeds 1–6 | At least 5 of 6 pass; none looks stamped. |
| **S4: print** | S2 printed, primed, drybrushed | Tip and bank shadows survive, grooves stay dark, nothing breaks. |

---

## 5. Open questions

1. Should fronds carry a midrib or sub-grooves for extra texture, or does
   the frond-to-frond groove suffice?  Judge on S1.
2. Final dimensions come from the S4 print.

---

## 6. Measured on the current code (v3; seed 1, 2 × 2)

| Check | Seed 1 |
|---|---|
| Ground (REQ-HILL) | 3.40–6.58 mm, max slope 34.0°, 78% ≤ 12° |
| Soil (REQ-SOIL-2) | 6 lenses, 9.6% of the tile; dirt showing between blades 9.1% |
| Blades | 3,288; ≥ 99.9% of every stroke layer wider than a line (REQ-B-1) |
| Grass above ground (REQ-B-7) | mean 1.28, p95 2.00, max 2.20 mm |
| Mean grass top (REQ-OUT-3) | 5.24 mm (OK) |
| Overhang audit | 15.4 mm² (0.14%) |
| Mesh, size, time | 1.74M faces, watertight, 87 MB, ≈ 100 s |

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
- **REQ-FDM-1…8 (rev 3) → REQ-B-1…8 (rev 4).**  The voxel FDM rules
  (45° downward faces, wedges not slots, ≥ 0.6 mm free features, a voxel
  backstop) described a height field with carved lips.  They never gave a
  visible undercut, and the slicer still dropped everything narrower than
  a line.  Replaced by defining blades in layers and lines.
- **REQ-SW-2…4 (swath height wave, shingled leading edges), REQ-FR-3/4
  (domed bodies, 45° lips), REQ-EDGE-6 (soft bank ends), REQ-SOIL-4
  pebbles.**  All belonged to the turf mass (skirts) or the height-field
  frond, both removed 2026-10-29.
