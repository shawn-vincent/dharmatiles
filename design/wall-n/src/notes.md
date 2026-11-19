# Wall N notes (running)
Target: reference N (ramp of long slabs, bedding ~40 deg rising to the back right). No controls, no gate.
Generator: gen.js (O's faces()/hew() reused; new build()). Stages: volume, beds, ends, push, chips, hew.
Words used: volume = box + slope plane + flat cap (top is a plane); cleave A = beds dip 40 facing 240 spacing 3.6-7.2; cleave B = ends across the dip (lean +-12 deg), spacing 13-24, carry .12; cleave C = along strike 9-16; setback per slab (0-3.8) plus per cell (0-2.2) along the slope normal; hew spacing 5, push .75.
Findings, biggest first:
1. Per-slab setback along the free face's normal (shared by the slab, plus a smaller per-cell jitter) makes the stepped slope and long step lines parallel to bedding. Per-cell only reads as loose blocks.
2. Push along y (O's way) makes slabs stick out horizontally against the bedding: wrong. Along the slope normal or the bed normal both read as slabs. Pushing bed planes along n_b only swells a cell into its neighbour (no visible step) -> setback follows the FREE FACE normal.
3. Camera: az 42, the view's left is 222 deg; bedding/slope facing 240-250 puts slab faces turned up and to the left as in N.
4. carry: 1 gives a grid (reads as a tilted brick wall); 0-0.15 reads as broken slabs. N wants low carry.
5. Square slab ends look like bricks; ends leaning +-12 deg off square read chipped. Over 25 deg gives shards. One cross set with a lean range was enough: no second, steeper set needed (answer to question 2: no).
6. Stepped silhouette = bed planes meeting the slope plane (slope 50 deg vs bed 40 deg), plus per-slab setback. Beds alone give only a faint step.
7. hew from O fits with spacing 5 and push .75 (O: 4.4/.95): thinner slabs and narrower faces want a coarser spacing and smaller push or the facets read as noise.
Still off: slabs too fat and few; no gravel; shard tips at the crest; no colour.

Round 2: crest = a top plane z=5+1.2(x-3) (cells cut by it) with a steep 84 deg front plane and a cut right end; this fixed the pyramid. Bed spacing 2.6-5; foot = 12 small blocks of the same rock; tone .30 base with a dark core 6.5 mm behind the faces so joints read dark; hew spacing follows the face's narrow side. 400 px: iframe at 400 has scrollWidth 400, no overflow (earlier overflow was headless window minimum width). Light: CDP Emulation.setEmulatedMedia, see cdp.mjs.

Round 3: coordinator's reading taken: the free face IS the bed plane (dip slope, 52 deg, facing 215). Beds 2-3 mm in the outer 28 mm, core behind. Each slab ends at its own plane along the crest direction (normal = strike), a drawn distance back; random per slab (rand) reads as well as monotone drop. Cross cuts 22-36 mm, lean 12, carry .12. Hew only on faces wider than 3.6 mm. Biggest lever by far: free face parallel to the beds + per-slab edge. Round 2's 84 deg front plane cutting across the beds gave fat slab ends. No new word: top per cell + cleave dip/facing/spacing.

Round 4: crest 13->43 mm (top plane through (3,10,13), slope .94); slabs 5.2 mm; cross cuts 9-14; innermost slab full width (no see-through); drop range up to n*2.4*.7. Thicker slabs + cross joints made it read as blocky shingles instead of blades.

Round 5: the hole under the ramp is shingle geometry: slabs recede inward by their thickness, so where outer slabs end early the ground shows between the face foot and the innermost slab. Fix: the draw for slab edges is 0..14 mm (not 0..n*drop*.7, i.e. mostly longer) and the innermost slab is full width. An apron box of the same rock, and moving the foot line along x, both gave a boxy front wall: rejected. Brightened tone (.44) so facets read against dark joints.

Round 6: chips 0-2 per cell, normals biased up (bright upturned facets), depth .8-1.9; more (1-3, up to 3.2 mm) destroyed the ramp. Foot blocks (14, same rock, 3-6 mm) laid along the wall's foot not scattered over the plate. Still missing: ground texture, crevice AO, the wider right end layering.

Round 8 (18 Nov, fracture round): the model changed from shingled slabs to ONE MASS. Free face = partition into convex cells recursively (3 levels: spacing 6.5/4/2.6 strike, x2.4 along dip; offset 1.5*1.9/.8/.34 mm; tilt gradients), each cell a prism from a common base (dOut-40) to its own top plane, so no gaps. Crack that dies: one side's top tilts along the crack so the step shrinks to zero. Persistence: a crack at level L lives only inside its parent cell. End of wedge: per-cell end plane (strike normal) drawn at each level. Corner chips: vertex plane along averaged normal, tilt<=25 deg, depth .12-.3 of narrow side, 30% of top corners. Pockets: level-3 cells in a stretched value-noise field lowered .2-1.5 mm. hew only on rubble now.
Camera moved to az 2 (the old az 42 viewed the slope at grazing angle; the wedge's steps need risers facing the camera).
Seen in the reference: ledges 4-6 mm wide, cracks end inside blocks, offsets ~ 1/4 of ledge width at each level, facets are fewer and blunter than mine at the finest level.
Pitfall: shot.sh does not rebuild test.html; run build.py (comp.sh does).

Round 11, the sculpting sheet (n-o-sculpting-sheet.jpg, N half; 25 mm tile, x1.4 for dmm). Sheet vs v10:
| quantity | sheet (x1.4 where mm) | v10 | action |
| overall slope | 32-38 deg (side section X) | dip 60 facing 240: section along x = atan(tan60*|cos240|)=41 deg, along y 56 | kept: photo silhouette wins; dip 55 (35 in section) made the wedge too low vs the photo |
| layers | 3-5, thickness base .38, .28, .22, cap .12 of the face's width | random strips 5.5 wide | 4 layers in that ratio across the ledges (ratio followed; the sheet) |
| face width | .15-.35 H = 5-12 dmm | 5.5 | layers ~8 wide |
| step height | .10-.30 H | ~2 dmm | layer offsets +-0.9, L1 +-1.1 (smaller than sheet; photo risers read as 1-3 mm: photo followed) |
| fracture spacing | .08-.20 H = 3-7 dmm | L2 3.6x9 | kept |
| edge bevel | .5-2 mm = .7-2.8 dmm | chips .12-.3 of narrow side (<=1) | chips 30%, .15-.35 |
| groove | width .7-2.8 dmm, depth .4-1.7 | none (joints 0) | 35% of leaf cells get a groove strip .7-2.2 wide, .4-1.7 deep, along the ledge 60%, a lowered cell (never a gap) |
| erosion pits | .5-2 mm clustered | pockets .6 | pockets .8 (clustered noise, cells lowered) |
Biggest gaps: layer ratio (random vs .38/.28/.22/.12), grooves (absent), edge bevels (too small).

Round 12 (18 Nov, unpainted panel + sheet 2 + family guide). Panel crop: ref-N-unpainted.jpg = on-squares-a-p-unpainted.jpg at (316,942) size 308, resized to 400 (matched against the painted sheet, mean error 6.7). Lighting: neutral grey, hemisphere .8 + one directional from above-front + cavity AO baked per triangle (BVH, 24 rays, 5 mm); ?noao=1 turns it off. v10 looked fair in it (dark slits gone, but far too many shards).
| sheet 2 / guide | value | v10 | v12 |
| main slope plane | 30-35 (sheet), 25-60 typical (guide) | dip 60 (41 in section) | dip 52, panel followed |
| diagonal cleavage | 30-60, about 45 | ledge line 20 deg off dip | kept (th=20) |
| faces | elongated 2.5:1 to 5:1 | 5.5x24 = 4.4:1 but tiny | 9x36 = 4:1, large |
| vertical cuts | few | many cross cracks | l1 36 -> fewer |
| ledges/shelves | 0-15, narrow | none | level-1 offsets 3dmm, sloped tops only |
| detail budget | form 70-80, structure 15-25, detail 5-10 | detail-dominated | L3 on 50% of cells; edge bevels 30% |
Guide numbers used: angle families (slope 25-60, diagonal 45), detail budget, "chips on exposed edges", "avoid dense noisy texture / uniform pitting". Not used: ledge depth .10-.40 D and setback .05-.30 D (setback left at the old values; to check).

Round 13 (v13, 18 Nov). v12 read as quartz. Changes: lean .4x, dying-crack tilt .04, diag 0/0/.08, cross cracks 25% of cracks, along-slab cracks step one way (sg=-1), min piece 3.2/2/1.5, spacing 12x40 / 8x18 / 5x9 (spar .5), eased lips on 85% of top edges (28-52 deg, .5-1.1 mm, <= .18 of edge length), corner chips 30%, grooves (strip cells .25-.5 wide, .3-.6 deep, parallel to the slab, 55% of cells wider than 4), foot = 11 flat blocks with chamfers (no hew). Hatch: end blocks (fam 2) overlapped crest blocks (fam 1); end blocks now stop at dcr-10 and reach 4.5 into x. Pitfall: shot.sh does not rebuild; cat head+tail into gen.js and run build.py, or comp.sh. Debug: ?famdbg=1, ?nofam=N, ?noao=1.
