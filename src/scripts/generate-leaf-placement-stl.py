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

# ── Constants ──────────────────────────────────────────────────────────────────

DEFAULT_OUTPUT    = pathlib.Path("debug/leaf-placement.stl")
SPHERE_RADIUS_MM  = 12.0
LEAF_LENGTH_MM    = 6.0
LEAF_WIDTH_MM     = 3.5
EMBED_DEPTH_MM    = 1.6
FLOOR_ANGLE_DEG   = 45.0
ROOT_DIAMETER_FRACTION = 0.90   # root-ring diameter as fraction of max leaf dim

_N_RING_PTS   = 16   # vertices on each loft ring
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

    The loft (circle → oval) is left open at the tip.  The leaf surface fans
    from the tip ring to an apex in the −L direction, closing the mesh.
    No overlapping parts; shared vertices at the junction.

    Returns ``(mesh, is_fail)``.
    """
    n    = surface_normal / (np.linalg.norm(surface_normal) + 1e-12)
    L, T = _leaf_frame(n)

    root_radius = 0.5 * ROOT_DIAMETER_FRACTION * max(LEAF_LENGTH_MM, 2.0 * LEAF_WIDTH_MM)
    root_center = attachment_point - EMBED_DEPTH_MM * n
    tip_pos     = root_center + (EMBED_DEPTH_MM + LEAF_LENGTH_MM) * n

    # The tip ring is the shared junction between loft and leaf.
    root_ring = _ellipse_ring(root_center, -L, -T, root_radius,          root_radius,   _N_RING_PTS)
    tip_ring  = _ellipse_ring(tip_pos,     -L, -T, LEAF_LENGTH_MM / 2.0, LEAF_WIDTH_MM, _N_RING_PTS)

    # Leaf apex: one full leaf-length in the −L direction from tip_pos,
    # placing it LEAF_LENGTH/2 beyond the far edge of the oval tip ring.
    leaf_apex = tip_pos + LEAF_LENGTH_MM * (-L)

    # Linear loft: circle → oval.
    ts    = np.linspace(0, 1, _N_LOFT_RINGS)
    rings = (1 - ts[:, None, None]) * root_ring + ts[:, None, None] * tip_ring

    NP, NR       = _N_RING_PTS, _N_LOFT_RINGS
    root_cap_vi  = NR * NP        # centre of root cap (buried end)
    leaf_apex_vi = NR * NP + 1   # leaf tip

    verts = np.vstack([rings.reshape(-1, 3), root_center, leaf_apex])

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

    # Leaf surface — stitched to the open tip ring; closes the mesh.
    for j in range(NP):
        j1 = (j + 1) % NP
        faces.append([leaf_apex_vi, _vi(NR - 1, j), _vi(NR - 1, j1)])

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
