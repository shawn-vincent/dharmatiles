# grasslab — Sculpted Grass Tile: Design

*Revision 2, 2026-10-27 (v1).  Implements `REQUIREMENTS.md` rev 2.
Code: `grasslab/grasslab.py`.  Revision 1 (v0: random lens blades in a
binary voxel volume) failed as "crumpled foil"; see REQUIREMENTS §7.3.*

---

## 1. Core idea: top-down, made of fronds, plus lips

> **Rev 3 (2026-10-27):** the mass map T is no longer seeded with the
> swath envelope.  T starts at bare ground (H₀), and **every bit of grass
> height comes from a frond's own body**: a broad *skirt* that carries the
> swath envelope under the frond's footprint, plus the blade on top.
> Boundaries are the outlines of the last fronds (REQ-EDGE-1…5).  The rim
> ease and the soil cut of the envelope are gone.  The diagram below still
> shows the stage order; read "MASS map" as "soft-max of frond skirts and
> blades over H₀".

The grass is built **from the largest visual scale down**:

```
 H₀ hills ──► direction field θ ──► SWATH partition (warped anisotropic Voronoi)
                                          │  per-cell axis, (s, l) coordinates,
                                          │  wave envelope E(s, l)
                                          ▼
                     FRONDS: Poisson roots, each owned by the swath under its root,
                     riding its own swath's envelope past the cell border
                                          │
              ┌───────────────────────────┴──────────────────────────┐
              ▼                                                      ▼
   MASS map T(x, y): soft-max of frond                 LIPS: the last ≤ 2 mm of each frond,
   dome tops, solid to the ground                      a column interval [bottom, top] with
   (a height field → no overhang)                      the underside chamfered at exactly 45°
              └───────────────────────────┬──────────────────────────┘
                                          ▼
          scalar field F(x, y, z) = max(T − z, lip intervals)  (vertical signed distance)
                                          ▼
     FDM backstop (support filter, fill holes, bed connectivity)  →  marching cubes on F
                                          ▼
                      ∪ DB socket base (manifold3d) → simplify → STL
```

Two guarantees drive this layout:

- **Undercuts exist only where they are designed.**  The mass is a height
  field, so it has no overhangs at all.  The *only* non-height-field
  geometry is the lips, and each lip's underside rises at exactly 45°
  from its root.  REQ-FDM-1, REQ-FDM-3 and REQ-FR-5 follow from the
  geometry and need no filtering.  The bottom-up support filter is kept
  only as a backstop, and its removal count is reported; it should be
  about 0.
- **No terracing.**  Marching cubes runs on a *continuous* field
  (T − z is linear in z), so surfaces interpolate exactly between grid
  samples.  There are no occupancy steps and no blur (REQ-GR-2).

## 2. Stages

### 2.1 Hills H₀

Unchanged from v0, which passed: two-scale band-limited noise, normalised
to 3 mm peak-to-valley, a slope cap, and a cosine feather to the edge
level.

Revised after the v1 review: landforms of σ 12 / 6 mm (mid weight 0.35),
landforms σ 10 / 5 mm, a 7 mm raw relief target, a 9 mm edge feather, a 34° slope cap, a 4.2 mm edge level and a 1 mm soft floor (soft-max, k 0.3).  The
earlier 6 mm feather forced steep slopes near the rim, which made the cap
scale the whole relief down.  Realised relief is about 5–6 mm.

### 2.1a Bare soil (rev 3: seams, not blobs)

Soil is computed **after the swath partition**, because it comes from it.

- **Seams**: column pairs with different swath owners give the seam
  pixels for each cell pair (a, b).  Seams shorter than 5 mm, or within
  4 mm of the rim, are ignored.  Each seam is scored by
  −z(mean H₀) − z(mean envelope) + noise, so low ground and seams that
  are already low (lateral or upstream edges) win.
- **Lanes**: N = U(2.5, 3.0) × squares seams are opened, as chains of up
  to 3 seams.
  - Two seams chain only when they meet at a triple junction, meaning
    (a, b) and (b, c) are both seams and (a, c) is too.  This makes the
    lanes wind continuously.
  - Each new lane starts 18 mm or more from existing ones, so the soil
    spreads across the tile.
