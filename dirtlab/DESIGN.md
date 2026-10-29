# dirtlab — Plain Bare Earth: Design

*Revision 1, 2026-10-29 (code v1g).  Implements `REQUIREMENTS.md` rev 1.
Code: `dirtlab/dirtlab.py`; section numbers match its stage banners.*

---

## 1. Core idea: one height field, built in scale order

Everything except the few larger stones is **one height field** z(x, y)
on a 0.15 mm grid (467 × 467 for 2 × 2).  A height field cannot
overhang, so the earth and the small/medium pebbles are supportless
by construction (REQ-FDM-1).  Each scale is a separate stage, and stages
combine by **soft max** (polynomial smooth max, width k), not by summing:
a lump on the ground is a new surface that wins where it is higher, with
a fillet of width k where it meets.  Summed bumps give porridge (D1);
soft-maxed shapes keep their identity and get soil-like creases.

```
ground  = EDGE + lift·w_tex + undulation + w_tex·soil_mid
soil    = smax(ground, clods)              (§2.2)
z       = soil + w_tex·grit  [+ cracks]    (§2.3, §2.6)
z      += stone soil banks; stones seated  (§2.5)
z       = smax(z, pebbles) + foot mounds   (§2.4)
z       = max(z, EDGE); z(rim) = EDGE      (rises only)
mesh    = uniform heightfield solid ∪ 1 mm slab box ∪ stones ∪ DB base
```

`w_tex` ramps 0 → 1 between 0.4 and 2.0 mm from the edge, so the
outermost 0.4 mm is exactly at `EDGE_MM` (REQ-H-1).

## 2. Stages

### 2.1 Ground (`ground_undulation`)

Two Gaussian-filtered white noises (σ 10 mm + 0.35 × σ 4 mm).  As grasslab
`make_ground`: subtract the 35% quantile, soft-floor at 0 (plains), scale
the peak to U(0.9, 1.6) mm, fade with a cosine ramp over 6 mm from the
rim, cap the slope at 14°.  A low fBm (2.2 mm, ±0.07 mm) adds broad soil
roughness so tamped ground isn't glassy.

### 2.2 Clods (`clod_layer`)

- **Patchiness**: a σ 6 mm noise field → density 0…1 (smoothstep).
  Candidate clods are rejected with probability 1 − density: cloddy
  patches and tamped patches (REQ-E-3).
- **Placement**: Poisson(0.7/mm² × area) candidates, largest first,
  accepted when ≥ 0.6 × (r₁ + r₂) from placed ones (a spatial hash).
  Radius 0.35–1.5 mm skewed small (U^2.2).  Mixed sizes plus partial
  overlap avoid a packed cell pattern (D3).
- **Shape** (crumbs, not blisters, REQ-E-2): 1–3 lobes (main + satellites
  at 45–80% size, 0.4–0.9 r away), each an ellipse (aspect 1–1.7) with
  angular radius harmonics k = 2, 3, 5 (σ 0.22/√k) and profile
  h·(1 − ρ^2.6)^0.8 (flattish top, soft shoulder).  Height 0.12–0.3 r,
  ≤ 0.45 mm; the whole clod is tilted (σ 0.05 mm/mm) and starts
  U(0, 0.5)·h below the ground (partly sunk).  Lobes and clods soft-max
  with k = 0.06 mm.

### 2.3 Grit (`grit`)

`dharmatiles.stone.noise.fbm`, 2 octaves at 0.45 mm, ±0.045 mm, plus
0.12/mm² sand grains (radius 0.15–0.3 mm, 0.05–0.12 mm high, max-combined).

### 2.4 Pebbles (`place_pebbles`, `pebble_shape`)

- **Clusters**: 4–7 Gaussian blobs (σ 3–7 mm) on a 0.25 floor, × (1 +
  0.8 × lowness of the 3 mm-blurred ground), normalised; darts are
  accepted with that probability (REQ-P-3).
- **Counts and sizes**: 5–9 medium (2–4 mm) and 45–75 small (0.5–1.5 mm)
  per square, skewed small (U^1.8); medium placed first; 60 darts each,
  ≥ 0.25 mm gap; no pebble inside a stone's footprint.
- **Shape**: a flattened cap hp·√(1 − ρ^p), p ∈ U(2, 3.5) (2 = ellipsoid,
  higher = slab-like); height 0.25–0.45 D, footprint aspect 1–1.9.
  - *Faceted* (75%): the min of K tangent planes of the cap (6–11 medium,
    4–7 small) at stratified random points, each tilted (σ 12°), lowered
    by 0.03–0.18 hp and capped at 65°.  rocklab's one-facet-per-seed idea
    in 2.5D: crisp ridges, mixed facet sizes.
  - *Worn* (25%): the cap itself with K/3 shallow flats.
  - *Polygonal skirt*: 5–8 side planes at 58°, 0.82–1.05 a out, so the
    outline is angular, not a disc (D2), and no side is a vertical,
    grid-aliased wall.
