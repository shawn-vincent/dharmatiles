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
import rtree.index as _rtree_idx
import trimesh

from ._utils import _hash01, _safe_norm
from .leaf import (
    build_leaf_oval_offsets,
    build_leaf_surface,
    compute_leaf_geometry,
    solidify_leaf,
)
from .mesh import (
    _avg_arc_for_z,
    _avg_z_for_arc,
    _build_meridians,
    _compute_row_z_positions,
    _contact_angle_for_sphere,
    _curl_bottom_margin_mm,
    _hash01_int,
    _LEAF_PLACEABLE_NORMAL_Z,
)

# Curl-region burial depth (mm) above which a post-placement leaf is discarded.
# Leaves where _contact_angle_for_mesh could not find a valid contact angle and
# returned contact_angle_rad=0 (flat) may have their curl region genuinely inside
# the parent mesh.  This threshold matches the artifact-detector's buried-leaf
# threshold so that discarded leaves are exactly the ones that would have been flagged.
_PREBURIED_DEPTH_MM: float = 0.25
# Leaves whose midrib tips fall within this distance below the mesh floor are
# still placed.  The Tz-based bottom anchor can leave the lowest row's tip
# ~0.087mm below z_min due to arc/world-Z discrepancy on steep cone sections —
# well below print-layer resolution, so we allow it rather than leaving the
# bottom row bald.
_FLOOR_TOL_MM: float = 0.1


@dataclasses.dataclass
class LeafPlacementStats:
    """Metrics collected during leaf placement on one mesh object."""
    label: str = ""
    # Totals
    n_rows: int = 0
    n_attempted: int = 0       # candidates that reached the leaf-build step
    n_placed: int = 0          # leaves successfully solidified
    # Skip / error breakdown
    skipped_downward: int = 0   # up_hint.z < -0.5 (downward-facing surface)
    skipped_small_r: int = 0   # local_r < 1.0 mm (too close to centroid)
    skipped_preburied: int = 0  # post-placement: curl region buried > _PREBURIED_DEPTH_MM
    skipped_below_floor: int = 0  # midrib tip (base + L*tangent) below mesh z_min
    contact_angle_clamped: int = 0  # initial contact angle ≥ π/2, clamped to max
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
    lift_mm: float = 0.0             # leaf lift parameter used for this run


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
    initial_contact_angle_rad: float,
    iterations: int = 8,
    contact_tol_mm: float = 0.02,
    preburied_tol_mm: float = _PREBURIED_DEPTH_MM,
) -> float:
    """Find the contact angle (radians) where the curl region first touches *mesh*.

    The contact angle is the lean angle of the leaf: 0 = leaf grows flat along
    the surface (belly floats above); π/2 = leaf grows back into the surface
    (belly buried).  The returned angle is the smallest value where the belly
    just grazes the surface — the lean needed to press the leaf against the mesh.

    Distances are signed from the closest triangle normal: positive = inside the
    parent mesh, negative = outside.  The target is the smallest angle where any
    contact candidate reaches zero distance, which keeps the result tied to the
    actual parent mesh rather than a smooth sphere proxy.
    """
    base = np.asarray(base_pos, float)
    T0   = _safe_norm(np.asarray(tangent0, float))
    N0   = _safe_norm(np.asarray(up_hint, float))
    A0   = _safe_norm(np.cross(N0, T0))

    dL = candidates[:, 0]
    dA = candidates[:, 1]
    dN = candidates[:, 2]

    def _points(contact_angle_rad: float) -> np.ndarray:
        c = float(math.cos(contact_angle_rad))
        s = float(math.sin(contact_angle_rad))
        return (
            base[np.newaxis]
            + (dL * c + dN * s)[:, np.newaxis] * T0[np.newaxis]
            + dA[:, np.newaxis] * A0[np.newaxis]
            + (-dL * s + dN * c)[:, np.newaxis] * N0[np.newaxis]
        )

    angle_max = (math.pi / 2.0) - 1e-5
    # Negative contact angles lean the leaf outward (away from the surface),
    # lifting a buried belly clear.  Needed when T0 points into the mesh body
    # (e.g. at the tip of a steep cluster) and the belly is already buried at
    # ca=0.  Cap at -π/4 so leaves don't stand fully upright off the surface.
    angle_min = -math.pi / 4.0

    eval_cache: dict[float, float] = {}

    def _max_inside(contact_angle_rad: float) -> float:
        contact_angle_rad = float(np.clip(contact_angle_rad, angle_min, angle_max))
        key = round(contact_angle_rad, 12)
        if key in eval_cache:
            return eval_cache[key]
        pts = _points(contact_angle_rad)
        closest, _, tri_id = proximity.on_surface(pts)
        normals = mesh.face_normals[np.asarray(tri_id, dtype=np.int64)]
        signed = -np.einsum("ij,ij->i", pts - closest, normals)
        val = float(np.max(signed)) if len(signed) else -math.inf
        eval_cache[key] = val
        return val

    contact_angle_rad = float(np.clip(initial_contact_angle_rad, 0.0, angle_max))
    d0 = _max_inside(contact_angle_rad)
    if abs(d0) <= contact_tol_mm:
        return contact_angle_rad

    lo = contact_angle_rad
    hi = contact_angle_rad

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
        # Use contact_tol as the loop threshold so that a tiny positive
        # burial (within tolerance) is treated as "just touching" rather
        # than "still buried", preventing the doubling step from jumping
        # over a real zero-crossing that sits marginally above zero due
        # to mesh-vertex discretisation artifacts.
        while lo > angle_min and _max_inside(lo) > contact_tol_mm:
            hi = lo
            lo = max(angle_min, lo - step)
            step *= 2.0
        if _max_inside(lo) > contact_tol_mm:
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