- **Width**: an EDT from the opened seam pixels gives the distance d and
  the nearest seam pixel.  The half-width is
  `hw = 5.8 · clip((n + 0.9)/1.3, 0, 1)^0.8`, where n is smooth noise at
  σ 3.5 mm sampled *at the nearest seam pixel*, so the width varies along
  the lane.  Stretches with hw < 1.1 mm close entirely (no slits), and the
  bank line gets 0.35 mm wobble.
- **Mask**: m = smoothstep over a 0.5 mm bank, tapered to zero within
  4–6 mm of the rim.
- **Grass response (rev 3)**: the mask only decides where fronds root;
  §2.4b gives the bank behaviour.  The rev 2 envelope cut, which carved a
  hole in the grass volume, was removed.
- **Soil relief**: −0.4·m_s (the trough, where m_s is the mask blurred
  with σ 0.8 mm so no step outlines the patch), plus 0.08 mm·m_s
  roughness, plus faceted pebbles.
  - **Pebbles**: Poisson with density 0.06/mm² of soil, 1.2 mm or more
    apart, radius skewed small (0.45–1.2 mm).
  - **Shape**: each pebble is a low dome (0.5–0.7 r) cut by 7–10 planes
    at 30–42°.  The rev 2 flat-topped, 45–60° prisms read as boxy chunks
    on the 0.12 mm grid.

### 2.2 Direction field θ

A scalar angle field: θ₀ + 35°·n(x, y), with n smooth noise at σ = 25 mm,
blended toward downhill by a bounded weight of 0.3 or less.  A scalar
angle field has **no singularities**, so it cannot form crowns or vortices
(REQ-SW-6).

### 2.3 Swath partition (REQ-SW-1…5)

- **Sites**: Poisson-disk at 13 mm over the tile plus a margin.  Each site
  takes its axis dᵢ = θ(siteᵢ).
- **Ownership**: each point takes the site minimising the anisotropic
  distance
  `‖(Δ·dᵢ)/1.0, (Δ·d⊥ᵢ)/0.65‖ + 2.5 mm · warp(x, y)`,
  where warp is smooth noise at σ ≈ 3 mm.  Cells come out elongated
  along their own flow, with irregular, non-polygonal borders
  (REQ-SW-5).  Every point has exactly one owner, so coverage is total
  by construction (REQ-SW-1).
- **Cell coordinates**: the along-axis position a = Δ·dᵢ and lateral
  position b = Δ·d⊥ᵢ, normalised per cell to
  s = (a − a_min)/(a_max − a_min) ∈ [0, 1] and l = b/|b|_max ∈ [−1, 1].
- **Wave envelope**:
  `E(s, l) = A_i · (0.25 + 0.75·s^1.6) · (1 − l²)^0.6`, with A_i drawn
  from 1.2–2.2 mm.  The envelope is low upstream and at the sides, and
  high at the downstream centre (REQ-SW-3).  Lateral neighbours meet
  near zero, so there are no side-by-side cliffs.

### 2.4 Fronds (REQ-FR-*)

- **Roots**: Poisson-disk at 2.0 mm over the tile plus a margin.  Each
  frond is owned by the swath under its root and records its own s_b and
  l_b.
- **Direction**: dᵢ rotated by a fan of l_b·12° plus ±4° jitter, with a
  slight curvature κ ∈ ±0.03 /mm (REQ-FR-3, REQ-FR-6).
- **Length**: L = U(4, 8) · (0.75 + 0.4·s_b), so fronds grow toward the
  crest.
- **Local coordinates** for a column: u is the distance along the spine
  (bent by κu²), and v is the lateral distance.
- **Planform half-width**:
  `w(u) = w_tip + (w_max − w_tip)·sin(π(0.25 + 0.75·u/L))`, with
  w_tip = 0.45 mm.  It is widest at about L/3, and past u = L it closes
  as a round cap of radius w_tip (REQ-FR-1, REQ-FDM-4).
