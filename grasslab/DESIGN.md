# grasslab — Sculpted Grass Tile: Design

*Revision 5, 2026-10-29 (code v3: stroke blades).  Implements
`REQUIREMENTS.md` rev 4.  Code: `grasslab/grasslab.py`; section numbers
below match its stage banners.  Superseded designs and their lessons: §5;
iteration log: §6.*

---

## 1. Core idea: separate blades on dirt, each built the way a printer draws

```
 1 H₀ hills (rises only) ──► 2 flow θ: regional angle + in-phase meanders + downhill pull
                                   │
          3 SWATH cells (warped anisotropic Voronoi) → strand lengths along each cell
                                   │
 4 soil LENSES (flow-aligned eyes) → flow wraps each lens → rim steering → final flow
                                   │
 5 STRANDS of roots on long streamlines → each FROND traced along the flow,
       drawn and shortened (rim, soil, obstacles)
                                   │
 5c each BLADE: a stack of 0.2 mm layers, each a stroke along its spine a whole
       number of 0.45 mm lines wide (blade_layers) → smoothed: a loft through
       the layer outlines in height-above-ground, marching cubes at 0.05 mm,
       seated on H₀, quadric-decimated (blade_mesh)
                                   │
 6 dirt ground solid (H₀ + dirt relief, from z = 0) ∪ every blade (batch union)
                                   │
 7 ∪ DB socket base (manifold3d) → simplify 0.005 mm → keep largest body → STL
```

Three guarantees drive the layout:

- **The model is what prints.**  A blade is defined in the printer's own
  units: whole 0.2 mm layers and whole 0.45 mm extrusion lines.  Nothing
  is narrower than one line, so nothing the slicer would drop or mangle
  ("detect thin walls") is ever modelled.  Overhangs are counted in
  layers: a layer reaches at most ½ line past the one below (≈ 45°).  The
  smooth version (for resin) keeps that shape and only melts the steps.
- **Blades on dirt, no turf mass.**  *(Shawn, 2026-10-29.)*  Every bit of
  grass is a blade; between blades is the dirt ground.  No pad, skirt or
  envelope layer fills between them (that mass is what buried the
  undercuts).
- **Undercuts by construction.**  Each blade stands on a keel (its bottom
  layer) and its upper layers reach further toward the tip, so every tip
  overhangs open air, to the last layer.

Grids: ground, flow and swaths on a 0.25 mm grid extended 12 mm (ground)
or 10 mm (swaths) past the tile; the ground solid and the grass-top map on
a 0.12 mm grid (0.1 mm at 1 × 1); each blade's distance field on its own
0.05 mm grid.

## 2. Stages

### 2.1 Hills H₀ (`make_ground`)

- Noise: unit-variance Gaussian-filtered noise at σ 10 mm plus 0.35 × σ
  5 mm.
- **Rises only**: shift so the interior's 40th percentile is 0, soft-floor
  at 0 with smoothmax (k = 0.25·std, offset so smoothmax(0, 0) = 0), clamp
  ≥ 0, and scale so the interior max is 7 mm.
- Cosine feather over 9 mm to the 3.4 mm edge level: h = 3.4 + w(dist)·raw.
- **Slope cap**: if any slope exceeds 34°, the relief above the edge level
  is scaled down uniformly.  On seed 1 the cap binds, leaving 3.40–6.58 mm
  (≈ 3.2 mm of rise); 78% of the area is ≤ 12° (90% on seed 2).
- `--flat` returns the constant edge level; the hill noise is still drawn
  so the rest of the RNG stream is unchanged.

### 2.2 Flow field (`make_direction`, `finish_flow`, `steer_rim`)

All steps before 4 operate on a scalar angle, which cannot form
singularities (REQ-FL-5).

1. **Regional angle**: θ_r = θ₀ + 60°·n(σ 15 mm), θ₀ uniform.
2. **Meander**: θ = θ_r + A·sin(2π·s/15 mm + φ).
   - s is the **least-squares along-flow coordinate**, ∇s ≈ (cos θ_r,
     sin θ_r), solved by FFT Poisson on a mirror-extended grid.  Lines of
     constant s cross the flow, so neighbouring streamlines wave in phase.
   - A = 10°–30° from smoothstep(0.5 + 0.35·n(σ 12 mm)); φ = π·n(σ 14 mm).
