# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Project Does

**dharmatiles** generates procedural 3D-printable terrain tiles as STL files. Each tile has a DungeonBlocks-compatible socket-peg underside, soil texture, scattered stones, and grass blades that grow segment-by-segment around obstacles. Tiles are designed for tabletop gaming (35 mm square grid, DungeonBlocks standard).

## Setup and Commands

```bash
# Install in editable mode (required before running anything)
pip install -e .

# Generate the default 1×1 all-grass tile → stl/tile.stl
generate-tile-stl

# Common flags
generate-tile-stl --seed 42
generate-tile-stl --cols 3 --rows 3
generate-tile-stl --spec src/tiles/half-grass-soil.tile
generate-tile-stl --spec src/tiles/grass-and-water.tile -o stl/custom.stl
generate-tile-stl --quiet   # suppress progress output

# Run a single script directly (no install needed)
python -m dharmatiles.terrains.tile --spec src/tiles/half-grass-soil.tile
```

There are no automated tests; correctness is verified by opening the STL in PrusaSlicer, MeshLab, or Windows 3D Builder and visually inspecting the coloured mesh.

## STL Regeneration Policy

**After every code change, regenerate at least one illustrative STL before finishing**, unless the user explicitly says otherwise. Use the most relevant output for the changed code:

| Changed file | Regeneration command |
|---|---|
| `src/extras/craft_paint_modular_organizer.py` | `python src/extras/craft_paint_modular_organizer.py` |
| Tile terrain / layers / core | `generate-tile-stl --spec src/tiles/grass-and-water.tile -o stl/openlock/1x1-grass-and-water-ol.stl` (or whichever spec exercises the change) |
| Default tile path | `generate-tile-stl` |

Always report the output path, vertex/face counts, and watertight status after generation.

## Spatial Terminology (canonical — do not deviate)

Three-tier hierarchy; these names are enforced throughout the codebase:

| Level | Name | Size | Key symbols |
|---|---|---|---|
| Full printed output | **tile** | cols × rows × 35 mm | `TileScene`, `build_tile()`, `stl/tile.stl` |
| One 35 mm DungeonBlocks unit | **square** | 35 × 35 mm | `cols`, `rows`, `cells_per_square`, `stones_per_square`, `groups_per_square` |
| One heightmap subdivision | **cell** | 35 mm / cells_per_square | `cell_w`, `cell_h`, `grid_w`, `grid_h` |

**Never call a 35 mm unit a "tile".** Density parameters always end in `_per_square`.

## Architecture

### Generation Pipeline (in order)

```
TileSpec (YAML) ──► build_tile_from_spec()
                          │
                          ▼
              region mask (flood-fill from boundary curves)
                          │
                          ▼
              terrain_z heightmap (IDW blend at boundaries,
                        smoothstep slope into water zones)
                          │
                          ▼
              flow field (angle + curvature grids)
                          │
             ┌────────────┼────────────────────┐
             ▼            ▼                    ▼
         SoilLayer   StonesLayer          GrassLayer
       (bumps on     (scattered           (segment-by-segment
        terrain_z)    half-ellipsoids)     blade growth)
             └────────────┼────────────────────┘
                          ▼
              terrain solid (make_heightmap_solid)
              DungeonBlocks base (make_dungeonblock_base)
                          │
                          ▼
              export_coloured_stl  →  stl/*.stl
```

`TileScene` is the mutable accumulator threaded through the pipeline. It holds:
- `terrain_z` — float heightmap, fixed at construction
- `support_z` — grows as layers rasterise occupancy
- `stone_mask` — bool grid; grass steers around stone footprints
- `grass_mask` — bool grid; confines grass seeding to eligible regions

### Key Modules

