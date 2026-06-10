#!/usr/bin/env python3
"""Generate a stand STL — a small slab matching the top of the Hex paint organizer.

Built from scratch in the stand's own coordinate frame (not by chopping a full
organizer build):

    +X  left → right across the desk (column direction, same as organizer)
    +Y  back → front depth (cup-axis direction; was organizer's +Z)
    +Z  up — vertical, gravity = −Z   (was organizer's +Y)

The cut plane lands on **z = 0** (the stand sits on its cut face on the desk).
Hex outlines stand vertically in the XZ plane and extrude along +Y.

For the default 4×3 organizer the surviving geometry is:

    cols 0, 2  →  thin top-edge strip       (≈0.5 mm of z above cut)
    cols 1, 3  →  upper half of the topmost cup, including its bore, retaining
                  depression and entry bevel

Of the 8 organizer magnet pockets, only the two top-flat (90°) pockets on
(col=0, top_row) and (col=2, top_row) sit above the cut.  They survive as
shallow (~1 mm) divots on the underside of the stand — not functional retainers,
just the honest cut-off cross-section.

Intentionally minimal: no logos, no roundovers, no origin dent.  Add
stand-specific features here as the design evolves.
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
    cup_centre,
    single_cup,
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
    # Only the topmost row of cups can survive a cut at cut_y — build just that row,
    # not the full organizer.
    top_row = spec.rows - 1
    cup = single_cup(spec)
    cups = [cup.translate((*cup_centre(spec, col, top_row), 0.0))
            for col in range(spec.cols)]
    body = cups[0]
    for c in cups[1:]:
        body = body + c

    # Of the 8 organizer magnets, only the two 90° top-flats on (col=0, top_row)
    # and (col=2, top_row) survive the cut.
    for col in (0, 2):
        if col >= spec.cols:
            continue
        cx, cy = cup_centre(spec, col, top_row)
        body = body - _magnet_pocket(cx, cy, 90.0, spec)

    # Drop everything below the cut.
    body = body ^ _y_halfspace_block(cut_y, body.bounding_box())

    # Reframe into stand coordinates:
    #   rotate +90° about +X:  (x, y, z) → (x, -z, y)
    #     → construction Y (rows) becomes stand Z (vertical)
    #     → construction Z (cup-axis) becomes stand Y (depth)
    #   translate so the cut face lands on z=0 and the slab sits at y ≥ 0.
    body = body.rotate([90.0, 0.0, 0.0])
    body = body.translate([0.0, spec.outer_wall_height, -cut_y])

    return body


def main() -> None:
    stand = HexStandSpec()
    spec = stand.organizer
    cut_y = stand.cut_y if stand.cut_y is not None else _default_cut_y(spec)

    print("Building hex organizer stand (from-scratch, stand frame)")
    print(f"  cut at organizer y = {cut_y:.2f} mm  (col-1 row-{spec.rows - 1} centre)")
    print(f"  cols 0, 2 → top strips;  cols 1, 3 → upper half-hexes")
    print(f"  magnets: (0, {spec.rows - 1}, 90°) and (2, {spec.rows - 1}, 90°) → shallow divots on cut face")

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
