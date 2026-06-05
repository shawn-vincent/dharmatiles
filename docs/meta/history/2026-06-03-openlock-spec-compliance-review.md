# OpenLOCK Spec Compliance Review

**Date**: 2026-06-03  
**Analyst**: Claude Sonnet 4.6  
**Scope**: All five current tile specs, OpenLOCK output only  
**Reference spec**: `docs/openlock-spec.md` (compiled from OpenSCAD-OpenLock template + Printable Scenery docs)

---

## Tiles Generated

All five current tile definitions were generated fresh for this review:

| Tile | Spec file | OpenLOCK output |
|---|---|---|
| Default grass | (flags) | `stl/tile-openlock.stl` |
| Half grass / soil | `tiles/half-grass-soil.tile` | `stl/half-grass-soil-openlock.stl` |
| Corner grass | `tiles/corner-grass.tile` | `stl/corner-grass-openlock.stl` |
| Grass and water | `tiles/grass-and-water.tile` | `stl/grass-and-water-openlock.stl` |
| Coast left | `tiles/coast-left.tile` | `stl/coast-left-openlock.stl` |

> **Pre-generation fix required**: `tiles/corner-grass.tile` was written in TOML format
> (`[[region]]` syntax) which the `_load_yaml_spec` loader does not support — it only
> handles YAML. The file was converted to YAML as part of this session. The spec
> loader has no TOML path; any future `.tile` file written in TOML will fail with a
> YAML parse error.

---

## Dimension Summary

Measurements taken by loading each STL with trimesh and inspecting bounding boxes and
vertex positions.

| Tile | XY footprint | Base bottom (Z) | Terrain top (Z) | Faces |
|---|---|---|---|---|
| Default grass | 25.0 × 25.0 mm | −8.00 mm | +8.57 mm | 1,515,080 |
| Half grass/soil | 25.0 × 25.0 mm | −8.00 mm | +7.99 mm | 1,053,320 |
| Corner grass | 25.0 × 25.0 mm | −8.00 mm | +8.21 mm | 485,480 |
| Grass and water | 25.0 × 25.0 mm | −8.00 mm | +8.93 mm | 1,002,968 |
| Coast left | 25.0 × 25.0 mm | −8.00 mm | +12.24 mm | 1,321,880 |

All tiles share identical base geometry — the difference is only in the terrain surface.
coast-left's higher terrain top (12.24 mm) comes from the extra 2+ layers of grass blades
growing up from the raised meadow (grass region at 5 mm terrain height + up to ~7 mm
blade height).

---

## Compliance Assessment

### ✅ PASS: Footprint (metric variant)

All tiles: **25.0 × 25.0 mm** per 1×1 square.

The OpenLOCK standard defines two variants:
- **Imperial** (canonical): 25.4 mm (1 inch) per unit
- **Metric**: 25.0 mm per unit — *what we implement*

Within a consistent metric ecosystem this is correct. Metric OpenLOCK tiles cannot
tessellate with imperial tiles; the 0.4 mm per-unit discrepancy compounds to 1.6 mm
over a 4-unit span, which is out of tolerance for clip engagement. Both variants are in
common use in the community; the important thing is to choose one and label it clearly.

**DharmaTiles uses the metric variant.**

---

### ✅ PASS: Base height

All tiles: base box from **z = −8.0 mm** to **z = 0.0 mm** = **8.0 mm tall**.

The OpenSCAD reference (`WallHeight = 8`) gives 8.0 mm. This matches exactly.

---

### ✅ PASS: Clip slot Z position and height

Inspected on `stl/tile-openlock.stl`:

| Measurement | Ours | Spec | Result |
|---|---|---|---|
| Slot bottom (from base bottom) | 1.4 mm | `CUTOUT_START_Z` = 1.4 mm | ✅ |
| Slot top (from base bottom) | 5.6 mm | 1.4 + 4.2 = 5.6 mm | ✅ |
| Slot height (Z span) | 4.2 mm | `CUTOUT_HEIGHT` = 4.2 mm | ✅ |
| Slot bottom absolute Z | −6.6 mm | −8.0 + 1.4 = −6.6 mm | ✅ |
| Slot top absolute Z | −2.4 mm | −6.6 + 4.2 = −2.4 mm | ✅ |

---

### ✅ PASS: Clip slot entry width

Slot mouth at the tile edge face: **14.0 mm** wide (x = 5.5 to 19.5 on 25 mm tile),
centred at x = 12.5 mm.

`CUTOUT_WIDE_1 = 14` ✅

---

### ✅ PASS: One slot per edge, centred per unit

