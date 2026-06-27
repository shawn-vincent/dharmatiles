"""Meridian-arc leaf placement on arbitrary closed meshes.

Public API
----------
place_leaves_on_mesh   -- place leaves on a trimesh and return meshes + stats
LeafPlacementStats     -- dataclass of coverage/quality metrics from a placement run
effective_ring_perimeter -- path length traced by leaf midpoints around one row ring
min_width_xy           -- minimum bounding-strip width of an XY point cloud
"""
from __future__ import annotations

import dataclasses
import math
from collections.abc import Callable

import numpy as np
import trimesh

from ._utils import _safe_norm
from .leaf import boundary_loop, build_leaf_surface, solidify_leaf
from .mesh import (
    _build_meridians,
    _compute_row_z_positions,
    _contact_angle_for_sphere,
    _hash01_int,
    _interpolate_meridian_normal,
)


@dataclasses.dataclass
class LeafPlacementStats:
    """Metrics collected during leaf placement on one mesh object."""
    label: str = ""
    # Totals
    n_rows: int = 0
    n_attempted: int = 0       # candidates that reached the leaf-build step
    n_placed: int = 0          # leaves successfully solidified
    # Skip / error breakdown
    skipped_downward: int = 0  # up_hint.z < -0.1 (downward-facing surface)
    skipped_small_r: int = 0   # local_r < 1.0 mm (too close to centroid)
    ca_clamped: int = 0        # contact angle ≥ π/2, placed flat (ca = 0)
    build_errors: int = 0      # RuntimeError / ValueError from leaf builder
    # Per-row data: (z, attempted, placed) — one entry per generated row
    rows: list[tuple[float, int, int]] = dataclasses.field(default_factory=list)
    # Per-leaf data
    base_positions: list[np.ndarray] = dataclasses.field(default_factory=list)
    base_tangents: list[np.ndarray] = dataclasses.field(default_factory=list)
    base_row_idx: list[int] = dataclasses.field(default_factory=list)
    root_depths: list[float] = dataclasses.field(default_factory=list)
    # Per-leaf: maximum distance of any curl-region vertex (> L/2 from base)
    # that lies OUTSIDE the mesh.  Near zero = curl pressed against mesh.
    leaf_float_dists: list[float] = dataclasses.field(default_factory=list)
    # Per-leaf: maximum depth of any curl-region vertex that lies INSIDE the
    # mesh (unsigned distance to nearest surface point).  > 0 = buried.
    leaf_buried_depths: list[float] = dataclasses.field(default_factory=list)
    # Per-row perimeter (sum of all polygon perimeters in that cross-section slice)
    row_perims: list[float] = dataclasses.field(default_factory=list)
    # Geometry parameters (filled in once after the row loop)
    leaf_length_mm: float = 0.0      # L
    leaf_width_mm: float = 0.0       # W — used for effective-overlap calculation
    col_step: float = 0.0            # min expected spacing between same-row leaves
    expected_row_step: float = 0.0   # L * (1 - v_overlap) — target z-gap between rows
    z_top: float = 0.0               # mesh apex Z
    z_top_anchor: float = 0.0        # expected topmost row Z ≈ z_top − 0.25·L
    cx: float = 0.0                  # mesh centroid X (for phi-sector analysis)
    cy: float = 0.0                  # mesh centroid Y


def min_width_xy(pts: np.ndarray) -> float:
    """Minimum width of the XY projection of *pts* (rotating-caliper approximation).

    Samples 90 evenly-spaced directions and returns the smallest
    (max_projection − min_projection).  For a circular ring this equals the
    diameter; for a long skinny set it equals the narrow dimension.
    """
    if len(pts) < 2:
        return 0.0
    xy = pts[:, :2]
    angles = np.linspace(0.0, np.pi, 90, endpoint=False)
    dirs = np.stack([np.cos(angles), np.sin(angles)], axis=1)  # (90, 2)
    projs = xy @ dirs.T                                          # (N, 90)
    widths = projs.max(axis=0) - projs.min(axis=0)              # (90,)
    return float(widths.min())


