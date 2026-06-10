#!/usr/bin/env python3
"""Generate a stand STL — a small slab matching the top of the Hex paint organizer.

Built from scratch in the stand's own coordinate frame (not by chopping a full
organizer build):

    +X  left → right across the desk (column direction, same as organizer)
    +Y  back → front depth (cup-axis direction; was organizer's +Z)
    +Z  up — vertical, gravity = −Z   (was organizer's +Y)

The cut plane lands on **z = 0** (the stand sits on its cut face on the desk).
Hex outlines stand vertically in the XZ plane and extrude along +Y.

For the default 4×3 organizer the surviving geometry, above the floor, is:

    cols 0, 2  →  thin top-edge strip       (≈0.5 mm of z above cut)
    cols 1, 3  →  upper half of a hex tube, open on both Y faces (no
                  retaining ring, no entry bevel — outer shell with a
                  through-bore)

A uniform 4 mm floor sits below the original cut plane, with its bottom
on z = 0 (the desk).  Floor top is flush with the original cut plane;
above it sit the strips (cols 0, 2) and the half-hex tubes (cols 1, 3).

The two surviving 90° magnet pockets bore 3 mm of body material (≈0.5 mm
of strip + ≈2.5 mm of floor), opening at the strip's top face, leaving
≈1.5 mm of floor below.

The outer vertical edges are rounded (matches the organizer's
`vertical_roundover`).  Intentionally minimal: no logos, no bottom
roundover, no origin dent.
"""
from __future__ import annotations

import sys
from dataclasses import dataclass, field
from pathlib import Path

import manifold3d as m3d
import numpy as np
import trimesh

sys.path.insert(0, str(Path(__file__).parent))
from hex_paint_organizer import (  # noqa: E402
    HexOrganizerSpec,
    _magnet_pocket,
    _rounded_hex_cs,
    _vertical_roundover,
    cup_centre,
    hex_polygon,
    hex_prism,
)


@dataclass(frozen=True)
class HexStandSpec:
    organizer: HexOrganizerSpec = field(default_factory=HexOrganizerSpec)
    # cut_y is in the *organizer's* Y axis (row direction).  The stand's z=0
    # plane corresponds to this value.  None → centre y of the topmost cup in
    # the upward-staggered column (col=1, top row).
    cut_y: float | None = None


def _default_cut_y(spec: HexOrganizerSpec) -> float:
    _, cy = cup_centre(spec, col=1, row=spec.rows - 1)
    return cy


def _y_halfspace_block(cut_y: float, bb) -> m3d.Manifold:
    """Oversized AA box covering the half-space y >= cut_y for the body in `bb`."""
    xmin, ymin, zmin, xmax, ymax, zmax = bb
    margin = 10.0
    return m3d.Manifold.cube(
        [
            (xmax - xmin) + 2.0 * margin,
            (ymax - cut_y) + margin,
            (zmax - zmin) + 2.0 * margin,
        ]
    ).translate([xmin - margin, cut_y, zmin - margin])


def build_stand(stand: HexStandSpec) -> m3d.Manifold:
    """Return the stand in its own frame: cut face on z=0, cup-axis along +Y."""
    spec = stand.organizer
    cut_y = stand.cut_y if stand.cut_y is not None else _default_cut_y(spec)

    # Construction frame matches the organizer's XYZ (X=cols, Y=rows, Z=cup-axis).
    # Only the topmost row can survive a cut at cut_y — build just that row.
    top_row = spec.rows - 1
    outer_f2f = spec.bore_f2f + 2.0 * spec.wall

    # 1. Solid outer hex prisms (no inner geometry).
    shells = [
        hex_prism(outer_f2f, spec.outer_wall_height).translate(
            (*cup_centre(spec, col, top_row), 0.0)
        )
        for col in range(spec.cols)
    ]
    body = shells[0]
    for s in shells[1:]:
        body = body + s

    # 2. Round the outside vertical edges (same profile as the organizer).
    if spec.vertical_roundover > 0.0:
        body = _vertical_roundover(body, spec, spec.vertical_roundover)

    # 3. Surviving magnet pockets — the two 90° top-flats on cols 0, 2.
    for col in (0, 2):
        if col >= spec.cols:
            continue
        cx, cy = cup_centre(spec, col, top_row)
        body = body - _magnet_pocket(cx, cy, 90.0, spec)

    # 4. Through-bore for cols 1, 3 — open hex tubes through the depth.
    #    Restricted to Y >= cut_y so the 1 mm floor below stays solid (forms
    #    the closed bottom of the half-hex tube).
    R_outer = outer_f2f / np.sqrt(3)
    bore_r = max(0.0, spec.vertical_roundover - (R_outer - spec.bore_f2f / np.sqrt(3)))
    if bore_r > 0.0:
        bore_cs = _rounded_hex_cs(spec.bore_f2f, bore_r)
    else:
        bore_cs = m3d.CrossSection([hex_polygon(spec.bore_f2f)])
    bore_template = m3d.Manifold.extrude(bore_cs, spec.outer_wall_height)
    for col in (1, 3):
        if col >= spec.cols:
            continue
        cx, cy = cup_centre(spec, col, top_row)
        bore_at_col = bore_template.translate((cx, cy, 0.0))
        bore_above_cut = bore_at_col ^ _y_halfspace_block(cut_y, bore_at_col.bounding_box())
        body = body - bore_above_cut

    # 5. Floor — uniform 4 mm slab below the cut.  The shells already extend
    #    down through this Y range, so the clip below is what actually defines
    #    the floor thickness.
    floor_thickness = 4.0
    floor_bottom = cut_y - floor_thickness
    body = body ^ _y_halfspace_block(floor_bottom, body.bounding_box())

    # Reframe into stand coordinates:
    #   rotate +90° about +X:  (x, y, z) → (x, -z, y)
    #     → construction Y (rows) becomes stand Z (vertical)
    #     → construction Z (cup-axis) becomes stand Y (depth)
    #   translate so the floor bottom lands on z=0 and the slab sits at y ≥ 0.
    body = body.rotate([90.0, 0.0, 0.0])
    body = body.translate([0.0, spec.outer_wall_height, -floor_bottom])

    return body


def main() -> None:
    stand = HexStandSpec()
    spec = stand.organizer
    cut_y = stand.cut_y if stand.cut_y is not None else _default_cut_y(spec)

    print("Building hex organizer stand (from-scratch, stand frame)")
    print(f"  cut at organizer y = {cut_y:.2f} mm  (col-1 row-{spec.rows - 1} centre)")
    print(f"  cols 0, 2 → top strips;  cols 1, 3 → open half-hex tubes")
    print(f"  outer vertical edges rounded (r={spec.vertical_roundover:.1f} mm)")
    print(f"  floor: uniform 4 mm slab below the cut plane")
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

    out_path = Path("stl/extras/hex_paint_organizer_stand.stl")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    mesh.export(str(out_path))

    print(f"Saved → {out_path}")
    print(f"Vertices: {len(mesh.vertices):,}  Faces: {len(mesh.faces):,}")
    print(f"Watertight: {mesh.is_watertight}")
    print(f"Volume: {mesh.volume:.1f} mm³")


if __name__ == "__main__":
    main()
