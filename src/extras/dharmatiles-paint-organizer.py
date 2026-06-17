#!/usr/bin/env python3
"""Generate a Dharmatiles paint organizer STL.

4×3 honeycomb grid (default) — 4 columns of 3 flat-top hexagonal cups (hex
points face left/right, flats face up/down).  Open front/back; magnets are
recessed into the exposed perimeter faces.  The code coordinate frame is the
intended viewing frame: +x = column direction (4 across), +y = row direction
(3 tall).
"""
from __future__ import annotations

import argparse
import contextlib
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Generator

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
    magnet_clearance: float = 0.1 # added to magnet pocket radius for press-fit clearance
    magnet_bevel: float = 0.4     # 45° chamfer at magnet hole entry (0 = disabled)
    magnet_pushout: bool = True   # 1 mm push-out hole through the back wall of each magnet pocket
    bottom_roundover: float = 1.0    # convex roundover radius on bottom perimeter edge
    vertical_roundover: float = 8.0  # convex outside vertical edge roundover radius
    cols: int = 4                    # columns across (x)
    rows: int = 3                    # cups per column (y)
    tolerance: float = 0.3          # added to bore/retaining diameters for clearance
    side_cutout_width: float = 0.4  # rectangular cutout per hex face: fraction of face length (0 = disabled)

    def __post_init__(self) -> None:
        if not 0.0 <= self.side_cutout_width <= 1.0:
            raise ValueError("side_cutout_width must be between 0 and 1")
        if self.interior_wall > 2.0 * self.wall:
            raise ValueError(
                "interior_wall cannot exceed 2 * wall; adjacent cup shells would separate"
            )
        if self.tolerance >= self.wall:
            raise ValueError(
                "tolerance must be less than wall thickness; bore would exceed outer shell"
            )
        if self.magnet_bevel < 0.0:
            raise ValueError("magnet_bevel must be non-negative")
        if self.magnet_bevel >= self.magnet_depth:
            raise ValueError("magnet_bevel must be less than magnet_depth")


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
    bore_f2f = spec.bore_f2f + 2.0 * spec.tolerance
    ret_f2f  = spec.retaining_f2f + 2.0 * spec.tolerance
    # Inner bore and retaining depression corner roundover radius:
    # bore_r = outer_roundover - (R_outer - R_inner)  keeps wall ~= spec.wall at corners.
    bore_r = max(0.0, spec.vertical_roundover - (R_outer - bore_f2f / np.sqrt(3)))
    ret_r  = max(0.0, spec.vertical_roundover - (R_outer - ret_f2f / np.sqrt(3)))

    # Main bore: from z=floor to z=height (open top)
    if bore_r > 0.0:
        bore = m3d.Manifold.extrude(_rounded_hex_cs(bore_f2f, bore_r),
                                    spec.height - spec.floor)
    else:
        bore = hex_prism(bore_f2f, spec.height - spec.floor)
    bore = bore.translate((0.0, 0.0, spec.floor))

    # Retaining depression: from z=base to z=floor
    if ret_r > 0.0:
        depression = m3d.Manifold.extrude(_rounded_hex_cs(ret_f2f, ret_r),
                                          spec.floor - spec.base)
    else:
        depression = hex_prism(ret_f2f, spec.floor - spec.base)
    depression = depression.translate((0.0, 0.0, spec.base))

    # 45° bevel at the top of the retaining ring: height = apothem drop so the
    # chamfer angle is 45° and paint tubes slide in without catching.
    bevel_h = min((bore_f2f - ret_f2f) / 2.0, spec.floor - spec.base - 0.1)
    bevel_cs = m3d.CrossSection([hex_polygon(ret_f2f)])
    bevel_scale = bore_f2f / ret_f2f
    entry_bevel = m3d.Manifold.extrude(bevel_cs, bevel_h, scale_top=(bevel_scale, bevel_scale))
    entry_bevel = entry_bevel.translate((0.0, 0.0, spec.floor - bevel_h))

    # Clip the bevel to the bore footprint.  The tapered hex has sharp corners
    # that extend further than the bore's rounded corners and would cut through
    # the outer wall.  Intersecting with the bore cross-section caps them.
    if bore_r > 0.0:
        bore_cap = m3d.Manifold.extrude(_rounded_hex_cs(bore_f2f, bore_r), bevel_h)
    else:
        bore_cap = hex_prism(bore_f2f, bevel_h)
    bore_cap = bore_cap.translate((0.0, 0.0, spec.floor - bevel_h))
    entry_bevel = entry_bevel ^ bore_cap

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
    r = spec.magnet_dia / 2.0 + spec.magnet_clearance
    cyl = m3d.Manifold.cylinder(length, r, circular_segments=64)

    if spec.magnet_bevel > 0.0:
        # 45° chamfer at the face-surface entry (z=MAGNET_OVERSHOOT in local space).
        # Frustum widens from r (at depth bevel) to r+bevel (at the face surface).
        b = spec.magnet_bevel
        bevel = m3d.Manifold.cylinder(b, r + b, r, circular_segments=64)
        cyl = cyl + bevel.translate([0.0, 0.0, MAGNET_OVERSHOOT])

    if spec.magnet_pushout:
        # 2 mm push-out hole along the pocket axis, extending 5 mm past the magnet
        # bottom — enough to punch through the ~0.7 mm back wall into the depression.
        pushout = m3d.Manifold.cylinder(length + 5.0, 1.0, circular_segments=32)
        cyl = cyl + pushout

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
        (0, 2, 150.0),   # upper-left  (top-left slope)
        (2, 0, 270.0),   # bottom flat
        (2, 2,  90.0),   # top flat
        (3, 0, 330.0),   # lower-right (bottom-right slope)
        (3, 2, 330.0),   # lower-right (bottom-right slope)
    ]
    for col, row, deg in placements:
        if col >= spec.cols or row >= spec.rows:
            continue
        cx, cy = cup_centre(spec, col, row)
        body -= _magnet_pocket(cx, cy, deg, spec)

    return body


