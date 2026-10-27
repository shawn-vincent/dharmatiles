# rocklab — Faceted Rock Scatter: Requirements

*Written 2026-10-27.  Status: v1.*

*Visual target: `reference/approved-reference.png`, a grid of about 60
painted, chiselled miniature rocks.  This is a standalone experiment,
built the same way as `grasslab/`: it may import dharmatiles primitives,
never modifies `src/dharmatiles/`, and writes to `rocklab/out/`.*

---

## 1. What this is

One STL: **a standard DungeonBlocks tile with a flat surface and a handful
of faceted rocks scattered on it.**  There is no grass, soil texture, hills
or anything else.  The purpose is to get the rocks themselves right.

## 2. What the reference shows

| Property | Observation |
|---|---|
| **Silhouette** | A broad, flat base with a body that is domed to blocky.  The rock is widest at or near the ground and never mushrooms out above it. |
| **Faceting** | The whole surface is planar facets of **mixed sizes**: a few large faces define the mass, and many small chips break up the corners.  It reads as chiselled or knapped, not as a smooth boulder or a noise-displaced blob. |
| **Edges** | Facet edges are crisp ridges.  These are what catch the drybrush highlight (the light lines in the reference). |
| **Shape variety** | Domes, low slabs, tall wedges or pyramids, and blocky cuboids.  Footprints range from round to elongated (up to about 2:1).  No two rocks look alike. |
| **Clusters** | About a quarter of the items are groups of 2–4 stones of different sizes, leaning together and fused. |
| **Size** | Largest about 3× the smallest.  Small rocks also carry chips, at a proportionally coarser facet scale. |
| **Colour** | Blue-grey with lighter edge highlights.  This is paint, out of scope. |

## 3. Requirements

### 3.1 Output and tile

- **REQ-OUT-1 [S]** A run writes `rocklab/out/<cols>x<rows>-rocks-s<seed>.stl`,
  defaulting to 2 × 2 DB squares.
- **REQ-OUT-2 [S]** It is one watertight manifold body: rocks fused to
  the slab, slab fused to the DB socket base
  (`dharmatiles.bases.dungeonblocks`).
- **REQ-OUT-3 [S]** The surface is flat at **4.0 mm above the terrain
  datum**, inside the official DB dirt band of 14.8–15.3 mm above piece
  bottom.
- **REQ-OUT-4 [S]** It is deterministic per seed, builds in under 30 s,
  and the STL is under 25 MB.

### 3.2 Rock form

- **REQ-ROCK-1 [R] Faceted.**  Every rock is a polyhedron of planar facets,
  roughly 30–100 facets per rock, with sizes ranging from about 40%
  of the rock's width down to about 8%.
- **REQ-ROCK-2 [R] Crisp edges.**  Facet edges are sharp ridges, not
  blurred or rounded away.
- **REQ-ROCK-3 [R] Grounded silhouette.**  The rock is widest at its base
  and its cross-sections never grow going up.
- **REQ-ROCK-4 [R] Variety.**  Each rock draws its own:
  - footprint (aspect 1–2, any orientation);
  - height-to-width ratio (0.4–1.0);
  - profile family (dome, slab, wedge, block);
  - chip count.
  
  No two rocks on a tile are near-copies.
- **REQ-ROCK-5 [R] Clusters.**  About 25% of placements are clusters of
  2–4 stones.  Each stone in a cluster is 40–100% of the largest stone's
  size, and the stones overlap and are fused.
- **REQ-ROCK-6 [S] Sizes.**  Footprint width is 6–15 mm, skewed toward
  small, with cluster stones at least 4 mm; height is 2.5–9 mm.

- **REQ-ROCK-7 [R] Three scales of texture.**  *(Shawn, 2026-10-27:
  "different degrees of texture… the surfaces should be rough and the
  corners of these big things should be chipped.")*

  | Scale | Size | What it is |
  |---|---|---|
  | 1. Facets | 2–7 mm | the large planar faces that define the mass |
  | 2. Chips | 0.2–0.9 mm deep | small planar chips knocked off corners and bevelled along sharp edges |
  | 3. Grain | ~0.45 mm, ±0.12 mm | fine, clearly visible surface roughness everywhere |

  Scale 1 and 2 edges stay crisp ridges, because grain is halved near
  them.  The texture must never blur the big facets into a potato (R1) or
  into crumpled foil (R3).
  - **Rejected:** mid-scale "fracture scallops" (Worley bowls about
    1.5 mm across).  Shawn: "horrible".  Don't reintroduce them.

### 3.3 Scatter

- **REQ-SC-1 [S] A handful.**  A 2 × 2 tile carries 8–12 placements
  (rocks or clusters).
- **REQ-SC-2 [R] Natural spacing.**  Placement is not on a grid, and no
  two placements overlap unintentionally (minimum clearance 1.5 mm).
  There is enough open floor for miniature bases.
- **REQ-SC-3 [S] Rim.**  Every rock lies entirely inside the tile, at
  least 1 mm from the wall.  Nothing is sliced by the edge.
- **REQ-SC-4 [S] Seated.**  Each rock sinks 0.3 mm into the slab so it
  is fused, with no floating gap.

### 3.4 FDM (0.4 mm nozzle, base-down print)

- **REQ-FDM-1 [S]** No downward-facing face is steeper than 45°.  This is
  guaranteed by REQ-ROCK-3 plus chip planes that never point downward
  past the limit.
- **REQ-FDM-2 [S]** No facet edge or chip is thinner than about 0.6 mm;
  chips on tiny rocks are clamped.  Texture displacement (scale 3)
  is small relative to its wavelength (slope ≲ 20°), and it fades to
  zero at the base and on any downward face, so it cannot create
  overhangs.
- **REQ-FDM-3 [S]** An audit reports watertightness, body count and
  overhang area, and any violation is reported.

## 4. Failure modes (named)

| # | Failure | Looks like |
|---|---|---|
| R1 | **Potato** | a smooth, rounded blob with no facets |
| R2 | **Gem** | regular, symmetric faceting, like a cut crystal or a die |
| R3 | **Crumpled foil** | noisy micro-facets everywhere, with no large faces |
| R4 | **Clones** | rocks of the same shape at different scales |
| R5 | **Mushroom** | wider above the ground than at it (and unprintable) |
| R6 | **Pebble dust** | rocks too small to read at tabletop distance |
| R7 | **Floating** | a visible gap or seam between a rock and the floor |

## 5. Acceptance

- **S1:** the 2 × 2 tile at seed 1.  Rocks read as the reference's
  chiselled stones and none of R1–R7 is visible.
- **S2:** seeds 1–6.  Each looks different.
- **S3:** a print, primed and drybrushed.  Facet edges catch highlights.

## 6. Out of scope (later)

Integration as a dharmatiles layer (the existing `scatter/stones.py` is
a separate, smoother "aged" family), rocks on hills, interaction with
grass, colour, and stacked or overhanging boulder piles.
