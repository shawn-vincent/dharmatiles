# Leaf Jitter, Placement Fixes, and n_col Improvements — Session 2026-06-29

## What We Were Working On

Wiring the long-disabled `leaf_angle_jitter_deg` / `leaf_pos_jitter` parameters through
the entire call chain, fixing several correctness bugs discovered in the process, and
making other incremental placement improvements.

---

## Changes Made This Session (all committed to `main`)

### 1. `n_col` uses max(belly_perim, row_perim)

`placement.py`: leaf count per row was always derived from the belly cross-section
perimeter (`_belly_perim`).  Changed to `max(_belly_perim, perim)` so whichever
ring is wider drives the leaf count.  Prevents under-populated rows on clusters where
the row cross-section is wider than the belly (common on the cone body near the dome
transition).

### 2. Bottom-row z-anchor uses world-Z directly

`mesh.py` `_compute_row_z_positions`: previously converted `z_placeable` to an arc
position then back to world-Z:
```python
# old
s_placeable  = avg_arc_for_z(z_placeable, meridians)
z_bot_anchor = avg_z_for_arc(s_placeable + leaf_length_mm, meridians)
```
Changed to:
```python
# new — avoids arc/Z round-trip bias on steep cone sections
z_bot_anchor = z_placeable + leaf_length_mm
```
The arc→Z→arc→Z round-trip introduced a small positive bias on steeply-angled
sections, pushing the bottom row slightly too high and leaving a visible gap.

### 3. Floor guard for tip-below-mesh leaves

Added `_FLOOR_TOL_MM = 0.1` tolerance and a per-leaf check:
```python
_tip_z = pt3d[2] + L * tangent[2]
if _tip_z < _z_min_mesh - _FLOOR_TOL_MM:
    stats.skipped_below_floor += 1
    continue
```
The contact angle (especially negative outward-lean values from steep sections) can
make the leaf tangent steeper than the Tz-based bottom anchor assumes, extending the
tip a fraction of a mm below the mesh floor.  The tolerance allows up to 0.1 mm
without flagging (below FDM print-layer resolution).  Tracked in
`LeafPlacementStats.skipped_below_floor`.

### 4. Phi mismatch fallback for belly alignment

When the 2D coordinate system of the belly cross-section is rotated relative to the
row cross-section (a Trimesh artefact on some tilted cluster geometries), the azimuthal
angle `phi_2d` maps to different physical directions in the two cross-section planes.
This caused twisted leaf frames and buried contact candidates.

Fix: measure the local-azimuth mismatch between `pt3d` (relative to its centroid) and
`pt3d_n` (relative to the belly centroid).  If the error exceeds 5°, fall back to
placing the belly query at `(belly_centroid + row_radial_offset, z_belly)` and snapping
to the mesh surface.

### 5. `_contact_angle_for_mesh` loop threshold fix

The doubling-step loop used `_max_inside(lo) > 0.0` as the "still buried" test.
Mesh-vertex discretisation can produce a tiny positive value (~`contact_tol_mm`) for
a leaf that is effectively "just touching."  Changed to `> contact_tol_mm` to treat
such marginal cases as touching rather than buried, preventing the loop from jumping
over the real zero-crossing.

### 6. Added `_avg_Tz_for_z()` helper

`mesh.py`: new helper that returns the average Tz (dz/ds, the z-component of the
meridian tangent = radial component of the outward normal) at a given world-Z.  Used
to estimate surface slope for the bottom-anchor calculation.  Falls back to 0.866
(= √3/2, a 30° slope) when no meridian covers the target Z.

---

## Jitter Implementation (the main focus of this session)

### Why jitter was previously a no-op

`leaf_angle_jitter_deg` and `leaf_pos_jitter` existed as parameters on the `Tree`
layer and `build_tree_mesh()` since early development, but were never forwarded to
`place_leaves_on_mesh()`.  The function signature didn't accept them and they weren't
in `_leaf_kw`.  The result: all leaves placed with zero jitter regardless of what the
tile spec said.

Additionally, `_build_foliage_cluster_mesh` was also missing both parameters in its
signature and call site, causing a `NameError` at tile generation time once the
wiring to `place_leaves_on_mesh` was added.

### What each parameter means (now correct)

**`leaf_angle_jitter_deg`** — rotates the leaf's growth direction (`T0`) around the
surface normal (`up_hint`) by a random angle in `±leaf_angle_jitter_deg`.  The leaf
base stays pinned at its attachment point; the tip swings azimuthally in the surface
tangent plane.  This is NOT a blade roll/tilt — the leaf stays flat against the
surface, just pointing in a slightly different direction.

