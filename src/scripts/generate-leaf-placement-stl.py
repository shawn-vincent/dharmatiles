#!/usr/bin/env python3
"""Generate a debug sphere with leaves at four top-to-bottom positions.

The attachment points follow one meridian of a pure sphere:

* top:       0/4 down, polar angle   0°
* upper:     1/4 down, polar angle  45°
* equator:   2/4 down, polar angle  90°
* lower:     3/4 down, polar angle 135°

There is intentionally no leaf at the bottom pole.

Run from the repository root:

    python src/scripts/generate-leaf-placement-stl.py

The default output is ``debug/leaf-placement.stl``.  It uses the repository's
Magics-compatible colour-STL exporter.  Applications that ignore STL facet
colours will display a single colour even though the debug colours are encoded.
"""
from __future__ import annotations

import argparse
import pathlib

import numpy as np
import trimesh

from dharmatiles.core.color import debug_material, export_color_stl, tag
from dharmatiles.trees.branchlet import build_branchlet_and_leaf


DEFAULT_OUTPUT = pathlib.Path("debug/leaf-placement.stl")
SPHERE_RADIUS_MM = 12.0
LEAF_LENGTH_MM   = 6.0
LEAF_WIDTH_MM    = 3.5

# Fractions of the meridian arc from the top pole to the bottom pole.
PLACEMENTS: tuple[tuple[str, float], ...] = (
    ("top", 0.00),
    ("upper-quarter", 0.25),
    ("equator", 0.50),
    ("lower-quarter", 0.75),
)


def _attachment(radius_mm: float, fraction_down: float) -> tuple[np.ndarray, np.ndarray]:
    """Return the surface point and outward normal on the +X meridian."""
    polar_angle = np.pi * float(fraction_down)
    normal = np.array(
        [np.sin(polar_angle), 0.0, np.cos(polar_angle)],
        dtype=float,
    )
    return radius_mm * normal, normal


def build_debug_mesh(
) -> tuple[trimesh.Trimesh, list[tuple[str, np.ndarray, float, float]]]:
    """Build the coloured sphere, branchlets, and four leaves."""
    sphere = trimesh.creation.icosphere(subdivisions=4, radius=SPHERE_RADIUS_MM)
    sphere.fix_normals()
    tag(sphere, debug_material(0))

    parts: list[trimesh.Trimesh] = [sphere]
    placements: list[tuple[str, np.ndarray, float, float]] = []

    for leaf_index, (name, fraction_down) in enumerate(PLACEMENTS):
        attachment_point, surface_normal = _attachment(
            SPHERE_RADIUS_MM,
            fraction_down,
        )
        # Ceiling: large enough that the search is never artificially capped.
        # Worst case is near the south pole; the leaf tip must travel ~2R in the
        # horizontal direction to exit the sphere.  3 × SPHERE_RADIUS + one leaf
        # length is a safe upper bound for all polar angles.
        max_branchlet_mm = 3.0 * SPHERE_RADIUS_MM + LEAF_LENGTH_MM
        leaf_parts = build_branchlet_and_leaf(
            attachment_point=attachment_point,
            surface_normal=surface_normal,
            branchlet_length_mm=max_branchlet_mm,
            floor_angle_deg=45.0,
            root_radius_mm=None,
            embed_depth_mm=1.6,
            yaw_deg=0.0,
            seed=leaf_index,
            leaf_length_mm=LEAF_LENGTH_MM,
            leaf_width_mm=LEAF_WIDTH_MM,
            leaf_thickness_mm=1.2,
            leaf_fold_angle_deg=3.0,
            leaf_keel_depth_mm=0.0,
            parent_mesh=sphere,
        )
        chosen_length = float(
            leaf_parts[0].metadata["branchlet_length_mm"]
        )
        chosen_radius = float(
            leaf_parts[0].metadata["branchlet_root_radius_mm"]
        )
        placements.append(
            (name, attachment_point, chosen_length, chosen_radius)
        )

        # Each component gets its own vivid debug colour.  The sphere is slot 0;
        # subsequent slots distinguish every branchlet and leaf shell.
        for part_index, part in enumerate(leaf_parts):
            color_index = 1 + leaf_index * 2 + part_index
            tag(part, debug_material(color_index))
            parts.append(part)

    combined = trimesh.util.concatenate(parts)
    combined.metadata["name"] = "leaf-placement"
    return combined, placements


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        "-o",
        type=pathlib.Path,
        default=DEFAULT_OUTPUT,
        help=f"Output colour STL path (default: {DEFAULT_OUTPUT})",
    )
    args = parser.parse_args()

    mesh, placements = build_debug_mesh()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    export_color_stl(mesh, args.output)

    print(f"Wrote {args.output}")
    print(
        f"  {len(mesh.vertices):,} vertices · {len(mesh.faces):,} faces · "
        f"watertight={mesh.is_watertight}"
    )
    for name, point, length_mm, radius_mm in placements:
        print(
            f"  {name:13s} attachment={np.round(point, 3).tolist()}  "
            f"length={length_mm:.3f} mm  base_diameter={2.0 * radius_mm:.3f} mm"
        )


if __name__ == "__main__":
    main()
