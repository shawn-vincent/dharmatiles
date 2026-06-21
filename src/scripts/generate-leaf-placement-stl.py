#!/usr/bin/env python3
"""Generate a debug sphere with branchlet+leaf assemblies at four latitudes.

Each branchlet grows straight out along the surface normal.  The loft morphs
from a circle at the root to the exact leaf perimeter at the tip, and the leaf
surface is stitched directly to that tip ring — loft walls, root cap, and leaf
surface form a single closed mesh with no overlapping parts.

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
    compute_leaf_geometry,
    _LEAF_N_LONG,
    _LEAF_N_LAT,
)

# ── Constants ──────────────────────────────────────────────────────────────────

DEFAULT_OUTPUT    = pathlib.Path("debug/leaf-placement.stl")
SPHERE_RADIUS_MM  = 12.0
LEAF_LENGTH_MM    = 6.0
LEAF_WIDTH_MM     = 3.5
LEAF_THICKNESS_MM = 1.2
LEAF_FOLD_DEG     = 3.0
EMBED_DEPTH_MM    = 1.6
FLOOR_ANGLE_DEG   = 45.0
ROOT_DIAMETER_FRACTION = 0.90   # root-ring diameter as fraction of max leaf dim

# Perimeter vertex count: rounded base + left edge (n_rings pts) +
#                         pointed tip + right edge (n_rings pts).
_N_RINGS  = _LEAF_N_LONG - 1          # interior longitudinal rings = 11
_N_LAT    = _LEAF_N_LAT               # lateral columns = 10
_NP       = 2 * _N_RINGS + 2          # perimeter vertex count = 24
_N_LOFT_RINGS = 2   # root circle + leaf perimeter; no intermediate rings needed

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


def _circle_ring(
    center: np.ndarray,
    u: np.ndarray, v: np.ndarray,
    radius: float,
    n_pts: int,
) -> np.ndarray:
    """Circle of *n_pts* vertices in the plane spanned by *u* and *v*."""
    t = np.linspace(0, 2 * np.pi, n_pts, endpoint=False)
    return center + radius * (np.cos(t[:, None]) * u + np.sin(t[:, None]) * v)


def _is_printable(surface_normal: np.ndarray) -> bool:
    """True iff the branchlet axis clears the FDM floor angle."""
    n = surface_normal / (np.linalg.norm(surface_normal) + 1e-12)
    return float(n[2]) >= np.sin(np.radians(FLOOR_ANGLE_DEG)) - 1e-6


# ── Branchlet + leaf (single closed mesh) ────────────────────────────────────

def build_branchlet_and_leaf(
    attachment_point: np.ndarray,
    surface_normal:   np.ndarray,
) -> tuple[trimesh.Trimesh, bool]:
    """Loft from a root circle to the leaf perimeter, stitched as one closed mesh.

    The loft morphs from a _NP-vertex circle at the embedded root to the exact
    24-vertex leaf perimeter at the tip.  The leaf surface (open top face) is
    stitched directly to that tip ring — no separate leaf object, no overlapping
    parts.  The root cap closes the buried end.

    Returns ``(mesh, is_fail)``.
    """
    n    = surface_normal / (np.linalg.norm(surface_normal) + 1e-12)
    L, T = _leaf_frame(n)

    root_radius = 0.5 * ROOT_DIAMETER_FRACTION * max(LEAF_LENGTH_MM, 2.0 * LEAF_WIDTH_MM)
    root_center = attachment_point - EMBED_DEPTH_MM * n
    tip_pos     = root_center + (EMBED_DEPTH_MM + LEAF_LENGTH_MM) * n

    # ── Leaf geometry in world space ──────────────────────────────────────────
    # The leaf's long axis is -L (base = rounded end at +L, tip = pointed end
    # at -L).  The leaf normal is n (crease faces outward from sphere).
    g = compute_leaf_geometry(
        base_pos=tip_pos + (LEAF_LENGTH_MM / 2.0) * L,
        tangent=-L,
        length_mm=LEAF_LENGTH_MM,
        width_mm=LEAF_WIDTH_MM,
        thickness_mm=LEAF_THICKNESS_MM,
        fold_angle_deg=LEAF_FOLD_DEG,
        up_hint=n,
    )

    NR = _N_LOFT_RINGS
    NP = _NP          # 24
    nr = _N_RINGS     # 11 (interior longitudinal rings)
    NT = _N_LAT       # 10 (lateral columns)

    # ── Leaf perimeter (tip ring, 24 vertices, CCW from +n) ──────────────────
    # Order: rounded_base | left_edge (s↑) | pointed_tip | right_edge (s↓)
    boundary = np.vstack([
        g.bp[np.newaxis],          # k=0:      rounded base   (+L direction)
        g.top_pts[:, 0, :],        # k=1..11:  left edge, j=0, s increasing
        g.v_tip[np.newaxis],       # k=12:     pointed tip    (-L direction)
        g.top_pts[::-1, NT, :],    # k=13..23: right edge, j=NT, s decreasing
    ])  # (NP, 3)

    # ── Root ring (circle, vertex 0 aligned to boundary vertex 0) ────────────
    # u=L, v=T → vertex 0 in +L direction; L×T = n → CCW from +n. ✓
    root_ring = _circle_ring(root_center, L, T, root_radius, NP)

    # ── Loft rings: linear blend from root circle to leaf perimeter ───────────
    ts    = np.linspace(0.0, 1.0, NR)
    rings = ((1 - ts)[:, None, None] * root_ring[None]
             +  ts   [:, None, None] * boundary  [None])  # (NR, NP, 3)

    # ── Leaf interior vertices (top_pts columns 1..NT-1) ─────────────────────
    leaf_int = g.top_pts[:, 1:NT, :].reshape(-1, 3)  # (nr*(NT-1), 3)

    # ── Vertex layout ─────────────────────────────────────────────────────────
    # 0 .. NR*NP-1          : loft rings (ring ri, vertex k  = ri*NP + k)
    # NR*NP                 : root_center (root cap centre)
    # NR*NP+1 .. NR*NP+nr*(NT-1) : leaf interior grid
    root_cap_vi = NR * NP

    verts = np.vstack([
        rings.reshape(-1, 3),
        root_center[np.newaxis],
        leaf_int,
    ])

    # ── Index helpers ──────────────────────────────────────────────────────────
    def loft_vi(ri: int, k: int) -> int:
        return ri * NP + k % NP

    def leaf_vi(ri: int, j: int) -> int:
        """Index for g.top_pts[ri, j].  ri=0..nr-1, j=0..NT."""
        if j == 0:
            return loft_vi(NR - 1, ri + 1)               # left boundary
        if j == NT:
            return loft_vi(NR - 1, 2 * nr + 1 - ri)      # right boundary
        return NR * NP + 1 + ri * (NT - 1) + (j - 1)    # interior

    def leaf_base_vi() -> int:
        return loft_vi(NR - 1, 0)        # k=0: rounded base

    def leaf_tip_vi() -> int:
        return loft_vi(NR - 1, nr + 1)   # k=12: pointed tip

    # ── Face construction ─────────────────────────────────────────────────────
    faces: list[list[int]] = []

    # Loft walls (NR-1 bands × NP quads each).
    for ri in range(NR - 1):
        for k in range(NP):
            k1 = (k + 1) % NP
            a, b = loft_vi(ri, k),    loft_vi(ri, k1)
            c, d = loft_vi(ri+1, k1), loft_vi(ri+1, k)
            faces += [[a, d, c], [a, c, b]]

    # Root cap (fan from root_center to ring 0).
    for k in range(NP):
        k1 = (k + 1) % NP
        faces.append([root_cap_vi, loft_vi(0, k1), loft_vi(0, k)])

    # Leaf surface — base fan (rounded base → ring 0 of top_pts).
    for j in range(NT):
        faces.append([leaf_base_vi(), leaf_vi(0, j), leaf_vi(0, j + 1)])

    # Leaf surface — body quads (interior rows 0..nr-2).
    for ri in range(nr - 1):
        for j in range(NT):
            a, b = leaf_vi(ri,   j),     leaf_vi(ri,   j + 1)
            c, d = leaf_vi(ri+1, j + 1), leaf_vi(ri+1, j)
            faces += [[a, d, c], [a, c, b]]

    # Leaf surface — tip fan (last row → pointed tip).
    for j in range(NT):
        faces.append([leaf_tip_vi(), leaf_vi(nr - 1, j + 1), leaf_vi(nr - 1, j)])

    mesh = trimesh.Trimesh(
        vertices=verts,
        faces=np.array(faces, dtype=np.int32),
        process=False,
    )
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
        mesh, is_fail = build_branchlet_and_leaf(SPHERE_RADIUS_MM * normal, normal)
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