- **Height**: the frond **rides its own swath's envelope**, continued past
  its cell's border.  Its s coordinate along the spine is
  s(u) = min(s_b + u/len_i, 1).  Then:
  `top(u, v) = H₀ + E_i(s(u), l_b) + h_f · rise(u) · dome(v/w)`
  where:
  - rise is a smoothstep from 0 at the base to 1 by 0.45 L, so the frond
    emerges flush from the mass (REQ-FR-4).  From 0.55 L it then **droops**
    smoothly to (1 − 0.35) at the tip, so the frond arches over instead of
    ending in a raised fin;
  - dome(q) = (1 − q²)^0.9, a ridged rather than flat-topped dome.
  
  A frond crossing into the next, lower swath therefore stays high, which
  is the shingled leading edge (REQ-SW-4).  Inside a swath, the next
  frond's base is low, so each tip laps the next base (REQ-FR-2).
- **Mass versus lip split**: lip_len = min(2.0, 0.35·L).
  - For u < L − lip_len, the frond contributes `top` to the mass map T
    with a soft-max (k = 0.12 mm).  This rounds groove bottoms while
    keeping crests crisp.
  - For u ≥ L − lip_len, the frond contributes a lip interval instead.
    The interval's top is `top`.  Its bottom is **measured back from the
    tip**: `tip_crest − 0.6 − (L_t − u)`, a plane at 45°.  Because the top
    only rises going back from the drooped tip, the centre line is always
    at least 0.6 mm thick.  Side columns thinner than 0.6 mm are left out,
    so the underside is only ever the chamfer plane.  Where the surface
    below is higher, the lip simply fuses.

### 2.4a Frond skirt (rev 3, REQ-EDGE-1/4)

- Each frond has a **skirt** under its blade.  Its half-width is
  ws(u) = 2.2·w_max·(1 − ((u − u_c)/u_h)²)^0.6, a lens running from
  u = −0.6·ws_max (a rounded base behind the root) to the tip L_t.
- **Skirt height**: `env_i(s(u), l_b) · taper(u) · (1 − q_s³)^1.5`, where
  q_s = |v|/ws and the taper drops 45% toward the tip.  Neighbouring
  skirts overlap heavily, so inside the grass their soft-max reproduces
  the envelope as before.  At any boundary the grass ends in scalloped
  frond lobes.
- **Swath borders**: a frond's skirt rides its own swath's envelope past
  the border and tapers with its own outline.  The leading edge is
  therefore lobes over the next swath, not a step.
- **Blade on top**: `h_f · rise(u) · dome(q)` inside the blade planform.
- **Lip columns**:
  - Only blade columns past u_lip become lip intervals.
  - Skirt columns stay in the mass.
  - Lip side columns too thin to undercut stay solid (a fused blade edge)
    rather than being dropped, which had made the tips saw-toothed.

### 2.4b Boundaries are where fronds grow (rev 3, REQ-EDGE-2/3/5)

- **Rim**: only fronds rooted inside the tile exist.  Each frond is
  **shortened** along its spine so the tip stops 0.4 mm inside the wall,
  and is never sliced.  Skirt bases of fronds rooted right at the rim may
  meet the wall.  There is no ease and no ramp.
- **Soil**: fronds don't root in soil (10% become sprigs).  Bank fronds
  are shortened where their spine reaches 1.5 mm deep into the soil
  (EDT).  The soil mask never touches T.
- **Shortened fronds are lower**: blade and skirt heights are scaled by
  k_h = clip(L/L₀, 0.3, 1)^0.8.
- **Bank fronds only** (REQ-EDGE-6):
  - A frond whose spine was stopped by soil gets its skirt ramped to 0
    over the last 2.2 mm (a pointed, lowering tip).
  - A frond rooted within 2.5 mm of soil gets its base cap ramped up.
  - All other fronds keep the rev 3 skirt unchanged.  Applying these
    ramps globally (tried in v1h-a/b) scalloped the whole tile.

### 2.5 Grooves (REQ-GR-*)

Grooves come from the geometry and are never carved:

- Adjacent domes meet in V-channels whose depth is about h_f at the
  frond edges.  The soft-max sets a minimum groove radius of about k.
- Each lip leaves a 45° wedge shadow.

The dome and spacing are sized so neighbouring fronds overlap their
edges, which keeps grooves about 0.6 mm wide or more at half depth.

### 2.6 Rim (REQ-FDM-6, REQ-EDGE-3)

