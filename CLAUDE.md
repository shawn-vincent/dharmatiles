# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Project Does

**dharmatiles** generates procedural 3D-printable terrain tiles as STL files. Each tile has a DungeonBlocks-compatible socket-peg underside, soil texture, scattered rocks, and grass blades that grow segment-by-segment around obstacles. Tiles are designed for tabletop gaming (35 mm square grid, DungeonBlocks standard). Output is also generated at OpenLOCK scale (25.4 mm/sq).

## Setup and Commands

```bash
# Install in editable mode (required before running anything)
pip install -e .

# Batch mode: process every src/tiles/*.tile.py → stl/{dungeonblocks,openlock}/…
generate-tile-stl

# Single spec (writes to canonical stl/{system}/{NxM}-{name}-{db|ol}.stl)
generate-tile-stl --spec "src/tiles/soil+grass.tile.py"
generate-tile-stl --spec "src/tiles/water+grass.tile.py"
generate-tile-stl --quiet   # suppress progress output

# Run a single script directly (no install needed)
python -m dharmatiles.terrains.tile --spec "src/tiles/soil+grass.tile.py"
```

There are no automated tests; correctness is verified by opening the STL in PrusaSlicer, MeshLab, or Windows 3D Builder and visually inspecting the mesh.

## STL Regeneration Policy

**After every code change, regenerate STLs before finishing**, unless the user explicitly says otherwise.

- **Never pass `-o` / `--output`** — let each spec write to its default path.
- For tile terrain / layers / core changes: regenerate **all** `.tile.py` specs:

```bash
for spec in src/tiles/*.tile.py; do generate-tile-stl --spec "$spec"; done
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
TileSpec (.tile.py) ──► build_tile_from_spec()
                          │
                          ▼
              region mask (flood-fill from boundary curves)
                          │
                          ▼
              terrain_z heightmap (IDW blend at boundaries,
                        quadratic slope into water zones)
                          │
                   ┌──────┴──────────────────────┐
                   ▼                             ▼
             (DB scale)                    (OL scale — re-run
                                            at 25.4 mm/sq)
                   │
         ┌─────────┼──────────┬────────────────┐
         ▼         ▼          ▼                ▼
   SoilCarpetLayer  GrassCarpetLayer  RocksLayer  FloppyGrassLayer
   (blob texture     (embossed 2D      (cut half-  (segment-by-segment
    on terrain_z)     stamps on         ellipsoids)  blade growth)
                      terrain_z)
         └─────────┼──────────┴────────────────┘
                   ▼
         (optional) WaterVolume
                   ▼
         terrain solid (make_heightmap_solid)
                   ▼
         base attach (dungeonblocks.export / openlock.export)
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
| `core/config.py` | Config dataclasses: `SceneConfig`, `SurfaceConfig`, `SpeciesConfig`, `GrassConfig`, `GrassUnderlayConfig`, `SoilConfig`, `RocksConfig`, `BaseConfig` |
| `core/spec.py` | `TileSpec` dataclasses + `.tile.py` loader |
| `core/tile.py` | `TileScene` accumulator + `make_xy_grids` |
| `core/region.py` | Boundary path generation, Bresenham rasterisation, BFS flood fill |
| `core/mesh.py` | `make_heightmap_solid` (uniform + adaptive Laplacian) |
| `core/grid.py` | `sample_grid` (bilinear) |
| `core/terrain.py` | `TerrainType` enum and height/transition helpers (metadata only) |
| `core/logo.py` | SVG lotus logo → manifold3d inset solid |
| `grass/seed.py` | `GrassSeed` dataclass — per-blade geometry, taper curves |
| `grass/grow.py` | Seeding (Voronoi groups, jitter grid) and segment-by-segment growth |
| `grass/mesh.py` | Blade mesh construction + vegetation support rasterisation |
| `grass/growers/flat.py` | `FlatGrassGrower` — cross-section rings, keel, spine smoothing |
| `grass/_geometry.py` | Shared helpers: `_blade_step_geometry`, `_stamp_segment`, `_contained_segment_cells`, etc. |
| `grass/layer.py` | `GrassLayer` / `FloppyGrassLayer` entry points |
| `layers/soil.py` | `SoilCarpetLayer` — two-tier super-Gaussian blobs into terrain_z |
| `layers/rocks.py` | `RocksLayer` — vectorised cut half-ellipsoid rocks with slope alignment |
| `layers/grass.py` | Compatibility re-export of `FloppyGrassLayer` |
| `layers/grass_carpet.py` | `GrassCarpetLayer` — embossed 2D blade-stamp texture into terrain_z |
| `layers/water.py` | Water displacement, ripple, and volume mesh |
| `bases/dungeonblocks.py` | DungeonBlocks socket-peg base; logo inset; STL export |
| `bases/openlock.py` | OpenLOCK T-slot base via manifold3d CSG; STL export |
| `terrains/tile.py` | Entry point: `build_tile_from_spec()`, spec collectors, CLI |

`core/` modules are pure primitives (array in / array out). `grass/` holds the full grass sub-pipeline. `layers/` modules implement the `.build(scene)` interface. `bases/` attaches system-specific underside geometry. `terrains/` is the entry point that assembles everything.

### Tile Spec Format (`.tile.py` files)

Python files in `src/tiles/`. Each file builds and binds a `TileSpec` to the
module-level name `tile`. All types are importable from `dharmatiles.core.spec`:

```python
from dharmatiles.core.spec import (
    TileSpec, RegionSpec, LayerSpec,
    BoundarySpec, BoundaryLayerSpec,
    SurfaceConfig, SpeciesConfig,
)
```

**Layer types** that can appear in `RegionSpec.layers` or `BoundarySpec.layers`:

| Type | Effect |
|---|---|
| `grass_carpet` | Embossed 2D blade stamps + noise base into terrain_z |
| `grass` | 3D blade growth (FloppyGrassLayer) |
| `soil_carpet` | Soil blob texture into terrain_z |
| `water` | Marks region as water pool (triggers shoreline + volume mesh) |
| `rocks` | Scattered rocks placed in this region |

**Height defaults** (total slab thickness from tile bottom to surface):

| Layer type | Default height_mm |
|---|---|
| `grass_carpet`, `soil_carpet`, `grass` | 5.0 mm |
| `water` | 3.0 mm (2 mm depression below ground) |
| `floor` | 10.0 mm |

Boundaries are curves from one tile edge to another. `width_mm=0` = zero-width dividing line; `width_mm > 0` = physical slope strip. Adjacent-region heights are IDW-blended across the strip.

Tile files may freely use imports, helper functions, calculations, shared constants,
and composition. The pipeline only consumes the final `TileSpec` object.

### Grass Carpet vs. 3D Grass

`grass_carpet` and `grass` layers use independently seeded positions — the carpet
provides a dense field of flat blade footprints; the 3D blades stand up through it
at sparser, separately seeded locations.  Pass the same `SpeciesConfig` instance to
both layers so they share identical blade *geometry* (width, taper, curl,
cross-section) even though positions differ.  See `soil+grass.tile.py` for an example.

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
  core/          pure primitives: config, spec, tile, region, mesh, grid, terrain, logo
  grass/         grass sub-pipeline: seed, grow, mesh, growers/, _geometry, layer, config
  layers/        soil.py, rocks.py, grass.py (wrapper), grass_carpet.py, water.py
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
