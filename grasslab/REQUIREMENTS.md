# grasslab — Sculpted Grass Tile: Requirements

*Revision 2, 2026-10-27.  Status: requirements for v1.*

*Visual target: `reference/approved-reference.png` ("Sculpted Grassy
Terrain Tile Showcase").  Source: the procedural sculpted-grass visual
specification Shawn supplied on 2026-10-27, which covers a full landscape
generator.  This document is the grass + hills slice of that spec,
checked against nine earlier grass attempts (§7) and the failed v0 (§7.3).*

*Revision 2 changes: the grass is now specified **top-down, by visual
scale**, with swaths as the defining feature.  Revision 1 specified
blade-scale geometry and hoped the larger structure would emerge, and v0
showed that it doesn't.*

---

## 1. What this is

A standalone experiment, kept apart from the dharmatiles tile pipeline, that
writes **one STL: a standard DungeonBlocks tile of gently hilly ground
covered in sculpted grass**.

> **Build the illusion, not botany.**  At tabletop distance the reference
> reads as grass because of how its **mid-scale masses** are organised:
> combed swaths that shingle over one another, each with a lit crest and a
> dark, undercut leading edge.  Fine detail only decorates that structure.

### 1.1 The three visual scales (the core of this spec)

| Scale | Size (from the reference) | What it does | Built from |
|---|---|---|---|
| **Swath** | 12–25 mm long, 8–16 mm wide | A combed mass where everything points one way.  It rises along the flow to a **crest** and ends in a **leading edge** that laps over the next swath with a dark shadow beneath.  Swaths carry the "flowing grass" read. | the swath layout and its height envelope |
| **Frond** | 4–8 mm long, 1.8–3 mm wide | Pointed, feather-like lobes that tile each swath's surface and make its leading edge jagged.  They shingle, with each upstream tip over the next downstream base, and fan slightly from the swath axis. | frond bodies |
| **Groove** | ≥ 0.6 mm wide, 0.4–1.0 mm deep | The dark channels between fronds and under their tips, where the base coat survives a drybrush. | the gaps between frond bodies |
| *Leaf point* | < 1 mm | **Paint and lighting, not geometry.** | — |

### 1.2 Geometry is true 3D

Real 3D undercuts are allowed, but **only where they are designed**: under
frond tips and swath leading edges, facing downstream (§2.2, REQ-FORM-5).
Everywhere else the grass is a solid mass down to the ground.

### 1.3 Scope

In scope: a standard DB tile (35 mm squares, the existing socket-peg base,
the official grass height band); independent hills; the grass hierarchy;
printability by construction plus an audit.  Exposed soil patches are in scope (§2.3a, added at Shawn's request on
2026-10-27).  Out of scope: rocks,
OpenLOCK, colour, seamless tiling, and `.tile.py` integration.  The project
lives in `grasslab/`, imports dharmatiles primitives freely, never modifies
`src/dharmatiles/`, and writes its output to `grasslab/out/`.

---

## 2. Requirements

Each requirement is checkable from either a viewer or render (**[R]**) or a
script or statistic (**[S]**).

### 2.1 Output and mesh

- **REQ-OUT-1 [S]** A single run writes one binary STL,
  `grasslab/out/<cols>x<rows>-sculpted-grass[-flat]-s<seed>.stl`.
- **REQ-OUT-2 [S]** The mesh is one closed manifold body.  It is
  watertight, has consistent normals, no self-intersections and no
  internal voids.  Each run reports vertex, face and body counts.
- **REQ-OUT-3 [S]** The tile uses the standard DB socket-peg base
  (`dharmatiles.bases.dungeonblocks`).  The *mean* grass-top height lies
  in the official DB grass band, **15.3–16.8 mm above the piece bottom**
  (about 4.4–5.9 mm above our terrain datum; commit `6eb0a55`).
- **REQ-OUT-4 [S]** The footprint is cols × rows DB squares, defaulting
  to 2 × 2.
- **REQ-OUT-5 [S]** It is deterministic: the same seed and parameters
  produce the same STL.
- **REQ-OUT-6 [S]** The STL is under 50 MB at 2 × 2, and the build takes
  under 60 s.

### 2.2 FDM rules (hard constraints; 0.4 mm nozzle, base-down print)

- **REQ-FDM-1 [S]** No downward-facing surface is steeper than **45°**
  from vertical, beyond micro-lips of 0.5 mm or less.
- **REQ-FDM-2 [S]** No islands: every layer is supported by the layer
  below within 45°.
- **REQ-FDM-3 [S] Gaps are wedges, never slots.**  An undercut opens from
  its root at 45° or wider, which is the chamfered underside of a lip.
  Parallel-walled air gaps thinner than 0.6 mm are forbidden, because
  slicers fuse them unpredictably and paint can't reach them.
- **REQ-FDM-4 [S]** Free features (lip tips) are at least 0.9 mm in XY
  and 0.6 mm thick in z.  No needles.
- **REQ-FDM-5 [S]** Every body is fused to the mass, with no hinges or
  pinches.
- **REQ-FDM-6 [S]** The tile edges are solid, with no thin lips.  How the
  grass *ends* at the rim is governed by REQ-EDGE-3.
- **REQ-FDM-7 [S]** Grass stays at most 3.5 mm above the local ground.
  This is budgeted on the inputs, never clamped afterwards.
- **REQ-FDM-8 [S]** An automated audit (overhang area, body count,
  support-filter removals) runs on every output.  Any violation is
  reported.

### 2.3 Hills (unchanged from revision 1)

- **REQ-HILL-1** The ground H₀ is generated independently of the grass,
  and still reads as rolling ground with the grass removed.
- **REQ-HILL-2 [R]** It is broad and irregular, with landforms 15–40 mm
  across, no grid and no repeated bumps.
- **REQ-HILL-3 [S]** Ground peak-to-valley is about 5–6 mm over smaller landforms (σ 10 mm), raised across three review rounds.  The ground stays ≥ 1 mm above the base (soft floor).
- **REQ-HILL-4 [S]** At least 60% of the area has a slope of 12° or less,
  and no slope exceeds 34°.
- **REQ-HILL-5** H₀ has no high-frequency noise.
- **REQ-HILL-6 [S]** The ground is feathered to a constant edge height.
- **REQ-HILL-8 [S] Rises only.**  *(Shawn, 2026-10-27.)*  The perimeter
  level is the lowest ground on the tile.  Hills rise from flat plains
  at edge height, about 40% of the interior stays plain, and nothing
  dips below the edge.
- **REQ-HILL-7** The grass seats on H₀ and follows its curvature.

### 2.3b Edges: grass ends as grass (all boundaries)

*Added 2026-10-27 after Shawn's review: "the grass is just being uniformly
chopped down like it was all made of clay", both at the tile edge (a bevel)
and at bare spots ("a hole in the grass volume with a couple of grass kind
of things poking around it").  Confirmed: the swath envelope was a
free-standing clay layer, and boundaries were cuts through it.*

- **REQ-EDGE-1 The grass volume is made of fronds.**  No grass height
  exists except as part of some frond's body.  There is no free-standing
  envelope or mass layer, and no mask or scale factor applied to the grass
  volume as a whole.
- **REQ-EDGE-2 [R] Soil banks.**  The edge of a bare patch is the outline
  of the last individual fronds: their tips and sides, at varied heights,
  with dark gaps between them, lapping slightly over the soil.  There are
  no carved walls, no continuous cliffs, and no hole-in-a-volume look.
- **REQ-EDGE-3 [R] Tile rim.**  Fronds near the rim end with their own
  shapes.  They are shortened (never sliced) so tips stop just inside the
  wall, and fronds rooted at the rim keep their rounded bases.  There is no
  uniform bevel or height ramp toward the edge.
- **REQ-EDGE-4 [R] Swath borders** inside the grass are likewise made of
  overlapping frond lobes, never vertical steps in a height layer.
- **REQ-EDGE-5 [R]** Fronds cut short at a boundary are also lower, so a
  fringe steps down rather than ending in tall stubs.
- **REQ-EDGE-7 [R] Grass flows to the rim, not away from it.**  Near the
  tile edge, fronds point *at* the edge or run *alongside* it.  They never
  point away from it (base at the wall, tip heading inward), which reads
  as grass sprouting from the wall.  This matches the reference, where
  grass meeting a tile edge is combed toward or along it.
  - The steering fades in over about 12 mm from the rim and blends into
    the interior flow, with no visible seam or band.
  - Fronds that already point toward the edge lean further toward it
    the closer they are.
  - All fronds of one swath turn the **same way** when steered away from
    an inward heading, so a swath never splits into a parting.
  - At the very rim, nothing heads further inward than parallel.
- **REQ-EDGE-8 [R] Irregular rim endings.**  Fronds stop at varied
  distances from the wall, and steering strength varies from frond to
  frond.  The rim must never show a ruled line of tips at a constant
  inset, while REQ-EDGE-7 still holds: nothing at the rim heads inward.
- **REQ-EDGE-6 [R]** Bank fronds end softly: no flat "sawn log" faces at a
  cut tip, and no cliff at a base facing the soil.  This must not change
  interior or rim fronds, because rounding every frond's ends turned the
  whole tile into fish scales (F7).

### 2.3a Bare soil

*Revised 2026-10-27 after Shawn rejected the first version ("bare patches
are terrible").  The first version made noise-warped ellipse puddles with
a faded grass ramp, a dark trench outline and round bubble pebbles, all
placed independently of the grass.  Reference analysis: bare earth is
where the grass **parts**.*

- **REQ-SOIL-1 [R] Soil comes from the grass structure.**  Bare earth
  appears only along a few **swath seams**, the natural breaks between
  combed masses, preferring low ground and seams where both swaths are
  already low.  It never appears as free-floating blobs.
- **REQ-SOIL-2 [S] A few exposures.**  They are spread across the tile
  (lanes start 18 mm or more apart), cover roughly 10–15% of the area, and
  stay at least 4 mm from the rim.  Grass stays dominant.
- **REQ-SOIL-3 [R] Shape.**  Exposures are elongated with the seam, have
  irregular widths, and are discontinuous: a lane breaks into a few
  clearings.  They are never circles, puddles or ellipses.
  - **No slits:** any stretch narrower than about 2.2 mm closes, so
    nothing reads as a crack or trench line.
- **REQ-SOIL-4 [R] Banks, not ramps.**  The soil mask decides only *where
  fronds may root*; it never cuts the grass (REQ-EDGE-1/2).
  - Bank fronds lap up to about 1.5 mm into the soil.
  - The soil sits about 0.4 mm below the surrounding ground, in a trough.
- **REQ-SOIL-5 [R] Soil texture is subordinate.**  It has low roughness
  (about 0.08 mm) and **sparse, angular, faceted pebbles** of varied size
  (0.45–1.2 mm radius), shaped as low domes cut by 7–10 shallow facets,
  with no flat tops, prism walls, round bubbles or clusters.  A few stray grass sprigs grow in the soil.  All of it is
  height field, so it is printable by construction.

### 2.4 Swaths (the defining feature)

- **REQ-SW-1 [S] Total coverage by construction.**  Every point of the
  tile belongs to exactly one swath.  There is no space between swaths.
- **REQ-SW-2 [R] Coherent inside, varied between.**  Within a swath the
  flow direction is nearly constant, varying ±15° or less apart from the
  frond fan.  Neighbouring swaths differ by noticeable but bounded
  angles, as a smooth regional field.  A 2 × 2 tile has at least three
  visibly different dominant directions.
- **REQ-SW-3 [R] Wave profile.**  A swath is low at its upstream end and
  at its lateral sides, and rises along the flow to a crest near its
  downstream end.  Lateral neighbours meet low, so there are **no cliffs
  or terraces between side-by-side swaths**.
- **REQ-SW-4 [R] Shingled leading edge.**  The fronds at a swath's
  downstream end overhang the low upstream end of the next swath.  That
  casts the **dominant shadow line** of the tile, a designed undercut
  roughly 0.8–2 mm high.
- **REQ-SW-5 [R] Irregular borders.**  Swath boundaries are never straight
  lines, polygons or repeated shapes.
- **REQ-SW-6 [R] No crowns or vortices.**  Swath directions come from a
  smooth, singularity-free field.  There are no starbursts or spirals at
  any scale.

### 2.5 Fronds

- **REQ-FR-1 [R]** Fronds are elongated along the local flow, at 4–8 mm,
  with length varying continuously and growing toward the crest.  Planform
  is widest about one third of the way along and tapers to a blunt point
  (tip half-width ≥ 0.45 mm).
- **REQ-FR-2 [R] Shingling.**  Fronds overlap along the flow, each tip
  lying over the base of the next one downstream.  Overlap always reads in
  that direction.
- **REQ-FR-3 [R] Fan.**  Fronds fan slightly away from the swath axis in
  proportion to their lateral position (up to about ±12°).  This gives the
  combed, feathered read of the reference close-up.
- **REQ-FR-4 [R] Domed bodies.**  The cross-section is a rounded dome that
  catches a drybrush, with no fins, ropes or tubes.  A frond emerges flush
  from the mass at its base, so no frond is a separate pod sitting on top.
- **REQ-FR-5 [R] Lifted tips.**  The last part of each frond is a lip
  whose underside is chamfered at 45°.  It casts a small shadow over
  whatever lies downstream, and fuses where that surface is high.
- **REQ-FR-6 [R] Variation without repetition.**  Size, length, fan and
  slight curvature vary, so no two neighbouring fronds are identical
  copies.  *Feathery is the target; identical, regularly repeated fronds
  are the failure (F7).*

- **REQ-FR-7 [R] Fronds arc, coherently.**  Compared against the
  Majestic Highlands grass texture (Shawn, 2026-10-27), our straight fronds
  read too rigid.
  - **Shape:** each frond is a smooth **sickle**.  Its heading turns
    progressively, with the bend concentrated toward the tip, for a total
    of roughly 20–70°.
  - **Not random:** neighbouring fronds arc **in the same sense and by
    similar amounts**, so clumps read as coherent swirls and sweeps.
  - **Regional variation:** curvature sign and strength vary smoothly
    across the tile, with some regions nearly straight and others strongly
    curled.
  - **Anti-goals:** no per-frond random bend, and no S-curves within one
    frond.

### 2.5a Flow ribbons (supersedes REQ-FR-7 sickles and the seam soil of §2.3a)

*Added 2026-10-27, from Shawn's wind-lodged grass photo: long combed
ribbons meandering in S-curves, with the flow parting around eye-shaped
bare patches.  The v1k sickles read as "clumps of commas" next to it,
because each frond bent on its own and no flow line continued from one
frond into the next.*

- **REQ-FL-1 [R] Fronds follow the flow.**  Every frond's spine is a
  streamline of the direction field, with no per-frond bend.  Neighbours
  stay parallel along their whole length, so curvature belongs to the
  flow, not to individual blades.
- **REQ-FL-2 [R] Meandering flow.**  The field carries gentle S-curves
  (wavelength about 10–20 mm, ±10–30°) that are **coherent across the
  flow**: neighbouring streamlines wave in phase, like combed hair.
  - Wave phase and amplitude drift across the tile, so the waves are
    never regular.
  - Regional direction still varies (several flow regions, not one
    sweep).
  - The field stays singularity-free except at the deliberate partings
    at the upstream end of bare patches (REQ-FL-4).
- **REQ-FL-3 [R] Ribbons.**  Fronds gather into strands about 2–3 mm
  apart.  Each strand is 15–30 mm long, built from several fronds
  shingled along the same streamline.
  - Dark seams run between strands.
  - Strands cross their neighbours only at shallow angles.  A few stray
    fronds (about 5%) cross more steeply.
- **REQ-FL-4 [R] Flow wraps around bare soil.**  Bare patches are
  **eye- or lens-shaped openings elongated along the local flow**, with
  irregular edges, preferring low ground.
  - The direction field bends tangentially around each patch: grass
    parts at the upstream end and rejoins downstream, like water around
    a stone.
  - Bank fronds lap onto the soil by a varying amount, and the
    occasional wisp crosses further.
- **REQ-FL-6 [R] Banks read as piles of blades.**  *(Shawn, 2026-10-27:
  "a big pile of grass leans up right against [a clearing]… a vertical
  wall… give it some texture… undercuts at the bottom of the grass blade
  so there's clear ridges down the side.")*
  - **Undercuts:** where grass faces a clearing, each blade's
    soil-facing edge overhangs the body below it as a ledge with a
    **45° chamfered undercut**.  A stack of blades at different heights
    therefore shows ridge lines down the bank face, each with a dark
    shadow beneath.  A vertical bank is fine; a smooth, featureless one
    is not.
  - **Variability:** bank grass doesn't all hug the clearing outline.
    Bank fronds wander in heading, vary in height and spill, and some
    cross the bank at an angle.  How strongly the flow bends around each
    clearing also varies, so the bank line meanders and its face has
    irregular texture.
  - **Printability:** every side ledge obeys REQ-FDM-1…4 by construction
    (45° underside, ≥ 0.6 mm thick at the edge, fused to its body).
- **REQ-FL-5 [R]** Rim rules (REQ-EDGE-7/8) still hold, now enforced on
  the flow field itself.  Irregularity comes from spatially varying
  steering and per-frond tip insets.

### 2.6 Grooves and surface quality

- **REQ-GR-1 [R]** Dark grooves run between adjacent fronds along the
  flow, and under every lip.  They are at least 0.6 mm wide at half depth
  and 0.4–1.0 mm deep.
- **REQ-GR-2 [R] Smooth surfaces.**  There is no voxel terracing or
  stair-stepping on any surface.  The tessellation resolves 0.6 mm
  grooves cleanly.
- **REQ-GR-3 [R] Crisp crests.**  Groove edges are defined, not blurred
  into soft hills (F10).

### 2.7 Parameters and diagnostics

- **REQ-PAR-1** Knobs are named, bounded module constants (promoted once
  accepted), with one `approved` preset and a seed.
- **REQ-DIAG-1 [S]** Each run prints its statistics: mesh, hills, grass
  height, DB band, FDM audit and timing.
- **REQ-DIAG-2** Before any STL is handed over, Claude checks it with one
  quick monochrome render, as an internal filter only.  Shawn's eye in a
  viewer is the judge.

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
| F7 | **Stamped repetition** | identical fronds or swaths in regular patterns (feathery itself is fine) | ThatchGrass bushy |
| F8 | **Fingerprints / swirls** | loops, spirals | Era 1 |
| F9 | **Crumpled foil / rough concrete** | relief everywhere with no mid-scale structure | **grasslab v0** |
| F10 | **Flat or melted** | no silhouette; grooves blurred out | Era 5; ThatchGrass E2 |
| F11 | **Fragile literal grass** | needles, floating tips | — |
| F12 | **Tent peaks** | stacks spiking above neighbours | Era 2; ThatchGrass E4 |
| F13 | **Z-jitter** | neighbours at random heights | Era 5; grasslab v0 |
| F14 | **Terracing** | voxel stair-steps, side-by-side swath cliffs | **grasslab v0** |
| F15 | **Ragged rim** | stubs along the tile edge | **grasslab v0** |
| F16 | **Puddles** | amoeba-shaped bare blobs with wiggly outlines and a trench rim, unrelated to the grass | **grasslab v1c/d** |
| F17 | **Bubbles** | round, clustered dome pebbles | **grasslab v1c/d** |
| F19 | **Clay cut** | grass ending as a carved wall, bevel or hole rather than at individual fronds | **grasslab v1e** (rim bevel, soil holes, swath-border steps) |
| F25 | **Smooth bank wall** | grass at a clearing ends in one featureless vertical face | grasslab v2d |
| F24 | **Commas** | short independently bent fronds with no continuous flow lines | grasslab v1k (vs lodged-grass photo) |
| F22 | **Rigid sticks** | straight fronds, no coherent arcing | grasslab v1i (vs Majestic Highlands) |
| F23 | **Ruled rim** | frond tips lined up at a constant inset along the wall | grasslab v1i |
| F21 | **Wall sprouts** | grass whose base sits at the tile edge and whose tip points inward | grasslab v1h (Shawn review) |
| F20 | **Soil debris** | boxy cube-like pebbles, prism chips, and a dashed step outlining each patch | grasslab v1g (fixed in v1h) |
| F18 | **Cracks** | hairline soil slits along seams | grasslab v1e (fixed) |

**Pre-paint test:** the bare grey print must read as grass.

---

## 4. Acceptance scenes

| Scene | Content | Passes when |
|---|---|---|
| **S0: hills only** | 2 × 2, grass off | Reads as natural ground; REQ-HILL-4 holds. |
| **S1: swath study** | 1 × 1, flat ground (`--flat`) | Individual swaths with crests, shingled leading edges, fronds and grooves read clearly.  This is judged **first**, before any hill or full-tile tuning. |
| **S2: approved tile** | 2 × 2 with hills, seed 1 | All of §2 holds, no F-mode is visible, and it reads as the reference's grass. |
| **S3: seed sweep** | S2 at seeds 1–6 | At least 5 of 6 pass, and none looks stamped. |
| **S4: print** | the S2 tile, printed, primed and drybrushed | Shadows under the swath edges survive, grooves stay dark, and nothing breaks. |

---

## 5. Physical starting points (calibrate from prints)

| Parameter | Start |
|---|---|
| Swath spacing / length / width | 11 mm / 12–25 / 8–16 mm |
| Swath crest height (envelope) | 1.2–2.2 mm above ground |
| Frond length / half-width / dome height | 5.5–9.5 / 0.55–0.9 / 0.65–1.0 mm (v2d) |
| Frond spacing (Poisson radius) | 2.0 mm |
| Lip length / tip thickness | ≤ 2 mm / ≥ 0.6 mm |
| Direction field coherence / swing | 15 mm / σ 60° |
| Rim ease | grass height × 0.5 at the edge, full at 3 mm in |

---

## 6. Open questions

1. Should fronds also carry a midrib or sub-grooves (feather barbs) for
   extra texture, or does the frond-to-frond groove suffice?  Judge on S1.
2. Should the swath crest sit at the downstream end, or nearer the middle
   with a short roll-over?  Judge on S1.
3. Final dimensions come from the S4 print.

---

## 7. Lessons from previous attempts

### 7.1 dharmatiles grass, Eras 0–8

These are recorded in
`docs/meta/history/2026-07-03-grass-greenfield-requirements.md` and
`docs/design/grass-mound-thatch.md`, and cover the eras up to the current
ThatchGrass default.  Recurring needs: proud-of-lower layering (reinvented
five times), coherent but varied direction, and designed (not emergent)
depth.  Recurring failures: height blowup, Z-jitter, bare borders, a flat
mat, and indecision churn.

### 7.2 What carries over

Each of these becomes a by-construction guarantee here:

- proud-of-lower layering, as shingling (REQ-FR-2);
- coherent direction, from a singularity-free field (REQ-SW-6);
- total coverage, from the swath partition (REQ-SW-1);
- the height budget, enforced on the inputs (REQ-FDM-7).

### 7.3 grasslab v0 (2026-10-27): why it failed

v0 built ~1,800 small lens blades in random order inside a binary voxel
volume.  It came out printable and watertight, and read as **crumpled
foil** (F9).

- **It was built bottom-up.**  There was no swath or crest structure in
  the requirements, so none appeared in the output.
- **Undercuts appeared incidentally** on every blade's sides, and the 45°
  filter chamfered them into grit.  Undercuts must be designed, and only
  at lips.
- **Random stacking order** produced Z-jitter (F13).
- **The binary voxel volume plus blur** produced terracing (F14).
- **Clipped blades** left a ragged rim (F15).
- **Process:** the 2 × 2 tile was built before S1 was looked at, and the
  STL was handed over unseen.
