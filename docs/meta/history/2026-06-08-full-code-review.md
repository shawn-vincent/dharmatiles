# Full Code Review — 2026-06-08

**Scope:** Complete read of `core/`, `grass/`, `layers/`, `terrains/tile.py`, `bases/`, and `.tile` specs.
Also compared CLAUDE.md documentation against the live codebase.

---

## Top 5 Recommendations

### 1. Grass-package DRY emergency — 7 functions duplicated across 3 modules

The grass subpackage has no shared geometry module.  The same low-level helpers
appear verbatim in `grass/growers/flat.py`, `grass/mesh.py`, and `grass/grow.py`:

| Function | flat.py | mesh.py | grow.py |
|---|---|---|---|
| `_spine_distances` (6 lines) | ✓ L140 | ✓ L59 | — |
| `_sample_grid` scalar bilinear (14 lines) | ✓ L374 | ✓ L88 | — |
| `_cell_index` (3 lines) | ✓ L368 | — | ✓ L429 |
| `_contained_segment_cells` / `_segment_cells` (~75 lines) | ✓ L461+L526 | ✓ L181 | — |

The segment-containment code is the most dangerous: `flat.py` has both
`_contained_segment_cells` (thin wrapper) and `_segment_cells` (the real body),
while `mesh.py` has its own `_contained_segment_cells` with a slightly different
corner ordering.

Additionally, `core/grid.py::rasterise_into_support` (exported in `core/__init__.py`)
appears to be **dead code** in the current build pipeline — the pipeline uses
`_stamp_swept_footprint` and `_rasterise_sloped_path` in the grass package instead.

**Fix:** create `grass/_geometry.py` with canonical versions of all five helpers;
delete the copies; remove the dead `rasterise_into_support` export.

---

### 2. `TerrainGrid` is disconnected from the pipeline and has a broken blend

`core/terrain.py:TerrainGrid` is imported by `core/tile.py` but **never constructed
in the actual tile-generation path**.  `build_tile_from_spec` computes `terrain_z`
directly from `_build_spec_terrain()` (IDW via scipy), bypassing `TerrainGrid`
entirely.  `TileScene.from_terrain_grid` is never called.

The class also has a **structural bug** in `terrain_grid_to_heightmap._blend_pair`:
both `cc_n` (high-side) and `cc_f` (low-side) indices are computed but only `cc_n`
is written to `z_blend`.  The low-side transition is silently discarded, making the
soft ground↔water blend one-sided regardless of the radius setting.

```python
# terrain.py:241-251
rr, cc_f = r0, c_lo + offset - 1   # ← computed but never read
...
z_blend[idx] = z_low + t_s * (...)  # ← only applies cc_n side
```

Since the class is not in the live pipeline, even fixing the blend has no effect
on output.  CLAUDE.md item 4 (the only surviving known-open item) understates the
problem — the class is architecturally orphaned, not just slow.

**Fix:** Either wire `TerrainGrid` into the pipeline (replacing the IDW approach)
or delete it along with `TerrainCell`, `terrain_grid_to_heightmap`,
`TileScene.from_terrain_grid`, and the import chain.

---

### 3. Grass species are region-unaware — silent correctness bug

`_collect_grass_configs` in `terrains/tile.py` harvests all `grass` `LayerSpec`s
from all regions into a flat list with no region association.  Each resulting
`SpeciesConfig` is then grown over the **entire combined `grass_mask`** (all grass
regions merged).

Compare to `_collect_soil_layers` and `_collect_stones_layers`, which both correctly
pair each config with a per-region placement mask:

```python
# soil and stones do this:
mask = (region_mask == idx) if region_mask is not None else None
result.append((SoilConfig(...), mask))

# grass does NOT:
cfgs.append(SpeciesConfig(**d))   # no region mask, grows everywhere
```

For current specs (each has exactly one grass region) this is invisible.  For a
spec with two grass regions using different species, both species invade both regions.

