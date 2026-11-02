# Design Session: Architecture Direction
**Date:** 2026-06-18  
**Participants:** svincent + Claude Sonnet 4.6 (Orin agents × 3)  
**Scope:** Full codebase elegance review (three passes) + design discussion on scatter
system, cross-object interaction, and 3D mesh queries as a generation primitive.

---

## Part 1 — The Orin Reviews

Three parallel Orin reviews were commissioned today and written to `docs/meta/history/`:

- `2026-06-18-orin-elegance-review.md` — End-to-end design review, 16 findings
- `2026-06-18-orin-strategic-review.md` — High-level architectural shape
- `2026-06-18-orin-technical-review.md` — Deep implementation / NumPy / algorithm review

Key findings are summarised below. Full details in those files.

### Highest-priority cleanup (from the elegance review)

These are purely subtractive — no behavior change, no STL regeneration needed.

1. **`core/terrain.py` is dead code.** `TerrainType` and its helpers are referenced
   nowhere in the live pipeline. Planning artifact from a superseded design. Delete.

2. **`SceneConfig` is a ghost.** Never constructed in the live pipeline. Superseded by the
   `Tile` dataclass. Delete from `core/config.py`.

3. **`core/color.py` is three modules.** Material palette, 3MF XML generation, and a
   Bambu X1C plate-packing algorithm all live in a file named "color". Split into
   `core/color.py` (palette only) and `core/export_3mf.py`.

4. **Frozen dataclass + custom `__init__` + values-dict anti-pattern.** `SpeciesConfig`,
   `SoilConfig`, `RocksConfig` all declare defaults twice (annotation and `__init__`
   parameter), then loop through a dict to set them. Replace with `__post_init__`
   validation on plain `@dataclass`. The dict loop is pure ceremony.

5. **Three single-use wrappers with no behavior.** `FloppyGrassLayer` (wraps two
   function calls), `TreeShape` (groups 8 Tree fields, immediately deconstructed),
   and `GROWERS` registry (`{"floppy": FlatGrassGrower}` — one entry, stringly-typed).
   All three can be deleted without changing any observable behavior.

6. **`GrassConfig.species` is always a length-1 list.** Change to a single
   `SpeciesConfig`. The multi-species future it anticipated has not arrived.

7. **`TileScene.parts` is never populated.** Layers return meshes via `apply()`; nothing
   appends to `scene.parts`. Remove the field.

8. **`load_spec` backward-compat alias.** One remaining call site in `terrains/tile.py`.
   Change to `load_tile`, remove the alias.

9. **`_LAYER_TERRAIN_MATERIAL` uses type-name strings for dispatch.** Rename-fragile.
   Replace with a `terrain_material: ClassVar[Material]` attribute on layers that want
   to declare one. Structural typing beats a string dict.

10. **`CLAUDE.md` mentions `trees/foliage.py` which does not exist.** The functionality
    was absorbed into `cloud_mesh.py`. Update the docs.

### Highest-priority technical findings

1. **Rock rasterisation has an O(n) Python loop inside the "vectorised" kernel**
   (`layers/rocks.py:274–309`). Each rock gets its own `np.meshgrid` call. Pre-compute
   a shared offset grid, broadcast over all N rocks, scatter-accumulate. Eliminates N
   separate meshgrid allocations.

2. **`_make_ring` bark loops** (`trees/cloud_mesh.py:1481–1513`). Called ~800× per tree
   with bark enabled. Two `[f(t) for t in theta]` list comprehensions over pure
   arithmetic. Vectorise `_bark_cut` and `_bark_surface_noise` to operate over the full
   angle array at once.

3. **`_lift_path_points` scalar grid sampling** (`grass/mesh.py:66–83`). `_sample_grid`
   already handles arrays. Extract x/y/z arrays and sample in one call. 2000 blades ×
   10 points = 20,000 scalar calls → 2,000 batched calls.

4. **Skeleton nodes as `list[np.ndarray]`** (`trees/cloud_skeleton.py`). Pre-allocate
   `np.empty((max_nodes, 3))`. Eliminates per-node copy-and-append and the
   `np.asarray(nodes)` conversion in `_simplify_skeleton`.

