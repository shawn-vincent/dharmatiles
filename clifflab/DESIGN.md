# clifflab — Heroic Cliff Run: Design

*Revision 2, 2026-10-30.  Implements `REQUIREMENTS.md` rev 1 with the
open questions settled as recommended: face line at 40% depth (foot strip
≈ 25 mm, plateau ≈ 40 mm), flat dirt plateau, a fixed standard end
section, face built from scratch (below).  Code: `clifflab/clifflab.py`,
`clifflab/contact_sheet.py`; section numbers match the stage banners.*

*Rev 2: Shawn (mid-build): "consider the original incoming requirements and
the reference photos.  I love those photos, especially the range.  We
should not build something that can't meet those needs."  Rev 1's face was
a depth map over ONE vertical plane: it could only ever make a straight
run.  Rev 2 puts the same face texture on the outline of any plan
footprint (§1.1), so corners (C), canyons (I/K), outcrops (A) and tiered
spires (E/P) are the same machinery; gen-1 still ships the run.*

---

## 1. Core idea: the face is a depth map on a footprint outline, printability is one pass

### 1.1 Bodies and forms

A cliff is a set of **bodies**.  Each has a plan **footprint** (polygon,
may extend past the tile), a **face line** along its edge (an open line
for a run's face, the closed ring for a free-standing body), a top height,
and an optional taper (inset per mm of height, for spires).  For every
(x, y) of the mesh grid: d = signed distance inside the footprint (shapely,
exact) and s = arc position of the nearest face-line point
(`line_locate_point`).  The face texture is a depth map D(s, z) per body
(below, with "y" read as "inset along the outward normal"): rock where
d ≥ D(s, z).  Because d is a true distance, the upward clamp on D bounds the
overhang angle exactly as on a plane.  Closed face lines wrap in s (noise,
Voronoi and fissure distances are periodic).

| Form | Bodies | Reference |
|---|---|---|
| `run` (gen-1) | one footprint behind a line across the tile | B, H, N |
| `corner` | an L-shaped footprint: an inside corner | C |
| `canyon` | two runs facing across a 20 mm gorge, different heights | I, K |
| `outcrop` | one lumpy closed blob, 14 mm high | A |
| `spire` | three stacked blobs (18, 32, 46 mm), tapered, the top one bare | E, P, F |

Still to come from the sheet: escarpments (D/O: a sloping top), stacked
overhanging slabs (L: limited to 45° undersides), pinnacle pairs (J),
ruins on top (G).  The machinery covers them; they need their footprints
and top shapes.

### 1.2 The face of one body

Tile coordinates: x along the run (0…70 mm), y into the tile (front y = 0,
back y = 70), z up.  The cliff face is a **depth map** F(x, z): the y of
the face at each point of the vertical plane.  Rock is behind it (y ≥ F).
Every face feature is a 2-D operation on F:

```
 base: face line y₀ + plan wobble(x) + batter·z
   + LARGE masses   (anisotropic warped Voronoi, 8–35 mm): per-mass plane (offset, tilt)
   + MEDIUM plates  (same generator, 3–9 mm), scaled by the activity field
   + LEDGES / SHELVES (short horizontal steps) + long DIAGONAL fractures
   + V-groove FISSURES at every cell boundary (depth by level, fading along the joint)
   → weathering: round convex edges only (F ← max(F, blur F)); recesses stay sharp
   + FINE: shattered plates and hairlines in hot spots, grain everywhere (≥ 0.15 mm)
   → END SECTION blend (x within 5 mm of either end → the fixed standard section)
   → 45° CLAMP, bottom-up per column: F(z) ≥ F(z − dz) − dz·tan 42°
```

**Why a depth map.**  The face's whole vocabulary (planes, steps,
fissures, ledges, undercuts) is relief *across* the face, and a depth map
makes it cheap 2-D work on a 0.18 mm grid.  It can't fold back on itself,
and that is exactly the FDM rule: the only overhang a depth map can make
is the face advancing as z rises (∂F/∂z < 0), and the upward clamp limits
that to 42° (3° margin).  So REQ-FDM-1 holds by construction, and every
undercut shelf comes out as a 45°-ish wedge.  Islands can't form: every
row of the face is attached to the row below.

