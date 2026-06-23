#!/usr/bin/env python3
"""Debug: sphere + trunk + N jitter-grid-placed leaf solids.

Leaves are seeded at evenly-distributed jittered points on the sphere surface.
Wall faces are coloured green (printable) or red (overhang) by FDM analysis.

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
    LEAF_WIDTH_MM_DEFAULT,
    leaf_placement_from_surface,
    place_leaf_on_sphere,
)
from _leaf_debug import color_leaf_walls_by_fdm

# ── Constants ──────────────────────────────────────────────────────────────────

DEFAULT_OUTPUT   = pathlib.Path("stl/debug/leaf-placement-debug-random.stl")
SPHERE_RADIUS_MM = 12.0
TRUNK_HEIGHT_MM  = SPHERE_RADIUS_MM * 2.0 / 3.0
TRUNK_RADIUS_MM  = TRUNK_HEIGHT_MM / 2.0
LEAF_FOLD_DEG    = 6.0
LEAF_CURL_DEG    = 40.0


# ── Placement helpers ──────────────────────────────────────────────────────────

def _jitter_sphere_pts(
    count:  int,
    radius: float,
    rng:    np.random.Generator,
) -> np.ndarray:
    """Return *count* evenly-distributed jittered points on a sphere of *radius*.

    The sphere surface is divided into equal-area cells in (cos θ, φ) space:
    ``rows`` latitude bands × ``cols`` longitude slices, with ``rows × cols ≥
    count``.  One stratified-random sample is drawn inside each cell, the
    cells are shuffled, and the first *count* are returned.
    """
    rows   = max(1, round(float(count) ** 0.5 / 2.0 ** 0.5))
    cols   = max(1, int(np.ceil(count / rows)))
    n_cell = rows * cols

    ct_edges = np.linspace(-1.0, 1.0, rows + 1)
    ph_edges = np.linspace(0.0, 2.0 * np.pi, cols + 1)

    ct = (ct_edges[:-1, None]
          + rng.uniform(0.0, 1.0, (rows, cols)) * np.diff(ct_edges)[:, None]).ravel()
    ph = (ph_edges[None, :-1]
          + rng.uniform(0.0, 1.0, (rows, cols)) * np.diff(ph_edges)[None, :]).ravel()

    st   = np.sqrt(np.clip(1.0 - ct ** 2, 0.0, 1.0))
    dirs = np.stack([st * np.cos(ph), st * np.sin(ph), ct], axis=1)

    idx = rng.permutation(n_cell)[:count]
    return dirs[idx] * radius


# ── Scene assembly ─────────────────────────────────────────────────────────────

def build_debug_mesh(
    *,
    count:              int          = 30,
    seed:               int          = 42,
    contact_angle_deg:  float | None = None,
    length_mm:          float        = LEAF_LENGTH_MM_DEFAULT,
    width_mm:           float        = LEAF_WIDTH_MM_DEFAULT,
    lift_mm:            float        = 1.0,
) -> trimesh.Trimesh:
    """Sphere + trunk + *count* randomly-placed leaf solids."""
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

    rng = np.random.default_rng(seed)
    pts = _jitter_sphere_pts(count, SPHERE_RADIUS_MM, rng)
    contact_angle_rad = None if contact_angle_deg is None else np.radians(contact_angle_deg)

    n_ok_total = n_fail_total = 0
    for i, pt in enumerate(pts):
        base_pos, T0, up_hint = leaf_placement_from_surface(sphere, pt)

        leaf, wall_faces = place_leaf_on_sphere(
            base_pos, T0, up_hint, SPHERE_RADIUS_MM, support_mesh,
            contact_angle_rad=contact_angle_rad,
            length_mm=length_mm, width_mm=width_mm,
            fold_angle_deg=LEAF_FOLD_DEG, curl_deg=LEAF_CURL_DEG,
            lift_mm=lift_mm,
        )

        tag(leaf, debug_material(2 + (i % 8)))
        color_leaf_walls_by_fdm(leaf, wall_faces, support_mesh)

        wall_colors = leaf.visual.face_colors[list(wall_faces)]
        n_ok_total   += int(np.sum(wall_colors[:, 0] < 128))
        n_fail_total += len(wall_faces) - int(np.sum(wall_colors[:, 0] < 128))
        parts.append(leaf)

    contact_label = "auto" if contact_angle_deg is None else f"{contact_angle_deg:.0f}°"
    print(f"  {count} leaves  contact={contact_label}  curl={LEAF_CURL_DEG:.0f}°  lift={lift_mm:.1f}mm  seed={seed}")
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
    parser.add_argument("--contact-angle-deg", type=float, default=None,
                        help="Fix contact angle (degrees). Omit for auto (default)")
    parser.add_argument("--length-mm", type=float, default=LEAF_LENGTH_MM_DEFAULT,
                        help=f"Leaf length mm (default: {LEAF_LENGTH_MM_DEFAULT})")
    parser.add_argument("--width-mm", type=float, default=LEAF_WIDTH_MM_DEFAULT,
                        help=f"Leaf width mm (default: {LEAF_WIDTH_MM_DEFAULT})")
    parser.add_argument("--lift-mm", type=float, default=1.0,
                        help="Tip lift in mm along leaf normal (default: 1.0)")
    args = parser.parse_args()

    mesh = build_debug_mesh(
        count=args.count,
        seed=args.seed,
        contact_angle_deg=args.contact_angle_deg,
        length_mm=args.length_mm,
        width_mm=args.width_mm,
        lift_mm=args.lift_mm,
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    export_color_stl(mesh, args.output)

    print(f"Wrote {args.output}")
    print(f"  {len(mesh.vertices):,} vertices · {len(mesh.faces):,} faces · "
          f"watertight={mesh.is_watertight}")


if __name__ == "__main__":
    main()
