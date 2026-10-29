# dirtlab — Plain Bare Earth: Requirements

*Revision 1, 2026-10-29 (matches code v1g).  Brief (Shawn): "Plain bare
earth.  No footprints, just dirt and pebbles.  Maybe some cracks, but
probably not even cracks, just texture and pebbles."  A standalone
experiment like `grasslab/` and `rocklab/`: it may import dharmatiles
primitives and rocklab read-only, never modifies `src/dharmatiles/`, and
writes to `dirtlab/out/`.*

---

## 1. What this is

One STL: **a 2 × 2 DungeonBlocks tile (70 × 70 mm) of bare earth**: dirt
texture and pebbles.  No footprints, grass, water or (by default) cracks.

### 1.1 Visual scales

| Scale | Size | What it does |
|---|---|---|
| **Undulation** | 4–10 mm wavelength, ≤ 1.6 mm rise | Gentle ground swell; flat plains between. |
| **Clods** | 0.35–1.5 mm radius, ≤ 0.45 mm high | Rounded, soft-edged, irregular soil crumbs, in patches; tamped smoother ground between. |
| **Grit** | ~0.45 mm, ±0.05 mm | Fine roughness and sparse sand grains. |
| **Pebbles** | 0.5–1.5 mm (many), 2–4 mm (some), 4.5–7.5 mm (very few) | Faceted or worn stones, half-sunk, loosely clustered. |

## 2. Requirements

### 2.1 Output and tile

- **REQ-OUT-1 [S]** A run writes `dirtlab/out/2x2-dirt-s<seed>.stl`
  (`-cracks` suffix when cracks are on).
- **REQ-OUT-2 [S]** One watertight body after an STL-style reload
  (float32, merge by position); the run raises otherwise.  Ground, slab,
  stones and DB socket base (`dharmatiles.bases.dungeonblocks`) are fused.
- **REQ-OUT-3 [S]** Deterministic per seed; build ≤ 10 s; STL ≤ 25 MB.
- **REQ-OUT-4 [S]** `--sheet --seeds …` writes a timestamped contact sheet
  `dirtlab/out/YYYY-MM-DD-HHMMSS-contact-sheet.png`: a row per seed, views
  painted / oblique / top / low / close, a caption under every image.

### 2.2 Height

- **REQ-H-1 [S]** The tile edge sits at exactly 4.0 mm above the datum
  (official DB dirt band) all round, with a 0.4 mm flat band, so adjacent
  tiles butt.
- **REQ-H-2 [S]** Interior ground ≈ 4.3–5.9 mm (median ≈ 4.5 mm).
- **REQ-H-3 [S]** Rises only: nothing inside dips below the edge level
  (grasslab REQ-HILL-8).  Texture rides on a 0.3 mm lift so it never
  clips flat.

### 2.3 Earth

- **REQ-E-1 [R] Multi-scale.**  All four scales of §1.1 read at once.
- **REQ-E-2 [R] Clods are crumbs, not blisters.**  Each is 1–3 lumpy
  lobes with a flattish, tilted top and a soft foot, partly sunk.
- **REQ-E-3 [R] Patchy, not uniform.**  Clod density follows a ~6 mm patch
  field: cloddy areas and tamped areas, never even noise everywhere.
- **REQ-E-4 [S] No cracks by default.**  `--cracks` adds a few fine
  meandering cracks (0.25 mm deep), still above the edge level.

### 2.4 Pebbles

- **REQ-P-1 [R] Sizes.**  ~50–75 small (0.5–1.5 mm) and 5–9 medium
  (2–4 mm) per square; 2–4 larger stones (4.5–7.5 mm) on the tile.
- **REQ-P-2 [R] Stone-like.**  Faceted (crisp planar facets, polygonal
  outline) or worn (rounded cap with a few flats); flattish, elongated up
  to 1.9:1.  Never round domes or discs (D2).
- **REQ-P-3 [R] Clustered loosely.**  4–7 soft clusters plus a sparse
  background, biased into low ground.  Never on a grid.
- **REQ-P-4 [S] Sunk.**  Pebbles bury 25–45% of their height and meet the
  soil through a soft fillet; medium ones get a soil heap at the foot.
  Larger stones sit ≥ 0.25 mm below the lowest ground under them, in a
  0.3 mm soil bank.  Nothing floats.

### 2.5 FDM (0.4 mm nozzle, supportless)

- **REQ-FDM-1 [S]** No overhang past 45°.  The earth and small/medium
  pebbles are one height field (cannot overhang); larger stones are
  rocklab rocks (convex, grounded), flattened and sunk.  The audit
  reports overhang area (n_z < −sin 48°).

## 3. Failure modes (named)

| # | Failure | Looks like |
|---|---|---|
| D1 | **Porridge** | uniform isotropic noise everywhere, no structure |
| D2 | **Bubbles** | round domes or discs, especially in clusters (v1, v3) |
| D3 | **Cells / bowls** | packed lumps forming a Voronoi-like cell pattern |
| D4 | **Rough concrete** | fine grit only, no clods or stones |
| D5 | **Teeth** | tall upright stones out of scale with the earth (v4) |
| D6 | **Pasted pebbles** | stones sitting on the surface, no bedding |
| D7 | **Freckles** | a regular sprinkle of identical dots (v7 grains) |

## 4. Acceptance

- **S1:** seed 1 reads as miniature bare earth with none of D1–D7.
- **S2:** seeds 1–3 differ in clod patches, pebble clusters and stones.
- **S3:** a print, primed and drybrushed: clod tops and pebble facets
  catch the highlight, the creases keep the base coat.

## 5. Out of scope

Footprints, tracks, puddles, grass, colour in the STL, integration as a
dharmatiles layer.

## Added 2026-10-29 (Shawn, after slicing)

- **REQ-D-TEX [S] Texture survives 0.2 mm layers everywhere.**  Within any
  1.5 mm window, local relief (max − min) is at least 0.2 mm, which is one
  layer, over at least 97% of the interior.  Measured by
  `scratchpad/texcheck.py`.  v1g failed this on 33% of the tile: flat
  "tamped" areas printed as smooth plains.
