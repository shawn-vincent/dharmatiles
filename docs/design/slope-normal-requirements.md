# Slope-Normal Requirements

Status: documented future requirement. Current implementation is intentionally
world-Z based except for the terrain heightmap itself.

## Current Invariants

- `terrain_z` stores true world-Z terrain height at every grid cell.
- Sloped terrain is already represented correctly in the base terrain mesh.
- Soil blobs currently add height as a vertical offset: `terrain_z += bump`.
- Stones, grass blades, and support cones currently use world-up/world-down
  frames rather than terrain-normal frames.

For `tiles/grass-and-water.tile`, this is acceptable today: grass grows only
in the flat 5 mm meadow, water is flat at 3 mm, and the 5 mm to 3 mm transition
strip is bare soil. The shoreline slope is therefore visually plausible even
though the small soil bumps are vertical offsets rather than true normal
displacements.

## Required Primitive

Add a per-position surface normal helper to `TileScene`:

```python
def terrain_normal(self, x_mm: float, y_mm: float) -> np.ndarray:
    """Unit terrain normal at world position (x_mm, y_mm)."""
```

The normal should be derived from the heightmap gradient:

```text
n = normalize([-dz/dx, -dz/dy, 1])
```

`dz/dx` and `dz/dy` are dimensionless mm/mm slopes. The helper should use
central differences over `terrain_z` and bilinear sampling so callers can ask
for normals at arbitrary world positions.

## Layer Requirements

| Feature | Current behavior | Slope-aware behavior |
|---|---|---|
| Stones | Placed at `terrain_z[x, y]`, upright in world-Z. | Align each stone's local +Z to `terrain_normal(cx, cy)`. Update support rasterization for the tilted footprint. |
| Grass blade bases | Rooted at `terrain_z`; sink/clearance uses world-Z. | Apply sink and clearance along the terrain normal, especially in slope zones. |
| Grass growth checks | Rise limits compare absolute `Δz`. | Compare displacement projected along the local terrain normal. |
| Support cones/posts | Bridging checks use world `-Z`. | Treat terrain-normal `-Z` as the local down direction. |
| Soil blobs | Bump height is a vertical offset. | For steep or feature-bearing slopes, displace surface detail along the terrain normal. |

## Implementation Notes

- Preserve the existing world-Z behavior for flat terrain: a flat heightmap
  must return `[0, 0, 1]`.
- Thread the normal helper through layer APIs only when a layer truly needs
  orientation or normal-space clearance.
- Slope-normal placement is most important once stones, grass, vegetation, or
  support structures are allowed in transition zones.