# ---------------------------------------------------------------------------
# Wall side cutouts
# ---------------------------------------------------------------------------

def _cutouts_fit(spec: HexOrganizerSpec) -> bool:
    """Return True if there is enough vertical space for the pointed cutout profile.

    The profile needs a straight section between the bottom point (z_bottom + w2)
    and the top point (z_top - w2).  When those meet or cross, the shape inverts.
    """
    z_bottom = spec.floor + 1.0
    z_top    = spec.height - 8.0
    outer_f2f = spec.bore_f2f + 2.0 * spec.wall
    w2 = spec.side_cutout_width * (outer_f2f / np.sqrt(3)) / 2.0
    return z_top - z_bottom >= 2.0 * w2


def _side_cutouts_manifold(spec: HexOrganizerSpec) -> m3d.Manifold:
    """Union of pointed-top/bottom cutouts through every hex face wall for all cups.

    Each face gets a hexagonal prism (pointed top + bottom, straight sides) whose
    cross-section lives in the (tangent × world-Z) plane.  45° points means the
    vertical rise to full width = w/2 (rise == run).

    Profile built in manifold3d's XY plane (X=width, Y=height), extruded along Z
    (depth through wall).  rotate([90, 0, 90+deg]) maps:
        local X → world tangent   (−sin deg, cos deg, 0)
        local Y → world Z         (0, 0, 1)
        local Z → world normal    (cos deg, sin deg, 0)
    Then translate along the normal to centre the cutter on the wall.

    Face normals for a flat-top hex (vertices at 0°,60°…): 30°,90°,150°,210°,270°,330°.
    """
    outer_f2f     = spec.bore_f2f + 2.0 * spec.wall
    outer_apothem = outer_f2f / 2.0
    inner_apothem = spec.bore_f2f / 2.0
    apothem_mid   = (inner_apothem + outer_apothem) / 2.0

    face_len  = outer_f2f / np.sqrt(3)
    cutout_w  = spec.side_cutout_width * face_len
    w2        = cutout_w / 2.0         # half-width; also the 45° rise distance
    z_bottom  = spec.floor + 1.0
    z_top     = spec.height - 8.0
    depth     = spec.wall + 0.2        # wall + tiny overshoot each side

    # CCW polygon in local XY (X = tangent width, Y = absolute height)
    profile = [
        ( 0.0,  z_bottom),
        ( w2,   z_bottom + w2),
        ( w2,   z_top    - w2),
        ( 0.0,  z_top),
        (-w2,   z_top    - w2),
        (-w2,   z_bottom + w2),
    ]
    cs    = m3d.CrossSection([profile])
    prism = m3d.Manifold.extrude(cs, depth)

    face_normals_deg = [30.0, 90.0, 150.0, 210.0, 270.0, 330.0]

    face_cutters = []
    for deg in face_normals_deg:
        rad = np.radians(deg)
        nx, ny = float(np.cos(rad)), float(np.sin(rad))
        cutter = prism.rotate([90.0, 0.0, 90.0 + deg])
        # After rotation the extrusion axis (Z → normal) spans 0 → depth.
        # Shift along the normal so the cutter is centred on the wall.
        offset = apothem_mid - depth / 2.0
        cutter = cutter.translate([offset * nx, offset * ny, 0.0])
        face_cutters.append(cutter)

    all_cutters = []
    for col in range(spec.cols):
        for row in range(spec.rows):
            cx, cy = cup_centre(spec, col, row)
            for fc in face_cutters:
                all_cutters.append(fc.translate((cx, cy, 0.0)))

    return m3d.Manifold.batch_boolean(all_cutters, m3d.OpType.Add)


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

