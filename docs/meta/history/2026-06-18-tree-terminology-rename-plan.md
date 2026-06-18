# Tree Terminology Rename Plan
**Date:** 2026-06-18  
**Status:** APPROVED — ready to execute  
**Scope:** All files under `src/dharmatiles/trees/`, `src/tiles/`, `CLAUDE.md`, and any
other files that import from or document the tree subsystem.

---

## Motivation

The tree subsystem has accumulated four overlapping naming problems:

1. **The "cloud" prefix is historical residue.** `cloud_skeleton.py`, `cloud_mesh.py`,
   `grow_cloud_skeleton()`, `build_cloud_tree_mesh()` all carry "cloud" from the old
   `CloudTree` class name (renamed in git). The prefix is now meaningless noise.

2. **"crown" and "canopy" are used interchangeably.** `TreeEnvelope` uses `crown_*`
   fields throughout; documentation uses "canopy". They mean the same thing. Pick one:
   **canopy**.

3. **"clump" and "foliage clump" and "leaf clump" describe the same thing
   inconsistently.** The blobby shapes at terminal branch tips are called `leaf_clumps`
   in `Tree` parameters, `foliage_clump` in internal function names, and `foliage clump`
   in comments. The canonical name going forward: **foliage cluster**.

4. **Graph "leaf node" collides with plant "leaf".** `is_leaf[i]` means "skeleton node
   with no children" (graph theory). `leaf_enable`, `leaf_base_count` etc. mean
   individual plant leaf blades. These must be distinguishable at a glance. Graph-theory
   leaf nodes become **terminal nodes**.

---

## Canonical Four-Part Vocabulary

| Term | Meaning |
|---|---|
| **branches** | The structural wood — trunk + all branching segments. The skeleton and its tube mesh. |
| **canopy** | The overall shape of the tree's leafy volume. Defined by `CanopyEnvelope`. Attractors are sampled on its surface. |
| **foliage clusters** | Blobby, rounded shapes built at the tips of terminal branches. Constructed as a separate displaced-icosphere mesh. |
| **leaves** | Individual leaf blades attached to foliage clusters. |

---

## Complete Rename Table

### Files

| Current path | New path |
|---|---|
| `src/dharmatiles/trees/cloud_skeleton.py` | `src/dharmatiles/trees/skeleton.py` |
| `src/dharmatiles/trees/cloud_mesh.py` | `src/dharmatiles/trees/mesh.py` |
| `src/tiles/ground/2x2-grass-cloud-tree.tile.py` | `src/tiles/ground/2x2-grass-tree.tile.py` |

Note: renaming the tile file changes the default STL output paths:
- `stl/dungeonblocks/2x2-grass-cloud-tree-db.stl` → `stl/dungeonblocks/2x2-grass-tree-db.stl`
- `stl/openlock/2x2-grass-cloud-tree-ol.stl` → `stl/openlock/2x2-grass-tree-ol.stl`

Old STL files should be deleted after regeneration.

---

### Public functions (imported by `layer.py` and potentially callers)

| Current | New |
|---|---|
| `grow_cloud_skeleton()` | `grow_skeleton()` |
| `build_cloud_tree_mesh()` | `build_tree_mesh()` |

---

### Classes

| Current | New | File |
|---|---|---|
| `TreeEnvelope` | `CanopyEnvelope` | `trees/envelope.py` (file stays `envelope.py`) |

---

### `Tree` public parameters (`trees/layer.py`)

These are user-facing — they appear in `.tile.py` spec files.

| Current | New | Notes |
|---|---|---|
| `crown_radius_mm` | `canopy_radius_mm` | Also on `TreeShape` |
| `crown_base_radius_mm` | `canopy_base_radius_mm` | Also on `TreeShape` |
| `leaf_clumps` | `foliage_clusters` | Bool: enable/disable foliage clusters |
| `leaf_clump_radius_mm` | `foliage_cluster_radius_mm` | Tip radius of each foliage cluster |
| `leaf_clump_length_mm` | `foliage_cluster_length_mm` | Max length of cluster zone on terminal branch |
| `leaf_enable` | `leaves` | Bool: enable/disable individual leaf blades |
| `n_attraction` | `n_attractors` | Count of attractor points |

