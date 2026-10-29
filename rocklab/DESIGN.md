# rocklab — Faceted Rock Scatter: Design

*Revision 2, 2026-10-27 (code v1f).  Implements `REQUIREMENTS.md` rev 2.
Code: `rocklab/rocklab.py`; section numbers match its stage banners.*

---

## 1. Core idea: lumpy convex body, then planes

Every rock is built in `manifold3d` from half-space cuts of a convex
body, so it stays convex, grounded and printable at every step:

1. **Body** (`make_body`): convex hull of 75–145 points on a
   *non-increasing profile* above a flat base, with angle-only lumpiness.
2. **Facets** (`cut_facets`, scale 1): blue-noise seeds with mixed radii
   on the body; each seed's tilted tangent plane is one facet; the rock is
   the intersection of those half-spaces with a box on z = 0.
3. **Chips** (`chip_corners_edges`, scale 2): more, smaller half-space
   cuts at corners and along sharp edges.
4. **Grain** (`texture`, scale 3): fine displacement along vertex normals,
   faded at the base and halved at sharp edges.

Guarantees:

- **Convex, so grounded (REQ-ROCK-3)**: an intersection of half-spaces is
  convex; the planes come from a body whose cross-sections never grow
  upward, and no plane with n_z < −0.5 is used, so there is no mushroom.
- **Printable (REQ-FDM-1)**: no face points down past 30°; grain is zero
  on downward faces.  Measured overhang 0.00–0.02 mm².
- **Crisp edges (REQ-ROCK-2)**: planes meet at sharp edges; grain is halved
  within 0.5 mm of them.
- **Mixed facet sizes by construction**: one facet per seed, size from the
  seed spacing.
- Clusters are unions of convex rocks; a union's surface is a subset of
  its parts', so the guarantees carry over.

## 2. Stages

### 2.1 Rock mass (`make_body`)

- **Width** D = 6 + 9·U^1.4 mm (skewed small); footprint semi-axes a = D/2,
  b = a / U(1, 2), random yaw.
- **Height** h = min(D·U(0.45, 0.85), 9 mm); slabs × 0.6; at least 2.5 mm.
- **Profile family** (`profile`, radius scale r(t), t = z/h, always
  non-increasing):

  | Family | r(t) | Weight |
  |---|---|---|
  | dome | (1 − t^2.2)^0.5 | 0.35 |
  | slab | (1 − t^2.8)^0.45, h × 0.6 | 0.2 |
  | block | (1 − t^3.5)^0.4, superellipse (p = 3) footprint | 0.2 |
  | wedge | (1 − t)^0.8, centre shifts by U(0.2, 0.45) of the footprint toward a random side as t grows | 0.25 |

- **Points**: base ring of 14–22 at z = 0 (radius × U(0.94, 1.0)); 60–120
  body points at t ∈ U(0.05, 0.97) (× U(0.93, 1.0)); 1–3 top points at
  t ∈ U(0.95, 1.0) (× U(0, 0.4)).  Jitter only shrinks, so the profile stays
  non-increasing.
- **Lumpiness**: radius × (1 + Σ_{k=2..5} a_k cos(kφ + φ_k)), a_k ~ N(0, 1)·
  0.08/2.  It depends on **angle only**, so every vertical line is scaled
  by a constant and the profile stays non-increasing.  Why: flat body
  regions (old block sides, slab tops) gave all their seeds the same
  tangent plane, which merged them into one oversized face (R9).
- **Hull**: `Manifold.hull_points`.

### 2.2 Facets (`cut_facets`, scale 1)

- **Seeds**: dart-throw over 4,000 area-weighted surface samples of the
  body (seeded from the rock RNG; trimesh's own sampler would use the
  unseeded global RNG).  Each candidate draws r = r_min + (r_max − r_min)·
  U^1.6 and is accepted when ≥ (r + r_j)/2 from every accepted seed.
  Radii 0.9–2.5 mm × √(D/15): small rocks get fewer, proportionally
  coarser facets.  Base samples (n_z < −0.3) are skipped: the flat base is
  its own facet.
