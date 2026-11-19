# Wall O notes (running)
Target: reference O. Direction from Shawn: no controls, no gate, match the look, algorithms that generalise.
Generator (gen.js): convex blocks as half-space lists; faces by clipping; flat shading in three.js.
Stages: mass (3 boxes, stepped height + depth) -> courses (horizontal beds 11-16 mm) -> joints (some through-running x, others staggered >=3.2 mm) -> depth (per-block push back 0..7.5 mm; skyline blocks vary in height) -> cleave (1-2 planes tilted <=0.3 off each face, tilted tops, side cracks) -> chips (1-3 corner/edge bites, 0.9-2.8 mm).
v1 findings:
- Stepping depth per block + level beds + dark joint gap alone already reads as O's massing. That is the strongest lever.
- Too many chips/tilts (>4 per block, tilt .5) turns blocks into rounded boulders again: the look needs FEW, LARGE planar cuts and square vertical edges.
- A convex block = a list of half-spaces is trivially generative: any new reference only needs different plane distributions.
- Bug found: a backing 'core' box in front of recessed blocks hides them; keep the core behind max y.
v2: chips cut to 0-2 per block, 0.7-1.9 mm; face tilts <=0.2; chisel facets (0.2-0.7 mm, tilt .13) added; per-mass backing cores; right mass pulled forward as a foot stair. Crisp square blocks need SMALL tilts and shallow bites; pillows came back above ~0.3 tilt or 3+ big chips.
Still off: faces are smooth (O is hewn, many tiny facets); wall fills ~half of O's visual weight; no left return face as in O; side cleave planes sometimes leave long wedges.
Carries: mass boxes at stepped height/depth, courses+through-joints, per-block depth push, planar cleave, shallow chisel facets, edge bites (see page table).
Lever list the look really needed: block depth push (0-7.5mm), course height (11-16), through-joint probability, tilt of cleave planes (<=.2), bite depth (<2mm).
Round 3 (composites comp1..comp6.png; fc.png was stale from a winding/core-bug build):
- Biggest single lever for "rock": RELIEF FACETS (stage 7 'hew'): fill each face polygon with jittered points ~3.4mm apart, boundary kept flat, interior pushed +-0.6mm along normal, Delaunay. Square block edges stay, faces read hewn. Clip-plane micro-cuts (many shallow half-spaces) always rounded the corners; do not use for texture.
- Camera: O is seen with the wall at back/right and the figure left-front; az +24, el 30, dist 150, fov 27 matched the composition. Wall must be nearly a cube-ish slab (about 35 wide x 50 high), 4 courses of 11-15 mm, blocks ~12 wide.
- Tone: cool blue-grey, low ambient, strong key light, dark base tone (.5) gives O's contrast.
Round 4-5: relief spacing 4.4 mm / amp .95 (cut faces .6) with per-facet tone .84-1.16 gave crisper chips than 3.4 mm; 1-3 corner/edge bites per block now fine because relief carries the texture. Small cap blocks (3.5-6.5 mm tall) on top courses break the skyline. Checked light mode and 400 px layout (fixed grid minmax(0,1fr)).
Order of what moved the look, biggest first: (1) relief facets; (2) per-block depth push + level beds + camera/composition; (3) dark cool tone with strong key; (4) larger blocks (3-4 courses of 11-15 mm, ~12 mm wide); (5) cap blocks. Chip planes and tilt planes were the least useful.
Not done: left return face, rough chipped vertical edges, crevice darkening (AO), ground.

