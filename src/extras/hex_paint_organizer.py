#!/usr/bin/env python3
"""Generate a hexagonal craft-paint organizer STL.

3×4 honeycomb grid (default) of pointy-top hexagonal cups with left/right
side walls and an open front/back.
"""
from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

import manifold3d as m3d
import numpy as np
import trimesh


@dataclass(frozen=True)
class HexOrganizerSpec:
    bore_f2f: float = 35.0        # main bore flat-to-flat
    retaining_f2f: float = 29.0   # retaining-depression flat-to-flat
    wall: float = 1.0             # wall thickness
    height: float = 60.0          # cup height
    floor: float = 14.5           # bore floor / inner ledge height (magnet top edge z=11, bevel starts 0.5mm above)
    base: float = 1.0             # solid base below retaining recess
    magnet_dia: float = 10.0      # magnet disc diameter
    magnet_depth: float = 3.0     # magnet disc thickness / bore depth
    outer_wall_height: float = 40.0  # height of the outer hex shell (cups extend to spec.height)
    bottom_roundover: float = 1.0    # convex roundover radius on bottom perimeter edge
    vertical_roundover: float = 8.0  # convex outside vertical edge roundover radius
    cols: int = 3
    rows: int = 4


# ---------------------------------------------------------------------------
# Hex geometry helpers
# ---------------------------------------------------------------------------

def hex_polygon(f2f: float) -> list[tuple[float, float]]:
    """CCW pointy-top regular hexagon vertices, centred at origin.

    Pointy-top: flat sides face left/right; vertices at top/bottom.
    Width (x, flat-to-flat) = f2f.  Height (y, point-to-point) = f2f * 2/√3.
    """
    R = f2f / np.sqrt(3)  # circumradius
    return [
        (float(R * np.cos(np.radians(a))), float(R * np.sin(np.radians(a))))
        for a in range(30, 390, 60)  # 30, 90, 150, 210, 270, 330
    ]


def hex_prism(f2f: float, height: float) -> m3d.Manifold:
    cs = m3d.CrossSection([hex_polygon(f2f)])
    return m3d.Manifold.extrude(cs, height=height)


def _rounded_hex_cs(f2f: float, r: float) -> m3d.CrossSection:
    """Hex cross-section with corners rounded inward by radius r.

    Double-offset: contract with Miter (sharpens corners), expand with Round
    (turns the sharp tips into arcs).  Corner apexes end up at
    approximately (f2f/√3 - delta) from centre, keeping wall thickness
    consistent with the outer vertical roundover.
    """
    cs = m3d.CrossSection([hex_polygon(f2f)])
    cs = cs.offset(-r, m3d.JoinType.Miter, miter_limit=10.0)
    return cs.offset(r, m3d.JoinType.Round, circular_segments=32)


