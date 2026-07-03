# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Project Does

**dharmatiles** generates procedural 3D-printable terrain tiles as STL files. Each tile has a DungeonBlocks-compatible socket-peg underside, soil texture, scattered rocks, and grass blades that grow segment-by-segment around obstacles. Tiles are designed for tabletop gaming (35 mm square grid, DungeonBlocks standard). Output is also generated at OpenLOCK scale (25.4 mm/sq).

## Setup and Commands

```bash
# Install in editable mode (required before running anything)
pip install -e .

# Batch mode: process every src/tiles/**/*.tile.py → stl/{db,ol}/…
dharmatiles-gen

# Single tile (writes to canonical stl/{db|ol}/{NxM}-{name}-{db|ol}.stl)
dharmatiles-gen --tile "src/tiles/ground/1x1-soil+grass.tile.py"
dharmatiles-gen --tile "src/tiles/ground/2x2-grass-tree.tile.py"
dharmatiles-gen --quiet   # suppress progress output

# Run a single script directly (no install needed)
python -m dharmatiles.terrains.tile --tile "src/tiles/ground/1x1-soil+grass.tile.py"
```

There are no automated tests; correctness is verified by opening the STL in PrusaSlicer, MeshLab, or Windows 3D Builder and visually inspecting the mesh.

## STL Regeneration

Regenerate STLs **only when the user asks** — do not regenerate automatically
after a code change.  When you do regenerate:

- **Never pass `-o` / `--output`** — let each tile write to its default path.
- For tile terrain / layers / core changes, regenerate the relevant `.tile.py`
  tiles (all of them if the change is broad):

```bash
for spec in src/tiles/**/*.tile.py; do dharmatiles-gen --tile "$spec"; done
```

- For `src/extras/dharmatiles-paint-organizer.py`: `python src/extras/dharmatiles-paint-organizer.py`

Report vertex/face counts and watertight status for each generated file.

## Spatial Terminology (canonical — do not deviate)

Three-tier hierarchy; these names are enforced throughout the codebase:

| Level | Name | Size | Key symbols |
|---|---|---|---|
| Full printed output | **tile** | cols × rows × square_mm | `TileScene`, `build_tile_from_spec()`, `stl/` |
| One 35 mm DungeonBlocks unit | **square** | 35 × 35 mm (or 25.4 for OL) | `cols`, `rows`, `cells_per_square`, `rocks_per_square`, `groups_per_square` |
| One heightmap subdivision | **cell** | square_mm / cells_per_square | `cell_w`, `grid_w`, `grid_h` |

**Never call a 35 mm unit a "tile".** Density parameters always end in `_per_square`.

## Architecture

### Generation Pipeline (in order)

```
Tile (.tile.py) ──► build_tile_from_spec()
                       │
                       ▼
           region mask (flood-fill from boundary curves)
                       │
                       ▼
           terrain_z heightmap (IDW blend of region.effective_height_mm,
                     quadratic slope into water zones)
                       │
                ┌──────┴──────────────────────┐
                ▼                             ▼
          (DB scale)                    (OL scale — re-run
                                         at 25.4 mm/sq)
                │
                ▼
   For each Region (then Boundary), run its layers in tile order:
   each layer.apply(scene, placement_mask=mask) mutates terrain_z /
   terrain_support_z / obstacle_mask and returns trimesh parts.

         SoilCarpet    — blob texture into terrain_z
         GrassCarpet   — embossed 2D blade stamps into terrain_z
         Scatter(
             Rocks(...),    — vectorised half-ellipsoids; stamp support_z
             Grass(...),    — plant + grow 3D blades around rocks
             Tree(...),— space-colonisation trees (see below)
         )
         Water         — reshape pool floor, emit water volume mesh
                │
                ▼
         terrain solid (make_heightmap_solid)
                │
                ▼
         union all parts + base attach (dungeonblocks / openlock)
                │
                ▼
         stl/{system}/{NxM}-{name}-{db|ol}.stl
```

