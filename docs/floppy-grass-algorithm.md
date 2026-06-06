# FloppyGrassLayer — Algorithm Reference

A plain-language description of how flat grass blades are grown, stacked,
and placed on the terrain.  Inputs, rules, and the reasoning behind each choice.

---

## What it produces

Flat ribbon grass blades lying on the terrain surface.  Each blade:

- grows outward from a seed point following the wind/flow direction,
- lies flat on the ground by default,
- rises only when it must cross a stone or another blade already lying there,
- drops back to the ground immediately after crossing the obstacle.

---

## Inputs

| Input | Source | What it is |
|---|---|---|
| `terrain_z` | `TileScene` | Per-cell terrain height (mm). Flat 5 mm for grass regions. |
| `scene.support_z` | `TileScene` | Terrain + stone stamps built by earlier layers. Never modified here. |
| `grass_mask` | `TileScene` | Boolean grid: which cells may grow grass. |
| `stone_mask` | `TileScene` | Boolean grid: which cells are under stone centres. |
| `flow_angle_field` | `build_flow_field()` | Per-cell wind/lean direction (radians). |
| `SpeciesConfig` | config | Blade geometry: width, length, segment length, curl, etc. |
| `SolverConfig` | config | `max_stack_height`: mm above terrain a blade may seed or grow into. |

---

## Key constants (flat-blade geometry)

```
FLAT_STAMP     = 0.06 mm   — physical blade thickness (bottom to top surface)
FLAT_CLEARANCE = 0.01 mm   — tiny lift so the blade never sits inside the terrain
```

The mesh builder (`_build_flat_blade_mesh`) puts the **bottom surface at the spine Z**
and the **top surface at spine Z + FLAT_STAMP**.  These constants must match exactly.

---

## Occupancy grids

| Name | Contents | Modified during growth? |
|---|---|---|
| `scene.support_z` | Terrain shape + stone stamps. Built by earlier layers. | **Never** — this layer only reads it as the "before-grass floor". |
| `occ_z` | Live copy that starts equal to `scene.support_z` and grows as blades stamp. | Yes — updated immediately after every blade step. |

Because `scene.support_z` is never written by this layer, it naturally serves as
the fallback floor — no frozen copy needed.

---

## The stamp: a perpendicular strip

Each time a blade places a new point, it stamps a **perpendicular strip** into `occ_z`:

```
one cell wide  in the growth direction
±hw cells wide perpendicular to growth direction
```

**Why not a full ±hw square footprint?**
The step size is `cell_w` ≈ 0.14 mm.  The blade half-width `hw` is up to 1 mm.
A ±hw square footprint around the new position would contain ~93% the same cells
as the stamp at the previous position — the "next cell" would always be inside
the "previous stamp", causing the blade to see its own trail as an obstacle and
climb it.

With a perpendicular strip, each step stamps a fresh line of cells.  The strip
at step N and the strip at step N+1 are adjacent, non-overlapping lines.  The
next target cell is never inside the previous stamp.

**Why still ±hw wide perpendicular?**
So crossing blades detect each other.  If blade A grows east and blade B grows
north, when B's path centre reaches a cell that A's perpendicular strip covers,
B reads A's stamp and rises above it.  A single-cell stamp would miss wide-blade
crossings at oblique angles.

---

## Own-trail detection (per-blade `own_stamps` dict)

Each blade carries `own_stamps`, a dict mapping `(row, col) → max_z` for every
cell this blade has stamped.  At each step, before computing the next Z:

```
sz_raw = occ_z[target_cell]
own_z  = own_stamps[target_cell]   (0 if never stamped by this blade)

if sz_raw > own_z:
    sz_t = sz_raw                       # external obstacle — rise to clear it
else:
    sz_t = scene.support_z[target_cell] # own trail only — ignore, use stone/terrain floor
```

**Why is this needed?**
Even with perpendicular strips, a blade growing diagonally may stamp cells that
partially overlap across steps.  The own-stamps check catches any residual
own-trail cells and prevents the blade climbing itself regardless of stamp shape.

**Why `scene.support_z` as fallback, not `terrain_z`?**
Stones are stamped into `scene.support_z` at their top surface.  If the blade
has previously crossed a stone and stamped over it, the fallback must still
include the stone height so the blade doesn't drop back through the stone.

---

## Step 1 — Plant seeds

For each Voronoi-style group cell (seeded from random spread sites across the
valid grass mask):

