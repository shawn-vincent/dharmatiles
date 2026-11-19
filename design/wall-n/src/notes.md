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