`TileScene` is the mutable accumulator threaded through the pipeline. It holds:
- `terrain_z` — float heightmap, read-only after construction
- `terrain_support_z` — grows as terrain and rock layers rasterise occupancy
- `vegetation_support_z` — grows as grass blades are stamped in
- `obstacle_mask` — bool grid; grass steers around placed obstacle footprints (rocks, flowers, …)
- `grass_mask` — bool grid; confines grass seeding to eligible regions

### Key Modules

| File | Role |
|---|---|
| `spec.py` | `Tile`, `Region`, `Boundary`, `TileLayer` protocol, `load_tile()` |
| `core/config.py` | Config dataclasses: `SceneConfig`, `SurfaceConfig`, `SpeciesConfig`, `GrassConfig`, `GrassUnderlayConfig`, `SoilConfig`, `RocksConfig`, `BaseConfig` |
| `core/tile.py` | `TileScene` accumulator + `make_xy_grids` |
| `core/region.py` | Boundary path generation, Bresenham rasterisation, BFS flood fill |
| `core/mesh.py` | `make_heightmap_solid` (uniform + adaptive Laplacian) |
| `core/grid.py` | `sample_grid` (bilinear) |
| `core/terrain.py` | `TerrainType` enum and height/transition helpers (metadata only) |
| `core/logo.py` | SVG lotus logo → manifold3d inset solid |
| `grass/seed.py` | `GrassSeed` dataclass — per-blade geometry, taper curves, `sort_key()` |
| `grass/grow.py` | Segment-by-segment growth; imports distribution helpers from `scatter/distribute.py` |
| `grass/mesh.py` | Blade mesh construction + vegetation support rasterisation |
| `grass/growers/flat.py` | `FlatGrassGrower` — cross-section rings, keel, spine smoothing |
| `grass/_geometry.py` | Shared helpers: `_blade_step_geometry`, `_stamp_segment`, `_contained_segment_cells`, etc. |
| `grass/layer.py` | `FloppyGrassLayer` — internal blade builder used by `scatter.Grass` |
| `scatter/config.py` | `ScatterConfig` — spatial distribution params (groups, gap, dir mode) |
| `scatter/seed.py` | `RockSeed` — fully-resolved rock instance with `sort_key()` |
| `scatter/distribute.py` | Voronoi grouping, jitter grid, `scatter_positions()` — shared by rocks + grass |
| `scatter/prototype.py` | `Rocks` + `Grass` — direct tile layers with `apply()` + `scatter(scene, ...)` |
| `layers/__init__.py` | Public layer classes: `SoilCarpet`, `GrassCarpet`, `Water` |
| `layers/soil.py` | `SoilCarpet` — two-tier super-Gaussian blobs into terrain_z |
| `layers/rocks.py` | `_build_rocks_mesh_core` / `_build_rocks_mesh_from_seeds` — vectorised half-ellipsoid kernel |
| `layers/grass_carpet.py` | `GrassCarpet` — embossed 2D blade-stamp texture into terrain_z |
| `layers/water.py` | `Water` — pool-floor reshape, displacement, ripples, volume mesh |
| `trees/envelope.py` | `CanopyEnvelope` — axisymmetric canopy envelope dataclass |
| `trees/skeleton.py` | `grow_skeleton()` — two-pass skeleton + `_branch_skeleton` BFS + `_compute_radii_bottom_up` |
| `trees/mesh.py` | `build_branch_mesh()` — tapered cubic-Bezier tubes, foliage cluster sweep, leaf placement dispatch |
| `trees/leaf.py` | Leaf primitives: `build_leaf_surface`, `build_leaf_oval_offsets`, `solidify_leaf` |
| `trees/placement_organic.py` | `place_leaves_organic` — union-surface maximal-Poisson leaf placer (the only leaf generator) |
| `trees/placement_leaf.py` | Shared per-leaf machinery: `_attempt_leaf` seat→build→cull pipeline, `LeafPlacementStats` |
| `trees/layer.py` | `Tree` — direct tile layer with `apply()` + `scatter()`; `_stamp_tree` obstacle stamping |
| `bases/dungeonblocks.py` | DungeonBlocks socket-peg base; logo inset; STL export |
| `bases/openlock.py` | OpenLOCK T-slot base via manifold3d CSG; STL export |
| `terrains/tile.py` | Entry point: `build_tile_from_spec()` flat orchestrator + CLI |

