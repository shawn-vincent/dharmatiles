# grasslab — Sculpted Grass Tile: Design

*Revision 4, 2026-10-28 (code v2g).  Implements `REQUIREMENTS.md` rev 3.
Code: `grasslab/grasslab.py`; section numbers below match its stage
banners.  Superseded designs and their lessons: §5; iteration log: §6.*

---

## 1. Core idea: top-down, made of fronds, plus intervals

```
 1 H₀ hills (rises only) ──► 2 flow θ: regional angle + in-phase meanders + downhill pull
                                   │
          3 SWATH cells (warped anisotropic Voronoi) → wave height envelope E(s, l)
                                   │
 4 soil LENSES (flow-aligned eyes) → flow wraps each lens → rim steering → final flow
                                   │
 5 STRANDS of roots on long streamlines → each FROND traced along the flow:
       skirt (carries E) + domed blade  ──soft-max──►  mass map T(x, y)   (height field)
       tip lip, bank side ledge          ──────────►  column intervals [bottom, top], 45° undersides
                                   │
 6 F(x, y, z) = max(T − z, intervals), clamped ±2 vox → FDM backstop → marching cubes
                                   │
 7 ∪ DB socket base (manifold3d) → simplify 0.015 mm → keep largest body → STL
```

Three guarantees drive the layout:

- **Undercuts only where designed.**  The mass T is a height field, so it
  has no overhangs.  The only non-height-field geometry is the intervals
  (tip lips, side ledges), each with a 45° underside measured from its own
  crest.  REQ-FDM-1/3 and REQ-FR-4 follow from the geometry; the support
  filter is a backstop whose changes are reported.
- **Made of fronds.**  T starts at bare ground; every bit of grass height
  is some frond's skirt or blade, so every boundary (rim, soil, swath
  border) is frond outlines (REQ-EDGE-1…5).
- **No terracing.**  Marching cubes runs on a *continuous* field (T − z is
  linear in z), so surfaces interpolate exactly between samples (REQ-GR-2).

Grids: ground, flow and swaths on a 0.25 mm grid extended 12 mm (ground)
or 10 mm (swaths) past the tile; grass on a 0.12 mm voxel grid (0.1 mm at
1 × 1) with column centres at (i + ½)·vox, so walls land on the tile edge.

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

### 2.3 Swath envelope (`Swaths`)

Swaths now supply **height only**; frond headings come from the flow.

- **Sites**: Poisson-disk at 11 mm over the tile + 10 mm margin.  Axis dᵢ =
  the pre-wrap flow (step 3) at the site.
- **Ownership**: argmin of ‖(Δ·dᵢ), (Δ·d⊥ᵢ)/0.65‖ + 2.5 mm·warp(x, y)·φᵢ,
  with warp smooth noise at σ 3 mm and a per-site phase φᵢ ∈ U(−1, 1) (a
  common warp would shift every distance equally and bend nothing).
  Every point has one owner (REQ-SW-1); borders are irregular (REQ-SW-4).
- **Cell coordinates** s = (a − a_min)/(a_max − a_min) ∈ [0, 1],
  l = b/|b|_max ∈ [−1, 1], normalised per cell on the 0.25 mm grid.
- **Envelope**: E(s, l) = Aᵢ·(0.25 + 0.75·s^1.6)·(1 − l²)^0.6, Aᵢ ∈
  U(1.2, 2.2) mm: low upstream and at the sides, cresting downstream
  (REQ-SW-2).

### 2.4 Soil lenses and soil relief (`place_lenses`, `Lenses`, `soil_relief`)

- **Placement**: N = round(U(1.3, 1.8) × squares), at least 1.  80
  candidates inside a 7 mm rim inset, scored by −z-score(ground height) +
  N(0, 0.8), accepted best-first ≥ 13 mm apart.  Typical coverage ≈ 10%.
