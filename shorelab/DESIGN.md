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
8. **Reeds**: 3–5 clumps on the waterline (G within −0.5…+0.6 mm of WL),
   Poisson-placed at least 12 mm apart.
   - Each clump has 5–12 stems: tapered cylinders, base radius 0.6–0.75 mm
     to tip radius 0.4 mm, height 5–9 mm, leaning 0–18° toward the water.
   - 25% of stems carry a cattail head (r 0.95, length 2.5 mm, 1.5 mm
     below the tip).
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

waterlab gained `Scene.from_fields` (scene from an external ground plus
obstacle solids) for this lab.  Its `ENCLOSE_FREE` went 0.3 → 0.4, so a
single bank stays exactly un-enclosed.
