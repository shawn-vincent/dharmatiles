#!/usr/bin/env python3
"""Simplified debug: sphere + trunk + four solid leaf solids.

Each leaf is placed at a fixed latitude, pressed as far into the sphere as
possible without penetrating it, then solidified.  Wall faces are coloured
green (printable) or red (overhang) by FDM analysis.

Run from the repository root::

    python src/scripts/generate-debug-leaf-placement-simplified-stl.py
"""
from __future__ import annotations

import argparse
import pathlib

import numpy as np
import trimesh

from dharmatiles.core.color import Material, debug_material, export_color_stl, tag
from dharmatiles.trees.leaf import place_leaf_on_sphere
from _leaf_debug import color_leaf_walls_by_fdm

# ── Constants ──────────────────────────────────────────────────────────────────

DEFAULT_OUTPUT   = pathlib.Path("stl/debug/leaf-placement-debug-simplified.stl")
SPHERE_RADIUS_MM = 12.0
TRUNK_HEIGHT_MM  = SPHERE_RADIUS_MM * 2.0 / 3.0
TRUNK_RADIUS_MM  = TRUNK_HEIGHT_MM / 2.0
LEAF_LENGTH_MM   = 6.0
LEAF_WIDTH_MM    = LEAF_LENGTH_MM * 2.0 / 3.0
LEAF_FOLD_DEG    = 6.0
LEAF_CURL_DEG    = 40.0

PLACEMENTS: tuple[tuple[str, float], ...] = (
    ("top",           0.00),
    ("upper-quarter", 0.25),
    ("equator",       0.50),
    ("lower-quarter", 0.75),
)


# ── Placement helpers ──────────────────────────────────────────────────────────

def _leaf_frame(surface_normal: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Return (L, T) — leaf longitudinal and lateral axes in the tangent plane.

    L is world-up projected onto the tangent plane (falls back to +X at poles).
    T = cross(surface_normal, L).
    """
    n    = surface_normal / (np.linalg.norm(surface_normal) + 1e-12)
    up   = np.array([0.0, 0.0, 1.0])
    proj = up - np.dot(up, n) * n
    if np.linalg.norm(proj) < 0.1:
        proj = np.array([1.0, 0.0, 0.0]) - np.dot([1.0, 0.0, 0.0], n) * n
    L = proj / (np.linalg.norm(proj) + 1e-12)
    T = np.cross(n, L);  T /= np.linalg.norm(T) + 1e-12
    return L, T


def _base_for_midpoint(
    midpoint:      np.ndarray,
    L:             np.ndarray,
    sphere_radius: float,
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Find the base point on the sphere whose flat-leaf midpoint ≈ midpoint.

    Returns (base, surface_normal_at_base, L_at_base).
    """
    candidate   = midpoint + (LEAF_LENGTH_MM / 2.0) * L
    base_normal = candidate / (np.linalg.norm(candidate) + 1e-12)
    base        = sphere_radius * base_normal
    L_base, _   = _leaf_frame(base_normal)
    return base, base_normal, L_base


# ── Scene assembly ─────────────────────────────────────────────────────────────

def build_debug_mesh() -> trimesh.Trimesh:
    """Sphere + trunk + four tilted leaf solids at fixed latitudes."""
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

    support_mesh = trimesh.util.concatenate([sphere, trunk])
    parts: list[trimesh.Trimesh] = [sphere, trunk]

    for leaf_index, (name, fraction_down) in enumerate(PLACEMENTS):
        polar          = np.pi * fraction_down
        surface_normal = np.array([np.sin(polar), 0.0, np.cos(polar)])
        midpoint       = SPHERE_RADIUS_MM * surface_normal
        L_mid, _       = _leaf_frame(surface_normal)

        if fraction_down == 0.0:
            base, n_base, L_base = midpoint, surface_normal, L_mid
        else:
            base, n_base, L_base = _base_for_midpoint(midpoint, L_mid, SPHERE_RADIUS_MM)

        # T0 = -L_base: flat tangent pointing the tip downward along the sphere.
        leaf, wall_faces = place_leaf_on_sphere(
            base, -L_base, n_base, SPHERE_RADIUS_MM, support_mesh,
            length_mm=LEAF_LENGTH_MM, width_mm=LEAF_WIDTH_MM,
            fold_angle_deg=LEAF_FOLD_DEG, curl_deg=LEAF_CURL_DEG,
        )

        tag(leaf, debug_material(leaf_index))
        color_leaf_walls_by_fdm(leaf, wall_faces, support_mesh)

        wall_colors = leaf.visual.face_colors[list(wall_faces)]
        n_ok   = int(np.sum(wall_colors[:, 0] < 128))
        n_fail = len(wall_faces) - n_ok
        print(f"  {name:13s}  wall green={n_ok} red={n_fail}  watertight={leaf.is_watertight}")
        parts.append(leaf)

    return trimesh.util.concatenate(parts)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", "-o", type=pathlib.Path, default=DEFAULT_OUTPUT,
                        help=f"Output colour-STL path (default: {DEFAULT_OUTPUT})")
    args = parser.parse_args()

    mesh = build_debug_mesh()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    export_color_stl(mesh, args.output)

    print(f"Wrote {args.output}")
    print(f"  {len(mesh.vertices):,} vertices · {len(mesh.faces):,} faces · "
          f"watertight={mesh.is_watertight}")


if __name__ == "__main__":
    main()