- **Shape**: axis = pre-wrap flow at the centre; half-length U(5, 9) mm,
  half-width × U(0.45, 0.65).  Lens metric f = |a|/ℓ + (b/w)², which gives
  pointed ends (an eye); approximate distance d = (f − 1)·w/2 plus a shared
  ±0.8 mm border warp (σ 1.2 mm).  Mask = smoothstep(0.5 − d/0.5 mm).
- **Grass response**: the mask decides only where fronds root and how far
  they spill (§2.6); it never touches T.
- **Relief** (added to H₀ on the grass grid):
  - trough −0.4 mm·m_s and roughness 0.08 mm·n(σ 0.4 mm)·m_s, where m_s
    is the mask blurred at σ 0.8 mm, so no step outlines the patch;
  - **pebbles**: Poisson count at 0.06 per mm² of soil, centres where
    m > 0.8, radius 0.45 + 0.75·U² mm, ≥ 1.2 mm + r + r′ apart.  Each is a
    low dome (height 0.5–0.7 r, elongated 1–1.5×) cut by 7–10 planes at
    30–42° with offsets 0.75–1.0 r, multiplied by the mask.

### 2.5 Strands and spines (`make_strand_roots`, `plan_fronds`)

- **Strands**: Poisson seeds at 2.65 mm over the tile + 8 mm margin.  Each
  strand is U(15, 30) mm long and starts U(0.2, 0.6) of its length
  upstream of its seed (traced backward), then is traced forward.
- **Roots** every 0.5·L·U(0.8, 1.2) along the strand, jittered ±0.3 mm
  laterally, with L = U(5.5, 9.5) mm; only roots inside the tile are kept.
- **Per root**: swath owner and (s_b, l_b); **bank** = within 4 mm of soil
  and not in it.  Heading offset: strays (5%, or 12% for bank roots) get
  ±U(20°, 35°), others U(±4°); bank roots add U(±15°) wander.
- **Spines**: one batched RK2 trace per root along the final flow, at
  0.25 mm unit-speed steps, rotated by the root's offset, to
  9.5·1.1 + 0.45 + 0.5 mm.  Seams between strands come from the 2.65 mm
  spacing against skirt half-widths of 1.2–2.0 mm: skirts merge into a
  continuous mass, blade crests stand as separate ribbons.

### 2.6 Frond draw and shortening (`draw_frond`)

