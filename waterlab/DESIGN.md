# waterlab — Design

*v0, 2026-10-28.  Code: `waterlab/waterlab.py`.*

## 1. Scene construction (shared by every option)

- **Grid**: 0.1 mm cells over 70 × 70 mm.  The final surface is
  z(x, y) = max(land, water) on that grid.
- **Land**: the shoreline position along the left side is
  x_s(y) = 17 mm + 4 mm·n(y), with n smooth noise.  For signed distance s
  from the shoreline (positive into land):
  `land = (WL − 1.0) + (LAND_TOP − WL + 1.0)·smoothstep(s / BANK_W)`,
  with BANK_W = 7 mm, plus a little broad noise.  So the bank enters the
  water as a slope and the shoreline emerges where land crosses the
  water surface.
- **Rock**: `rocklab.make_rock`, one stone about 15 mm across, seated
  with its base 1.4 mm below the water level.  Its footprint at the
  water level (`Manifold.slice`) is rasterised as an obstacle.
- **Obstacles**: O = land above the water level, or the rock footprint.
- **Distance**: d = EDT from O (mm), lightly smoothed (σ 1 mm) so iso-lines
  don't trace every facet or bump.  The nearest obstacle point comes from
  the EDT indices and tells whether a cell is nearer the rock or the
  shore.
- **Rim**: relief × smoothstep(dist_to_rim / 4 mm), so the water meets
  the tile edge at the calm level (REQ-W-9).
- **Solid**: `dharmatiles.core.mesh.make_heightmap_solid` (top = z,
  bottom = 0) ∪ rock ∪ DB base, with `stl_safe`-style reload
  verification.

## 2. Water generators

All return relief r(x, y) in mm, added to WL in water cells.

### 2.1 legacy (dharmatiles port)

- Noise: white noise smoothed at σ 3 mm, normalised to RMS A_n.
- Ripples: N ≈ obstacle perimeter / 6 mm segments.
  - Each is a sine of wavelength 3 mm on the smoothed distance field,
    within one wavelength band after a random offset of 0–20 mm, with
    amplitude falling linearly with the offset.
  - Each has a flat-top length of 10–20 mm and a Hann-faded tail of
    15 mm, measured along the shore via the smoothed nearest-source
    coordinates.
- This mirrors `make_water_displacement` + `make_water_ripple_displacement`
  in spirit and parameters.  Amplitudes are reduced to fit REQ-W-3:
  noise RMS 0.25 mm, ripples 0.5 mm.

### 2.2 lapping (contour wavelets + quiet swell)

- **Phase**: the distance d is first warped by
  d_w = d + 0.7 mm·n₁ (smooth noise at σ 3 mm), so crest lines wobble
  rather than trace perfect offsets (W2).  Spacing widens outward:
  λ(d) = λ₀·(1 + g·d), so φ = ln(1 + g·d_w)/(g·λ₀).
- **Crest profile**: c(φ) = ((1 + cos 2πφ)/2)^p with p ≈ 1.8.  That gives
  narrower rounded crests and broad smooth troughs (REQ-W-8), not a
  symmetric sine (W3/W5).
- **Breaks**: the gate is computed per crest index k = round(φ).  Crests
  fade in and out along their length via smooth noise sampled at
  (nearest obstacle point + k·offset), so each crest breaks in a
  different place (no targets, W2).  The gate is smoothstepped to give
  clear gaps.
- **Envelope**: A(d) = A₀·exp(−d/decay), with rock A₀ and decay larger than
  the shore's.  The two are blended by nearest-obstacle type.  Because
  it is one distance field to the union of obstacles, rock and shore
  crests merge smoothly where they meet (REQ-W-7).
- **Contact lip**: a raised lip of 0.25 mm at d ≈ 0.6 mm (the meniscus),
  where the water climbs the obstacle.
- **Open water**: a quiet swell, isotropic band-limited noise (σ 5 mm,
  RMS 0.15 mm), × (1 − 0.7·near), where near is the normalised wavelet
  envelope.

### 2.3 crescents (lapping + open-water crescents)

Lapping near obstacles, plus sparse finite crescent crests in open water:

- **Placement**: Poisson-disc centres 9 mm apart, at least 6 mm from
  obstacles.
- **Shape**: each crescent is an arc of radius 4–9 mm and arc length
  5–11 mm at a random orientation, with a rounded cross-section (half
  width 0.7 mm) and tapered ends.
- **Height**: 0.25–0.45 mm, faded near obstacles so they don't collide
  with the lapping crests.
- **Combination**: a soft maximum of the crescents, so overlaps blend
  instead of spiking.

### 2.4 strokes (wind-combed sculpted strokes)

