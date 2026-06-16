#!/usr/bin/env python3
"""Generate a stand STL for the Dharmatiles paint organizer.

Builds geometry directly in place instead of generate-then-cut-then-rotate:
it avoids assembling an entire organizer-frame body (X=cols, Y=rows,
Z=cup-axis) at full height only to clip it with oversized half-space boxes
and reframe into the stand's own axes as a final step.

Here the floor and bore cuts are 2D half-plane clips applied to the hex
*cross-sections* before they are ever extruded — clipping a cross-section
to `v >= v0` and then extruding along an axis the clip doesn't touch is the
same solid as extruding first and clipping the result (the two operations
commute), so this produces the identical solid without ever materialising
the part that gets thrown away. The vertical (row) coordinate has the
`floor_bottom` shift baked in from the moment each cup centre is computed,
so cross-sections already live at their final Z coordinate.

The only unavoidable transform is a single rotate at the very end: manifold3d's
`extrude()` always extrudes a 2D cross-section along its own Z axis, so the
cup-axis (this script's extrude axis) has to be swapped into the stand's Y
(depth) axis once the solid is complete. That swap is a fixed frame-convention
fix, not a cutting step.

Final stand frame:

    +X  left -> right across the desk (column direction, same as organizer)
    +Y  back -> front depth (cup-axis direction)
    +Z  up — vertical, gravity = -Z

The cut plane lands on z = 0 (the stand sits on its cut face on the desk).
For the default 4x3 organizer the surviving geometry, above the floor, is:

    cols 0, 2  ->  thin top-edge strip
    cols 1, 3  ->  upper half of a hex tube, open on both Y faces

A uniform 4 mm floor sits below the cut plane. The two surviving 90° magnet
pockets bore into the floor. Outer vertical edges are rounded (matches the
organizer's `vertical_roundover`).
"""
from __future__ import annotations

import argparse
import importlib.util
import sys
from dataclasses import dataclass, field
from pathlib import Path

import manifold3d as m3d
import numpy as np
import trimesh

_ORGANIZER_PATH = Path(__file__).with_name("dharmatiles-paint-organizer.py")
_ORGANIZER_SPEC = importlib.util.spec_from_file_location(
    "dharmatiles_paint_organizer", _ORGANIZER_PATH
)
if _ORGANIZER_SPEC is None or _ORGANIZER_SPEC.loader is None:
    raise ImportError(f"Could not load {_ORGANIZER_PATH}")
_ORGANIZER = importlib.util.module_from_spec(_ORGANIZER_SPEC)
sys.modules[_ORGANIZER_SPEC.name] = _ORGANIZER
_ORGANIZER_SPEC.loader.exec_module(_ORGANIZER)

BOOLEAN_OVERLAP = _ORGANIZER.BOOLEAN_OVERLAP
HexOrganizerSpec = _ORGANIZER.HexOrganizerSpec
_magnet_pocket = _ORGANIZER._magnet_pocket
_rounded_hex_cs = _ORGANIZER._rounded_hex_cs
cup_centre = _ORGANIZER.cup_centre
hex_cross_section = _ORGANIZER.hex_cross_section

FLOOR_THICKNESS_MM = 4.0


@dataclass(frozen=True)
class HexStandSpec:
    organizer: HexOrganizerSpec = field(default_factory=HexOrganizerSpec)
    # cut_y is in the *organizer's* Y axis (row direction).  The stand's z=0
    # plane corresponds to this value.  None → centre y of the topmost cup in
    # the upward-staggered column (col=1, top row).
    cut_y: float | None = None


def _stand_depth(spec: HexOrganizerSpec) -> float:
    """Front-to-back stand depth, including the organizer's inner-wall clearance."""
    return spec.height + spec.tolerance


def _default_cut_y(spec: HexOrganizerSpec) -> float:
    _, cy = cup_centre(spec, col=1, row=spec.rows - 1)
    return cy


def _halfplane_cs(v_min: float, margin: float = 1000.0) -> m3d.CrossSection:
    """A cross-section covering the half-plane v >= v_min (clipped to +-margin)."""
    return m3d.CrossSection.square([2.0 * margin, margin]).translate([-margin, v_min])


