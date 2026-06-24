#!/usr/bin/env python3
"""Debug: sphere + trunk + leaf solids covering the full sphere surface.

Leaves are placed on a jittered latitude/longitude grid:
  - Row height  = leaf_length × (1 − v_overlap)  arc on the sphere surface.
  - Column width = leaf_width  × (1 − h_overlap)  arc, varied per latitude band
                   so coverage stays even from equator to pole.
  - Each cell gets one leaf at a random position within the cell (jitter).

Run from the repository root::

    python src/scripts/generate-debug-leaf-placement-random-stl.py
    python src/scripts/generate-debug-leaf-placement-random-stl.py --v-overlap 0.5
    python src/scripts/generate-debug-leaf-placement-random-stl.py --seed 7 --length-mm 8
"""
from __future__ import annotations

import argparse
import pathlib

import numpy as np
import trimesh

from dharmatiles.core.color import Material, debug_material, export_color_stl, tag
from dharmatiles.trees.leaf import (
    LEAF_LENGTH_MM_DEFAULT,
    LEAF_WIDTH_MM_DEFAULT,
    leaf_placement_from_surface,
    place_leaf_on_sphere,
)
from _leaf_debug import color_leaf_walls_by_fdm

# ── Constants ──────────────────────────────────────────────────────────────────

_REPO_ROOT       = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_OUTPUT   = _REPO_ROOT / "stl/debug/leaf-placement-debug-random.stl"
SPHERE_RADIUS_MM = 30.0
TRUNK_HEIGHT_MM  = 40.0
TRUNK_RADIUS_MM  = 6.0
BASE_SIZE_MM     = 35.0
BASE_DEPTH_MM    = 6.0
LEAF_FOLD_DEG    = 6.0
LEAF_CURL_DEG    = 40.0


# ── Placement helpers ──────────────────────────────────────────────────────────

def _grid_sphere_pts(
    radius:    float,
    length_mm: float,
    width_mm:  float,
    h_overlap: float,
    v_overlap: float,
    rng:       np.random.Generator,
    jitter_xy: float = 0.20,
) -> np.ndarray:
    """Jittered latitude/longitude grid points on a sphere.

    Rows start at the north pole and step by length_mm × (1 − v_overlap) arc.
    Each row's column count is rounded to fit the circumference at that latitude;
    leaves are distributed evenly so they fill the circle.
    Odd rows are offset by half a column width (brick pattern).
    Each point is perturbed by ±jitter_xy of the local cell dimensions.
    """
    d_theta = length_mm * (1.0 - v_overlap) / radius   # row step, radians
    col_arc = width_mm  * (1.0 - h_overlap)            # target arc per column

    pts: list[np.ndarray] = []
    row_idx = 0
    theta   = 1e-4                                      # start just off the north pole
    while theta < 0.8 * np.pi:
        # Leaves are widest at s=1/3 from their base (tip points toward south
        # pole).  Pack by the circumference at THAT latitude so widest points
        # of adjacent leaves butt up at h_overlap=0.
        theta_peak = min(theta + (length_mm / 3.0) / radius, np.pi - 1e-6)
        circ_peak  = 2.0 * np.pi * radius * np.sin(theta_peak)
        n_cols = max(1, round(circ_peak / col_arc))
        d_phi  = 2.0 * np.pi / n_cols
        offset = 0.5 * d_phi if row_idx % 2 else 0.0

        for j in range(n_cols):
            phi = offset + j * d_phi
            jt  = float(np.clip(
                theta + rng.uniform(-jitter_xy, jitter_xy) * d_theta,
                1e-6, np.pi - 1e-6,
            ))
            jp  = phi + rng.uniform(-jitter_xy, jitter_xy) * d_phi
            sin_t = np.sin(jt)
            pts.append(np.array([
                radius * sin_t * np.cos(jp),
                radius * sin_t * np.sin(jp),
                radius * np.cos(jt),
            ]))

        theta   += d_theta
        row_idx += 1

    return np.array(pts)


# ── Scene assembly ─────────────────────────────────────────────────────────────