- **Seating** (REQ-P-4): base = the 0.5 mm-blurred surface at the centre
  minus U(0.25, 0.45)·hp; soft max into the soil with k = 0.08 mm (small)
  / 0.22 mm (medium), so soil fillets the foot.  Medium pebbles first get
  a soil heap (0.2 hp Gaussian, 1.25 × radius).  `Field.pebble` marks the
  cells where a stone won (used for paint).

### 2.5 Stones (`make_stones`, `bed_stones`)

2–4 rocklab rocks (`rl.make_rock`, D 4.5–7.5 mm skewed small), scaled to
0.65 in z (D5: rocklab's boulders read as upright teeth on dirt), random
yaw, dart-thrown ≥ 2 mm from the rim and ≥ 4 mm apart.  A 0.3 mm soil bank
(Gaussian beyond 0.7 R) is added to z; the rock's base goes to
min(lowest ground under 0.85 R − 0.25 mm, median − 0.4 h).  The z-scale
steepens downward facets, but they lie below the soil; measured overhang
is 0.00 mm².

### 2.6 Cracks (`crack_layer`, `--cracks`, off by default)

5 random-walk polylines per 2 × 2 (0.3 mm steps, heading σ 0.35 rad,
20–60 steps), carved as a 0.35 mm-wide, 0.25 mm-deep rounded V (from a
distance transform).  They stay within the 0.3 mm lift.

### 2.7 Tile (`assemble`)

`make_heightmap_solid(z, W, H, base_h=0, subsample=1)` (a uniform mesh:
waterlab found the adaptive mesher faceted smooth surfaces) ∪ an exact
1 mm slab box ∪ the stones (`batch_boolean`), then ∪ the DB base
(`make_base`, peg from `select_peg_height` on the top).  `stl_safe`
(grasslab's export guarantee, copied: separate pinches, float32 + merge,
keep the largest body, raise unless watertight).

### 2.8 Preview (`painted`, `contact_sheet`)

- Paint per face: earth ramp (5 browns) by 0.7 × relief over a 0.8 mm
  blur + 0.3 × relief over 4 mm (what a drybrush catches); stones and
  pebbles grey by facing and relief; walls dark.
- Views: painted, oblique (clay), top, low, close (20 mm, painted).
- `dharmatiles.render.render` (pyrender).  When it has no screen (a
  locked macOS session: pyglet sees zero screens), the sheet falls back to
  a matplotlib renderer of the finished tile's top surface (ray-cast from
  the mesh, so stones are included), Lambert-lit, surface only; captions
  then say "(fallback renderer)".

## 3. Parameters (module constants, `dirtlab.py`)

| Stage | Constants (value) |
|---|---|
| tile | `CELL_MM` 0.15, `EDGE_MM` 4.0, `LIFT_MM` 0.3, `SLAB_BOX_MM` 1.0, `RIM_FLAT_MM` 0.4, `RIM_TEX_MM` 2.0 |
| ground | `UND_SIGMA_MM` 10 / 4, `UND_MID_WEIGHT` 0.35, `UND_PLAIN_FRAC` 0.35, `UND_RELIEF_MM` 0.9–1.6, `UND_EDGE_MM` 6, `UND_MAX_SLOPE_DEG` 14, `SOIL_MID_MM` 2.2, `SOIL_MID_AMP_MM` 0.07 |
| clods | `CLOD_PATCH_SIGMA_MM` 6, `CLOD_PATCH_BIAS` 0.15, `CLOD_DENSITY` 0.7, `CLOD_R_MM` 0.35–1.5, `CLOD_R_EXP` 2.2, `CLOD_H_RATIO` 0.12–0.3, `CLOD_H_MAX_MM` 0.45, `CLOD_ASPECT` 1–1.7, `CLOD_LUMP` 0.22, `CLOD_OVERLAP` 0.6, `CLOD_SOFT_MM` 0.06, `CLOD_SUNK` 0–0.5, `CLOD_LOBES` 1–3, `CLOD_FLAT` 2.6, `CLOD_TILT` 0.05 |
| grit | `GRIT_MM` 0.45, `GRIT_AMP_MM` 0.045, `GRAIN_DENSITY` 0.12, `GRAIN_R_MM` 0.15–0.3, `GRAIN_H_MM` 0.05–0.12 |
| pebbles | `PEB_CLUSTERS_2X2` 4–7, `PEB_CLUSTER_SIGMA_MM` 3–7, `PEB_BACKGROUND` 0.25, `PEB_LOW_BIAS` 0.8, `SMALL_PER_SQ` 45–75, `SMALL_D_MM` 0.5–1.5, `MED_PER_SQ` 5–9, `MED_D_MM` 2–4, `PEB_D_EXP` 1.8, `PEB_ASPECT` 1–1.9, `PEB_H_RATIO` 0.25–0.45, `PEB_SINK` 0.25–0.45, `PEB_ROUND_PROB` 0.25, `PEB_FACETS` 6–11, `PEB_FACETS_SMALL` 4–7, `PEB_FACET_DROP` 0.03–0.18, `PEB_ROUND_DROP` 0.06–0.15, `PEB_MAX_FACET_DEG` 65, `PEB_FACET_TILT_DEG` 12, `PEB_FLAT` 2–3.5, `PEB_SKIRT_DEG` 58, `PEB_SKIRT_SIDES` 5–8, `PEB_GAP_MM` 0.25, `PEB_SOFT_MM` 0.08 / 0.22, `PEB_MOUND` 0.2 |
| stones | `STONES_2X2` 2–4, `STONE_D_MM` 4.5–7.5, `STONE_Z_SCALE` 0.65, `STONE_SINK` 0.4, `STONE_MOUND_MM` 0.3, `STONE_RIM_MM` 2, `STONE_GAP_MM` 4 |
| cracks | `CRACKS_2X2` 5, `CRACK_W_MM` 0.35, `CRACK_DEPTH_MM` 0.25 |

