# Tile Naming Convention

## Terrain Type Hierarchy

Types are always listed low → high elevation:

    water < soil < grass < floors < walls/cliffs

## File Naming Pattern

```
[lower-type]+[higher-type].tile
[lower-type]-[shape]+[higher-type].tile
[lower-type]+[higher-type]-[shape].tile
```

- **Pure tile** (single terrain): just the type name — `grass.tile`, `water.tile`
- **Mixed tile**: `[lower]+[higher]` — the `+` separates the two terrain regions
- **Shape suffix** (joined with `-`) describes the topology of the terrain it is attached to
- Shape suffix is **omitted** when the higher type occupies one full side (half the tile) — that is the implicit default

The `+` separates terrain groups.  `water-corridor+grass` means a water
corridor through grass; `soil+grass-corner` means a grass corner on soil.

For the full current convention, see `docs/tile-design-guide.md`.

## Shape Vocabulary

Shape names are borrowed from wall/room topology:

| Suffix | Higher type occupies… |
|---|---|
| *(none — implicit)* | One full side (half the tile) |
| `-corner` | One corner |
| `-angle` | Two adjacent sides (L-shape) |
| `-corridor` | Two opposing sides |
| `-u` | Three sides |
| `-corridor-turn` | Corridor connecting two adjacent sides |
| `-corridor-t` | Corridor connecting three sides |
| `-corridor-x` | Corridor connecting all four sides |
| `-corridor-open` | Corridor width opening into a midpoint-width transition |

## Current Tiles

| File | Description |
|---|---|
| `grass.tile` | All grass |
| `water.tile` | All water |
| `soil+grass.tile` | Half soil, half grass (grass along one side) |
| `water+grass.tile` | Half water, half grass (grass along one side) |
| `soil+grass-corner.tile` | Soil base, grass accent in one corner |
