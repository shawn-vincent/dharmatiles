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
from .leaf import boundary_loop, build_leaf_surface, compute_leaf_geometry, solidify_leaf
from .mesh import (
    _avg_arc_for_z,
    _avg_z_for_arc,
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


def _polygon_point_at_phi(
    exterior_coords: np.ndarray,
    cx: float,
    cy: float,
    phi: float,
) -> np.ndarray | None:
    """Ray→polygon-edge intersection at 2-D azimuth phi from (cx, cy).

    Returns the outermost 2-D point on the polygon boundary hit by the ray, or
    None if the ray misses all edges (degenerate polygon or pole singularity).
    """
    pts   = exterior_coords[:-1]        # drop the repeated closing vertex
    n     = len(pts)
    if n < 2:
        return None
    vx, vy = pts[:, 0] - cx, pts[:, 1] - cy
    wx     = np.roll(vx, -1)
    wy     = np.roll(vy, -1)
    dx, dy = wx - vx, wy - vy
    cos_p, sin_p = math.cos(phi), math.sin(phi)
    det    = cos_p * dy - sin_p * dx
    valid  = np.abs(det) >= 1e-9
    safe   = np.where(valid, det, 1.0)
    t_arr  = np.where(valid, (vx * dy - vy * dx) / safe, -1.0)
    s_arr  = np.where(valid, (vx * sin_p - vy * cos_p) / safe, -1.0)
    ok     = valid & (t_arr > 1e-6) & (s_arr >= -1e-6) & (s_arr <= 1.0 + 1e-6)
    if not ok.any():
        return None
    idx = int(np.argmax(np.where(ok, t_arr, -1.0)))
    s_b = float(np.clip(s_arr[idx], 0.0, 1.0))
    return pts[idx] + s_b * (pts[(idx + 1) % n] - pts[idx])


def _leaf_contact_candidates(
    *,
    length_mm:      float,
    width_mm:       float,
    thickness_mm:   float,
    fold_angle_deg: float,
    inner_curve:    float,
    outer_curve:    float,
    curl_deg:       float,
    lift_mm:        float,
) -> np.ndarray:
    """Canonical curl-region displacements that can bind against the parent mesh.

    The old sphere formula assumes a single midrib dip point is always the first
    contact.  On faceted or non-spherical parent meshes the binding point can
    move laterally or onto the upper/lower surface, so solve against the same
    tip-half surface sample set used by the artifact detector.
    """
    g = compute_leaf_geometry(
        base_pos=np.zeros(3, dtype=float),
        tangent=np.array([1.0, 0.0, 0.0]),
        up_hint=np.array([0.0, 0.0, 1.0]),
        length_mm=length_mm,
        width_mm=width_mm,
        thickness_mm=thickness_mm,
        fold_angle_deg=fold_angle_deg,
        inner_curve=inner_curve,
        outer_curve=outer_curve,
        curl_deg=curl_deg,
        lift_mm=0.0,
    )

    pts = np.vstack([
        g.upper_grid.reshape(-1, 3),
        g.lower_grid.reshape(-1, 3),
        g.tip_pt[np.newaxis],
    ])
    base_d = np.linalg.norm(pts - g.base_pt[np.newaxis], axis=1)
    return pts[base_d > (float(length_mm) / 2.0)]


def _contact_angle_for_mesh(
    mesh: trimesh.Trimesh,
    proximity: trimesh.proximity.ProximityQuery,
    base_pos: np.ndarray,
    tangent0: np.ndarray,
    up_hint: np.ndarray,
    candidates: np.ndarray,
    *,
    initial_angle: float,
    iterations: int = 8,
    contact_tol_mm: float = 0.02,
) -> float:
    """Find the first contact angle where the curl region touches *mesh*.

    Distances are signed locally from the closest triangle normal: positive is
    inside the parent mesh and negative is outside.  The target angle is the
    smallest rotation where any contact candidate reaches zero distance, which
    keeps the contact calculation tied to the actual parent mesh rather than to
    a smooth sphere proxy.
    """
    base = np.asarray(base_pos, float)
    T0   = _safe_norm(np.asarray(tangent0, float))
    N0   = _safe_norm(np.asarray(up_hint, float))
    A0   = _safe_norm(np.cross(N0, T0))

    dL = candidates[:, 0]
    dA = candidates[:, 1]
    dN = candidates[:, 2]

    def _points(ca: float) -> np.ndarray:
        c = float(math.cos(ca))
        s = float(math.sin(ca))
        return (
            base[np.newaxis]
            + (dL * c + dN * s)[:, np.newaxis] * T0[np.newaxis]
            + dA[:, np.newaxis] * A0[np.newaxis]
            + (-dL * s + dN * c)[:, np.newaxis] * N0[np.newaxis]
        )

    eval_cache: dict[float, float] = {}

    def _max_inside(ca: float) -> float:
        ca = float(np.clip(ca, 0.0, (math.pi / 2.0) - 1e-5))
        key = round(ca, 12)
        if key in eval_cache:
            return eval_cache[key]
        pts = _points(ca)
        closest, _, tri_id = proximity.on_surface(pts)
        normals = mesh.face_normals[np.asarray(tri_id, dtype=np.int64)]
        signed = -np.einsum("ij,ij->i", pts - closest, normals)
        val = float(np.max(signed)) if len(signed) else -math.inf
        eval_cache[key] = val
        return val

    angle_max = (math.pi / 2.0) - 1e-5
    ca0 = float(np.clip(initial_angle, 0.0, angle_max))
    d0 = _max_inside(ca0)
    if abs(d0) <= contact_tol_mm:
        return ca0

    lo = ca0
    hi = ca0

    # Use the analytical sphere angle as a local predictor, then expand only
    # as far as the actual mesh requires.  This preserves arbitrary-mesh
    # correctness while avoiding a full 0..90 degree search for every leaf.
    step = math.radians(2.0)
    if d0 < 0.0:
        while hi < angle_max and _max_inside(hi) < 0.0:
            lo = hi
            hi = min(angle_max, hi + step)
            step *= 2.0
        if _max_inside(hi) < 0.0:
            return hi
    else:
        while lo > 0.0 and _max_inside(lo) > 0.0:
            hi = lo
            lo = max(0.0, lo - step)
            step *= 2.0
        if _max_inside(lo) > 0.0:
            return lo

    if _max_inside(lo) >= 0.0:
        return lo
    if _max_inside(hi) <= 0.0:
        return hi

    for _ in range(iterations):
        mid = 0.5 * (lo + hi)
        if _max_inside(mid) >= 0.0:
            hi = mid
        else:
            lo = mid
    return hi


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

    z_top_anchor      = float(row_zs[-1]) if row_zs else z_top
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

    parts: list[trimesh.Trimesh] = []
    _contact_candidates = _leaf_contact_candidates(**_leaf_kw)
    _proximity = trimesh.proximity.ProximityQuery(mesh)

    for row_idx, z_row in enumerate(row_zs):
        row_attempt = 0
        row_placed  = 0
        row_perim   = 0.0

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

        # Belly slice: mesh cross-section L/2 arc below this row.
        # Used to count n_col from the actual widest perimeter the leaves span.
        _s_row   = _avg_arc_for_z(z_row, meridians)
        _s_belly = max(0.0, _s_row - L / 2.0)
        _z_belly = _avg_z_for_arc(_s_belly, meridians)
        _belly_polys: list[tuple[np.ndarray, float]] = []
        if _z_belly < z_row - 1e-3:
            _belly_sec = mesh.section(
                plane_origin=np.array([0.0, 0.0, _z_belly]),
                plane_normal=np.array([0.0, 0.0, 1.0]),
            )
            if _belly_sec is not None:
                try:
                    _bp2d, _bxf = _belly_sec.to_planar()
                    for _bpoly in _bp2d.polygons_full:
                        _bc   = _bpoly.centroid
                        _bc4d = _bxf @ np.array([float(_bc.x), float(_bc.y), 0.0, 1.0])
                        _belly_polys.append((_bc4d[:3].copy(), float(_bpoly.length)))
                except Exception:
                    pass

        for poly in path2d.polygons_full:
            perim = float(poly.length)
            row_perim += perim
            if perim < 1e-3:
                continue

            c2d = poly.centroid
            c4d = xform @ np.array([float(c2d.x), float(c2d.y), 0.0, 1.0])
            centroid_3d = c4d[:3].copy()

            if _belly_polys:
                _cdists = [float(np.linalg.norm(bc[:2] - centroid_3d[:2]))
                           for bc, _ in _belly_polys]
                _belly_perim = _belly_polys[int(np.argmin(_cdists))][1]
            else:
                _belly_perim = perim
            n_col = max(1, int(math.ceil(_belly_perim / col_step)))

            _poly_coords = np.array(poly.exterior.coords, dtype=float)[:, :2]
            _cx2d, _cy2d = float(c2d.x), float(c2d.y)

            for ci in range(n_col):
                phi_2d = 2.0 * math.pi * float(ci) / float(n_col)
                _pt2d  = _polygon_point_at_phi(_poly_coords, _cx2d, _cy2d, phi_2d)
                if _pt2d is not None:
                    p4d  = xform @ np.array([float(_pt2d[0]), float(_pt2d[1]), 0.0, 1.0])
                    pt3d = p4d[:3].copy()
                else:
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

                row_attempt += 1

                outward  = pt3d - centroid_3d
                outward -= float(np.dot(outward, up_hint)) * up_hint
                plen     = float(np.linalg.norm(outward))
                if plen < 1e-6:
                    radial = np.array([math.cos(phi), math.sin(phi), 0.0])
                    radial -= float(np.dot(radial, up_hint)) * up_hint
                    T0 = _safe_norm(radial)
                else:
                    T0 = outward / plen

                ca_guess = _cached_ca(local_r)
                if ca_guess >= math.pi / 2:
                    ca_guess = (math.pi / 2.0) - 1e-5
                    stats.ca_clamped += 1

                ca = _contact_angle_for_mesh(
                    mesh,
                    _proximity,
                    pt3d,
                    T0,
                    up_hint,
                    _contact_candidates,
                    initial_angle=ca_guess,
                )

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
                root_depth = float(np.max(np.linalg.norm(root_v - perim_v, axis=1)))

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
