# DharmaTiles Elegance Review — 2026-06-18

**Scope:** End-to-end design review: simplicity, elegance, conceptual clarity.
**Not:** DRY/duplication (see 2026-06-13 review for those).
**Reviewer:** Orin (claude-sonnet-4-6)
**Lines surveyed:** ~12,000 across 50+ files

---

## Status as of 2026-06-18 (post-review)

**Resolved:**
- ✅ Finding 1 — `core/terrain.py` deleted (18b7e5c area)
- ✅ Finding 2 — `SceneConfig` deleted
- ✅ Finding 3 — `core/color.py` split; `core/export_3mf.py` extracted (4b23608)
- ✅ Finding 4 — compat `__init__` removed from `SpeciesConfig`/`SoilConfig`/`RocksConfig` (802c111)
- ✅ Finding 5 — `FloppyGrassLayer` class removed; `grass/layer.py` deleted
- ✅ Finding 7 — `TreeShape` wrapper flattened into `Tree`
- ✅ Finding 9 — `GrassConfig.species` collapsed to single `SpeciesConfig`
- ✅ Finding 12 — `TileScene.parts` field removed
- ✅ Finding 14 — `load_spec` alias removed
- ✅ Finding 15 — String-keyed `_LAYER_TERRAIN_MATERIAL` dict replaced with `terrain_material: ClassVar[Material]` protocol on layers

**All open findings resolved (second pass, 2026-06-18):**
- ✅ Finding 6 — `growers/` subpackage collapsed: `growers/flat.py` → `grass/grower.py`; subpackage deleted
- ✅ Finding 8 — `FlatHeight` removed; `Region` uses `height_mm: float` directly; all water tiles updated to `height_mm=3.0`
- ✅ Finding 10 — `trees/_utils.py` created with `_safe_norm`, `_hash01`, `_WUP_VEC`; dead `_hash01` in `leaf.py` removed; both modules import from `_utils`
- ✅ Finding 11 — `_stamp_tree` vectorised with meshgrid + boolean mask
- ✅ Finding 13 — `TileScene.derive_seed()` method removed; one caller in `grass_carpet.py` updated to free function
- ✅ Finding 16 — `export()` removed from `bases/dungeonblocks.py` and `bases/openlock.py`; shared `_attach_and_export()` helper added to `systems.py`; `DungeonBlocks.export()` and `OpenLOCK.export()` call `make_base()` + helper directly

---

## Executive Summary

The codebase is architecturally sound at a high level. The pipeline is linear and easy to trace. The tile spec language is clean and expressive. The scatter system is genuinely well-unified. The grower/layer/scene separation holds.

The entropy lives in specific seams: a dead abstraction layer (TerrainType), a ghost field (SceneConfig), a god-module (core/color.py), configuration classes that use an anti-pattern to achieve immutability, several single-use wrapper classes that add ceremony without value, and a few naming/structural inconsistencies that create false complexity. None of these are load-bearing failures — the codebase works — but they are drag.

---

## Finding 1 — `TerrainType` enum is dead code [HIGH] ✅ DONE

**File:** `src/dharmatiles/core/terrain.py`

`TerrainType` defines `WATER`, `GROUND`, `GRASS`, `CONSTRUCTED_FLOOR`, `WALL`, etc. with helpers `default_height()` and `transition_style()`. The module docstring says "`TerrainGrid` was removed." The enum itself is referenced nowhere in the live pipeline. No orchestrator step reads it. No layer assigns it. No spec file imports it.

This is a planning artifact from an earlier design that was superseded before reaching completion. The concept it represents — typed terrain regions with built-in defaults — was replaced by the simpler `FlatHeight(mm)` approach where heights are explicit per-region values declared in the spec.

**Effect:** Zero functional harm, but a new contributor reading the codebase will reasonably assume `TerrainType` is load-bearing, spend time understanding it, and discover it connects to nothing. The longer it lives, the more it will mislead.

**Fix:** Delete `core/terrain.py` entirely.

---

## Finding 2 — `SceneConfig` is a ghost [HIGH] ✅ DONE

**File:** `src/dharmatiles/core/config.py` (lines 594–606)