Each 25.0 mm edge has one slot, centred at 12.5 mm from each end. For multi-square
tiles the pattern extends with one slot centred per square. Matches the spec. ✅

---

### ❌ FAIL: Clip slot is rectangular, not T-shaped

**This is the most significant functional defect.**

The official OpenLOCK clip socket is a **stepped T-shape** in plan (looking down):

```
Tile edge face
│
│  ←———— 14 mm ————→   (0–2 mm depth)
│    ←—— 12 mm ——→     (2 mm depth step)
│      ←— 10 mm —→     (2–5 mm depth)
│      ←— 10 mm —→     (5–7 mm, safety relief)
```

This T-shape is what mechanically **retains** the clip: the clip slides into the wide
entry, then its head engages the narrower inner cavity and cannot be pulled out straight.
Releasing requires pressing the clip's spring arm.

**Our implementation** creates a plain rectangle: 14 mm wide × 7 mm deep × 4.2 mm
tall — no step at 2 mm, no narrowing at 5 mm. This means:

1. A real OpenLOCK clip can enter our socket (the entry is correct size)
2. The clip will **not lock** — it can slide back out without engaging
3. Two tiles using our sockets will not clip together

**Root cause** (`src/dharmatiles/bases/openlock.py`, `_add_socket_void`):

```python
u0 = center - CUTOUT_WIDE_1 / 2.0   # 5.5
u1 = center + CUTOUT_WIDE_1 / 2.0   # 19.5
v0 = 0.0
v1 = CUTOUT_DEEP_3 + 2.0            # 7.0  ← one box, no stepping
```

The function builds five quads forming a simple rectangular box. The constants
`CUTOUT_WIDE_2 = 12`, `CUTOUT_DEEP_1 = 2`, `CUTOUT_WIDE_3 = 10`, `CUTOUT_DEEP_3 = 5`
are all imported but never used in the void geometry.

**Fix required**: Replace the single-rectangle void with the stepped polygon from the
OpenSCAD template:

```python
# Correct T-shaped pocket (extruded in Z from CUTOUT_START_Z to CUTOUT_START_Z + CUTOUT_HEIGHT)
# In local (u, v) coordinates (u = along edge, v = depth into tile):
polygon_uv = [
    (-7, -2), (-7, 2),   # outer mouth: 14mm wide entry, opening extends 2mm outside face
    (-6,  2), (-5, 5),   # step to 12mm at v=2, then 10mm at v=5
    (-5,  7), ( 5, 7),   # back wall at 10mm wide, v=7
    ( 5,  5), ( 6, 2),   # mirror steps
    ( 7,  2), ( 7, -2),  # right side of mouth
]
```

---

### ❌ FAIL: Missing clip retention side cut

The OpenSCAD reference includes a second cut for the clip's retention arm:

```scad
translate([-8.35, 0, (cutoutheight + cutoutstartz)/2 - 1])
cube([4.7, 18, cutoutheight + cutoutstartz + 2], center=true);
```

This is a **4.7 mm wide × 18 mm tall** slot offset **8.35 mm** to one side of the
socket centre. It provides the clearance pocket that the clip's spring tab needs to
deflect during insertion. Without it, the clip arm has nowhere to flex — clips may
either not seat at all or crack the socket during insertion.

Our implementation has no equivalent. This is required alongside the T-shape fix.

---

### ⚠️ WARNING: Mesh is not watertight

All five tiles report `is_watertight: False`. Edge analysis:

| Location | Open edge count |
|---|---|
| z ≈ 0 (terrain–base join seam) | 391,746 |
| z ≈ −8 (base bottom) | 114 |
| Terrain surface (z ≈ 3–12) | 4,153,380 |
| **Total** | **4,545,240** |

**The terrain surface open edges** (~4.1 M) are the concerning category — these are
within the heightmap mesh itself, not at the join seam. They arise because
`make_heightmap_solid` builds each quad as two separate triangles whose vertices are
not merged, and the grass blade meshes are appended via `trimesh.util.concatenate`
without merging shared edges.

In practice, PrusaSlicer and Bambu Studio auto-repair non-watertight meshes during
slicing and will likely handle these without issue. However a clean manifold mesh is
the correct target for 3D-printable geometry, and the non-watertight status will cause
some tools (MeshLab volume calculation, boolean operations, strict validators) to
fail.

**DungeonBlocks tiles have the same issue** — confirmed by parallel generation.

---

### ⚠️ WARNING: XY terrain scale compression

The OpenLOCK export pipeline scales the terrain from the 35 mm DharmaTiles grid down
to 25 mm:

```python
XY_SCALE = OPENLOCK_SQUARE_MM / DUNGEONBLOCKS_SQUARE_MM  # = 25.0 / 35.0 ≈ 0.714
```

