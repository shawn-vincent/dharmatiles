# Hex Paint Organizer — Magnet Interconnect Design

## Goal

Add recessed 10 × 3 mm disc-magnet pockets so multiple copies of the organizer
can be clipped together edge-to-edge on a desktop (and optionally stacked
vertically). Magnets are embedded **directly in the hexagon walls** — no added
frame, cap, or other structure. The cup's solid lower region (base + retaining
ring) provides the bulk that backs each pocket.

---

## Orientation & Terminology

The geometry is built in **code coordinates**: pointy-top hexes whose flat faces
face ±x and whose vertices point ±y. A code-row of `cols` cups is staggered in x;
rows stack in y.

The magnet spec, however, was given in the **user's view** — the part rotated
**+90° CCW** — so that:

- hex points are **horizontal**,
- each code-row of `cols` cups reads as a **vertical column** (the default is
  3 cups tall), and
- adjacent columns alternate **offset up / offset down** (the honeycomb stagger).

| User-view feature | Code equivalent |
|---|---|
| Column (3 cups tall) | one code-row, cups `c0…c{cols-1}` |
| Top cup of a column | highest cx → `c{cols-1}` |
| Bottom cup of a column | lowest cx → `c0` |
| Flat **top** face | right flat, normal **0° (+x)** |
| Flat **bottom** face | left flat, normal **180° (−x)** |
| Right-side diagonals (upper, lower) | normals **300°, 240°** |
| Left-side diagonals (upper, lower) | normals **60°, 120°** |
| **High** column (offset up) | even code-row (here row 0, rightmost) |
| **Low** column (offset down) | odd code-row (here row 3, leftmost) |

Hex edge-normal directions (code degrees): `0` right flat, `60` upper-right,
`120` upper-left, `180` left flat, `240` lower-left, `300` lower-right.

---

## Cup Parameters (unchanged honeycomb)

| Field | Value | Note |
|---|---|---|
| `bore_f2f` | 35.0 mm | main bore flat-to-flat |
| `retaining_f2f` | 29.0 mm | retaining-depression flat-to-flat |
| `wall` | 4.0 mm | gives ≥1 mm backing behind a 3 mm pocket in the bore wall |
| `height` | 60.0 mm | |
| `floor` | 10.0 mm | bore floor above the depression |
| `base` | 5.0 mm | solid base below the depression |
| `magnet_dia` | 10.0 mm | N52 disc magnet diameter |
| `magnet_depth` | 3.0 mm | N52 disc thickness / pocket bore depth |

Derived (3×4 default): `outer_f2f` = 43.0 mm, circumradius `R` = 24.83 mm,
`col_pitch` = 39.0 mm (cups overlap by `wall`, merging into shared walls),
`row_pitch` = 33.78 mm.

---

## Pocket Geometry (all magnets)

Every magnet is a **horizontal cylindrical pocket**, axis along the outward face
normal, bored into the wall from the exposed outer surface:

- Diameter 10 mm, depth 3 mm.
- Centre height **z = 6 mm** — i.e. **1 mm clearance** below the 10 mm disc
  (disc spans z = 1 → 11 mm). This keeps the magnet in the cup's bulk lower
  region: z = 0–5 solid base, z = 5–10 retaining ring (≈7 mm of radial backing),
  with only the top ~1 mm reaching the thinner bore wall.
- The cutter starts 0.6 mm outside the face for a clean boolean break; the
  effective recess depth into the material is exactly 3 mm.

Implementation: `_magnet_pocket(cx, cy, deg, spec, tangent_offset=…)` builds the
cylinder along +z, rotates `+z → +x`, then rotates about z by `deg + 180` so the
axis bores **inward** (−normal). `tangent_offset` slides it along the face for the
paired flat-face magnets.

---

## Magnet Catalogue (20 pockets total)

### Flat top / bottom — 16 pockets (2 per face)

Magnets sit only on the **first and third rows** of each column (top and bottom
cups); the middle row is skipped.

- **Top-flat** (normal 0°): 2 magnets on the +x flat of every top cup
  (`c{cols-1}` of each row) → 4 faces × 2 = **8**.
- **Bottom-flat** (normal 180°): 2 magnets on the −x flat of every bottom cup
  (`c0` of each row) → 4 faces × 2 = **8**.

The pair straddles the face centre by `±R/4` (≈ ±6.2 mm), keeping both 10 mm
discs within the ~24.8 mm face width. These enable vertical stacking
(top faces of one copy mate with bottom faces of the copy above).

### Side diagonals — 4 pockets

Side faces are numbered **0…5 top→bottom** along one side of a column (each cup
contributes two diagonal faces). Magnets go on the two perimeter columns only:

- **High column** (row 0, rightmost, offset up) → **faces 1 & 4**
  - face 1 = top cup's lower-right diagonal — normal **240°**
  - face 4 = bottom cup's upper-right diagonal — normal **300°**
- **Low column** (row 3, leftmost, offset down) → **faces 0 & 3**
  - face 0 = top cup's upper-left diagonal — normal **60°**
  - face 3 = middle cup's lower-left diagonal — normal **120°**

This offset pairing is what lets a high column's right edge mesh with the
neighbouring copy's low column left edge: high-side face 1 meets low-side face 0,
and high-side face 4 meets low-side face 3, with opposing (attracting) normals.

---

## Fit Checks

| Pocket location | Bore depth | Radial backing | Margin |
|---|---|---|---|
| Flat / diagonal face at z 5–10 (retaining ring) | 3 mm | ≈7 mm | ✓ |
| Same face at z 10–11 (bore wall) | 3 mm | 1 mm (wall − depth) | ✓ |
| Below disc | — | 1 mm to z = 0 | ✓ |

---

## Module Connectivity

Insert a 10 × 3 mm neodymium disc into each pocket (press-fit or glue flush).
Modules clip together along the perimeter side faces (horizontal tiling) or stack
vertically via the top/bottom flat-face magnets.

**Polarity rule:** magnets on opposing faces must attract. Because a high column
meets a low column when tiling, verify orientation against the face-pairing above
before gluing.

---

## Implementation Notes

- `_subtract_magnets(body, spec)` cuts all 20 pockets after the honeycomb union;
  it is the only magnet entry point and is called at the end of
  `build_organizer`.
- The previous rectangular-frame / top-cap design (and helpers `_rect_box`,
  `_hex_bounds`) has been removed — the magnets now live in the hex walls
  themselves.
- Constants `MAGNET_Z` (6 mm) and `MAGNET_OVERSHOOT` (0.6 mm) are module-level in
  `src/extras/hex_paint_organizer.py`.
</content>
</invoke>
