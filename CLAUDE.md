# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Project Does

**dharmatiles** generates procedural 3D-printable terrain tiles as STL files. Each tile has a DungeonBlocks-compatible socket-peg underside, soil texture, scattered rocks, and grass blades that grow segment-by-segment around obstacles. Tiles are designed for tabletop gaming (35 mm square grid, DungeonBlocks standard). Output is also generated at OpenLOCK scale (25.4 mm/sq).

## Setup and Commands

```bash
# Install in editable mode (required before running anything)
pip install -e .

# Batch mode: process every src/tiles/*.tile.py → stl/{dungeonblocks,openlock}/…
dharmatiles-gen

# Single spec (writes to canonical stl/{system}/{NxM}-{name}-{db|ol}.stl)
dharmatiles-gen --spec "src/tiles/soil+grass.tile.py"
dharmatiles-gen --spec "src/tiles/water+grass.tile.py"
dharmatiles-gen --quiet   # suppress progress output

# Run a single script directly (no install needed)
python -m dharmatiles.terrains.tile --spec "src/tiles/soil+grass.tile.py"
```

There are no automated tests; correctness is verified by opening the STL in PrusaSlicer, MeshLab, or Windows 3D Builder and visually inspecting the mesh.

## STL Regeneration Policy

**After every code change, regenerate STLs before finishing**, unless the user explicitly says otherwise.

- **Never pass `-o` / `--output`** — let each spec write to its default path.
- For tile terrain / layers / core changes: regenerate **all** `.tile.py` specs:

```bash
for spec in src/tiles/*.tile.py; do dharmatiles-gen --spec "$spec"; done
```

- For `src/extras/hex_paint_organizer.py`: `python src/extras/hex_paint_organizer.py`

Always report vertex/face counts and watertight status for each generated file.

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
   For each Region (then Boundary), run its layers in spec order:
   each layer.apply(scene, placement_mask=mask) mutates terrain_z /
   terrain_support_z / rock_mask and returns trimesh parts.

         SoilCarpetLayer    — blob texture into terrain_z
         GrassCarpetLayer   — embossed 2D blade stamps into terrain_z
         ScatterLayer(
             Rocks(...),    — vectorised half-ellipsoids; stamp support_z
             Grass(...),    — plant + grow 3D blades around rocks
         )
         WaterLayer         — reshape pool floor, emit water volume mesh
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
- `rock_mask` — bool grid; grass steers around rock footprints
- `grass_mask` — bool grid; confines grass seeding to eligible regions

### Key Modules

| File | Role |
|---|---|
| `spec.py` | `Tile`, `Region`, `Boundary`, `TileLayer` protocol, `load_spec()` |
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
| `scatter/layer.py` | `ScatterLayer` — runs `Rocks` / `Grass` things in spec order |
| `layers/__init__.py` | Public layer classes: `SoilCarpetLayer`, `GrassCarpetLayer`, `ScatterLayer`, `WaterLayer` |
| `layers/soil.py` | `SoilCarpetLayer` — two-tier super-Gaussian blobs into terrain_z |
| `layers/rocks.py` | `_build_rocks_mesh_core` / `_build_rocks_mesh_from_seeds` — vectorised half-ellipsoid kernel |
| `layers/grass_carpet.py` | `GrassCarpetLayer` — embossed 2D blade-stamp texture into terrain_z |
| `layers/water.py` | `WaterLayer` — pool-floor reshape, displacement, ripples, volume mesh |
| `bases/dungeonblocks.py` | DungeonBlocks socket-peg base; logo inset; STL export |
| `bases/openlock.py` | OpenLOCK T-slot base via manifold3d CSG; STL export |
| `terrains/tile.py` | Entry point: `build_tile_from_spec()` flat orchestrator + CLI |

`core/` modules are pure primitives (array in / array out). `grass/` holds the grass growth sub-pipeline. `scatter/` is the unified placement system for rocks and grass. `layers/` has terrain-texture layers (soil, grass carpet, water). `bases/` attaches system-specific underside geometry. `terrains/` is the entry point that assembles everything.

### Tile Spec Format (`.tile.py` files)

Python files in `src/tiles/`. Each file builds and binds a `Tile` to the
module-level name `tile`. The spec language IS the implementation language —
`Region.layers` holds real layer instances, not strings.

```python
from dharmatiles.spec   import Tile, Region, Boundary, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import (
    SoilCarpetLayer, GrassCarpetLayer, ScatterLayer, WaterLayer,
)
from dharmatiles.scatter import Rocks, Grass

species = SpeciesConfig()
tile = Tile(
    surface=SurfaceConfig(seed=42),
    regions=[
        Region(id='meadow', contains=(0.25, 0.5), layers=[
            GrassCarpetLayer(species=species, groups_per_square=240),
            ScatterLayer(
                Rocks(r_min=0.8, r_max=2.2),
                Grass(species=species, groups_per_square=24),
            ),
        ]),
        Region(id='dirt', contains=(0.75, 0.5), layers=[
            SoilCarpetLayer(),
        ]),
    ],
)
```

**Public layer classes** (all in `dharmatiles.layers`):

| Class | Effect | `height_default_mm` |
|---|---|---|
| `SoilCarpetLayer(**SoilConfig kwargs)` | Soil blob texture into terrain_z | 5.0 |
| `GrassCarpetLayer(species=…, **GrassUnderlayConfig kwargs)` | Embossed 2D blade stamps into terrain_z | 5.0 |
| `ScatterLayer(*things)` | Runs each `Rocks` / `Grass` thing in spec order | 5.0 |
| `WaterLayer(embed_mm=…, height_mm=…)` | Reshape pool floor + emit water volume mesh | 3.0 |