def effective_ring_perimeter(
    base_positions: list[np.ndarray],
    tangents: list[np.ndarray],
    leaf_length_mm: float,
    cx: float,
    cy: float,
) -> float:
    """Compute the actual path length traced by leaf midpoints around one row ring.

    Each leaf's widest cross-section is at L/2 along its growth direction
    (tangent).  Projects midpoints to XY, sorts by angle around (cx, cy),
    sums consecutive distances, and closes the ring only when leaves span most
    of the circumference (largest angular gap < 120°).  Makes no assumption
    about shape — works for spheres, cylinders, and tilted or curved clusters.
    """
    if len(base_positions) < 2:
        return 0.0
    half_L = leaf_length_mm / 2.0
    mids = np.array([b + half_L * t for b, t in zip(base_positions, tangents)])
    xy   = mids[:, :2]
    angles = np.arctan2(xy[:, 1] - cy, xy[:, 0] - cx)
    order  = np.argsort(angles)
    xy_s   = xy[order]
    ang_s  = angles[order]

    gaps    = np.diff(ang_s)
    max_gap = float(gaps.max()) if len(gaps) else 2 * math.pi
    close   = max_gap < (2 * math.pi * 2 / 3)   # close if no gap > 120°

    dists = np.linalg.norm(np.diff(xy_s, axis=0), axis=1)
    total = float(dists.sum())
    if close:
        total += float(np.linalg.norm(xy_s[-1] - xy_s[0]))
    return total


