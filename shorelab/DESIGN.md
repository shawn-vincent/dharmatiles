# shorelab — Design

*v0, 2026-10-28.  Code: `shorelab/shorelab.py`.*

## Pipeline

1. **Ground**: grasslab's rises-only hills (the field), plus a **bank**
   across the tile.
   - Shoreline position x_s(y) = 33 mm + 11 mm·n₁₄(y) + 5 mm·n₅.₅(y), giving
     bays and headlands plus coves.  Each n is smooth noise normalised to
     2σ and clipped, so a single extreme peak can't flatten the rest.
     Land is on the left.
   - For signed distance s = x_s − x: G = bed + (hills − bed)·smoothstep((s
     + 3)/8 mm), with bed = 0.4 mm.
   - The waterline (G = 2.6 mm) is where the bank crosses the water level,
     so its shape follows the hills as well as the wobble.
2. **Rocks**: rocklab scatter (6–8), posed.  Each rock is classified as
   *wet* (centre in water) or *dry*.  All footprints block grass.
3. **Soil**: meadow free lenses on land, plus a **mud strip**.
   - The strip covers land within w of the waterline, where w is smooth
     noise mapped to 0–4 mm.  It is zero over part of the shore, so there
     grass reaches the water.
   - Water (G < WL) is a grass **blocker**: no rooting, and fronds stop
     0.5 mm in, lapping over the waterline.
4. **Grass flow**: grasslab flow, wrapped around lenses and rock circles
   (meadow), plus **shore steering**.
   - Within 9 mm of the waterline, headings that point away from the
     water are compressed toward along-shore, and others lean toward the
     water.  It is applied as a smoothed rotation (same as `steer_rim`).
5. **Grass**: `grasslab.build_grass(blocker = water ∪ rock footprints)`,
   then field, backstop and mesh.
6. **Water**: `waterlab.Scene.from_fields(G sampled on the water grid,
   wet rocks)`, then `gen_swell_chaos`.
   - Water surface z_w = WL + relief·rim·smoothstep(d/0.4) + tide.
   - Water solid: a heightfield of z_w where z_w > G **inside the basin**
     (the water mask dilated by 0.6 mm), otherwise G − 0.3 (buried).
     Without the basin limit, the tide (±0.6 mm) flooded low soil lenses
     on land into puddles (v0a).
7. **Rocks**: seated with meadowlab's `seat_rocks`.  Against the water
   surface the grass top is taken as WL + 0.6 mm, so wet rocks emerge
   from the water.
8. **Reeds**: 3–5 clumps in the shallows (G within −1.1…+0.2 mm of WL),
   built *before* the water so their stems can emit ripple rings,
   Poisson-placed at least 12 mm apart.
   - Each clump has 5–12 **blades** with a lemon (pointed-oval)
     cross-section: base half-width 1.3–1.8 mm and half-thickness 0.35–0.45
     of that.  They are extruded and tapered moderately (scale 0.25–0.35 at
     the top) to a blunt point, height 8–13 mm.
   - They rise from a common base (within 0.6 mm of the clump centre) and
     **curve** outward: each blade is warped by k·z², so it rises vertical
     and bends out to a 14–34° tip angle.  The flat face points toward the
     centre, plus 8° toward the water.
   - Chunky at the base so they don't snap.  Cattails were dropped in
     v0e at Shawn's request: "nothing more sophisticated".  The
     contract stays at the base: the blade is about 1.8–2.6 mm wide.
9. **Union**: grass ∪ water ∪ rocks ∪ reeds ∪ slab ∪ DB base, then
   `stl_safe`.
10. **Check**: a painted preview with grass, mud, water, rock and reed
    colours, plus oblique, top and low views, written as a timestamped
    contact sheet.

## Iteration log