def single_cup(spec: HexOrganizerSpec) -> m3d.Manifold:
    outer_f2f = spec.bore_f2f + 2.0 * spec.wall
    R_outer = outer_f2f / np.sqrt(3)

    outer = hex_prism(outer_f2f, spec.outer_wall_height)

    # Inner bore and retaining depression corner roundover radius:
    # bore_r = outer_roundover - (R_outer - R_inner)  keeps wall ~= spec.wall at corners.
    bore_r = max(0.0, spec.vertical_roundover - (R_outer - spec.bore_f2f / np.sqrt(3)))
    ret_r  = max(0.0, spec.vertical_roundover - (R_outer - spec.retaining_f2f / np.sqrt(3)))

    # Main bore: from z=floor to z=height (open top)
    if bore_r > 0.0:
        bore = m3d.Manifold.extrude(_rounded_hex_cs(spec.bore_f2f, bore_r),
                                    spec.height - spec.floor)
    else:
        bore = hex_prism(spec.bore_f2f, spec.height - spec.floor)
    bore = bore.translate((0.0, 0.0, spec.floor))

    # Retaining depression: from z=base to z=floor
    if ret_r > 0.0:
        depression = m3d.Manifold.extrude(_rounded_hex_cs(spec.retaining_f2f, ret_r),
                                          spec.floor - spec.base)
    else:
        depression = hex_prism(spec.retaining_f2f, spec.floor - spec.base)
    depression = depression.translate((0.0, 0.0, spec.base))

    # 45° bevel at the top of the retaining ring: height = apothem drop so the
    # chamfer angle is 45° and paint tubes slide in without catching.
    bevel_h = min((spec.bore_f2f - spec.retaining_f2f) / 2.0, spec.floor - spec.base - 0.1)
    bevel_cs = m3d.CrossSection([hex_polygon(spec.retaining_f2f)])
    bevel_scale = spec.bore_f2f / spec.retaining_f2f
    entry_bevel = m3d.Manifold.extrude(bevel_cs, bevel_h, scale_top=(bevel_scale, bevel_scale))
    entry_bevel = entry_bevel.translate((0.0, 0.0, spec.floor - bevel_h))

    return outer - bore - depression - entry_bevel


# ---------------------------------------------------------------------------
# Magnet pocket subtraction
# ---------------------------------------------------------------------------
#
# Magnets sit in the exposed perimeter faces of the honeycomb itself (no added
# frame / cap).  Geometry is described in CODE coordinates (pointy-top hexes:
# flat faces face ±x, vertices point ±y) but the spec was given in the USER's
# view, which is this part rotated +90° CCW so that the hex points are
# horizontal and a code-row of `cols` cups reads as a vertical column.
#
# Edge-normal directions (code degrees), per pointy-top hex:
#     0   = right flat   (+x)  → user TOP flat
#   180   = left flat    (−x)  → user BOTTOM flat
#    60   = upper-right diagonal
#   120   = upper-left  diagonal
#   240   = lower-left  diagonal
#   300   = lower-right diagonal
#
# Within a column, cups read top→bottom in the user view as c2 → c1 → c0
# (higher cx is higher up).  Side faces are numbered 0..5 top→bottom; each cup
# contributes two diagonal faces to a side.

MAGNET_Z = 6.0            # pocket centre height: 1 mm clearance below a 10 mm disc
MAGNET_OVERSHOOT = 0.6    # start pocket this far outside the face for a clean cut


def _face_normal(deg: float) -> tuple[float, float]:
    return (float(np.cos(np.radians(deg))), float(np.sin(np.radians(deg))))


def _magnet_pocket(
    cx: float, cy: float, deg: float, spec: HexOrganizerSpec,
    *, tangent_offset: float = 0.0,
) -> m3d.Manifold:
    """A horizontal cylindrical pocket cut into the face at edge-normal `deg`.

    The cup centre is (cx, cy); the face outer surface lies one apothem out
    along the normal.  `tangent_offset` slides the pocket along the face
    (used to place two magnets on a single flat face).
    """
    apothem = (spec.bore_f2f + 2.0 * spec.wall) / 2.0
    nx, ny = _face_normal(deg)
    tx, ty = -ny, nx  # in-plane tangent (perpendicular to normal)

    # Outer face midpoint, slid along the tangent, started just outside the face.
    fx = cx + apothem * nx + tangent_offset * tx + MAGNET_OVERSHOOT * nx
    fy = cy + apothem * ny + tangent_offset * ty + MAGNET_OVERSHOOT * ny

    length = spec.magnet_depth + MAGNET_OVERSHOOT
    cyl = m3d.Manifold.cylinder(length, spec.magnet_dia / 2.0, circular_segments=64)
    cyl = cyl.rotate([0, 90, 0])        # +z axis → +x
    cyl = cyl.rotate([0, 0, deg + 180]) # +x → inward (−normal), boring into the wall
    return cyl.translate([fx, fy, MAGNET_Z])


