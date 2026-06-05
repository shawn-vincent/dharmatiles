---
name: terminology-tile-square-cell
description: Canonical three-tier naming for the spatial hierarchy in dharmatiles
metadata:
  type: project
---

# Three-tier spatial terminology

Established 2026-06-02, commit 3b4da64.

| Level | Name | Size | Code symbol |
|---|---|---|---|
| Full printed output | **tile** | cols × rows × 35 mm | `TileScene`, `build_tile()`, `stl/tile.stl` |
| One 35 mm DungeonBlocks unit | **square** | 35 × 35 mm | `cols`, `rows`, `cells_per_square`, `stones_per_square`, `groups_per_square` |
| One heightmap subdivision | **cell** | 35 mm / cells_per_square | `cell_w`, `cell_h`, `grid_w`, `grid_h` |

**Why:** "tile" was used for both the full output AND the 35 mm unit — a collision.
DungeonBlocks community and OpenLOCK both call the 35 mm grid unit a "square".

**How to apply:** Never call a 35 mm unit a "tile" or "tile unit" in code, comments, or CLI flags.
A 3×3 tile has 9 squares. Density params end in `_per_square`.

**Orin review findings (same session)** — open items to resolve:

1. `detail_mult=2` in SoilConfig doubles CPU but hires_bump is discarded in `terrains/tile.py:59`. Wire or remove. [[soil-detail-mult]]
2. `build_sub_hull_mesh` imported but never called — delete from `grass.py`, `mesh.py`, `core/__init__.py`
3. `core/collision.py` entire module is dead — delete + remove re-exports from `core/__init__.py`; `SolverConfig.strict_mode` / `strict_base_t` also stranded
4. `TerrainGrid` allocated at heightmap resolution (65k Python objects) — wrong abstraction, fix before `from_terrain_grid` path is wired
5. `GrassLayer` has 5 dead class-level defaults overwritten in `__init__` — remove them
6. `GrassSeed.base_x`, `base_y`, `direction` are write-only after construction — dict duplication with `live` entries
7. `cell_mm_h` in `soil.py:46` misnaming — rename to `cell_mm`
8. `CELL_SIZE_MM` legacy constant — delete from `config.py` and `core/__init__.py`
9. `cell_h` is always equal to `cell_w` — redundant property
10. `_make_compat_scene` in `tile.py` raises unconditionally — delete
11. `TerrainGrid.fill()` uses `cols.start or 0` wrong idiom — fix before wiring
