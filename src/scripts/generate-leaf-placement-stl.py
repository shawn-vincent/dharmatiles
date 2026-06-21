#!/usr/bin/env python3
"""Generate a debug sphere with branchlet+leaf assemblies at four latitudes.

Each branchlet grows straight out along the surface normal.  The leaf surface
is stitched directly to the open tip of the loft — loft walls, root cap, and
leaf surface form a single closed mesh with no overlapping parts.

Geometry that violates the FDM floor-angle rule is coloured debug-red
(DEBUG_COLOR_0, reserved for failures).  Passing parts use slots 1+.

Run from the repository root::

    python src/scripts/generate-leaf-placement-stl.py
"""
from __future__ import annotations

import argparse
import pathlib

import numpy as np
import trimesh

from dharmatiles.core.color import Material, debug_material, export_color_stl, tag
from dharmatiles.trees.leaf import (
    _LEAF_N_LONG,
    _LEAF_CREASE_SHARPNESS,
    _LEAF_LONG_T_PEAK,
    _leaf_width_profile,
)

# ── Constants ──────────────────────────────────────────────────────────────────

DEFAULT_OUTPUT    = pathlib.Path("debug/leaf-placement.stl")
SPHERE_RADIUS_MM  = 12.0
LEAF_LENGTH_MM    = 6.0
LEAF_WIDTH_MM     = 3.5
LEAF_FOLD_DEG     = 3.0
EMBED_DEPTH_MM    = 1.6
FLOOR_ANGLE_DEG   = 45.0
ROOT_DIAMETER_FRACTION = 0.90   # root-ring diameter as fraction of max leaf dim

_N_PERIM      = 2 + 2 * (_LEAF_N_LONG - 1)   # vertices on the leaf perimeter ring
_N_LOFT_RINGS = 8

FAIL_MATERIAL: Material = debug_material(0)   # red — reserved for FDM failures

PLACEMENTS: tuple[tuple[str, float], ...] = (
    ("top",           0.00),
    ("upper-quarter", 0.25),
    ("equator",       0.50),
    ("lower-quarter", 0.75),
)


# ── Geometry helpers ───────────────────────────────────────────────────────────