`SceneConfig` bundles `SurfaceConfig`, `SoilConfig`, `RocksConfig`, `BaseConfig`, and `max_stack_height`. Its docstring says "layers receive only the sub-config they need; none reads across layer boundaries." This is good doctrine, but the object itself is never constructed in the live pipeline. `build_tile_from_spec()` in `terrains/tile.py` constructs a `TileScene` directly from a `Tile` spec; it never touches `SceneConfig`. Nothing in `layers/`, `scatter/`, `grass/`, or `terrains/` imports or uses it.

`SceneConfig` appears to be a predecessor to the `Tile` dataclass — a config-bundle design that was superseded when the spec language became live Python objects. The two bundled configs it references (`SoilConfig`, `RocksConfig`) now live directly on their layer instances.

**Fix:** Delete `SceneConfig` from `core/config.py`. If it is ever needed again (e.g. for a programmatic API that bypasses spec files), it can be reconstructed from the existing primitives trivially.

---

## Finding 3 — `core/color.py` is three modules in a trench coat [HIGH] ✅ DONE (4b23608)

**File:** `src/dharmatiles/core/color.py`

The module started with a clear purpose: material tagging (`Material` enum, `RGBA` palette, `tag()`, `export_color_stl()`). It now also contains:

- `build_scene()` — constructs a manifold3d scene for 3MF export
- `_terrain_group()` — groups tile meshes by type
- `_size_dims()` — computes tile print dimensions
- `_pack_plates()` — plate packing layout algorithm (Bambu X1C 256mm plate)
- `export_3mf_colored()` — ~350 lines of 3MF XML generation

The module name `color.py` no longer describes its contents. The plate-packing algorithm, the 3MF XML writer, and the material palette are three completely separate concerns.

Callers import from it in ways that reveal the scope creep: `terrains/tile.py` imports `build_scene` and `export_3mf_colored` from `core.color`, making the orchestrator depend on an export module nominally named "color".

**Fix:** Split into:
- `core/color.py` — keeps `Material`, `RGBA`, `DEBUG_COLORS`, `debug_material()`, `tag()`, `export_color_stl()`
- `export/export_3mf.py` (or `core/export_3mf.py`) — takes `build_scene`, `_pack_plates`, `export_3mf_colored` and supporting privates

No functional change needed; just a move. Callers update their imports.

---

## Finding 4 — Frozen dataclass + custom `__init__` anti-pattern [HIGH] ✅ DONE (802c111)

**Files:** `src/dharmatiles/core/config.py` — `SpeciesConfig`, `SoilConfig`, `RocksConfig`

All three use the same pattern:
```python
@dataclass(frozen=True, init=False)   # or @dataclass(init=False)
class SpeciesConfig:
    field_a: type = default
    ...
    def __init__(self, ...) -> None:
        # ... validation, range compat ...
        values = {"field_a": value_a, ...}
        for field_name, value in values.items():
            object.__setattr__(self, field_name, value)  # or setattr
```

The `values` dict is pure ritual. Its only purpose is to avoid writing `object.__setattr__` twenty times. But `@dataclass(frozen=True)` with `__post_init__` is the idiomatic Python way to do exactly this:

```python
@dataclass
class SpeciesConfig:
    blade_width: Sample[float] = 1.2
    ...
    def __post_init__(self) -> None:
        self.blade_width = _range_compat("blade_width", self.blade_width, ...)
```

Or, if immutability post-construction is important, use `@dataclass(frozen=True)` with a classmethod factory or keep the `__init__` but drop the dict loop and just call `object.__setattr__` directly per field.

The current approach confuses readers because `@dataclass(frozen=True, init=False)` says "this class has no dataclass-generated init" but it *does* have an init — a manually written one. The field declarations with their defaults serve only as documentation; the actual default logic lives in the `__init__`. This means default values are declared in two places: the dataclass field annotation AND the `__init__` parameter defaults. Drift between them is an active maintenance hazard.

`SoilConfig` is `@dataclass(init=False)` (not frozen) and still uses the dict loop pattern — here there is zero justification, since `setattr(self, k, v)` in a loop is strictly equivalent to `self.k = v` but less readable.

**Fix (two paths):**

Path A (prefer): Drop `frozen=True` on all three; use `@dataclass` with `__post_init__` for validation. Declare real defaults in field annotations. Accept that tile specs calling `SpeciesConfig()` can in theory mutate the result — this is not a real risk since spec objects are ephemeral.

Path B (keep frozen): Keep `@dataclass(frozen=True)` but eliminate the dict loop. Write the `object.__setattr__` calls directly or factor them into a `_set_frozen(self, **kwargs)` helper called once at the end of `__init__`.