3. **Downhill pull**: vector blend toward −∇H₀ with weight
   0.3·clip(slope / tan 34°, 0, 1).
4. **Lens wrap** (on the vector field), per lens with its own band
   U(1.5, 4.5) mm and strength U(0.55, 1.0):
   - d = lens-metric distance (§2.4), n̂ = normalised metric gradient,
     w = strength·(1 − smoothstep(d / band)).
   - Remove the inward component: v ← v − w·min(v·n̂, 0)·n̂.
   - **Parting** at the upstream tip: v ← v − 0.6·w·min(v·n̂, 0)·tanh(b / 0.8 mm)·p̂,
     with b the across-lens coordinate and p̂ the lens perpendicular.  The
     smooth tanh side choice avoids a herringbone seam upstream.
   - Renormalise.  Lens partings and rejoins are the only singularities.
5. **Rim steering** (`steer_rim`, rev v2g).  Three smooth knob maps
   (smoothstep(0.5 + 0.35·n(σ 8 mm))): ramp distance 9–15 mm, allowed angle
   φ_ok 70–88°, pull 0.15–0.45.  φ is the heading measured from an edge's
   outward normal, w = smoothstep(1 − dist/ramp).
   - **Turn side, once per stretch of rim** (`_rim_sides`).  Per perimeter
     sample, the w-weighted mean tangential lean sin φ across the steering
     band, laid out as one counter-clockwise loop (side +1 = turning CCW on
     every edge), smoothed at σ 20 mm *along the loop, across corners*.
     Its sign is the side: one per long stretch, and a corner continues the
     adjacent edge.  `hard` = that sign smoothed at 2.5 mm (which way
     inward headings turn), `soft` = smoothed at 8 mm (≈ 0 around a flip).
   - **Per edge**, a rotation Δ_e of the input angle:
     - Inward headings (|φ| > φ_ok) are compressed toward ±φ_ok on the rim
       side; the two candidate rotations (turn +, turn −) are blended by
       λ = ½(1 + hard), except that headings within 30° of parallel keep
       their own side (this also masks each candidate's unwrap cut at 90°).
     - **Pull outward** (|φ| ← |φ|·(1 − pull·w)) only for headings already
       going outward, faded over ±0.35 rad about 90°.  Pulling turned-in
       headings too gave an S-bend that read as a herringbone spine
       parallel to the wall.
     - **Square up around a flip**: there the pull rises to 0.85·(1 − |soft|)
       and its ramp reaches up to 2× further, so where two stretches meet the
       flow comes square to the wall and ends there.
     - **Run alongside** within 3 mm: blend toward sign(soft)·φ_ok with
       smoothstep(1 − dist/3 mm)·|soft|, i.e. faded out around a flip.
       (Without this every rim frond pointed into the wall and was cut
       short, leaving a bare "picture frame".)
   - **Corners**: Δ = Σ P_e Δ_e with a nearest-edge partition of unity
     P_e ∝ exp(−(d_e − d_min)/2 mm) (far edges have Δ_e = 0).
   - Applied as a **smoothed scalar rotation**: θ + G_σ1.5mm(Δ).  A
     continuous Δ cannot create a singularity.
   - Before v2g the side came from a pointwise smoothed preference
     (σ 14 mm, 2-D) and edges were applied in a fixed order.  Where the
     preference flipped, the run-alongside band forced opposite headings
     side by side: long feathered V seams and corner starbursts (meadow
     seeds 2, 4, 5).

### 2.3 Swaths (`Swaths`)

Swaths no longer carry height (there is no turf mass); they group strand
roots and scale frond length along each cell.

- **Sites**: Poisson-disk at 11 mm over the tile + 10 mm margin.  Axis dᵢ =
  the pre-wrap flow (step 3) at the site.
- **Ownership**: argmin of ‖(Δ·dᵢ), (Δ·d⊥ᵢ)/0.65‖ + 2.5 mm·warp(x, y)·φᵢ,
  with warp smooth noise at σ 3 mm and a per-site phase φᵢ ∈ U(−1, 1).
- **Cell coordinates** s = (a − a_min)/(a_max − a_min) ∈ [0, 1],
  l = b/|b|_max ∈ [−1, 1], normalised per cell on the 0.25 mm grid; s at a
  root lengthens fronds toward the cell's downstream crest (§2.6).

### 2.4 Soil lenses and soil relief (`place_lenses`, `Lenses`)

- **Placement**: N = round(U(1.3, 1.8) × squares), at least 1.  80
  candidates inside a 7 mm rim inset, scored by −z-score(ground height) +
  N(0, 0.8), accepted best-first ≥ 13 mm apart.  Typical coverage ≈ 10%.
- **Shape**: axis = pre-wrap flow at the centre; half-length U(5, 9) mm,
  half-width × U(0.45, 0.65).  Lens metric f = |a|/ℓ + (b/w)², which gives
  pointed ends (an eye); approximate distance d = (f − 1)·w/2 plus a shared
  ±0.8 mm border warp (σ 1.2 mm).  Mask = smoothstep(0.5 − d/0.5 mm).
- **Grass response**: the mask decides only where fronds root and how far
  they spill (§2.6); it never cuts a blade.
- **Ground texture**: a soft trough −0.4 mm·m_s (m_s = the mask blurred at
  σ 0.8 mm, so no step outlines the patch) in H₀, plus the shared dirt
  texture `dirtlab.dirt_relief` (roughness, clods, grit, pebbles) over all
  exposed ground: bare patches and between blades.

### 2.5 Strands and spines (`make_strand_roots`, `plan_fronds`)

- **Strands**: Poisson seeds at 2.65 mm over the tile + 8 mm margin.  Each
  strand is U(15, 30) mm long and starts U(0.2, 0.6) of its length
  upstream of its seed (traced backward), then is traced forward.
- **Roots** every 0.5·L·U(0.8, 1.2) along the strand, jittered ±0.3 mm
  laterally, with L = U(3.5, 6.5) mm; only roots inside the tile are kept.
- **Per root**: swath owner and (s_b, l_b); **bank** = within 4 mm of soil
  and not in it.  Heading offset: strays (5%, or 12% for bank roots) get
  ±U(20°, 35°), others U(±4°); bank roots add U(±15°) wander.
- **Spines**: one batched RK2 trace per root along the final flow, at
  0.25 mm unit-speed steps, rotated by the root's offset, to 6.5·1.1 +
  0.5 mm.  Neighbouring strands 2.65 mm apart against blades 0.9–1.8 mm
  wide leave dirt showing between ribbons (≈ 9% of the grass area).

### 2.6 Frond draw and shortening (`draw_frond`)

- **Skips**: roots under an obstacle (meadowlab's rocks, shorelab's
  water); roots in soil become **sprigs** with 10% probability (L
  U(1.8, 3.0), half-width 0.5, crest 0.6 mm), otherwise are dropped.
- **Dimensions**: L = L_strand·(0.8 + 0.3·s_b) (2.8–7.2 mm, longer toward
  the swath crest); nominal half-width U(0.8, 1.25) (→ width in lines);
  crest height h_f U(1.5, 2.0) mm.  Bank fronds: h_f × U(0.65, 1.15).
- **Per-frond endings**: rim tip inset 0.2 + 1.3·U² mm; soil spill
  0.8 + 2.4·U² mm, or 4 mm for wisps (4%).
- **Shortening** (never slicing): the spine stops at the first sample
  inside the rim inset, deeper than the spill into soil (EDT), or deeper
  than `tuck_mm` into an obstacle; L becomes that arc length.  Fronds
  shorter than 1.5 mm are dropped.  Shortened fronds are lower: h_f ×
  clip(L/L₀, 0.3, 1)^0.8 (REQ-EDGE-5).

### 2.7 Blade strokes (`blade_layers`)

A blade of crest height h has N = round(h / 0.2) layers.  Layer k is a
stroke along the traced spine (the frond's streamline), a whole number of
lines wide, built as nested round-capped buffers of the spine segment:

- **Cross-section (line counts per layer)**: n_k = round(n_max·sin(π(k +
  ½)/N)^0.6), n_max = clip(round(2·half-width / 0.45), 2, 4); the bottom
  layer ≥ 2 lines (the keel), the top layer exactly 1 line (the sharpest
  ridge FDM can make), and n_k ≤ n_{k−1} + 1 (growth ≤ ½ line a side per
  layer ≈ 45°).  Example, 10 layers, 4-line body: 2 3 4 4 4 4 4 3 2 1.
- **Profile (where a layer starts)**: u_back = L·(1 − (1 − (k + ½)/N)^{1/2}),
  the root-side end of the crest 1 − (1 − u/L)²: a convex arch rising
  from the ground at the root and levelling out at the tip.
- **Tip (where a layer ends)**: u_front = L − (N − 1 − k)·0.2 mm.  Only the
  top layer reaches the tip; each layer below stops 0.2 mm shorter, so
  the bow is raked and overhangs open air to the last layer, with the
  keel underneath.
- **Points**: within a layer the stroke loses one line every 0.45 mm
  toward each end, so both ends close to a single line's round end: in
  plan, every horizontal section ends in a point.
- **Height map**: each layer's footprint raises the grass-top map T to
  H₀ + (k + 1)·0.2 on the 0.12 mm grid (used to seat rocks, for the DB
  band check and the gap statistic).

The stroke stack is exactly what an FDM slicer would draw: opening each
layer by just under half a line keeps 99.9% of its area
(`blades_demo.py --strokes` writes it unsmoothed).

### 2.8 Smoothing and seating (`blade_mesh`, `build_blade`, `ground_solid`)

- **Loft**: each layer outline is closed by 0.3 mm (fills the notches where
  a stroke steps down a line) and becomes a 2-D signed distance field on a
  0.05 mm grid.  In height above ground h, the 3-D field blends linearly
  between layer centres, so the surface runs straight from one outline to
  the next: the layer steps vanish but the plan shape (outline, keel,
  point) is kept.  Above the top centre the field closes ½ line over ½
  layer (a rounded ridge); below the bottom centre it continues 0.3 mm
  into the ground (`EMBED_MM`).
- **Blur** σ 0.05 mm in plan and 0.15 mm in h rounds the loft's creases at
  each layer centre.  (A vertical blur alone could not remove the steps:
  a layer's back end moves ≈ 0.5 mm along the blade per 0.2 mm up.
  Catmull-Rom interpolation between layers overshot the whole-line
  jumps and made the blade lumpy.)
- **Mesh**: marching cubes; each vertex is lifted by the bilinear H₀
  under it, so blades follow the hills; quadric decimation to 1,500 faces
  (≤ ≈ 20 µm off the field, below resin XY resolution; 800 faces read
  faceted in a viewer).
- **Obstacles**: with `side_low < 1` (meadowlab), a blade's height is
  scaled by the side factor at its tip, where it presses against a rock.
- **Ground**: H₀ + dirt relief as a heightfield solid from z = 0
  (`make_heightmap_solid`); blades are meshed on worker threads and
  batch-unioned with it.  Internal voids (sealed under overlapping
  blades) are separate negative-volume bodies and are dropped with the
  crumbs, i.e. filled.
- **Base**: ∪ exact 1 mm slab box ∪ DB base (peg height from the tallest
  point), `simplify(0.005 mm)`, keep the largest body, `stl_safe`, export.

## 3. Parameters (module constants, `grasslab.py`)

| Stage | Constants (value) |
|---|---|
| grids | `SQUARE_MM` 35, `FIELD_RES_MM` 0.25, `VOX_MM` 0.12, `VOX_1X1_MM` 0.10, `MARGIN_MM` 10 |
| 1 hills | `HILL_RELIEF_MM` 7.0, `GROUND_EDGE_MM` 3.4, `HILL_SIGMA_LARGE_MM` 10, `HILL_SIGMA_MID_MM` 5, `HILL_MID_WEIGHT` 0.35, `HILL_PLAIN_FRAC` 0.4, `HILL_PLAIN_SOFT` 0.25, `EDGE_MARGIN_MM` 9, `MAX_GROUND_SLOPE_DEG` 34, `USABLE_SLOPE_DEG` 12 |
| 2 flow | `COHERENCE_MM` 15, `THETA_STD_DEG` 60, `TERRAIN_FOLLOW` 0.3, `MEANDER_WAVELENGTH_MM` 15, `MEANDER_AMP_DEG` 10–30, `MEANDER_AMP_SIGMA_MM` 12, `MEANDER_PHASE_SIGMA_MM` 14 |
| 3 swaths | `SWATH_SPACING_MM` 11, `SWATH_LATERAL_ASPECT` 0.65, `SWATH_WARP_MM` 2.5, `SWATH_WARP_SIGMA_MM` 3, `SWATH_AMP_MM` 1.2–2.2, `ENV_FLOOR` 0.25, `ENV_EXP` 1.6, `ENV_LATERAL_EXP` 0.6 |
| 4 lenses | `SOIL_LENSES_PER_SQ` 1.3–1.8, `SOIL_LENS_CANDIDATES` 80, `SOIL_LENS_SCORE_NOISE` 0.8, `SOIL_LENS_HALF_LEN_MM` 5–9, `SOIL_LENS_ASPECT` 0.45–0.65, `SOIL_LENS_SEP_MM` 13, `SOIL_RIM_KEEP_MM` 7, `SOIL_LENS_WARP_MM` 0.8, `SOIL_LENS_WARP_SIGMA_MM` 1.2, `SOIL_EDGE_MM` 0.5 |
| 4 wrap | `DEFLECT_BAND_MM` 1.5–4.5, `DEFLECT_STRENGTH` 0.55–1.0, `PARTING_PUSH` 0.6, `PARTING_SOFT_MM` 0.8 |
| 4 dip | `SOIL_DIP_MM` 0.4, `SOIL_DIP_SOFT_MM` 0.8 (texture: `dirtlab.dirt_relief`) |
| 4b rim | `RIM_NOISE_SIGMA_MM` 8, `EDGE_FLOW_MM` 9–15, `EDGE_OK_DEG` 70–88, `EDGE_PULL` 0.15–0.45, `RIM_ALONG_MM` 3, `STEER_SIDE_SIGMA_MM` 20, `STEER_SIDE_HARD_MM` 2.5, `STEER_SIDE_SOFT_MM` 8, `STEER_NATURAL_DEG` 30, `RIM_SQUARE_PULL` 0.85, `RIM_SQUARE_REACH` 1.0, `PULL_SOFT_RAD` 0.35, `STEER_CORNER_MM` 2, `STEER_SMOOTH_MM` 1.5 |
| 5 strands | `STRAND_SPACING_MM` 2.65, `STRAND_LEN_MM` 15–30, `STRAND_BACK_FRAC` 0.2–0.6, `STRAND_MARGIN_MM` 8, `STRAND_GAP_FRAC` 0.5, `STRAND_LATERAL_MM` 0.3, `TRACE_STEP_MM` 0.25 |
| 5 fronds | `FROND_LEN_MM` 3.5–6.5, `FROND_HALF_W_MM` 0.8–1.25, `FROND_H_MM` 1.5–2.0, `FROND_MIN_LEN_MM` 1.5, `FROND_DELTA_DEG` 4, `STRAY_FRAC` 0.05, `STRAY_DELTA_DEG` 20–35, `SPRIG_PROB` 0.10, `SPRIG_LEN_MM` 1.8–3.0, `SPRIG_HALF_W_MM` 0.5, `SPRIG_H_MM` 0.6 |
| 5a strokes | `LINE_W_MM` 0.45, `LAYER_MM` 0.2, `KEEL_LINES` 2, `TOP_LINES` 1, `MAX_LINES` 2–4, `LENS_EXP` 0.6, `RAKE_MM` 0.2, `TIP_STEP_MM` 0.45, `RISE_EXP` 2.0 |
| 5a smoothing | `BLADE_VOX_MM` 0.05, `SMOOTH_CLOSE_MM` 0.3, `SMOOTH_SIG_MM` 0.05, `SMOOTH_SIG_Z_MM` 0.15, `EMBED_MM` 0.3, `BLADE_FACES` 1500 |
| 5b endings, banks | `RIM_TIP_INSET_MM` 0.2–1.5, `SIDE_INNER_MM` 0.8, `SOIL_SPILL_MM` 0.8–3.2, `WISP_FRAC` 0.04, `WISP_SPILL_MM` 4.0, `BANK_REACH_MM` 4, `BANK_WANDER_DEG` 15, `BANK_STRAY_FRAC` 0.12, `BANK_H_SCALE` 0.65–1.15 |
| 6–7 mesh | `SIMPLIFY_TOL_MM` 0.005, `SLAB_BOX_MM` 1.0, `OVERHANG_DEG` 45 (audit counts n_z < −sin 48°), `DB_GRASS_BAND` 4.4–5.9 |

`build_grass(…, blocker=None, tuck_mm=0.5, side_low=1.0, side_mm=1.0)` is
the hook meadowlab and shorelab use for rock footprints (and water); with
`blocker=None` it is grasslab's own behaviour.  With `side_low < 1`, the
side factor side_low + (1 − side_low)·smoothstep(d_out/side_mm) at
distance d_out outside the blocker, falling to 0 over `SIDE_INNER_MM`
(0.8 mm) inside it, scales each blade's height at its tip, so grass laps
against an obstacle's foot instead of swallowing it.  It returns `Grass`
with `solid` (dirt ground ∪ blades, a Manifold), `T` (grass-top map) and
`H0` (seating ground).

## 4. Known quirks

- `WISP_FRAC` is drawn for every frond; it only matters where a spine
  reaches soil.
- Swaths no longer carry height (no turf mass), only the strand length
  scaling through s_b; their envelope code is unused but kept for the RNG
  stream and as the hook for a future turf base layer.
- Meshing ≈ 80 s of the ≈ 100 s build (3,300 blades, ≈ 24 ms each on 4
  threads); a 2 × 2 STL is ≈ 85 MB at 1.7M faces.

## 5. Superseded designs (lessons kept)

- **Height-field fronds with lips, ledges and a voxel backstop (rev 1–4,
  v1–v2g, retired 2026-10-29)**: each frond was a skirt (carrying the
  swath envelope) plus a domed blade soft-maxed into a mass map T, with
  column intervals for 45° tip lips and side ledges, a field F = max(T −
  z, intervals), an FDM voxel backstop (support filter, hole fill, sealed
  pocket fill, slicer-island removal) and marching cubes at 0.12 mm.
  Lessons:
  - A height field plus carved intervals never gave a visible undercut:
    neighbouring skirts walled the air under every tip, the walls sealed
    it, and the pocket fill then filled it (cross-sections showed it).
    Undercuts must be part of the blade's own construction.
  - A shape defined as a "theoretical ideal" (domes, knife edges, needle
    points) is not what prints: everything narrower than a line was
    dropped by the slicer, or mangled by "detect thin walls".  Define
    the blade in layers and lines instead.
  - The turf mass (skirts) existed only to hide the ground between
    blades; on a lone blade it read as a stair-stepped pad, and between
    blades it buried the undercuts.  Removed; blades stand on dirt.
  - Tried on the way and dropped: a 45° cone from the blade core (blunt
    front face; round sections at low heights), a hull of nested pointed
    sections (vertical keel post until the sections grew from the keel),
    a J-curve (concave) rise.
- **Free-standing envelope layer (rev 2, v1–v1e)**: T was seeded with the
  swath envelope, eased to 0.5× at the rim and cut at soil.  Result:
  rim bevel, carved holes, border steps ("clay", F19).  Lesson: no grass
  height outside a blade.
- **Swath-axis fronds with fan** (l_b·12° fan, ±4° jitter, κ ± 0.03/mm):
  superseded by streamline fronds; the flow, not the cell, carries
  direction.
- **Seam soil (v1e–v1k)**: lanes along swath seams (triple-junction
  chains, width from noise at the nearest seam pixel, slits < 2.2 mm
  closed).  Better than puddles, but unrelated to the flow; replaced by
  lenses the flow wraps.
- **Curvature field and sickle spines (v1j–v1k)**: κ = 0.24·tanh(1.6·n)
  /mm at σ 5.5 mm, bend concentrated toward the tip.  Coherent but still
  per frond: "commas" (F24) beside the lodged-grass photo.
- **Per-frond edge steering (v1i–v1j)**: steering each frond at its root.
  Superseded by steering the field; per-point side flips had created a
  starburst.
- **Global soft frond ends (v1h-a/b)**: fish scales everywhere.
- **Soil roughness and faceted pebbles**: retired for the shared dirt
  texture (2026-10-29).
- **v0**: random lens blades in a binary voxel volume; see REQUIREMENTS §7.2.

## 6. Iteration log (2026-10-27, chronological)

| Iter | Trigger | Change |
|---|---|---|
| v1a | self-check: **shark fins** (tips at the crest, vertical fronts), **roof shingles** (wide flat domes), mean top 4.26 mm (below the band) | tip droop, lip underside measured from the tip; narrower, denser fronds; dome exponent 0.55 → 0.9; edge level 3.0 → 3.3 |
| v1b | handed to Shawn | — |
| v1c | Shawn: "increase the hills a bit… a few patches of bare ground" | hills ≈ 4 mm; first bare-soil patches |
| v1d | Shawn: "slightly more hills, slightly more bare patches" | relief ≈ 5 mm, 30° cap, edge 4.0 mm; 4 patches (8.6%) |
| v1e | Shawn: "bare patches are terrible" (puddles, trench rims, bubble pebbles) | seam-lane soil, sharp banks with limited spill, trough, faceted pebbles, sprigs |
| v1f | Shawn: grass "chopped down like it was all made of clay" (rim bevel, soil walls, border steps) | grass made of fronds: skirts carry the envelope, T starts at H₀, boundaries by shortening, shortened fronds lower |
| v1g | Shawn: "more hills and bigger bare patches" | landforms σ 10/5 mm, 34° cap, 9 mm feather; wider lanes (12.8%) |
| v1h | Shawn: "bare patches have some weird artifacts" | bank-only soft ends (global version scalloped the tile, reverted), dome-faceted pebbles, blurred dip mask |
| v1i | Shawn: rim grass "shouldn't point in the other direction" | per-frond edge-flow steering |
| v1j | Shawn: "our grass is very straight… should be more curved, but not randomly"; rim too regular | curvature field + curved spines; per-frond steering and tip inset variation |
| v1k | Shawn: "more curves… not just one big sweep" | K_MAX 0.16 → 0.24; θ swing 35° → 60° at σ 25 → 15 mm; swaths 13 → 11 mm |
| v2a | Shawn: lodged-grass photo → "go with the flow-following ribbons" | in-phase meandering field, soil lenses that deflect the flow, rim steering on the field, strands of streamline fronds |
| v2b | self-check: rim starburst, herringbone upstream of lenses, soil 5% | smoothed scalar rim rotation with smoothed turn side; tanh parting; lenses ≈ 10%; edge 4.2 → 3.9 mm |
| v2c | Shawn: "at miniature scale the blades should be shorter… still like that meandering look" | fronds 7–12 → 4–7 mm, narrower and lower; strands 3.0 → 2.3 mm |
| v2d | Shawn: "split the difference" | fronds 5.5–9.5 mm, half-width 0.55–0.9, h_f 0.65–1.0; strands 2.65 mm |
| v2e | Shawn: bank "vertical wall" should be a pile of blades | side ledges with 45° undercuts, opened under visible ledges; bank wander/stray/height scale; per-lens deflection band and strength |
| v2f | Shawn: hills should rise from a consistent perimeter | rises only; edge 3.9 → 3.4 mm (mean top 5.34 mm); usable 88% → 78% |
| v2g | meadow seed sweep: long feathered V seams and a corner starburst at the rim (meadow s2/4/5) | rim turn side chosen once per stretch of the rim loop (σ 20 mm, across corners) instead of pointwise; square-up and no run-alongside around a flip; pull for outward headings only; nearest-edge corners (§2.2.5) |
| — | refactor (no geometry change; all outputs byte-identical) | stages split (`Surface`, `plan_fronds`, `draw_frond`, `sculpt_frond`, `open_ledges`); constants grouped by stage; meshing retry on non-manifold marching cubes (previously a silent empty STL, seen in meadowlab) |


### Export guarantees (2026-10-27)

- **Exact slab box**: the marching-cubes solid is unioned with an exact
  box ([0, W] × [0, H] × [0, 1 mm], inside the ground, which sits at the
  3.4 mm edge level or higher) before the DB base.  The base must meet a
  clean planar face.  Meeting the MC bottom directly left 4 sliver edges
  at the peg-groove apexes under the tile seams, and they collapsed on
  STL reload.
- **`stl_safe`**, run before every export:
  1. `dharmatiles.stone.solidify.separate_pinches` nudges coincident but
     unconnected vertices apart.
  2. The mesh is merged exactly as a slicer does (float32 positions,
     merge by position).
  3. Zero-volume components (back-to-back union "fins") are dropped.
  4. It **raises** unless the result is one watertight body.
  
  Verified on seeds 1–4.  Before this, grass seed 3 would have exported
  a non-watertight file without any warning.


## Exposed ground = dirt (2026-10-29; written for the v2 height-field grass)

Shawn: dirt should be the texture wherever ground is exposed, across all
labs, and mud and other dirt textures are eliminated.  This lab now
uses `dirtlab.dirt_relief(seed, W, H)`: dirtlab's soil roughness, clods,
grit and pebbles, without its hills or big stones, as a relief offset
sampled onto this lab's grid.
- `Surface`: fronds still seat on plain H0 (the old soft soil dip is
  kept), so the grass look is unchanged.  T starts at, and is floored to,
  **H0 + dirt**, so dirt shows on bare patches and in the gaps between
  fronds.
- The old soil roughness and pebbles are retired.  `soil_relief` is still
  called, and its output discarded, only to keep the RNG stream and so
  the grass layout identical.
- The dirt seed is derived from the ground field, with no RNG draw.
- Meadowlab and shorelab inherit all of this.
- **Carved gaps stay open (2026-10-29)**: the dirt floor was re-applied
  after `open_ledges`, and dirt bumps refilled the 0.6 mm air gaps under
  tips and side ledges.  On shore s1, gapped tip columns fell from 40% to
  20%.  Carved columns now floor to plain H0 (shadowed ground under a
  ledge); everywhere else floors to H0 + dirt.  Restored to 40% (s1) and
  33% (s3), with 0 slicer islands.

## v3: stroke blades (2026-10-29)

Driven by Shawn inspecting single blades (`blades_demo.py`) in MeshLab and
Bambu Studio:

| Step | Trigger (Shawn) | Change |
|---|---|---|
| 1 | "there is no undercut" (vertical wall under the tip) | sections showed the 45° wedge built, then sealed by the blade's own solid dome edges + pocket fill; a 45° cone underside from the blade core |
| 2 | "same pointiness as the tip… like a keel"; "every cross section… ends in a point" | pointed planform (ogive tip); a keel strip; hull of nested pointed sections grown from the keel |
| 3 | "base rise… more of a curve" → "convex" | crest 1 − (1 − u/L)² |
| 4 | "what is going on at the base" → "why do we ever want the pad?" → "blades on dirt, no turf mass" | skirts removed everywhere |
| 5 | Bambu: "the top of the blade doesn't fill the volume we define"; thin-wall detection worse | blades defined in the printer's units: layers × whole lines (§2.7) |
| 6 | "sharper on top"; "overhangs right to the last layer" | 1-line top layer; rake every layer |
| 7 | "resin printable… SMOOTH the resulting blade" | loft through the layer outlines (§2.8) |
| 8 | "make that the … only way to generate grass blades" | the height-field renderer, lips/ledges, voxel backstop and shorelab's grass overrides deleted; shore blade sizes are the defaults for every lab |