CUP_LOGO_SIZE_MM   = 20.0    # logo bounding square in each cup floor; fits inside 29 mm retaining hex
_LOGO_BORDER_FRAC  = 20.0 / 1024.0  # outer frame ring width as fraction of logo size (measured from SVG)


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


def _arrow_cs(length: float, line_w: float, *, point_right: bool = True) -> m3d.CrossSection:
    """Closed arrow polygon: shaft + arrowhead, centred at origin.

    point_right=True  → tip at +x  (row axis)
    point_right=False → tip at +y  (column axis, rotated 90° CCW)
    """
    head_w = 5.0 * line_w
    head_h = 4.0 * line_w
    hw     = line_w / 2.0
    hbw    = head_w / 2.0
    x0     = -length / 2.0
    xhb    = x0 + length - head_h   # x where head base meets shaft

    # CCW winding (manifold3d convention): bottom-left → right along bottom →
    # head-base-bottom → tip → head-base-top → right along top → back.
    pts: list[tuple[float, float]] = [
        (x0,          -hw),
        (xhb,         -hw),
        (xhb,        -hbw),
        (x0 + length, 0.0),
        (xhb,         hbw),
        (xhb,          hw),
        (x0,           hw),
    ]
    if not point_right:
        pts = [(-p[1], p[0]) for p in pts]  # rotate 90° CCW → tip at +y

    return m3d.CrossSection([pts])