`core/` modules are pure primitives (array in / array out). `grass/` holds the grass growth sub-pipeline. `scatter/` holds direct tile layers for placed elements (Rocks, Grass, Flowers) plus distribution primitives. `layers/` has terrain-texture layers (soil, grass carpet, water). `trees/` holds the Tree generator (also a direct tile layer). `bases/` attaches system-specific underside geometry. `terrains/` is the entry point that assembles everything.

### Tile Format (`.tile.py` files)

Python files in `src/tiles/`. Each file builds and binds a `Tile` to the
module-level name `tile`. The tile language IS the implementation language —
`Region.layers` holds real layer instances, not strings.

```python
from dharmatiles.spec   import Tile, Region, Boundary, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import GrassCarpet
from dharmatiles.scatter import Rocks, Grass, Tree, Uniform

species = SpeciesConfig()
tile = Tile(
    surface=SurfaceConfig(seed=42),
    areas=[
        Region(id='meadow', selector=FloodFill(0.5, 0.5), layers=[
            GrassCarpet(species=species, placement=Grouped(groups_per_square=3)),
            Rocks(r_min=0.8, r_max=2.2),
            Tree(height_mm=40.0, placement=Uniform(count_per_square=0.25)),
            Grass(species=species),
        ]),
    ],
)
```

Layer ordering in `Region.layers` is the contract for state dependencies:
`Rocks` before `Grass` so blades steer around stamped rock footprints;
`Tree` before `Grass` for the same reason.

**Public layer classes** (all in `dharmatiles.layers`):

| Class | Effect | `height_default_mm` |
|---|---|---|
| `SoilCarpet(**SoilConfig kwargs)` | Soil blob texture into terrain_z | 5.0 |
| `GrassCarpet(species=…, **GrassUnderlayConfig kwargs)` | Embossed 2D blade stamps into terrain_z | 5.0 |
| `Water(embed_mm=…, height_mm=…)` | Reshape pool floor + emit water volume mesh | 3.0 |

**Direct placement layers** (all in `dharmatiles.scatter`, or `dharmatiles.trees` for Tree):

| Class | Effect | `height_default_mm` |
|---|---|---|
| `Rocks(*, placement=…, **RocksConfig kwargs)` | Vectorised half-ellipsoid rocks; stamps `terrain_support_z` + `obstacle_mask` | 5.0 |
| `Grass(species=…, *, placement=…, max_stack_height=…)` | 3D blades grown as a field simulation around prior obstacles | 5.0 |
| `Flowers(*, placement=…, **FlowerConfig kwargs)` | Dome-on-column 3D flowers; stamps `terrain_support_z` + `obstacle_mask` | 5.0 |
| `Tree(height_mm=…, canopy_radius_mm=…, placement=…, **kwargs)` | Space-colonisation tree (see Tree section) | 5.0 |

`Region` height falls back to its first layer's `height_default_mm` when
`height_mm=None`.  Boundary curves go from one tile edge to another;
`width_mm=0` = zero-width dividing line, `width_mm > 0` = physical strip
with its own `layers=[…]`.  Adjacent-region heights are IDW-blended across
boundaries.

Tile files are plain Python: imports, helpers, shared constants, and
composition all work.  The orchestrator (`terrains/tile.py`) walks
`tile.regions` and `tile.boundaries` in tile order and calls
`layer.apply(scene, placement_mask=mask)` on each layer.

### Tree Generator

`Tree` is a direct tile layer that builds printable trees via a two-pass
algorithm in `trees/skeleton.py` + `trees/mesh.py`.  It implements
`apply(scene, *, placement_mask)` and sits in `Region.layers` alongside
`Rocks`, `Grass`, `SoilCarpet`, and other layers.

**Invariants (hard constraints; must never be broken):**
- Every attractor is a **terminal node** — attractors are never branch points.
- Every branch terminates by landing **exactly at** an attractor position.
- Branching happens at **synthetic interior nodes only** (never at attractor positions).

