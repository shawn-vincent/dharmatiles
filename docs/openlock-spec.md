# OpenLOCK Technical Specification

**System**: OpenLOCK by Printable Scenery  
**Current version**: OpenLOCK 8.6 (Clip 5.4 as of 2025)  
**License**: Creative Commons Attribution–NonCommercial (CC BY-NC); commercial BSD licence available from Printable Scenery  
**Official page**: https://www.printablescenery.com/2017/04/26/openlock-developer-information/  
**Template source used for this document**: OpenSCAD-OpenLock by Caitlyn Byrne  
&nbsp;&nbsp;https://github.com/caitlynb/OpenSCAD-OpenLock (OpenLock.scad, master branch)

---

## 1. Grid and Tile Footprint

OpenLOCK tiles are based on the **1-inch dungeon tile** standard, allowing a 25 mm miniature base to fit within one grid square.

| Variant | Unit width | Notes |
|---|---|---|
| Imperial (canonical) | **25.4 mm** (exactly 1 inch) | Official standard |
| Metric | **25.0 mm** | Widely used "close enough" variant |

A single 1×1 tile = 1 unit × 1 unit = 25.4 mm × 25.4 mm (imperial) or 25 mm × 25 mm (metric).  
Tiles can be any NxM multiple (Part A = 1×1, Part E = 2×2, Part R = 4×2, etc.).

### Tile Letter Codes (from OpenSCAD templates)

| Part | Size (units) | Clip positions |
|---|---|---|
| A | 0.5 × 2 | Wall section, clips on long sides |
| B | 0.5 × 2 | Wall section (alternate) |
| C | 0.5 × 2.5 | Wall corner |
| D | 0.5 × 3 | Wall extension |
| E | 2 × 2 | Standard 2×2 floor |
| F | 2 × 2 | Round/circular variant |
| G | 2.5 × 2.5 | Floor + circular recess |
| R | 4 × 2 | Long floor |
| S | 1 × 2 | Half floor |
| SA | 1 × 3 | Medium floor |
| SB | 1 × 4 | Long half floor |
| U | 4 × 4 | Large floor |
| V | 4 × 4 | Large round floor |

**Critical rule**: *"The footprint tessellation of each piece must never change, and the location of the OpenLOCK ports must never alter."* Tiles of the same letter code from any manufacturer must mate perfectly.

---

## 2. Wall Heights

| Height name | mm | Inches |
|---|---|---|
| Mini / floor | 7.0–9.2 | ~¼" |
| Default floor base | **8.0 mm** | — |
| Half-height wall | 25.4 mm | 1" |
| Full-height wall | **50.8 mm** | 2" |
| Tall wall | 76.2 mm | 3" |
| Extra-tall wall | 101.6 mm | 4" |

The **floor tile base** (the underside box with clip sockets) is **8.0 mm** tall by the OpenSCAD reference implementation (`WallHeight = 8`).

Some sources quote a total floor assembly of **9.2 mm** (7 mm base + 2.2 mm floor deck), which includes an additional floor-surface layer not part of the base itself.

---

## 3. Clip Socket Geometry

This is the most critical compatibility dimension. Clip sockets are cut into the side walls of the base box. The socket void is a stepped T-shape in plan view (looking down) and a rectangle in elevation.

### 3a. Elevation (height in Z)

```
Z = 0          ← top of base (terrain attachment face)
Z = -1.4       ← top edge of clip slot    (= -WALL_HEIGHT + CUTOUT_START_Z)
Z = -5.6       ← bottom edge of clip slot (= -1.4 - CUTOUT_HEIGHT)
Z = -8.0       ← bottom face of base      (= -WALL_HEIGHT)
```

| Parameter | Value (mm) | Description |
|---|---|---|
| `WALL_HEIGHT` | 8.0 | Total height of base box |
| `CUTOUT_START_Z` | 1.4 | Gap from base bottom to slot bottom edge |
| `CUTOUT_HEIGHT` | 4.2 | Slot height (Z span) |
| Slot centre Z | −3.7 | = −1.4 − 4.2/2 from top of base |

### 3b. Plan (shape looking down from above)

The slot is a stepped/tapered T-shape. Dimensions are measured inward from the tile edge face:

```
Edge face
│
│  ←—— 14mm (CUTOUT_WIDE_1) ——→    depth = 0 … 2mm
│    ←— 12mm (CUTOUT_WIDE_2) —→    depth = 2mm
│      ←- 10mm (CUTOUT_WIDE_3) -→  depth = 2 … 5mm
│      ←- 10mm ——————————————→    depth = 5 … 7mm  (safety relief)
```