```python
_theta = radians(angle_jitter_deg) * (_hash01(seed, "ang_j", row_idx, ci) * 2 - 1)
T0 = normalize(T0 * cos(_theta) + cross(up_hint, T0) * sin(_theta))
```

**`leaf_pos_jitter`** — nudges the attachment point `pt3d` in two independent random
directions within the surface tangent plane, then snaps it back to the mesh surface:
- Along `T0` (tangential to the surface, in the growth direction)
- Along `cross(up_hint, T0)` (lateral direction in the tangent plane)

Scale is `pos_jitter * leaf_length_mm` for each axis independently.

```python
_jmm = pos_jitter * L
_lat = cross(up_hint, T0)
pt3d += T0 * (_jmm * r_t) + _lat * (_jmm * r_l)
pt3d = proximity.on_surface(pt3d)[0]   # snap back to mesh
```

Both applied AFTER `T0` is computed (so the surface frame is available) and BEFORE
the contact angle (so the contact angle adapts to the jittered position and direction).

### Bug: `_hash01_int` produces correlated per-leaf values

The first jitter attempt used `_hash01_int(seed, tag, row_idx, ci) / 2^64` for the
random values.  `_hash01_int` is FNV-1a without the murmur3 fmix64 finalizer.  When
only `ci` varies between leaves in the same row, the high bits of the hash barely
change — the `/2^64` normalization means high bits dominate the fractional value, so
all leaves in a row get nearly the same random value.

The docstring for `_hash01` in `_utils.py` explicitly warns about this:
> "Without the finalizer, FNV barely diffuses the trailing bytes, so varying only
> the last argument (e.g. a column index) leaves the high bits almost unchanged —
> collapsing per-leaf jitter into a visible grid."

Fix: use `_hash01` (imported from `._utils`) which includes the fmix64 avalanche step.

### Early wrong implementations (documented for future reference)

| Attempt | What it did | Problem |
|---|---|---|
| Phi_2d shift | Perturbed `phi_2d` before polygon lookup | Just more position jitter; indistinguishable from `pos_jitter` |
| `up_placed` roll (Rodrigues) | Rotated blade around stem axis | Leaves tilt side-to-side (user called it "rock/roll") — wrong feel |
| World-XY tangential nudge | `pt3d += cross(radial_xy, z_hat) * jmm` | Ignores surface slope; Z component zero |
| World-Z nudge `pt3d[2] += ...` | Direct Z offset | Too small at `pos_jitter * col_step` scale (≈0.5 mm); invisible |
| Correct: surface-frame nudge | T0 + lateral in tangent plane, snap | Correct; scale `pos_jitter * L` visible |

### Parameters used in test and tile

| Context | `leaf_angle_jitter_deg` | `leaf_pos_jitter` | `leaf_lift_mm` | `leaf_h_overlap` | `leaf_v_overlap` |
|---|---|---|---|---|---|
| Algorithm default | 24.0 | 0.165 | 3.0 | 0.2 | 0.5 |
| Test script | 24.0 | 0.165 | 1.5 | 0.2 | 0.5 |
| 1x1-grass-tree+water tile | — | — | 1.5 | 0.1 | 0.25 |

---

## Key Files

| File | Role |
|---|---|
| `src/dharmatiles/trees/placement.py` | `place_leaves_on_mesh` — jitter applied at ~line 644 (angle) and ~line 655 (pos) |
| `src/dharmatiles/trees/mesh.py` | `_build_foliage_cluster_mesh`, `_compute_row_z_positions`, `_avg_Tz_for_z` |
| `src/dharmatiles/trees/layer.py` | `Tree` layer — jitter params stored and forwarded |
| `src/dharmatiles/trees/_utils.py` | `_hash01` (with fmix64 finalizer) — must use this, NOT `_hash01_int / 2^64` |
| `src/scripts/test-leaf-placement.py` | Test harness; `_PLACE_KW` now includes jitter params and correct defaults |
| `src/tiles/water/1x1-grass-tree+water.tile.py` | `leaf_lift_mm=1.5` |

---

## Commits This Session

```
fbc832e trees: wire jitter params through _build_foliage_cluster_mesh; tile lift 1.5 mm
0938b1f test: halve leaf_lift_mm to 1.5
47f16d8 test: set leaf lift/h_overlap/v_overlap to algorithm defaults
f9c2a75 trees: fix leaf jitter — surface-frame pos, independent angle per leaf
7d5efa6 trees: leaf placement refinements — max-perimeter n_col, floor guard, z-anchor
```