def _subtract_magnets(body: m3d.Manifold, spec: HexOrganizerSpec) -> m3d.Manifold:
    """Cut 8 magnet pockets into the honeycomb's exposed perimeter faces.

    4 flat (1 per corner cup: user-row 1 and 3, left and right ends)
    + 4 side (high column faces 1 & 4, low column faces 0 & 3).

    User-view rows: row 1 (top) = code col=cols-1, row 3 (bottom) = code col=0.
    Left/right ends = code row=rows-1 (leftmost) and code row=0 (rightmost).
    """
    col_pitch = spec.bore_f2f + spec.wall
    row_pitch = col_pitch * np.sqrt(3) / 2.0
    top_col = spec.cols - 1   # highest cx → top of user-view column
    bot_col = 0               # lowest cx → bottom of user-view column

    def centre(row: int, col: int) -> tuple[float, float]:
        x_stagger = 0.0 if (row % 2) else (col_pitch / 2.0)
        return (col * col_pitch + x_stagger, row * row_pitch)

    # --- flat magnets: 1 centered magnet on each corner cup's outward flat face.
    # Row 1 (top, code col=top_col): face 0° (+x).  Row 3 (bottom, code col=0): face 180° (−x).
    # Code rows 0 and 2 (0-based) = 1st and 3rd of 4 rows.
    for end_row in (0, spec.rows - 2):
        cx, cy = centre(end_row, top_col)
        body -= _magnet_pocket(cx, cy, 0.0, spec)
        cx, cy = centre(end_row, bot_col)
        body -= _magnet_pocket(cx, cy, 180.0, spec)

    # --- side magnets on the two perimeter columns ------------------------
    # Rightmost code-row (row 0, smallest cy) = HIGH column → faces 1 & 4.
    #   face 1 = top cup's lower-right diagonal (240°)
    #   face 4 = bottom cup's upper-right diagonal (300°)
    high_row = 0
    cx, cy = centre(high_row, top_col)
    body -= _magnet_pocket(cx, cy, 240.0, spec)
    cx, cy = centre(high_row, bot_col)
    body -= _magnet_pocket(cx, cy, 300.0, spec)

    # Leftmost code-row (largest cy) = LOW column → faces 0 & 3.
    #   face 0 = top cup's upper-left diagonal (60°)
    #   face 3 = middle cup's lower-left diagonal (120°)
    low_row = spec.rows - 1
    mid_col = spec.cols // 2
    cx, cy = centre(low_row, top_col)
    body -= _magnet_pocket(cx, cy, 60.0, spec)
    cx, cy = centre(low_row, mid_col)
    body -= _magnet_pocket(cx, cy, 120.0, spec)

    return body


# ---------------------------------------------------------------------------
# Main build function
# ---------------------------------------------------------------------------
# Vertical convex edge roundover
# ---------------------------------------------------------------------------

def _vertical_roundover(body: m3d.Manifold, spec: HexOrganizerSpec, r: float) -> m3d.Manifold:
    """Round all convex outside vertical edges with radius r; leave concave corners square.

    Uses the double-offset technique: contract inward with Miter (sharpens convex
    corners, leaves concave corners alone), then expand outward with Round (turns
    the sharp convex points into arcs). The extruded result is intersected with
    the body to cut the corners without touching any interior feature.
    """
    # Slice below magnet pockets for a clean outer cross-section (no bore cutouts)
    slice_z = max(0.1, MAGNET_Z - spec.magnet_dia / 2.0 - 0.5)
    cs = body.slice(slice_z)
    cs_in = cs.offset(-r, m3d.JoinType.Miter, miter_limit=10.0)
    cs_round = cs_in.offset(r, m3d.JoinType.Round, circular_segments=32)
    h = body.bounding_box()[5]
    return body ^ m3d.Manifold.extrude(cs_round, h)


# ---------------------------------------------------------------------------
# Bottom perimeter roundover
# ---------------------------------------------------------------------------