def place_leaves_on_mesh(
    mesh: trimesh.Trimesh,
    *,
    length_mm: float,
    width_mm: float,
    thickness_mm: float,
    fold_angle_deg: float,
    inner_curve: float,
    outer_curve: float,
    curl_deg: float,
    lift_mm: float,
    h_overlap: float = 0.0,
    v_overlap: float = 0.0,
    n_meridians: int = 6,
    z_samples: int = 64,
    seed: int = 0,
    label: str = "mesh",
    row_color_fn: Callable[[int], tuple[int, int, int, int]] | None = None,
) -> tuple[list[trimesh.Trimesh], LeafPlacementStats]:
    """Run meridian-arc leaf placement on any closed mesh.

    Sections the mesh at arc-equidistant z-heights, samples positions along
    each cross-section polygon, orients each leaf using an interpolated surface
    normal, applies the contact-angle tilt so the leaf belly grazes the surface,
    and solidifies the result.

    Parameters
    ----------
    mesh:
        Closed, watertight trimesh to cover with leaves.
    length_mm, width_mm, thickness_mm, fold_angle_deg,
    inner_curve, outer_curve, curl_deg, lift_mm:
        Leaf geometry — passed through to :func:`build_leaf_surface` and
        :func:`_contact_angle_for_sphere`.
    h_overlap:
        Horizontal (along-ring) overlap fraction.  0 = leaves just touch;
        positive = overlap; negative = gap.
    v_overlap:
        Vertical (row-to-row) overlap fraction.  Same sign convention.
    n_meridians:
        Number of meridian arcs used to sample the mesh surface.
    z_samples:
        Number of z samples per meridian arc.
    seed:
        Master RNG seed; each leaf gets a deterministic per-leaf seed derived
        from this value plus its row/column indices.
    label:
        Human-readable name printed in progress messages.
    row_color_fn:
        Optional callback ``(row_idx) -> (R, G, B, A)`` applied to every
        solidified leaf mesh.  Pass ``None`` to skip coloring.

    Returns
    -------
    (leaf_meshes, stats)
        List of solidified leaf :class:`trimesh.Trimesh` objects and a
        :class:`LeafPlacementStats` with coverage and quality metrics.
    """
    L   = float(length_mm)
    W   = float(width_mm)
    hov = float(h_overlap)
    vov = float(v_overlap)

    # Shared kwargs forwarded to both the contact-angle calculator and the leaf builder.
    _leaf_kw = dict(
        length_mm      = L,
        width_mm       = W,
        thickness_mm   = float(thickness_mm),
        fold_angle_deg = float(fold_angle_deg),
        inner_curve    = float(inner_curve),
        outer_curve    = float(outer_curve),
        curl_deg       = float(curl_deg),
        lift_mm        = float(lift_mm),
    )

    col_step = max(W * (1.0 - hov), 1e-3)
    z_top    = float(mesh.vertices[:, 2].max())
    cx       = float(mesh.vertices[:, 0].mean())
    cy       = float(mesh.vertices[:, 1].mean())
    mesh_centroid_3d = np.array([cx, cy, float(mesh.vertices[:, 2].mean())])

    meridians = _build_meridians(mesh, n_meridians=n_meridians, z_samples=z_samples)
    row_zs    = _compute_row_z_positions(meridians, L, vov, z_top)

    z_top_anchor      = float(max(m.z_vals[-1] for m in meridians))
    expected_row_step = L * max(1.0 - vov, 0.05)

    stats = LeafPlacementStats(
        label             = label,
        leaf_length_mm    = L,
        leaf_width_mm     = W,
        col_step          = col_step,
        expected_row_step = expected_row_step,
        z_top             = z_top,
        z_top_anchor      = z_top_anchor,
        cx                = cx,
        cy                = cy,
    )

    print(f"  [{label}] meridians={len(meridians)}  rows={len(row_zs)}")
    if row_zs:
        print(f"    z_range=[{row_zs[0]:.2f}, {row_zs[-1]:.2f}]  (z_top={z_top:.2f})")

    # Contact-angle cache: identical for all leaves sharing the same local radius.
    _ca_cache: dict[int, float] = {}

    def _cached_ca(r: float) -> float:
        key = round(r * 1000)
        if key not in _ca_cache:
            _ca_cache[key] = _contact_angle_for_sphere(r, **_leaf_kw)
        return _ca_cache[key]

    # Pre-compute ca using the mesh bounding radius as a proxy for local_r.
    # Used to inflate the 2D cross-section perimeter to the effective leaf-midpoint
    # ring path before computing n_col (fixes underfilled rings near the apex).
    _r_mesh_est = float(np.max(np.linalg.norm(
        mesh.vertices - mesh_centroid_3d, axis=1
    )))
    _ca_ncol   = _cached_ca(max(_r_mesh_est, 1.0))
    _c_ca_ncol = math.cos(_ca_ncol)
    _s_ca_ncol = math.sin(_ca_ncol)

    parts: list[trimesh.Trimesh] = []

    for row_idx, z_row in enumerate(row_zs):
        row_attempt = 0
        row_placed  = 0
        row_perim   = 0.0

        _nz_row    = [float(np.interp(z_row, m.z_vals, m.normals[:, 2]))
                      for m in meridians if m.z_vals[0] <= z_row <= m.z_vals[-1]]
        _nz_avg    = float(np.mean(_nz_row)) if _nz_row else 0.0
        _nr_avg    = math.sqrt(max(0.0, 1.0 - _nz_avg ** 2))
        _t_xy_ncol = max(0.0, _nz_avg * _c_ca_ncol - _nr_avg * _s_ca_ncol)

        sec = mesh.section(
            plane_origin = np.array([0.0, 0.0, z_row]),
            plane_normal = np.array([0.0, 0.0, 1.0]),
        )
        if sec is None:
            stats.rows.append((z_row, 0, 0))
            stats.row_perims.append(0.0)
            continue
        try:
            path2d, xform = sec.to_planar()
        except Exception:
            stats.rows.append((z_row, 0, 0))
            stats.row_perims.append(0.0)
            continue

        for poly in path2d.polygons_full:
            perim = float(poly.length)
            row_perim += perim
            if perim < 1e-3:
                continue

            c2d = poly.centroid
            c4d = xform @ np.array([float(c2d.x), float(c2d.y), 0.0, 1.0])
            centroid_3d = c4d[:3].copy()

            _r_ring = perim / (2.0 * math.pi) if perim > 1e-6 else 1.0
            _r_mid  = _r_ring + (L / 2.0) * _t_xy_ncol
            _eff_perim_est = (perim * (_r_mid / max(_r_ring, 1e-6))
                              if perim > col_step else perim)
            n_col = max(1, int(math.ceil(_eff_perim_est / col_step)))

            for ci in range(n_col):
                t    = float(ci) / float(n_col)
                pt2  = poly.exterior.interpolate(t, normalized=True)
                p4d  = xform @ np.array([float(pt2.x), float(pt2.y), 0.0, 1.0])
                pt3d = p4d[:3].copy()

                phi     = float(np.arctan2(pt3d[1] - cy, pt3d[0] - cx))
                up_hint = _interpolate_meridian_normal(meridians, phi, z_row)

                if float(up_hint[2]) < -0.1:
                    stats.skipped_downward += 1
                    continue

                local_r = float(np.linalg.norm(pt3d - mesh_centroid_3d))
                if local_r < 1.0:
                    stats.skipped_small_r += 1
                    continue

                ca = _cached_ca(local_r)
                if ca >= math.pi / 2:
                    ca = 0.0
                    stats.ca_clamped += 1

                row_attempt += 1

                grav  = np.array([0.0, 0.0, -1.0])
                proj  = grav - float(np.dot(grav, up_hint)) * up_hint
                plen  = float(np.linalg.norm(proj))
                if plen < 1e-6:
                    arb = (np.array([1.0, 0.0, 0.0])
                           if abs(float(up_hint[0])) < 0.9
                           else np.array([0.0, 1.0, 0.0]))
                    T0 = _safe_norm(np.cross(up_hint, arb))
                else:
                    T0 = proj / plen

                c_ca = math.cos(ca)
                s_ca = math.sin(ca)
                tangent   = _safe_norm(T0 * c_ca - up_hint * s_ca)
                up_placed = _safe_norm(up_hint * c_ca + T0 * s_ca)

                lseed = int(_hash01_int(seed, "leaf", row_idx, ci))
                try:
                    surf  = build_leaf_surface(
                        base_pos = pt3d,
                        tangent  = tangent,
                        up_hint  = up_placed,
                        seed     = lseed,
                        **_leaf_kw,
                    )
                    loop        = boundary_loop(surf)
                    solid, _    = solidify_leaf(surf, up_placed, parent_mesh=mesh)
                except (RuntimeError, ValueError):
                    stats.build_errors += 1
                    continue

                # Root-depth measurement
                n_surf     = len(surf.vertices)
                NP         = len(loop)
                perim_v    = surf.vertices[np.array(loop)]
                root_v     = solid.vertices[n_surf : n_surf + NP]
                root_depth = float(np.mean(np.linalg.norm(root_v - perim_v, axis=1)))

                # Curl-region float / bury check
                base_dists_v = np.linalg.norm(surf.vertices - pt3d, axis=1)
                curl_mask    = base_dists_v > (L / 2.0)
                if curl_mask.any():
                    curl_verts = surf.vertices[curl_mask]
                    _, _curl_dists, _ = trimesh.proximity.closest_point(mesh, curl_verts)
                    _inside   = mesh.contains(curl_verts)
                    outside_d = _curl_dists[~_inside]
                    inside_d  = _curl_dists[_inside]
                    stats.leaf_float_dists.append(
                        float(outside_d.max()) if len(outside_d) else 0.0
                    )
                    stats.leaf_buried_depths.append(
                        float(inside_d.max()) if len(inside_d) else 0.0
                    )
                else:
                    stats.leaf_float_dists.append(0.0)
                    stats.leaf_buried_depths.append(0.0)

                stats.base_positions.append(pt3d.copy())
                stats.base_tangents.append(tangent.copy())
                stats.base_row_idx.append(row_idx)
                stats.root_depths.append(root_depth)
                row_placed     += 1
                stats.n_placed += 1

                if len(solid.vertices) > 0:
                    if row_color_fn is not None:
                        rgba  = row_color_fn(row_idx)
                        color = np.asarray(rgba, dtype=np.uint8)
                        solid.visual = trimesh.visual.ColorVisuals(
                            mesh=solid,
                            face_colors=np.tile(color, (len(solid.faces), 1)),
                        )
                    parts.append(solid)

        stats.rows.append((z_row, row_attempt, row_placed))
        stats.row_perims.append(row_perim)
        stats.n_attempted += row_attempt

    stats.n_rows = len(row_zs)
    return parts, stats