**Solid.**  With G(x, y) the foot ground and P(x, y) the plateau top:

```
f(x, y, z) = max( G − z,  min( y − F(x, z),  P − z ) )      solid where f > 0
```

The terms are (near-)distances in mm, so marching cubes interpolates them
cleanly; y − F is linear in y, so the face is placed exactly in depth.
The solid is meshed on a 0.18 mm grid from just below the foot to above
the plateau, clipped to the tile box, decimated, then unioned with the
slab under it, the scree rocks and the DB base.

## 2. Stages

### 2.1 Frame

- Foot level z_f = 4.0 mm (DB dirt band).  Plateau level z_p = z_f +
  35 mm (REQ-OUT-3; fixed, so the end sections match).
- Face line y₀ = 28 mm (40% of 70).  Batter: the face leans back 4°
  (y grows 0.07 mm per mm of height).
- Plan wobble: 1-D smooth noise along x (σ 9 mm), scaled to ±3 mm; the
  large masses add ±2 mm more (REQ-RUN-1: about ±5 mm overall).

### 2.2 Activity field (REQ-ROCK-3: 60 / 30 / 10)

A smooth noise field on the face (σ 7 mm) plus a bump near major fissures
(fractures cluster at big joints), **rank-transformed** to a uniform u ∈
[0, 1].  Then u < 0.6 is *quiet* (medium cracks at 15% strength, no fine
shatter), 0.6–0.9 is *fractured* (medium at full strength, fine at 30%),
and u > 0.9 is *shattered* (everything at full strength).  Ranking makes
the split exact by area on every seed, and clustered rather than sprinkled.

### 2.3 Masses: anisotropic warped Voronoi at three levels

One generator, used at three sizes (the self-similar family, §1.1):

- **Seeds**: dart-thrown, each with a size s_i (large: 9–22 mm, skewed
  small, so widths vary ≥ 4×), a joint direction θ_i, and an aspect
  (large 2.4: tall).  θ_i: 80% within ±20° of vertical, 20% at 35–55°
  (diagonal masses).  Medium seeds inherit their parent mass's θ ± 10°.