**Scatter things** (all in `dharmatiles.scatter`):

| Class | Effect |
|---|---|
| `Rocks(*, scatter=ScatterConfig(...), **RocksConfig kwargs)` | Vectorised half-ellipsoid rocks; stamps `terrain_support_z` + `rock_mask` |
| `Grass(species=…, *, scatter=…, max_stack_height=…, **SpeciesConfig overrides)` | 3D blades planted + grown around rocks |

`Region` height falls back to its first layer's `height_default_mm` when
`height_mm=None`.  Boundary curves go from one tile edge to another;
`width_mm=0` = zero-width dividing line, `width_mm > 0` = physical strip
with its own `layers=[…]`.  Adjacent-region heights are IDW-blended across
boundaries.

Tile files are plain Python: imports, helpers, shared constants, and
composition all work.  The orchestrator (`terrains/tile.py`) walks
`tile.regions` and `tile.boundaries` in spec order and calls
`layer.apply(scene, placement_mask=mask)` on each layer.

### Scatter System (rocks + grass)

`ScatterLayer` runs the `Rocks` / `Grass` instances it was constructed with,
in the order they appear in its argument list.  Put `Rocks` first so the
following `Grass` blades can steer around already-stamped rock footprints.

The same ordering rule applies across regions and boundaries: the
orchestrator runs all regions in spec order, then all boundaries in spec
order.  3D grass only steers around rocks that have already been stamped
into `terrain_support_z` *before* `Grass.scatter()` runs.  Put any region
whose rocks the grass should respect ahead of the grass-bearing region in
the spec.  Rocks in a boundary always run after every region, so grass
blades growing into a boundary strip will plow through its rocks —
documented behaviour: put rocks on grass, you get rocks on grass.

- `Rocks.scatter()` samples positions from its `ScatterConfig`, builds
  `RockSeed`s, sorts big→small, and calls `_build_rocks_mesh_from_seeds`
  (vectorised NumPy) which also stamps `terrain_support_z` and `rock_mask`.
- `Grass.scatter()` syncs `vegetation_support_z` from the completed
  `terrain_support_z`, then delegates to `FloppyGrassLayer` which plants
  seeds (`GrassSeed.sort_key() = (upstream_dist, direction)` so seeds
  facing the tile boundary grow first) and runs the segment-by-segment
  grower.

`ScatterConfig` (in `scatter/config.py`) controls: `items_per_square` (hard
count), `groups_per_square` (Voronoi clumps; 0 = uniform random), `gap_mm`,
`group_dir_mode`.  Defaults: `Rocks` → count-based, no groups; `Grass` →
area-based, Voronoi groups from `SpeciesConfig`.  Distribution helpers
(`voronoi_groups`, `jitter_grid_xy`, `scatter_positions`) live in
`scatter/distribute.py` and are imported by `grass/grow.py` too.

### Grass Carpet vs. 3D Grass

`GrassCarpetLayer` and `Grass` use independently seeded positions — the
carpet provides a dense field of flat blade footprints; the 3D blades stand
up through it at sparser, separately seeded locations.  Pass the same
`SpeciesConfig` instance to both so they share identical blade *geometry*
(width, taper, curl, cross-section) even though positions differ.  See
`src/tiles/soil+grass.tile.py` for an example.

### Colour Encoding

STLs are plain binary STL.  Face colours are not currently encoded in the output;
the base mesh's `face_colors` are set to zero.  The `#rrggbb` colours documented
in earlier versions (soil brown, stone grey, grass green, water blue) are not
active in the export pipeline.

### Slope Assumption

All geometry layers (soil, rocks, grass) treat the terrain surface as **locally horizontal** — heights and orientations are in world coordinates. This is correct for flat grass regions. The slope strip between regions is bare soil with no placed features, so the error is negligible. See `TileScene` docstring for the planned `terrain_normal()` API when slope-aware placement is needed.

## Known Open Items (from 2026-06-09 architecture review)

1. **Grass directions are purely random** — `core/flow.py` (the spatial direction
   field) was removed as dead code.  Blade groups use uniform random directions
   with no spatial coherence.  A future direction-field system would live in
   `core/` and hook into `plant_seeds` / `_make_seed` to enable wind-swept,
   radial, or swirl patterns.

## Project Layout

```
src/dharmatiles/
  spec.py        Tile / Region / Boundary + TileLayer protocol + load_spec
  core/          pure primitives: config, tile, region, mesh, grid, terrain, logo
  scatter/       unified placement system: config, seed, distribute, prototype, layer
  grass/         grass growth sub-pipeline: seed, grow, mesh, growers/, _geometry, layer, config
  layers/        soil.py, rocks.py (kernel), grass_carpet.py, water.py
  bases/         dungeonblocks.py, openlock.py
  terrains/      tile.py (main entry point + CLI)
src/tiles/       .tile.py spec files (Python)
src/extras/      standalone non-terrain utilities (hex_paint_organizer.py)
src/scripts/     standalone utilities; src/scripts/archived/ = old generations
src/scad/        OpenSCAD files and experiments
stl/
  dungeonblocks/ generated STL output — DungeonBlocks base system
  openlock/      generated STL output — OpenLOCK base system
docs/            design notes, architecture review transcripts, session memory
docs/meta/history/  architecture review transcripts
docs/memory/     persistent session memory (MEMORY.md index)
```