*Superseded in rev 3.*  The old 0.5 → 1 height ease produced a uniform
bevel ("chopped at an angle") and was removed.  See §2.4b.

### 2.7 Field, FDM backstop, meshing

- F = clamp(max(T − z, lip intervals), ±2·vox), on a grid of vox = 0.12 mm
  (0.1 mm for 1 × 1).
- Backstop on occupancy F > 0: a bottom-up 45° support filter, hole
  filling and 6-connected bed connectivity.  Every voxel these change has
  F forced to ±vox.  All changes are counted and reported, and should be
  about 0 by construction.
- Meshing pads with air and runs marching cubes on F at level 0, with no
  blur.  Grid centres sit at (i + ½)·vox so walls land exactly on the tile
  edges.  Then: ∪ DB base, `simplify(0.015 mm)`, keep the largest body
  (crumbs are reported), and export.

## 3. Parameters (module constants)

| Constant | v1 |
|---|---|
| `SWATH_SPACING_MM`, aspect (along : lateral), warp | 13, 1 : 0.65, 2.5 mm @ σ 3 mm |
| `SWATH_AMP_MM` | 1.2–2.2 |
| envelope floor / exponent / lateral exponent | 0.25 / 1.6 / 0.6 |
| `FROND_SPACING_MM` | 1.6 |
| frond L / w_max / h_f | 5–9 / 0.6–1.0 / 0.7–1.1 mm |
| dome exponent / tip droop | 0.9 / 0.35 |
| ground edge level | 3.3 mm (puts the mean grass top in the DB band) |
| fan / jitter / curvature | 12° / 4° / ±0.03 /mm |
| lip max / tip t_min | 2.0 mm (0.35 L) / 0.6 mm |
| soft-max k | 0.12 mm |
| θ coherence / swing / terrain follow | 25 mm / 35° / 0.3 |
| rim ease | 0.5 → 1 over 3 mm |

## 4. Risks to judge on S1

- **Swaths reading as fish scales or roof tiles** (F7) if the envelopes
  are too regular.  Levers: amplitude spread, warp, aspect.
- **Lip shadows too faint.**  Lever: D and the swath amplitude contrast.
- **Fronds reading as pods.**  Levers: a lower dome exponent and longer L.

## 5. v1 iteration log (2026-10-27)

| Iter | Named artifact (Claude's self-check) | Fix |
|---|---|---|
| v1a | **shark fins**: frond tips at the crest with vertical fronts; **roof shingles**: wide flat domes, no along-flow grooves; mean top 4.26 mm (below the band) | droop to the tip, underside measured from the tip; narrower and denser fronds; dome exponent 0.55 → 0.9; ground edge 3.0 → 3.3 |
| v1b | handed to Shawn | — |
| v1d | Shawn: "slightly more hills, slightly more bare patches" | relief ≈ 5 mm, 30° cap, edge 4.0 mm; 4 patches (8.6%) |
| v1h | Shawn: rim good; "bare patches have some weird artifacts" → sawn-log frond ends at banks, boxy prism pebbles, dashed dip step | bank-only soft frond ends (global version scalloped the tile, reverted), dome-faceted pebbles, blurred dip mask |
| v1g | Shawn: "more hills and bigger bare patches" | smaller landforms (σ 10), relief ≈ 5.4 mm, 34° cap, 1 mm soft floor; soil lanes wider (hw 5.8) and more seams → 12.8% |
| v1f | Shawn: grass "uniformly chopped down like it was all made of clay" at the rim and the bare spots; confirmed by close-up renders (rim bevel, carved soil walls, swath-border steps) | grass made of fronds: skirts carry the envelope, T starts at H₀, boundaries come from shortening fronds, shortened fronds are lower; rim ease and soil envelope cut removed |
| v1e | Shawn: "bare patches are terrible" (puddles, trench rims, bubble pebbles) | soil rebuilt from swath seams: winding lanes, sharp banks with limited spill, trough, faceted pebbles, sprigs; slit stretches closed; lanes spread 18 mm apart |
| v1c | Shawn: "increase the hills a bit… a few patches of bare ground" | hills ≈ 4 mm (see §2.1); bare-soil patches (§2.1a) |