def _x_halfplane_cs(x0: float, *, keep_greater: bool, margin: float = 1000.0) -> m3d.CrossSection:
    """A cross-section covering x >= x0 (or x <= x0 if keep_greater is False)."""
    x_start = x0 if keep_greater else x0 - margin
    return m3d.CrossSection.square([margin, 2.0 * margin]).translate([x_start, -margin])


# ---------------------------------------------------------------------------
# Tip-back brackets
# ---------------------------------------------------------------------------
#
# One L-shaped brace per raised half-hex column (cols 1, 3 in the default
# layout) — modelled after a tapered shelf-bracket gusset. Both arms meet at
# the column's OUTER corner edge: at the front (Y=depth, the far open end of
# the half-hex tube, away from the desk's near edge) and proud of the desk
# surface by the plate thickness (Z=t):
#
#   - vertical arm: a flat panel fills the half-hex tube's open front
#     (literally giving it a back wall, rounded like the hex itself) for the
#     column's own height, then continues upward as a tapered fin to the
#     full arm length.
#   - horizontal arm: a tapered foot lying flush on the desk, extending
#     forward (+Y, away from the stand) to widen the effective support
#     footprint.
#
# Both arms taper from a wide elbow to a rounded tip. A single round rib
# runs the whole bend — a frustum down each arm's outside face into one
# sphere at the corner — so it reads as one continuous bead that thickens
# near the corner and blends smoothly around it, rather than two straight
# ribs butted together.

BRACKET_ARM_LENGTH_MM        = 100.0  # each arm's length from the elbow
BRACKET_THICKNESS_MM         = 3.0    # flat plate thickness
BRACKET_WIDTH_ELBOW_MM       = 16.0   # plate width at the elbow
BRACKET_WIDTH_TIP_MM         = 6.0    # plate width at the tip
BRACKET_RIB_RADIUS_TIP_MM    = 1.5    # rib bead radius at each tip
BRACKET_RIB_RADIUS_CORNER_MM = 4.0    # rib bead radius at the corner (thicker)


def _column_xz(spec: HexOrganizerSpec, floor_bottom: float, col: int) -> tuple[float, float]:
    """Column centre (x, z) in the stand's final frame."""
    cx, cy = cup_centre(spec, col, spec.rows - 1)
    return cx, cy - floor_bottom


def _trapezoid_cs(cx: float, w0: float, w1: float, y0: float, y1: float) -> m3d.CrossSection:
    """CCW trapezoid in local (x, y): width w0 at y=y0, width w1 at y=y1."""
    pts = [
        (cx - w0 / 2.0, y0),
        (cx + w0 / 2.0, y0),
        (cx + w1 / 2.0, y1),
        (cx - w1 / 2.0, y1),
    ]
    if y1 < y0:
        pts = pts[::-1]  # keep CCW winding regardless of taper direction
    return m3d.CrossSection([pts])


def _rounded_tip_cs(cx: float, w0: float, w1: float, y0: float, y1: float) -> m3d.CrossSection:
    """Tapered trapezoid with a semicircular cap (radius w1/2) at the y1 end."""
    trapezoid = _trapezoid_cs(cx, w0, w1, y0, y1)
    cap = m3d.CrossSection.circle(w1 / 2.0, circular_segments=24).translate((cx, y1))
    return m3d.CrossSection.batch_boolean([trapezoid, cap], m3d.OpType.Add)


def _frustum_between(p0: tuple[float, float, float], r0: float,
                      p1: tuple[float, float, float], r1: float,
                      *, segments: int = 24) -> m3d.Manifold:
    """Tapered cylinder from p0 (radius r0) to p1 (radius r1).

    Restricted to segments lying in a single X=const plane (true for every
    rib segment here, since the bend lives entirely in the column's (Y, Z)
    plane) — direction has no X-component, so a single rotation about the
    global X axis aligns the cylinder's default +Z axis with it.
    """
    x0, y0, z0 = p0
    x1, y1, z1 = p1
    dy, dz = y1 - y0, z1 - z0
    length = float(np.hypot(dy, dz))
    cyl = m3d.Manifold.cylinder(length, r0, r1, circular_segments=segments)
    theta_deg = float(np.degrees(np.arctan2(-dy, dz)))
    return cyl.rotate([theta_deg, 0.0, 0.0]).translate([x0, y0, z0])


