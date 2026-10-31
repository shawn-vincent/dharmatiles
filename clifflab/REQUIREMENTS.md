# clifflab — Heroic Cliff Run: Requirements

*Revision 1, 2026-10-30 (no code yet).  Visual targets:
`reference/heroic-cliff-spec.md` (Shawn's "Universal Heroic Cliff Face"
spec, the authority on surface character) and `reference/reference-sheet.png`
(16 painted cliff miniatures; **B, H and N** are this lab's form).  A
standalone experiment built like the other labs: it may import dharmatiles
primitives and the other labs, never modifies `src/dharmatiles/`, and
writes to `clifflab/out/`.*

*Shawn's choices for gen-1 (2026-10-30): a **straight cliff run**, **FDM
without supports**, **about one storey (30–40 mm)**, **dirt top and scree
foot**, no vegetation.*

---

## 1. What this is

One STL: **a standard 2 × 2 DungeonBlocks tile crossed by a cliff.**  Low
ground (the foot) runs along the front, a plateau (the top) along the
back, and between them a monumental, weathered, fractured rock face about
one storey high.

> **Heroic without being eccentric.**  Broad, powerful, quiet stone faces
> interrupted by deep, expressive fractures, mostly vertical and steeply
> diagonal, with broken ledges.  The face must read as a big cliff and as
> a low ledge alike: **no scale markers**.

### 1.1 Visual scales (nested, self-similar)

| Scale | Size on the tile | What it does |
|---|---|---|
| **Large forms** | masses 8–35 mm wide; major fissures 2–5 mm deep | Buttresses, recesses and broad faces changing plane; the silhouette and the big shadows. |
| **Medium forms** | 3–10 mm | Broken plates, shallow shelves, overlapping faces, chipped corners, secondary fractures. |
| **Fine detail** | 0.5–3 mm | Chips, hairline cracks, pits, grain, erosion channels.  Printable sizes only (§2.2). |

A small crack and a major fissure belong to the same family: the same
directions, the same wedge profile, the same branching, at different sizes.

### 1.2 What it is not (gen-1 scope)

Out of scope for acceptance: corners, spires, outcrops, escarpments,
canyons (later forms from the sheet); grass and other vegetation
(grasslab can add them later); colour and paint; resin-specific detail;
OpenLOCK.

- **REQ-RANGE-1** *(Shawn, 2026-10-30: "I love those photos, especially the
  range.  We should not build something that can't meet those needs.")*
  The design must be able to reach every form on the reference sheet
  (subject to the FDM rules): nothing in it may assume a single flat face.
  Corner, canyon, outcrop and spire previews are built alongside the run.

---

## 2. Requirements

Each requirement is checkable from a viewer or render (**[R]**) or from a
script or statistic (**[S]**).

### 2.1 Output and tile

- **REQ-OUT-1 [S]** A run writes `clifflab/out/2x2-cliff-s<seed>.stl`.
- **REQ-OUT-2 [S]** One watertight manifold body (passes the slicer-style
  reload check `stl_safe`), fused to the standard DB socket-peg base
  (`dharmatiles.bases.dungeonblocks`).
- **REQ-OUT-3 [S]** The foot ground sits in the DB dirt band (4.0 mm above
  the terrain datum, as rocklab and dirtlab).  The plateau top is **30–40
  mm above the foot**.
- **REQ-OUT-4 [S]** Deterministic per seed; builds in under 2 minutes;
  STL under 100 MB.

### 2.2 FDM rules (hard; 0.4 mm nozzle, 0.2 mm layers, base-down, no supports)

*Lesson from grasslab v3: model only what prints.*

- **REQ-FDM-1 [S]** No downward-facing surface steeper than **45° from
  vertical**.  Overhanging shelves and undercuts are wedges with 45°
  undersides.
- **REQ-FDM-2 [S]** No islands at 0.2 mm layers: every layer is supported
  by the one below.
- **REQ-FDM-3 [S] Fissures are wedges, never slots.**  Every fracture
  opens outward; its mouth is ≥ 0.6 mm wide wherever it is deeper than
  0.5 mm, so paint can reach and slicers don't fuse it.
- **REQ-FDM-4 [S] No sub-line features.**  Free ridges, chips and plates
  are ≥ 0.45 mm (one line) wide; relief smaller than ≈ 0.15 mm is not
  modelled (it would not print, so it would only mislead the viewer).
- **REQ-FDM-5 [S]** The plateau and the foot are solid ground to the base.

### 2.3 Layout of the run

- **REQ-RUN-1 [R]** The cliff crosses the tile from the left edge to the
  right edge, its face toward the front.  The face line wanders in plan
  by about ±5 mm: buttresses stand out, recesses and gullies cut in.  It
  is never a straight wall.
- **REQ-RUN-2 [R]** The face is near-vertical overall (mean batter 0–10°
  back from vertical), steeper and overhanging locally (within
  REQ-FDM-1).
- **REQ-RUN-3 [R] Rock all round; contact rims seal.**  *(Shawn,
  2026-10-30, rev 3 of this requirement: "a big wall made of rock, textured
  on all four sides with dirt on the top… the edge of the face that is
  against the tile edge needs to be low relief… the middle can have big
  cuts and rocky construction.")*  A cliff is a rock body textured on every
  side (front, ends and back), the texture continuous round its corners
  (a front mass and the crevice above it curl round onto the end).  Faces
  lying along the tile boundary may cut inward freely in their middle but
  never cross the boundary, and at their rims (top, bottom) they come
  flush with it, so the piece fits tight against neighbouring cliffs and
  the ground beside it.  No matching profiles, nothing repeating.
- **REQ-WALL-1 [R]** A free-standing rock wall (`wall` form) across the
  tile, both faces fully textured: both sides of it are useful.
- **REQ-EDGE-1 [S] Edge cliffs are thin bands.**  *(Shawn, 2026-10-30.)*
  Cliffs that run to the tile edges (run, corner, canyon) hug the back
  edge(s) as a band ≈ 8 mm deep, a small share of a 1 × 1 tile (they must
  work on 1 × 1), with lots of depth projecting forward from it and a
  little lean out.  The top is ground (dirt), at plateau level at the back
  edge, so raised ground set behind the cliff merges with it; it need not
  be wide enough to stand a miniature on.  Free-standing forms (outcrops,
  spires) have no such limit.
- **REQ-RUN-4 [R]** The top edge is broken and irregular: blocks standing
  proud, notches, and a few step-backs, never a ruled line.

### 2.4 Rock masses and fractures (the face)

- **REQ-ROCK-1 [R] Interlocking masses, not stacked boulders.**  Broad,
  angular, somewhat flattened masses that appear naturally joined.  Some
  faces are flat and imposing, others project as chunky broken shoulders.
  Mass widths vary by at least 4×.
- **REQ-ROCK-2 [R] Vertical character, broken horizontals.**  Joints run
  mostly vertical and steeply diagonal (most within ±25° of vertical, a
  minority at 30–60°).  They are interrupted by short irregular
  horizontal ledges, diagonal fractures that cross several faces,
  occasional projecting shelves and undercuts, and subtle discontinuous
  layering.  Joints bend, branch, stop and reappear.
- **REQ-ROCK-3 [S] Controlled irregularity.**  By face area: about 60%
  broad quiet faces, 30% substantial fractures, offsets, ridges and broken
  edges, 10% intensely detailed crevices and shattered zones, in clusters
  rather than spread evenly.
- **REQ-ROCK-4 [R] Planes, not noise.**  Faces are planar or gently
  curved facets meeting at edges; the rock is never a noise blob or a
  melted surface.  Fine detail decorates the planes; it doesn't replace
  them.
- **REQ-ROCK-5 [R] Age.**  Exposed convex edges are softened by weathering
  (rounded about 0.3–0.8 mm) with small missing chips and shallow erosion
  channels.  Deep recesses stay sharp.
- **REQ-ROCK-6 [R] Heroic depth.**  A little deeper than realistic: major
  fissures 2–5 mm deep, pronounced changes of plane, strong projecting
  edges, so the face carries dramatic shadow.  Not cartoonish.
- **REQ-ROCK-7 [R] No pattern, no repetition.**  No regular blocks or
  courses, no repeated column widths, no regular joint spacing.  Two seeds
  look like the same stone, never the same cliff.

### 2.5 Top and foot

- **REQ-TOP-1 [R]** The plateau is ground with the shared dirt texture
  (`dirtlab.dirt_relief`).  It meets the cliff edge as rock: the tops of
  the cliff's masses show through near the edge.
- **REQ-FOOT-1 [R]** The foot is dirt ground with **scree**: fallen blocks
  of the same stone (rocklab-style faceted rocks, 1–8 mm, mixed sizes),
  denser at the cliff base and below fissure mouths, thinning out toward
  the front edge.  The scree never forms a uniform carpet of same-size
  pebbles (a scale marker).
- **REQ-FOOT-2 [R]** No vegetation anywhere.

### 2.6 Parameters and diagnostics

- **REQ-PAR-1** Named, bounded module constants grouped by stage, with a
  seed.
- **REQ-DIAG-1 [S]** Each run prints mesh, heights, overhang and island
  audits, the quiet/fracture/detail area split (REQ-ROCK-3) and timing.
- **REQ-DIAG-2** Before handing over an STL, Claude checks it with quick
  renders (oblique, face-on and raking light) as an internal filter only.
  Shawn's eye in a viewer and slicer is the judge.

---

## 3. Failure modes (named anti-goals)

| # | Failure | Looks like |
|---|---|---|
| C1 | **Stacked boulders** | rounded lumps piled on each other; no structural masses |
| C2 | **Organ pipes** | regular vertical columns (basalt) |
| C3 | **Layer cake** | continuous even horizontal bedding (sandstone) |
| C4 | **Masonry** | blocks in courses, right angles everywhere: a wall, not a cliff |
| C5 | **Gravel** | every square millimetre fractured; no quiet faces; no sense of scale |
| C6 | **Noise blob** | a melted, lumpy surface with no planes or edges |
| C7 | **Wallpaper** | a visible repeat or a tiled texture |
| C8 | **Paint-on** | relief too shallow to cast shadow; a flat wall with scratches |
| C9 | **Cartoon** | absurdly deep, jagged or spiky |
| C10 | **Scale marker** | uniform pebbles, regular bedding or same-size blocks that announce a size |
| C11 | **Ruled edge** | a straight top edge or a straight foot line |
| C12 | **Sawn end** | a flat cut face at the tile edge |
| C13 | **Unprintable** | shelves overhanging past 45°, slot cracks, needles, islands |

**Pre-paint test:** the bare grey print reads as a heroic cliff.

---

## 4. Acceptance scenes

| Scene | Content | Passes when |
|---|---|---|
| **S1: face study** | the cliff face only, on a flat base | §2.4 holds; judged first, before top, foot or sweeps. |
| **S2: full tile** | 2 × 2, seed 1, with top and foot | All of §2 holds; no C-mode is visible. |
| **S3: seed sweep** | S2 at seeds 1–6 | At least 5 of 6 pass; none looks like another. |
| **S4: long run** | 3 tiles of different seeds side by side | One continuous cliff; no visible joints (REQ-RUN-3). |
| **S5: print** | S2 on FDM, primed, drybrushed | Fissures stay dark, faces catch the highlight, nothing broke or needed support. |

---

## 5. Open questions

1. **Where the face stands on the tile.**  Proposed: the face line at about
   40% of the depth from the front, giving a foot strip of ≈ 25 mm and a
   plateau of ≈ 40 mm.  A narrower foot reads more like a wall section;
   a wider one gives more room for scree.
2. **Plateau shape.**  Flat with the dirt texture, or gently rolling (a
   little of grasslab's hills)?
3. **Standard end section (REQ-RUN-3).**  A fixed section for every seed
   means the ends always match; the price is that every tile's ends look
   alike for the last ≈ 5 mm.
4. **Reuse.**  The face could be built from the shared stone primitives
   (`src/dharmatiles/stone/`, as the fieldstone walls do) or from scratch.
   That is a design decision, settled in DESIGN.md.