- **Crest-direction field** θ: an *unoriented* field, blended in
  doubled-angle space.
  - Near obstacles it is tangent to the iso-distance lines, with weight
    exp(−d/7 mm), so strokes wrap the rock and run along the shore.
  - In open water it is a smooth meandering field (σ 12 mm, ±70°): locally
    coherent, not globally aligned.
- **Strokes**: Poisson seeds 4.6 mm apart.  Each traces a centreline
  along θ in both directions.
  - Length is 9–18 mm near obstacles and 7–14 mm far away.
  - The cross-section is rounded, (1 − (r/hw)²)^1.3, with half-width
    1.0–1.1 mm and tapered ends.
  - Height is 0.5–0.85 mm near obstacles and 0.15–0.35 mm far away.
- **Merging**: overlaps use a smooth union, applied only where two
  strokes overlap.  A blanket softmax lifted every patch by k·ln 2 and
  drew visible box edges (v2 bug).
- **Contact lip** plus a weak swell.

### 2.5 hybrid (lapping contact crests + strokes)

Lapping owns d < 6 mm, and strokes fade in beyond it.

## 3. Parameters

These are module constants per generator; see the code.

## 4. Iteration log

| Iter | What | Notes |
|---|---|---|
| v0 | legacy, lapping, crescents | **legacy**: shore stripes plus a single big loop (reads as a pasted ring, W2/W7); quiet elsewhere.  **lapping**: 3 tidy rings round the rock (a target, W2), shore crests nicked by Voronoi seams, open water dead flat.  **crescents**: scattered arcs read as raindrop commas (W2) |
| v1 | + strokes; lapping breaks and per-crest jitter | strokes v1 too dense and short: worms / "rice" (W5) |
| v2 | strokes sparser, longer and wider; smoothed gate coordinates for lapping | strokes show faint box edges (softmax bug) |
| v3 | softmax fixed; lapping decay 8 mm; + hybrid | **strokes**: best so far, wind-combed sculpted water that wraps rock and shore.  **lapping**: best rock reaction but still target-like.  **hybrid**: radial strokes burst out of the rock (the near/far field conflict), not yet good |

## 5. Round 2 (2026-10-28): combed / lapped / rings + shared tide

Shawn's feedback on v3: bigger relief is good.  Strokes and hybrid both
have good features.  Two changes requested:
- **Stroke ends**: these should fade gradually and organically, so they
  look like ripples, not worms.
- **A larger-scale motion under the ripples**: like dharmatiles' shared
  sinusoidal edge profile.

The options were renamed, and the earlier ones were retired from the
sheet: strokes → **combed**, hybrid → **lapped**, lapping → **rings**.
**crescents** was dropped.

- **Tide** (shared by every option, including legacy):
  - Interior band-limited noise at σ 9 mm, RMS 0.35 mm.
  - Within 9 mm of the rim, it is forced to dharmatiles' shared edge
    profile: 0.6·sin(2π(t − (17.5 − λ/4))/λ) with λ = 17.5 mm, the same
    phase convention as `_force_tile_edge_water_profile`, and
    inverse-distance weighted at corners.  Any two water tiles therefore
    meet with the same dip-and-rise.
  - Ripples still fade to 0 within 4 mm of the rim, so the rim is
    exactly the profile.
- **Direction field**: an oriented regional field.  The component normal
  to obstacles is removed with weight exp(−d/7 mm), so it runs tangent
  to the rock and the shore.  This replaces the doubled-angle
  near/far blend, which radiated out of the rock in v3 "hybrid".
- **Ripple strokes**:
  - Gaussian crest exp(−2.2 q²), with no hard edge.
  - Shallow troughs either side (−0.28 h, centred at q ≈ 1.55).
  - Height bell sin(πs)^1.3 along the length; width tapers 35% → 100% → 35%.
  - Height varies ±20% along the stroke.
  - Poisson spacing 5.6 mm.  Lengths are 10–20 mm near obstacles and 7–15
    mm far away; heights are 0.75–1.15 mm near and 0.3–0.6 mm far.
- **lapped**: rings own d < 8 mm, with break bias 0.45; strokes beyond.
- **rings**: lapping with 1.2 mm wobble, amplitude 1.0 mm (rock) and
  0.9 mm (shore).
- **Per-option RNG** is keyed by option name (crc32), so the sheet order
  doesn't change the results.
- **Contact sheet**: `waterlab/contact_sheet.py` writes
  `out/YYYY-MM-DD-HHMMSS-contact-sheet-s<seed>.png`.  Rows are ranked
  best → worst, legacy is last, and every image carries a caption.

