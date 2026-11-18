# Proposal: the cliff's words (for review)

Goal: let a cliff in the tile language match 15 reference photos of rock walls (planar slab faces, columns, terraces, leaning spires, overhangs, free-standing walls), with algorithms that generalize to new references. No debris/scree (belongs to other elements). Printability is secondary here.

## Today (explorer "Stone and Courses", the prototype of the cliff words)
Cliff controls (explorer page words unless noted): tiles, wall, free.front/back/sides, depth, top (registry), crest.bumps, crest.w, band.depth, band.bumps, band.w, band.stay, width (registry), height (registry), bond (registry), recess, lip, sbw.bumps, sbw.w, shift, wander, rise, span, regularity, lean, tilt, planes.bumps, planes.w, wave, width-skew, height-skew, outl.s/bumps/core/w. Rock (per stone) stages: cuts (width breadth height dip facing patch bite aim), chunks (strata across uneven jitter carry halved setback tilt corners fall), chips, pockets, grain, wear.
Model: horizontal row planes (lean/tilt/wave), each row subdivided into stones by width/bond/shift; faces set back by recess/lip.

## Big ideas
1. Joint sets. A wall is a volume cut by families of near-parallel planes. Blocks are the cells. Bedding (horizontal) = courses; cross joints (vertical) = columns; a third oblique set = facets. One mechanism covers slab walls, columns, strata, running bond, leaning columns.
2. Depth per cell. Each cell of a set stands back from (or out past) its neighbour along the set's normal. Back-stepping bedding cells = terraces; negative = cantilevered ledge/lip; a cross set with setback = blocks at stepped depths; setback varies smoothly or per block.
3. Tops are planes. A block ends on a bedding plane, so the crest is stepped, flat-topped blocks; the crest's height follows `top` (an expression of position, or a weighted range).
4. Planar faces. A face is a cut plane, not a rounded stone. The same cleave, at fine spacing inside a stone, makes sharp cracks and chips. One verb at two scales.
5. Lean and convergence. A set's dip may be a range (wobble up the wall) and its facing may be a point (planes lean toward / fan from it): leaning spires, diagonal ramps.
The footprint (corner, run, spire, bay, free-standing wall) stays the job of `region`, as now.

## Proposed words
NEW: `cleave` (child node of `cliff`, zero, one or many; later also of `rock`): one family of near-parallel cut planes.
  properties (all weighted ranges / values as elsewhere):
  - `dip` angle from horizontal (0deg bedding, 90deg vertical; a range = wobble foot to crest). Reuses rock's page word.
  - `facing` which way the planes face in plan: an angle, or a point (kind decides: angle = direction, point = toward that point). Reuses rock's page word.
  - `spacing` distance between neighbouring planes, a weighted range. (replaces width and height on the cliff)
  - `wander` how far planes bend and drift (replaces wave, regularity, wander).
  - `carry` 0..1: how far a cut runs on through neighbouring cuts of other sets (1 = continuous grid, 0 = each cell cut alone, running bond between). Replaces bond/shift/rise/span on the cliff. Rock's page word `carry` in chunks; meaning is the same family.
  - `setback` weighted range: each cell stands back from the one before it along the normal; negative = out past (lip, cantilever). Rock's page word, widened. Replaces recess, lip, sbw.*, band.depth/bumps/w/stay.
  Correlation length of a weighted range along the wall/up: existing registry word `bumpSize`.
KEPT: `top` (crest: a weighted range or an expression of $x $y; replaces crest.bumps, crest.w), `joints` (width of the groove between blocks; registry), `bond` (kept only if `carry` proves unable to hold it: then `carry` is dropped instead; one of the two), `region`/`at=`, `rock` finishing words (chips, pockets, grain, wear).
RETIRED from the cliff: width, height, shift, wander(old), rise, span, regularity, lean, tilt, planes.bumps, planes.w, wave, width-skew, height-skew, recess, lip, sbw.bumps, sbw.w, crest.bumps, crest.w, band.depth, band.bumps, band.w, band.stay, free.front/back/sides (a free face is any face of the region not a seam: derived), depth (region), outl.s/bumps/core/w (the stone's outline is its own `cleave`, phase 3). About 30 words out, 1 new, 3-4 page words promoted to the registry.
Phase 3: rock's `cuts` and `chunks` (strata, across, uneven, jitter, carry, halved, tilt...) become `cleave` children of `rock`: a stone is cleaved by the same node.

## Examples
M skyline of columns:
cliff at=crag top="[34dmm..62dmm]" bumpSize=14dmm {
  cleave dip=90deg facing=front spacing="[10dmm..14dmm]" carry=1
  cleave dip=0deg spacing="[10dmm..16dmm]" carry=0 wander=0.4 setback="[-3dmm..0dmm]"
}
E terraces:
cliff at=crag top="$edge + 8dmm + 24dmm * $y / 35" {
  cleave dip=2deg facing=front spacing="[6.5dmm..10dmm]" carry=1 setback="[4dmm..7dmm]"
  cleave dip=90deg facing=front spacing="[12dmm..22dmm]" carry=0.3
}
P leaning columns (region = three summit pins):
cliff at=spires top="[56dmm..68dmm]" {
  cleave dip="[72deg..80deg]" facing="0.5,0.5" spacing="[4dmm..7dmm]" carry=1
  cleave dip=0deg spacing="[10dmm..14dmm]" carry=0
}
D oblique facets and a cap:
cliff at=crag top="[44dmm..56dmm]" {
  cleave dip=0deg spacing="[12dmm..18dmm]" carry=0.9 setback="[-4dmm..-2.5dmm]"
  cleave dip=60deg facing=30deg spacing="[8dmm..15dmm]" carry=0.5
  cleave dip=90deg facing=front spacing="[8dmm..15dmm]"
}