---

## Finding 5 — `FloppyGrassLayer` is ceremony around two function calls [MEDIUM] ✅ DONE (grass/layer.py deleted)

**File:** `src/dharmatiles/grass/layer.py`

`FloppyGrassLayer` is a class with a single method `build()` that calls `grow_all()` then `build_meshes()`. It holds a `GrassConfig` that it passes through. It is instantiated once, immediately used, and discarded inside `Grass.scatter()`:

```python
layer = FloppyGrassLayer(config)
paths = layer.build(scene, ...)
```

This could be written as two direct function calls. The class exists only to bundle `grow_all` + `build_meshes` with a config — which is exactly what a function does. The name `FloppyGrassLayer` implies it participates in the `TileLayer` protocol (it has an `apply()` method), but `Grass.scatter()` calls `build()`, not `apply()`.

The word "Floppy" in the name is also leakage: it refers to the `FlatGrassGrower` (aliased "floppy" in the GROWERS registry) which is the only concrete grower. The abstraction anticipates multiple grower types that have not materialised in two years.

**Fix:** Replace `FloppyGrassLayer` with a free function `build_grass(config, scene, ...)` that does `paths = grow_all(...); return build_meshes(paths, ...)`. Delete the class. The `Grass.scatter()` caller calls the function directly.

---

## Finding 6 — `GROWERS` registry with one entry [MEDIUM] ✅ DONE (`growers/flat.py` → `grass/grower.py`; subpackage deleted)

**File:** `src/dharmatiles/grass/growers/__init__.py`

```python
GROWERS = {
    "floppy": FlatGrassGrower,
}
```

The registry maps string names to grower classes. There is exactly one entry. `SpeciesConfig.grower` is always `"floppy"`. The registry mechanism — dict + string key + dynamic lookup — exists entirely to support a multi-grower future that has not arrived.

The `grower: str = "floppy"` field on `SpeciesConfig` is a stringly-typed reference with no validation at assignment time. A typo (`"fllopy"`) silently passes through config construction and only fails at runtime when the GROWERS lookup returns `None`.

The `growers/` subdirectory with its `__init__.py` and `flat.py` adds a layer of directory structure for a single concrete implementation.

**Fix (now):** Inline the `FlatGrassGrower` class into `grass/grow.py` or rename it to just `GrassGrower`. Remove the registry dict and the `grower: str` field from `SpeciesConfig`. Delete the `growers/` subdirectory.

**Fix (if multi-grower ever arrives):** When a second grower is added, reintroduce the registry — but type it as `dict[str, type[GrassGrower]]` and validate the `grower` field against it in `SpeciesConfig.__post_init__`.

---

## Finding 7 — `TreeShape` wrapper adds one indirection for zero value [MEDIUM] ✅ DONE (flattened into Tree)

**File:** `src/dharmatiles/trees/layer.py` (lines 19–28, 91–100)

`TreeShape` is a frozen dataclass grouping `height_mm`, `trunk_height_mm`, `crown_radius_mm`, `crown_base_radius_mm`, and four profile parameters. It is constructed once in `Tree.__init__()` from the same-named constructor arguments, stored as `self.shape`, and then accessed field-by-field in `_sample_envelope()`:

```python
height = max(0.0, float(sample(self.shape.height_mm, rng)))
trunk  = float(np.clip(sample(self.shape.trunk_height_mm, rng), 0.0, height))
```

`TreeShape` adds exactly one level of indirection (`self.shape.height_mm` vs `self.height_mm`) with no encapsulated behavior — no methods, no validation, no derived properties. The grouping intent is achieved at the cost of an extra wrapping object.

The `Tree.__init__` signature lists the same eight parameters that `TreeShape` holds, then constructs `TreeShape` from them. If `TreeShape` were removed, those eight parameters would become direct attributes of `Tree` (as the other 35+ Tree init parameters already are).

**Fix:** Flatten `TreeShape` into `Tree`. Store the eight fields directly as `self.height_mm`, etc. Delete the `TreeShape` dataclass. `_sample_envelope` reads `self.height_mm` directly. No behavioral change.

---

## Finding 8 — `FlatHeight` wraps a single float [MEDIUM] ✅ DONE (removed; `Region.height_mm` is now the direct field; all water tiles updated)

**File:** `src/dharmatiles/spec.py` (lines 164–177)