- **Ownership**: argmin_i ‖(Δ·a_i, Δ·b_i / aspect)‖ + warp(s, z)·φ_i (an
  *unweighted* anisotropic Voronoi), with a_i ⊥ θ_i, b_i ∥ θ_i, and warp a
  smooth noise (σ 3 mm, ±0.07 × the level's mean cell size).  Size variety
  comes from the variable-radius seed spacing; the anisotropy (large 3.0,
  medium 2.2) gives vertical-dominant joints; the per-seed warp phase bends
  them.  Triple junctions are the branching.  (Rev 1 divided the distance
  by each seed's size: an Apollonius diagram, whose curved boundaries read
  as cobbles, C1/C6.)
- **Plane per cell**: offset (large N(0, 2.4) mm clipped ±5, medium
  N(0, 0.45)), tilt across (large ±28°, medium ±9°) and in height (large
  −1.5…+9°, medium −1.5…+6°: they barely lean forward; forward-leaning
  masses made big undersides, which B/H/N don't have).  Offset differences between neighbours are the
  "pronounced changes of plane"; proud cells are buttresses and shoulders.
- **Fissures**: V-grooves along every cell boundary, from the EDT of the
  boundary mask: depth·(1 − d/w)₊.  Large: depth 2–5 mm, half-width 0.9 mm
  (mouth ≈ 1.8 mm).  Medium: 0.6–1.4 mm deep, half-width 0.45.  Fine:
  0.25–0.45 mm deep, half-width 0.3 (mouth 0.6, REQ-FDM-3).  Depth is
  modulated along each joint by a smooth noise, so joints fade out and
  reappear (REQ-ROCK-2); medium and fine are also scaled by activity.

### 2.4 Ledges, shelves and long diagonals

- **Ledges** (4–8 per run): a short near-horizontal line (4–14 mm, ±10°)
  across which the face above steps back 0.6–1.6 mm, giving a lit shelf top.
  35% are **undercut shelves** instead: the face *below* steps back, and the
  45° clamp turns the step into a wedge underside.
- **Long diagonals** (1–3 per run): bent polylines at 30–60°, 15–40 mm long,
  carved as medium-depth V-grooves across several masses.

### 2.5 Weathering and fine detail (REQ-ROCK-5, REQ-FDM-4)

- **Convex-only rounding**: F ← max(F, G_σ(F)), σ 0.45 mm.  Behind-the-
  face is +y, so proud edges are local minima of F and rise to the blur,
  rounding them; fissures and inside corners are maxima and stay sharp.
- **Fine shatter**: the fine level (1.2–3 mm cells) in active areas only.
- **Grain**: noise at σ 0.5 mm, amplitude 0.16 mm (≥ the printable
  threshold), halved on proud edges; a few pits (r 0.3–0.6 mm, 0.2 mm
  deep) in shattered zones.  Then a light second rounding (σ 0.15 mm).

### 2.6 Ends, clamp, top edge

- **Standard end section** (REQ-RUN-3): within 5 mm of x = 0 and x = W, F
  blends (smoothstep) to F_end(z) = y₀ + batter + a fixed ledge profile
  (built from a fixed RNG seed, identical for every tile seed), and the
  top edge blends to z_p.  Any two tiles therefore meet with the same
  section.  *Deviation from REQ-RUN-3*: like every DB tile, the solid is
  cut at the tile boundary, so a lone tile's end shows the section as a
  flat cut; in a run it is hidden.
- **Clamp**: bottom-up per column, D(z) ≥ D(z − dz) − dz·tan 42°.  Then
  the undersides (where the clamp acted, mask blurred 0.6 mm) are replaced
  by the blur of the clamped surface (σ 1.0 mm along s, 0.35 mm in z) and
  clamped again.  Averaging can't steepen a slope, so the rule still
  holds; without it each underside was the edge profile above extruded
  down as vertical streaks (a "comb").
- **Top edge** (REQ-RUN-4): each large mass that reaches the top gets a
  top height z_p + U(−4, +1.5) mm, each medium plate adds U(−1.4, +0.7).  Major fissures notch the top as
  V-dips.  The rock top runs back 2–7 mm as a gently tilted surface, then
  dips at 45° under the plateau dirt.

### 2.7 Ground

- **Plateau** P = max(cap, rock top).  The dirt cap follows the rock's top
  line near the edge and eases to the plateau level over 6 mm behind the
  rock band, so lowered blocks and notches show in the silhouette (a fixed
  cap level hid them: a ruled edge, C11).  The rock top shows through near
  the edge (REQ-TOP-1).
- **Foot** G = z_f + dirt + **talus**: a low apron against the face base,
  up to 0.6–2.2 mm high, decaying over 3–7 mm, higher below major
  fissures.
- Dirt = `dirtlab.dirt_relief`, seeded from the tile seed.

### 2.8 Scree (REQ-FOOT-1)

25–45 rocklab faceted rocks per 70 mm of face, D = 2.5–8 mm skewed small
(no single size dominates: no scale marker), drawn low and blocky (block,
slab and wedge families, height 0.3–0.55 × width; rocklab's small domes
read as spikes).  Positions: face-line points weighted toward
major-fissure mouths, pushed out along the outward normal by 0.3·D +
Exp(3.5 mm), kept 3 mm off the tile rim and outside every body; dart-thrown with clearance.  Seated
0.3 mm into the ground at their spot.

### 2.9 Mesh, base, audits

- Marching cubes (skimage) on f, 0.18 mm grid, padded with air; trimesh;
  quadric decimation to `MESH_FACES` (1.6M); manifold ∩ tile box ∪ slab
  box ∪ scree ∪ DB base; keep the largest body; `stl_safe` (grasslab).
- **Audits printed**: heights, overhang area past 48°, slicer islands at
  0.2 mm layers (shapely, as used for the shore), the 60/30/10 split
  measured on the face, face-line range, timing.

## 3. Painting (contact sheet only; REQ-DIAG-2)

A simulated cheap-acrylic job, per vertex, then quantised to a palette:

- **Stone**: base coat dark warm grey; a black-brown wash in recesses
  (cavity = how far the vertex sits below its Laplacian-smoothed position
  after many iterations: ambient-occlusion-like); then **drybrush**
  layers by edge convexity (a few iterations) plus a bonus for upward
  facing, the way a dry brush catches raised edges and top surfaces; very
  occasional ochre staining (a low-frequency mask).
- **Dirt** (plateau and foot, up-facing ground): brown base, light
  drybrush on bumps.  **Tile walls and base**: flat dark.

## 4. Parameters

All module constants in `clifflab.py`, grouped by stage as above.  The
contact sheet varies seeds and a few presets (quiet, deep fissures,
big masses, diagonal) through a `preset` override dict.

## 5. Iteration log (2026-10-30)

Adversarial self-review against the spec and the reference sheet, on run
seed 1, painted and grey:

| Round | Seen | Change |
|---|---|---|
| 0 | 25% of the area "overhanging" | the marched chunk floated 0.3 mm above the slab (its flat bottom counted): slab raised into it |
| — | Shawn: the design must be able to reach the whole reference range | rev 2: bodies + footprints + face lines (§1.1); five forms |
| 1 | curved, cobble-like cell network (C1/C6); a flat wall with drawn-on cracks (C8); weak verticality; comb streaks under shelves; orange-peel quiet faces; washed-out paint, vertical stain stripes | unweighted Voronoi; mass offsets 2.4 mm σ (±5), tilt ±22°; aspect 3.0/2.2; gentler grain, fine shatter only where shattered; darker palette, 3-D stain patches |
| 2 | comb curtains under proud masses; ruled top edge (C11); spiky scree; camouflage drybrush | blurred clamp correction; cap follows the top line; low blocky scree; wider convexity + paint smoothing |
| 3 | curtains still striated; top line nearly flat | no forward lean; undersides = blur of the clamped surface; per-plate top heights |
| 4 | rectangular "E" frames from undercut ledges (C4-ish); masses a little soft vs B/H/N | ledges taper over 3 mm, bend, eased bands; mass tilt ±28° |

Remaining gaps against B/H/N, honestly: the masses are softer and less
prismatic than the references' crisp blocks; there are no chamfered block
tops; the drybrush preview is a heuristic.

## Rev 3: thin edge bands, low-profile ends (2026-10-30)

Shawn: "spectacular… the best cliffs I've ever seen", then three tweaks.
(1) Edge cliffs should take a small share of the tile (1 × 1-friendly),
hugging the back edge, depth going forward, a little lean out, a ground top.
(2) Ends: he looked at the Majestic Highlands cliff STLs (MH-117 High Wall
1, MH-120 High Angle, MH-123 High End: a lens-shaped band on one edge of a
3 × 3, ≈ 17 mm deep at most, tapering to exactly zero at both ends, so two
at right angles join into one curve).  He likes the narrow band and the
right-angle behaviour, not the zero: ends should be low-profile natural
rock, a consistent profile so they match, not flat, no repeating pattern.
(3) Try a few things; show end and top views.

- **Edge bodies** (`Body.backs` = the tile edge(s) behind the face): the
  face line sits `EDGE_T_MM` 8 mm in from the back edge.  Masses are biased
  1 mm forward and recesses soft-limited (softplus knee 0.4 mm) so the wall
  to the tile edge is never thinner than 1.5 mm: depth goes out, not in.
  Batter −2° (leans out).  Rock top band 0.6–2 mm, cap ease 3 mm, so the top
  is mostly dirt and reaches plateau level at the back edge.
- **Ends**: within `END_RAMP_MM` 8 mm of each tile crossing, the face line
  curves (smoothstep) back to `END_T_MM` 3.2 mm from the back edge, so the
  face turns the corner as textured rock instead of a right angle.  At the
  crossing D blends to `end_profile(z)`: a fixed-seed (identical on every
  tile) smooth wander + five irregular ledges + two notches.  No sine.
- **End relief** (`end_cuts`): each exposed end is recessed from the tile
  edge by a shallow noise relief (0.3–0.9 mm, σ 0.7 mm, per-tile seed),
  from just above the foot to the top (fading it out at the top made the
  end step outward), clamped at 45° in z.
- **Support backstop** in `mesh_solid`: every voxel needs rock below it
  within one voxel (4-neighbour, ≈ 45°).  It only removes specks where two
  surfaces meet (a face and its recessed end: up to three 0.001–0.023 mm²
  islands per tile before it), so every audited tile has 0 islands.
- **Presets** `end_thin` (2.0 mm, ramp 6) and `end_thick` (4.6 mm, ramp 10)
  are the "few things" to try; the contact sheet shows each as a butted
  pair and an outside corner, plus run → corner tile → rotated run (an L).
- Corner and canyon are thin bands too (an L band, a corridor); outcrops
  and spires scale with the tile.  `--cols/--rows` default to 1 × 1.
- Contact sheet views: painted oblique, top-down, end view (from −x),
  painted close-up of an end or seam.
- **Back edge flush**: within 2.5 mm of an edge band's back edge the top
  eases to exactly plateau level (z_p − 0.4 mm + dirt, the dirt being 0 at
  the rim), so raised ground set behind the cliff merges with it.  Without
  it a thin band's cap followed the rock's top line all the way back.
  Verified: back-edge top 38.60 mm along the whole edge (run, corner,
  canyon).
- **Crowned free-standing tops**: outcrop and spire caps rise 1.8 mm toward
  the middle; a flat cap below a rock rim read as a crater from above (the
  new top-down view caught it).

## Rev 4: ends are faces (2026-10-30)

Shawn, seeing the textured standard ends: drop the matching profile; the
ends get the same texture as the faces ("like the stuff we're doing with
spires… can go right around the edge"); the only constraint is that where
an end meets the back wall it comes within a millimetre or two of the
tile's side, so neighbours read as a crevice.  No repeating anything.  And
thumbnails: whole tiles only, from several sides; no crops.

- **Bands are closed bodies inside the tile** (`_band`): a box from
  END_GAP_MM (1.2) in from each side to 2 mm past the back edge, convex
  corners rounded 3 mm, concave (corner form) 4 mm; its outline is the face
  line, wrapping in s.  Ends are faces like any other.  Removed: the
  standard end section, `end_profile`, the end-relief cuts, the
  `end_thin`/`end_thick` presets, `Body.ends`.
- **Per face-line sample** (`face_map`): position, outward normal, and
  `wf` (1 on fronts — facing away from the back wall — 0 on ends).
  - Fronts: plan wander, 1 mm forward bias, −2° lean out, the band-wall
    limit (inward ray to the tile boundary − 1.5 mm, soft).
  - Ends: no plan wander (it was z-constant: an end pushed out by it sat
    against the limit full height, a ruled surface), relief × 0.6, 1.5 mm
    recess bias, +4° lean back (leaning out ran ends into the limit and
    left long streaky undersides).
  - Back taper: relief × smoothstep(distance to the back wall / 1.5 mm),
    and the top edge → plateau level there.
  - Tile-edge limit for every body: soft max(D, −(outward ray to the tile
    boundary − clearance)), knee 0.7 mm, where the clearance is itself a
    rock relief (0.3–1.1 mm): rock pressed against the edge stays textured.
    Applied again after the underside smoothing (σ 2.0 mm along s now),
    whose blur had carried a front's proud corner mass past the tile edge
    into the tile clip, slicing it flat; then re-clamped.
- **Contact sheet**: six whole-tile views per row (oblique left/right,
  front, left end, right end, top-down); assemblies: two butted pairs, an
  outside corner, run + corner tile + run.

## Rev 5: rock bodies textured all round; contact rims seal (2026-10-30)

Shawn on rev 4: the ends weren't cohesive with the front ("a big mass in
front with a horizontal crevice above it should curl around to the
side").  Make the cliff not a special case: a big rock wall textured on
all four sides with dirt on top; the only constraint is that faces against
the tile edge are low relief at their *rims* (they must fit tight against
neighbouring cliffs and the ground), while their middles can have big cuts.

- **One texture, no front/end special cases.**  Removed the per-face
  bias / scale / lean / wander rules of rev 4.  Bodies: `run` = a slab
  `box(0.6, H − 8, W − 0.6, H)` against the back edge; `wall` (new) = an
  11 mm free-standing wall across the middle; `corner`, `canyon` likewise.
  Convex corners rounded 3 mm, concave 8 mm (4 mm streaked: points just
  inside a tight concave corner flip between the two faces' textures).
- **Contact faces** (`c` → 1 when the outward ray to the tile boundary is
  under 1–3 mm): no plan wander, no batter (upright on the boundary), and
  the texture shifted back per column (by its smoothed minimum) so its
  proudest point just touches the boundary: it cuts inward with its whole
  relief (× 0.6), never bulges past.  Clipping the bulges instead left
  flat panels and a foil-like surface.  Eased to flush over 2.5 mm at the
  top and bottom rims; top edge at plateau level.  Open faces (`c` → 0):
  full relief, 4° batter.  Blended by `c` along the outline, so the
  texture carries round the corners.
- **Cuts never meet**: each face recesses at most half the chord through
  the body (ray within the footprint), less 0.75 mm (soft).  Large masses
  sink at most 2 mm (they stand proud up to 5): deeper ones hit that limit
  and went flat.
- Tile-edge clearance relief reduced to 0.3–0.65 mm.
- Contact sheet: seven whole-tile views (adds the back); wall rows.
- Audits: all forms watertight, micro-lips ≤ 1.7 mm², 0 islands except one
  0.002 mm² speck near a back corner on run s2 (sub-nozzle).

### Rev 5 fixes from Shawn's review (2026-10-30)

| Seen | Cause | Fix |
|---|---|---|
| vertical crease near wall ends (convex and concave) | the open → contact blend happened over ≈ 2 mm of a 3 mm corner, so the two treatments (relief + batter vs shifted + upright) met in a line | `c` blurred along the outline (σ 2.5 mm, `CONTACT_BLEND_MM`) |
| big flat triangles on run (deep), walls, corner | cuts reaching the "never meet" limit held at a flat floor; the simplifier merged those planes into a few big triangles, and the preview painted each triangle one band | the limit's floor carries a 0–0.7 mm rock relief (`FLOOR_TEX_MM`); the preview paints a subdivided copy (≤ 0.6 mm edges), paint measured on the built mesh and carried over by barycentric interpolation |

## Rev 6: the face is stacked blocks (2026-10-30)

Shawn: "the aesthetics have regressed… the sense of big piled-up slabs
doesn't fit."  Diagnosis: (1) the thin-slab and contact-face limits had
flattened the masses into a crack network on a flat wall; (2) more
fundamentally, a Voronoi cell network with V-grooves can never make the
references' stacked prismatic blocks with flat chipped tops.  Rewrite of
the large scale (`build_blocks`), everything else kept:

- **Columns** along the face line: widths 3–12 mm (skewed narrow), joints
  within ±7° of vertical (12% at 18–32°), wobbly; 25% of joints stop for one
  5–12 mm stretch, so one block bridges two columns.  (Bug found on the way:
  numbering columns by *counting* active joints renumbered every column to
  the right of a gap — horizontal stripes across the whole face.)
- **Blocks**: each column split by near-horizontal joints (±5°) every
  7–24 mm; 55% of columns snap a joint to one of 2–3 shared strata (subtle
  layering).  Row joints shallow (0.45–1.0 mm) and fading, column joints
  deep (1.5–3.2 mm): vertical dominance.
- **Offsets**: masses of 1–2 columns step forward 0–8 mm, columns ± 1.8,
  blocks ± 0.25; a block above steps back 0.6–2 mm with p 0.18 (lit block
  tops; every-row steps made a layer cake); 8% of blocks recessed (voids).
- **Faces**: per block, a front plane (±7° across, lean back ≤ 7°) maxed
  with 4 chisel facets (18–40°, starting 0.45–0.9 of the half-extent out;
  top chamfer on 35% of blocks only; no downward facets) and a 1 mm 45°
  chamfer at every block edge: convex, crisp, chipped.
- **Top edge** per column, −5…+3 mm (blocks stand proud of the top line).
- Medium plates now only score blocks (plane offsets × 0.25); ledges
  removed (blocks make them); diagonals 0–2; slab 13 mm.

### Butted tiles touch (2026-10-30)

Shawn: "significant light between adjacent cliff tiles viewed head on…
there should be a fair bit of touching."  Measured (rays along x onto each
end face): 0% within 0.5 mm, median gap 1.4–2.2 mm per tile.  Causes, all
stacking: footprints inset 0.6 mm; every corner rounded 3 mm, including
those on the tile boundary; a 0.3–0.65 mm clearance relief; softplus knees
(0.4–0.7 mm) adding k·ln 2 of setback even at a face's proudest point;
contact faces referenced to their single proudest point.
Fixes: footprints run to the tile edge; only free (front) corners are
rounded, corners on the boundary stay square (`_slab_round_front`); the
tile-edge limit sits 0.15 mm *past* the edge with a 0.05 mm knee, so flush
rock is clipped exactly on the boundary; contact faces are referenced to
their 60th-percentile level (`CONTACT_FLUSH`), so ~60% lies flush and the
joints and recesses stay as cuts, knee 0.12 mm, offset removed.
Result: 64–81% of each end face touches the boundary (< 0.1 mm), 83–93%
within 0.5 mm, median 0.0–0.07 mm; all forms watertight, 0 islands.
- **Correction (Shawn): "you're clipping adjacent edges flat.  They should be
  regular textured surfaces whose outside limits of texture touch the
  edge."**  The 60th-percentile flush and the past-the-edge limit are gone:
  contact faces are referenced to their proudest points again (a running
  minimum along s, so smoothing never lifts a peak past it), no clearance,
  0.04 mm knee.  Two meshing faults surfaced: a grid row exactly on x = 0
  meshed the whole left end flat (≈ 300 mm²; the right end, with no row on
  x = W, was fine) — the grid is now cell-centred and symmetric; and the
  constant air pad pulled every surface ≈ 0.1 mm inside the tile — the x/y
  pad is now the field's own continuation, then air, so a peak that
  reaches the edge meshes exactly to it.  Result: 0 mm² flat in the side
  planes; 5–28% of each end within 0.1 mm of the edge (the texture's
  peaks), 16–54% within 0.5 mm.  One 0.008 mm² speck at a slab's front
  corner on some seeds (sub-nozzle).
