# rocklab — Faceted Rock Scatter: Design

*Written 2026-10-27, v1.  Implements `REQUIREMENTS.md`.  Code:
`rocklab/rocklab.py`.*

---

## 1. Core idea: smooth body, then facet planes

A rock is built in two steps, both in `manifold3d`:

1. **Body**: the convex hull of about 75–145 points sampled on a
   *non-increasing profile* above a flat base.  This is a smooth-ish
   convex shape that defines the rock's mass.
2. **Facets**: blue-noise **facet seeds** with *mixed radii* are sampled
   on the body's surface.  Each seed's tangent plane, tilted slightly at
   random, becomes one facet.  The rock is the intersection of those
   half-spaces with a box standing on z = 0.

This gives exactly one facet per seed, so facet sizes come from the seed
spacing and are mixed by construction.  It also gives these guarantees:

- **Convex, so grounded (REQ-ROCK-3)**: an intersection of half-spaces
  is convex.  The tangent planes come from a body whose cross-sections
  never grow upward, and facets with n_z < −0.5 are dropped, so there is
  no mushroom.
- **Printable (REQ-FDM-1)**: no facet points down past 30° (n_z ≥ −0.5).
  The measured overhang is 0.00 mm².
- **Crisp edges (REQ-ROCK-2)**: planes meet at sharp edges, and nothing
  smooths them.

Clusters are unions of 2–4 convex rocks.  A union's surface is a subset of
its parts' surfaces, so the guarantees carry over.

## 2. Rock mass

- **Size**: the width D is drawn skewed small, D = 4 + 9·U^1.4 mm.  The
  footprint has semi-axes a = D/2 and b = a / U(1, 2), at a random yaw.
- **Height**: h = D · U(0.4, 1.0), capped at 9 mm.
- **Profile family** (REQ-ROCK-4): the radius scale is r(t) for
  t = z/h ∈ [0, 1], always non-increasing.

  | Family | r(t) | Weight |
  |---|---|---|
  | dome | (1 − t^2.2)^0.5 | 0.35 |
  | slab | (1 − t^2.8)^0.45, h × 0.6 | 0.2 |
  | block | (1 − t^3.5)^0.4 with a superellipse (p = 3) footprint | 0.2 |
  | wedge | (1 − t)^0.8 with an apex offset toward one side | 0.25 |

- **Points**:
  - **Base ring**: 7–11 points at z = 0, at random angles on the
    footprint ellipse with ±12% radius jitter.
  - **Body**: 12–28 points at random t ∈ (0, 0.97], at radius
    r(t)·(0.85–1.0) along random angles.  The wedge family shifts its
    centre toward one side as t increases.
  - **Top**: 1–3 points near t = 1.
  
  Radius jitter only shrinks points, never pushes them out past r(t), so
  the profile stays non-increasing.
- **Lumpiness**: radius × (1 + Σ_{k=2..5} a_k cos(kφ + φ_k)), ±8%.  It
  depends on **angle only**, so every vertical line is scaled by a
  constant and the profile stays non-increasing.
  - **Why:** flat body regions (the old block sides and slab tops) gave
    all the seeds there the same tangent plane, which merged them into
    one oversized face.  A curved, lumpy body keeps every facet small.
- **Hull**: `Manifold.hull_points`.

## 3. Facets

- **Seeds**: dart-throw over 4,000 area-weighted surface samples of the
  body.  Each candidate draws a radius r = r_min + (r_max − r_min)·U^1.6
  (skewed small), and is accepted when it is at least (r + r_j)/2 from
  every accepted seed.  The radii (0.9, 2.5) mm are scaled by √(D/15).
  Small rocks therefore get fewer, proportionally coarser facets.
  Samples on the base (n_z < −0.3) are skipped, because the flat base is
  its own facet.
- **Plane**: the surface normal is tilted by U(0, 14°) about a random
  tangent axis, and the offset along the normal is jittered by
  U(−0.15, +0.1) mm.  The rock keeps n·x ≤ n·p + offset.
- **Result**: 30–100 facets per rock, and the rock keeps 85–98% of the
  body's volume.

*Superseded (v1a/b): random chip cuts that shave the current extreme
vertex.  These converge to uniformly sized "dice" facets (R2) and removed
50–68% of the volume.*

## 3a. Chips (scale 2: corners and edges, planar)

After the facets, the polytope gets more half-space cuts, so it stays
convex and printable:

- **Corners**: each vertex above z = 0.6 is chipped with probability
  0.45.  The plane normal is the vertex normal (the sum of incident face
  normals), tilted by up to 25°.  Depth is U(0.3, 0.9)·√(D/15) mm.