1. Choose a random number of blades for this group (`group_min`–`group_max`).
2. For each blade, pick a random cell inside the group cell and jitter within that cell.
3. **Reject** if the cell is under a stone, outside the grass mask, or `occ_z` is already
   more than `max_stack_height` mm above terrain.
4. **Z of the seed point** = `max(terrain_z, occ_z) + FLAT_CLEARANCE`
   — sits on whatever is already there.
5. **Direction** = flow angle at the seed location + small Gaussian jitter.
6. **Stamp** a perpendicular strip in `occ_z` (and record in `own_stamps`)
   so blades seeded later nearby can detect this one.

Seeds are planted with immediate stamps in the global blade order, so seed B
planted after seed A at the same location correctly stacks above A.

---

## Step 2 — Grow (rounds)

Growth runs for up to `round(blade_length / blade_segment_length)` rounds, where
`blade_length` is sampled from `blade_length_min` to `blade_length_max` at seed
creation time.  Each round, every alive blade attempts one step of size
`blade_segment_length` forward.

A **single global processing order** (a random permutation fixed before growth
begins) is reused every round.  Blade A always stamps before blade B in every
round — stacking precedence is consistent across all rounds.

Stamps are written **immediately** after each step, so blade B, processed after
blade A in the same round, already sees A's new strip.

### Per-step rules (in order)

**Rule 1 — Boundary.**  If the next point would leave the tile footprint, stop.

**Rule 2 — Grass mask.**  If the next cell is outside the grass region, stop.

**Rule 3 — Compute target Z.**

```
sz_raw = occ_z[target_cell]
own_z  = own_stamps[target_cell]

sz_t = sz_raw if sz_raw > own_z else scene.support_z[target_cell]

nz = max(terrain_z[target_cell], sz_t) + FLAT_CLEARANCE
```

On flat terrain with no obstacles, `sz_raw == own_z` (own trail), so
`sz_t = terrain`, and `nz = terrain + FLAT_CLEARANCE`.  The blade lies flat.

**Rule 4 — Rise cap.**  If `nz > prev_z + rise_cap` (default 2 mm), stop.
The blade cannot clear this obstacle in one step.

**Rule 5 — Append and stamp.**

```
blade.path.append((tx, ty, nz))
_stamp_strip(occ_z, tx, ty, nz + FLAT_STAMP, hw, direction, own_stamps)
```

**Why drops are automatic:**
After crossing a stone, the cells beyond the stone have `occ_z = scene.support_z = terrain`.
`own_z = 0`.  `sz_t = terrain`.  `nz = terrain + FLAT_CLEARANCE`.  The blade drops
back to ground level with no special logic.

---

## Step 3 — Build meshes

For each blade with ≥ 2 path points:

1. Taper the width from full → 25% over the last 18.75% of the blade length (the tip).
2. Clip path XY to tile footprint.
3. Call `_build_flat_blade_mesh(path_arr, widths)` → closed watertight ribbon.
   Bottom at `path_z`, top at `path_z + 0.06 mm`.
4. Hard-clamp all vertices to tile footprint (catches smoothing overshoot).


---

## Summary of rules

```
Flat terrain, no obstacles:   nz = terrain + FLAT_CLEARANCE  (lies flat)
Another blade at target cell: nz = other_blade_top + FLAT_CLEARANCE  (rises)
Stone at target cell:         nz = stone_top + FLAT_CLEARANCE  (rises)
One step past obstacle:       nz = terrain + FLAT_CLEARANCE  (drops automatically)
Rise > rise_cap:              blade stops
Outside tile or grass region: blade stops
```

---

## Tuning parameters

| Parameter | Default | Effect |
|---|---|---|
| `blade_segment_length` | 0.8 mm | Physical growth step length |
| `blade_length_min / blade_length_max` | 8.0–14.4 mm | Target blade length range sampled at seed creation |
| `rise_cap` | 2.0 mm | Tallest single-step climb; stones taller than this stop the blade |
| `max_stack_height` | 2.0 mm | Max occ_z above terrain for seeding or growing |
| `blade_width_min / blade_width_max` | 0.75–2.0 mm | Blade width range (also sets perpendicular stamp coverage) |
| `curl_min / curl_max` | 0.0–0.8 rad | Curl magnitude range; sign is chosen randomly per seed |
| `groups_per_square` | 50 | Blade group density per 35 mm square |
| `group_min / group_max` | 20–30 | Blades per group (controls clumping) |
| `group_dir_jitter` | 0.14 rad | Per-blade direction noise within a group |
