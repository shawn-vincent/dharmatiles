# Tile Spec Redesign: Specs as Executable Layer Objects

Status: design — 2026-06-10
Author: design session with Claude

## Summary

The `.tile.py` spec format today is a symbolic data structure: regions and
boundaries hold `LayerSpec(type='grass_carpet', params=dict(...))` entries
where `type` is a magic string and `params` is an opaque dict. The runtime
translates those strings into layer objects through several `_collect_*`
passes in `terrains/tile.py` that reconstruct configs, partition kwargs, and
dispatch on the type string.

This proposal replaces the symbolic spec with a direct one: spec files
construct **the real implementation classes** — `SoilCarpetLayer`,
`GrassCarpetLayer`, `ScatterLayer`, `WaterLayer` — and the orchestrator
just calls `apply()` on each one in spec order. `ScatterLayer` takes the
things to scatter (`Rocks(...)`, `Grass(...)`) as its arguments, preserving
the unified scatter system. No wrappers, no parallel spec hierarchy, no
phase enum, no priority. The spec language is the implementation language.

## Motivation

### Problems with the current symbolic spec

1. **No type safety.** `LayerSpec(type='grsas_carpet', ...)` (typo) is silently
   accepted at construction time; the layer just never executes. Misspelled
   param keys in the `params` dict also slip through in some paths.
2. **Hidden coupling between spec strings and engine code.** `'grass_carpet'`,
   `'soil_carpet'`, `'grass'`, `'rocks'`, `'water'`, `'floor'` are duplicated
   across `core/spec.py` (`HEIGHT_DEFAULTS`), `terrains/tile.py` (every
   `_collect_*` function), and tile specs themselves. Adding a new layer
   requires touching all three.
3. **Param partitioning hacks.** `_collect_grass_carpet_layers` introspects
   `GrassUnderlayConfig` and `SpeciesConfig` field names and partitions a flat
   kwargs dict between them. `_collect_scatter_pairs` does the same for
   `RocksConfig` and for `SpeciesConfig`. This logic is invisible from the
   spec file and breaks the moment field names collide.
4. **Layer extension is out of reach for tile authors.** A user who wants a
   one-off layer must either add it to the engine's collector dispatch or
   shoehorn it into an existing type. There is no in-tile extension point.
5. **Two languages for one job.** The spec language ("type strings + dicts")
   and the implementation language (Python classes) are isomorphic but
   incompatible — every concept exists twice with a translator in between.
6. **Engine-imposed phase machinery for ordering the author already states.**
   The `_collect_*` functions walk the spec multiple times to enforce "all
   soil carpets first, then all grass carpets, then all rocks big→small
   globally, then all grass". The author already wrote those layers in that
   order in the spec — the engine just hides that fact behind dispatch tables.

### What direct construction buys

```python
# Before
LayerSpec(type='grass_carpet', params=dict(species=shared_species, groups_per_square=240))

# After
GrassCarpetLayer(species=shared_species, groups_per_square=240)
```

- IDE autocomplete and mypy on every layer argument.
- Typos surface immediately at import time.
- The `HEIGHT_DEFAULTS` table and all the `_collect_*` functions disappear.
- One mental model: a spec file is just Python that builds a tree of
  executable layer objects, run in spec order.

## Target API

```python
# src/tiles/soil+grass.tile.py
from dharmatiles.spec import Tile, Region, Boundary, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import (
    SoilCarpetLayer, GrassCarpetLayer, ScatterLayer,
)
from dharmatiles.scatter import Rocks, Grass

species = SpeciesConfig()  # shared geometry between carpet + 3D blades

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
    boundaries=[
        Boundary(
            id='margin',
            from_anchor=('top', 0.48), to_anchor=('bottom', 0.52),
            amplitude_mm=5.0, wavelength_mm=10.0,
        ),
    ],
)
```

- `Region.layers` holds **layer instances** of the real implementation classes.
- `ScatterLayer` takes the things to scatter (`Rocks`, `Grass`) as variadic
  positional args. Order inside the `ScatterLayer` decides the order each
  thing is placed — rocks first means grass can steer around them.
- Layer constructors take typed kwargs — no more `params=dict(...)`.
- Sharing a `SpeciesConfig` between `GrassCarpetLayer` and `Grass` is one
  variable.

## Layer interface

