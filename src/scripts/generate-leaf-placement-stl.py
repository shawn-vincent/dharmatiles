#!/usr/bin/env python3
"""Generate a debug sphere with leaves at four top-to-bottom positions.

The attachment points follow one meridian of a pure sphere:

* top:           0/4 down, polar angle   0°
* upper-quarter: 1/4 down, polar angle  45°
* equator:       2/4 down, polar angle  90°
* lower-quarter: 3/4 down, polar angle 135°

Each branchlet grows **straight out along the surface normal** — no FDM-angle
tilting, no adaptive length search.  When a branchlet or its leaf violates the
FDM floor-angle rule its geometry is coloured debug-red (DEBUG_COLOR_0).  That
slot is reserved for failures and is never used for the sphere or passing parts.

The leaf is built once in a canonical local frame (base at origin, long axis
= +X, lateral = +Y, normal = +Z) and then rigidly transformed to world space
for each attachment.

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

# Root-ring diameter = this fraction of the leaf's largest surface dimension.
ROOT_DIAMETER_FRACTION = 0.90

# Visible branchlet length beyond the sphere surface.
VISIBLE_LENGTH_MM = LEAF_LENGTH_MM

# Loft geometry parameters.
_N_LOFT_RINGS = 8
_N_PERIM      = 2 + 2 * (_LEAF_N_LONG - 1)   # = 24

# DEBUG_COLOR_0 (red) is reserved exclusively for FDM failures.
FAIL_MATERIAL: Material = debug_material(0)

PLACEMENTS: tuple[tuple[str, float], ...] = (
    ("top",           0.00),
    ("upper-quarter", 0.25),
    ("equator",       0.50),
    ("lower-quarter", 0.75),
)


# ── Geometry helpers ───────────────────────────────────────────────────────────

def _attachment(
    radius_mm: float,
    fraction_down: float,
) -> tuple[np.ndarray, np.ndarray]:
    """Return the surface point and outward normal on the +X meridian."""
    polar  = np.pi * float(fraction_down)
    normal = np.array([np.sin(polar), 0.0, np.cos(polar)], dtype=float)
    return radius_mm * normal, normal


def _leaf_frame(
    surface_normal: np.ndarray,
) -> tuple[np.ndarray, np.ndarray]:
    """Return (L, T) — leaf axes in the tangent plane.

    L is world-up projected into the plane perpendicular to *surface_normal*
    (falls back to +X at the poles).  T = cross(surface_normal, L).
    No FDM-angle tilting is applied; the branchlet goes straight along the
    surface normal.
    """
    n  = surface_normal / (np.linalg.norm(surface_normal) + 1e-12)
    up = np.array([0.0, 0.0, 1.0])
    proj     = up - float(np.dot(up, n)) * n
    proj_len = float(np.linalg.norm(proj))
    if proj_len < 0.1:
        ref  = np.array([1.0, 0.0, 0.0])
        proj = ref - float(np.dot(ref, n)) * n
        proj_len = float(np.linalg.norm(proj)) + 1e-12
    L = proj / proj_len
    T = np.cross(n, L)
    T /= float(np.linalg.norm(T)) + 1e-12
    return L, T


def _leaf_perimeter_local(
    length_mm: float,
    width_mm:  float,
    fold_deg:  float,
) -> np.ndarray:
    """Leaf outline in canonical local space, **centred at the origin**.

    Local axes: long axis = +X, lateral = +Y, leaf normal = +Z.
    The perimeter is shifted so its centroid lies at the origin:
    the leaf base (j=0) is at X = −length/2 and the pointed tip (j=N//2)
    is at X = +length/2.  When this is transformed to world space with
    origin = tip_pos, the centroid lands exactly on the branchlet axis —
    no shear.

    Returns ``(_N_PERIM, 3)`` with vertex ordering:
    base → right edge (s increasing) → tip → left edge (s decreasing).
    """
    s_int  = np.linspace(0.0, 1.0, _LEAF_N_LONG + 1)[1:-1]            # (11,)
    w_s    = width_mm * _leaf_width_profile(s_int)                     # (11,)
    long_t = (s_int ** 0.5 * (1.0 - s_int) ** 1.5) / _LEAF_LONG_T_PEAK
    fold_h = float(np.tanh(_LEAF_CREASE_SHARPNESS)) * w_s * float(np.tan(np.radians(fold_deg))) * long_t

    half   = length_mm / 2.0
    # x runs from −half (base) to +half (tip) so the centroid is at 0.
    x_mid  = s_int * length_mm - half                                  # (11,) centred

    Y = np.array([0.0, 1.0, 0.0])
    Z = np.array([0.0, 0.0, 1.0])

    midribs    = np.column_stack([x_mid, np.zeros(len(s_int)), np.zeros(len(s_int))])
    right_edge = midribs + w_s[:, None] * Y + fold_h[:, None] * Z
    left_edge  = midribs - w_s[:, None] * Y + fold_h[:, None] * Z

    return np.concatenate([
        np.array([[-half, 0.0, 0.0]]),             # j = 0       base
        right_edge,                                # j = 1..11   right
        np.array([[ half, 0.0, 0.0]]),             # j = 12      tip
        left_edge[::-1],                           # j = 13..23  left (s decreasing)
    ], axis=0)                                     # (_N_PERIM, 3)


def _transform_points(
    pts: np.ndarray,
    origin: np.ndarray,
    x_world: np.ndarray,
    y_world: np.ndarray,
    z_world: np.ndarray,
) -> np.ndarray:
    """Rigid transform: local→world.  local +X→x_world, +Y→y_world, +Z→z_world."""
    R = np.column_stack([x_world, y_world, z_world])   # (3, 3)
    return (R @ pts.T).T + origin                       # (N, 3)


def _root_ring(
    center:     np.ndarray,
    u:          np.ndarray,
    v:          np.ndarray,
    radius_mm:  float,
    n_pts:      int,
) -> np.ndarray:
    """Circle of *n_pts* vertices in the plane spanned by (u, v).

    j=0 → −u direction (leaf-base side); j=n//2 → +u (leaf-tip side).
    This ordering matches the local-space leaf perimeter so the loft is
    twist-free.
    """
    j_arr  = np.arange(n_pts, dtype=float)
    angles = -np.pi / 2.0 + 2.0 * np.pi * j_arr / n_pts
    return center[None] + radius_mm * (
        np.cos(angles[:, None]) * v[None]
        + np.sin(angles[:, None]) * u[None]
    )  # (n_pts, 3)


def _is_printable(
    loft:             trimesh.Trimesh,
    floor_angle_deg:  float,
    n_wall_faces:     int,
    n_root_cap_faces: int,
    embed_depth_mm:   float,
    total_length_mm:  float,
) -> bool:
    """Return True iff the loft satisfies watertightness and FDM floor-angle.

    Root-cap faces (buried end) are always excluded.  The embedded wall portion
    is estimated by fraction; tip-cap faces are always checked.
    """
    if not loft.is_watertight:
        return False

    floor_rad = np.radians(floor_angle_deg)
    n_faces   = len(loft.faces)

    # Mark exterior (non-embedded) faces.
    ext_mask = np.zeros(n_faces, dtype=bool)
    faces_per_ring = n_root_cap_faces * 2
    n_rings        = n_wall_faces // max(faces_per_ring, 1)
    embed_frac     = embed_depth_mm / max(float(total_length_mm), 1e-9)
    n_skip         = max(0, min(n_rings, int(np.floor(embed_frac * n_rings))))
    ext_mask[n_skip * faces_per_ring : n_wall_faces] = True
    ext_mask[n_wall_faces + n_root_cap_faces :]       = True   # tip cap

    threshold = -float(np.cos(floor_rad))
    return not (loft.face_normals[ext_mask, 2] < threshold - 1e-6).any()


# ── Leaf at origin ─────────────────────────────────────────────────────────────

def build_leaf_at_origin(leaf_index: int) -> trimesh.Trimesh:
    """Build the leaf mesh in canonical local space, **centred at the origin**.

    Local axes: long axis = +X, lateral = +Y, leaf normal = +Z.
    The leaf base is placed at X = −LEAF_LENGTH_MM/2 so the centroid sits at
    the origin, matching :func:`_leaf_perimeter_local`.  The rigid transform in
    :func:`build_simple_branchlet_and_leaf` then places the centroid at tip_pos
    with no shear.
    """
    parts = [
        p for p in build_leaf_mesh(
            base_pos          = np.array([-LEAF_LENGTH_MM / 2.0, 0.0, 0.0]),
            tangent           = np.array([1.0, 0.0, 0.0]),
            length_mm         = LEAF_LENGTH_MM,
            width_mm          = LEAF_WIDTH_MM,
            thickness_mm      = LEAF_THICKNESS_MM,
            fold_angle_deg    = LEAF_FOLD_DEG,
            keel_depth_mm     = LEAF_KEEL_MM,
            up_hint           = np.array([0.0, 0.0, 1.0]),
            seed              = leaf_index,
        )
        if len(p.vertices) > 0
    ]
    if not parts:
        return trimesh.Trimesh()
    return trimesh.util.concatenate(parts) if len(parts) > 1 else parts[0]


# ── Branchlet builder ─────────────────────────────────────────────────────────

def build_simple_branchlet_and_leaf(
    attachment_point: np.ndarray,
    surface_normal:   np.ndarray,
    leaf_local:       trimesh.Trimesh,
    leaf_index:       int,
) -> tuple[list[trimesh.Trimesh], bool]:
    """Grow a straight-along-normal branchlet and cap it with *leaf_local*.

    The branchlet root is embedded *EMBED_DEPTH_MM* into the surface; the tip
    extends *VISIBLE_LENGTH_MM* beyond it.  The loft morphs from a circle at
    the root to the leaf outline at the tip.  No exit-direction tilting is
    applied — the branchlet goes straight along *surface_normal*.

    Parameters
    ----------
    attachment_point
        World-space point on the parent sphere surface.
    surface_normal
        Outward normal at *attachment_point* (need not be unit-length).
    leaf_local
        Leaf mesh in canonical local space (from :func:`build_leaf_at_origin`).
        Transformed to world space and attached at the branchlet tip.
    leaf_index
        Integer seed for roll variation.

    Returns
    -------
    parts
        ``[loft_mesh, leaf_mesh]`` in world space.
    is_fail
        True iff any part violates the FDM floor-angle rule.
    """
    n0 = surface_normal / (np.linalg.norm(surface_normal) + 1e-12)
    L, T = _leaf_frame(n0)
    N    = n0   # leaf normal = surface normal

    # Root radius: 90% of the leaf's largest surface dimension.
    max_leaf_dim = max(LEAF_LENGTH_MM, 2.0 * LEAF_WIDTH_MM)
    root_radius  = 0.5 * ROOT_DIAMETER_FRACTION * max_leaf_dim

    # Positions — branchlet goes straight along n0.
    root_center   = attachment_point - EMBED_DEPTH_MM * n0
    total_length  = EMBED_DEPTH_MM + VISIBLE_LENGTH_MM
    tip_pos       = root_center + total_length * n0

    # ── Leaf perimeter in world space ─────────────────────────────────────────
    # Local +X → −L (pointed tip faces gravity), +Y → −T, +Z → N.
    perim_local = _leaf_perimeter_local(LEAF_LENGTH_MM, LEAF_WIDTH_MM, LEAF_FOLD_DEG)
    leaf_perim  = _transform_points(perim_local, tip_pos, -L, -T, N)  # (_N_PERIM, 3)

    # ── Root ring (circle, perpendicular to n0) ───────────────────────────────
    # u = −L, v = −T so j=0 lines up with j=0 of the leaf perimeter.
    root_ring = _root_ring(root_center, u=-L, v=-T, radius_mm=root_radius, n_pts=_N_PERIM)

    # ── Linear loft: circle → leaf outline ───────────────────────────────────
    ts    = np.linspace(0.0, 1.0, _N_LOFT_RINGS)
    rings = (
        (1.0 - ts[:, None, None]) * root_ring[None]
        +       ts[:, None, None] * leaf_perim[None]
    )  # (_N_LOFT_RINGS, _N_PERIM, 3)

    NP, NR      = _N_PERIM, _N_LOFT_RINGS
    root_cap_vi = NR * NP
    tip_cap_vi  = NR * NP + 1

    verts = np.vstack([
        rings.reshape(-1, 3),
        root_center[None],
        np.mean(leaf_perim, axis=0)[None],
    ])

    def _vi(ri: int, j: int) -> int:
        return ri * NP + (j % NP)

    faces: list[list[int]] = []
    for i in range(NR - 1):
        for j in range(NP):
            j1 = (j + 1) % NP
            a, b = _vi(i,   j),   _vi(i,   j1)
            c, d = _vi(i+1, j1),  _vi(i+1, j)
            faces += [[a, d, c], [a, c, b]]
    for j in range(NP):
        j1 = (j + 1) % NP
        faces.append([root_cap_vi, _vi(0, j1), _vi(0, j)])
    for j in range(NP):
        j1 = (j + 1) % NP
        faces.append([tip_cap_vi, _vi(NR - 1, j), _vi(NR - 1, j1)])

    n_wall_faces     = (NR - 1) * NP * 2
    n_root_cap_faces = NP

    loft = trimesh.Trimesh(
        vertices = verts,
        faces    = np.array(faces, dtype=np.int32),
        process  = False,
    )
    loft.fix_normals()

    is_fail = not _is_printable(
        loft, FLOOR_ANGLE_DEG,
        n_wall_faces, n_root_cap_faces,
        EMBED_DEPTH_MM, total_length,
    )

    # ── Leaf mesh: transform local → world ────────────────────────────────────
    # Build a 4×4 rigid transform: local +X→−L, +Y→−T, +Z→N, origin→tip_pos.
    tf = np.eye(4)
    tf[:3, 0] = -L
    tf[:3, 1] = -T
    tf[:3, 2] =  N
    tf[:3, 3] =  tip_pos
    leaf_world = leaf_local.copy()
    leaf_world.apply_transform(tf)

    return [loft, leaf_world], is_fail


# ── Scene assembly ─────────────────────────────────────────────────────────────

def build_debug_mesh() -> trimesh.Trimesh:
    """Build the coloured sphere, branchlets, and four leaves."""
    sphere = trimesh.creation.icosphere(subdivisions=4, radius=SPHERE_RADIUS_MM)
    sphere.fix_normals()
    tag(sphere, debug_material(1))   # slot 1 — slot 0 is reserved for failures

    parts: list[trimesh.Trimesh] = [sphere]

    # Build one normalised leaf mesh and reuse it for every attachment.
    # (In a real pipeline the leaf could vary per site; here a single prototype
    # is enough to validate the geometry.)
    leaf_proto = build_leaf_at_origin(leaf_index=0)

    for leaf_index, (name, fraction_down) in enumerate(PLACEMENTS):
        attachment_point, surface_normal = _attachment(SPHERE_RADIUS_MM, fraction_down)
        leaf_parts, is_fail = build_simple_branchlet_and_leaf(
            attachment_point = attachment_point,
            surface_normal   = surface_normal,
            leaf_local       = leaf_proto,
            leaf_index       = leaf_index,
        )

        if is_fail:
            status = "FAIL (red)"
        else:
            # Two colour slots per leaf (loft + leaf blade); never slot 0.
            base_slot = 2 + leaf_index * 2
            status    = f"ok  (slots {base_slot}–{base_slot + 1})"

        print(f"  {name:13s}  {status}")

        for part_index, part in enumerate(leaf_parts):
            mat = FAIL_MATERIAL if is_fail else debug_material(base_slot + part_index)
            tag(part, mat)
            parts.append(part)

    return trimesh.util.concatenate(parts)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output", "-o",
        type    = pathlib.Path,
        default = DEFAULT_OUTPUT,
        help    = f"Output colour-STL path (default: {DEFAULT_OUTPUT})",
    )
    args = parser.parse_args()

    mesh = build_debug_mesh()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    export_color_stl(mesh, args.output)

    print(f"Wrote {args.output}")
    print(
        f"  {len(mesh.vertices):,} vertices · {len(mesh.faces):,} faces · "
        f"watertight={mesh.is_watertight}"
    )


if __name__ == "__main__":
    main()
