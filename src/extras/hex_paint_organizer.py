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
    wall: float = 4.0             # wall thickness (was 1.0; ≥4 needed for magnet pockets)
    height: float = 60.0          # cup height
    floor: float = 10.0           # bore floor height above bottom (was 5.0)
    base: float = 5.0             # solid base below depression (was 2.0)
    magnet_dia: float = 10.0      # magnet disc diameter
    magnet_depth: float = 3.0     # magnet disc thickness / bore depth
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


def single_cup(spec: HexOrganizerSpec) -> m3d.Manifold:
    outer_f2f = spec.bore_f2f + 2.0 * spec.wall

    outer = hex_prism(outer_f2f, spec.height)

    # Main bore: from z=floor to z=height (open top)
    bore = hex_prism(spec.bore_f2f, spec.height - spec.floor)
    bore = bore.translate((0.0, 0.0, spec.floor))

    # Retaining depression: from z=base to z=floor
    depression = hex_prism(spec.retaining_f2f, spec.floor - spec.base)
    depression = depression.translate((0.0, 0.0, spec.base))

    return outer - bore - depression


# ---------------------------------------------------------------------------
# Rectangular frame / cap helpers
# ---------------------------------------------------------------------------

def _rect_box(
    x0: float, x1: float,
    y0: float, y1: float,
    z0: float, z1: float,
) -> m3d.Manifold:
    """Solid axis-aligned rectangular prism."""
    box = m3d.Manifold.cube([x1 - x0, y1 - y0, z1 - z0])
    return box.translate([x0, y0, z0])


def _hex_bounds(spec: HexOrganizerSpec) -> tuple[float, float, float, float]:
    """Tight bounding box of the full hex cup assembly: (xmin, xmax, ymin, ymax)."""
    outer_f2f = spec.bore_f2f + 2.0 * spec.wall
    R_outer = outer_f2f / np.sqrt(3)
    col_pitch = spec.bore_f2f + spec.wall
    row_pitch = col_pitch * np.sqrt(3) / 2.0

    cx_list, cy_list = [], []
    for row in range(spec.rows):
        x_stagger = 0.0 if (row % 2) else (col_pitch / 2.0)
        for col in range(spec.cols):
            cx_list.append(col * col_pitch + x_stagger)
            cy_list.append(row * row_pitch)

    return (
        min(cx_list) - outer_f2f / 2.0,   # xmin (leftmost flat face)
        max(cx_list) + outer_f2f / 2.0,   # xmax (rightmost flat face)
        min(cy_list) - R_outer,            # ymin (bottommost vertex)
        max(cy_list) + R_outer,            # ymax (topmost vertex)
    )


# ---------------------------------------------------------------------------
# Magnet pocket subtraction
# ---------------------------------------------------------------------------

def _subtract_magnets(body: m3d.Manifold, spec: HexOrganizerSpec) -> m3d.Manifold:
    """Cut 12 magnet pockets from body: 8 side (horizontal) + 2 bottom + 2 top."""
    xmin, xmax, ymin, ymax = _hex_bounds(spec)
    fw = spec.wall  # frame wall matches cup wall
    fxmin, fxmax = xmin - fw, xmax + fw
    fymin, fymax = ymin - fw, ymax + fw
    frame_w = fxmax - fxmin
    frame_d = fymax - fymin

    z_side = spec.floor / 2.0          # vertical centre of side pockets = 5 mm
    cap_h = spec.magnet_depth + 1.0    # top cap height = 4 mm

    cyl = m3d.Manifold.cylinder(spec.magnet_depth, spec.magnet_dia / 2.0, circular_segments=32)

    # --- 8 side pockets (horizontal axis, 2 per face) ----------------------
    #
    # Rotation conventions (single-axis, degrees):
    #   rotate([-90, 0, 0])  →  +z axis points in +y  (south face, bore inward)
    #   rotate([ 90, 0, 0])  →  +z axis points in -y  (north face, bore inward)
    #   rotate([  0, 90, 0]) →  +z axis points in +x  (west face,  bore inward)
    #   rotate([  0,-90, 0]) →  +z axis points in -x  (east face,  bore inward)

    # South face  (y = fymin, bore in +y)
    cyl_py = cyl.rotate([-90, 0, 0])
    for frac in (1 / 3, 2 / 3):
        body -= cyl_py.translate([fxmin + frame_w * frac, fymin, z_side])

    # North face  (y = fymax, bore in -y)
    cyl_ny = cyl.rotate([90, 0, 0])
    for frac in (1 / 3, 2 / 3):
        body -= cyl_ny.translate([fxmin + frame_w * frac, fymax, z_side])

    # West face  (x = fxmin, bore in +x)
    cyl_px = cyl.rotate([0, 90, 0])
    for frac in (1 / 3, 2 / 3):
        body -= cyl_px.translate([fxmin, fymin + frame_d * frac, z_side])

    # East face  (x = fxmax, bore in -x)
    cyl_nx = cyl.rotate([0, -90, 0])
    for frac in (1 / 3, 2 / 3):
        body -= cyl_nx.translate([fxmax, fymin + frame_d * frac, z_side])

    # --- 2 bottom pockets (vertical, from z=0 upward) ----------------------
    # Centred in the south and north frame walls; x = midpoint of frame width.
    xc_bot = (fxmin + fxmax) / 2.0
    y_south_wall = (fymin + ymin) / 2.0   # midline of south 4 mm wall
    y_north_wall = (fymax + ymax) / 2.0   # midline of north 4 mm wall
    for yc in (y_south_wall, y_north_wall):
        body -= cyl.translate([xc_bot, yc, 0.0])

    # --- 2 top pockets (vertical, from z=height+cap_h downward) -----------
    # Same XY as bottom pockets; bored from the top of the cap downward.
    z_top = spec.height + cap_h - spec.magnet_depth   # = height + 1 mm
    for yc in (y_south_wall, y_north_wall):
        body -= cyl.translate([xc_bot, yc, z_top])

    return body


# ---------------------------------------------------------------------------
# Main build function
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

    return result


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main() -> None:
    spec = HexOrganizerSpec()

    outer_f2f = spec.bore_f2f + 2.0 * spec.wall
    R_outer = outer_f2f / np.sqrt(3)
    col_pitch = spec.bore_f2f + spec.wall
    row_pitch = col_pitch * np.sqrt(3) / 2.0
    xmin, xmax, ymin, ymax = _hex_bounds(spec)
    fw = spec.wall
    frame_w = (xmax - xmin) + 2 * fw
    frame_d = (ymax - ymin) + 2 * fw

    print(f"Building hex organizer  ({spec.cols}×{spec.rows} cups, open front/back)")
    print(f"  cup: outer F2F {outer_f2f:.1f} mm  col pitch {col_pitch:.1f} mm  row pitch {row_pitch:.2f} mm")
    print(f"  frame outer: {frame_w:.1f} × {frame_d:.1f} mm  height {spec.height:.0f} mm")

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