All terrain features are scaled:
- Grass blade widths (0.80–1.07 mm at 35 mm) → **0.57–0.76 mm at 25 mm**
- Stone radii (1.82–2.40 mm at 35 mm) → **1.30–1.71 mm at 25 mm**
- Soil bump sigma (0.22–1.03 mm at 35 mm) → **0.16–0.74 mm at 25 mm**

**Grass blade widths at 0.57–0.76 mm are below the practical FDM extrusion width**
(0.4 mm nozzle → 0.4–0.5 mm minimum feature). Blades thinner than one extrusion width
print as a single-perimeter line and are structurally fragile. This is a print-quality
concern at 25 mm, less so at 35 mm.

No action required to satisfy the OpenLOCK spec (the spec only constrains the base),
but worth noting for practical printability.

---

### ℹ️ NOTE: terrain top height above base

The spec does not constrain terrain height. Our tiles:

| Tile | Terrain top | Vs. base top (z=0) |
|---|---|---|
| Default grass | +8.57 mm | 8.57 mm above base face |
| Half grass/soil | +7.99 mm | 7.99 mm |
| Corner grass | +8.21 mm | 8.21 mm |
| Grass and water | +8.93 mm | 8.93 mm |
| Coast left | +12.24 mm | 12.24 mm |

These are reasonable — taller than a bare floor deck but within normal terrain tile
heights. The coast-left tile's extra height comes from tall grass blades on a 5 mm
terrain base.

---

## Summary Table

| Criterion | Status | Notes |
|---|---|---|
| Footprint per unit | ✅ PASS | 25.0 mm (metric variant) |
| Base height | ✅ PASS | 8.0 mm |
| Clip slot Z: bottom | ✅ PASS | −6.6 mm (1.4 mm from base bottom) |
| Clip slot Z: height | ✅ PASS | 4.2 mm |
| Clip slot entry width | ✅ PASS | 14.0 mm (CUTOUT_WIDE_1) |
| Clip socket count/position | ✅ PASS | 1 per 25 mm edge, centred |
| **Clip slot T-shape** | **❌ FAIL** | Rectangular, not stepped — clips won't lock |
| **Clip retention side cut** | **❌ FAIL** | Missing 4.7 mm × 18 mm retention slot |
| Mesh watertight | ⚠️ WARN | ~4.5 M open edges; slicers auto-repair |
| Metric vs. imperial | ⚠️ WARN | 25.0 mm ≠ 25.4 mm; parallel ecosystem |
| Feature scale at 25 mm | ⚠️ WARN | Grass ~0.6 mm blades: borderline FDM |

---

## Recommended Fixes (in priority order)

### Priority 1 — Functional: T-shaped clip socket

**File**: `src/dharmatiles/bases/openlock.py`

Replace `_add_socket_void` with one that builds the proper stepped T-polygon and
extrudes it over the CUTOUT_HEIGHT z span. The OpenSCAD polygon is well-documented in
`docs/openlock-spec.md` §3b. The fix is ~40 lines.

### Priority 2 — Functional: Clip retention side cut

**File**: `src/dharmatiles/bases/openlock.py`

After placing each T-socket, subtract an additional rectangular slot (4.7 mm wide ×
18 mm in Y ×`CUTOUT_HEIGHT + CUTOUT_START_Z` tall) centred 8.35 mm to one side of
the socket centre. See spec §3c.

### Priority 3 — Quality: Mesh watertightness

**File**: `src/dharmatiles/core/mesh.py` and `terrains/tile.py`

After `trimesh.util.concatenate`, run `mesh.merge_vertices()` and verify
`mesh.is_watertight`. The terrain–base seam can be closed by ensuring the base mesh
top-cap vertices exactly match the terrain solid bottom vertices. Alternatively,
`trimesh.boolean.union` over the combined parts would produce a manifold result but is
significantly slower.

### Priority 4 — Compatibility: Imperial option

Add an `openlock_imperial: bool` flag to `BaseConfig` (default `False`). When set,
use `OPENLOCK_SQUARE_MM = 25.4` and update the XY_SCALE. Document clearly that mixing
metric and imperial tiles is not recommended.

---

## Spec Document Created

`docs/openlock-spec.md` — complete OpenLOCK technical specification compiled from:
- OpenSCAD-OpenLock source (github.com/caitlynb/OpenSCAD-OpenLock) — primary authority on dimensions
- Printable Scenery developer page — overview and version information
- TerrainTinker knowledge base — background and operational context

The document covers: grid variants, wall heights, clip socket geometry (full polygon),
clip positions, tessellation requirements, and a quick-reference dimension table.