def _mark_origin_cup(body: m3d.Manifold, spec: HexOrganizerSpec) -> m3d.Manifold:
    """Two orientation arrows grooved into the (col=0, row=0) cup floor.

    One arrow sits below the logo pointing right (+x, row axis).
    One arrow sits left of the logo pointing up (+y, column axis).
    Line width and groove depth match the logo's outer border ring.
    """
    cx, cy    = cup_centre(spec, 0, 0)
    depth     = spec.base / 4.0
    z_base    = spec.base - depth
    line_w    = CUP_LOGO_SIZE_MM * _LOGO_BORDER_FRAC
    logo_half = CUP_LOGO_SIZE_MM / 2.0
    gap       = 0.3                        # clearance from logo edge to arrow head base
    arrow_len = CUP_LOGO_SIZE_MM * 0.75   # 15 mm for the default 20 mm logo
    head_hw   = 2.5 * line_w              # half of head_w (must match _arrow_cs)

    # Arrow below logo, tip pointing right (+x = row axis)
    ay = cy - logo_half - gap - head_hw
    cs = _arrow_cs(arrow_len, line_w, point_right=True).translate((cx, ay))
    body -= m3d.Manifold.extrude(cs, depth).translate((0.0, 0.0, z_base))

    # Arrow left of logo, tip pointing up (+y = column axis)
    ax = cx - logo_half - gap - head_hw
    cs = _arrow_cs(arrow_len, line_w, point_right=False).translate((ax, cy))
    body -= m3d.Manifold.extrude(cs, depth).translate((0.0, 0.0, z_base))

    return body


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

    if spec.side_cutout_width > 0.0 and _cutouts_fit(spec):
        result -= _side_cutouts_manifold(spec)

    return result


def build_organizer(spec: HexOrganizerSpec) -> m3d.Manifold:
    if spec.height < 15:
        return _build_hollow(spec)

    result = m3d.Manifold.extrude(honeycomb_footprint_cs(spec), spec.height)
    result = _subtract_cup_cutters(result, spec)
    if spec.side_cutout_width > 0.0 and _cutouts_fit(spec):
        result -= _side_cutouts_manifold(spec)

    if spec.vertical_roundover > 0.0:
        result = _vertical_roundover(result, spec, spec.vertical_roundover)
    if spec.bottom_roundover > 0.0:
        result = _bottom_roundover(result, spec)
    # Magnet holes drilled after roundovers so the intersection in
    # _vertical_roundover cannot fill the pocket openings back in.
    result = _subtract_magnets(result, spec)
    # Cut the logo stamps and orientation marker last: they sit inside the
    # base slab, and the roundovers slice the body at low z to build their
    # templates — interior holes there would get swept through the model.
    result = _stamp_logos(result, spec)
    result = _mark_origin_cup(result, spec)
    return result


# ---------------------------------------------------------------------------
# CLI output helpers
# ---------------------------------------------------------------------------

_SPINNER_FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
_SPINNER_FPS    = 12.5


def _time_color(elapsed: float) -> str:
    """dim → yellow → red gradient keyed on elapsed seconds."""
    if elapsed < 2.0:
        return "dim white"
    if elapsed < 10.0:
        return "yellow"
    return "bold red"


class _SpinnerLine:
    """Rich renderable: indented spinner that aligns with the ✓ column."""

    def __init__(self, label: str) -> None:
        from rich.markup import escape
        self._label = escape(label)
        self._t0    = time.monotonic()

    def __rich_console__(self, console, options):
        from rich.text import Text
        elapsed = time.monotonic() - self._t0
        frame   = _SPINNER_FRAMES[int(elapsed * _SPINNER_FPS) % len(_SPINNER_FRAMES)]
        t_str   = "    s" if elapsed < 0.005 else f"{elapsed:.2f}s"
        yield Text.from_markup(
            f"  [cyan]{frame}[/cyan] {self._label:<38}"
            f" [cyan]{t_str}[/cyan]"
        )