@dataclasses.dataclass
class _MeshCtx:
    """Per-mesh working state threaded through place_leaves_on_multiple_meshes."""
    mesh:         trimesh.Trimesh
    proximity:    trimesh.proximity.ProximityQuery
    meridians:    list
    row_zs:       list[float]
    z_min:        float
    centroid_3d:  np.ndarray
    seed:         int
    stats:        LeafPlacementStats
    parts:        list   # list[trimesh.Trimesh], mutated in place
    # Scene-wide geometric index — shared by reference across every _MeshCtx in
    # one placement run so any leaf can query the full scene state.
    #
    # scene_proximity: ProximityQuery over all parent meshes concatenated.
    #     Built once in Phase 1; static for the lifetime of the run.
    #     Use for surface-normal / distance queries that should consider every
    #     parent cluster, not just this leaf's own mesh.
    #
    # placed_index: 3-D R-tree (rtree.index.Index, dimension=3) of every leaf
    #     successfully placed so far across ALL meshes.  Updated immediately
    #     after each placement.  Insert key = index into placed_meshes.
    #     Query with a 6-tuple (xmin, ymin, zmin, xmax, ymax, zmax) bbox.
    #
    # placed_meshes: list[trimesh.Trimesh] parallel to placed_index integer IDs.
    #     placed_meshes[i] is the solidified leaf whose bbox was inserted at key i.
    scene_proximity: trimesh.proximity.ProximityQuery
    placed_index:    _rtree_idx.Index     # 3-D R-tree, shared across all contexts
    placed_meshes:   list                 # list[trimesh.Trimesh], parallel to index IDs


@dataclasses.dataclass
class _LeafSlot:
    """All data required to place one leaf candidate.  Built by _collect_row_slots."""
    ctx:         _MeshCtx
    row_idx:     int
    xform:       np.ndarray
    poly_coords: np.ndarray
    cx2d:        float
    cy2d:        float
    n_col:       int
    centroid_3d: np.ndarray
    best_bi:     int
    belly_polys: list
    z_belly:     float
    poly:        object   # shapely Polygon
    ci:          int
    # Approximate 3-D base position on the parent mesh surface (computed from
    # the row cross-section polygon at this slot's azimuthal angle phi_2d =
    # 2π*ci/n_col).  Available before _place_leaf_slot runs; used for local
    # face assignment and future proximity queries.
    approx_base:  np.ndarray   # (3,) mm
    # Parent mesh faces whose centroid falls within L mm of approx_base.
    # Overlapping across adjacent slots is intentional — every face that
    # could plausibly interact with the leaf is included.
    local_faces:  np.ndarray   # (K,) int face indices into ctx.mesh
    # approx_base and local_faces are retained for future proximity work.