- **Plane**: the sample normal tilted by U(0, 14°) about a random tangent
  axis; planes with n_z < −0.5 are skipped; offset jitter U(−0.15, +0.1) mm
  along the normal.
- **Result**: 30–165 facets per rock; the facetted rock keeps about
  70–90% of the body's volume.

### 2.3 Chips (`chip_corners_edges`, scale 2)

- **Corners**: every polytope vertex above z = 0.6 mm, with probability
  0.45.  Normal = sum of incident face normals; depth U(0.3, 0.9)·√(D/15).
- **Edges**: every sharp edge (dihedral > 18°) with both ends above 0.6 mm,
  with probability 0.35, at a random point 20–80% along it.  Normal = the
  two faces' normal sum; depth U(0.2, 0.55)·√(D/15).
- Candidates are shuffled; each plane is tilted by U(0, 25°).  A cut is
  skipped if n_z < −0.5 or it would remove more than 3% of the volume.  At
  most U{8…16} chips per rock.  After chips the rock keeps about 60–80% of
  the body volume.

### 2.4 Grain (`texture`, scale 3)

- **Remesh**: `Manifold.refine_to_length(0.25 mm)` (trimesh's
  `subdivide_to_size` left T-junctions, which made the mesh non-manifold).
- **Grain**: `dharmatiles.stone.noise.fbm`, 4 octaves at 0.45 mm, amplitude
  0.12 mm, along vertex normals.
- **Edge keep**: × (0.5 + 0.5·smoothstep(dist to the nearest sharp edge /
  0.5 mm)).
- **Base fade**: × smoothstep((z − 0.15)/0.8 mm), and zero where the vertex
  normal has n_z < −0.3.  The result is trimmed at z = 0.

### 2.5 Clusters (`make_item`)

- Probability 0.25 per placement; k = 2–4 stones.
- Satellite widths max(D·U(0.45, 0.9), 4 mm), at evenly spaced angles
  (± 0.5 rad jitter) around the main stone, at (D/2 + d/2)·U(0.35, 0.6) from
  its centre so they overlap.  Each stone gets its own family and texture.
- Boolean union; the placement radius R covers the whole cluster.

### 2.6 Scatter (`scatter`, `pose`)

- N = round(U(9, 12) × area / 4 squares) items are **built first**, then
  placed largest first by dart throwing: ≤ 400 darts each, accepted when
  ≥ 1 mm + R from the rim and ≥ 1.5 mm + R_i + R_j from every placed item.
  An item that finds no spot is dropped.
- `pose` gives each placement a random yaw.  (meadowlab reuses
  `scatter(…, placements)` and `pose`.)

### 2.7 Tile (`build_tile`)

- Slab [0, W] × [0, H] × [0, 4.0] (z = 0 is the terrain datum, as in
  dharmatiles); rocks translated to z₀ = 4.0 − 0.3 mm.
- Slab ∪ rocks ∪ DB base (`make_base`, peg height from `select_peg_height`
  on the tallest point), exported via trimesh.
- **Audit**: watertight, body count, overhang area (n_z < −sin 48°,
  z > 0.05), per-rock facet and chip ranges, timing.
- Each rock is evaluated (`num_tri`) as soon as it is made.  manifold3d is
  lazy; deferring rocks into the tile union gives the same geometry with
  a different triangle order, so this keeps STLs byte-stable.

## 3. Parameters (module constants, `rocklab.py`)

