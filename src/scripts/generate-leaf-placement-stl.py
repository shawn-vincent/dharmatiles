#!/usr/bin/env python3
"""Generate a debug sphere with leaves at four top-to-bottom positions.

The attachment points follow one meridian of a pure sphere:

* top:           0/4 down, polar angle   0°
* upper-quarter: 1/4 down, polar angle  45°
* equator:       2/4 down, polar angle  90°
* lower-quarter: 3/4 down, polar angle 135°

Each branchlet grows straight out along the surface normal.  Geometry that
violates the FDM floor-angle rule is coloured debug-red (DEBUG_COLOR_0, reserved
for failures).  Passing parts use debug colour slots 1+.

Run from the repository root::

    python src/scripts/generate-leaf-placement-stl.py
"""
from __future__ import annotations

import argparse
import pathlib

import numpy as np
import trimesh

from dharmatiles.core.color import Material, debug_material, export_color_stl, tag
from dharmatiles.trees.leaf import build_leaf_mesh

# ── Constants ──────────────────────────────────────────────────────────────────

DEFAULT_OUTPUT    = pathlib.Path("debug/leaf-placement.stl")
SPHERE_RADIUS_MM  = 12.0
LEAF_LENGTH_MM    = 6.0
LEAF_WIDTH_MM     = 3.5
LEAF_THICKNESS_MM = 1.2
LEAF_FOLD_DEG     = 3.0
LEAF_KEEL_MM      = 0.0
EMBED_DEPTH_MM    = 1.6
FLOOR_ANGLE_DEG   = 45.0
ROOT_DIAMETER_FRACTION = 0.90   # root-ring diameter as fraction of max leaf dim

_N_RING_PTS  = 16   # vertices on each loft ring
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


# ── Branchlet builder ─────────────────────────────────────────────────────────

def build_simple_branchlet_and_leaf(
    attachment_point: np.ndarray,
    surface_normal:   np.ndarray,
    leaf_local:       trimesh.Trimesh,
) -> tuple[list[trimesh.Trimesh], bool]:
    """Straight-along-normal loft (circle → oval) capped with *leaf_local*.

    Returns ``([loft, leaf], is_fail)``.
    """
    n    = surface_normal / (np.linalg.norm(surface_normal) + 1e-12)
    L, T = _leaf_frame(n)

    root_radius = 0.5 * ROOT_DIAMETER_FRACTION * max(LEAF_LENGTH_MM, 2.0 * LEAF_WIDTH_MM)
    root_center = attachment_point - EMBED_DEPTH_MM * n
    tip_pos     = root_center + (EMBED_DEPTH_MM + LEAF_LENGTH_MM) * n

    # Both rings centred on the branchlet axis — no shear.
    root_ring = _ellipse_ring(root_center, -L, -T, root_radius,          root_radius,   _N_RING_PTS)
    tip_ring  = _ellipse_ring(tip_pos,     -L, -T, LEAF_LENGTH_MM / 2.0, LEAF_WIDTH_MM, _N_RING_PTS)

    # Linear loft: circle → oval.
    ts    = np.linspace(0, 1, _N_LOFT_RINGS)
    rings = (1 - ts[:, None, None]) * root_ring + ts[:, None, None] * tip_ring

    NP, NR      = _N_RING_PTS, _N_LOFT_RINGS
    root_cap_vi = NR * NP
    tip_cap_vi  = NR * NP + 1
    verts = np.vstack([rings.reshape(-1, 3), root_center, np.mean(tip_ring, axis=0)])

    def _vi(ri: int, j: int) -> int:
        return ri * NP + (j % NP)

    faces: list[list[int]] = []
    for i in range(NR - 1):
        for j in range(NP):
            j1 = (j + 1) % NP
            a, b, c, d = _vi(i, j), _vi(i, j1), _vi(i+1, j1), _vi(i+1, j)
            faces += [[a, d, c], [a, c, b]]
    for j in range(NP):
        j1 = (j + 1) % NP
        faces.append([root_cap_vi, _vi(0, j1), _vi(0, j)])
    for j in range(NP):
        j1 = (j + 1) % NP
        faces.append([tip_cap_vi, _vi(NR - 1, j), _vi(NR - 1, j1)])

    loft = trimesh.Trimesh(vertices=verts, faces=np.array(faces, dtype=np.int32), process=False)
    loft.fix_normals()

    # Transform leaf: local +X → −L, +Y → −T, +Z → n, origin → tip_pos.
    tf = np.eye(4)
    tf[:3, :3] = np.column_stack([-L, -T, n])
    tf[:3,  3] = tip_pos
    leaf_world = leaf_local.copy()
    leaf_world.apply_transform(tf)

    return [loft, leaf_world], not _is_printable(n)


# ── Scene assembly ─────────────────────────────────────────────────────────────

def build_debug_mesh() -> trimesh.Trimesh:
    """Build the coloured sphere, branchlets, and four leaves."""
    sphere = trimesh.creation.icosphere(subdivisions=4, radius=SPHERE_RADIUS_MM)
    sphere.fix_normals()
    tag(sphere, debug_material(1))   # slot 0 reserved for failures

    parts: list[trimesh.Trimesh] = [sphere]

    leaf_proto_parts = [
        p for p in build_leaf_mesh(
            base_pos=np.array([-LEAF_LENGTH_MM / 2.0, 0.0, 0.0]),
            tangent=np.array([1.0, 0.0, 0.0]),
            length_mm=LEAF_LENGTH_MM, width_mm=LEAF_WIDTH_MM,
            thickness_mm=LEAF_THICKNESS_MM, fold_angle_deg=LEAF_FOLD_DEG,
            keel_depth_mm=LEAF_KEEL_MM, up_hint=np.array([0.0, 0.0, 1.0]),
            seed=0,
        ) if len(p.vertices) > 0
    ]
    leaf_proto = trimesh.util.concatenate(leaf_proto_parts) if leaf_proto_parts else trimesh.Trimesh()

    for leaf_index, (name, fraction_down) in enumerate(PLACEMENTS):
        polar  = np.pi * fraction_down
        normal = np.array([np.sin(polar), 0.0, np.cos(polar)])
        leaf_parts, is_fail = build_simple_branchlet_and_leaf(
            SPHERE_RADIUS_MM * normal, normal, leaf_proto,
        )
        base_slot = 2 + leaf_index * 2
        status = "FAIL (red)" if is_fail else f"ok  (slots {base_slot}–{base_slot + 1})"
        print(f"  {name:13s}  {status}")
        for i, part in enumerate(leaf_parts):
            tag(part, FAIL_MATERIAL if is_fail else debug_material(base_slot + i))
            parts.append(part)

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