```python
@dataclass(frozen=True)
class FlatHeight:
    height_mm: float
```

`FlatHeight` exists to support planned future terrain types (`GaussianMound`, `HeightmapPNG`). The docstring describes the intended protocol. In practice, the codebase has exactly one concrete terrain type and the `Region.terrain` field is always a `FlatHeight`.

The cost: every region height access goes through `.terrain.height_mm`. The `Region.effective_height_mm` property exists specifically to unwrap this: `return self.terrain.height_mm if self.terrain is not None else 5.0`. There is also a backward-compat `height_mm` shortcut field on `Region` that constructs a `FlatHeight` in `__post_init__`. Three mechanisms serve one value.

The wrapping pattern is correct IF the protocol solidifies into multiple implementations. But until a second terrain type exists, `FlatHeight` is speculative abstraction. The protocol makes the code harder to read now in exchange for a refactor it may save later.

**Assessment:** This is a judgment call. If `GaussianMound` is genuinely imminent, keep it. If it has been "planned" for more than six months without arriving, flatten it: store `height_mm: float` directly on `Region`, remove `FlatHeight`, remove `effective_height_mm`, fix the three call sites in `_build_spec_terrain`. The backward-compat `height_mm` shortcut on `Region` already satisfies the user-facing API.

---

## Finding 9 — `GrassConfig.species` is `list[SpeciesConfig]` but always length 1 [MEDIUM] ✅ DONE (collapsed to single SpeciesConfig)

**File:** `src/dharmatiles/core/config.py` (line 156)

```python
@dataclass(frozen=True)
class GrassConfig:
    species: list[SpeciesConfig] = field(default_factory=lambda: [SpeciesConfig()])
```

Every call site accesses `config.species[0]`. No code iterates over multiple species. The list was presumably intended to support multi-species patches, but that feature was never implemented. The `[0]` indexing appears throughout grass seeding code as a constant accessor.

Meanwhile, `Grass` (the scatter thing) takes a `species: SpeciesConfig` parameter directly — a single species, not a list. The tile spec API (`Grass(species=my_species)`) is correctly singular. Only the internal `GrassConfig` wrapper retains the list illusion.

**Fix:** Change `GrassConfig.species` to `species: SpeciesConfig`. Update the two internal sites that index `[0]`. The user-facing API is unchanged (it never saw `GrassConfig` directly).

---

## Finding 10 — `_hash01` and `_safe_norm` duplicated in the trees subpackage [MEDIUM] ✅ DONE (`trees/_utils.py` created; dead `_hash01` in `leaf.py` removed)

**Files:** `src/dharmatiles/trees/cloud_mesh.py`, `src/dharmatiles/trees/leaf.py`

Both files define identical `_hash01` and `_safe_norm` private functions. The docstring in `leaf.py` explains: "tiny shared helpers (self-contained so this module has no tree imports)." The motivation is to avoid circular imports within the `trees/` package.

The circular-import concern is real: `leaf.py` is imported by `cloud_mesh.py`, so if `leaf.py` imported from `cloud_mesh.py` for helpers, there would be a cycle. But the helpers aren't in `cloud_mesh.py` — they could go in `trees/__init__.py` or a new `trees/_utils.py` with no import cycles.

**Fix:** Add `src/dharmatiles/trees/_utils.py` with `_hash01`, `_safe_norm`, and `_WUP_VEC` (the module constant in `cloud_mesh.py` that shadows `_WUP` locally — a naming inconsistency in the same file). Import from `_utils` in both `cloud_mesh.py` and `leaf.py`.

---

## Finding 11 — `_stamp_tree` uses a Python nested loop [MEDIUM] ✅ DONE (vectorised with meshgrid + boolean mask)

**File:** `src/dharmatiles/trees/layer.py` (lines 275–290)

`_stamp_tree` iterates a Python double loop over `(j0..j1) × (i0..i1)` to rasterize a circular footprint into `terrain_support_z` and `obstacle_mask`. This is the only Python nested loop in the hot path that has vectorized alternatives readily available:

```python
yy = np.arange(j0, j1 + 1) * cw
xx = np.arange(i0, i1 + 1) * cw
YY, XX = np.meshgrid(yy, xx, indexing='ij')
in_circle = (XX - x)**2 + (YY - y)**2 <= rr**2
scene.terrain_support_z[j0:j1+1, i0:i1+1][in_circle] = np.maximum(
    scene.terrain_support_z[j0:j1+1, i0:i1+1][in_circle],
    env.terrain_z + env.height_mm,
)
if scene.obstacle_mask is not None:
    scene.obstacle_mask[j0:j1+1, i0:i1+1][in_circle] = True
```