## Pass 5: the sculpting sheet (measured against v12; sheet numbers x1.4 for our 35 dmm; H = wall height about 38 dmm, L = depth 35)
| Sheet (O) | Sheet in dmm | v12 | Gap |
|---|---|---|---|
| 4 layers .26/.26/.28/.20 H | 9.9/9.9/10.6/7.6 | 3 courses of 11.5-14.5 (.30-.38 H), top often shortened | courses too tall by a third; photo shows 3 courses, photo followed |
| footprint steps .20/.60/.20 of L; height steps .26/.52/.80 | tiers back to front | one slab, front depth varies 0-3.5 plus one 6-8 recess | biggest gap: no tiers stepping back with height |
| block w:h 1.6-2.4 | w 15-22 at h 9-10 | w 10-14 at h 12-14, about 1:1 | too narrow and square; photo is about 1.2:1, photo followed |
| block width .20-.50 H | 7.6-19 | 10-14 | in range |
| block height .20-.30 H | 7.6-11.4 | 11.5-14.5 | .30-.38 H, slightly tall |
| ledge depth .10-.35 H | 3.8-13 | 0-3.5, recess 6-8 | too shallow except the one recess |
| fracture spacing .08-.25 H | 3-9.5 | skin cracks 4-8 | in range |
| face angles 85-90 / 70-85 | | skin tilt 1-4 degrees: 86-89 | secondary faces (70-85) missing |
| edge bevels 2-6 mm at 25 -> 2.8-8.4 dmm | | 2-3.6 dmm | bevels about half the sheet's |
| groove width .5-2 mm at 25 -> .7-2.8 dmm, depth .3-1.2 -> .4-1.7 | | seam .1 dmm, no pits | no grooves or pits |
Tried (gen.v13-try.js): (1) layers .26/.26/.28/.20 with block width = 1.5-2.2 x height and tiers 1-3 dmm per course: four narrow rows, a rubble look, worse than v12 against the photo; reverted to the photo's three courses. (2) A micro level: groove seams of .25-.7 dmm on 30% of cracks and 1-2 pits per large cell (a convex pit piece set back a further .4-1.2, the cell cut into convex pieces around it): the subtraction makes many thin pieces and reads as shattered rubble; tamed (pits only in cells over 45 mm2) it was still worse than v12. Not published; v12 stays live.

## Pass 6 (v13): unpainted panel + family guide
Panel O in on-squares-a-p-unpainted.jpg is at (628,941) size 310 (ref-O-unpainted.jpg, 400 px). Light: light grey, hemisphere fill, one soft shadowed key, per-vertex AO from 14 rays over 9 mm (view.js bakeAO). Multi-light shadow-map AO failed (everything went dark), so AO is baked on the CPU.
v12 in the new light (c46/k46): smooth rounded stones, long diagonal cracks, no relief. v13: no diagonals, relief back at ~1/5 amplitude.
Sheet 2 / guide numbers: vertical faces 85-90 held (skin tilt 1-4 deg); cap/shelf planes 0-10 (tops tilt <=.14: held); diagonal striations removed; detail budget form 70-80 / structure 15-25 / surface 5-10 used to set relief low; ledge depth .10-.40 D and setback per tier .05-.30 D (D=17 dmm, so 0.9-5) tried as 1.2-2.2 per course and dropped (ragged right masses). Mass layering 2-4 tiers: we have 3. Pits/grooves not done.
Panel vs sheets: panel wins; photo's three courses kept.

## Pass 7 (v14): fat blocks, incised cracks, chamfers
Coordinator list followed in order. (1) Skin: 60% of faces get no split, else one; set back .1-.3, tilt .2-1.2 deg, chips cap 1 (was up to 3 cracks/face, 4 pieces). About 12 fat blocks + caps and foot. (2) Relief facets dropped. Cracks: polylines in the face plane, near-vertical (+-.22 rad, wobble .14), 1-5 per face by area, optional branch .5-.9 rad, V groove half-width .5 depth .42 on a 1 dmm / .2 dmm adaptive grid, faded near the face edge. Bug found: faces() stored cut as a boolean so the cleave facets that make up most fronts were skipped; cut now carries its tag (1 cleave, 2 chip, 3 chamfer) and cracks go on tag 0/1. (3) Chamfers: top-front always (drop 1-2), both top side edges always, both vertical front edges 80%. (4) Courses: right masses step +-1.2 per course, top course 1 further forward; v8's recess/overhang kept. (5) Back core slabs removed; camera az -22 el 40 dist 146. (6) pebbles not done.
Page: strip builds with AO, step pictures without (a build with cracks is about 70k triangles; the full page took 75 s in headless swiftshader, a few seconds on a GPU).