| Iter | Artifact | Fix |
|---|---|---|
| v4 | combed strokes too dense; merged into feather / palm-frond texture | spacing 4.2 → 5.6 mm, narrower Gaussian |
| v5/6 | lapped: contact rings mostly broken away on this seed (break gate) | break bias 0.45 for lapped; still weak, open item |

## 6. Round 3 (2026-10-28): physics-first ("real water, exaggerated")

Shawn's feedback on round 2:
- **rings and legacy** are still liked.
- **The strokes are too short and too uniform.**  The longest should be
  a significant share of the tile, and ripples near obstacles should be
  a different size from ripples further out.
- **Scales are unrelated.**  Big swells and short uniform ripples, with
  nothing between.

**Diagnosis:** real water is a **continuous spectrum**.  Height scales with
wavelength, and obstacles don't stamp rings.  Incoming waves **shoal**
(shorter and taller in the shallows), **reflect** and **diffract**, so the
near-obstacle size difference and the related scales emerge from one
process.  Fantasy water is that physics, exaggerated: sharper crests,
louder near obstacles.

- **`_wave_sim`**: a 2D damped linear wave equation on a 0.25 mm grid,
  1400 steps.
  - Speed c = 0.45 cells/step × √(depth / 1 mm), clamped at
    √0.35.  Clamping lower made grid-scale spikes at the bank.
  - Rock and land are fixed (reflecting) boundaries.
  - Waves are **injected** in a 5 mm band at the three water edges as a
    sum of 6 wavelengths (2.5–14 mm, log-uniform) × 3 sub-directions.
    Amplitude ∝ λ^0.7.
  - Directions are uniform (sim) or within ±30° of one wind (sim-wind).
  - The snapshot is zoomed to 0.1 mm and normalised.
  - *v1 failure:* forcing by in-place oscillating noise patterns, which
    pulsed blobs instead of propagating waves.
- **`_sharpen`**: u + k(u² − 1)/2 on tanh-tamed unit-RMS u, giving sharp
  crests and broad troughs.
- **sim / sim-wind**: sharpened × 0.32 mm RMS × (1 + 0.9·e^(−d/6)), plus the
  contact lip.
- **sim-rings**: the sim chop × 0.75, calmed up to 85% within 7 mm of
  obstacles, plus the rings field × 1.25 (break bias 0.25).
- **spectrum**: 70 plane waves (2.5–22 mm, a ∝ λ^0.65, ±75° about a
  wind), a Stokes second harmonic, and phase compression near obstacles.
  This one is the weakest: lumpy, with an odd calm zone round the rock.
- The **tide** (round 2) still underlies every option.
- **combed** and **lapped** stay in the code but are retired from the sheet.

## 7. Round 4 (2026-10-28): emitted (waves come from sources)

Shawn: fewer crossing directions.  Waves should come **from the
obstacles**, with land a strong source and rocks weak.  With neither,
pick a couple of tile edges and send waves perpendicular to the edge.
All-directions chop "ends up with a whole bunch of spiky shit".

- **Sources**, each with (strength, decay, λ₀, growth):

  | Source | Strength | Decay | λ₀ | Growth |
  |---|---|---|---|---|
  | land | 1.0 | 34 mm | 3.8 mm | 0.022/mm |
  | rock | 0.55 | 10 mm | 3.0 mm | 0.05 |
  | 2 random non-land tile edges | 0.28 | 50 mm | 5 mm | — |

- **Distances**: geodesic (Dijkstra through water, 8-connected, on a 0.25 mm
  grid), so land waves wrap round the rock instead of passing through it.
  - Distances are smoothed *within water only* (normalised convolution).
    Plain blurring leaked the 1e3 "unreachable" values out of land and
    rock and erased every wave (v4c bug).
  - Edge sources use plain distance from the edge, so waves travel
    perpendicular to it.
- **Train per source**: a wobbled log-phase (spacing widens outward) and
  sharpened crests ((1 + cos)/2)^1.7.  Long, rare breaks (bias 0.55,
  σ 7 mm) apply to land and rock only; gating edge waves made dashes.
  A same-source swell at 0.45× wavenumber and 0.55× amplitude rides
  underneath, so the two scales are related.
- **Combination (emitted)**: a partition of unity with weights w⁸, so the
  dominant source rules its territory and only a few directions cross,
  at hand-overs.  Amplitude × (0.35 + 0.65√w_max): far waves are quieter
  but still readable.
- **emitted-sum**: trains weighted by w and summed, so more crossing.
- **Result (seed 1)**: shore-parallel crests sweep out from the bank,
  wrap the rock and its contact rings, with crossing edge waves at the
  far corner.  Risks to watch:
  - wood-grain or directional rows where land dominates;
  - small dots and dashes near the shore at break gates.