All other `leaf_*` parameters (`leaf_base_count`, `leaf_length_mm`, `leaf_width_mm`,
`leaf_thickness_mm`, `leaf_fold_angle_deg`, `leaf_keel_tip_angle_deg`,
`leaf_spacing_factor`, `leaf_cap_count`, `leaf_angle_jitter_deg`, `leaf_pos_jitter`)
refer unambiguously to individual leaf blades — **keep as-is**.

---

### `CanopyEnvelope` fields (`trees/envelope.py`)

| Current | New |
|---|---|
| `crown_radius_mm` | `canopy_radius_mm` |
| `crown_base_radius_mm` | `canopy_base_radius_mm` |
| `crown_height` (property) | `canopy_height` |
| `crown_base_z` (property) | `canopy_base_z` |
| `crown_top_z` (property) | `canopy_top_z` |

---

### Internal names in `trees/mesh.py` (was `cloud_mesh.py`)

| Current | New |
|---|---|
| `foliage_radius_mm` (param to `build_tree_mesh`) | `foliage_cluster_radius_mm` |
| `leaf_clump_length_mm` (param to `build_tree_mesh`) | `foliage_cluster_length_mm` |
| `render_foliage` (local bool) | `render_foliage_clusters` |
| `is_foliage_leaf` (local bool per edge) | `has_foliage_cluster` |
| `clump_len` (local float) | `cluster_len` |
| `is_leaf` (bool array, graph-theory leaf) | `is_terminal` |
| `_build_foliage_clump_mesh()` | `_build_foliage_cluster_mesh()` |
| `_foliage_bark_endpoint_t_by_id()` | `_foliage_cluster_bark_endpoint_t_by_id()` |
| `_foliage_bark_endpoint_maps()` | `_foliage_cluster_bark_endpoint_maps()` |
| `_N_FOLIAGE_SIDES` | `_N_CLUSTER_SIDES` |
| `_N_FOLIAGE_DOME_LATS` | `_N_CLUSTER_DOME_LATS` |

---

### Internal names in `trees/skeleton.py` (was `cloud_skeleton.py`)

| Current | New |
|---|---|
| `_sample_cloud()` | `_sample_canopy()` |
| `crown_radius_mm` references via `env` | follow `CanopyEnvelope` rename |
| `crown_height` references via `env` | follow `CanopyEnvelope` rename |
| Docstring: *"Every attractor is a LEAF node"* | *"Every attractor is a TERMINAL node"* |
| Docstring: *"leaf branches"*, *"leaf mode"* | *"terminal branches"*, *"terminal mode"* |

---

### Seed label strings (deterministic — changing these changes RNG output)

These are internal strings passed to `derive_seed()`. Changing them will produce
different random seeds, meaning regenerated STLs will have different rock/grass/tree
positions. This is acceptable since the visual quality is unchanged.

| Current | New |
|---|---|
| `"cloud-trees-scatter"` (in `layer.py`) | `"trees-scatter"` |
| `"foliage-bark-end"` (hash key in `mesh.py`) | `"foliage-cluster-bark-end"` |

---

### `TreeShape` dataclass (`trees/layer.py`)

Per the Orin elegance review, `TreeShape` is a candidate for elimination (it wraps 8
fields with no behavior). This rename plan treats it as still present but updated.
The elimination of `TreeShape` should happen as a separate commit after this rename.

| Current | New |
|---|---|
| `TreeShape.crown_radius_mm` | `TreeShape.canopy_radius_mm` |
| `TreeShape.crown_base_radius_mm` | `TreeShape.canopy_base_radius_mm` |

---

### `trees/__init__.py`

Update all re-exports to use new function/class names.

---

## Tile spec files affected

