# Hex Paint Organizer — Magnet Interconnect Design

## Goal

Add recessed 10 × 3 mm disc-magnet pockets so that multiple copies of the organizer
can be joined together in a flat 2-D grid: front-to-back, left-to-right, or any
combination.

---

## Terminology

Because the organizer sits flat on a table, "top" and "bottom" refer to the two
**horizontal faces** (the ones you'd see looking straight down or up), while
"sides" refers to the four **vertical outer walls**.  The user requested:

| Name used here | Physical face | Connections to… |
|---|---|---|
| **Top face** | z = height (looking up) | underside of a vertically stacked copy |
| **Bottom face** | z = 0 (resting on table) | top face of a copy below |
| **North wall** | high-Y vertical face | south wall of a copy behind |
| **South wall** | low-Y vertical face | north wall of a copy in front |
| **West wall** | low-X vertical face | east wall of a copy to the left |
| **East wall** | high-X vertical face | west wall of a copy to the right |

**Primary use case:** horizontal tiling (north ↔ south, west ↔ east) — multiple
organizers clipped together on a desktop.  Top/bottom magnets enable optional
vertical stacking (one organizer on top of another).

---

## Parameter Changes

| Field | Old | New | Reason |
|---|---|---|---|
| `wall` | 1.0 mm | **4.0 mm** | Accommodates 3 mm-deep magnet pocket + 1 mm backing |
| `floor` | 5.0 mm | **10.0 mm** | Taller bottom ring gives room for all side magnet centres |
| `base` | 2.0 mm | **5.0 mm** | Ensures solid material for bottom-face vertical pockets |
| `magnet_dia` | *(new)* | **10.0 mm** | Standard N52 disc magnet diameter |
| `magnet_depth` | *(new)* | **3.0 mm** | Standard N52 disc magnet thickness / bore depth |

---

## Derived Dimensions (3 cols × 4 rows default)

| Symbol | Formula | Value |
|---|---|---|
| `outer_f2f` | `bore_f2f + 2 × wall` | 43.00 mm |
| `R_outer` | `outer_f2f / √3` | 24.83 mm |
| `col_pitch` | `bore_f2f + wall` | 39.00 mm |
| `row_pitch` | `col_pitch × √3/2` | 33.78 mm |
| Hex bounding box W | `2×col_pitch + outer_f2f + col_pitch/2` | 140.50 mm |
| Hex bounding box D | `3×row_pitch + 2×R_outer` | 150.98 mm |

---

## Structural Additions

### 1. Rectangular outer frame (z = 0 – 10 mm)

The hex honeycomb's outer perimeter is irregular: in a pointy-top layout the
boundary alternates between flat ±x faces and 60° angled vertices, so there are
no consistent flat faces in the ±y direction.  A **rectangular outer frame** wraps
the lower 10 mm to give four clean flat faces for magnet pockets and module-to-module
contact.

```
Frame inner rect  (= hex bounding box):
  X  [−21.50,  119.00]   width 140.50 mm
  Y  [−24.83,  126.15]   depth 150.98 mm

Frame outer rect  (inner + frame_wall = 4 mm on each side):
  X  [−25.50,  123.00]   width 148.50 mm
  Y  [−28.83,  130.15]   depth 158.98 mm

Frame height:  10 mm  (z = 0 → floor)
Frame wall:     4 mm
```

The frame is built as a solid rectangular prism (outer dims) with the hex cup
assembly unioned into it.  Hex bores and depressions are subtracted afterwards,
leaving solid frame walls everywhere from z = 0 to z = 10 mm.

### 2. Solid top cap (z = 60 – 64 mm)

At z = height (60 mm) only the 4 mm-wide cup-wall ring is solid per cup.
A 10 mm-diameter vertical bore cannot fit in 4 mm of ring material (needs 5 mm
clearance each side).  A **solid rectangular top cap** (same outer footprint as the
frame, 4 mm tall) is added above the cups to provide a flush top surface with
sufficient material for vertical magnet pockets.

```
Top cap outer rect:  X [−25.50, 123.00]  Y [−28.83, 130.15]
Top cap height:  4 mm  (z = 60 → 64)
```

The cup bores pierce through the cap (bore F2F 35 mm hex subtracted to z = 64 mm)
so the bottles still drop in.  The solid cap material is the rectangular frame
ring, identical in XY cross-section to the bottom frame.

---

## Magnet Pocket Catalogue

### Side magnets — 8 total (2 per vertical face)

All are **horizontal cylinders**, axis perpendicular to the face, 10 mm dia, 3 mm
deep, centred at **z = 5 mm** (mid-height of the 10 mm frame).  Backing = 1 mm
(4 mm wall − 3 mm bore).

```
South face  (y_outer = −28.83 mm, bore axis = +y):
  S1:  x =  24.00 mm,  y_entry = −28.83 mm
  S2:  x =  73.50 mm,  y_entry = −28.83 mm

North face  (y_outer = 130.15 mm, bore axis = −y):
  N1:  x =  24.00 mm,  y_entry = 130.15 mm
  N2:  x =  73.50 mm,  y_entry = 130.15 mm

West face  (x_outer = −25.50 mm, bore axis = +x):
  W1:  y =  24.17 mm,  x_entry = −25.50 mm
  W2:  y =  77.16 mm,  x_entry = −25.50 mm

East face  (x_outer = 123.00 mm, bore axis = −x):
  E1:  y =  24.17 mm,  x_entry = 123.00 mm
  E2:  y =  77.16 mm,  x_entry = 123.00 mm
```

Spacing derivation:
- S/N x-centres at 1/3 and 2/3 of frame width 148.50 mm → −25.50 + 49.5 = **24.0 mm**, −25.50 + 99.0 = **73.5 mm**
- W/E y-centres at 1/3 and 2/3 of frame depth 158.98 mm → −28.83 + 53.0 = **24.17 mm**, −28.83 + 106.0 = **77.17 mm**

Centre-to-centre separation: 49.5 mm (S/N), 53.0 mm (W/E) — both far exceed the 10 mm minimum.

### Bottom face magnets — 2 total

**Vertical cylinders**, axis = +z, bored from z = 0 upward 3 mm.  Solid material
extends to z = base = 5 mm, giving 2 mm margin beyond the bore.

Position: centred in the solid south and north **frame walls** at the bottom face.

```
Bottom-1:  x =  48.75 mm,  y = −26.83 mm   (south frame-wall midline)
Bottom-2:  x =  48.75 mm,  y = 128.15 mm   (north frame-wall midline)
```

`x = 48.75` is the midpoint of the frame's outer X span (−25.5 to 123.0).
`y` values are midpoints of the south/north frame wall bands.

### Top face magnets — 2 total

**Vertical cylinders**, axis = −z, bored from z = 64 mm downward 3 mm.  Same XY
positions as bottom magnets so a stacked module aligns exactly.

```
Top-1:  x =  48.75 mm,  y = −26.83 mm,  z_entry = 64 mm
Top-2:  x =  48.75 mm,  y = 128.15 mm,  z_entry = 64 mm
```

---

## Fit Checks

| Pocket | Bore depth | Available material | Backing |
|---|---|---|---|
| Side (frame wall) | 3 mm | 4 mm wall | 1 mm ✓ |
| Bottom (frame base) | 3 mm | 5 mm solid base | 2 mm ✓ |
| Top (top cap) | 3 mm | 4 mm cap height | 1 mm ✓ |

---

## Module Connectivity

When two modules share a face, their opposing magnet pockets face each other.
Insert a 10 × 3 mm neodymium disc magnet into each pocket (press-fit or glue
flush).  Modules click together along any of the four vertical walls (horizontal
tiling) or stack vertically via the top/bottom face magnets.

Polarity rule: magnets on **opposing faces must attract**.  When installing, verify
orientation before gluing.

---

## Implementation Plan

1. **Update `HexOrganizerSpec`** — add `floor`, `base`, `magnet_dia`, `magnet_depth` fields; update `wall` default.
2. **`build_organizer`** — after the honeycomb union, union in the rectangular outer frame (bottom 10 mm) and the top cap (top 4 mm).
3. **`_subtract_magnets(manifold, spec)`** — helper that cuts the 12 cylinder pockets (8 side + 2 bottom + 2 top) from the assembled manifold.
4. **`main`** — report new overall dimensions.

No changes to `single_cup` geometry except the updated `wall` / `floor` / `base` defaults.