For large trees (radius ~15mm, cell_w ~0.27mm) the loop iterates ~(60×60=3600) iterations per tree. With 4 trees per tile this is negligible. The vectorized version is clearer.

---

## Finding 12 — `TileScene.parts` is an unused field [LOW] ✅ DONE (field removed)

**File:** `src/dharmatiles/core/tile.py` (line 80)

`TileScene` declares `parts: List[trimesh.Trimesh] = field(default_factory=list)`. The pipeline architecture has layers **returning** their meshes via `apply()` — they never append to `scene.parts`. The accumulator for returned meshes is a local `parts: list` in `_build_tile_mesh`. The `scene.parts` field is always empty when the pipeline finishes.

**Fix:** Remove the `parts` field from `TileScene`. If it is being kept for a future "layers can push to scene" design, document that intention explicitly. As-is it is noise.

---

## Finding 13 — `derive_seed` is both a free function and a method [LOW] ✅ DONE (`TileScene.derive_seed()` method removed; `grass_carpet.py` updated to use free function)

**File:** `src/dharmatiles/core/tile.py` (lines 36–47, 122–129)

`derive_seed(master, label, layer_idx)` exists as a module-level free function. It is also a method on `TileScene` that delegates to the free function with `self.surface.seed` as the master. Having both means callers must decide which to use; in practice most callers import the free function (more explicit), making the method mostly decorative.