def _bracket_for_column(spec: HexOrganizerSpec, floor_bottom: float, depth: float,
                         col: int) -> m3d.Manifold:
    """Build one tip-back brace for the raised half-hex at `col`, in the
    stand's final (X, Y, Z) frame."""
    cx, cz    = _column_xz(spec, floor_bottom, col)
    outer_f2f = spec.bore_f2f + 2.0 * spec.wall
    t, L      = BRACKET_THICKNESS_MM, BRACKET_ARM_LENGTH_MM
    we, wt    = BRACKET_WIDTH_ELBOW_MM, BRACKET_WIDTH_TIP_MM
    r_tip     = BRACKET_RIB_RADIUS_TIP_MM
    r_corner  = BRACKET_RIB_RADIUS_CORNER_MM

    y_attach = depth        # the tube's far open end — front, away from the desk's near edge
    y_outer  = y_attach + t # vertical plate's outside (convex) face

    # Vertical arm: extrude a 2D shape in local (x, z) — using the same
    # rounded-corner hex profile as the tube itself — and rotate it into the
    # Y=[y_attach, y_outer] slab flush against the column's open front.
    # rotate([90,0,0]) maps (x, y, z) -> (x, -z, y), so the cross-section's
    # own y becomes final z; translating by y_outer relocates the resulting
    # [-t, 0] slab to [y_attach, y_outer] without disturbing that mapping.
    column_outer_cs = (
        _rounded_hex_cs(outer_f2f, spec.vertical_roundover).translate((cx, cz))
        ^ _halfplane_cs(0.0)
    )
    fin_cs         = _rounded_tip_cs(cx, we, wt, 0.0, L)
    vertical_cs    = m3d.CrossSection.batch_boolean([column_outer_cs, fin_cs], m3d.OpType.Add)
    vertical_plate = (m3d.Manifold.extrude(vertical_cs, t)
                       .rotate([90.0, 0.0, 0.0])
                       .translate([0.0, y_outer, 0.0]))

    # Horizontal arm: a tapered, rounded-tip foot already in (x, y), extruded
    # directly along z — no rotation needed, it sits flush on the desk
    # (z in [0, t]) and extends forward from the elbow to y_attach + L.
    foot_cs    = _rounded_tip_cs(cx, we, wt, y_attach, y_attach + L)
    foot_plate = m3d.Manifold.extrude(foot_cs, t)

    # Rib: one continuous round bead — frustum up the vertical arm's outside
    # face, a sphere at the corner (thicker, and smooth in every direction
    # so the bend has no seam), frustum out along the horizontal arm's
    # outside face. Both bend-plane coordinates only ever vary in (y, z), so
    # _frustum_between's single-axis rotation is exact here.
    corner_pt   = (cx, y_outer, t)
    vertical_rib   = _frustum_between((cx, y_outer, L), r_tip, corner_pt, r_corner)
    horizontal_rib = _frustum_between(corner_pt, r_corner, (cx, y_attach + L, t), r_tip)
    corner_bead    = m3d.Manifold.sphere(r_corner, circular_segments=24).translate(corner_pt)

    bracket = vertical_plate + foot_plate + vertical_rib + horizontal_rib + corner_bead

    # The corner bead's radius exceeds the foot's thickness, so it dips
    # below the desk plane (z=0) — clip it flush rather than letting the
    # brace poke through the print bed.
    above_desk = m3d.Manifold.cube([2000.0, 2000.0, 2000.0]).translate([-1000.0, -1000.0, 0.0])
    return bracket ^ above_desk