def _leaf_frame(surface_normal: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Return (L, T) — leaf long and lateral axes in the tangent plane.

    L is world-up projected onto the plane perpendicular to *surface_normal*
    (falls back to +X at the poles).  T = cross(surface_normal, L).
    """
    n    = surface_normal / (np.linalg.norm(surface_normal) + 1e-12)
    up   = np.array([0.0, 0.0, 1.0])
    proj = up - np.dot(up, n) * n
    if np.linalg.norm(proj) < 0.1:
        proj = np.array([1.0, 0.0, 0.0]) - np.dot([1, 0, 0], n) * n
    L = proj / (np.linalg.norm(proj) + 1e-12)
    T = np.cross(n, L);  T /= np.linalg.norm(T) + 1e-12
    return L, T


def _leaf_perimeter_local(
    length_mm: float,
    width_mm:  float,
    fold_deg:  float,
) -> np.ndarray:
    """Leaf outline in canonical local space, centred at the origin.

    Local axes: long axis = +X, lateral = +Y, leaf normal = +Z.
    Base (rounded end) at X = −length/2; pointed tip at X = +length/2.

    The ring is rolled so j=0 is the pointed tip (+X), matching root-ring j=0
    (which sits in the −u direction).  This keeps the loft twist-free.

    Returns ``(_N_PERIM, 3)``.
    """
    half   = length_mm / 2.0
    s_int  = np.linspace(0.0, 1.0, _LEAF_N_LONG + 1)[1:-1]
    x_mid  = s_int * length_mm - half
    w_s    = width_mm * _leaf_width_profile(s_int)
    fold_h = (np.tanh(_LEAF_CREASE_SHARPNESS) * w_s
              * np.tan(np.radians(fold_deg))
              * (s_int ** 0.5 * (1.0 - s_int) ** 1.5) / _LEAF_LONG_T_PEAK)

    midribs    = np.column_stack([x_mid, np.zeros(len(s_int)), np.zeros(len(s_int))])
    right_edge = midribs + np.column_stack([np.zeros(len(s_int)),  w_s, fold_h])
    left_edge  = midribs + np.column_stack([np.zeros(len(s_int)), -w_s, fold_h])

    perim = np.concatenate([
        [[-half, 0.0, 0.0]],    # base (rounded end)
        right_edge,
        [[ half, 0.0, 0.0]],    # pointed tip
        left_edge[::-1],
    ])
    # Roll so j=0 is the pointed tip (local +X → world −L), matching root ring.
    return np.roll(perim, -(_N_PERIM // 2), axis=0)


def _ellipse_ring(
    center: np.ndarray,
    u: np.ndarray, v: np.ndarray,
    half_a: float, half_b: float,
    n_pts: int,
) -> np.ndarray:
    """Elliptical ring of *n_pts* vertices centred at *center*.

    *half_a* along *u*, *half_b* along *v*.  ``half_a == half_b`` gives a circle.
    """
    t = np.linspace(0, 2 * np.pi, n_pts, endpoint=False)
    return center + half_a * np.cos(t[:, None]) * u + half_b * np.sin(t[:, None]) * v


def _is_printable(surface_normal: np.ndarray) -> bool:
    """True iff the branchlet axis clears the FDM floor angle.

    For a straight frustum the worst wall-face normal is ⊥ to the axis, so
    axis elevation ≥ floor_angle is both necessary and sufficient.
    """
    n = surface_normal / (np.linalg.norm(surface_normal) + 1e-12)
    return float(n[2]) >= np.sin(np.radians(FLOOR_ANGLE_DEG)) - 1e-6


# ── Branchlet + leaf (single closed mesh) ────────────────────────────────────

def build_simple_branchlet_and_leaf(
    attachment_point: np.ndarray,
    surface_normal:   np.ndarray,
) -> tuple[trimesh.Trimesh, bool]:
    """Straight-along-normal loft stitched to a leaf surface as one closed mesh.

    The loft morphs from a circle at the root to the leaf perimeter at the tip.
    The leaf surface fans from the perimeter ring inward to the centroid,
    closing the mesh.  No overlapping parts; shared vertices at the junction.

    Returns ``(mesh, is_fail)``.
    """
    n    = surface_normal / (np.linalg.norm(surface_normal) + 1e-12)
    L, T = _leaf_frame(n)

    root_radius = 0.5 * ROOT_DIAMETER_FRACTION * max(LEAF_LENGTH_MM, 2.0 * LEAF_WIDTH_MM)
    root_center = attachment_point - EMBED_DEPTH_MM * n
    tip_pos     = root_center + (EMBED_DEPTH_MM + LEAF_LENGTH_MM) * n

    # Root ring: circle in the plane perpendicular to n.
    root_ring = _ellipse_ring(root_center, -L, -T, root_radius, root_radius, _N_PERIM)

    # Tip ring: leaf perimeter in local space, transformed to world.
    # Local +X → −L (pointed tip faces gravity), +Y → −T, +Z → n.
    R        = np.column_stack([-L, -T, n])
    tip_ring = (R @ _leaf_perimeter_local(LEAF_LENGTH_MM, LEAF_WIDTH_MM, LEAF_FOLD_DEG).T).T + tip_pos

    # Linear loft: circle → leaf outline.
    ts    = np.linspace(0, 1, _N_LOFT_RINGS)
    rings = (1 - ts[:, None, None]) * root_ring + ts[:, None, None] * tip_ring

    NP, NR           = _N_PERIM, _N_LOFT_RINGS
    root_cap_vi      = NR * NP        # centre of root cap (buried end)
    leaf_centroid_vi = NR * NP + 1   # centroid of leaf perimeter

    verts = np.vstack([rings.reshape(-1, 3), root_center, np.mean(tip_ring, axis=0)])

    def _vi(ri: int, j: int) -> int:
        return ri * NP + (j % NP)

    faces: list[list[int]] = []

    # Loft walls.
    for i in range(NR - 1):
        for j in range(NP):
            j1 = (j + 1) % NP
            a, b, c, d = _vi(i, j), _vi(i, j1), _vi(i+1, j1), _vi(i+1, j)
            faces += [[a, d, c], [a, c, b]]

    # Root cap — closes the buried end.
    for j in range(NP):
        j1 = (j + 1) % NP
        faces.append([root_cap_vi, _vi(0, j1), _vi(0, j)])

    # Leaf surface — fans from the tip ring to its centroid, closing the mesh.
    for j in range(NP):
        j1 = (j + 1) % NP
        faces.append([leaf_centroid_vi, _vi(NR - 1, j), _vi(NR - 1, j1)])

    mesh = trimesh.Trimesh(vertices=verts, faces=np.array(faces, dtype=np.int32), process=False)
    mesh.fix_normals()

    return mesh, not _is_printable(n)


# ── Scene assembly ─────────────────────────────────────────────────────────────

def build_debug_mesh() -> trimesh.Trimesh:
    """Build the coloured sphere and four branchlet+leaf assemblies."""
    sphere = trimesh.creation.icosphere(subdivisions=4, radius=SPHERE_RADIUS_MM)
    sphere.fix_normals()
    tag(sphere, debug_material(1))   # slot 0 reserved for failures

    parts: list[trimesh.Trimesh] = [sphere]

    for leaf_index, (name, fraction_down) in enumerate(PLACEMENTS):
        polar  = np.pi * fraction_down
        normal = np.array([np.sin(polar), 0.0, np.cos(polar)])
        mesh, is_fail = build_simple_branchlet_and_leaf(SPHERE_RADIUS_MM * normal, normal)
        slot   = 2 + leaf_index
        status = "FAIL (red)" if is_fail else f"ok  (slot {slot})  watertight={mesh.is_watertight}"
        print(f"  {name:13s}  {status}")
        tag(mesh, FAIL_MATERIAL if is_fail else debug_material(slot))
        parts.append(mesh)

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
