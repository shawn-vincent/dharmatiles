#!/usr/bin/env python3
"""Generate a hexagonal craft-paint organizer STL.

4×3 honeycomb grid (default) — 4 columns of 3 flat-top hexagonal cups (hex
points face left/right, flats face up/down).  Open front/back; magnets are
recessed into the exposed perimeter faces.  The code coordinate frame is the
intended viewing frame: +x = column direction (4 across), +y = row direction
(3 tall).
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass
from pathlib import Path

import manifold3d as m3d
import numpy as np
import trimesh

from dharmatiles.core.logo import make_logo_manifold


@dataclass(frozen=True)
class HexOrganizerSpec:
    bore_f2f: float = 35.0        # main bore flat-to-flat
    retaining_f2f: float = 29.0   # retaining-depression flat-to-flat
    wall: float = 1.0             # perimeter wall thickness
    interior_wall: float = 2.0    # shared wall thickness between adjacent cups
    height: float = 60.0          # cup height
    floor: float = 14.5           # bore floor / inner ledge height (magnet top edge z=11, bevel starts 0.5mm above)
    base: float = 1.0             # solid base below retaining recess
    magnet_dia: float = 10.0      # magnet disc diameter
    magnet_depth: float = 3.0     # magnet disc thickness / bore depth
    bottom_roundover: float = 1.0    # convex roundover radius on bottom perimeter edge
    vertical_roundover: float = 8.0  # convex outside vertical edge roundover radius
    cols: int = 4                    # columns across (x)
    rows: int = 3                    # cups per column (y)

    def __post_init__(self) -> None:
        if self.interior_wall > 2.0 * self.wall:
            raise ValueError(
                "interior_wall cannot exceed 2 * wall; adjacent cup shells would separate"
            )


# ---------------------------------------------------------------------------
# Hex geometry helpers
# ---------------------------------------------------------------------------

def hex_polygon(f2f: float) -> list[tuple[float, float]]:
    """CCW flat-top regular hexagon vertices, centred at origin.

    Flat-top: vertices point left/right; flat sides face up/down.
    Height (y, flat-to-flat) = f2f.  Width (x, point-to-point) = f2f * 2/√3.
    """
    R = f2f / np.sqrt(3)  # circumradius
    return [
        (float(R * np.cos(np.radians(a))), float(R * np.sin(np.radians(a))))
        for a in range(0, 360, 60)  # 0, 60, 120, 180, 240, 300
    ]


def hex_prism(f2f: float, height: float) -> m3d.Manifold:
    cs = m3d.CrossSection([hex_polygon(f2f)])
    return m3d.Manifold.extrude(cs, height=height)


BOOLEAN_OVERLAP = 0.02  # mm; prevents tangent-cell cracks in the honeycomb union


def hex_cross_section(f2f: float) -> m3d.CrossSection:
    return m3d.CrossSection([hex_polygon(f2f)])


# ---------------------------------------------------------------------------
# Honeycomb layout
# ---------------------------------------------------------------------------
#
# Flat-top hex packing for a 4-columns-of-3 grid:
#   - cups in a column share horizontal flat walls  → vertical pitch = f2f + interior_wall
#   - adjacent columns interlock at half height     → horizontal pitch = pitch_y·√3/2
#   - odd columns sit half a cup higher than even ones, so the offset reads
#     low, high, low, high from left to right

def _pitches(spec: HexOrganizerSpec) -> tuple[float, float]:
    pitch_y = spec.bore_f2f + spec.interior_wall  # within a column (shared flat walls)
    pitch_x = pitch_y * np.sqrt(3) / 2.0      # between columns (offset packing)
    return pitch_x, pitch_y


def cup_centre(spec: HexOrganizerSpec, col: int, row: int) -> tuple[float, float]:
    """Centre (x, y) of the cup at grid position (col, row).

    col increases left→right (0..cols-1); row increases bottom→top (0..rows-1).
    """
    pitch_x, pitch_y = _pitches(spec)
    y_stagger = pitch_y / 2.0 if (col % 2 == 1) else 0.0
    return (col * pitch_x, row * pitch_y + y_stagger)


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


def honeycomb_footprint_cs(spec: HexOrganizerSpec) -> m3d.CrossSection:
    """One clean outer footprint for the organizer's full honeycomb body.

    The construction cells intentionally overlap by a tiny amount. Without that,
    cells with a 2 mm shared-wall pitch and 1 mm perimeter shells only touch
    tangentially, which can leave visible cracks or notches after booleans.
    """
    construction_f2f = spec.bore_f2f + 2.0 * spec.wall + BOOLEAN_OVERLAP
    cells = []
    for col in range(spec.cols):
        for row in range(spec.rows):
            cx, cy = cup_centre(spec, col, row)
            cells.append(hex_cross_section(construction_f2f).translate((cx, cy)))
    return m3d.CrossSection.batch_boolean(cells, m3d.OpType.Add)


def _cup_cutters(spec: HexOrganizerSpec) -> tuple[m3d.Manifold, m3d.Manifold, m3d.Manifold]:
    outer_f2f = spec.bore_f2f + 2.0 * spec.wall
    R_outer = outer_f2f / np.sqrt(3)
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

    return bore, depression, entry_bevel


def _subtract_cup_cutters(body: m3d.Manifold, spec: HexOrganizerSpec) -> m3d.Manifold:
    bore, depression, entry_bevel = _cup_cutters(spec)
    for col in range(spec.cols):
        for row in range(spec.rows):
            cx, cy = cup_centre(spec, col, row)
            body -= bore.translate((cx, cy, 0.0))
            body -= depression.translate((cx, cy, 0.0))
            body -= entry_bevel.translate((cx, cy, 0.0))
    return body


# ---------------------------------------------------------------------------
# Magnet pocket subtraction
# ---------------------------------------------------------------------------
#
# Magnets sit in the exposed perimeter faces of the honeycomb itself (no added
# frame / cap).  Each flat-top cup has six faces, named by their outward-normal
# direction in degrees:
#
#     90  = top flat      (+y)
#    270  = bottom flat    (−y)
#     30  = upper-right diagonal
#    150  = upper-left  diagonal
#    210  = lower-left  diagonal
#    330  = lower-right diagonal
#
# Eight magnets are placed on the perimeter (see _subtract_magnets); they are
# asymmetric by design so that two organizers only mate in one orientation.

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

    Placement (col, row, face-normal degrees) — asymmetric by design:

        (0,0) → bottom flat (270°) + upper-left  diagonal (150°)
        (0,2) → top flat (90°)     + lower-left  diagonal (210°)
        (2,0) → bottom flat (270°)
        (2,2) → top flat (90°)
        (3,0) → lower-right diagonal (330°)
        (3,1) → upper-right diagonal (30°)

    Cup centres come from cup_centre().
    """
    # (col, row, face-normal degrees)
    placements = [
        (0, 0, 270.0),   # bottom flat
        (0, 0, 150.0),   # upper-left (top-left slope)
        (0, 2,  90.0),   # top flat
        (0, 2, 210.0),   # lower-left  (bottom-left slope)
        (2, 0, 270.0),   # bottom flat
        (2, 2,  90.0),   # top flat
        (3, 0, 330.0),   # lower-right (bottom-right slope)
        (3, 1,  30.0),   # upper-right (top-right slope)
    ]
    for col, row, deg in placements:
        if col >= spec.cols or row >= spec.rows:
            continue
        cx, cy = cup_centre(spec, col, row)
        body -= _magnet_pocket(cx, cy, deg, spec)

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

