# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Project Does

**dharmatiles** generates procedural 3D-printable terrain tiles as STL files. Each tile has a DungeonBlocks-compatible socket-peg underside, soil texture, scattered rocks, and grass blades that grow segment-by-segment around obstacles. Tiles are designed for tabletop gaming (35 mm square grid, DungeonBlocks standard). Output is also generated at OpenLOCK scale (25.4 mm/sq).

## Setup and Commands

```bash
# Install in editable mode (required before running anything)
pip install -e .

# Batch mode: process every src/tiles/**/*.tile.py → stl/{dungeonblocks,openlock}/…
dharmatiles-gen

# Single tile (writes to canonical stl/{system}/{NxM}-{name}-{db|ol}.stl)
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
| `scatter/prototype.py` | `Rocks` + `Grass` — scatter-thing classes with `scatter(scene, ...)` |
| `scatter/layer.py` | `Scatter` — runs `Rocks` / `Grass` / `Tree` things in tile order |
| `layers/__init__.py` | Public layer classes: `SoilCarpet`, `GrassCarpet`, `Scatter`, `Water` |
| `layers/soil.py` | `SoilCarpet` — two-tier super-Gaussian blobs into terrain_z |
| `layers/rocks.py` | `_build_rocks_mesh_core` / `_build_rocks_mesh_from_seeds` — vectorised half-ellipsoid kernel |
| `layers/grass_carpet.py` | `GrassCarpet` — embossed 2D blade-stamp texture into terrain_z |
| `layers/water.py` | `Water` — pool-floor reshape, displacement, ripples, volume mesh |
| `trees/envelope.py` | `CanopyEnvelope` — axisymmetric canopy envelope dataclass |
| `trees/skeleton.py` | `grow_skeleton()` — two-pass skeleton + `_branch_skeleton` BFS + `_compute_radii_bottom_up` |
| `trees/mesh.py` | `build_tree_mesh()` — tapered cubic-Bezier tube mesh |
| `trees/layer.py` | `Tree` — scatter-thing class; `_stamp_tree` obstacle stamping |
| `bases/dungeonblocks.py` | DungeonBlocks socket-peg base; logo inset; STL export |
| `bases/openlock.py` | OpenLOCK T-slot base via manifold3d CSG; STL export |
| `terrains/tile.py` | Entry point: `build_tile_from_spec()` flat orchestrator + CLI |

`core/` modules are pure primitives (array in / array out). `grass/` holds the grass growth sub-pipeline. `scatter/` is the unified placement system for rocks, grass, and trees. `layers/` has terrain-texture layers (soil, grass carpet, water). `trees/` holds the Tree generator. `bases/` attaches system-specific underside geometry. `terrains/` is the entry point that assembles everything.

### Tile Format (`.tile.py` files)

Python files in `src/tiles/`. Each file builds and binds a `Tile` to the
module-level name `tile`. The tile language IS the implementation language —
`Region.layers` holds real layer instances, not strings.

```python
from dharmatiles.spec   import Tile, Region, Boundary, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import GrassCarpet, Scatter
from dharmatiles.scatter import Rocks, Grass, Tree, Uniform

