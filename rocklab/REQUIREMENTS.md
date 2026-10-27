# rocklab — Faceted Rock Scatter: Requirements

*Revision 2, 2026-10-27 (matches code v1f).  Visual target:
`reference/approved-reference.png`, a grid of about 60 painted, chiselled
miniature rocks.  A standalone experiment built like `grasslab/`: it may
import dharmatiles primitives, never modifies `src/dharmatiles/`, and
writes to `rocklab/out/`.*

---

## 1. What this is

One STL: **a standard DungeonBlocks tile with a flat surface and a handful
of faceted rocks scattered on it.**  No grass, soil texture or hills; the
purpose is to get the rocks themselves right.  (`meadowlab/` puts them on
grassy hills.)

## 2. What the reference shows

| Property | Observation |
|---|---|
| **Silhouette** | A broad, flat base with a domed-to-blocky body, widest at or near the ground, never mushrooming above it. |
| **Faceting** | Planar facets of **mixed sizes** over the whole surface: a few large faces define the mass, many small ones break up the corners.  Chiselled or knapped, not a smooth boulder or a noise blob. |
| **Edges** | Crisp facet ridges that catch the drybrush highlight. |
| **Shape variety** | Domes, low slabs, wedges or pyramids, blocky cuboids.  Footprints round to elongated (up to about 2:1).  No two alike. |
| **Clusters** | About a quarter of the items are 2–4 stones of different sizes leaning together, fused. |
| **Size** | Largest about 3× the smallest; small rocks carry proportionally coarser facets. |
| **Colour** | Blue-grey with lighter edge highlights: paint, out of scope. |

## 3. Requirements

### 3.1 Output and tile

- **REQ-OUT-1 [S]** A run writes `rocklab/out/<cols>x<rows>-rocks-s<seed>.stl`,
  defaulting to 2 × 2 DB squares.
- **REQ-OUT-2 [S]** One watertight manifold body: rocks fused to the slab,
  slab fused to the DB socket base (`dharmatiles.bases.dungeonblocks`).
- **REQ-OUT-3 [S]** The surface is flat at **4.0 mm above the terrain
  datum**, inside the official DB dirt band (14.8–15.3 mm above piece
  bottom).
- **REQ-OUT-4 [S]** Deterministic per seed (byte-identical STL), builds in
  under 30 s, STL under 25 MB.

### 3.2 Rock form

- **REQ-ROCK-1 [R] Faceted.**  Every rock is a polyhedron of planar
  facets (roughly 30–165 per rock), sized from about 40% of the rock's
  width down to about 8%.
- **REQ-ROCK-2 [R] Crisp edges.**  Facet edges are sharp ridges, never
  blurred or rounded away.
- **REQ-ROCK-3 [R] Grounded silhouette.**  Widest at the base; cross-
  sections never grow going up.
- **REQ-ROCK-4 [R] Variety.**  Each rock draws its own footprint (aspect
  1–2, any orientation), height-to-width ratio (0.45–0.85; slabs × 0.6),
  profile family (dome, slab, wedge, block), lumpiness, facets and chips.
  No two rocks on a tile are near-copies.
- **REQ-ROCK-5 [R] Clusters.**  About 25% of placements are clusters of
  2–4 overlapping, fused stones, each 45–90% of the main stone's width.
- **REQ-ROCK-6 [S] Sizes.**  Width 6–15 mm skewed small; cluster stones
  ≥ 4 mm; height 2.5–9 mm.
- **REQ-ROCK-7 [R] Three scales of texture.**  *(Shawn: "different
  degrees of texture… the surfaces should be rough and the corners of
  these big things should be chipped.")*

  | Scale | Size | What it is |
  |---|---|---|
  | 1. Facets | 2–7 mm | the planar faces that define the mass |
  | 2. Chips | 0.2–0.9 mm deep | small planar chips off corners and bevels along sharp edges |
  | 3. Grain | ~0.45 mm, ±0.12 mm | fine, clearly visible roughness everywhere |

  Scale 1 and 2 ridges stay crisp (grain is halved near them).  Texture
  never blurs the facets into a potato (R1) or crumpled foil (R3).

### 3.3 Scatter

- **REQ-SC-1 [S] A handful.**  A 2 × 2 tile asks for 9–12 placements
  (rocks or clusters); one that finds no free spot is dropped.
- **REQ-SC-2 [R] Natural spacing.**  Not on a grid; no unintended overlap
  (≥ 1.5 mm clearance); open floor left for miniature bases.
- **REQ-SC-3 [S] Rim.**  Every rock lies inside the tile, ≥ 1 mm from the
  wall; nothing is sliced by the edge.
- **REQ-SC-4 [S] Seated.**  Rocks sink 0.3 mm into the slab: fused, no
  floating gap.

### 3.4 FDM (0.4 mm nozzle, base-down print)

- **REQ-FDM-1 [S]** No downward face steeper than 45°.  Guaranteed by
  REQ-ROCK-3 plus facet and chip planes that never point down past 30°
  (n_z ≥ −0.5).
- **REQ-FDM-2 [S]** Chip depths scale with √(D/15), so small rocks get
  proportionally small chips; grain is small (±0.12 mm) and fades to zero
  at the base and on any downward face, so it cannot create overhangs.
- **REQ-FDM-3 [S]** An audit reports watertightness, body count and
  overhang area (faces with n_z < −sin 48°); any violation is reported.

## 4. Failure modes (named)

| # | Failure | Looks like |
|---|---|---|
| R1 | **Potato** | a smooth, rounded blob with no facets |
| R2 | **Gem / dice** | regular, symmetric or uniformly sized faceting |
| R3 | **Crumpled foil** | noisy micro-facets everywhere, no large faces |
| R4 | **Clones** | rocks of the same shape at different scales |
| R5 | **Mushroom** | wider above the ground than at it (and unprintable) |
| R6 | **Pebble dust** | rocks too small to read at tabletop distance |
| R7 | **Floating** | a visible gap between a rock and the floor |
| R8 | **Scallops** | mid-scale fracture bowls (Worley, ~1.5 mm): "horrible" (v1d) |
| R9 | **Big flat faces** | a few oversized facets instead of a mosaic (v1c, fixed in v1f) |

## 5. Acceptance

- **S1:** the 2 × 2 tile at seed 1.  Rocks read as the reference's
  chiselled stones; none of R1–R9 is visible.
- **S2:** seeds 1–6.  Each looks different.
- **S3:** a print, primed and drybrushed.  Facet edges catch highlights.

## 6. Rejected approaches (keep the lessons)

- **Shave-the-extreme-vertex chip cuts** (v1a/b): converge to uniformly
  sized "dice" facets (R2) and removed 50–68% of the volume.  → One
  tangent-plane facet per blue-noise seed.
- **Fracture scallops** (v1d): Shawn: "horrible".  Don't reintroduce.
  → Turn up the fine grain instead.
- **Harder facet tilt** (22–28°, v1f trial): removed 40–60% of the volume
  and still left the big faces.  → Smaller facet spacing plus angle-only
  body lumpiness.
- **Flake scars**: proposed and declined by Shawn.
- **Uncapped chip rate**: with smaller facets there are many more
  corners; uncapped chips eroded rocks to about 60% of their volume.  →
  At most 8–16 chips per rock, each removing ≤ 3%.

## 7. Out of scope (later)

Integration as a dharmatiles layer (`scatter/stones.py` is a separate,
smoother "aged" family), colour, stacked or overhanging boulder piles.
Rocks on hills and among grass: see `meadowlab/`.