def _collect_row_slots(
    ctx: _MeshCtx,
    row_idx: int,
    z_row: float,
    *,
    L: float,
    W: float,
    col_step: float,
) -> tuple[list, float]:
    """Build leaf slots for one row on one mesh.

    For invalid/empty cross-sections, appends zero-row entries to ctx.stats
    directly and returns ([], 0.0) so the caller needs no special-case.
    For valid cross-sections with at least one slot, returns (slots, row_perim)
    and leaves ctx.stats updates to the caller (done after placement).
    """
    mesh      = ctx.mesh
    meridians = ctx.meridians
    stats     = ctx.stats

    sec = mesh.section(
        plane_origin = np.array([0.0, 0.0, z_row]),
        plane_normal = np.array([0.0, 0.0, 1.0]),
    )
    if sec is None:
        stats.rows.append((z_row, 0, 0))
        stats.row_perims.append(0.0)
        return [], 0.0
    try:
        path2d, xform = sec.to_planar()
    except Exception:
        stats.rows.append((z_row, 0, 0))
        stats.row_perims.append(0.0)
        return [], 0.0

    # Belly slice: mesh cross-section L/2 arc below this row.
    # Drives n_col (leaf count) AND supplies an alternative sampling polygon
    # when the row cross-section is a degenerate sliver.
    # Tuple: (centroid_3d, perim, coords2d, xform, cx2d, cy2d, shapely_poly)
    _s_row   = _avg_arc_for_z(z_row, meridians)
    _s_belly = max(0.0, _s_row - L / 2.0)
    _z_belly = _avg_z_for_arc(_s_belly, meridians)
    _belly_polys: list[tuple] = []
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
                    _belly_polys.append((
                        _bc4d[:3].copy(),
                        float(_bpoly.length),
                        np.array(_bpoly.exterior.coords, dtype=float)[:, :2],
                        _bxf.copy(),
                        float(_bc.x),
                        float(_bc.y),
                        _bpoly,
                    ))
            except Exception:
                pass

    try:
        _polygons_full = path2d.polygons_full
    except Exception:
        stats.rows.append((z_row, 0, 0))
        stats.row_perims.append(0.0)
        return [], 0.0

    row_perim = 0.0
    slots: list = []
    for poly in _polygons_full:
        if poly is None:
            continue
        perim = float(poly.length)
        row_perim += perim
        if perim < 1e-3:
            continue
        c2d = poly.centroid
        c4d = xform @ np.array([float(c2d.x), float(c2d.y), 0.0, 1.0])
        centroid_3d = c4d[:3].copy()
        if _belly_polys:
            _cdists      = [float(np.linalg.norm(bd[0][:2] - centroid_3d[:2]))
                            for bd in _belly_polys]
            _best_bi     = int(np.argmin(_cdists))
            _belly_perim = _belly_polys[_best_bi][1]
        else:
            _best_bi     = -1
            _belly_perim = perim
        n_col        = max(1, int(math.ceil(max(_belly_perim, perim) / col_step)))
        _poly_coords = np.array(poly.exterior.coords, dtype=float)[:, :2]
        _cx2d, _cy2d = float(c2d.x), float(c2d.y)
        _face_cents = mesh.triangles_center   # (F, 3) — trimesh-cached after first access
        _W_half = W / 2.0
        for ci in range(n_col):
            _phi_2d = 2.0 * math.pi * float(ci) / float(n_col)
            _pt2d   = _polygon_point_at_phi(_poly_coords, _cx2d, _cy2d, _phi_2d)
            if _pt2d is not None:
                _p4d         = xform @ np.array([float(_pt2d[0]), float(_pt2d[1]), 0.0, 1.0])
                _approx_base = _p4d[:3].copy()
            else:
                _t           = float(ci) / float(n_col)
                _pt2         = poly.exterior.interpolate(_t, normalized=True)
                _p4d         = xform @ np.array([float(_pt2.x), float(_pt2.y), 0.0, 1.0])
                _approx_base = _p4d[:3].copy()

            # Approximate leaf local frame from cross-section geometry.
            # up_approx  ≈ surface normal (outward from cross-section centroid).
            # T0_approx  ≈ steepest surface descent (world-down onto tangent plane).
            # lat_approx ≈ lateral direction across the leaf.
            _radial    = _approx_base - centroid_3d
            _rad_len   = float(np.linalg.norm(_radial))
            _up_approx = _radial / _rad_len if _rad_len > 1e-6 else np.array([0.0, 0.0, 1.0])
            _down      = np.array([0.0, 0.0, -1.0])
            _T0_raw    = _down - float(np.dot(_down, _up_approx)) * _up_approx
            _T0_len    = float(np.linalg.norm(_T0_raw))
            if _T0_len > 1e-6:
                _T0_approx = _T0_raw / _T0_len
            else:
                # Near apex: fall back to radial-outward in XY.
                _phi_ab    = math.atan2(_approx_base[1] - centroid_3d[1],
                                        _approx_base[0] - centroid_3d[0])
                _rad_xy    = np.array([math.cos(_phi_ab), math.sin(_phi_ab), 0.0])
                _rad_xy   -= float(np.dot(_rad_xy, _up_approx)) * _up_approx
                _T0_approx = _rad_xy / (float(np.linalg.norm(_rad_xy)) + 1e-9)
            _lat_approx = np.cross(_up_approx, _T0_approx)
            _lat_len    = float(np.linalg.norm(_lat_approx))
            if _lat_len > 1e-6:
                _lat_approx /= _lat_len

            # Ellipsoidal face filter: semi-axis L along growth, W/2 lateral.
            _delta       = _face_cents - _approx_base   # (F, 3)
            _dL          = _delta @ _T0_approx           # (F,)
            _dW          = _delta @ _lat_approx          # (F,)
            _local_faces = np.where(_dL**2 / L**2 + _dW**2 / _W_half**2 < 1.0)[0]

            slots.append(_LeafSlot(
                ctx=ctx, row_idx=row_idx,
                xform=xform, poly_coords=_poly_coords,
                cx2d=_cx2d, cy2d=_cy2d, n_col=n_col,
                centroid_3d=centroid_3d,
                best_bi=_best_bi, belly_polys=_belly_polys,
                z_belly=_z_belly, poly=poly, ci=ci,
                approx_base=_approx_base,
                local_faces=_local_faces,
            ))

    if not slots:
        stats.rows.append((z_row, 0, 0))
        stats.row_perims.append(row_perim)
        return [], row_perim

    return slots, row_perim