```python
class TileLayer(Protocol):
    height_default_mm: ClassVar[float]

    def apply(
        self,
        scene: TileScene,
        *,
        placement_mask: np.ndarray | None,
    ) -> list[trimesh.Trimesh]:
        """Mutate `scene` and/or return mesh parts for the final union."""
```

Layers run in the order they appear in `Region.layers` / `Boundary.layers`.
Regions are walked in `tile.regions` order, then boundaries in
`tile.boundaries` order. `apply()` does whatever it needs to do — modify
`terrain_z`, stamp `terrain_support_z`, mark `rock_mask`, return mesh parts
— and the orchestrator appends those parts to the union.

### Region heights

`HEIGHT_DEFAULTS` disappears. Every layer class carries a class attribute:

```python
@property
def effective_height_mm(self) -> float:
    if self.height_mm is not None:
        return self.height_mm
    return self.layers[0].height_default_mm if self.layers else 5.0
```

### `terrain_support_z` sync

Each carpet-style layer updates `terrain_support_z` alongside `terrain_z`
at the end of its `apply()`. Scatter-style layers read `terrain_support_z`
the same way they do today. No implicit cross-layer sync points.

### Orchestrator

```python
def build_tile(tile: Tile, scene_cfg: SceneConfig) -> trimesh.Trimesh:
    region_mask = build_region_mask(tile)
    terrain_z   = build_terrain_from_heights(tile, region_mask)
    scene = TileScene(config=scene_cfg, terrain_z=terrain_z, ...)

    parts: list[trimesh.Trimesh] = []
    for idx, region in enumerate(tile.regions):
        mask = (region_mask == idx)
        for layer in region.layers:
            parts += layer.apply(scene, placement_mask=mask)
    for boundary in tile.boundaries:
        mask = (region_mask < 0)
        for layer in boundary.layers:
            parts += layer.apply(scene, placement_mask=mask)

    parts.insert(0, make_heightmap_solid(scene.terrain_z, ...))
    return union(parts)
```

That's the whole pipeline. No `_collect_*` functions, no phase enum, no
priority tie-breaks.

## ScatterLayer

`ScatterLayer` is a first-class layer that takes one or more things to
scatter. The things are `Rocks` and `Grass` (renamed from `RockPrototype`
and `GrassPrototype`).

```python
class ScatterLayer:
    height_default_mm = 5.0

    def __init__(self, *things: Scatterable) -> None:
        self.things = things

    def apply(self, scene, *, placement_mask):
        parts: list[trimesh.Trimesh] = []
        for thing in self.things:
            parts += thing.scatter(scene, placement_mask=placement_mask)
        return parts
```

Each thing is responsible for its own seed sampling, within-thing sort
(big→small for rocks; upstream-first for grass), and mesh build:

```python
class Rocks:
    def __init__(self, *, scatter: ScatterConfig | None = None, **rocks_kwargs):
        self.rocks = RocksConfig(**rocks_kwargs)
        self.scatter = scatter   # optional placement override

    def scatter(self, scene, *, placement_mask) -> list[trimesh.Trimesh]:
        # sample positions, build seeds, sort big→small, build mesh,
        # stamp rock_mask + terrain_support_z, return mesh part.
        ...

class Grass:
    def __init__(self, species: SpeciesConfig | None = None, *,
                 scatter: ScatterConfig | None = None,
                 **species_overrides):
        base = species or SpeciesConfig()
        self.species = (dataclasses.replace(base, **species_overrides)
                        if species_overrides else base)
        self.scatter = scatter

    def scatter(self, scene, *, placement_mask) -> list[trimesh.Trimesh]:
        # plant seeds, grow blades, stamp vegetation_support_z, return parts.
        ...
```

What this preserves from the current scatter system:
- Unified seed → sort → realise pipeline shared across rock and grass.
- Per-thing `ScatterConfig` for spacing / grouping overrides.
- Grass natively reading `rock_mask` and `terrain_support_z` updated by
  any `Rocks` that ran earlier in the same `ScatterLayer`.

What it drops:
- Cross-region big→small rock sorting. Each `ScatterLayer` instance sorts
  independently. Rocks are region-masked, so visual impact is negligible.
- The `ScatterLayer` meta-layer's hard-coded "priority 0 then priority 1"
  ordering. The author writes `Rocks` before `Grass` and that's the order.
