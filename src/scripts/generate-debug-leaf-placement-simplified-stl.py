#!/usr/bin/env python3
"""Simplified debug: sphere + trunk + four solid leaf solids.

Each leaf is placed at a fixed latitude, pressed as far into the sphere as
possible without penetrating it, then solidified.  Wall faces are coloured
green (printable) or red (overhang) by FDM analysis.

Run from the repository root::

    python src/scripts/generate-debug-leaf-placement-simplified-stl.py
    python src/scripts/generate-debug-leaf-placement-simplified-stl.py --diagnostic

``--diagnostic`` generates a second STL with 5-color per-category colouring:
  lime   = SEED_OK   (centroid grounded + angle_ok)
  purple = SEED_OVER (centroid grounded + bad angle)
  green  = BFS       (not grounded but reachable via BFS)
  orange = ISOLATED  (angle_ok but cut off from support)
  red    = FAIL      (definite overhang)

It also prints a per-face table (sorted by centroid Z) for each leaf so you
can inspect exactly how every face was classified.
"""
from __future__ import annotations

import argparse
import pathlib

import numpy as np
import trimesh

from dharmatiles.core.color import Material, debug_material, export_color_stl, tag
from dharmatiles.trees.leaf import place_leaf_on_sphere
from _leaf_debug import (
    color_leaf_walls_by_fdm,
    color_leaf_walls_diagnostic,
    print_leaf_diagnostic,
)

# ── Constants ──────────────────────────────────────────────────────────────────

_REPO_ROOT       = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_OUTPUT   = _REPO_ROOT / "stl/debug/leaf-placement-debug-simplified.stl"
DEFAULT_DIAG_OUT = _REPO_ROOT / "stl/debug/leaf-placement-debug-simplified-diagnostic.stl"
SPHERE_RADIUS_MM = 30.0
TRUNK_HEIGHT_MM  = 40.0
TRUNK_RADIUS_MM  = 6.0
BASE_SIZE_MM     = 35.0
BASE_DEPTH_MM    = 6.0
LEAF_LENGTH_MM   = 9.0
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

def _make_support(
    sphere: trimesh.Trimesh,
    trunk:  trimesh.Trimesh,
) -> trimesh.Trimesh:
    return trimesh.util.concatenate([sphere, trunk])


def _build_sphere_trunk() -> tuple[trimesh.Trimesh, trimesh.Trimesh, trimesh.Trimesh]:
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

    return sphere, trunk, base


def _place_leaves(
    support_mesh: trimesh.Trimesh,
    *,
    diagnostic: bool = False,
) -> tuple[list[trimesh.Trimesh], list[dict | None]]:
    """Build and color all four leaves.

    Returns (leaf_meshes, diag_infos).  diag_infos entries are None when
    ``diagnostic=False``.
    """
    leaves:     list[trimesh.Trimesh] = []
    diag_infos: list[dict | None]     = []

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

        if diagnostic:
            info = color_leaf_walls_diagnostic(leaf, wall_faces, support_mesh)
            diag_infos.append(info)
        else:
            color_leaf_walls_by_fdm(leaf, wall_faces, support_mesh)
            diag_infos.append(None)

            wall_colors = leaf.visual.face_colors[list(wall_faces)]
            n_ok   = int(np.sum(wall_colors[:, 0] < 128))
            n_fail = len(wall_faces) - n_ok
            print(f"  {name:13s}  wall green={n_ok} red={n_fail}  watertight={leaf.is_watertight}")

        leaves.append(leaf)

    return leaves, diag_infos


def build_debug_mesh(*, diagnostic: bool = False) -> trimesh.Trimesh:
    """Sphere + trunk + four tilted leaf solids at fixed latitudes."""
    sphere, trunk, base = _build_sphere_trunk()
    support_mesh  = _make_support(sphere, trunk)
    leaves, _     = _place_leaves(support_mesh, diagnostic=diagnostic)
    return trimesh.util.concatenate([sphere, trunk, base] + leaves)


def build_debug_mesh_diagnostic() -> tuple[trimesh.Trimesh, list[dict]]:
    """Like :func:`build_debug_mesh` but returns (mesh, diag_infos)."""
    sphere, trunk, base = _build_sphere_trunk()
    support_mesh  = _make_support(sphere, trunk)
    leaves, diag_infos = _place_leaves(support_mesh, diagnostic=True)
    mesh = trimesh.util.concatenate([sphere, trunk, base] + leaves)
    return mesh, [d for d in diag_infos if d is not None]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", "-o", type=pathlib.Path, default=DEFAULT_OUTPUT,
                        help=f"Output colour-STL path (default: {DEFAULT_OUTPUT})")
    parser.add_argument("--diagnostic", action="store_true",
                        help="5-color diagnostic mode: classify faces into 5 categories, "
                             "print per-face table, write a separate -diagnostic.stl")
    parser.add_argument("--diag-output", type=pathlib.Path, default=DEFAULT_DIAG_OUT,
                        help=f"Diagnostic STL path (default: {DEFAULT_DIAG_OUT})")
    args = parser.parse_args()

    if args.diagnostic:
        print("Building diagnostic mesh (5-color)…")
        sphere, trunk, base = _build_sphere_trunk()
        support_mesh  = _make_support(sphere, trunk)

        diag_parts: list[trimesh.Trimesh] = [sphere, trunk, base]
        all_infos: list[dict] = []

        for leaf_index, (name, fraction_down) in enumerate(PLACEMENTS):
            polar          = np.pi * fraction_down
            surface_normal = np.array([np.sin(polar), 0.0, np.cos(polar)])
            midpoint       = SPHERE_RADIUS_MM * surface_normal
            L_mid, _       = _leaf_frame(surface_normal)

            if fraction_down == 0.0:
                base, n_base, L_base = midpoint, surface_normal, L_mid
            else:
                base, n_base, L_base = _base_for_midpoint(midpoint, L_mid, SPHERE_RADIUS_MM)

            leaf, wall_faces = place_leaf_on_sphere(
                base, -L_base, n_base, SPHERE_RADIUS_MM, support_mesh,
                length_mm=LEAF_LENGTH_MM, width_mm=LEAF_WIDTH_MM,
                fold_angle_deg=LEAF_FOLD_DEG, curl_deg=LEAF_CURL_DEG,
            )
            tag(leaf, debug_material(leaf_index))

            info = color_leaf_walls_diagnostic(leaf, wall_faces, support_mesh)
            all_infos.append(info)
            diag_parts.append(leaf)

        # Print per-face tables
        for (name, _fraction), info in zip(PLACEMENTS, all_infos):
            print_leaf_diagnostic(name, info)

        # Write diagnostic STL
        diag_mesh = trimesh.util.concatenate(diag_parts)
        args.diag_output.parent.mkdir(parents=True, exist_ok=True)
        export_color_stl(diag_mesh, args.diag_output)
        print(f"Wrote {args.diag_output}")
        print(f"  {len(diag_mesh.vertices):,} vertices · {len(diag_mesh.faces):,} faces · "
              f"watertight={diag_mesh.is_watertight}")

    else:
        mesh = build_debug_mesh()
        args.output.parent.mkdir(parents=True, exist_ok=True)
        export_color_stl(mesh, args.output)
        print(f"Wrote {args.output}")
        print(f"  {len(mesh.vertices):,} vertices · {len(mesh.faces):,} faces · "
              f"watertight={mesh.is_watertight}")


if __name__ == "__main__":
    main()