Measured (2 × 2, v1g): seed 1: 997 clods, 280 small + 28 medium pebbles
(79 worn), 3 stones; ground 4.00–5.81 mm (median 4.51); top 7.28 mm;
444,910 faces, 22.2 MB, one watertight body, overhang 0.00 mm², peg
5.7 mm, build 2.8 s (field ≈ 0.7 s, rest is meshing, CSG and the export
check).  Seeds 2 and 3 are similar (2.8–3.1 s, 0.00 mm²).  Byte-identical
STL across runs.

## 4. Iteration log

| Iter | Artifact / trigger | Fix |
|---|---|---|
| v1a | first build: clods were isolated round blisters on smooth ground (D2); small pebbles were sharp pyramids | — |
| v1b | | clods became 1–3 lumpy lobes with flat tilted tops (profile exponent 2.6), density 0.33 → 0.7/mm², overlap 0.75 → 0.6, height 0.18–0.4 → 0.12–0.3 r; pebble height 0.35–0.65 → 0.3–0.5 D.  Now reads as crumbly soil with tamped patches |
| v1c | medium pebbles were square "tent" pyramids | pebble = min of tangent planes of a cap (rocklab facet idea in 2.5D) |
| v1d | too many tiny planes: smooth round domes, clustered bubbles (D2) | 6–11 facets, tilt σ 12°, drop 0.03–0.18 hp, slab-like cap exponent 2–3.5, worn share 0.35 → 0.25 |
| v1e | slab caps ended in vertical aliased walls; pebbles looked pasted on | 58° skirt; foot mound 0.12 → 0.2 hp, medium fillet 0.18 → 0.22 mm |
| v1f | rocklab stones stood up like teeth (D5); pebbles still a bit round | stones × 0.65 in z, sink 0.3 → 0.4 h; pebble height 0.25–0.45 D, aspect up to 1.9 |
| v1g | circular skirt left disc outlines on worn pebbles (D2); sand grains a regular sprinkle in tamped areas (D7) | polygonal 5–8-sided skirt; grain density 0.25 → 0.12/mm² |

v1a–v1d were judged on pyrender renders; from v1e the macOS session was
locked (pyrender: no screen), so v1e–v1g were judged on the matplotlib
fallback, which is coarser (every 3rd sample on full-tile views).

## Round 2 (2026-10-29)

Shawn's feedback:
- more relief;
- no frame at the rim;
- pebbles that register at tabletop distance;
- stronger cloddy/smooth contrast;
- texture visible everywhere at 0.2 mm layers.  His slice of v1g showed
  large smooth plains.

| Change | Before → after |
|---|---|
| Rises (undulation) | σ (10, 4) → (14, 5), mid weight 0.35 → 0.15, edge fade 6 → 9 mm, slope cap 14 → 24°.  The rises were slope-capped to about 1.1 mm; now about 2.2–2.5 mm (ground up to about 7.5 mm). |
| Rim frame | flat band 0.4 → 0, texture ramp 2 → 1 mm |
| Clods | radius 0.35–1.5 → 0.45–2.4, height up to 0.75, h/r 0.16–0.35; patch contrast 1.0, with a **density floor of 0.3** so tamped ground still carries clods |
| Soil roughness | 0.07 mm @ 2.2 → 0.22 mm @ 1.6; grit 0.045 → 0.08 |
| Pebbles | medium 5–9 → 8–13 per square at 2.2–4.5 mm; stones 2–4 → 4–7; background 0.25 → 0.12 (sparser outside clusters) |

Texture metric (windows of 1.5 mm with relief < 0.2 mm): **32.8% → 1.9%**
(seed 1); seeds 2 and 3 about 2–2.6%.  Build about 3 s; overhang about
0 mm².