| Iter | Artifact | Fix |
|---|---|---|
| v0a | shoreline nearly straight; blue puddles on land; mud strip only ~1% | two-scale shore wobble; water confined to the basin; mud 0–4 mm, bias 0 |
| v0b | still straight on some seeds (one big wobble peak dominated) | wobble normalised to 2σ instead of the max |
| v0c | handed to Shawn | — |
| v0k | Bambu: "floating regions" on shore s1 | A slicer-style island check (0.2 mm layers, shapely) found 15 islands of 0.002–0.08 mm² at the carved grass ledges.  Plain grasslab and dirtlab have none.  Cause: the voxel support filter allows a one-voxel diagonal step per 0.12 mm row, which builds up to an unsupported start over a 0.2 mm layer on thin carved features.  New grasslab `SLICER_LAYER_MM` (default off; the shore sets 0.2): every voxel row must directly overlap the row one layer-height below, components that don't are removed, then support is re-propagated.  Result: s1 and s3 have 0 islands; s2 has 1 of 0.003 mm² (sub-nozzle).  The undercuts are kept (40% gap columns).  Grass, meadow and water are byte-identical. |
| v0j | Shawn: side undercuts too, so the blade is defined all round except at the base; wider blades ("little tufts at this scale") | new grasslab options (off by default): `SIDE_CARVE_ALL` puts side ledges on both edges of every blade past `SIDE_BASE_FRAC` = 0.3 of its length, with a `SIDE_GAP_MM` air gap.  Shore settings: gap 0.6, blade half-width 0.8–1.25 (was 0.5–0.8), side lip 0.8 mm.  Undercut columns with a gap: ~40%.  Overhang audit 63 mm² (~1% of the surface; micro-lips at the carved edges, z 3.5–5 mm).  STL 33 MB. |
| v0i | Shawn: "more — a 45° cutout, so the tips are separate geometry" | Diagnosed: tip lip bottoms sat *below* the ground (tips only ~1 mm high), and neighbours' skirts covered most tips, so there was no room for a gap.  Fixes: **tips lift** (`DROOP` −0.35, curl up instead of drooping); blade height 1.1–1.5; grasslab `TIP_GAP_MM` (0.6 mm open air under each tip's 45° underside; shore sets it) and `TIP_VIS_TOL_MM` (tips buried ≤ 0.45 mm by a neighbour are exposed and the neighbour cut back).  Tip columns with a gap over 0.4 mm: ~7% → ~37%.  Overhang audit 12.5 mm² (micro-lips at the carved tip edges).  A 0.45 mm tip was tried and doubled the overhang, so it stays 0.6.  The STL is 30 MB. |
| v0h | Shawn: undercut the grass tips to emphasise them; stockier reeds | new grasslab option `TIP_CARVE` (default off, so grasslab and meadowlab are unchanged): tip lips go through the same carve as bank ledges, so neighbouring skirts no longer fill under visible tips.  Shore overrides: `TIP_CARVE` on, `LIP_FRAC` 0.35 → 0.5, `DROOP` 0.35 → 0.15.  Reeds: half-width 1.7–2.3, thickness ratio 0.5–0.65, 5–9 per clump.  Overhang is still 0.48 mm².  The painted preview now shows the dark gaps under tips as mud brown (preview heuristic, not geometry). |
| v0g | Shawn: smoother (less sharp) ripples at the shore and rocks; reeds should rise from near-coincident bases and curve out gently | shore-only water overrides: `SWELL_SHORT_SHARPEN` 0.9 → 0.45 plus a 0.3 mm blur of the relief (waterlab keeps its own look).  Reed bases within 0.35 mm; each blade is warped by offset = k·z² toward its fan direction, so it starts vertical and curves out to a 14–34° tip angle (plus 8° toward the water).  Seed 2's tallest reed tip passed 15 mm, so its tile takes the tall peg. |
| v0f | Shawn: shorter grass blades with deeper crevasses; reeds larger, spreading from a common base, less taper but still pointed | shore-only grass overrides (`GRASS_OVERRIDES`): frond length 3.5–6.5, blade height 0.95–1.35, skirt × 1.55, dome exponent 1.5; grasslab and meadowlab keep their own look.  Reeds: half-width 1.3–1.8, height 8–13 mm, bases within 0.6 mm of the clump centre, fanning 10–32° outward (evenly spaced around the clump, flat face toward the centre) plus 8° toward the water, tip scale 0.25–0.35.  Overhang audit 0.25 mm²; top 14.3 mm, so the peg stays short. |
| v0e | Shawn: reeds should be blades (diamond or lemon section, tapering to a point), short and chunky | lemon-section tapered blades; cattails removed |
| v0d | Shawn: reeds in water should have small ripples circling out from them | reeds root in the shallows (ground −1.1…+0.2 mm about WL, mostly wet).  Each stem footprint becomes waterlab's `sc.reed_mask`, and `gen_swell_chaos` **adds** tiny rings: band 1.8–3 mm, 0.45 mm amplitude, e^(−d/4.5 mm).  They are added rather than blended because reeds sit at the shore, where the land's dominant waves would drown them in a blend. |

waterlab gained `Scene.from_fields` (scene from an external ground plus
obstacle solids) for this lab.  Its `ENCLOSE_FREE` went 0.3 → 0.4, so a
single bank stays exactly un-enclosed.


## Exposed ground = dirt (2026-10-29)

Shawn: dirt should be the texture wherever ground is exposed, across all
labs, and mud and other dirt textures are eliminated.  This lab now
uses `dirtlab.dirt_relief(seed, W, H)`: dirtlab's soil roughness, clods,
grit and pebbles, without its hills or big stones, as a relief offset
sampled onto this lab's grid.
- The mud strip now carries the dirt texture through grasslab; it is
  still a strip of bare earth by the water, just not a separate "mud"
  texture.