| Parameter | Value (mm) | Description |
|---|---|---|
| `CUTOUT_WIDE_1` | 14 | Opening width at tile face |
| `CUTOUT_DEEP_1` | 2 | Depth at which width narrows from 14 → 12 |
| `CUTOUT_WIDE_2` | 12 | Intermediate step width |
| `CUTOUT_DEEP_2` | 2 | Depth step (same as DEEP_1; both = 2 mm) |
| `CUTOUT_WIDE_3` | 10 | Inner/deepest width |
| `CUTOUT_DEEP_3` | 5 | Depth of inner cavity |
| Total void depth | 7 | = CUTOUT_DEEP_3 + 2 (polygon relief) |

**OpenSCAD polygon** (cross-section, u = lateral, v = depth into tile):
```scad
polygon([
  [-7, -2], [-7,  2],   // left side of 14mm mouth, entry
  [-6,  2], [-5,  5],   // step to 12mm then 10mm
  [-5,  7], [ 5,  7],   // back wall at 10mm wide
  [ 5,  5], [ 6,  2],   // mirror
  [ 7,  2], [ 7, -2]    // right side of 14mm mouth
]);
```

### 3c. Clip Retention Side Cut

In addition to the main T-slot, a side cut is milled for clip retention:

```
translate([-8.35, 0, (h+z)/2-1])  cube([4.7, 18, h+z+2], center=true)
translate([-8.35, 0, (h+z+lh)/2]) cube([4.7, 14, h+z+lh], center=true)
```

Where `h = CUTOUT_HEIGHT`, `z = CUTOUT_START_Z`, `lh = layer_height`.  
This creates a 4.7 mm × 18 mm cutout at x offset −8.35 mm from the slot centre (to the left of the socket when viewed from outside), spanning the full clip slot height. This is what the clip's locking tab engages.

---

## 4. Clip Socket Positions

Clip sockets are centred on each 1-unit (25.4 mm) segment along each edge.

For a 1×1 floor tile (Part A equivalent): **one socket per edge, centred at 0.5 × unit_width = 12.7 mm from the corner**.

For an N-unit edge: N sockets, at positions `(i + 0.5) × unit_width` for `i = 0…N-1`.

**Position note**: The SCAD reference places clips at `unitwidth/4` for wall pieces (which are 0.5 units wide in that axis). For floor tiles the pattern is one clip per square, centred.

---

## 5. Tessellation Requirement

All tiles must use the same unit grid (all 25.4 mm or all 25 mm — do not mix). Tile edges must align exactly at:

- **Straight edges**: flush faces at multiples of the unit width
- **Corner cuts**: 45° chamfers at corner intersections where specified by letter code

---

## 6. Floor Tile Top Surface

The tile can be of any height above the base. The standard floor deck height is:

| Element | Height above base top (z = 0 reference) |
|---|---|
| Flat bare floor | 0 mm (base top = floor) |
| Textured floor | 0…2 mm above base top |
| Grass/terrain | 5–10 mm typical |
| Half walls | 25.4 mm |
| Full walls | 50.8 mm |

The tile top surface (terrain, floor texture, etc.) is entirely freeform. Only the base footprint and clip socket locations are constrained by the standard.

---

## 7. Compatibility Notes

### Metric vs. Imperial
Most published OpenLOCK content is designed for the 25.4 mm (1") imperial grid. Tiles designed to a 25 mm metric grid will **not tessellate** with standard 25.4 mm tiles; they form a parallel, incompatible ecosystem. Both variants exist in the community; choose one and be consistent.

### Clip Version
The OpenLOCK clip has evolved through several versions. **Clip 5.4** is current as of the OpenSCAD reference used here. Older clips (v1–v4) may require slightly different socket depths; Clip 5.4 sockets are backward-compatible with the 4.x clip series.

### Minimum Wall Thickness
The base box walls (between clip sockets) should be at least 2–3 mm thick to avoid delamination during printing. For a 25.4 mm square with a 14 mm wide socket, the remaining wall on each side is (25.4 − 14) / 2 = 5.7 mm — adequate.

---

## 8. Summary: Key Dimensions at a Glance

| Dimension | Value |
|---|---|
| Square size (imperial) | 25.4 mm |
| Square size (metric variant) | 25.0 mm |
| Base height | 8.0 mm |
| Clip slot Z: bottom edge | 1.4 mm from base bottom |
| Clip slot Z: top edge | 5.6 mm from base bottom |
| Clip slot Z: height | 4.2 mm |
| Clip slot entry width | 14 mm |
| Clip slot depth (inner) | 5 mm (+ 2 mm relief = 7 mm total) |
| Clip slot inner width | 10 mm |
| Clip retention cut width | 4.7 mm |
| Clip retention cut offset | 8.35 mm to one side of centre |
| Clips per edge segment | 1 per 1-unit (25.4 mm) square |
| Full-height wall | 50.8 mm |
| Half-height wall | 25.4 mm |

---

*Sources: OpenSCAD-OpenLock (github.com/caitlynb/OpenSCAD-OpenLock), Printable Scenery developer page (printablescenery.com), TerrainTinker knowledge base.*