def _place_leaf_slot(
    slot: _LeafSlot,
    *,
    contact_candidates: np.ndarray,
    ca_cache: dict,
    leaf_kw: dict,
    angle_jitter_deg: float,
    pos_jitter: float,
    row_color_fn: Callable[[int], tuple[int, int, int, int]] | None,
    color_row_idx: int | None = None,
) -> tuple[int, int]:
    """Place one leaf for the given slot.  Mutates slot.ctx.stats and slot.ctx.parts.

    Returns (attempted, placed) as 0-or-1 integers so the caller can
    accumulate per-row counts across a shuffled batch.
    """
    ctx              = slot.ctx
    row_idx          = slot.row_idx
    ci               = slot.ci
    n_col            = slot.n_col
    xform            = slot.xform
    _poly_coords     = slot.poly_coords
    _cx2d, _cy2d    = slot.cx2d, slot.cy2d
    centroid_3d      = slot.centroid_3d
    _best_bi         = slot.best_bi
    _belly_polys     = slot.belly_polys
    _z_belly         = slot.z_belly
    poly             = slot.poly

    mesh             = ctx.mesh
    proximity        = ctx.proximity
    stats            = ctx.stats
    parts            = ctx.parts
    seed             = ctx.seed
    z_min_mesh       = ctx.z_min
    mesh_centroid_3d = ctx.centroid_3d

    L = float(leaf_kw["length_mm"])

    def _cached_ca(r: float) -> float:
        key = round(r * 1000)
        if key not in ca_cache:
            ca_cache[key] = _contact_angle_for_sphere(r, **leaf_kw)
        return ca_cache[key]

    phi_2d = 2.0 * math.pi * float(ci) / float(n_col)

    # Base position: always from the row cross-section.
    _pt2d = _polygon_point_at_phi(_poly_coords, _cx2d, _cy2d, phi_2d)
    if _pt2d is not None:
        p4d  = xform @ np.array([float(_pt2d[0]), float(_pt2d[1]), 0.0, 1.0])
        pt3d = p4d[:3].copy()
    else:
        t    = float(ci) / float(n_col)
        pt2  = poly.exterior.interpolate(t, normalized=True)
        p4d  = xform @ np.array([float(pt2.x), float(pt2.y), 0.0, 1.0])
        pt3d = p4d[:3].copy()

    # Normal position: belly cross-section at the same azimuthal angle.
    # Use the polygon-based belly point (original approach),
    # but detect cases where the 2-D coordinate system of the belly
    # section is rotated relative to the row section — this causes
    # a phi mismatch that twists the leaf frame and buries contact
    # candidates inside the mesh.  Mismatch is measured relative to
    # each section's own centroid (so that tilted cluster geometry,
    # where the centroid shifts with z, does not trigger the fallback).
    # When the local-phi mismatch exceeds ~5°, fall back to placing
    # the snap query at (belly_centroid + row_radial_offset, z_belly)
    # to keep the belly reference azimuthally aligned with the row base.
    if _best_bi >= 0:
        _bd = _belly_polys[_best_bi]
        _pt2d_n = _polygon_point_at_phi(_bd[2], _bd[4], _bd[5], phi_2d)
        if _pt2d_n is not None:
            _p4d_n = _bd[3] @ np.array([float(_pt2d_n[0]), float(_pt2d_n[1]), 0.0, 1.0])
            pt3d_n = _p4d_n[:3].copy()
        else:
            _t_n   = float(ci) / float(n_col)
            _pt2_n = _bd[6].exterior.interpolate(_t_n, normalized=True)
            _p4d_n = _bd[3] @ np.array([float(_pt2_n.x), float(_pt2_n.y), 0.0, 1.0])
            pt3d_n = _p4d_n[:3].copy()
        # Phi mismatch: compare azimuth of pt3d relative to its own
        # cross-section centroid vs azimuth of pt3d_n relative to the
        # belly centroid.  True 2-D rotation artefacts show up here;
        # geometric centroid shifts from tilting do not.
        _belly_c3d = _bd[0]
        _phi_row_local = math.atan2(
            pt3d[1]   - centroid_3d[1], pt3d[0]   - centroid_3d[0],
        )
        _phi_bel_local = math.atan2(
            pt3d_n[1] - _belly_c3d[1],  pt3d_n[0] - _belly_c3d[0],
        )
        _phi_err = abs(math.atan2(
            math.sin(_phi_bel_local - _phi_row_local),
            math.cos(_phi_bel_local - _phi_row_local),
        ))
        if _phi_err > math.radians(5.0) and abs(_z_belly - pt3d[2]) > 1e-3:
            # Translate pt3d's radial offset from its centroid to the
            # belly centroid's XY, then snap to the mesh surface.
            _rad_xy = np.array([
                _belly_c3d[0] + (pt3d[0] - centroid_3d[0]),
                _belly_c3d[1] + (pt3d[1] - centroid_3d[1]),
                _z_belly,
            ])
            _snap, _, _ = proximity.on_surface(_rad_xy[np.newaxis])
            pt3d_n = _snap[0].copy()
    else:
        pt3d_n = pt3d  # no belly available — use row base

    # Surface normal via proximity query at the belly position.
    # Barycentric-interpolated vertex normals give smooth results.
    _sp, _sd, _st = proximity.on_surface(pt3d_n[np.newaxis])
    _bary   = trimesh.triangles.points_to_barycentric(
        mesh.triangles[_st[0]][np.newaxis], _sp,
    )[0]
    up_hint = _safe_norm(
        _bary @ mesh.vertex_normals[mesh.faces[int(_st[0])]]
    )

    # When the belly is below the equator of the mesh (belly
    # normal.z < 0), projecting outward ⊥ to that downward normal
    # flips T0.z positive, which makes the leaf grow toward the
    # apex instead of away from it.  Fall back to the row
    # position's own normal, which is always on the upper
    # hemisphere for any leaf that passes the downward filter.
    if float(up_hint[2]) < 0.0:
        _sp_r, _, _st_r = proximity.on_surface(pt3d[np.newaxis])
        _bary_r = trimesh.triangles.points_to_barycentric(
            mesh.triangles[_st_r[0]][np.newaxis], _sp_r,
        )[0]
        up_hint = _safe_norm(
            _bary_r @ mesh.vertex_normals[mesh.faces[int(_st_r[0])]]
        )

    if float(up_hint[2]) < _LEAF_PLACEABLE_NORMAL_Z:
        stats.skipped_downward += 1
        return 0, 0

    local_r = float(np.linalg.norm(pt3d - mesh_centroid_3d))
    if local_r < 1.0:
        stats.skipped_small_r += 1
        return 0, 0

    # Leaf growth direction: steepest descent on the mesh surface.
    # Project world-down onto the local tangent plane.
    _d_raw = np.array([0.0, 0.0, -1.0])
    _d_raw -= float(np.dot(_d_raw, up_hint)) * up_hint
    plen   = float(np.linalg.norm(_d_raw))
    if plen < 1e-6:
        # Near-horizontal surface (apex): fall back to radially
        # outward from the row cross-section centroid.
        phi    = float(np.arctan2(
            pt3d[1] - centroid_3d[1], pt3d[0] - centroid_3d[0],
        ))
        radial = np.array([math.cos(phi), math.sin(phi), 0.0])
        radial -= float(np.dot(radial, up_hint)) * up_hint
        T0 = _safe_norm(radial)
    else:
        T0 = _d_raw / plen

    # Angle jitter: rotate T0 around up_hint so the leaf's growth
    # direction pivots in the surface tangent plane — base pinned,
    # tip swings azimuthally. Uses _hash01 (with fmix64 finalizer)
    # for proper per-leaf independence; _hash01_int without finalizer
    # has near-constant high bits when only ci varies in a row.
    if angle_jitter_deg != 0.0:
        _theta = math.radians(angle_jitter_deg) * (_hash01(seed, "ang_j", row_idx, ci) * 2.0 - 1.0)
        _ct, _st = math.cos(_theta), math.sin(_theta)
        T0 = _safe_norm(T0 * _ct + np.cross(up_hint, T0) * _st)

    # Position jitter: two independent offsets in the surface tangent
    # plane (along T0 and the lateral direction cross(up_hint, T0)),
    # then snap back to the mesh surface.  Done after T0 so the basis
    # is the true local surface frame, not world coordinates.
    if pos_jitter != 0.0:
        _jmm    = pos_jitter * L
        _lat    = np.cross(up_hint, T0)
        _r_t    = _hash01(seed, "pos_jt", row_idx, ci) * 2.0 - 1.0
        _r_l    = _hash01(seed, "pos_jl", row_idx, ci) * 2.0 - 1.0
        pt3d    = pt3d + T0 * (_jmm * _r_t) + _lat * (_jmm * _r_l)
        _snp, _, _ = proximity.on_surface(pt3d[np.newaxis])
        pt3d    = _snp[0].copy()

    contact_angle_guess_rad = _cached_ca(local_r)
    if contact_angle_guess_rad >= math.pi / 2:
        contact_angle_guess_rad = (math.pi / 2.0) - 1e-5
        stats.contact_angle_clamped += 1

    contact_angle_rad = _contact_angle_for_mesh(
        mesh,
        proximity,
        pt3d,
        T0,
        up_hint,
        contact_candidates,
        initial_contact_angle_rad=contact_angle_guess_rad,
    )
    tangent   = _safe_norm(
        T0 * math.cos(contact_angle_rad) - up_hint * math.sin(contact_angle_rad)
    )
    up_placed = _safe_norm(
        up_hint * math.cos(contact_angle_rad) + T0 * math.sin(contact_angle_rad)
    )

    # Skip leaves whose midrib tip would extend below the mesh floor.
    # The row anchor is set to keep tips above z_min, but the contact
    # angle (especially negative outward-lean values) can make the
    # tangent steeper than Tz-based estimation assumes.
    _tip_z = pt3d[2] + L * tangent[2]
    if _tip_z < z_min_mesh - _FLOOR_TOL_MM:
        stats.skipped_below_floor += 1
        return 1, 0

    lseed = int(_hash01_int(seed, "leaf", row_idx, ci))
    try:
        surf, geom = build_leaf_surface(
            base_pos = pt3d,
            tangent  = tangent,
            up_hint  = up_placed,
            seed     = lseed,
            **leaf_kw,
        )
        # Build the oval using the contact-angle-adjusted tangent so it follows
        # the actual leaf growth direction rather than the surface-tangent T0.
        # Using T0 overshoots on an inverted cone (narrow-at-base cluster):
        # the steep downward T0 extends the oval into the narrow base where the
        # lateral half-width W/2 exceeds the cluster radius.  Using 'tangent'
        # (which bends back toward the cluster via the contact angle) keeps the
        # oval in the wider part of the surface where it reliably fits.
        _lat_ov = np.cross(up_hint, tangent)
        _lat_ov_len = float(np.linalg.norm(_lat_ov))
        if _lat_ov_len > 1e-6:
            _lat_ov /= _lat_ov_len
        _oval_off = build_leaf_oval_offsets(
            n_hat=up_hint, T_along=tangent, across=_lat_ov,
            L=L, W=float(leaf_kw["width_mm"]),
        )
        inner_v = _oval_off + pt3d[np.newaxis]
        if not mesh.contains(inner_v).all():
            raise RuntimeError("oval not fully inside parent mesh")
        solid, _ = solidify_leaf(surf, inner_v)
    except (RuntimeError, ValueError):
        stats.build_errors += 1
        return 1, 0

    root_depth = 0.75  # embed_mm used by build_leaf_oval_offsets

    # Curl-region float / bury check
    base_dists_v = np.linalg.norm(surf.vertices - pt3d, axis=1)
    curl_mask    = base_dists_v > (L / 2.0)
    if curl_mask.any():
        curl_verts = surf.vertices[curl_mask]
        _, _curl_dists, _ = trimesh.proximity.closest_point(mesh, curl_verts)
        _inside   = mesh.contains(curl_verts)
        outside_d = _curl_dists[~_inside]
        inside_d  = _curl_dists[_inside]
        _float_d   = float(outside_d.max()) if len(outside_d) else 0.0
        _burial_d  = float(inside_d.max()) if len(inside_d) else 0.0
    else:
        _float_d  = 0.0
        _burial_d = 0.0

    # Discard leaves where the actual curl region is buried beyond
    # the visible-burial threshold.  This catches apex-row leaves
    # that _contact_angle_for_mesh placed at contact_angle_rad=0
    # (flat) because no valid contact angle existed there.
    # Using actual solid vertices (not the contact candidates) avoids
    # false positives on steep bottom-row leaves where the candidates
    # overreport burial due to canonical-frame mismatch.
    if _burial_d > _PREBURIED_DEPTH_MM:
        stats.skipped_preburied += 1
        return 1, 0

    stats.leaf_float_dists.append(_float_d)
    stats.leaf_buried_depths.append(_burial_d)
    stats.base_positions.append(pt3d.copy())
    stats.base_tangents.append(tangent.copy())
    stats.base_row_idx.append(row_idx)
    stats.root_depths.append(root_depth)
    stats.n_placed += 1

    if len(solid.vertices) > 0:
        if row_color_fn is not None:
            _cidx = color_row_idx if color_row_idx is not None else row_idx
            rgba  = row_color_fn(_cidx)
            color = np.asarray(rgba, dtype=np.uint8)
            solid.visual = trimesh.visual.ColorVisuals(
                mesh=solid,
                face_colors=np.tile(color, (len(solid.faces), 1)),
            )
        parts.append(solid)
        # Register in the shared scene-wide placed-leaf index.
        _leaf_id = len(ctx.placed_meshes)
        _bb = solid.bounds   # (2, 3): [[xmin,ymin,zmin],[xmax,ymax,zmax]]
        ctx.placed_index.insert(
            _leaf_id,
            (_bb[0, 0], _bb[0, 1], _bb[0, 2], _bb[1, 0], _bb[1, 1], _bb[1, 2]),
        )
        ctx.placed_meshes.append(solid)

    return 1, 1