- `sort_priority` class attributes on prototypes.

## Implementation: retrofit existing classes

The redesign **does not** add new wrapper classes. Existing implementations
become the spec-facing classes directly.

### Per-class changes

**`SoilCarpetLayer`** (`layers/soil.py`)
- Today: `__init__(surface, soil_config)`; `build(scene, placement_mask)`.
- New: `__init__(**soil_kwargs)` — flat kwargs build the `SoilConfig`.
  `apply(scene, *, placement_mask)` reads `surface` from
  `scene.config.surface` and ends with `terrain_support_z[:] = terrain_z`.
- Add: `height_default_mm = 5.0` class attribute.

**`GrassCarpetLayer`** (`layers/grass_carpet.py`)
- Today: `__init__(grass_underlay_config)`; `build(scene, placement_mask)`.
- New: `__init__(species=None, **carpet_kwargs)` — flat kwargs build
  `GrassUnderlayConfig`. `apply(...)` with terrain-support sync.
- Add: `height_default_mm = 5.0`.

**`ScatterLayer`** (`scatter/layer.py`)
- Today: `__init__(prototype_mask_pairs)`; `build(scene, ...)` runs
  prototypes in `sort_priority` order across a flat list.
- New: `__init__(*things)`; `apply(scene, *, placement_mask)` runs each
  thing in spec order, passing the region mask down.
- Add: `height_default_mm = 5.0`.

**`Rocks`** (renamed from `RockPrototype` in `scatter/prototype.py`)
- Today: `__init__(rocks_config, scatter_config)`; exposes
  `make_seed(...)` + `realize(seeds, scene, surface)`.
- New: `__init__(*, scatter=None, **rocks_kwargs)`. The seed/realize split
  collapses into one `scatter(scene, *, placement_mask)` method that
  samples positions, builds + sorts seeds, builds the mesh, stamps
  `rock_mask` and `terrain_support_z`, and returns the mesh part.
- Drop: `sort_priority` class attribute.

**`Grass`** (renamed from `GrassPrototype` in `scatter/prototype.py`)
- Today: `__init__(species, scatter)`; `realize(scene, surface, ...)`
  delegates to `FloppyGrassLayer`.
- New: `__init__(species=None, *, scatter=None, **species_overrides)`.
  `scatter(scene, *, placement_mask)` does the seed planting + growth +
  mesh build inline (or keeps `FloppyGrassLayer` as an internal helper if
  the body is large — but no longer a public API).
- Drop: `sort_priority` class attribute.

**`WaterLayer`** (new, in `layers/water.py`)
- New class consolidating `make_water_displacement`,
  `make_water_ripple_displacement`, `make_water_volume`, the
  `_extend_bank_slope_into_pool` logic, and the pool-floor flattening
  currently scattered between `layers/water.py` and `terrains/tile.py`.
- `__init__(*, embed_mm=2.0)`.
- `apply(scene, *, placement_mask)` reshapes `scene.terrain_z` (extend bank
  slope, flatten pool floor), builds and returns the volume mesh.
- `height_default_mm = 3.0`.

### What disappears

- `RockPrototype`, `GrassPrototype` — renamed to `Rocks`, `Grass` with
  their seed/realize split collapsed.
- `TileSpec`, `RegionSpec`, `LayerSpec`, `BoundarySpec`,
  `BoundaryLayerSpec` (`core/spec.py`) — replaced by `Tile`, `Region`,
  `Boundary`.
- `HEIGHT_DEFAULTS` — replaced by `height_default_mm` class attributes.
- `_collect_scatter_pairs`, `_collect_soil_carpet_layers`,
  `_collect_grass_carpet_layers`, `_build_mesh`, `_collect_water_info`
  (`terrains/tile.py`) — replaced by the flat orchestrator loop.
- The legacy `RocksLayer` in `layers/rocks.py` (already unused) — delete.
- The abstract `GrassLayer` base in `grass/layer.py` (unused) — delete.

### What stays

- `ScatterConfig`, `RockSeed`, `GrassSeed`, `scatter/distribute.py`
  (Voronoi grouping, jitter grid, `scatter_positions`). Internals used by
  `Rocks` and `Grass`; users only touch them via the optional `scatter=`
  kwarg.
