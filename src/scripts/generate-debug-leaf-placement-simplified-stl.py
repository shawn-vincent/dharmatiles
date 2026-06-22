#!/usr/bin/env python3
"""Simplified debug: sphere + trunk + four solid leaf solids.

Each leaf surface is closed into a solid by projecting its perimeter inward
along -normal by LEAF_ROOT_DEPTH_MM, forming a root ring that pierces the
parent mesh.  Walls connect the leaf perimeter to the root ring; a bottom cap
closes the root end.

Each leaf is placed with its base on the sphere surface and tilted as far down
as possible without any part of the leaf penetrating the sphere (plus a small
tolerance gap).

Run from the repository root::

    python src/scripts/generate-debug-leaf-placement-simplified-stl.py
"""
from __future__ import annotations

import argparse
import pathlib

import numpy as np
import trimesh

from dharmatiles.core.color import debug_material, export_color_stl, tag
from dharmatiles.trees.leaf import (
    build_leaf_surface,
    find_max_dip_for_sphere,
    find_tip_root,
    solidify_leaf,
)
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

# Minimum clearance from the sphere surface for any non-base-zone leaf vertex.
TOLERANCE_GAP_MM = 0.0

PLACEMENTS: tuple[tuple[str, float], ...] = (
    ("top",           0.00),
    ("upper-quarter", 0.25),
    ("equator",       0.50),
    ("lower-quarter", 0.75),
)


# ── Sphere-specific geometry helpers ──────────────────────────────────────────

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



def _build_tilted_leaf(
    base:           np.ndarray,
    L:              np.ndarray,
    surface_normal: np.ndarray,
    theta:          float,
    curl_deg:       float = LEAF_CURL_DEG,
) -> tuple[trimesh.Trimesh, np.ndarray]:
    """Build the leaf at tilt angle θ; return (surface, midplane_normal)."""
    c, s = np.cos(theta), np.sin(theta)

    up_hint  = c * surface_normal + s * L
    up_hint /= np.linalg.norm(up_hint) + 1e-12
    tangent  = -c * L - s * surface_normal
    tangent /= np.linalg.norm(tangent) + 1e-12

    leaf = build_leaf_surface(
        base_pos=base,
        tangent=tangent,
        length_mm=LEAF_LENGTH_MM,
        width_mm=LEAF_WIDTH_MM,
        fold_angle_deg=LEAF_FOLD_DEG,
        curl_deg=curl_deg,
        up_hint=up_hint,
    )

    verts   = leaf.vertices
    tip_pt  = verts[np.argmax((verts - base) @ tangent)]
    midrib  = tip_pt - base;  midrib /= np.linalg.norm(midrib) + 1e-12
    lateral = np.cross(surface_normal, L);  lateral /= np.linalg.norm(lateral) + 1e-12
    normal  = np.cross(midrib, lateral);    normal  /= np.linalg.norm(normal)  + 1e-12
    if np.dot(normal, surface_normal) < 0:
        normal = -normal
    return leaf, normal



def _build_leaf_for_sphere(
    base:           np.ndarray,
    L:              np.ndarray,
    surface_normal: np.ndarray,
    sphere_radius:  float,
    support_mesh:   trimesh.Trimesh,
) -> tuple[trimesh.Trimesh, np.ndarray, float, float, np.ndarray | None]:
    """Find tilt; keep curl if the tip raycast hits the parent mesh, else drop it.

    Casts a ray from the leaf tip at the minimum FDM-printable angle toward
    the parent mesh.  If the mesh is hit within the threshold the curl is kept
    and the hit point is returned as the tip root; otherwise the leaf is
    rebuilt with curl = 0 and the raycast is retried.

    Returns (leaf_surface, normal, theta, curl_deg_used, tip_root).
    ``tip_root`` is ``None`` when the zero-curl leaf's tip also misses
    (edge case: pass straight to :func:`solidify_leaf` for default behaviour).
    """
    for curl_deg in (LEAF_CURL_DEG, 0.0):
        # T0 = -L: the flat tangent pointing the tip downward along the sphere.
        # cross(-T0, up_hint) = cross(L, surface_normal) — Rodrigues axis matches.
        theta = find_max_dip_for_sphere(
            base, -L, surface_normal, sphere_radius,
            clearance_mm=TOLERANCE_GAP_MM,
            length_mm=LEAF_LENGTH_MM,
            width_mm=LEAF_WIDTH_MM,
            fold_angle_deg=LEAF_FOLD_DEG,
            curl_deg=curl_deg,
        )
        leaf_surf, normal = _build_tilted_leaf(base, L, surface_normal, theta,
                                               curl_deg=curl_deg)
        tip_root = find_tip_root(leaf_surf, normal, support_mesh)
        if curl_deg == 0.0 or tip_root is not None:
            return leaf_surf, normal, theta, curl_deg, tip_root
    raise AssertionError("zero-curl leaf construction did not return")


# ── Scene assembly ─────────────────────────────────────────────────────────────

def build_debug_mesh(*, debug_solidify: bool = False) -> trimesh.Trimesh:
    """Sphere + trunk + four tilted bare leaf surfaces."""
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
    depth = 1.0   # root embedding depth (mm)
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

        leaf_surf, normal, theta, curl_used, tip_root = _build_leaf_for_sphere(
            base, L_base, n_base, SPHERE_RADIUS_MM, support_mesh,
        )

        leaf, wall_faces = solidify_leaf(leaf_surf, normal, depth, tip_root=tip_root)
        tag(leaf, debug_material(2 + leaf_index))
        color_leaf_walls_by_fdm(leaf, wall_faces, support_mesh)

        wall_colors = leaf.visual.face_colors[list(wall_faces)]
        n_ok   = int(np.sum(wall_colors[:, 0] < 128))
        n_fail = len(wall_faces) - n_ok
        curl_note = f"  curl={curl_used:.0f}°" if curl_used != LEAF_CURL_DEG else ""
        print(f"  {name:13s}  tilt={np.degrees(theta):5.1f}°{curl_note}  "
              f"normal=({normal[0]:+.3f}, {normal[1]:+.3f}, {normal[2]:+.3f})  "
              f"wall green={n_ok} red={n_fail}  watertight={leaf.is_watertight}")
        parts.append(leaf)

    return trimesh.util.concatenate(parts)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", "-o", type=pathlib.Path, default=DEFAULT_OUTPUT,
                        help=f"Output colour-STL path (default: {DEFAULT_OUTPUT})")
    parser.add_argument(
        "--debug-solidify",
        action="store_true",
        help="Print perimeter/root coordinates around each solidified leaf tip",
    )
    args = parser.parse_args()

    mesh = build_debug_mesh(debug_solidify=args.debug_solidify)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    export_color_stl(mesh, args.output)

    print(f"Wrote {args.output}")
    print(f"  {len(mesh.vertices):,} vertices · {len(mesh.faces):,} faces · "
          f"watertight={mesh.is_watertight}")


if __name__ == "__main__":
    main()