CUP_LOGO_SIZE_MM = 20.0   # logo bounding square in each cup floor; fits inside 29 mm retaining hex


def _stamp_logos(body: m3d.Manifold, spec: HexOrganizerSpec) -> m3d.Manifold:
    """Stamp the dharmatiles lotus into each cup floor and into the back centre.

    Each cup gets a CUP_LOGO_SIZE_MM logo recessed into the retaining-recess
    floor (visible looking into the cup, cut downward from z=spec.base).  The
    back face (z=0) gets a double-sized logo at the assembly's XY centre, cut
    upward from below.  Both stamps go 1/4 of the way through the spec.base
    slab.
    """
    depth     = spec.base / 4.0
    back_size = CUP_LOGO_SIZE_MM * 2.0

    for col in range(spec.cols):
        for row in range(spec.rows):
            cx, cy = cup_centre(spec, col, row)
            body -= make_logo_manifold(cx, cy, CUP_LOGO_SIZE_MM,
                                       z_base=spec.base - depth,
                                       depth_mm=depth)

    pitch_x, pitch_y = _pitches(spec)
    top = spec.rows - 1
    last_stagger = pitch_y / 2.0 if (spec.cols - 1) % 2 == 1 else 0.0
    center_x = (spec.cols - 1) * pitch_x / 2.0
    center_y = (top * pitch_y + last_stagger) / 2.0
    body -= make_logo_manifold(center_x, center_y, back_size,
                               z_base=0.0, depth_mm=depth)

    return body


