#!/usr/bin/env python3
"""Generate a stand STL by chopping the Hexpaint organizer with a flat cut.

Build the full organizer, then keep only the slab with y >= cut_y.  The
default cut passes through the centre y of the topmost cup in the
upward-staggered columns (cols 1, 3 in the default 4-col layout), so:

    col 0  →  thin top-edge strip      (~0.5 mm of y left)
    col 1  →  half hexagon              (top half of the topmost outer hex)
    col 2  →  thin top-edge strip
    col 3  →  half hexagon

Two and a half to three hexagons per column are erased.  All organizer
features above the cut are preserved (vertical roundover, bottom-edge
roundover, base, retaining recesses, any logos/bevels, and the (col 0,2
row 2, +y face) magnet pockets if they survive the slice).
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
    build_organizer,
    cup_centre,
)


@dataclass(frozen=True)
class HexStandSpec:
    organizer: HexOrganizerSpec = field(default_factory=HexOrganizerSpec)
    # cut_y: if None, compute from the organizer geometry (centre y of the
    # topmost cup in the upward-staggered column, i.e. col 1).
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
    org = stand.organizer
    body = build_organizer(org)

    cut_y = stand.cut_y if stand.cut_y is not None else _default_cut_y(org)
    return body ^ _y_halfspace_block(cut_y, body.bounding_box())


def main() -> None:
    stand = HexStandSpec()
    org = stand.organizer
    cut_y = stand.cut_y if stand.cut_y is not None else _default_cut_y(org)

    print(f"Building hex organizer stand by chopping the organizer")
    print(f"  cut at y = {cut_y:.2f} mm  (col-1 row-{org.rows - 1} centre)")
    print(f"  half-hex columns: 1, 3  (each contributes the top half of one outer hex)")
    print(f"  edge-strip columns: 0, 2  (each contributes a thin top strip in y)")

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