- **Skips**: roots under an obstacle (meadowlab's rocks); roots in soil
  become **sprigs** with 10% probability (L U(1.8, 3.0), half-width 0.5,
  height 0.5, no skirt), otherwise are dropped.
- **Dimensions**: L = L_strand·(0.8 + 0.3·s_b) (4.4–10.45 mm, longer
  toward the crest); half-width w_max U(0.55, 0.9); blade height h_f
  U(0.65, 1.0); skirt half-width 2.2·w_max.  Bank fronds: h_f × U(0.65, 1.15).
- **Per-frond endings**: rim tip inset 0.2 + 1.3·U² mm; soil spill
  0.8 + 2.4·U² mm, or 4 mm for wisps (4%).
- **Shortening** (never slicing): the spine stops at the first sample
  inside the rim inset, deeper than the spill into soil (EDT), or deeper
  than `tuck_mm` into an obstacle.  L becomes that arc length minus the tip
  cap; fronds shorter than 1.5 mm are dropped.  Shortened fronds are lower:
  k_h = clip(L/L₀, 0.3, 1)^0.8 scales h_f and the skirt (REQ-EDGE-5).
- **Soft ends** (bank fronds only, REQ-EDGE-6): *tip_soft* if the spine was
  stopped by soil; *base_soft* if rooted within 2.5 mm of soil.

### 2.7 Frond body (`frond_spine`, `sculpt_frond`)

- **Spine** = traced path to L_t + 0.3 mm (every second sample) plus a
  straight cap of length c_b + 0.3 behind the root, c_b = 0.6·skirt width.
  Columns are projected onto the polyline: arc length u, signed offset v.
- **Blade planform**: w(u) = w_tip + (w_max − w_tip)·sin(π(0.25 + 0.75·u/L)),
  w_tip = 0.45 mm, widest at L/3; past u = L a round cap of radius w_tip;
  L_t = L + w_tip.
- **Skirt planform**: ws(u) = ws_max·clip(1 − ((u − u_c)/u_h)², 0, 1)^0.6,
  a lens from −c_b to L_t.
- **Skirt height**: k_h·h_scale·E(s(u), l_b)·taper·(1 − q_s³)^1.5, with
  s(u) = min(s_b + u/len_i, 1) (the frond rides its own swath's envelope
  past the border: shingled leading edges, REQ-SW-3), q_s = |v|/ws, and
  taper = 1 − 0.45·smoothstep((u − L/2)/(L_t − L/2)).  tip_soft multiplies
  by a ramp to 0 over the last min(2.2, 0.4·L_t) mm; base_soft ramps the
  base cap up over 1.5·c_b.
- **Blade height**: h_f·rise(u)·(1 − q²)^0.9, q = |v|/w.  rise is a
  smoothstep from 0 to 1 by 0.45 L (flush base), then **droops** from
  0.55 L to (1 − 0.35) at L_t, so the frond arches over instead of ending
  in a fin.
- **top** = H₀ + skirt + blade.  Mass columns contribute
  T ← smoothmax(T, top, k = 0.12 mm), which rounds groove bottoms and
  keeps crests crisp.
- **Tip lip**: blade columns with u ≥ L_t − min(2.0, 0.35·L).  Bottom =
  tip_crest − 0.6 − (L_t − u), a 45° plane measured back from the drooped
  crest (ground at the tip + skirt at the tip + h_f·rise(L_t)).  Columns at
  least 0.6 mm thick become intervals; thinner side columns stay in the
  mass (dropping them sawtoothed the tips).
- **Bank side ledge** (REQ-FL-6), bank fronds only:
  - A column *faces soil* when the outward lateral direction (spine
    tangent ⊥ · sign v) · (−∇soil_dist) > 0.3 and it lies within 6 mm of
    soil.  Skirt columns beyond the blade on the facing side are dropped,
    so nothing fills under the ledge.
  - Facing blade columns with e = w − |v| < 0.7 mm (and not lip) become a
    ledge interval: top = the normal frond top, bottom = top − 0.6 − e, a
    45° underside ≥ 0.6 mm thick that fuses to the recessed body face at
    e = 0.7 mm.  (Measuring from the skirt height at the blade edge instead
    dug holes to bare ground.)
- **Opening the undercuts** (`open_ledges`): neighbouring skirts fill in
  under almost every ledge, so after all fronds, T is cut down to the ledge
  bottom wherever the ledge top is the surface (top ≥ T − 0.05), and those
  columns become intervals: a wedge, never a slot.

### 2.8 Field, FDM backstop, meshing (`make_field`, `fdm_backstop`, `mesh_field`)

- F = clamp(max(T − z, min(z − bottom, top − z) per interval), ±2·vox) up
  to max height + 0.3 mm, in float32.
- **Backstop** on occupancy F > 0: fill holes (open at the top), then up
  to four rounds of a bottom-up support filter (each voxel needs a
  4-neighbour or itself below: 45° at one voxel per layer) and bed
  connectivity (components touching z = 0).  Changed voxels get F = ±vox.
  Counts are reported; ≈ 1–3k voxels of ~3·10⁷, mostly sealed pockets
  filled.
- **Meshing**: pad with air, marching cubes at level 0, trimesh vertex
  merge, orient outward, convert to manifold3d.  If manifold3d rejects the
  mesh (coincident vertices from field values ≈ 0 at grid nodes, merged
  into pinched edges), values with |F| < 10⁻³·vox are pushed to ±10⁻³·vox
  and the field is re-meshed once.  Valid meshes never take this path.
- **Base**: ∪ DB base (peg height from the tallest point),
  `simplify(0.015 mm)`, keep the largest body, report crumbs, export.

## 3. Parameters (module constants, `grasslab.py`)

| Stage | Constants (value) |
|---|---|
| grids | `SQUARE_MM` 35, `FIELD_RES_MM` 0.25, `VOX_MM` 0.12, `VOX_1X1_MM` 0.10, `MARGIN_MM` 10 |
| 1 hills | `HILL_RELIEF_MM` 7.0, `GROUND_EDGE_MM` 3.4, `HILL_SIGMA_LARGE_MM` 10, `HILL_SIGMA_MID_MM` 5, `HILL_MID_WEIGHT` 0.35, `HILL_PLAIN_FRAC` 0.4, `HILL_PLAIN_SOFT` 0.25, `EDGE_MARGIN_MM` 9, `MAX_GROUND_SLOPE_DEG` 34, `USABLE_SLOPE_DEG` 12 |
| 2 flow | `COHERENCE_MM` 15, `THETA_STD_DEG` 60, `TERRAIN_FOLLOW` 0.3, `MEANDER_WAVELENGTH_MM` 15, `MEANDER_AMP_DEG` 10–30, `MEANDER_AMP_SIGMA_MM` 12, `MEANDER_PHASE_SIGMA_MM` 14 |
| 3 swaths | `SWATH_SPACING_MM` 11, `SWATH_LATERAL_ASPECT` 0.65, `SWATH_WARP_MM` 2.5, `SWATH_WARP_SIGMA_MM` 3, `SWATH_AMP_MM` 1.2–2.2, `ENV_FLOOR` 0.25, `ENV_EXP` 1.6, `ENV_LATERAL_EXP` 0.6 |
| 4 lenses | `SOIL_LENSES_PER_SQ` 1.3–1.8, `SOIL_LENS_CANDIDATES` 80, `SOIL_LENS_SCORE_NOISE` 0.8, `SOIL_LENS_HALF_LEN_MM` 5–9, `SOIL_LENS_ASPECT` 0.45–0.65, `SOIL_LENS_SEP_MM` 13, `SOIL_RIM_KEEP_MM` 7, `SOIL_LENS_WARP_MM` 0.8, `SOIL_LENS_WARP_SIGMA_MM` 1.2, `SOIL_EDGE_MM` 0.5 |
| 4 wrap | `DEFLECT_BAND_MM` 1.5–4.5, `DEFLECT_STRENGTH` 0.55–1.0, `PARTING_PUSH` 0.6, `PARTING_SOFT_MM` 0.8 |
| 4 relief | `SOIL_DIP_MM` 0.4, `SOIL_DIP_SOFT_MM` 0.8, `SOIL_ROUGH_MM` 0.08, `SOIL_ROUGH_SIGMA_MM` 0.4, `PEBBLE_DENSITY` 0.06, `PEBBLE_R_MM` 0.45–1.2, `PEBBLE_GAP_MM` 1.2 |
| 4b rim | `RIM_NOISE_SIGMA_MM` 8, `EDGE_FLOW_MM` 9–15, `EDGE_OK_DEG` 70–88, `EDGE_PULL` 0.15–0.45, `RIM_ALONG_MM` 3, `STEER_SIDE_SIGMA_MM` 20, `STEER_SIDE_HARD_MM` 2.5, `STEER_SIDE_SOFT_MM` 8, `STEER_NATURAL_DEG` 30, `RIM_SQUARE_PULL` 0.85, `RIM_SQUARE_REACH` 1.0, `PULL_SOFT_RAD` 0.35, `STEER_CORNER_MM` 2, `STEER_SMOOTH_MM` 1.5 |
| 5 strands | `STRAND_SPACING_MM` 2.65, `STRAND_LEN_MM` 15–30, `STRAND_BACK_FRAC` 0.2–0.6, `STRAND_MARGIN_MM` 8, `STRAND_GAP_FRAC` 0.5, `STRAND_LATERAL_MM` 0.3, `TRACE_STEP_MM` 0.25 |
| 5 fronds | `FROND_LEN_MM` 5.5–9.5, `FROND_HALF_W_MM` 0.55–0.9, `FROND_H_MM` 0.65–1.0, `FROND_TIP_HALF_W_MM` 0.45, `FROND_MIN_LEN_MM` 1.5, `FROND_DELTA_DEG` 4, `STRAY_FRAC` 0.05, `STRAY_DELTA_DEG` 20–35, `SPINE_PAD_MM` 0.3, `SPRIG_PROB` 0.10, `SPRIG_LEN_MM` 1.8–3.0, `SPRIG_HALF_W_MM` 0.5, `SPRIG_H_MM` 0.5 |
| 5a body | `DOME_EXP` 0.9, `DROOP` 0.35, `SKIRT_FACTOR` 2.2, `SKIRT_BASE_CAP` 0.6, `SKIRT_TIP_DROP` 0.45, `LIP_MAX_MM` 2.0, `LIP_FRAC` 0.35, `LIP_MIN_T_MM` 0.6, `SOFTMAX_K_MM` 0.12 |
| 5b endings, banks | `RIM_TIP_INSET_MM` 0.2–1.5, `SOIL_SPILL_MM` 0.8–3.2, `WISP_FRAC` 0.04, `WISP_SPILL_MM` 4.0, `SOIL_BASE_SOFT_MM` 2.5, `SKIRT_END_MM` 2.2, `BANK_REACH_MM` 4, `BANK_FACE_MM` 6, `BANK_WANDER_DEG` 15, `BANK_STRAY_FRAC` 0.12, `BANK_H_SCALE` 0.65–1.15, `SIDE_LIP_W_MM` 0.7 |
| 6–7 mesh | `SIMPLIFY_TOL_MM` 0.015, `OVERHANG_DEG` 45 (audit counts n_z < −sin 48°), `DB_GRASS_BAND` 4.4–5.9 |

`build_grass(…, blocker=None, tuck_mm=0.5, side_low=1.0, side_mm=1.0)` is
the hook meadowlab uses for rock footprints; with `blocker=None` it is
grasslab's own behaviour.  With `side_low < 1`, every frond's skirt and
blade height is scaled by side_low + (1 − side_low)·smoothstep(d_out/side_mm)
at distance d_out outside the blocker, and falls to 0 over `SIDE_INNER_MM`
(0.8 mm) inside it, so grass laps against an obstacle's foot instead of
swallowing it.

## 4. Known quirks (kept: changing them changes the approved output)

- The tip-lip crest uses k_h·E for the skirt part but not the bank height
  scale h_scale, which the skirt itself does get.  For bank fronds the lip
  underside reference is therefore slightly off the real crest (thinner
  lips when h_scale < 1, thicker when > 1).
- `WISP_FRAC` is drawn for every frond; it only matters where a spine
  reaches soil.

## 5. Superseded designs (lessons kept)

- **Free-standing envelope layer (rev 2, v1–v1e)**: T was seeded with the
  swath envelope, eased to 0.5× at the rim and cut at soil.  Result:
  rim bevel, carved holes, border steps ("clay", F19).  Lesson: no grass
  height outside a frond body.
- **Swath-axis fronds with fan** (l_b·12° fan, ±4° jitter, κ ± 0.03/mm):
  superseded by streamline fronds; the flow, not the cell, carries
  direction.
- **Seam soil (v1e–v1k)**: lanes along swath seams (triple-junction
  chains, width from noise at the nearest seam pixel, slits < 2.2 mm
  closed).  Better than puddles, but unrelated to the flow; replaced by
  lenses the flow wraps.
- **Curvature field and sickle spines (v1j–v1k)**: κ = 0.24·tanh(1.6·n)
  /mm at σ 5.5 mm, bend concentrated toward the tip.  Coherent but still
  per frond: "commas" (F24) beside the lodged-grass photo.  The spine
  projection (`project_spine`) survived and is used for streamlines.
- **Per-frond edge steering (v1i–v1j)**: steering each frond at its root.
  Superseded by steering the field; per-point side flips had created a
  starburst.
- **Global soft frond ends (v1h-a/b)**: fish scales everywhere; now bank
  fronds only.
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


## Exposed ground = dirt (2026-10-29)

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
