#!/usr/bin/env python3
"""Debug: sphere + trunk + N randomly-placed leaf solids via leaf_placement_from_surface.

Leaves are seeded at random points on the sphere surface.  Each point is
snapped to the nearest mesh vertex (via leaf_placement_from_surface), which
returns the base position, a gravity-down-in-tangent-plane axis, and the
outward surface normal.  A configurable dip angle is then applied to angle
the tip into the sphere before building the leaf solid.

Wall faces are coloured green (printable) or red (overhang / unsupported)
using the same FDM analysis as the simplified script.

Run from the repository root::

    python src/scripts/generate-debug-leaf-placement-random-stl.py
    python src/scripts/generate-debug-leaf-placement-random-stl.py --count 50 --dip-deg 45
    python src/scripts/generate-debug-leaf-placement-random-stl.py --seed 7 --length-mm 8
"""
from __future__ import annotations

import argparse
import pathlib

import numpy as np
import trimesh

from dharmatiles.core.color import debug_material, export_color_stl, tag
from dharmatiles.trees.leaf import (
    LEAF_LENGTH_MM_DEFAULT,
    LEAF_ROOT_DEPTH_MM,
    LEAF_WIDTH_MM_DEFAULT,
    build_leaf_surface,
    find_max_dip_for_sphere,
    leaf_placement_from_surface,
    solidify_leaf,
)
from _leaf_debug import color_leaf_walls_by_fdm

# ── Constants ──────────────────────────────────────────────────────────────────

DEFAULT_OUTPUT   = pathlib.Path("stl/debug/leaf-placement-debug-random.stl")
SPHERE_RADIUS_MM = 12.0
TRUNK_HEIGHT_MM  = SPHERE_RADIUS_MM * 2.0 / 3.0
TRUNK_RADIUS_MM  = TRUNK_HEIGHT_MM / 2.0
LEAF_FOLD_DEG    = 6.0
LEAF_CURL_DEG    = 40.0


# ── Scene assembly ─────────────────────────────────────────────────────────────

def build_debug_mesh(
    *,
    count:      int          = 30,
    seed:       int          = 42,
    dip_deg:    float | None = None,
    length_mm:  float        = LEAF_LENGTH_MM_DEFAULT,
    width_mm:   float        = LEAF_WIDTH_MM_DEFAULT,
) -> trimesh.Trimesh:
    """Sphere + trunk + *count* randomly-placed leaf solids.

    When *dip_deg* is ``None`` (default) the dip for each leaf is found
    automatically via binary search — the maximum angle that presses the leaf
    flush against the sphere without penetrating it, matching the behaviour of
    the simplified placement script.
    """
    sphere = trimesh.creation.icosphere(subdivisions=4, radius=SPHERE_RADIUS_MM)
    sphere.fix_normals()
    tag(sphere, debug_material(1))

    _trunk_embed = 5.0
    trunk = trimesh.creation.cylinder(
        radius=TRUNK_RADIUS_MM,
        height=TRUNK_HEIGHT_MM + _trunk_embed,
        sections=32,
    )
    trunk.apply_translation(
        [0.0, 0.0, -SPHERE_RADIUS_MM - TRUNK_HEIGHT_MM / 2.0 + _trunk_embed / 2.0]
    )
    trunk.fix_normals()
    tag(trunk, debug_material(1))

    support_mesh = trimesh.util.concatenate([sphere, trunk])
    parts: list[trimesh.Trimesh] = [sphere, trunk]

    # Random points uniformly distributed on the sphere surface.
    rng  = np.random.default_rng(seed)
    dirs = rng.standard_normal((count, 3))
    dirs /= np.linalg.norm(dirs, axis=1, keepdims=True)
    pts  = dirs * SPHERE_RADIUS_MM

    leaf_shape = dict(
        length_mm=length_mm, width_mm=width_mm,
        fold_angle_deg=LEAF_FOLD_DEG, curl_deg=LEAF_CURL_DEG,
    )
    fixed_dip_rad = None if dip_deg is None else np.radians(dip_deg)

    n_ok_total = n_fail_total = 0
    for i, pt in enumerate(pts):
        # Placement: snap to sphere surface, get surface frame.
        base_pos, T0, up_hint = leaf_placement_from_surface(sphere, pt)

        # Dip: find max angle that presses leaf against sphere (or use override).
        dip_rad = (
            fixed_dip_rad
            if fixed_dip_rad is not None
            else find_max_dip_for_sphere(
                base_pos, T0, up_hint, SPHERE_RADIUS_MM, **leaf_shape
            )
        )

        tangent = T0 * np.cos(dip_rad) - up_hint * np.sin(dip_rad)
        tangent /= np.linalg.norm(tangent) + 1e-12

        leaf_surf = build_leaf_surface(
            base_pos=base_pos,
            tangent=tangent,
            up_hint=up_hint,
            **leaf_shape,
        )

        leaf, wall_faces = solidify_leaf(
            leaf_surf, up_hint, LEAF_ROOT_DEPTH_MM,
            parent_sphere_radius=SPHERE_RADIUS_MM,
        )

        # Cycle through colours 2–9 so leaves are visually distinct.
        tag(leaf, debug_material(2 + (i % 8)))
        color_leaf_walls_by_fdm(leaf, wall_faces, support_mesh)

        wall_colors = leaf.visual.face_colors[list(wall_faces)]
        n_ok   = int(np.sum(wall_colors[:, 0] < 128))
        n_fail = len(wall_faces) - n_ok
        n_ok_total   += n_ok
        n_fail_total += n_fail

        parts.append(leaf)

    dip_label = "auto" if dip_deg is None else f"{dip_deg:.0f}°"
    print(f"  {count} leaves  dip={dip_label}  seed={seed}")
    print(f"  wall faces: {n_ok_total} green  {n_fail_total} red  "
          f"({'%.0f' % (100*n_ok_total/(n_ok_total+n_fail_total+1e-9))}% printable)")
    return trimesh.util.concatenate(parts)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", "-o", type=pathlib.Path, default=DEFAULT_OUTPUT,
                        help=f"Output STL path (default: {DEFAULT_OUTPUT})")
    parser.add_argument("--count", "-n", type=int, default=30,
                        help="Number of leaves (default: 30)")
    parser.add_argument("--seed", type=int, default=42,
                        help="Random seed (default: 42)")
    parser.add_argument("--dip-deg", type=float, default=None,
                        help="Fix dip angle (degrees). Omit for auto: max dip without penetration (default)")
    parser.add_argument("--length-mm", type=float, default=LEAF_LENGTH_MM_DEFAULT,
                        help=f"Leaf length mm (default: {LEAF_LENGTH_MM_DEFAULT})")
    parser.add_argument("--width-mm", type=float, default=LEAF_WIDTH_MM_DEFAULT,
                        help=f"Leaf width mm (default: {LEAF_WIDTH_MM_DEFAULT})")
    args = parser.parse_args()

    mesh = build_debug_mesh(
        count=args.count,
        seed=args.seed,
        dip_deg=args.dip_deg,
        length_mm=args.length_mm,
        width_mm=args.width_mm,
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    export_color_stl(mesh, args.output)

    print(f"Wrote {args.output}")
    print(f"  {len(mesh.vertices):,} vertices · {len(mesh.faces):,} faces · "
          f"watertight={mesh.is_watertight}")


if __name__ == "__main__":
    main()