**Pass 1 — Skeleton (`_branch_skeleton`):**

Each branch owns a set of attractors and a current tip position (BFS queue).
At each step:

1. Compute `main_dir` = blended direction toward centroid of owned attractors
   (`smoothing_alpha` blend with incoming heading for C1 continuity).
2. **Lookahead stray detection**: compute `next_pos = pos + main_dir * seg_len`.
   Classify owned attractors as *primary* (within `branch_split_angle_deg` of
   `main_dir` when measured from `next_pos`) or *stray* (outside). Stray
   clusters spawn sub-branches FROM the current tip (before stepping) — this
   guarantees every primary attractor is reachable forward from `next_pos` and
   no sub-branch ever walks backward. The old z-passover check is superseded:
   any attractor that would be overshot in z is automatically stray from
   `next_pos` by angle.
3. When primary reduces to 1 → **terminal branch mode**: `_grow_to_leaf` drives from
   current synthetic node toward the single target, initialising `cur_dir` from
   `dir_to_target` (not inherited heading) so intermediate nodes never walk
   backward, landing exactly on the attractor.
4. Otherwise advance one segment, repeat.
5. Safety: if `max_steps` budget is exhausted, force-split primary (keep nearest
   as terminal target, hand rest to a new sub-branch from current synthetic position).

**Attractor sampling (`_sample_canopy`):** canopy side-surface sampling weighted
by surface area of revolution. Attractors are not sampled on the flat bottom
disk defined by `canopy_base_radius_mm`, but the side surface can receive
attractors all the way to the top taper. Do NOT revert to uniform-t sampling;
the apex club-top artefact returns immediately.

**Pass 2 — Radii (`_compute_radii_bottom_up`):** bottom-up pipe model.
Terminal nodes → `min_radius_mm`. Internal nodes → `(Σ r_child^e)^(1/e)` where
`e = branch_exponent`. Root radius is fully derived (not configured).

**Mesh (`build_branch_mesh`):** each (parent, child) skeleton edge is a
tapered cubic Bezier tube. Start/end tangents are `prior_dirs`, giving C1
continuity at forks. A root flare anchors the trunk to the terrain surface.

**Foliage & leaves:** each terminal branch gets a swept rounded-cone foliage
cluster (`foliage_cluster_*` params below). Leaves are separate watertight
solids placed by the **organic union-surface placer**
(`trees/placement_organic.py`) — the only leaf generator (meridian/greedy/
shoots placers were deleted 2026-07-03). The clusters are boolean-unioned
into ONE placement surface; roots are dart-thrown to maximal Poisson-disk
saturation with a normal-aware grid (total coverage by construction); blades
point down-slope rotated by a coherent positional angle field; overlap is
layered by height-sorted standoffs; blade shape blends continuously from
curled/lifted on upward faces to a flush end-to-end arch on undersides
(supportless FDM). Leaves whose blades skewer an exposed branch tube are
culled. Per-leaf seat/build/cull machinery lives in
`trees/placement_leaf.py`. Spec: `docs/design/leaf-placement.md`; history:
`docs/meta/history/2026-07-03-leaf-placement-complete-history.md`. Leaf
shape params on `Tree`: `leaves` (True), `leaf_length_mm` (4.5),
`leaf_width_mm` (3.0), `leaf_thickness_mm` (0.24), `leaf_fold_angle_deg`,
`leaf_inner_curve`, `leaf_outer_curve`, `leaf_curl_deg` (placer caps the
effective curl at 32°).

**Key parameters:**