def _bottom_roundover(body: m3d.Manifold, spec: HexOrganizerSpec, steps: int = 10) -> m3d.Manifold:
    """Convex roundover on the bottom perimeter edge, radius spec.bottom_roundover.

    Slices at a z safely below any features (retaining recess, magnet pockets)
    to get a clean outer cross-section for the template.
    """
    r = spec.bottom_roundover
    # Stay below the retaining recess (starts at z=base) and magnet pockets
    # (start at z = MAGNET_Z - magnet_dia/2); take 90% of whichever is lower.
    safe_z = min(spec.base, MAGNET_Z - spec.magnet_dia / 2.0) * 0.9
    safe_z = max(0.01, safe_z)
    cs = body.slice(safe_z)
    h = body.bounding_box()[5]  # bounding_box() → (xmin,ymin,zmin, xmax,ymax,zmax)
    template = m3d.Manifold.extrude(cs, h - r).translate([0.0, 0.0, r])
    for i in range(steps):
        z0 = r * i / steps
        z1 = r * (i + 1) / steps
        z_mid = (z0 + z1) * 0.5
        inset = r - np.sqrt(r * r - (r - z_mid) ** 2)
        cs_z = cs.offset(-inset, m3d.JoinType.Round, circular_segments=16)
        template += m3d.Manifold.extrude(cs_z, z1 - z0).translate([0.0, 0.0, z0])
    return body ^ template


# ---------------------------------------------------------------------------

def build_organizer(spec: HexOrganizerSpec) -> m3d.Manifold:
    col_pitch = spec.bore_f2f + spec.wall
    row_pitch = col_pitch * np.sqrt(3) / 2.0

    # Honeycomb cup union
    cup = single_cup(spec)
    cups = []
    for row in range(spec.rows):
        x_stagger = 0.0 if (row % 2) else (col_pitch / 2.0)
        for col in range(spec.cols):
            cups.append(cup.translate((col * col_pitch + x_stagger, row * row_pitch, 0.0)))
    result = cups[0]
    for c in cups[1:]:
        result = result + c

    result = _subtract_magnets(result, spec)
    if spec.vertical_roundover > 0.0:
        result = _vertical_roundover(result, spec, spec.vertical_roundover)
    if spec.bottom_roundover > 0.0:
        result = _bottom_roundover(result, spec)
    return result


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main() -> None:
    spec = HexOrganizerSpec()

    outer_f2f = spec.bore_f2f + 2.0 * spec.wall
    col_pitch = spec.bore_f2f + spec.wall
    row_pitch = col_pitch * np.sqrt(3) / 2.0

    print(f"Building hex organizer  ({spec.cols}×{spec.rows} cups, open front/back)")
    print(f"  cup: outer F2F {outer_f2f:.1f} mm  col pitch {col_pitch:.1f} mm  row pitch {row_pitch:.2f} mm")
    print(f"  magnets: {spec.magnet_dia:.0f}×{spec.magnet_depth:.0f} mm, z={MAGNET_Z:.0f} mm  (4 flat corners + 4 side diagonals = 8 total)")

    manifold = build_organizer(spec)
    raw = manifold.to_mesh()
    mesh = trimesh.Trimesh(
        vertices=np.array(raw.vert_properties, dtype=float)[:, :3],
        faces=np.array(raw.tri_verts, dtype=int),
        process=False,
    )
    mesh.fix_normals()
    if mesh.volume < 0:
        mesh.invert()

    out_path = Path("stl/extras/hex_paint_organizer.stl")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    mesh.export(str(out_path))

    print(f"Saved → {out_path}")
    print(f"Vertices: {len(mesh.vertices):,}  Faces: {len(mesh.faces):,}")
    print(f"Watertight: {mesh.is_watertight}")
    print(f"Volume: {mesh.volume:.1f} mm³")


if __name__ == "__main__":
    main()