@contextlib.contextmanager
def _step(console, label: str) -> Generator[None, None, None]:
    """Spin while work runs, print ✓ when done.  Falls back to plain print."""
    t0 = time.perf_counter()
    if console is not None:
        from rich.live import Live
        live = Live(
            _SpinnerLine(label),
            console=console,
            refresh_per_second=12,
            transient=True,
        )
        live.__enter__()
        try:
            yield
        finally:
            live.__exit__(None, None, None)
            elapsed = time.perf_counter() - t0
            tc = _time_color(elapsed)
            console.print(
                f"  [green]✓[/green] {label:<38} [{tc}]{elapsed:.2f}s[/]"
            )
    else:
        print(f"  {label}…", end="", flush=True)
        try:
            yield
        finally:
            elapsed = time.perf_counter() - t0
            print(f"  done  ({elapsed:.2f}s)")


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main() -> None:
    parser = argparse.ArgumentParser(description="Generate a Dharmatiles paint organizer STL.")
    parser.add_argument("--cols",      type=int,   default=4,   help="Number of columns (default: 4)")
    parser.add_argument("--rows",      type=int,   default=3,   help="Number of rows (default: 3)")
    parser.add_argument("--height",    type=float, default=60.0, help="Overall cup height in mm (default: 60.0)")
    parser.add_argument("--tolerance", type=float, default=0.3,
                        help="Bore clearance added to each bore diameter in mm (default: 0.3)")
    parser.add_argument("--magnet-clearance", type=float, default=0.1,
                        help="Extra radius added to magnet pocket for press-fit clearance in mm (default: 0.1)")
    parser.add_argument("--magnet-bevel", type=float, default=0.4,
                        help="45° chamfer depth at magnet hole entry in mm, 0=disabled (default: 0.4)")
    parser.add_argument("--no-magnet-pushout", action="store_true",
                        help="Disable the 1 mm push-out hole through the back of each magnet pocket")
    parser.add_argument("--side-cutout-width", type=float, default=0.4,
                        help="Rectangular cutout per hex face: fraction of face length, 0=disabled (default: 0.8)")
    parser.add_argument("--quiet",  action="store_true",      help="Suppress all output")
    args = parser.parse_args()

    spec      = HexOrganizerSpec(cols=args.cols, rows=args.rows, height=args.height,
                                  tolerance=args.tolerance,
                                  magnet_clearance=args.magnet_clearance,
                                  magnet_bevel=args.magnet_bevel,
                                  magnet_pushout=not args.no_magnet_pushout,
                                  side_cutout_width=args.side_cutout_width)
    out_path  = Path("stl/extras/dharmatiles-paint-organizer.stl")
    outer_f2f = spec.bore_f2f + 2.0 * spec.wall
    pitch_x, pitch_y = _pitches(spec)

    # ── Pick output mode ─────────────────────────────────────────────────────
    console = None
    if not args.quiet:
        try:
            import sys
            from rich.console import Console
            if sys.stdout.isatty():
                console = Console(highlight=False)
        except ImportError:
            pass

    # ── Header ───────────────────────────────────────────────────────────────
    if args.quiet:
        pass
    elif console is not None:
        from rich.rule import Rule
        console.print()
        console.print(Rule(
            f"[bold cyan]Dharmatiles Paint Organizer[/bold cyan]"
            f"  [dim]·  {spec.cols} cols × {spec.rows} rows  ·  {spec.height:.0f} mm tall[/dim]",
            style="cyan",
        ))
        tol_str = f"  ·  tolerance [cyan]{spec.tolerance:+.2f}[/cyan] mm" if spec.tolerance else ""
        console.print(
            f"  [dim]cup:[/dim]  outer F2F [cyan]{outer_f2f:.1f}[/cyan] mm"
            f"  ·  perimeter wall [cyan]{spec.wall:.1f}[/cyan] mm"
            f"  ·  interior wall [cyan]{spec.interior_wall:.1f}[/cyan] mm"
            + tol_str
        )
        console.print(
            f"  [dim]layout:[/dim]  col pitch [cyan]{pitch_x:.2f}[/cyan] mm"
            f"  ·  row pitch [cyan]{pitch_y:.1f}[/cyan] mm"
        )
        bevel_str   = (f"  ·  bevel [cyan]{spec.magnet_bevel:.2f}[/cyan] mm"
                       if spec.magnet_bevel > 0.0 else "")
        pushout_str = "  ·  push-out hole" if spec.magnet_pushout else ""
        console.print(
            f"  [dim]magnets:[/dim]  {spec.magnet_dia:.0f}×{spec.magnet_depth:.0f} mm"
            f"  at z={MAGNET_Z:.0f} mm"
            f"  ·  clearance [cyan]{spec.magnet_clearance:+.2f}[/cyan] mm"
            + bevel_str + pushout_str +
            f"  [dim](4 flat + 4 diagonal = 8 total)[/dim]"
        )
        console.print(
            "  [dim]marker:[/dim]  orientation arrows in the (0,0) cup floor"
            "  [dim]→ right (row) · ↑ up (col)[/dim]"
        )
    else:
        print(f"Building Dharmatiles paint organizer  ({spec.cols} columns of {spec.rows} cups, open front/back)")
        print(
            f"  cup: outer F2F {outer_f2f:.1f} mm  perimeter wall {spec.wall:.1f} mm  "
            f"interior wall {spec.interior_wall:.1f} mm"
        )
        print(f"  layout: col pitch {pitch_x:.2f} mm  row pitch {pitch_y:.1f} mm")
        bevel_str   = f"  bevel {spec.magnet_bevel:.2f} mm" if spec.magnet_bevel > 0.0 else ""
        pushout_str = "  push-out hole" if spec.magnet_pushout else ""
        print(f"  magnets: {spec.magnet_dia:.0f}×{spec.magnet_depth:.0f} mm, z={MAGNET_Z:.0f} mm  clearance {spec.magnet_clearance:+.2f} mm{bevel_str}{pushout_str}  (4 flat + 4 diagonal = 8 total)")
        print("  marker: orientation arrows in (0,0) cup floor  → row axis · ↑ col axis")

    # ── Build ────────────────────────────────────────────────────────────────
    t0_total = time.perf_counter()

    with _step(console, "Build honeycomb solid"):
        manifold = build_organizer(spec)

    with _step(console, "Convert to trimesh + fix normals"):
        raw  = manifold.to_mesh()
        mesh = trimesh.Trimesh(
            vertices=np.array(raw.vert_properties, dtype=float)[:, :3],
            faces=np.array(raw.tri_verts, dtype=int),
            process=False,
        )
        mesh.fix_normals()
        if mesh.volume < 0:
            mesh.invert()

    with _step(console, "Export STL"):
        out_path.parent.mkdir(parents=True, exist_ok=True)
        mesh.export(str(out_path))

    total_elapsed = time.perf_counter() - t0_total

    # ── Stats ────────────────────────────────────────────────────────────────
    if args.quiet:
        return

    n_verts    = len(mesh.vertices)
    n_faces    = len(mesh.faces)
    watertight = mesh.is_watertight
    volume     = mesh.volume

    if console is not None:
        wt_icon  = "[green]●[/green]" if watertight else "[bold red]✗[/bold red]"
        wt_label = "watertight" if watertight else "NOT watertight"
        tc_total = _time_color(total_elapsed)
        console.print(
            f"\n  {wt_icon} [dim]{wt_label}[/dim]"
            f"  [dim]{n_verts:,} verts · {n_faces:,} faces · {volume:.0f} mm³[/dim]"
        )
        console.print(f"  [#0078d4]{out_path}[/#0078d4]")
        console.print(
            f"  [dim]──[/dim] [{tc_total}]{total_elapsed:.1f}s total[/] [dim]──[/dim]\n"
        )
    else:
        print(f"Saved → {out_path}")
        print(f"Vertices: {n_verts:,}  Faces: {n_faces:,}")
        print(f"Watertight: {watertight}")
        print(f"Volume: {volume:.1f} mm³")
        print(f"Total: {total_elapsed:.1f}s")


if __name__ == "__main__":
    main()