- **Edges**: each sharp edge (dihedral > 18°) is chipped with probability
  0.35, at a random point along it.  The normal is the bisector of the
  two faces, tilted by up to 25°.  Depth is U(0.2, 0.55)·√(D/15) mm.
- **Guards**: a cut is skipped if it would remove more than 3% of the
  volume, or if its normal has n_z < −0.5.  At most 8–16 chips are made
  per rock: with smaller facets there are many more corners, and an
  uncapped rate eroded rocks to about 60% of their volume.

## 3b. Texture (scale 3: grain displacement)

- **Remesh**: `Manifold.refine_to_length(0.25 mm)`.  trimesh's
  `subdivide_to_size` left T-junctions, which made the mesh non-manifold.
- **Grain**: `dharmatiles.stone.noise.fbm`, 4 octaves at 0.45 mm,
  ±0.12 mm, along vertex normals.
- **Weights**:
  - **Edge keep**: grain × (0.5 + 0.5·smoothstep(dist to the nearest
    sharp polytope edge / 0.5 mm)), so facet and chip ridges stay crisp.
  - **Base fade**: × smoothstep((z − 0.15)/0.8), and zero where the
    vertex normal has n_z < −0.3.
- **Measured overhang** on seed 1: 0.02 mm².
- *Removed in v1e:* Worley "fracture scallops" (rejected by Shawn).

## 4. Clusters

- **Probability**: 0.25 per placement.
- **Stones**: k = 2–4.  Stone sizes are D_main · U(0.4, 1.0), placed
  around the main stone at a distance of 0.35–0.6 × the summed radii so
  they overlap.  Each stone gets its own family and chips.
- **Union**: `Manifold` boolean union.  The cluster's footprint radius
  is used for placement clearance.

## 5. Scatter

- **Count**: N = U(8, 12) on a 2 × 2 tile (scaled by area).
- **Placement**: dart throwing.  Each placement has a radius R (footprint
  plus cluster spread).  A dart is accepted when it lies at least 1 mm
  + R from the rim and at least 1.5 mm + R_i + R_j from every placed
  item.  Larger items are placed first.
- **Seating**: rocks are built at z ≥ 0, translated to z₀ = 4.0 − 0.3,
  then unioned.

## 6. Tile

- The slab is the box [0, W] × [0, H] × [0, 4.0] (z = 0 is the terrain
  datum, as in dharmatiles).
- Slab ∪ all rocks ∪ DB base (`make_base`, with peg height from
  `select_peg_height` on the tallest point) is unioned in manifold3d,
  then exported.
- **Audit**: watertight, body count, overhang area
  (faces with n_z < −sin 48°, z > 0.05), per-rock facet count, timing.

## 7. Parameters (module constants)

| Constant | v1 |
|---|---|
| width D | 6–15 mm (U^1.4); cluster stones ≥ 4 mm |
| aspect | 1–2 |
| height ratio | 0.45–0.85, cap 9 mm |
| families (dome/slab/block/wedge) | 0.35 / 0.2 / 0.2 / 0.25 |
| body points | 14–22 base, 60–120 body, 1–3 top |
| facet seeds | radius 1.0–3.6 mm × √(D/15), U^1.6; tilt ≤ 14°; offset −0.15…+0.1; n_z ≥ −0.5 |
| cluster prob / size | 0.25 / 2–4 stones |
| placements | 9–12 per 2 × 2 |
| clearance | 1.5 mm between items, 1 mm to the rim |
| surface / sink | 4.0 mm / 0.3 mm |

## 8. Iteration log

| Iter | Artifact | Fix |
|---|---|---|
| v1a | first build: rocks small, smooth gems (R2), 1.7 mm cluster pebbles (R6) | bigger (6–15 mm), cluster stones ≥ 4 mm, more chips |
| v1b | chips converge to uniform "dice" facets and remove half the volume | facet-seed construction (§3) |
| v1c | handed to Shawn | — |
| v1f | Shawn: texture "isn't doing it"; reviewed against the reference (big flat faces vs. a mosaic of small facets); flake scars proposed and declined → "make the largest faces smaller" | facet spacing 1.0–3.6 → 0.9–2.5 mm; rounded block/slab profiles; ±8% angle-only body lumpiness; chips capped at 8–16 per rock.  Harder tilt (22–28°) was tried and rejected: it removed 40–60% of the volume and left the big faces. |
| v1e | Shawn: "the fracture scallops are horrible… just turn up the fine grain" | scallops deleted; grain 0.05 → 0.12 mm at 0.45 mm, 4 octaves, halved on edges |
| v1d | Shawn: needs "different degrees of texture… rough surfaces… chipped corners" | corner/edge chips (§3a), fracture scallops + grain (§3b); first try rounded every edge, fixed with the edge-keep weight; tile 4.9 MB, 3.2 s |
