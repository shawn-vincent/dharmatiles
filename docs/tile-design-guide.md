# Tile Design Guide

This guide records the conventions that keep independently generated tiles
compatible when placed next to each other.

## Filename Convention

Tile filenames describe terrain types from lower elevation to higher elevation:

```text
1x1-[lower-terrain]+[higher-terrain].tile.py
1x1-[lower-terrain]-[shape]+[higher-terrain].tile.py
1x1-[lower-terrain]+[higher-terrain]-[shape].tile.py
```

Attach a shape suffix to the terrain whose footprint has that shape.  For
example, a water corridor through grass is:

```text
1x1-water-corridor+grass.tile.py
```

not:

```text
1x1-water+grass-corridor.tile.py
```

Pure single-terrain tiles omit `+`: `1x1-grass.tile.py`,
`1x1-soil.tile.py`, `1x1-water.tile.py`.

Rotations are not separate specs.  Each shape has one canonical orientation;
rotate the printed tile when a different orientation is needed.

## Terrain Order

Use this terrain order in mixed filenames:

```text
water < soil < grass
```

Current mixed families:

| Family | Meaning |
|---|---|
| `soil+grass` | soil and grass at ground height |
| `water+soil` | water with soil banks |
| `water+grass` | water with grass, usually with a soil shoreline boundary |

When the lower terrain has the shape, put the suffix before `+`:
`water-corridor+grass`, `soil-u+grass`.

When the higher terrain has the shape, put the suffix after that terrain:
`soil+grass-corner`, `water+grass-angle`.

## Edge Coordinates

Boundary anchors use normalized edge coordinates.  `0.0` and `1.0` are corners;
`0.5` is the midpoint of an edge.

Use these standard intersections:

| Shape family | Edge points |
|---|---|
| straight half transition | `0.50` nominally; existing organic specs use near-midpoint `0.48` and `0.52` to avoid perfectly mirrored paths |
| corner | `0.50` on two adjacent sides |
| angle | `0.50` on two adjacent sides |
| corridor | `0.33` and `0.67` on two opposite sides |
| u | `0.33` and `0.67` on one side |
| corridor-turn | `0.33` and `0.67` on two adjacent sides |
| corridor-t | `0.33` and `0.67` on three sides |
| corridor-x | `0.33` and `0.67` on all four sides |
| corridor-open | `0.33` and `0.67` on face A; `0.50` on the two adjacent faces |

The exact anchor points must match even when the path is organic.  Organic
boundaries may wander inside the tile, but the endpoints are compatibility
points with neighboring tiles.

## Canonical Shape Orientation

The canonical orientation is only a spec-writing convention.  Printed tiles can
be rotated freely.

| Shape | Canonical openings |
|---|---|
| straight half transition | left/right boundary, lower terrain below or right depending on the existing family |
| corner | bottom-left corner |
| angle | bottom-left cutout, with the named shaped terrain occupying the L around it |
| corridor | left to right |
| u | right side, endpoints at `0.33` and `0.67` |
| corridor-turn | left to top |
| corridor-t | left, right, and top |
| corridor-x | left, right, top, and bottom |
| corridor-open | corridor enters from left and opens toward the right; boundaries connect left `0.33` to bottom `0.50`, and left `0.67` to top `0.50` |

## Shape Inventory

The current shape suffixes are:

| Suffix | Meaning |
|---|---|
| none | one broad side or half-tile transition |
| `corner` | named terrain occupies one corner |
| `angle` | named terrain occupies two adjacent sides |
| `corridor` | named terrain connects opposite sides at `0.33`/`0.67` |
| `u` | named terrain enters and exits the same side at `0.33`/`0.67` |
| `corridor-turn` | named terrain connects two adjacent sides at `0.33`/`0.67` |
| `corridor-t` | named terrain connects three sides at `0.33`/`0.67` |
| `corridor-x` | named terrain connects all four sides at `0.33`/`0.67` |
| `corridor-open` | named terrain transitions from corridor width to a midpoint-width opening |

## Boundary Guidelines

Use normal `Boundary` objects for all shapes.  Same-side and waypoint-constrained
boundaries are still ordinary boundaries.

Use `waypoints` when a boundary needs to route through the tile while preserving
specific edge endpoints.  A `straight` path follows the anchor/waypoint chain
exactly.  An `organic` path passes through each anchor and waypoint, then
wanders between them.

For water-to-land transitions, keep the shoreline boundary as a finite strip
with `width_mm` and soil/rock shoreline layers.  Existing corridor water specs
use `width_mm=2.5`, `amplitude_mm=4.0`, and `wavelength_mm=12.0` for long
shorelines; use those as the default unless a shape needs a tighter turn.
