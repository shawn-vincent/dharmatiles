# Continuation: extend bank slope below waterline

## Context

Working in `/Users/svincent/projects/dharmatiles`, a Python STL terrain-tile generator.
Relevant file: `src/dharmatiles/terrains/tile.py`.

## What exists

`_extend_bank_slope_into_pool(terrain_z, water_mask, water_height, surface)` already
extrapolates the boundary-strip slope into pool (water-mask) cells, smoothly going from
`water_height` (3 mm) at the inner ring down to 0 mm at the pool bed.  This is correct
and should NOT be changed.

`_extend_bank_below_water(terrain_z, water_mask, region_mask)` was JUST ADDED (wrongly).
It rescales boundary-strip cells (region_mask < 0) so terrain reaches 0 mm at the
water-facing edge.  This changes the slope INSIDE the boundary strip, which the user
does NOT want.  **Remove this function and its call site.**

## What the user actually wants

The boundary-strip slope should be UNCHANGED: the IDW terrain goes from ~5 mm (grass
edge) to ~3 mm (water_height) at the water-facing edge of the strip — same as before.

Past the boundary edge, underneath the water, the slope should CONTINUE at the same
gradient, going from 3 mm at the boundary-water junction down to 0 mm some additional
distance into the pool.

`_extend_bank_slope_into_pool` already does this in principle, but it uses a smoothstep
that has zero derivative at the inner ring, so terrain stays near 3 mm for a bit before
sloping — creating a subtle flat shelf right at the waterline.

## The fix

Replace the smoothstep in `_extend_bank_slope_into_pool` with a **linear ramp** (or a
curve whose derivative is NON-ZERO at the inner ring), so the slope begins immediately
from 3 mm as soon as you enter the pool, with no flat shelf.

Specifically, change from smoothstep `t_s = t*t*(3-2*t)` to just `t_s = t` (pure
linear), or use `smoothstep` only at the FAR end (bed side) so the slope eases into the
flat bed but starts immediately at the waterline.

A good option: use a half-smoothstep that is linear at the top (inner ring, t=0) and
smoothly eases to zero slope at the bottom (bed, t=1):

    t_s = t * t   # quadratic: starts with slope, eases to zero at bed

Or simply use linear (`t_s = t`) which is the plainest interpretation of "continue the
slope".

## Also fix embed_mm

The embed was correctly derived from `max(b.width_mm for b in spec.boundaries)` = 2.5 mm.
Keep that.  Keep passing `water_embed_mm=embed_mm` to `_build_mesh`.

## Summary of changes

1. Delete `_extend_bank_below_water` function body and its call in
   `build_tile_from_spec`.
2. In `_extend_bank_slope_into_pool`, change `t_s = t * t * (3.0 - 2.0 * t)` (smoothstep)
   to `t_s = t * t` (quadratic ease-out) so the slope starts immediately at the
   waterline with no flat shelf.
3. Regenerate `src/tiles/grass-and-water.tile`.