This is a minor issue but adds conceptual surface. The method is convenience sugar — documenting that it exists should suffice, or the method should be the only path (but then callers who only have a seed integer can't use it). Currently it is the worst of both worlds: two functions, both public, doing the same thing.

---

## Finding 14 — The `load_spec` alias is a ghost export [LOW] ✅ DONE (alias removed)

**File:** `src/dharmatiles/spec.py` (line 372)

```python
load_spec = load_tile   # backward-compat alias
```

`load_spec` is listed in `__all__` and exported. The entire codebase now uses `load_tile`. The only active `load_spec` call remaining is in `terrains/tile.py` at line 872 inside `_render_all_pngs`. The rest of the file uses `load_tile`.

**Fix:** Change the one remaining `load_spec` call to `load_tile`. Remove the alias and the `__all__` entry. The alias has fulfilled its migration purpose.

---

## Finding 15 — `_LAYER_TERRAIN_MATERIAL` uses type-name strings for dispatch [LOW] ✅ DONE (replaced with `terrain_material: ClassVar[Material]` protocol on layers)

**File:** `src/dharmatiles/terrains/tile.py` (lines 159–166)

```python
_LAYER_TERRAIN_MATERIAL: dict[str, "Material"] = {
    'GrassCarpet':      Material.GRASS,
    'GrassCarpetLayer': Material.GRASS,
    'SoilCarpet':       Material.SOIL,
    ...
}
```

The comment says "uses type-name strings to avoid cross-package imports." The strings map `type(layer).__name__` to a material. This means adding a new layer class with terrain-coloring semantics requires updating this dict, which is invisible to the layer author. A rename of any listed class silently breaks coloring.

The principled fix is a protocol: layers that want to declare a terrain material implement `terrain_material: ClassVar[Material]`. Then `_color_terrain_faces` checks for that attribute. No dict, no strings, no cross-package coupling — callers that define the attribute opt in automatically.

This is a small surface but represents the wrong direction (stringly-typed dispatch vs. structural typing).

---

## Finding 16 — `systems.py` / `bases/` double indirection [LOW] ✅ DONE (`export()` removed from both base modules; `_attach_and_export()` helper in `systems.py`; bases expose only `make_base()`)

**Files:** `src/dharmatiles/systems.py`, `src/dharmatiles/bases/dungeonblocks.py`, `src/dharmatiles/bases/openlock.py`

The call chain for building a DungeonBlocks tile is:

```
tile.systems[0].export(...)       # DungeonBlocks in systems.py
  → dungeonblocks.export(...)     # bases/dungeonblocks.py
      → make_base(...)
      → export_color_stl(...)
```

`DungeonBlocks.export()` in `systems.py` does nothing except construct a `BaseConfig` and delegate to `dungeonblocks.export()`. The `BaseConfig` it constructs is immediately discarded after its two fields are used. The `bases/` modules expose both a `make_base()` and an `export()` function — the `export()` in `bases/` is called only from `systems.py`, making it an internal detail that could live there.

The two-layer split (system class + base module) was introduced to keep the base geometry separate from the dispatch logic, which is correct intent. But the `export()` function in each base module is unnecessary: `systems.py` could call `make_base()` directly and handle the `export_color_stl()` tail itself, eliminating one layer of indirection.

As noted in the DRY review (Finding 2), the `export()` tails are identical between `dungeonblocks.py` and `openlock.py`. A shared helper `_export_with_base(base_mesh, colored, path)` would address both the duplication and the layering question: `systems.py` constructs the base, calls the helper.

---

## Tile Spec Language Assessment

The tile spec language is one of the strongest parts of the codebase. The three-tier `Region` / `Boundary` / `Layer` composition is clean. `FloodFill`, `Edge`, `Anchor` are well-named. `D[min:max].power(n)` is elegant. The `repeat_sizes` helper is appropriately simple.

The main friction points for tile authors:

**Ordering is load-bearing but invisible.** The orchestrator runs `Rocks` before `Grass` within a `Scatter` (so grass steers around rocks), and runs regions before boundaries. This is documented in `CLAUDE.md` but not in `spec.py` or any user-visible docstring. A new tile author who swaps the `Scatter` argument order discovers the behavioral difference only by looking at output.

**`SpeciesConfig` is passed twice.** In nearly every tile file, the same `_species` object is passed to both `GrassCarpet(species=...)` and `Grass(species=...)`. This is correct (it keeps blade geometry aligned), but it is subtle. The relationship between the two layers isn't expressed in the type system — there is nothing stopping a tile author from passing different species objects and getting inconsistent geometry.

**`FlatHeight` appears in tile files only for water tiles.** Most tile authors never see it; water tile authors use `terrain=FlatHeight(3.0)` while others use `height_mm=3.0` shortcut. Two API paths for the same thing create a documentation burden.

**The `boundary_ids` parameter to `reporter.tile_begin()` is sent but the reporter does nothing with it.** It is collected from `tile.boundaries` and passed through, but `TileReporter.tile_begin()` ignores it (the method signature accepts it but the base implementation discards it; `RichReporter` and `TextReporter` print it, but it is not used for anything structural). Minor noise.

The tile spec language would benefit from one explicit improvement: a module-level note in `spec.py` (or inline in `Scatter`) that Scatter argument order is semantically meaningful, not arbitrary.

---

## The System Parallelism (DungeonBlocks vs OpenLOCK)

The two systems are genuinely parallel and handled cleanly. The `systems.py` abstraction (`surface_for()` + `export()`) is correct. The cache in `build_tile_from_spec()` that shares the mesh build when both systems produce the same grid dimensions is appropriate optimization.

One small asymmetry: `DungeonBlocks.export()` passes `terrain_z` to its base module; `openlock.export()` ignores it (`del base_cfg, terrain_z` at line 172). This is because OpenLOCK peg height selection isn't implemented (the peg concept doesn't exist in the T-slot standard). The `del` suppression is fine but the parameter should arguably not appear in the function signature if it isn't used.

---

## What Is Actually Good

**The scatter system.** `Rocks`, `Grass`, `Tree`, `Flowers` all implement the same duck-typed protocol (`scatter(scene, *, placement_mask, layer_idx)` + `footprint_mm()`). `ScatterLayer` runs them in order with no special cases. Adding a new scatter thing requires implementing two methods and registering in `__init__.py`. The voronoi grouping is shared. The position seeding is shared. This is textbook good design.

**The `D[...]` distribution DSL.** Concise, composable, deterministic. `D[0.8:2.2].power(1.5)` expresses intent clearly. The `sample()` free function fast-paths plain numbers. Used consistently everywhere a parameter might vary.

**Deterministic seeding via `derive_seed`.** `blake2s(master, label, layer_idx)` gives every sub-process a unique, reproducible seed with no ad-hoc XOR constants. Clean.

**`TileScene` as the one accumulator.** Three mutable arrays (`terrain_z`, `terrain_support_z`, `obstacle_mask`) threaded through the pipeline is the right level of coupling. Layers read what they need; they don't need to know about each other's internal state.