| File | Changes |
|---|---|
| `src/tiles/ground/1x1-grass-tree.tile.py` | `crown_radius_mm` → `canopy_radius_mm`, `crown_base_radius_mm` → `canopy_base_radius_mm`, `leaf_clumps` → `foliage_clusters` |
| `src/tiles/ground/2x2-grass-cloud-tree.tile.py` | All of the above + file rename → `2x2-grass-tree.tile.py`, `debug_attractors` and `leaf_clumps` → `foliage_clusters` |

---

## CLAUDE.md updates

- `trees/cloud_skeleton.py` → `trees/skeleton.py` in module table
- `trees/cloud_mesh.py` → `trees/mesh.py` in module table
- Remove stale `trees/foliage.py` row (file does not exist; functionality is in `mesh.py`)
- `grow_cloud_skeleton()` → `grow_skeleton()`
- `build_cloud_tree_mesh()` → `build_tree_mesh()`
- `TreeEnvelope` → `CanopyEnvelope`
- `crown_radius_mm`, `crown_base_radius_mm` → `canopy_*` throughout Tree section
- `leaf_clumps` → `foliage_clusters` in parameter table
- `leaf_clump_radius_mm` → `foliage_cluster_radius_mm` in parameter table
- `leaf_clump_length_mm` → `foliage_cluster_length_mm` in parameter table
- `leaf_enable` → `leaves` in parameter table
- `n_attraction` → `n_attractors` in parameter table
- Update invariants: *"leaf node"* → *"terminal node"* in SCA description
- Update `foliage_bulge_mm` row: already says "canopy surface" — verify consistent

---

## What does NOT change

- `attractor`, `n_attractors` (after rename) — correct technical term, keep
- `bark`, `BarkConfig`, all `bark_*` names — separate concern, not part of this rename
- `trunk_height_mm` — correct and unambiguous, keep
- `Tree` class name — keep
- `trees/envelope.py` filename — keep (just rename the class inside)
- `trees/bark.py`, `trees/leaf.py` filenames — keep
- `trees/layer.py` filename — keep
- All `leaf_base_count`, `leaf_length_mm`, `leaf_width_mm`, `leaf_thickness_mm`,
  `leaf_fold_angle_deg`, `leaf_keel_tip_angle_deg`, `leaf_spacing_factor`,
  `leaf_cap_count`, `leaf_angle_jitter_deg`, `leaf_pos_jitter` — unambiguously refer
  to individual leaf blades, keep as-is
- `debug_attractors` — keep

---

## Execution order

1. Rename files (`cloud_skeleton.py` → `skeleton.py`, `cloud_mesh.py` → `mesh.py`)
2. Update all imports in `trees/layer.py`, `trees/__init__.py`, and any other importers
3. Rename `CanopyEnvelope` and update all field names in `envelope.py`
4. Rename internal names in `skeleton.py`
5. Rename internal names and public params in `mesh.py`
6. Update `Tree.__init__` params and `TreeShape` fields in `layer.py`
7. Update tile spec files
8. Rename `2x2-grass-cloud-tree.tile.py` → `2x2-grass-tree.tile.py`
9. Update `CLAUDE.md`
10. Delete old STL files for the renamed tile; regenerate
11. Verify no remaining `cloud_`, `crown_`, `leaf_clump`, `leaf_enable`,
    `n_attraction`, `is_leaf`, `foliage_clump` in `trees/` or `tiles/`

---

## Verification grep after execution

```bash
# Should return zero results:
grep -rn "cloud_skeleton\|cloud_mesh\|grow_cloud\|build_cloud\|crown_radius\|crown_base\|crown_height\|crown_base_z\|crown_top_z\|leaf_clump\|leaf_enable\|n_attraction\b\|is_foliage_leaf\|render_foliage\b\|foliage_clump\|TreeEnvelope\|_sample_cloud\|is_leaf\b" \
  src/dharmatiles/trees/ src/tiles/ CLAUDE.md \
  --include="*.py" --include="*.md"
```