species = SpeciesConfig()
tile = Tile(
    surface=SurfaceConfig(seed=42),
    areas=[
        Region(id='meadow', selector=FloodFill(0.5, 0.5), layers=[
            GrassCarpet(species=species, placement=Grouped(groups_per_square=3)),
            Scatter(
                Rocks(r_min=0.8, r_max=2.2),
                Grass(species=species),
                Tree(height_mm=40.0, placement=Uniform(count_per_square=0.25)),
            ),
        ]),
    ],
)
```

**Public layer classes** (all in `dharmatiles.layers`):

| Class | Effect | `height_default_mm` |
|---|---|---|
| `SoilCarpet(**SoilConfig kwargs)` | Soil blob texture into terrain_z | 5.0 |
| `GrassCarpet(species=…, **GrassUnderlayConfig kwargs)` | Embossed 2D blade stamps into terrain_z | 5.0 |
| `Scatter(*things)` | Runs each `Rocks` / `Grass` / `Tree` thing in tile order | 5.0 |
| `Water(embed_mm=…, height_mm=…)` | Reshape pool floor + emit water volume mesh | 3.0 |

**Scatter things** (all in `dharmatiles.scatter`):

| Class | Effect |
|---|---|
| `Rocks(*, scatter=ScatterConfig(...), **RocksConfig kwargs)` | Vectorised half-ellipsoid rocks; stamps `terrain_support_z` + `obstacle_mask` |
| `Grass(species=…, *, scatter=…, max_stack_height=…, **SpeciesConfig overrides)` | 3D blades planted + grown around rocks |
| `Tree(height_mm=…, canopy_radius_mm=…, canopy_base_radius_mm=…, placement=…, **kwargs)` | Space-colonisation tree (see Tree section) |

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

`Tree` is a scatter-thing (placed by `Scatter`) that builds printable
trees via a two-pass algorithm in `trees/skeleton.py` +
`trees/mesh.py`.

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

**Mesh (`build_tree_mesh`):** each (parent, child) skeleton edge is a
tapered cubic Bezier tube. Start/end tangents are `prior_dirs`, giving C1
continuity at forks. A root flare anchors the trunk to the terrain surface.

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

### Scatter System (rocks + grass + trees)

`Scatter` runs the `Rocks` / `Grass` / `Tree` instances it was
constructed with, in the order they appear in its argument list.  Put `Rocks`
first so following `Grass` blades can steer around already-stamped rock
footprints.

The same ordering rule applies across regions and boundaries: the
orchestrator runs all regions in tile order, then all boundaries in spec
order.  3D grass only steers around rocks that have already been stamped
into `terrain_support_z` *before* `Grass.scatter()` runs.

- `Rocks.scatter()` samples positions from its `ScatterConfig`, builds
  `RockSeed`s, sorts big→small, and calls `_build_rocks_mesh_from_seeds`
  (vectorised NumPy) which also stamps `terrain_support_z` and `obstacle_mask`.
- `Grass.scatter()` syncs `vegetation_support_z` from the completed
  `terrain_support_z`, then delegates to `FloppyGrassLayer` which plants
  seeds (`GrassSeed.sort_key() = (upstream_dist, direction)` so seeds
  facing the tile boundary grow first) and runs the segment-by-segment grower.
- `Tree.scatter()` calls `grow_skeleton()` per placed tree, then
  `build_tree_mesh()`, then stamps the tree footprint into
  `terrain_support_z` and `obstacle_mask`.

`ScatterConfig` (in `scatter/config.py`) controls: `items_per_square` (hard
count), `groups_per_square` (Voronoi clumps; 0 = uniform random), `gap_mm`,
`group_dir_mode`.  Defaults: `Rocks` → count-based, no groups; `Grass` →
area-based, Voronoi groups from `SpeciesConfig`.  Distribution helpers
(`voronoi_groups`, `jitter_grid_xy`, `scatter_positions`) live in
`scatter/distribute.py` and are imported by `grass/grow.py` too.

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
  scatter/       unified placement system: config, seed, distribute, prototype, layer
  grass/         grass growth sub-pipeline: seed, grow, mesh, growers/, _geometry, layer, config
  layers/        soil.py, rocks.py (kernel), grass_carpet.py, water.py
  trees/         Tree generator: envelope, skeleton, mesh, layer
  bases/         dungeonblocks.py, openlock.py
  terrains/      tile.py (main entry point + CLI)
src/tiles/
  ground/        ground-type tile specs (.tile.py)
  water/         water-type tile specs (.tile.py)
src/extras/      standalone non-terrain utilities (dharmatiles-paint-organizer.py)
src/scripts/     standalone utilities; src/scripts/archived/ = old generations
src/scad/        OpenSCAD files and experiments
stl/
  dungeonblocks/ generated STL output — DungeonBlocks base system
  openlock/      generated STL output — OpenLOCK base system
docs/            design notes, architecture review transcripts, session memory
docs/meta/history/  architecture review transcripts
docs/memory/     persistent session memory (MEMORY.md index)
```
