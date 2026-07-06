# Walls: Commercial Sets Analysis (2026-07-04)

Detailed analysis of every wall-bearing reference system for the walls
campaign, requested by Shawn ("Can you make a detailed analysis of all
these different sets?").  Sources:

- **DungeonBlocks** — the full 15-set commercial library at
  `~/projects/dungeonblocks/` (the compatibility target).  Measured with
  trimesh; rendered with `src/extras/stl_render.py`.
- **Hirst Arts** — mold PDFs (m50, m40, m24 cryptstone, m70 fieldstone)
  saved in `hirst-arts/`, plus the painting tutorial.
- **Fat Dragon Games** — DRAGONLOCK dungeon (fdg0160) and caverns
  (fdg0170) starter-set pages.
- **The Dragon's Rest** — "HQ01 TDRQ Modular Board Foundation" and
  "AP006 High Ground 01" catalog PDFs (`~/Downloads/`).

Companion docs: `README.md` (photo references + wall vocabulary),
`../../meta/history/2026-07-04-walls-greenfield-requirements.md`
(baseline + interview).

---

## 1. DungeonBlocks — measured compatibility standards

These numbers are measured from the commercial STLs and are the
hard targets ("compatible by default, configurable for freedom"):

| Quantity | Value | Source piece |
|---|---|---|
| Square grid | 35 × 35 mm | everywhere |
| Standard wall height | ≈ 49.7 mm total piece bbox; the slab rises **≈ 24.4 mm above the piece's own floor** (floor plateau ≈ 25.3, tall socket base) | `RR-095-Wall`, `UD-001-Wall` |
| Tall wall height | ≈ 89 mm bbox; slab ≈ 62 mm above its floor | `RR-103-Tall Wall` |
| Wall slab thickness | ≈ 7 mm, aligned flush to one tile edge | `RR-095-Wall` |
| Rampart walkway level | ≈ 35 mm | `RR-086-Rampart Straight` |
| XL block plan | 105 × 105 mm (3×3 squares) | sets 11, 14, 15 |
| XL height system | Low / Medium (≈67 mm) / High folders | sets 11, 14, 15 |
| Mesh budget | 320–660k faces per piece | across sets |

## 1b. Ground surface heights (measured 2026-07-05)

Upward-face area histograms over representative pieces, heights above
the piece bottom.  Ground pieces use the short socket base (≈ 10.9 mm
= 5.7 peg + 5.2 flare, matching our `BaseConfig`), so the dharmatiles
equivalent is *(official − 10.9)* above our terrain datum:

| Surface | Official (above piece bottom) | Above our datum | Source pieces |
|---|---|---|---|
| Water surface | 12.8 – 13.3 | 1.9 – 2.4 | UD-074, LC-092, PS-187 |
| Outdoor dirt | 14.8 – 15.3 | 3.9 – 4.4 | RR-011/012 |
| Grass | 15.3 – 16.8 | 4.4 – 5.9 | RR-015/016 |
| Outdoor stone path | 16.3 – 17.3 | 5.4 – 6.4 | MN-009/010 |
| **Interior stone floor** | **17.8 – 18.8** | **6.9 – 7.9** | UD-057, TS-019 |
| Themed grounds (graveyard, swamp) | 18.3 – 19.8 | 7.4 – 8.9 | HG-003, CS-102 |
| Rock ground (outcrop) | ≈ 20.3 | ≈ 9.4 | RR-078 |

dharmatiles calibration: grass (~5.6) and soil (~4.9) already sit in
the official bands; water (~3.0) is close; `StoneFloor` was raised to
`top_mm = 7.4` (dead-centre interior floor) — use `top_mm ≈ 6.0` for
an outdoor path.  Wall pieces use the TALL socket base (16.6; cavity
ceiling measured at 16.75 on UD-001) and their integrated floor sits
at ≈ 25.3; the wall slab itself rises only ≈ 24.4 mm above that floor
— our old `height_mm = 49.7` default (measured from the seat) was
roughly 2× the official VISIBLE wall height; fixed 2026-07-05:
default is now 29.6 (top ≈ 33 above datum, matching the official wall
top), tall ≈ 68.8.

Structural idiom: a DungeonBlocks "wall" is a **floor square with a
wall slab rising from one edge** — wall and floor are one printed
piece.  Corners are the same square with two adjacent edge slabs.  The
face count matters: these are *heavily sculpted* surfaces, not flat
plates with a bump map.  That is the quality bar.

## 2. DungeonBlocks — per-set findings

Rendered representatives are in the session scratchpad (`db-*.png`);
family assignments below.

| # | Set | Wall treatment | Family |
|---|---|---|---|
| 01 | Ultimate Dungeon | Large irregular cut-stone blocks, uneven course heights, chipped/rounded arrises. The core "dungeon" read. | irregular cut stone |
| 02 | Toxic Sewer | Small coursed bricks, regular bond, eroded joints; pipes and grates punctuate runs. | coursed brick |
| 03 | Lost Cave | Fully organic stratified rock; horizontal bedding reads as "courses"; stalagmites at the foot; wall mass ≈16 mm, no flat faces anywhere. | organic cave rock |
| 04/07 | Medieval Town 1+2 | Timber-frame panels over a coursed stone base course; plaster infill between timbers. Composite of two families. | timber-frame + stone |
| 05 | Haunted Graveyard | **Low ruin walls**: 1–2 courses of large rounded blocks, ragged top line made of *missing blocks* (crenellation-by-decay), cracked flagstone floor. | ruin / low wall |
| 06 | Infinite Spaceship | Smooth panel walls with greebles — out of masonry scope, but proves the slab-on-edge idiom is texture-agnostic. | (panels) |
| 08 | Rampart & Ruins | The wall-standard set: standard + tall walls, crenellated parapets, 35 mm walkway, ruin variants. Same irregular cut stone as set 01. | irregular cut stone |
| 09 | Egyptian Temple | Smooth dressed ashlar, tight joints, engraved glyph band as a mid-wall frieze. Relief is *engraved into* flat faces rather than per-block. | smooth ashlar + engraving |
| 10 | Creepy Swamp | Stone walls being eaten by vegetation — moss clumps and creepers overriding the block grid. | cut stone + vegetation |
| 11 | Majestic Highlands (XL) | Natural cliff faces: multi-thickness bedding strata + vertical joint fractures, grassy vegetated tops. 105 mm blocks. | natural cliff |
| 12 | Pirate Ship | Wood plank construction, plank runs + framing. | wood plank |
| 13 | Dungeon Extension 1 | Tall irregular block wall; **crack network runs *across* block boundaries** — cracks are an independent overlay on the block grid, exactly like our stone crack lofts. | irregular cut stone |
| 14 | Colossal Dungeon (XL) | Hybrid: cut-block masonry transitioning into raw rock within one piece — masonry "built into" the mountain. | cut stone / cliff hybrid |
| 15 | Mythic Northlands | XL cliff family under snow/ice treatment; same strata skeleton as set 11. | natural cliff (snow) |

**Cross-set observations:**

1. Every masonry set models **individual stones as proud volumes** with
   deep joints; nothing is a flat face with shallow displacement.
2. Course lines are horizontal in every family, but course *height*
   varies within a wall (sets 01, 13) — the grid is broken by height
   variety, spalls, and missing blocks (set 05).
3. Ornament rides on top of structure as an independent layer: cracks
   crossing blocks (13), glyph friezes (09), vegetation (10), snow (15).
4. Tops are always designed: flat cap course, crenellation, walkway, or
   ragged ruin — never a raw extrusion cut.

## 3. Hirst Arts — the casting-block decomposition

Hirst's whole system is a **½" (12.7 mm) module**: every block is ½"
tall; lengths are 1", ¾", ½"; walls are usually ½" thick.  Wall = blocks
stacked in running bond, so joints land on a strict grid — the texture
variety lives *inside each block face*.

| Mold | Family | Texture mechanism |
|---|---|---|
| m50 "chipped stone" (Shawn's callout) | irregular cut stone | Dense small angular **chip facets** at random orientations covering each face; every block cast is unique; edges slightly irregular. Reads as tool-worked stone. Accessories: arrow slits, arches, demon-face sconces. |
| m40 basic blocks | rough-hewn block | All-over rough tumbled texture, milder than m50; the "plain" wall filler. |
| m24 cryptstone | small-block coursed | Two mini-courses per ½" strip; worn faces, occasional crack. The sheet's assembled+painted wall photo is a **perfect target read**: horizontal courses, deep dark joints, per-block tone variety, cracked blocks breaking the grid. |
| m70 fieldstone | fieldstone/drystone | Cast **strips** of random rounded stones with very deep joints; stone sizes vary inside a strip; corners get dedicated quoin pieces; arches built of fieldstone voussoirs. Coursing is weak (strip boundaries only). |

**Painting page (painting1.html):** dark grey base "slopped" on so it
runs into every crevice, then medium and light drybrush passes with
short strokes.  The technique *depends on* negative-relief joints and
per-block micro-relief — flat faces get nothing from it.  This
converts our "paint-catching relief" requirement into a mechanism:
joints must be deep enough to hold the dark base, and block faces must
have enough micro-relief to catch two drybrush tones.

## 4. Fat Dragon DRAGONLOCK

28 mm scale, 2"×2" (50.8 mm) floor tiles, **walls as separate pieces**
that attach to floor edges with proprietary "Dragonbite" clips —
opposite decomposition from DungeonBlocks' integrated wall+floor
squares.  Dungeon starter (fdg0160): 6 straight walls, 2 corners,
pillar, doorway with movable door, stairs.  Caverns starter (fdg0170):
38 organic cave/stalagmite pieces — the same "cave walls are geology,
not masonry" statement as DungeonBlocks set 03.  Design takeaway: the
wall/floor split is a real alternative, but our region-footprint model
already subsumes it (the wall body is a separate solid; whether it
prints attached to a tile is our choice, and DB-compat says attached).

## 5. The Dragon's Rest

**HQ01 (Dragon's Rest Quest foundation):** HeroQuest-idiom board.
Floors are 1×4 / 1×5 flagstone strips (cracks, drains, grates breaking
the grid) on pallet-style bases; **walls are fully loose low pieces**
placed on the floor plan: 2–3 courses of chunky, strongly rounded,
individually modeled blocks; ragged stepped top course; occasional
crack; corner pieces interlock like quoins; window arches and
door-frame arches built block-by-block (printed with supports).  This
is the strongest available example of "the wall is a stack of discrete
stone volumes" — literally what our faceted-stone primitive produces.

**AP006 High Ground:** Shawn's founding sketch as a commercial
product.  Each "chunk" (A–O) is a **cliff wall on a modular floor
strip**: organic rock face rising from the base, ragged vegetated top,
integrated props (trees, bushes, ladders, wooden stairs, mine-shaft
timbering).  Also: ragged edge-grass strips, a rock-rimmed 2×2 hole,
a 60 mm pit and slope-rubble pieces for terraced height, and plank
bridges to span between high ground.  Height terracing at 60 mm is
their "level" unit.  Design takeaways: (a) the wall-as-region-footprint
model handles cliffs and masonry identically; (b) ragged silhouette +
vegetated top is what sells the natural-cliff family; (c) props are a
separate later layer, not part of the wall body.

## 6. Consolidated texture-family taxonomy

All families found across all sources, per Shawn's answer 2 ("do
analyses of all of the different texture families that you've found").
Ordered by how central they are to the DB-compatible dungeon table:

| Family | Seen in | Construction read | Key by-construction features |
|---|---|---|---|
| **1. Irregular cut stone** ("chipped block") | DB 01/08/13, Hirst m50/m40, Dragon's Rest walls | Quarried blocks, hand-dressed, laid in uneven courses | Per-block proud volumes; varied course heights & block lengths; chip-facet face texture; rounded arrises; crack overlay crossing blocks; flat-cap top showing block tops |
| **2. Small-block coursed / brick** | DB 02, Hirst m24, brick photo ref | Regular bond of small units, mortar joints | Strict horizontal courses; running bond; eroded mortar = negative relief; spalled/missing units break the grid; visible brick tops on the cap |
| **3. Fieldstone / drystone** | Hirst m70, drystone photo ref | Uncut stones packed with deep voids, no mortar | Rounded/angular stone mix, big size variety, deep shadow joints, throughstones, pinning stones; weak coursing; cap = larger flat-topped stones |
| **4. Smooth ashlar + engraving** | DB 09, Krak photo ref | Sawn dressed stone, tight joints | Near-flat faces, thin joint lines, engraved ornament bands; battered (sloped) bases on fortress massing |
| **5. Organic cave rock** | DB 03, Fat Dragon caverns | Geology, not construction | Horizontal bedding strata as pseudo-courses; no flat faces; stalagmites/flowstone at the foot; wall mass thick (~16 mm) |
| **6. Natural cliff / high ground** | DB 11/14/15, AP006, quarry photo ref | Exposed bedrock face | Multi-thickness strata + vertical joint fractures; ragged vegetated top (grass system!); rubble benches; terraced heights |
| **7. Timber-frame + stone base** | DB 04/07 | Framed building wall | Composite: stone base course + timber grid + plaster infill. Later gen. |
| **8. Wood plank / palisade** | DB 12, Dragon's Rest planks | Carpentry | Plank runs, framing, rope lashings. Later gen. |

Families 1–3 are pure per-unit masonry and map directly onto the
shipped faceted-stone pipeline (hull stones, blur-remesh, broadband
relief, crack lofts).  Family 6 additionally reuses the grass system
for tops.  Families 4/5 need face-level rather than unit-level
texture.  7/8 are explicitly out of gen-1 scope.

## 7. Interview answers → gen-1 constraints (Shawn, 2026-07-04)

1. **Compatibility:** DungeonBlocks-compatible **by default**
   (35 mm grid, ≈50/89 mm heights, ≈7 mm edge-aligned slab), every
   dimension configurable for freedom.
2. **Texture families:** analyze all (done above); gen-1 build order
   to be decided in the requirements doc.
3. **Tops:** flat cap, but **textured as the top of the wall texture**
   — you see the tops of bricks/blocks/stones, not a smooth plate and
   not soil.
4. **Tile seams:** plain **butt-join** — no seamless course matching
   across tiles; a clean vertical cut at the tile edge is correct
   (matches how DungeonBlocks pieces butt anyway).
5. **Gen-1 scope:** **straight runs and corners on flat ground.**
   Ruins, gates/doors/windows/arches, towers, slope-following, and
   interior rooms are later.

## 8. Design implications carried forward

- The wall body is a **separate solid** keyed to a region/boundary
  footprint (heightmap faces are untexturable — baseline probe).
- Per-unit stone modeling is non-negotiable for families 1–3; the
  faceted-stone primitive is the unit generator, walls add *coursed
  placement* (a masonry layout solver: courses → blocks → joints).
- Joint depth and per-block micro-relief are requirements with a
  painting-derived acceptance test (dark wash must pool, two drybrush
  tones must catch).
- The block grid must be broken by construction: course-height
  variety, block-length variety, spalls/missing units, crack overlay
  crossing blocks, ragged-but-capped top course.
- Corners are first-class (quoin interlock reads at every corner in
  every reference), and gen-1 scope includes them.
- Mesh budget: DB pieces run 320–660k faces; our walls can afford
  real per-stone geometry.