5. **`_build_closed_edge_solid` takes 18 arguments, 7 always bark-related.** Introduce
   `_BarkEdgeState` dataclass to bundle the bark arguments.

### Strategic finding: ordering is load-bearing and invisible

`Scatter(Rocks(...), Grass(...))` looks like a set — "scatter these things together" —
but it is a sequence. Grass steers around rocks only because `Rocks` appears first in the
argument list. The causal constraint lives in a Python list position, not in the type
system or the API shape.

Same problem at the `tile.areas` level: region ordering controls which rocks get stamped
before which region's grass grows. Documented in `CLAUDE.md` but invisible at the call
site.

This is the single most important API issue for the tile spec language as the tile library
grows.

---

## Part 2 — Design Discussion: The Scatter System

### Should scatter things just be layers?

Yes. `ScatterLayer.apply()` does exactly what `Region.layers` already does: runs things
in order, passes the same `placement_mask`, gives each thing a `layer_idx` for seed
derivation, collects meshes. `ScatterLayer` adds *grouping* — which is organizational, not
mechanical.

The conversion: `Rocks`, `Grass`, `Tree` implement `apply(scene, *, placement_mask)` and
appear directly in `Region.layers` alongside `SoilCarpet` and `GrassCarpet`. The only
real cost is a seed change — existing tiles regenerate with different rock/grass positions
(same quality, different arrangement).

This makes the ordering constraint *more honest*: `Region(layers=[GrassCarpet(), Rocks(),
Grass()])` is visually a sequence. `Scatter(Rocks(), Grass())` looks like a set.

### The primitive + distributor idea

The user proposed: separate "one rock / one grass blade / one tree" (the primitive) from
"grouping, distribution, alignment" (the wrapper). This is the right instinct, but the
three things are not equivalent:

**Rocks — independent per instance.** One rock doesn't know about the others during
construction. The current vectorised batch build is an optimization, not a design
requirement. A `Rock` primitive (one rock's geometry) calling a general `Scatter` wrapper
N times would be architecturally cleaner at some performance cost.

**Tree — independent per instance (currently).** Each tree is a self-contained call to
`grow_cloud_skeleton` + `build_cloud_tree_mesh`. From the scatter level, tree is
`f(position) → mesh`. A clean primitive.

**Grass — NOT independent per instance.** This is the key distinction. Blades are sorted
by upstream distance and grown sequentially. Each blade stamps a shared `occ_z` occupancy
grid that subsequent blades read to route around. Blade N's path depends on blades 1–N-1.
You cannot build one grass blade without the state left by prior blades.

**Conclusion:** Grass is a *field simulation*, not a collection of independent instances.
It's structurally closer to `SoilCarpetLayer` (covers an area via a process) than to
`Rocks` (places independent items). Calling it a scatter thing is the category error.
The right model: `Rock` and `Tree` are scatter primitives; `GrassField` is a coverage
layer that happens to use scatter positions as seeds.

---

## Part 3 — Design Discussion: Cross-Object Interaction

### Tree-tree interaction (future)

The user proposed: trees should not grow through each other. Specifically:
- Trunks should not intersect other trunks
- Canopy point clouds should interfere (branches avoid each other's attractor regions)

This collapses the "Tree is independent per instance" argument. Tree 2's skeleton growth
would need to query Tree 1's placed geometry in 3D — steering away from existing trunk
segments. This requires `TileScene` to carry a record of placed 3D geometry that skeleton
growth can query during BFS.

This makes Tree sequentially dependent across instances, like Grass. The difference is
scope: Grass blades share a 2D occupancy grid; Trees would share a 3D object list.

### Grass growing alongside trees (future)

The user proposed: grass blades near a tree trunk should grow taller — tufting up
alongside the trunk rather than being cut off flat.

This is achievable elegantly without 3D mesh queries by changing how `_stamp_tree` writes
`terrain_support_z`. Currently it writes a flat ceiling at `tree_height`. Instead, write a
**radial falloff from the trunk edge**:

```python
dist_from_trunk = max(0.0, sqrt((xx - x)**2 + (yy - y)**2) - trunk_radius_mm)
allowed_height  = tree_height * exp(-dist_from_trunk / falloff_mm)
scene.terrain_support_z[j, i] = max(current, terrain_z + allowed_height)
```

Grass blades reading `vegetation_support_z` (which syncs from `terrain_support_z`)
naturally grow taller near the trunk base. No new mechanism needed in the grass system.
The tree stamp becomes a 3D-aware influence field rather than a flat 2D ceiling.

---

## Part 4 — Design Discussion: 3D Mesh Queries as a Generation Primitive

### The core idea

Currently the pipeline builds objects analytically using known geometry formulas. The
user proposed: build things, add them to the scene as meshes, and query the accumulated
mesh when building subsequent things.

`trimesh` already provides the full toolkit:

```python
# Sample N points uniformly on any mesh surface, get normals for free
points, face_ids = trimesh.sample.sample_surface(mesh, count=200)
normals = mesh.face_normals[face_ids]

# Find closest point on placed geometry to any query position
closest, distance, face_id = trimesh.proximity.closest_point(placed_mesh, query_points)

# Raycast — find where a ray hits placed geometry
locations, ray_ids, face_ids = placed_mesh.ray.intersects_location(origins, directions)
```

### Two distinct use cases

**Use case A: Surface decoration — sample the mesh to place things on it.**

Leaves on foliage clusters, moss on rocks, bark texture, lichen. Currently
`_build_foliage_clump_mesh` does hundreds of lines of analytical math to answer "where is
the foliage surface at this point, and which way does it face?" — math that only works
because the geometry is known symbolically.

`trimesh.sample.sample_surface()` answers that question for *any* mesh, with correct
outward normals, in two lines. The complex icosphere-deformation surface inversion in the
leaf placement code goes away. You build the foliage clump, sample its surface, orient
leaves to the normals. This is simpler and more general — it would work for any future
foliage shape without new math.

**Use case B: Spatial queries during generation of subsequent things.**

Grass avoiding trees, trunks avoiding each other, vines climbing. The placed mesh becomes
queryable by anything that comes after it in the pipeline:

- *Grass tufts near trunk:* `closest_point(tree_mesh, blade_seed_positions)` →
  distance to trunk surface → scale allowed blade height. One vectorised call over all
  blade seeds. More correct than the `terrain_support_z` falloff approximation above
  because it uses the actual trunk geometry.

- *Trunk-trunk avoidance:* During BFS skeleton growth, query `closest_point(placed_trunks,
  next_pos)` at each step. Steer away if distance < min_gap_mm.

- *Vine on tree:* Raycast along the trunk surface to find attachment points, then
  path-follow on the mesh surface using sequential raycasts. Not possible with the current
  2D system.

### Architectural implication: `TileScene.placed_solids`

`TileScene` needs one new field:

```python
placed_solids: trimesh.Trimesh | None = None
```

Every time a solid object is placed (rock, tree, wall), its mesh is concatenated into
`placed_solids`. Subsequent layers and scatter things query it. The trimesh BVH is built
lazily on first query and cached.

The existing 2D fields (`obstacle_mask`, `terrain_support_z`) remain as fast-paths for
the common case (horizontal placement on flat terrain). They can be derived from
`placed_solids` rather than being maintained separately, but there is no urgency to change
that now.

### The water distinction

Water is a volume you pass through, not a solid you avoid. The material tagging system
already handles this: `placed_solids` contains only meshes tagged with solid materials
(ROCK, WOOD, BASE). A separate `placed_volumes` field (or material-filtered queries on the
full mesh) handles non-solid interactions. A vine grows on WOOD and avoids ROCK but passes
through WATER. The query just filters by material.

### Performance

For offline generation this is not a bottleneck. A BVH query against a 50,000-triangle
tree mesh is microseconds. With 2,000 grass blade seeds querying a tree mesh:
~milliseconds total. Tile generation already takes seconds; this is not the limiting
factor.

---

## Recommendations Going Forward

### Immediate (cleanup, no behavior change)

Taken directly from the elegance review. In priority order:

1. Delete `core/terrain.py`, `SceneConfig`, `TileScene.parts`, `load_spec` alias
2. Replace `FloppyGrassLayer`, `TreeShape`, `GROWERS` registry with direct calls / flat attrs
3. Fix `SpeciesConfig` / `SoilConfig` / `RocksConfig` to use `__post_init__`
4. Collapse `GrassConfig.species: list[SpeciesConfig]` to `species: SpeciesConfig`
5. Split `core/color.py` into `core/color.py` + `core/export_3mf.py`
6. Replace `_LAYER_TERRAIN_MATERIAL` string dict with `ClassVar[Material]` on layers
7. Update `CLAUDE.md`: remove `trees/foliage.py` reference, add ordering-constraint note
   to `Scatter` docs

### Near-term (design clarification)

8. **Make scatter things into direct layers.** Remove `ScatterLayer` / `Scatter` wrapper.
   `Rocks`, `Grass`, `Tree` implement `apply()` and sit directly in `Region.layers`. The
   ordering constraint becomes as visible as soil/carpet layer ordering.

9. **Acknowledge the Grass/Rock/Tree distinction in naming and docs.** Grass is a field
   simulation; Rock and Tree are instance placements. This distinction should be visible
   in the module structure and docstrings, even if the `apply()` interface is shared.

10. **Change `_stamp_tree` to write a radial falloff** into `terrain_support_z` rather
    than a flat ceiling. Enables grass tufting near trunks with zero grass-side changes.
    Low cost, high visual payoff.

### Future (architectural direction)

11. **Add `TileScene.placed_solids: trimesh.Trimesh | None`.** Populated by each scatter
    thing after placing its geometry. Initially used for trunk-trunk avoidance (trees query
    it during skeleton growth) and for surface decoration (moss, lichen, bark texture
    via `trimesh.sample.sample_surface()`).

12. **Refactor leaf placement in `_build_foliage_clump_mesh` to use mesh surface
    sampling.** Build the foliage clump mesh, sample its surface, orient leaves to face
    normals. Eliminates the analytical surface-inversion math. Unlocks leaves on any
    future foliage shape automatically.

13. **Tree-tree point cloud interference.** When placing trees, suppress or deflect
    attractors that fall inside an existing tree's canopy envelope. Requires `placed_solids`
    or a list of placed `TreeEnvelope` objects on `TileScene`. Produces architecturally
    natural-looking multi-tree clusters where canopies don't interpenetrate.

14. **Trunk-trunk avoidance during skeleton growth.** Query `placed_solids` during BFS to
    steer new trunk segments away from existing trunks. This makes Tree fully sequential
    and stateful across instances — the same model as Grass, just with 3D queries instead
    of a 2D occupancy grid.

15. **Vine / surface-following geometry.** Enabled by `placed_solids` + mesh raycasting.
    A vine is a path that follows the nearest solid surface, growing upward. Not possible
    with the current 2D system; straightforward with `trimesh.ray`.

---

## The Central Architectural Shift

The current system builds objects analytically using known geometry formulas, then
summarises their presence into a 2D grid (`obstacle_mask`, `terrain_support_z`). This is
efficient and correct for "features sitting on flat horizontal terrain."

The direction this session points toward: **build things, add them to the scene as meshes,
query the accumulated mesh when building subsequent things.** The 2D grids become
fast-path approximations for the common case; the 3D mesh becomes the authoritative scene
representation for anything that needs real spatial math.

The heightmap (`terrain_z`) stays as ground truth for the terrain floor — it is efficient
and the flat-ground assumption is correct for the main tile surface. Above the terrain, the
accumulated `placed_solids` mesh becomes the scene. That is the right division.

This shift doesn't require abandoning the existing pipeline. It's an additive change:
populate `placed_solids` as things are placed, add mesh-query call sites where 3D
awareness is needed. The first concrete payoff is leaf placement simplification. The second
is grass tufting near trunks. The third is tree-tree interaction. Each one is an
independent, incremental step.
