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
generate-tile-stl --spec tiles/half-grass-soil.tile
generate-tile-stl --spec tiles/grass-and-water.tile -o stl/custom.stl
generate-tile-stl --quiet   # suppress progress output

# Run a single script directly (no install needed)
python -m dharmatiles.terrains.tile --spec tiles/half-grass-soil.tile
```

There are no automated tests; correctness is verified by opening the STL in PrusaSlicer, MeshLab, or Windows 3D Builder and visually inspecting the coloured mesh.

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
| `core/grid.py` | `sample_grid` (bilinear), `rasterise_into_support` |
| `core/seed.py` | `GrassSeed` dataclass — one per blade, fully self-contained |
| `layers/soil.py` | Two-tier super-Gaussian blob texture on terrain_z |
| `layers/stones.py` | Random cut half-ellipsoid stones |
| `layers/grass.py` | Segment-by-segment blade growth with obstacle steering |
| `layers/water.py` | Water layer (placeholder; dry riverbed mode active) |
| `terrains/tile.py` | Entry point; `build_tile()` (flags) and `build_tile_from_spec()` (YAML) |

`core/` modules are pure primitives (array in / array out). `layers/` modules implement the `.build(scene)` interface. `terrains/` modules are entry points that assemble layers into a full pipeline.

### Tile Spec Format (`.tile` files)

YAML files in `tiles/`. Two region types drive the pipeline:
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

## Known Open Items (from architecture review)

Items tracked in `docs/memory/terminology-tile-square-cell.md`:

1. `SoilConfig.detail_mult` doubles CPU but the hi-res bump is discarded in `terrains/tile.py` — wire or remove
2. `build_sub_hull_mesh` imported but never called — delete from `grass.py`, `mesh.py`, `core/__init__.py`
3. `core/collision.py` entire module is dead — delete + remove re-exports; `SolverConfig.strict_mode` / `strict_base_t` also stranded
4. `TerrainGrid` allocates at heightmap resolution (65k Python objects) — wrong abstraction
5. `GrassLayer` has 5 dead class-level defaults overwritten in `__init__`
6. `GrassSeed.base_x`, `base_y`, `direction` are write-only after construction
7. `cell_mm_h` in `soil.py:46` — rename to `cell_mm`
8. `CELL_SIZE_MM` legacy constant — delete from `config.py` and `core/__init__.py`
9. `cell_h` is always equal to `cell_w` — redundant property
10. `_make_compat_scene` in `tile.py` raises unconditionally — delete
11. `TerrainGrid.fill()` uses `cols.start or 0` wrong idiom

## Project Layout

```
src/dharmatiles/
  core/          pure primitives (config, spec, tile, region, flow, mesh, grid, seed)
  layers/        grass.py, soil.py, stones.py, water.py
  terrains/      tile.py (main entry point)
tiles/           .tile spec files (YAML)
stl/             generated STL output (gitignored)
scripts/         standalone utilities; scripts/archived/ = old generations
docs/            design notes, architecture review transcripts, session memory
docs/meta/history/  architecture review transcripts
docs/memory/     persistent session memory (MEMORY.md index)
```
