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