| Stage | Constants (value) |
|---|---|
| tile | `SQUARE_MM` 35, `SURFACE_MM` 4.0, `SINK_MM` 0.3 |
| mass | `WIDTH_MM` 6–15, `WIDTH_EXP` 1.4, `ASPECT` 1–2, `HEIGHT_RATIO` 0.45–0.85, `HEIGHT_CAP_MM` 9, `HEIGHT_MIN_MM` 2.5, `SLAB_HEIGHT` 0.6, `FAMILIES` dome 0.35 / slab 0.2 / block 0.2 / wedge 0.25, `WEDGE_OFFSET` 0.2–0.45, `BASE_PTS` 14–22, `BODY_PTS` 60–120, `TOP_PTS` 1–3, `LUMP_AMP` 0.08 |
| facets | `FACET_SAMPLES` 4000, `FACET_R_MM` 0.9–2.5, `FACET_R_EXP` 1.6, `FACET_TILT_DEG` 14, `FACET_OFFSET_MM` −0.15…+0.1, `FACET_MIN_NZ` −0.5 |
| chips | `SHARP_EDGE_DEG` 18, `CHIP_MIN_Z_MM` 0.6, `CORNER_CHIP_FRAC` 0.45, `CORNER_CHIP_MM` 0.3–0.9, `EDGE_CHIP_FRAC` 0.35, `EDGE_CHIP_MM` 0.2–0.55, `CHIP_TILT_DEG` 25, `MAX_CHIPS` 8–16, `CHIP_MAX_LOSS` 0.03 |
| grain | `TEX_EDGE_MM` 0.25, `GRAIN_MM` 0.45, `GRAIN_AMP_MM` 0.12, `TEX_BASE_FADE_MM` 0.8, `EDGE_KEEP_MM` 0.5 |
| clusters | `CLUSTER_PROB` 0.25, `CLUSTER_STONES` 2–4, `CLUSTER_SIZE` 0.45–0.9, `MIN_STONE_MM` 4, `CLUSTER_DIST` 0.35–0.6 |
| scatter | `PLACEMENTS_2X2` 9–12, `CLEARANCE_MM` 1.5, `RIM_MM` 1.0, `DART_TRIES` 400, `OVERHANG_DEG` 45 |

Measured (2 × 2): seed 1: 11 placements (4 clusters), 18 stones, width
4.2–15.0 mm, 30–163 facets, top 12.28 mm, overhang 0.00 mm², 4.9 MB, ≈ 3 s.
Seed 2: 10 placements, 13 stones, 51–151 facets, overhang 0.02 mm².

## 4. Iteration log

| Iter | Artifact / trigger | Fix |
|---|---|---|
| v1a | first build: small, smooth gems (R2); 1.7 mm cluster pebbles (R6) | bigger (6–15 mm), cluster stones ≥ 4 mm, more chips |
| v1b | chips converge to uniform "dice" facets and remove half the volume | facet-seed construction (§2.2) |
| v1c | handed to Shawn | — |
| v1d | Shawn: needs "different degrees of texture… rough surfaces… chipped corners" | corner/edge chips, fracture scallops + grain; first try rounded every edge, fixed by the edge-keep weight |
| v1e | Shawn: "the fracture scallops are horrible… just turn up the fine grain" | scallops deleted; grain 0.05 → 0.12 mm at 0.45 mm, 4 octaves, halved on edges |
| v1f | Shawn: texture "isn't doing it"; big flat faces vs. the reference's mosaic → "make the largest faces smaller" | facet spacing 1.0–3.6 → 0.9–2.5 mm; rounded block/slab profiles; ±8% angle-only lumpiness; chips capped at 8–16.  Harder tilt (22–28°) tried and rejected |
| — | refactor (2026-10-27) | facet sampling seeded from the rock RNG (it used the unseeded global RNG, so every run differed: REQ-OUT-4 was violated); `make_body` / `cut_facets` split out of `make_rock`; `chips` info field renamed `facets` (it counted facets), real chip count reported; `Placement` record and `pose` shared with meadowlab; dead `family=` parameter and unused stats dropped |


### Export guarantee (2026-10-27)

Before export, the mesh is merged exactly as a slicer does (float32
positions, merge by position) and the run raises unless it is
watertight.  Seeds 1–4 pass with no repair needed.


## Exposed ground = dirt (2026-10-29)

Shawn: dirt should be the texture wherever ground is exposed, across all
labs, and mud and other dirt textures are eliminated.  This lab now
uses `dirtlab.dirt_relief(seed, W, H)`: dirtlab's soil roughness, clods,
grit and pebbles, without its hills or big stones, as a relief offset
sampled onto this lab's grid.
- The flat 4.0 mm slab is replaced by a dirt heightfield (4.0 mm + dirt)
  plus an exact 1 mm box.  Rocks sink an extra 0.25 mm (`DIRT_SINK_MM`),
  so dirt dips never expose a rock's base.