def build_debug_mesh(
    *,
    h_overlap:          float        = 0.2,
    v_overlap:          float        = 0.5,
    seed:               int          = 42,
    contact_angle_deg:  float | None = None,
    length_mm:          float        = LEAF_LENGTH_MM_DEFAULT,
    width_mm:           float        = LEAF_WIDTH_MM_DEFAULT,
    lift_mm:            float        = 3.0,
) -> trimesh.Trimesh:
    """Sphere + trunk + full-coverage leaf solids."""
    sphere = trimesh.creation.icosphere(subdivisions=4, radius=SPHERE_RADIUS_MM)
    sphere.fix_normals()
    tag(sphere, Material.DEBUG_COLOR_1)

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
    tag(trunk, Material.DEBUG_COLOR_1)

    base = trimesh.creation.box(extents=[BASE_SIZE_MM, BASE_SIZE_MM, BASE_DEPTH_MM])
    base.apply_translation([0.0, 0.0, -(SPHERE_RADIUS_MM + TRUNK_HEIGHT_MM + BASE_DEPTH_MM / 2.0)])
    base.fix_normals()
    tag(base, Material.DEBUG_COLOR_1)

    support_mesh = trimesh.util.concatenate([sphere, trunk])
    parts: list[trimesh.Trimesh] = [sphere, trunk, base]

    rng = np.random.default_rng(seed)
    pts = _grid_sphere_pts(
        SPHERE_RADIUS_MM, length_mm, width_mm, h_overlap, v_overlap, rng
    )
    contact_angle_rad = None if contact_angle_deg is None else np.radians(contact_angle_deg)

    n_ok_total = n_fail_total = 0
    for i, pt in enumerate(pts):
        base_pos, T0, up_hint = leaf_placement_from_surface(sphere, pt)

        # Angle jitter: rotate T0 around up_hint by ±10°.
        alpha     = float(rng.uniform(-np.radians(20.0), np.radians(20.0)))
        T0_rot    = np.cos(alpha) * T0 + np.sin(alpha) * np.cross(up_hint, T0)
        T0        = T0_rot / (np.linalg.norm(T0_rot) + 1e-12)

        # Roll jitter: rotate up_hint around T0 (spine axis) by ±10°,
        # tilting one leaf edge up and the other down.
        roll      = float(rng.uniform(-np.radians(20.0), np.radians(20.0)))
        up_rolled = np.cos(roll) * up_hint + np.sin(roll) * np.cross(T0, up_hint)
        up_hint   = up_rolled / (np.linalg.norm(up_rolled) + 1e-12)

        # Lift jitter: ±18% of lift_mm.
        leaf_lift = lift_mm * float(rng.uniform(0.82, 1.18))

        # Size jitter: ±5% applied uniformly to length and width.
        size_scale  = float(rng.uniform(0.95, 1.05))
        leaf_length = length_mm * size_scale
        leaf_width  = width_mm  * size_scale

        leaf, wall_faces = place_leaf_on_sphere(
            base_pos, T0, up_hint, SPHERE_RADIUS_MM, support_mesh,
            contact_angle_rad=contact_angle_rad,
            length_mm=leaf_length, width_mm=leaf_width,
            fold_angle_deg=LEAF_FOLD_DEG, curl_deg=LEAF_CURL_DEG,
            lift_mm=leaf_lift,
        )

        tag(leaf, debug_material(i))
        color_leaf_walls_by_fdm(leaf, wall_faces, support_mesh)

        wall_colors = leaf.visual.face_colors[list(wall_faces)]
        n_ok_total   += int(np.sum(wall_colors[:, 0] < 128))
        n_fail_total += len(wall_faces) - int(np.sum(wall_colors[:, 0] < 128))
        parts.append(leaf)

    print(f"  {len(pts)} leaves  h_overlap={h_overlap:.0%}  v_overlap={v_overlap:.0%}  "
          f"curl={LEAF_CURL_DEG:.0f}°  lift={lift_mm:.1f}mm  seed={seed}")
    print(f"  wall faces: {n_ok_total} green  {n_fail_total} red  "
          f"({'%.0f' % (100*n_ok_total/(n_ok_total+n_fail_total+1e-9))}% printable)")
    return trimesh.util.concatenate(parts)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", "-o", type=pathlib.Path, default=DEFAULT_OUTPUT,
                        help=f"Output STL path (default: {DEFAULT_OUTPUT})")
    parser.add_argument("--h-overlap", type=float, default=0.2,
                        help="Horizontal (side-to-side) overlap fraction 0–1 (default: 0)")
    parser.add_argument("--v-overlap", type=float, default=0.5,
                        help="Vertical (tip-to-base shingle) overlap fraction 0–1 (default: 0.3)")
    parser.add_argument("--seed", type=int, default=42,
                        help="Random seed (default: 42)")
    parser.add_argument("--contact-angle-deg", type=float, default=None,
                        help="Fix contact angle (degrees). Omit for auto (default)")
    parser.add_argument("--length-mm", type=float, default=LEAF_LENGTH_MM_DEFAULT,
                        help=f"Leaf length mm (default: {LEAF_LENGTH_MM_DEFAULT})")
    parser.add_argument("--width-mm", type=float, default=LEAF_WIDTH_MM_DEFAULT,
                        help=f"Leaf width mm (default: {LEAF_WIDTH_MM_DEFAULT})")
    parser.add_argument("--lift-mm", type=float, default=3.0,
                        help="Tip lift in mm along leaf normal (default: 3.0)")
    args = parser.parse_args()

    mesh = build_debug_mesh(
        h_overlap=args.h_overlap,
        v_overlap=args.v_overlap,
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