- `SurfaceConfig`, `SoilConfig`, `RocksConfig`, `SpeciesConfig`,
  `GrassUnderlayConfig`, `BaseConfig` — config dataclasses are still useful
  for gathering many parameters into one shareable object.
- `_build_rocks_mesh_from_seeds`, `_build_rocks_mesh_core` — the
  vectorised mesh builder; `Rocks.scatter()` calls them.
- `FloppyGrassLayer` may stay as an internal implementation that `Grass`
  delegates to, but it loses its public API status.

## Migration plan

1. **Add `dharmatiles/spec.py`** with `Tile`, `Region`, `Boundary`, and the
   `TileLayer` protocol. Re-export `SurfaceConfig` / `SpeciesConfig`.
2. **Add `dharmatiles.layers`** umbrella re-exporting `SoilCarpetLayer`,
   `GrassCarpetLayer`, `ScatterLayer`, `WaterLayer`.
3. **Add `dharmatiles.scatter`** umbrella re-exporting `Rocks`, `Grass`,
   `ScatterConfig`.
4. **Retrofit each class** as listed above: flat-kwargs constructors,
   `build` → `apply` rename, `height_default_mm` class attribute,
   per-layer `terrain_support_z` sync.
5. **Build `WaterLayer`** by lifting water-specific code out of
   `terrains/tile.py` into `layers/water.py`.
6. **Rename `RockPrototype` → `Rocks`, `GrassPrototype` → `Grass`** and
   collapse their seed/realize split into a single `scatter()` method.
7. **Rewrite `terrains/tile.py`** as the flat orchestrator. Delete every
   `_collect_*` function and `_build_mesh`.
8. **Convert each `.tile.py` in `src/tiles/`** to the new API (six files).
   Order layers carpets → scatter → water to match today's visual output.
9. **Regenerate all STLs** and verify byte equivalence (or document any
   intentional diffs).
10. **Delete old spec types**: `TileSpec`, `RegionSpec`, `LayerSpec`,
    `BoundarySpec`, `BoundaryLayerSpec`, `HEIGHT_DEFAULTS`. `load_spec`
    stays but returns a `Tile`.

This is a breaking change for any out-of-tree spec files (none in the repo
today). No deprecation shim — clean cut.

## Risks

1. **Order matters now.** A user who puts `[FloppyGrass, Rocks]` inside a
   `ScatterLayer` gets grass that doesn't steer around rocks. We treat this
   as a feature, not a foot-gun. Every example tile uses the natural order;
   `Region.layers` / `ScatterLayer.__init__` docstrings call out the
   convention.
2. **Per-layer `terrain_support_z` sync.** One full-grid copy per carpet
   layer instead of one total. Grids are small; negligible.
3. **Cross-region big→small rock sort is gone.** Each `ScatterLayer`
   instance sorts independently. Visual impact should be zero since rocks
   are region-masked, but worth eyeballing the regenerated STLs.
4. **`FloppyGrass` was multi-species per region.** Today
   `GrassConfig.species: list[SpeciesConfig]` lets a single grass layer mix
   species. The new model is one species per `Grass` instance; multi-species
   regions become multiple `Grass` entries inside one `ScatterLayer`. No
   live tile uses the multi-species form.
5. **`floor` and `wall`** in `HEIGHT_DEFAULTS` have no engine
   implementation today. The redesign drops them silently; reintroduce as
   real classes when there is code to back them.
6. **Inline custom layers** work for free because spec files are executed
   Python. We don't ship an example tile for this — it just works for
   anyone who needs it.

## Out of scope

- Slope-aware feature placement (still uses `terrain_normal()` future work).
- Variants / per-size overrides — the new `Tile` dataclass can host this
  later; current `# TODO: variants` note carries over.
- Replacing the `.tile.py` execution model with a sandboxed loader.

## Confirmed decisions

- **Flat kwargs** on layer and scatter-thing constructors. Plus an
  optional config-object kwarg (`rocks=RocksConfig(...)`,
  `species=SpeciesConfig(...)`) for shared configs.
- **Per-layer `terrain_support_z` sync** at the end of each carpet's
  `apply()`.
- **No custom-layer example tile.** Inline custom layers work; users who
  need them will discover.
- **No layer-order linting.** Trust the spec author.
- **Keep `ScatterLayer`** as a first-class layer the user constructs
  directly, taking `Rocks` / `Grass` as the things to scatter.