| Parameter | Default | Effect |
|---|---|---|
| `n_attractors` | 200 | Number of attractor points (= number of terminal nodes) |
| `segment_length_mm` | 1.0 | Step size for skeleton growth; smaller = less backtracking slip at low branchiness |
| `branch_split_angle_deg` | 30.0 | Half-angle of primary cone; larger = earlier splits |
| `target_fdm_angle_deg` | 35.0 | Target FDM angle — the elevation above horizon (+90° = straight up, 0° = horizontal) the skeleton *tries* to keep every branch above. One of the **two** stray-detection conditions evaluated from the forecasted next step (`next_pos`): an owned attractor becomes stray (→ spawns a sub-branch) if reaching it would leave the split cone (`branch_split_angle_deg`) **OR** require a heading below this angle. Not a guarantee — terminal landing segments may still dip below it. |
| `strict_fdm_angle_deg` | 26.0 | Strict FDM angle — the hard printability floor (elevation above horizon). Re-checked at mesh build; any branch edge below this is treated as an unprintable-overhang **failure** and reported via `RuntimeWarning` (backstops the terminal/single-attractor segments the branching can't reroute). Set below `target_fdm_angle_deg` so the band between them is tolerated without a failure. |
| `max_branches_per_step` | 3 | Max stray clusters per step |
| `branch_exponent` | 3.0 | Pipe-model exponent; larger = thicker trunk relative to branches |
| `min_radius_mm` | 1.0 | Terminal branch radius; scales the entire radius tree |
| `smoothing_alpha` | 0.1 | Heading blend (0 = pure centroid, 1 = straight ahead) |
| `debug_attractors` | False | Render attractor positions as yellow icosphere markers |
| `group_width_mm` | 20.0 | Target XY diameter (mm) of each attractor cluster. Attractors are pre-partitioned into spatial Voronoi groups so entire groups split off together (coarser, more architectural splits). `None` disables grouping. |
| `group_height_mm` | 20.0 | Target Z height (mm) of each attractor cluster. Defaults to `group_width_mm` when not specified. Ratio `group_width / group_height` controls ellipsoidal aspect ratio of clusters. |
| `foliage_bulge_mm` | 6.0 | Per-group outward bulge (mm). Requires `group_width_mm`. Edge attractors stay on the canopy surface; the interior is pushed outward by up to this amount following a dome (circular-arc) profile normal to the canopy surface. |
| `branch_split_eagerness` | 0.8 | 0–1. Controls where splits happen. 1 = eager (split at `branch_split_angle_deg`). 0 = maximally lazy (split only when attractor approaches 90° from next_pos — the hard no-backtracking limit). Implemented as `split_cos_effective = branch_split_eagerness × cos(branch_split_angle_deg)`; lower values produce fewer, longer interior branches with splitting concentrated near the tips. |
| `branch_target` | 0.33 | 0–1. Aim point within the owned attractor cloud. 0 = lowest-z attractor, 1 = highest-z attractor. Implemented as `target = lowest + branch_target × (highest − lowest)`. Lower values pull branches outward and downward; higher values drive growth upward. |
| `branch_fork_balance` | 1.0 | 0–1. How evenly attractors are redistributed at each fork. 0 = each branch keeps only the attractors already classified as stray (no redistribution). 1 = all K branches at the fork receive equal shares of the full attractor pool. Higher values produce more architecturally balanced trees. |
| `foliage_clusters` | True | Whether to generate foliage clusters on terminal branches. |
| `foliage_cluster_radius_mm` | 5.5 | Tip radius (mm) of each foliage cluster. Each terminal branch gets a D-section cone tapering from the parent branch radius up to this value at the attractor tip. |
| `foliage_cluster_length_mm` | 10.5 | Maximum cluster length (mm). The cone covers only the last `min(branch_len, K)` mm of each terminal branch; the remainder is drawn as a plain wood tube. Taper rate is fixed at `(foliage_cluster_radius_mm − r_wood) / K`, so short branches produce proportionally smaller-tipped cones. `None` = full branch is a cone. |

### Placement Layer Ordering

`Rocks`, `Grass`, `Tree`, and `Flowers` are direct `TileLayer`s placed
in `Region.layers` (or `Boundary.layers`) alongside `SoilCarpet`,
`GrassCarpet`, and `Water`.  Each implements `apply(scene, *, placement_mask)`.

**Ordering is the author's contract for state dependencies.**  The
orchestrator runs layers in list order within each area, then areas in
spec order.  3D grass only steers around obstacles stamped into
`terrain_support_z` *before* `Grass.apply()` runs, so place `Rocks`
and `Tree` before `Grass` in the same `Region.layers`.

- `Rocks.apply()` samples positions from its placement config, builds
  `RockSeed`s, sorts big→small, and calls `_build_rocks_mesh_from_seeds`
  (vectorised NumPy) which also stamps `terrain_support_z` and `obstacle_mask`.
- `Grass.apply()` syncs `vegetation_support_z` from the completed
  `terrain_support_z`, then runs a field simulation: seeds are planted and
  grown sequentially, each blade reading the shared occupancy grid left by
  prior blades.  (`GrassSeed.sort_key() = (upstream_dist, direction)` so seeds
  facing the tile boundary grow first.)
- `Tree.apply()` calls `grow_skeleton()` per placed tree, then
  `build_branch_mesh()`, then stamps the tree footprint into
  `terrain_support_z` and `obstacle_mask`.

Placement is configured via `Uniform` or `Grouped` (both in `dharmatiles.scatter`):

| Strategy | Controls |
|---|---|
| `Uniform(count_per_square=N, gap_mm=…)` | Uniform random positions; `Rocks` default |
| `Grouped(groups_per_square=N, gap_mm=…, group_dir_mode=…)` | Voronoi-grouped positions; `Grass` default |

Distribution helpers (`voronoi_groups`, `jitter_grid_xy`, `scatter_positions`)
live in `scatter/distribute.py` and are imported by `grass/grow.py` too.

### Grass Carpet vs. 3D Grass

`GrassCarpet` and `Grass` use independently seeded positions — the
carpet provides a dense field of flat blade footprints; the 3D blades stand
up through it at sparser, separately seeded locations.  Pass the same
`SpeciesConfig` instance to both so they share identical blade *geometry*
(width, taper, curl, cross-section) even though positions differ.  See
`src/tiles/ground/2x2-grass-tree.tile.py` for an example with a tree.

### Colour Encoding

STLs are plain binary STL.  Face colours are not currently encoded in the output;
the base mesh's `face_colors` are set to zero.  The `#rrggbb` colours documented
in earlier versions (soil brown, stone grey, grass green, water blue) are not
active in the export pipeline.

### Slope Assumption

All geometry layers (soil, rocks, grass) treat the terrain surface as **locally horizontal** — heights and orientations are in world coordinates. This is correct for flat grass regions. The slope strip between regions is bare soil with no placed features, so the error is negligible. See `TileScene` docstring for the planned `terrain_normal()` API when slope-aware placement is needed.

## Known Open Items (from 2026-06-15)

1. **Grass directions are purely random** — `core/flow.py` (the spatial direction
   field) was removed as dead code.  Blade groups use uniform random directions
   with no spatial coherence.  A future direction-field system would live in
   `core/` and hook into `plant_seeds` / `_make_seed` to enable wind-swept,
   radial, or swirl patterns.

## Project Layout

```
src/dharmatiles/
  spec.py        Tile / Region / Boundary + TileLayer protocol + load_tile
  core/          pure primitives: config, tile, region, mesh, grid, terrain, logo
  scatter/       direct placement layers: Rocks, Grass, Flowers + config, seed, distribute
  grass/         grass growth sub-pipeline: seed, grow, mesh, growers/, _geometry, layer, config
  layers/        soil.py, rocks.py (kernel), grass_carpet.py, water.py
  trees/         Tree generator: envelope, skeleton, mesh, leaf, placement_leaf, placement_organic, bark, layer
  bases/         dungeonblocks.py, openlock.py
  terrains/      tile.py (main entry point + CLI)
src/tiles/
  ground/        ground-type tile specs (.tile.py)
  water/         water-type tile specs (.tile.py)
src/extras/      standalone non-terrain utilities (dharmatiles-paint-organizer.py)
src/scripts/     standalone utilities; src/scripts/archived/ = old generations
src/scad/        OpenSCAD files and experiments
stl/
  db/            generated STL output — DungeonBlocks base system (db)
  ol/            generated STL output — OpenLOCK base system (ol)
docs/            design notes, architecture review transcripts, session memory
docs/meta/history/  architecture review transcripts
docs/memory/     persistent session memory (MEMORY.md index)
```