def build_stand(stand: HexStandSpec) -> m3d.Manifold:
    """Return the stand directly in its own frame: cut face on z=0, cup-axis along +Y."""
    spec = stand.organizer
    depth = _stand_depth(spec)
    cut_y = stand.cut_y if stand.cut_y is not None else _default_cut_y(spec)
    floor_bottom = cut_y - FLOOR_THICKNESS_MM

    top_row = spec.rows - 1
    outer_f2f = spec.bore_f2f + 2.0 * spec.wall

    def cup_xv(col: int) -> tuple[float, float]:
        """Cup centre (x, v) where v is already the stand's final vertical coordinate."""
        cx, cy = cup_centre(spec, col, top_row)
        return cx, cy - floor_bottom

    # 1. Outer footprint, in (X, V): union the per-cup hex cross-sections for
    #    the single surviving row.  Padded by BOOLEAN_OVERLAP so adjacent
    #    columns' tangent walls union cleanly (matches the organizer's
    #    honeycomb_footprint_cs technique).
    construction_f2f = outer_f2f + BOOLEAN_OVERLAP
    cells = [
        hex_cross_section(construction_f2f).translate(cup_xv(col))
        for col in range(spec.cols)
    ]
    footprint_cs = m3d.CrossSection.batch_boolean(cells, m3d.OpType.Add)

    # 2. Round the outside vertical edges (same double-offset profile as the
    #    organizer) while the footprint is still a flat, feature-free 2D shape.
    if spec.vertical_roundover > 0.0:
        r = spec.vertical_roundover
        cs_in = footprint_cs.offset(-r, m3d.JoinType.Miter, miter_limit=10.0)
        footprint_cs = cs_in.offset(r, m3d.JoinType.Round, circular_segments=32)

    # 3. Floor clip: keep v >= 0.  Clipping the 2D cross-section here and
    #    extruding the result is the same solid as extruding first and
    #    clipping the 3D body afterwards (the half-plane doesn't depend on
    #    the extrude axis) — so this is the floor cut, done once, with no
    #    intermediate full-height body ever built only to be chopped down.
    footprint_cs = footprint_cs ^ _halfplane_cs(0.0)

    # 3b. Column 0's left end: the natural hex corner there tapers off on the
    #     vertical_roundover arc and pinches to a point at the floor (its
    #     vertex sits below v=0, so the floor clip slices through the curved
    #     corner instead of a flat). Swap that taper for a half-circle end
    #     cap of radius = strip_top/2, tangent to both the top flat and the
    #     floor — same idea as a stadium/rounded-rect end. The cap starts
    #     exactly where the existing rounded corner's tangent point already
    #     is (vertex_x + r*tan(30°), the standard offset for rounding a 60°
    #     hex corner with radius vertical_roundover), so it splices in without
    #     disturbing the untouched flat top to its right.
    cx0, v0 = cup_xv(0)
    strip_top = construction_f2f / 2.0 + v0
    if strip_top > 0.0:
        R_construction = construction_f2f / np.sqrt(3)
        vertex_x = cx0 - R_construction / 2.0
        cap_x = vertex_x + spec.vertical_roundover * np.tan(np.radians(30.0))
        cap_r = strip_top / 2.0
        cap = m3d.CrossSection.circle(cap_r, circular_segments=32).translate([cap_x, cap_r])
        cap = cap ^ _x_halfplane_cs(cap_x, keep_greater=False)
        footprint_cs = (footprint_cs ^ _x_halfplane_cs(cap_x, keep_greater=True)) + cap

    body = m3d.Manifold.extrude(footprint_cs, depth)

    # 4. Surviving magnet pockets — the two 90° top-flats on cols 0, 2.
    for col in (0, 2):
        if col >= spec.cols:
            continue
        cx, v = cup_xv(col)
        body -= _magnet_pocket(cx, v, 90.0, spec)

    # 5. Through-bore for cols 1, 3 — open hex tubes through the depth.
    #    The bore cross-section is clipped to v >= FLOOR_THICKNESS_MM (i.e.
    #    above the cut) before extruding, so the floor below stays solid
    #    without ever building the unclipped bore and cutting it down.
    #    bore_f2f matches the organizer's actual open-bore diameter (see
    #    dharmatiles-paint-organizer.py _cup_cutters) — without the tolerance pad this
    #    tube would print 2*tolerance narrower than the cavity it stands in for.
    bore_f2f = spec.bore_f2f + 2.0 * spec.tolerance
    R_outer = outer_f2f / np.sqrt(3)
    bore_r = max(0.0, spec.vertical_roundover - (R_outer - bore_f2f / np.sqrt(3)))
    bore_cs_local = (
        _rounded_hex_cs(bore_f2f, bore_r) if bore_r > 0.0 else hex_cross_section(bore_f2f)
    )
    for col in (1, 3):
        if col >= spec.cols:
            continue
        cx, v = cup_xv(col)
        bore_cs = bore_cs_local.translate((cx, v)) ^ _halfplane_cs(FLOOR_THICKNESS_MM)
        body -= m3d.Manifold.extrude(bore_cs, depth)

    # 6. Frame fix: extrude() always extrudes along its own Z, so the extrude
    #    axis (the cup-axis / stand depth) needs to land on Y. rotate([90,0,0])
    #    maps (x, y, z) -> (x, -z, y); translating by +depth then puts the
    #    depth axis in [0, depth] with the floor cut already sitting at z=0
    #    (baked into the footprint's V coordinate above, no separate shift
    #    needed here).
    body = body.rotate([90.0, 0.0, 0.0])
    body = body.translate([0.0, depth, 0.0])

    # 7. Tip-back brackets: one per raised half-hex column, built directly in
    #    this final (X, Y, Z) frame (see _bracket_for_column).
    for col in (1, 3):
        if col >= spec.cols:
            continue
        body = body + _bracket_for_column(spec, floor_bottom, depth, col)

    return body


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Generate a stand STL matching the top of the Dharmatiles paint organizer."
    )
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
                        help="Rectangular cutout per hex face: fraction of face length, 0=disabled (default: 0.4) "
                             "(accepted for parity with the organizer's spec; the stand has no side cutouts)")
    args = parser.parse_args()

    organizer_spec = HexOrganizerSpec(
        cols=args.cols, rows=args.rows, height=args.height,
        tolerance=args.tolerance,
        magnet_clearance=args.magnet_clearance,
        magnet_bevel=args.magnet_bevel,
        magnet_pushout=not args.no_magnet_pushout,
        side_cutout_width=args.side_cutout_width,
    )
    stand = HexStandSpec(organizer=organizer_spec)
    spec = stand.organizer
    cut_y = stand.cut_y if stand.cut_y is not None else _default_cut_y(spec)

    print("Building Dharmatiles paint organizer stand (in-place, stand frame)")
    print(f"  cut at organizer y = {cut_y:.2f} mm  (col-1 row-{spec.rows - 1} centre)")
    print(f"  depth: {spec.height:.2f} mm + inner-wall clearance {spec.tolerance:.2f} mm = {_stand_depth(spec):.2f} mm")
    print(f"  cols 0, 2 → top strips;  cols 1, 3 → open half-hex tubes")
    print(f"  outer vertical edges rounded (r={spec.vertical_roundover:.1f} mm)")
    print(f"  floor: uniform {FLOOR_THICKNESS_MM:.0f} mm slab below the cut plane")
    print(f"  magnets: (0, {spec.rows - 1}, 90°) and (2, {spec.rows - 1}, 90°) → 3 mm pockets in the floor")

    manifold = build_stand(stand)
    raw = manifold.to_mesh()
    mesh = trimesh.Trimesh(
        vertices=np.array(raw.vert_properties, dtype=float)[:, :3],
        faces=np.array(raw.tri_verts, dtype=int),
        process=False,
    )
    mesh.fix_normals()
    if mesh.volume < 0:
        mesh.invert()

    out_path = Path("stl/extras/dharmatiles-paint-organizer-stand.stl")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    mesh.export(str(out_path))

    print(f"Saved → {out_path}")
    print(f"Vertices: {len(mesh.vertices):,}  Faces: {len(mesh.faces):,}")
    print(f"Watertight: {mesh.is_watertight}")
    print(f"Volume: {mesh.volume:.1f} mm³")


if __name__ == "__main__":
    main()