| File | Role |
|---|---|
| `core/config.py` | All config dataclasses: `SceneConfig`, `SurfaceConfig`, `FlowConfig`, `GrassConfig`, `SoilConfig`, `StonesConfig`, `BaseConfig`, `SolverConfig` |
| `core/spec.py` | `TileSpec` dataclasses + YAML/.tile.py loader |
| `core/tile.py` | `TileScene` accumulator + `make_xy_grids` |
| `core/region.py` | Boundary path generation, Bresenham rasterisation, BFS flood fill |
| `core/flow.py` | Analytic flow fields (linear/swirl/radial/drain/dipole/random-zones/curl) |
| `core/mesh.py` | Low-level primitives: blade tube, terrain solid, DungeonBlocks base, coloured STL writer |
| `core/grid.py` | `sample_grid` (bilinear) |
| `core/seed.py` | `GrassSeed` dataclass — one per blade, fully self-contained |
| `layers/soil.py` | Two-tier super-Gaussian blob texture on terrain_z |
| `layers/stones.py` | Random cut half-ellipsoid stones |
| `layers/grass.py` | Segment-by-segment blade growth with obstacle steering |
| `layers/water.py` | Water layer (placeholder; dry riverbed mode active) |
| `terrains/tile.py` | Entry point; `build_tile()` (flags) and `build_tile_from_spec()` (YAML) |

`core/` modules are pure primitives (array in / array out). `layers/` modules implement the `.build(scene)` interface. `terrains/` modules are entry points that assemble layers into a full pipeline.

### Tile Spec Format (`.tile` files)

YAML files in `src/tiles/`. Two region types drive the pipeline:
- **`grass`** — vegetated ground at 5 mm default height
- **`water`** — pool at 3 mm default height (2 mm depression); currently renders as dry riverbed with sloped soil bed
- **`soil`** — bare ground (no layers specified)

Boundaries are curves from one tile edge to another. `width_mm: 0` = zero-width dividing line; `width_mm > 0` = physical slope strip. The shoreline strip height is interpolated via IDW blend + smoothstep slope.

A `.tile.py` Python escape hatch is also supported — the file must bind a `TileSpec` to the name `tile`.

### Colour Encoding

STL output uses VisCAM/SolidView per-face colour (bit 15 set in the 2-byte attribute field). Colours:
- Soil / terrain top: earthy brown `(101, 67, 33)`
- Stones: mid-grey `(120, 120, 120)`
- Grass: natural green `(50, 120, 30)`
- Water: blue `(30, 100, 200)` (currently unused in dry riverbed mode)

### Slope Assumption

All geometry layers (soil, stones, grass) treat the terrain surface as **locally horizontal** — heights and orientations are in world coordinates. This is correct for flat grass regions. The slope strip between regions is bare soil with no placed features, so the error is negligible. See `TileScene` docstring for the planned `terrain_normal()` API when slope-aware placement is needed.

## Known Open Items (from 2026-06-08 architecture review)

The previous list was 90 % stale.  The four real open items are:

1. **Grass-package DRY** — `grass/_geometry.py` was added to hold canonical
   shared helpers (`_spine_distances`, `_sample_grid`, `_cell_index`,
   `_contained_segment_cells`).  No further duplicates known.

2. **`TerrainGrid` was orphaned** — `TerrainCell`, `TerrainGrid`,
   `terrain_grid_to_heightmap`, and `TileScene.from_terrain_grid` have been
   deleted.  The live pipeline uses the IDW approach in `_build_spec_terrain`.
   `core/terrain.py` retains `TerrainType` and transition helpers for future use.

3. **Grass species region-awareness** — fixed: `_collect_grass_configs` now
   returns `(SpeciesConfig, placement_mask)` pairs mirroring soil and stones,
   and `_build_mesh` threads the per-region mask into each seeder.

4. **`terrain_z` immutability** — fixed: the water-floor zero-out now happens
   in `build_tile_from_spec` before scene construction, honouring the
   "`terrain_z` read-only after init" contract.

## Project Layout

```
src/dharmatiles/
  core/          pure primitives (config, spec, tile, region, flow, mesh, grid, seed)
  layers/        grass.py, soil.py, stones.py, water.py
  terrains/      tile.py (main entry point)
src/tiles/       .tile spec files (YAML)
src/scripts/     standalone utilities; src/scripts/archived/ = old generations
src/scad/        OpenSCAD files and experiments
stl/             generated STL output (committed)
stl/extras/      STL outputs for non-terrain extras
docs/            design notes, architecture review transcripts, session memory
docs/meta/history/  architecture review transcripts
docs/memory/     persistent session memory (MEMORY.md index)
```