def _mark_origin_cup(body: m3d.Manifold, spec: HexOrganizerSpec) -> m3d.Manifold:
    """Shallow circular dent in the floor of the (col=0, row=0) cup.

    Orientation-check marker: confirms the (0,0) cup sits at the expected
    bottom-left corner of the model.  Cut a ~5 mm shallow disc into the
    retaining-recess floor (top of the solid base at z=spec.base).
    """
    cx, cy = cup_centre(spec, 0, 0)
    depth, radius = 0.5, 5.0
    dent = m3d.Manifold.cylinder(depth + 0.2, radius, circular_segments=48)
    dent = dent.translate([cx, cy, spec.base - depth])
    return body - dent


def _build_hollow(spec: HexOrganizerSpec) -> m3d.Manifold:
    """Hollow tube mode (height < 50): union solid outer shells, apply
    roundovers to the solid union, then subtract bores.  Roundovers must run
    on the solid honeycomb — the 8 mm inward offset collapses thin ring
    cross-sections, which is what you get if bores are cut first."""
    outer_f2f = spec.bore_f2f + 2.0 * spec.wall
    R_outer = outer_f2f / np.sqrt(3)
    bore_r = max(0.0, spec.vertical_roundover - (R_outer - spec.bore_f2f / np.sqrt(3)))

    result = m3d.Manifold.extrude(honeycomb_footprint_cs(spec), spec.height)

    if spec.vertical_roundover > 0.0:
        result = _vertical_roundover(result, spec, spec.vertical_roundover)
    if spec.bottom_roundover > 0.0:
        result = _bottom_roundover(result, spec)

    if bore_r > 0.0:
        bore = m3d.Manifold.extrude(_rounded_hex_cs(spec.bore_f2f, bore_r), spec.height)
    else:
        bore = hex_prism(spec.bore_f2f, spec.height)
    for c in range(spec.cols):
        for r in range(spec.rows):
            cx, cy = cup_centre(spec, c, r)
            result -= bore.translate((cx, cy, 0.0))

    return result


def build_organizer(spec: HexOrganizerSpec) -> m3d.Manifold:
    if spec.height < 50:
        return _build_hollow(spec)

    result = m3d.Manifold.extrude(honeycomb_footprint_cs(spec), spec.height)
    result = _subtract_cup_cutters(result, spec)

    result = _subtract_magnets(result, spec)
    if spec.vertical_roundover > 0.0:
        result = _vertical_roundover(result, spec, spec.vertical_roundover)
    if spec.bottom_roundover > 0.0:
        result = _bottom_roundover(result, spec)
    # Cut the logo stamps and orientation marker last: they sit inside the
    # base slab, and the roundovers slice the body at low z to build their
    # templates — interior holes there would get swept through the model.
    result = _stamp_logos(result, spec)
    result = _mark_origin_cup(result, spec)
    return result


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main() -> None:
    parser = argparse.ArgumentParser(description="Generate a hexagonal craft-paint organizer STL.")
    parser.add_argument("--cols", type=int, default=4, help="Number of columns (default: 4)")
    parser.add_argument("--rows", type=int, default=3, help="Number of rows (default: 3)")
    parser.add_argument("--height", type=float, default=60.0, help="Overall cup height in mm (default: 60.0)")
    args = parser.parse_args()

    spec = HexOrganizerSpec(cols=args.cols, rows=args.rows, height=args.height)

    outer_f2f = spec.bore_f2f + 2.0 * spec.wall
    pitch_x, pitch_y = _pitches(spec)

    print(f"Building hex organizer  ({spec.cols} columns of {spec.rows} cups, open front/back)")
    print(
        f"  cup: outer F2F {outer_f2f:.1f} mm  perimeter wall {spec.wall:.1f} mm  "
        f"interior wall {spec.interior_wall:.1f} mm"
    )
    print(f"  layout: col pitch {pitch_x:.2f} mm  row pitch {pitch_y:.1f} mm")
    print(f"  magnets: {spec.magnet_dia:.0f}×{spec.magnet_depth:.0f} mm, z={MAGNET_Z:.0f} mm  (4 flat + 4 diagonal = 8 total)")
    print("  marker: orientation dent in the (0,0) cup floor (bottom-left)")

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