**Fix:** Change `_collect_grass_configs` to return
`list[tuple[SpeciesConfig, np.ndarray | None]]` pairs (mirroring the other two
collectors), and thread the per-region mask into the seeder via `_build_mesh`.

---

### 4. `scene.terrain_z` mutated inside `_build_mesh` against its own contract

`TileScene`'s docstring says `terrain_z` is "read-only after init".
`_build_mesh` violates this at `terrains/tile.py:82`:

```python
if water_mask is not None:
    scene.terrain_z[water_mask] = 0.0   # destructive mutation
scene.terrain_support_z[:] = scene.terrain_z
```

Three compounding problems:

1. **Contract breach.** Any caller that reads `terrain_z` after `_build_mesh`
   returns gets corrupted data (water cells are now 0.0, not their natural heights).
2. **Wrong constant.** `_extend_bank_slope_into_pool` uses a named `terrain_z_min = 0.0`.
   Hard-coding `0.0` here creates a silent coupling.  If the minimum slab floor
   changes, this line won't track it.
3. **Wrong location.** This mutation happens *after* soil has baked into `terrain_z`.
   Soil should never have seen the pool in the first place.

**Fix:** Apply the water-floor zero-out in `build_tile_from_spec` *before* the
scene is created, as an explicit shaping step on the raw heightmap, not inside
a mesh-building helper.

---

### 5. CLAUDE.md "Known Open Items" section is 90% stale

The section lists 11 items.  A grep of the current codebase shows **10 of 11 are
already resolved or never existed in this codebase**:

| Item | Status |
|---|---|
| 1. `SoilConfig.detail_mult` doubles CPU | ❌ not in code |
| 2. `build_sub_hull_mesh` never called | ❌ not in code |
| 3. `core/collision.py` is dead | ❌ not in code |
| 4. `TerrainGrid` 65 k Python objects | ✅ **still live** — also disconnected |
| 5. `GrassLayer` 5 dead class-level defaults | ❌ `__init__` is clean |
| 6. `GrassSeed.base_x/y/direction` write-only | ❌ those fields don't exist |
| 7. `cell_mm_h` rename | ❌ already `cell_mm` in soil.py |
| 8. `CELL_SIZE_MM` legacy constant | ❌ not in code |
| 9. `cell_h` always equals `cell_w` | ❌ `cell_h` property doesn't exist |
| 10. `_make_compat_scene` unconditional raise | ❌ not in code |
| 11. `TerrainGrid.fill()` wrong `cols.start or 0` idiom | ❌ fixed (`is not None`) |

The list misses all four items above (DRY crisis, disconnected `TerrainGrid` detail,
species region-blindness, `terrain_z` mutation contract).

**Fix:** Clear the section; add the four real open items; note that `TerrainGrid`'s
problem is broader than performance.

---

## Minor Findings (not top-5)

- **`SoilConfig.blob_warp_str_mm` and `blob_texture_amp` default 0.0 but still
  allocate noise fields.**  `_compute_bump_field` generates full `wx`, `wy`, `tex`
  arrays unconditionally; the "disabled" parameters are gated by `if wx is not None`
  which is never None.  The disable path doesn't actually skip the allocation.

- **`scipy.ndimage.binary_dilation` imported inside `_build_mesh` function body**
  (tile.py:117).  All other scipy imports are at module top; this one is an outlier
  that will fail at import time if scipy is missing — only when a tile has water.

- **`SoilConfig.blob_h_min` / `blob_h_max`** are documented "floor/ceiling for
  secondary tier" but are also the fallback height range for the primary tier when
  `not perturb` (soil.py:174).  The comment misleads about secondary-tier exclusivity.

---

## Documentation Drift Summary

| File | Drift |
|---|---|
| `CLAUDE.md` "Known Open Items" | 10/11 items stale; all 4 real items missing |
| `core/terrain.py` docstring | Describes `TerrainGrid` as wired in ("wired to all entry points") but it isn't |
| `core/tile.py` `TileScene` docstring | "terrain_z read-only after init" violated by `_build_mesh` |
| `core/grid.py` | `rasterise_into_support` exported and documented but apparently dead |