def place_leaves_on_multiple_meshes(
    meshes: list[trimesh.Trimesh],
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
    seeds: int | list[int] = 0,
    labels: str | list[str] | None = None,
    angle_jitter_deg: float = 0.0,
    pos_jitter: float = 0.0,
    row_color_fn: Callable[[int], tuple[int, int, int, int]] | None = None,
) -> tuple[list[list[trimesh.Trimesh]], list[LeafPlacementStats]]:
    """Meridian-arc leaf placement across multiple meshes, interleaved by global z.

    Phase 1 — pre-computation (all meshes before any leaf is placed):
      For each mesh: build meridians, compute arc-equidistant row z-positions,
      construct a BVH proximity object, and initialise per-mesh stats.
      All (z, mesh_idx, row_idx) tuples are collected and sorted by z.

    Phase 2 — placement (bottom-to-top across all meshes):
      Rows are visited in ascending global z order, regardless of which mesh
      they belong to.  Each row dispatches to _place_row_on_mesh, which uses
      only that mesh's BVH and surface normals for solidify_leaf.

    The contact-angle sphere cache (keyed by local radius in µm) is shared
    across all meshes because leaf_kw is identical for every mesh.

    Parameters
    ----------
    meshes:
        One or more closed, watertight trimeshes to cover with leaves.
    seeds:
        Per-mesh RNG seed(s).  A single int is broadcast to all meshes.
    labels:
        Per-mesh labels for LeafPlacementStats.  A single str is broadcast.
        Defaults to "mesh-0", "mesh-1", … when None.

    All other parameters mirror :func:`place_leaves_on_mesh`.

    Returns
    -------
    (parts_per_mesh, stats_per_mesh)
        parts_per_mesh[i] — list of solidified leaf Trimesh objects for mesh i.
        stats_per_mesh[i] — LeafPlacementStats for mesh i.
    """
    n = len(meshes)
    seeds_list: list[int] = [seeds] * n if isinstance(seeds, int) else list(seeds)
    if labels is None:
        labels_list: list[str] = [f"mesh-{i}" for i in range(n)]
    elif isinstance(labels, str):
        labels_list = [labels] * n
    else:
        labels_list = list(labels)

    L   = float(length_mm)
    W   = float(width_mm)
    hov = float(h_overlap)
    vov = float(v_overlap)
    col_step          = max(W * (1.0 - hov), 1e-3)
    expected_row_step = L * max(1.0 - vov, 0.05)
    leaf_kw = dict(
        length_mm      = L,
        width_mm       = W,
        thickness_mm   = float(thickness_mm),
        fold_angle_deg = float(fold_angle_deg),
        inner_curve    = float(inner_curve),
        outer_curve    = float(outer_curve),
        curl_deg       = float(curl_deg),
        lift_mm        = float(lift_mm),
    )
    contact_candidates = _leaf_contact_candidates(**leaf_kw)
    ca_cache: dict[int, float] = {}   # shared: leaf_kw is identical for all meshes

    # ── Phase 1: pre-computation for every mesh ───────────────────────────────

    # Scene-wide proximity: BVH over every parent mesh concatenated.
    _scene_mesh = trimesh.util.concatenate(meshes) if len(meshes) > 1 else meshes[0]
    _scene_proximity = trimesh.proximity.ProximityQuery(_scene_mesh)

    # Shared 3-D R-tree for placed leaves — updated after every successful placement.
    _rtree_prop = _rtree_idx.Property()
    _rtree_prop.dimension = 3
    _placed_index: _rtree_idx.Index = _rtree_idx.Index(properties=_rtree_prop)
    _placed_meshes: list[trimesh.Trimesh] = []

    contexts: list[_MeshCtx] = []
    all_rows: list[tuple[float, int, int]] = []   # (z, mesh_idx, per_mesh_row_idx)

    for mi, (mesh, seed, label) in enumerate(zip(meshes, seeds_list, labels_list)):
        z_top       = float(mesh.vertices[:, 2].max())
        cx          = float(mesh.vertices[:, 0].mean())
        cy          = float(mesh.vertices[:, 1].mean())
        centroid_3d = np.array([cx, cy, float(mesh.vertices[:, 2].mean())])
        meridians   = _build_meridians(mesh, n_meridians=n_meridians, z_samples=z_samples)
        row_zs      = _compute_row_z_positions(
            meridians, L, vov, z_top,
            bottom_extra_mm=_curl_bottom_margin_mm(L, curl_deg, lift_mm),
        )
        stats = LeafPlacementStats(
            label             = label,
            leaf_length_mm    = L,
            leaf_width_mm     = W,
            col_step          = col_step,
            expected_row_step = expected_row_step,
            z_top             = z_top,
            z_top_anchor      = float(row_zs[-1]) if row_zs else z_top,
            cx                = cx,
            cy                = cy,
            lift_mm           = float(lift_mm),
        )
        ctx = _MeshCtx(
            mesh             = mesh,
            proximity        = trimesh.proximity.ProximityQuery(mesh),
            meridians        = meridians,
            row_zs           = row_zs,
            z_min            = float(mesh.vertices[:, 2].min()),
            centroid_3d      = centroid_3d,
            seed             = seed,
            stats            = stats,
            parts            = [],
            scene_proximity  = _scene_proximity,
            placed_index     = _placed_index,
            placed_meshes    = _placed_meshes,
        )
        contexts.append(ctx)
        for ri, z in enumerate(row_zs):
            all_rows.append((z, mi, ri))

    # ── Phase 2: place rows in ascending global z, batching same-z rows ─────────
    # Rows at exactly the same z are collected into one batch; their combined
    # slots are shuffled together so placement order is random across all meshes
    # contributing to that z level.  In the common case (rows at distinct z
    # values) each batch contains exactly one row, but the same-z path is
    # exercised when two meshes happen to align at a cross-section level.
    all_rows.sort(key=lambda r: r[0])
    _ctx_id_to_mi = {id(ctx): mi for mi, ctx in enumerate(contexts)}
    _n_total = len(all_rows)
    _step = 0
    i = 0
    while i < len(all_rows):
        # Extend the batch to all rows sharing the same z value.
        z0 = all_rows[i][0]
        j  = i + 1
        while j < len(all_rows) and all_rows[j][0] == z0:
            j += 1
        batch = all_rows[i:j]

        batch_slots: list = []
        row_perims: dict  = {}
        valid_rows: set   = set()

        for z_row, mi, row_idx in batch:
            slots, perim = _collect_row_slots(
                contexts[mi], row_idx, z_row, L=L, W=W, col_step=col_step,
            )
            row_perims[(mi, row_idx)] = perim
            if slots:
                valid_rows.add((mi, row_idx))
                batch_slots.extend(slots)
            # _collect_row_slots already appended zero-row stats for invalid/empty rows.

        if batch_slots:
            # One shuffle across all slots at this z step.
            _rng   = np.random.default_rng(_step)
            _order = _rng.permutation(len(batch_slots))

            _row_att: dict = {key: 0 for key in valid_rows}
            _row_pl:  dict = {key: 0 for key in valid_rows}

            for k in _order:
                slot = batch_slots[k]
                mi_k = _ctx_id_to_mi[id(slot.ctx)]
                att, pl = _place_leaf_slot(
                    slot,
                    contact_candidates=contact_candidates, ca_cache=ca_cache,
                    leaf_kw=leaf_kw, angle_jitter_deg=angle_jitter_deg,
                    pos_jitter=pos_jitter, row_color_fn=row_color_fn,
                    color_row_idx=round(_step * 15 / max(_n_total - 1, 1)),
                )
                key = (mi_k, slot.row_idx)
                _row_att[key] += att
                _row_pl[key]  += pl

            for z_row, mi, row_idx in batch:
                key = (mi, row_idx)
                if key not in valid_rows:
                    continue  # already handled by _collect_row_slots
                att   = _row_att[key]
                pl    = _row_pl[key]
                perim = row_perims[key]
                contexts[mi].stats.rows.append((z_row, att, pl))
                contexts[mi].stats.row_perims.append(perim)
                contexts[mi].stats.n_attempted += att

        i = j
        _step += 1

    for ctx in contexts:
        ctx.stats.n_rows = len(ctx.row_zs)

    return [ctx.parts for ctx in contexts], [ctx.stats for ctx in contexts]


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
    angle_jitter_deg: float = 0.0,
    pos_jitter: float = 0.0,
    row_color_fn: Callable[[int], tuple[int, int, int, int]] | None = None,
) -> tuple[list[trimesh.Trimesh], LeafPlacementStats]:
    """Place leaves on a single mesh.  Delegates to place_leaves_on_multiple_meshes."""
    parts_list, stats_list = place_leaves_on_multiple_meshes(
        [mesh],
        length_mm=length_mm, width_mm=width_mm, thickness_mm=thickness_mm,
        fold_angle_deg=fold_angle_deg, inner_curve=inner_curve,
        outer_curve=outer_curve, curl_deg=curl_deg, lift_mm=lift_mm,
        h_overlap=h_overlap, v_overlap=v_overlap,
        n_meridians=n_meridians, z_samples=z_samples,
        seeds=seed, labels=label,
        angle_jitter_deg=angle_jitter_deg, pos_jitter=pos_jitter,
        row_color_fn=row_color_fn,
    )
    return parts_list[0], stats_list[0]
