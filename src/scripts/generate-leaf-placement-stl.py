#!/usr/bin/env python3
"""Generate a debug sphere with leaves at four top-to-bottom positions.

The attachment points follow one meridian of a pure sphere:

* top:           0/4 down, polar angle   0°
* upper-quarter: 1/4 down, polar angle  45°
* equator:       2/4 down, polar angle  90°
* lower-quarter: 3/4 down, polar angle 135°

Each branchlet grows straight out along the surface normal — no exit-direction
tilting, no adaptive length search.  When a branchlet or its leaf violates the
FDM floor-angle rule the geometry is coloured debug-red (DEBUG_COLOR_0).
That slot is reserved for failures; the sphere and passing parts use slots 1+.

The leaf is built once in a canonical local frame (centred at origin, long axis
= +X, lateral = +Y, normal = +Z) and rigidly transformed for each attachment.

Run from the repository root::

    python src/scripts/generate-leaf-placement-stl.py

Default output: ``debug/leaf-placement.stl``.
"""
from __future__ import annotations

import argparse
import pathlib

import numpy as np
import trimesh

from dharmatiles.core.color import Material, debug_material, export_color_stl, tag
from dharmatiles.trees.leaf import (
    build_leaf_mesh,
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
LEAF_THICKNESS_MM = 1.2
LEAF_FOLD_DEG     = 3.0
LEAF_KEEL_MM      = 0.0
EMBED_DEPTH_MM    = 1.6
FLOOR_ANGLE_DEG   = 45.0
ROOT_DIAMETER_FRACTION = 0.90        # root-ring diameter as fraction of max leaf dim

_N_LOFT_RINGS = 8
_N_PERIM      = 2 + 2 * (_LEAF_N_LONG - 1)

FAIL_MATERIAL: Material = debug_material(0)   # red — reserved for FDM failures

PLACEMENTS: tuple[tuple[str, float], ...] = (
    ("top",           0.00),
    ("upper-quarter", 0.25),
    ("equator",       0.50),
    ("lower-quarter", 0.75),
)


# ── Geometry helpers ───────────────────────────────────────────────────────────

def _leaf_frame(
    surface_normal: np.ndarray,
) -> tuple[np.ndarray, np.ndarray]:
    """Return (L, T) — leaf axes in the tangent plane.

    L is world-up projected into the plane perpendicular to *surface_normal*
    (falls back to +X at the poles).  T = cross(surface_normal, L).
    """
    n  = surface_normal / (np.linalg.norm(surface_normal) + 1e-12)
    up = np.array([0.0, 0.0, 1.0])
    proj = up - float(np.dot(up, n)) * n
    if np.linalg.norm(proj) < 0.1:
        ref  = np.array([1.0, 0.0, 0.0])
        proj = ref - float(np.dot(ref, n)) * n
    L = proj / (np.linalg.norm(proj) + 1e-12)
    T = np.cross(n, L)
    T /= np.linalg.norm(T) + 1e-12
    return L, T


def _leaf_perimeter_local(
    length_mm: float,
    width_mm:  float,
    fold_deg:  float,
) -> np.ndarray:
    """Leaf outline in canonical local space, centred at the origin.

    Local axes: long axis = +X, lateral = +Y, leaf normal = +Z.
    Base (j=0) at X = −length/2; pointed tip (j=N//2) at X = +length/2.
    Centred so a transform with origin=tip_pos places the centroid on the
    branchlet axis with no shear.

    Returns ``(_N_PERIM, 3)``; ordering: base → right → tip → left.
    """
    half   = length_mm / 2.0
    s_int  = np.linspace(0.0, 1.0, _LEAF_N_LONG + 1)[1:-1]         # (11,)
    x_mid  = s_int * length_mm - half                               # centred x coords
    w_s    = width_mm * _leaf_width_profile(s_int)
    fold_h = (np.tanh(_LEAF_CREASE_SHARPNESS) * w_s
              * np.tan(np.radians(fold_deg))
              * (s_int ** 0.5 * (1.0 - s_int) ** 1.5) / _LEAF_LONG_T_PEAK)

    midribs    = np.column_stack([x_mid, np.zeros(len(s_int)), np.zeros(len(s_int))])
    right_edge = midribs + np.column_stack([np.zeros(len(s_int)),  w_s, fold_h])
    left_edge  = midribs + np.column_stack([np.zeros(len(s_int)), -w_s, fold_h])

    return np.concatenate([
        [[-half, 0.0, 0.0]],    # j = 0       base
        right_edge,              # j = 1..11   right edge (s increasing)
        [[ half, 0.0, 0.0]],    # j = 12      tip
        left_edge[::-1],         # j = 13..23  left edge (s decreasing)
    ])


def _transform_points(
    pts: np.ndarray,
    origin: np.ndarray,
    x_world: np.ndarray,
    y_world: np.ndarray,
    z_world: np.ndarray,
) -> np.ndarray:
    """Rigid local→world transform.  local +X→x_world, +Y→y_world, +Z→z_world."""
    return (np.column_stack([x_world, y_world, z_world]) @ pts.T).T + origin


def _root_ring(
    center:    np.ndarray,
    u:         np.ndarray,
    v:         np.ndarray,
    radius_mm: float,
    n_pts:     int,
) -> np.ndarray:
    """Circle of *n_pts* vertices in the plane spanned by (u, v)."""
    angles = -np.pi / 2.0 + 2.0 * np.pi * np.arange(n_pts) / n_pts
    return center + radius_mm * (
        np.cos(angles[:, None]) * v + np.sin(angles[:, None]) * u
    )


def _is_printable(
    loft:            trimesh.Trimesh,
    embed_depth_mm:  float,
    total_length_mm: float,
) -> bool:
    """Return True iff all exterior loft faces satisfy the FDM floor-angle rule.

    Wall faces in the embedded (buried) root portion are excluded; root-cap
    faces are always excluded.  Tip-cap faces are always checked.
    """
    if not loft.is_watertight:
        return False
    NP, NR    = _N_PERIM, _N_LOFT_RINGS
    n_wall    = (NR - 1) * NP * 2
    n_root_cap = NP
    n_skip    = int(np.floor(embed_depth_mm / total_length_mm * (NR - 1))) * NP * 2
    ext_mask  = np.zeros(len(loft.faces), dtype=bool)
    ext_mask[n_skip:n_wall] = True          # un-embedded wall faces
    ext_mask[n_wall + n_root_cap:] = True   # tip cap
    threshold = -float(np.cos(np.radians(FLOOR_ANGLE_DEG)))
    return not (loft.face_normals[ext_mask, 2] < threshold - 1e-6).any()


# ── Branchlet builder ─────────────────────────────────────────────────────────

def build_simple_branchlet_and_leaf(
    attachment_point: np.ndarray,
    surface_normal:   np.ndarray,
    leaf_local:       trimesh.Trimesh,
) -> tuple[list[trimesh.Trimesh], bool]:
    """Straight-along-normal branchlet loft capped with *leaf_local*.

    Returns ``([loft, leaf], is_fail)`` where *is_fail* is True iff the loft
    fails the FDM floor-angle check.
    """
    n           = surface_normal
    L, T        = _leaf_frame(n)
    root_radius  = 0.5 * ROOT_DIAMETER_FRACTION * max(LEAF_LENGTH_MM, 2.0 * LEAF_WIDTH_MM)
    root_center  = attachment_point - EMBED_DEPTH_MM * n
    total_length = EMBED_DEPTH_MM + LEAF_LENGTH_MM
    tip_pos      = root_center + total_length * n

    # Tip ring: leaf perimeter centred at tip_pos.  Local +X → −L so the
    # pointed tip faces gravity; +Y → −T; +Z → N (surface normal).
    leaf_perim = _transform_points(
        _leaf_perimeter_local(LEAF_LENGTH_MM, LEAF_WIDTH_MM, LEAF_FOLD_DEG),
        tip_pos, -L, -T, n,
    )
    root_ring = _root_ring(root_center, u=-L, v=-T, radius_mm=root_radius, n_pts=_N_PERIM)

    # Linear loft: circle → leaf outline.
    ts    = np.linspace(0.0, 1.0, _N_LOFT_RINGS)
    rings = (1.0 - ts[:, None, None]) * root_ring + ts[:, None, None] * leaf_perim
    # (shapes: root_ring/leaf_perim are (_N_PERIM, 3) — numpy broadcasts correctly)

    NP, NR      = _N_PERIM, _N_LOFT_RINGS
    root_cap_vi = NR * NP
    tip_cap_vi  = NR * NP + 1
    verts = np.vstack([rings.reshape(-1, 3), root_center, np.mean(leaf_perim, axis=0)])

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

    return [loft, leaf_world], not _is_printable(loft, EMBED_DEPTH_MM, total_length)


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
        polar  = np.pi * float(fraction_down)
        normal = np.array([np.sin(polar), 0.0, np.cos(polar)])
        attachment_point = SPHERE_RADIUS_MM * normal
        leaf_parts, is_fail = build_simple_branchlet_and_leaf(
            attachment_point, normal, leaf_proto,
        )
        # Slot 0 = red (failures).  Passing parts: sphere=1, then 2 slots per leaf.
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