**The `load_tile()` package trick.** Loading `.tile.py` files as real Python modules with importlib so sibling imports work and debuggers see them is a non-obvious but correct solution to what would otherwise be a messy exec()-based loader.

**`cloud_skeleton.py` / `cloud_mesh.py` SCA correctness.** The invariants (attractors are always leaf nodes, branches terminate exactly at attractor positions, radii are derived bottom-up via pipe model) are the right constraints for producing printable trees. The two-pass architecture (skeleton first, mesh second) is clean.

**`TreeEnvelope` encapsulation.** The axisymmetric crown math (`radius_at_t`, `radius_at_z`, `outward_normal_at`) is cleanly encapsulated. The envelope is passed to skeleton growth and has no mesh dependencies.

**The reporter pattern.** `TileReporter` (base no-op), `SilentReporter`, `TextReporter`, `RichReporter` — correct layering. The `make_reporter()` factory picks the right one. Rich is optional (graceful degradation).

**The tile spec format itself.** `.tile.py` files are real Python. They are short (20–80 lines), readable by designers who aren't deep Python programmers, and the language *is* the implementation. No DSL parsing, no YAML, no string-based layer types.

---

## Bigger Picture

**Module boundaries are mostly logical, not historical.** The five major zones (`core/`, `grass/`, `scatter/`, `layers/`, `trees/`) each have a coherent concern. The one exception is `core/color.py` which has grown out of its initial scope (Finding 3).

**The codebase is not over-abstracted.** Many earlier designs would have added a `Layer` base class, a `ScatterThing` base class, a `TerrainGenerator` base class. This codebase uses duck typing throughout, keeping the number of concepts small.

**The configuration story is the weakest part.** Three config classes (`SpeciesConfig`, `SoilConfig`, `RocksConfig`) use an anti-pattern (Finding 4). Two config objects (`SceneConfig`, ghost `GrassConfig.species`) are either dead or misshapen. This zone has the most accumulated historical weight and would benefit from a single focused cleanup pass.

**Performance bottlenecks are known and located correctly.** The hot paths (terrain mesh, boolean unions, grass growth) are in C-backed libraries. The Python loops that remain (`_stamp_tree`, rock rasterization) are in cold paths. The codebase is not chasing false performance at the cost of clarity.

**The "foliage.py" file does not exist.** `trees/` has `cloud_skeleton.py`, `cloud_mesh.py`, `bark.py`, `leaf.py`, `envelope.py`, `layer.py`. The CLAUDE.md mentions `trees/foliage.py` with `build_foliage_clumps()` — this file was apparently absorbed into `cloud_mesh.py` where foliage clump building lives now (`_build_foliage_clump_mesh`). The CLAUDE.md is stale on this point.

---

## Minimal PR Plan

These are reversible, independent. Do in order.

**Commit 1 — Delete dead abstractions (no behavior change)**
- Delete `core/terrain.py`
- Delete `SceneConfig` from `core/config.py`
- Delete `TileScene.parts` field from `core/tile.py`
- Change the one `load_spec` call in `terrains/tile.py:872` to `load_tile`, remove alias from `spec.py`

**Commit 2 — Flatten single-implementation wrappers**
- Replace `FloppyGrassLayer` class with a `build_grass()` free function
- Flatten `TreeShape` into `Tree` (8 attributes become direct `self.*`)
- Remove `GROWERS` registry; inline `FlatGrassGrower` reference in `grow.py`
- Remove `grower: str` field from `SpeciesConfig`

**Commit 3 — Fix config anti-pattern**
- Convert `SpeciesConfig`, `SoilConfig`, `RocksConfig` to use `__post_init__` for validation
- Drop the `values` dict + setattr loop in all three
- Collapse `GrassConfig.species: list[SpeciesConfig]` to `species: SpeciesConfig`

**Commit 4 — Split core/color.py**
- Move `build_scene`, `_pack_plates`, `export_3mf_colored` to `core/export_3mf.py`
- Update imports in `terrains/tile.py` and anywhere else that uses these

---

## Architecture Verdict: DRIFTING → CLEAN (post second-pass fixes 2026-06-18)

The core pipeline is CLEAN. The drift is concentrated in config classes, dead abstractions, and one module whose scope has grown beyond its name. None of these are architectural failures — they are accumulated drag from features that were planned but not built, migrations that were half-completed, and one module that needed to absorb scope without being split. A focused cleanup pass on Findings 1–4 would return the codebase to CLEAN.
