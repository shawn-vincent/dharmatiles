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
import time
from collections.abc import Callable

import numpy as np
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
# Maximum distance (mm) an embedded leaf-oval vertex may protrude outside the
# parent mesh and still be accepted.  The oval is the leaf's root, pushed
# embed_mm (=0.75) into the surface; on a convex or tilted cluster the straight
# oval overshoots where the surface curves away, poking a few tip-end vertices
# out.  As long as the protrusion is ≤ the embed depth the root's outer skin is
# still at/inside the surface, so the leaf plugs in cleanly and the blade hides
# it.  A strict all-inside test rejected such leaves and left bald stripes on
# the leeward/windward faces of tilted clusters; this tolerance keeps them.
_OVAL_PROTRUSION_TOL_MM: float = 0.75
# Minimum world-Z clearance required between the visible blade tip and the
# embedded root-oval tip.  The root remains plugged at the parent surface; only
# the blade surface is raised, and only by the sub-mm amount needed to preserve
# the expected visual ordering.
_TIP_Z_CLEARANCE_MM: float = 0.02

# ── Imbrication (shingling) ───────────────────────────────────────────────────
# Overlapping leaves are staggered along the surface normal so they stack into
# layers instead of phasing through one another coplanar.  A coarse world-space
# voxel occupancy grid, shared across all meshes in a placement run, records per
# cell a bitmask of occupied normal-offset *layers*.  Each new leaf takes the
# lowest layer free across the cells its W×L footprint covers (greedy lowest-free
# colouring, which bounds the layer count to the local overlap multiplicity),
# then stands its *blade* off the surface by layer×_SHINGLE_DELTA_MM along the
# surface normal.  The root oval stays plugged at the surface, so the leaf never
# detaches — solidify_leaf just builds a taller connecting neck.  Fully
# proactive: the layer is chosen before any geometry is built; no BVH, no
# closest_point, no reactive retry.
_SHINGLE_MAX_LAYERS: int = 4        # cap on distinct layers (bounds total standoff)
_SHINGLE_DELTA_MM: float = 0.30     # per-layer outward standoff (≈ leaf thickness)
_SHINGLE_WORLD_CELL_MM: float = 1.25  # voxel edge length (≈ 1/4 of the default 4.5mm leaf)
# Footprint is the leaf's overlapping *core*, scaled below its full W×L so that
# merely touch-packed leaves (spaced exactly one leaf-width/length apart, as at
# h_overlap=v_overlap=0) do NOT register as overlapping and stay on layer 0.
# Only leaves that genuinely overlap by more than (1−scale) of a dimension get
# bumped to a higher shingle layer.
_SHINGLE_FOOTPRINT_SCALE: float = 0.80

# ── Cross-cluster culling ─────────────────────────────────────────────────────
# When foliage clusters intersect, a leaf on one cluster can grow across into a
# neighbour ("half cut off" where the neighbour's skin clips the blade, or a
# blade-on-blade skewer).  Every leaf is tested against neighbouring cluster
# solids via embree `mesh.contains` (fast inside/outside; embreex is a hard dep):
#   - a cheap pre-build pass on the midrib centerline drops obviously-buried
#     slots before geometry is built;
#   - a final pass on the built + pulled-away blade surface removes leaves that
#     intersect a neighbour with their blade width / curl, or that pull-away
#     pushed in (the centerline pass, run before pull-away, misses these).
# A leaf is culled when at least this FRACTION of its blade-surface vertices land
# inside a neighbouring cluster.  Fraction (not absolute count) so the threshold
# is independent of leaf tessellation.  This removes skewering leaves (blade
# straddling a neighbour's skin) and fully-buried leaves (invisible — dropping
# them also cuts face count), while keeping leaves that merely graze a seam.
_CROSS_CLUSTER_BLADE_INSIDE_FRAC: float = 0.30

# ── Analytic contact angle + normal pull-away ─────────────────────────────────
# The old contact solve made the lean angle do two jobs at once: orient the leaf
# AND keep it from penetrating the parent mesh.  The non-penetration job forced
# sub-degree precision, so it was solved by a ~9-eval mesh bisection over a
# ~140-point candidate cloud — the dominant cost of tree-tile generation.
#
# These two jobs are decoupled here:
#   1. Lean is solved in CLOSED FORM (_contact_angle_analytic) — the belly-dip
#      grazing angle against a locally-planar surface model.  No iteration.
#   2. Non-penetration is handled by a separate outward TRANSLATION of the blade
#      surface along the surface normal ("pull-away") until it clears the mesh.
#      This is a translation, NOT `lift` (which is a rotation about the base) and
#      NOT the shingle standoff (which stacks overlapping leaves).  The root oval
#      stays plugged at the surface; solidify_leaf just builds a taller neck, the
#      same mechanism the shingle offset already relies on.
# Because pull-away absorbs any residual penetration, the closed-form lean need
# not be exact.  Consequence: leaves stand off slightly more on strongly-curved
# regions (apex, small clusters), and buried leaves that the old path REJECTED
# now get pulled out and placed (coverage can rise).
_ANALYTIC_CONTACT: bool = True
# Extra outward clearance (mm) added on top of the measured penetration depth.
_PULL_CLEARANCE_MM: float = 0.05
# Max pull-away (mm).  A leaf still penetrating by more than this after the
# analytic lean is genuinely stuck inside the mesh and is rejected (preburied).
_PULL_MAX_MM: float = 3.0
# When True, measure the surface normal at the belly-dip point (1 extra proximity
# eval per leaf) to capture local curvature and reduce standoff; when False, use
# the base normal (0 extra evals — leaves stand off more on curved regions).
_ANALYTIC_MEASURE_BELLY: bool = True


def _shingle_world_cells(
    base: np.ndarray,
    tangent: np.ndarray,
    lat: np.ndarray,
    W: float,
    L: float,
    cell_mm: float,
) -> list[tuple[int, int, int]]:
    """World-voxel cells covered by a leaf's W×L footprint core.

    The footprint is sampled from the unoffset surface position, not the
    layer-offset blade position, so two overlapping leaves map to the same cells
    regardless of which standoff layer they receive.
    """
    cell = max(float(cell_mm), 1e-3)
    W_eff = float(W) * _SHINGLE_FOOTPRINT_SCALE
    L_eff = float(L) * _SHINGLE_FOOTPRINT_SCALE
    # Keep the reduced footprint centred on the physical blade span from base
    # to tip: at scale=0.8 this samples 10%..90% of the length.
    along = np.linspace((float(L) - L_eff) / 2.0, (float(L) + L_eff) / 2.0, 5)
    across = np.linspace(-W_eff / 2.0, W_eff / 2.0, 3)
    cells: set[tuple[int, int, int]] = set()
    base_arr = np.asarray(base, dtype=float)
    T = _safe_norm(np.asarray(tangent, dtype=float))
    A = _safe_norm(np.asarray(lat, dtype=float))
    for a in along:
        for w in across:
            p = base_arr + a * T + w * A
            cells.add((
                int(math.floor(float(p[0]) / cell)),
                int(math.floor(float(p[1]) / cell)),
                int(math.floor(float(p[2]) / cell)),
            ))
    return list(cells)


def _shingle_pick_layer(occ: dict, cells: list[tuple[int, ...]]) -> int:
    """Lowest normal-offset layer free across every covered cell (greedy colouring).

    ORs the layer bitmasks of all covered cells, then returns the index of the
    lowest clear bit.  Clamps to _SHINGLE_MAX_LAYERS-1 so total standoff stays
    bounded even in pathologically dense regions (leaves there reuse the top
    layer rather than climbing indefinitely).
    """
    mask = 0
    for c in cells:
        mask |= occ.get(c, 0)
    layer = 0
    while layer < _SHINGLE_MAX_LAYERS and (mask >> layer) & 1:
        layer += 1
    return min(layer, _SHINGLE_MAX_LAYERS - 1)


def _shingle_write(occ: dict, cells: list[tuple[int, ...]], layer: int) -> None:
    """Mark *layer* occupied in every covered cell."""
    bit = 1 << layer
    for c in cells:
        occ[c] = occ.get(c, 0) | bit


# ── Profiling accumulators (cleared and printed by place_leaves_on_multiple_meshes) ──
_PROF: dict[str, float] = {}
_PROF_N: dict[str, int] = {}

def _pt(key: str, dt: float, n: int = 1) -> None:
    _PROF[key]  = _PROF.get(key,  0.0) + dt
    _PROF_N[key] = _PROF_N.get(key, 0) + n

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
    skipped_cross_buried: int = 0  # base inside another cluster's solid (Option A cull)
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
    # Per-leaf shingle layer (0 = flat on surface, n = stood off by n×delta).
    shingle_layers: list[int] = dataclasses.field(default_factory=list)
    # Per-leaf: visible blade-tip z minus embedded root-oval-tip z, after any
    # tiny blade-only correction.  Should never be negative for placed leaves.
    tip_z_clearances: list[float] = dataclasses.field(default_factory=list)
    tip_z_lifts: list[float] = dataclasses.field(default_factory=list)
    # Per-leaf: outward normal translation applied to slide the blade clear of the
    # parent mesh (analytic-contact path).  0 = blade already clear at the lean.
    pull_aways: list[float] = dataclasses.field(default_factory=list)
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


def _leaf_belly_dip(
    *,
    length_mm:      float,
    width_mm:       float,
    thickness_mm:   float,
    fold_angle_deg: float,
    inner_curve:    float,
    outer_curve:    float,
    curl_deg:       float,
    lift_mm:        float,
) -> tuple[float, float]:
    """Canonical (d_along, d_normal) of the leaf's belly-dip vertex.

    The belly dip is the tip-half midrib vertex with the smallest normal
    displacement — the point that presses into the parent mesh first as the
    contact angle grows (the binding constraint used by _contact_angle_for_sphere
    and now by _contact_angle_analytic).  Shape-only: independent of placement, so
    it is computed once per placement run and threaded into every slot.
    """
    g = compute_leaf_geometry(
        base_pos=np.zeros(3, dtype=float),
        tangent=np.array([1.0, 0.0, 0.0]),
        up_hint=np.array([0.0, 0.0, 1.0]),
        length_mm=length_mm, width_mm=width_mm,
        thickness_mm=thickness_mm, fold_angle_deg=fold_angle_deg,
        inner_curve=inner_curve, outer_curve=outer_curve,
        curl_deg=curl_deg, lift_mm=0.0,
    )
    col      = g.lower_grid.shape[1] // 2
    tip_half = g.s_int > 0.5
    mid_pos  = g.lower_grid[tip_half, col, :]
    cands    = np.vstack([mid_pos, g.tip_pt[np.newaxis]])
    d        = cands - g.base_pt[np.newaxis]
    d_along  = d @ g.along_axis
    d_normal = d @ g.normal_axis
    dip      = int(np.argmin(d_normal))
    return float(d_along[dip]), float(d_normal[dip])


def _contact_angle_analytic(
    dL: float,
    dN: float,
    T0: np.ndarray,
    up_hint: np.ndarray,
    *,
    m: np.ndarray | None = None,
    D0: float = 0.0,
    angle_min: float = -math.pi / 4.0,
    angle_max: float = math.pi / 2.0 - 1e-5,
) -> float:
    """Closed-form contact (lean) angle so the belly-dip grazes the local surface.

    The parent surface is modelled locally as a plane with outward normal ``m``
    (defaults to the base normal ``up_hint``).  Rotating the belly-dip
    displacement (dL, dN) about the base in the (T0, up_hint) plane by θ using the
    same rotation the mesh solver applied::

        p(θ) = base + (dL·cosθ + dN·sinθ)·T0 + (−dL·sinθ + dN·cosθ)·up_hint

    its inside-depth against the plane is ``A·cosθ + B·sinθ`` with
    ``A = dL·t + dN·n``, ``B = dN·t − dL·n`` (t = T0·m, n = up_hint·m).  Setting
    that equal to the base inside-depth ``D0`` gives θ in closed form.  The lean
    closest to flat is returned; residual penetration from surface curvature is
    absorbed by the outward pull-away, so a plane model is sufficient.
    """
    N0 = up_hint
    if m is None:
        m = N0
    t = float(np.dot(T0, m))
    n = float(np.dot(N0, m))
    A = dL * t + dN * n
    B = dN * t - dL * n
    R = math.hypot(A, B)
    if R < 1e-9:
        return 0.0
    phi = math.atan2(B, A)
    ratio = D0 / R
    if ratio >= 1.0 or ratio <= -1.0:
        # Belly cannot reach the surface by rotation alone; lean toward the
        # binding phase and let pull-away resolve the remaining penetration.
        return float(np.clip(phi, angle_min, angle_max))
    d = math.acos(ratio)
    # Two roots symmetric about phi; normalise to (−π, π] and take the lean
    # closest to flat that lies in the printable band.
    cand = [
        math.atan2(math.sin(phi - d), math.cos(phi - d)),
        math.atan2(math.sin(phi + d), math.cos(phi + d)),
    ]
    valid = [c for c in cand if angle_min - 1e-6 <= c <= angle_max + 1e-6]
    pick  = min(valid, key=abs) if valid else min(cand, key=abs)
    return float(np.clip(pick, angle_min, angle_max))


def _inside_neighbour(
    other_meshes: list,
    pts: np.ndarray,
    near_pt: np.ndarray,
    reach: float,
    min_frac: float,
) -> bool:
    """True if ≥ ``min_frac`` of ``pts`` land inside any one neighbouring solid.

    Uses embree ``mesh.contains`` (fast; a hard dep) rather than
    ``signed_distance`` (rtree closest-point, ~15× slower) because the cull only
    needs inside/outside.  A cheap per-neighbour AABB reject keyed on ``near_pt``
    (expanded by ``reach``, one leaf length) skips neighbours the leaf cannot
    touch, so the O(leaves × neighbours) test stays bounded on dense canopies.
    """
    n = len(pts)
    if n == 0:
        return False
    thresh = max(1, int(math.ceil(min_frac * n)))
    for _om in other_meshes:
        _b = _om.bounds
        if (near_pt < _b[0] - reach).any() or (near_pt > _b[1] + reach).any():
            continue
        if int(np.count_nonzero(_om.contains(pts))) >= thresh:
            return True
    return False


def _contact_angle_for_mesh(
    mesh: trimesh.Trimesh,
    proximity: trimesh.proximity.ProximityQuery,
    base_pos: np.ndarray,
    tangent0: np.ndarray,
    up_hint: np.ndarray,
    candidates: np.ndarray,
    *,
    initial_contact_angle_rad: float,
    iterations: int = 5,
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
        _t_eval = time.perf_counter()
        closest, _, tri_id = proximity.on_surface(pts)
        normals = mesh.face_normals[np.asarray(tri_id, dtype=np.int64)]
        signed = -np.einsum("ij,ij->i", pts - closest, normals)
        val = float(np.max(signed)) if len(signed) else -math.inf
        _pt("contact.bvh_on_surface", time.perf_counter() - _t_eval, len(pts))
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
    # Shingle occupancy grid: world voxel key → bitmask of occupied layers.
    # Shared across all mesh contexts in a multi-mesh placement run; grows as
    # leaves are placed.
    occ:          dict = dataclasses.field(default_factory=dict)
    # Proximity queries for every OTHER cluster (Option A cross-cluster cull).
    # A leaf slot whose base lies inside any of these solids is skipped.
    other_meshes: list = dataclasses.field(default_factory=list)


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
    # Meridional surface arc-length at this row's z.  Retained for diagnostics
    # and row/belly geometry; shingle occupancy is world-space.
    s_row:       float
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
                s_row=_s_row,
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
    belly_dip: tuple[float, float],
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

    _t_contact = time.perf_counter()
    if _ANALYTIC_CONTACT:
        # Closed-form lean from the belly-dip; non-penetration is handled later by
        # the outward pull-away, so no mesh bisection is needed.
        _m = None
        if _ANALYTIC_MEASURE_BELLY:
            _p0 = pt3d + belly_dip[0] * T0 + belly_dip[1] * up_hint
            _mc, _, _mt = proximity.on_surface(_p0[np.newaxis])
            _m = mesh.face_normals[int(_mt[0])]
        contact_angle_rad = _contact_angle_analytic(
            belly_dip[0], belly_dip[1], T0, up_hint, m=_m,
        )
    else:
        contact_angle_rad = _contact_angle_for_mesh(
            mesh,
            proximity,
            pt3d,
            T0,
            up_hint,
            contact_candidates,
            initial_contact_angle_rad=contact_angle_guess_rad,
        )
    _pt("slot.contact_angle", time.perf_counter() - _t_contact)
    tangent   = _safe_norm(
        T0 * math.cos(contact_angle_rad) - up_hint * math.sin(contact_angle_rad)
    )
    up_placed = _safe_norm(
        up_hint * math.cos(contact_angle_rad) + T0 * math.sin(contact_angle_rad)
    )

    # Cross-cluster culling happens AFTER the blade is built and pulled away (see
    # the blade cull further down).  A pre-build midrib/base test was tried and
    # removed: in a dense canopy a cluster's own surface is frequently inside a
    # neighbour, so any base/centerline-inside test culls almost every leaf, not
    # just the ones that visibly skewer through a neighbour's skin.

    # ── Imbrication (shingling) ────────────────────────────────────────────
    # Decide this leaf's normal-offset layer from the shared world-space
    # occupancy grid, then stand the blade off the surface by layer×delta along
    # the surface normal (up_hint).  The footprint is keyed from pt3d (surface
    # depth, before standoff), so overlapping leaves share cells regardless of
    # which layer they eventually receive.
    _t_shingle = time.perf_counter()
    _lat = np.cross(up_hint, tangent)
    _lat_len = float(np.linalg.norm(_lat))
    if _lat_len > 1e-6:
        _lat /= _lat_len
    _cells = _shingle_world_cells(
        pt3d, tangent, _lat, float(leaf_kw["width_mm"]), L, _SHINGLE_WORLD_CELL_MM,
    )
    _layer  = _shingle_pick_layer(ctx.occ, _cells)
    _delta  = float(_layer) * _SHINGLE_DELTA_MM
    base_blade = pt3d + _delta * up_hint
    _pt("slot.shingle", time.perf_counter() - _t_shingle)

    # Skip leaves whose midrib tip would extend below the mesh floor.
    # The row anchor is set to keep tips above z_min, but the contact
    # angle (especially negative outward-lean values) can make the
    # tangent steeper than Tz-based estimation assumes.
    _tip_z = base_blade[2] + L * tangent[2]
    if _tip_z < z_min_mesh - _FLOOR_TOL_MM:
        stats.skipped_below_floor += 1
        return 1, 0

    lseed = int(_hash01_int(seed, "leaf", row_idx, ci))
    _t_build = time.perf_counter()
    try:
        surf, geom = build_leaf_surface(
            base_pos = base_blade,
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
        _t_oval_contains = time.perf_counter()
        _outside = ~mesh.contains(inner_v)
        _pt("slot.oval_contains", time.perf_counter() - _t_oval_contains, len(inner_v))
        if _outside.any():
            # Accept small overshoots: only reject when the deepest protruding
            # oval vertex clears the embed depth (root no longer reaches the
            # surface).  See _OVAL_PROTRUSION_TOL_MM.
            _t_oval_prox = time.perf_counter()
            _, _ov_d, _ = trimesh.proximity.closest_point(mesh, inner_v[_outside])
            _pt("slot.oval_proximity", time.perf_counter() - _t_oval_prox, int(_outside.sum()))
            if float(_ov_d.max()) > _OVAL_PROTRUSION_TOL_MM:
                raise RuntimeError("oval protrudes past embed depth")
        _tip_idx = len(surf.vertices) - 1
        solid, _ = solidify_leaf(surf, inner_v)
    except (RuntimeError, ValueError):
        stats.build_errors += 1
        return 1, 0
    _pt("slot.initial_build", time.perf_counter() - _t_build)

    root_depth = 0.75  # embed_mm used by build_leaf_oval_offsets

    # ── Penetration measurement → normal pull-away ─────────────────────────────
    # Signed depth of every blade-surface vertex from its closest triangle (inside
    # positive — the same normal-sign convention the contact solve used).  On the
    # analytic-contact path the blade is then translated OUT along up_hint by the
    # deepest penetration + clearance, sliding it clear of the parent mesh.  This
    # is a pure translation (distinct from `lift`, a rotation) and sets the leaf's
    # standoff so the closed-form lean need not be exact.  The root oval stays
    # plugged at pt3d.  Leaves that the old bisection path would have rejected as
    # preburied are pulled out and placed instead.
    _t_curl = time.perf_counter()
    _cl, _, _ti = proximity.on_surface(surf.vertices)
    _nrm    = mesh.face_normals[np.asarray(_ti, dtype=np.int64)]
    _signed = -np.einsum("ij,ij->i", surf.vertices - _cl, _nrm)   # inside positive
    _curl_mask = np.linalg.norm(surf.vertices - pt3d, axis=1) > (L / 2.0)
    _pt("slot.curl_check", time.perf_counter() - _t_curl)

    # Penetration is measured on the curl/blade region only (> L/2 from the
    # base).  The base region is meant to sit at/in the surface (it plugs the root
    # oval), so including it would inflate the pull-away and float the whole leaf.
    _curl_signed = _signed[_curl_mask] if _curl_mask.any() else np.empty(0)
    _burial_d  = float(_curl_signed.max()) if len(_curl_signed) else 0.0
    _pull_away = 0.0
    if _ANALYTIC_CONTACT:
        if _burial_d > _PULL_MAX_MM:
            # Genuinely stuck deep inside the mesh — no reasonable standoff clears
            # it; treat as preburied (matches the old rejection semantics).
            stats.skipped_preburied += 1
            return 1, 0
        if _burial_d > 0.0:
            _pull_away = _burial_d + _PULL_CLEARANCE_MM
            _pull_vec  = _pull_away * up_hint
            surf.vertices                       += _pull_vec
            solid.vertices[:len(surf.vertices)] += _pull_vec
            _signed  -= _pull_away
            _burial_d = 0.0
    elif _burial_d > _PREBURIED_DEPTH_MM:
        # Legacy path: discard leaves whose curl region is buried beyond the
        # visible-burial threshold instead of pulling them out.
        stats.skipped_preburied += 1
        return 1, 0

    # Float artifact stat: curl-region vertices now standing off the surface.
    _curl_signed = _signed[_curl_mask] if _curl_mask.any() else np.empty(0)
    _float_d = (
        float((-_curl_signed[_curl_signed < 0.0]).max())
        if np.any(_curl_signed < 0.0) else 0.0
    )

    # ── Cross-cluster blade cull (final, geometry-accurate) ────────────────────
    # The pre-build midrib test only sampled the centerline before pull-away, so
    # leaves whose blade width/curl enters a neighbouring cluster — or that
    # pull-away pushed in — survive it.  Test the actual pulled-away blade surface
    # against every neighbour and drop the leaf if any vertex sits inside one
    # beyond the tolerance.  Only seam leaves have neighbours (bbox-sphere pruned),
    # so the extra signed_distance is paid only where intersections can occur.
    if ctx.other_meshes:
        _t_xblade = time.perf_counter()
        if _inside_neighbour(
            ctx.other_meshes, surf.vertices, pt3d, L, _CROSS_CLUSTER_BLADE_INSIDE_FRAC,
        ):
            _pt("slot.cross_bury_cull", time.perf_counter() - _t_xblade)
            stats.skipped_cross_buried += 1
            return 0, 0
        _pt("slot.cross_bury_cull", time.perf_counter() - _t_xblade)

    # ── Tip-z ordering: visible blade tip must sit above the embedded root tip ──
    # Computed after pull-away (which already raised the blade); apply only the
    # residual +Z nudge still needed.
    _tip_z_lift = 0.0
    _tip_z_clearance = float(
        solid.vertices[_tip_idx, 2] - solid.vertices[len(surf.vertices) + _tip_idx, 2]
    )
    if _tip_z_clearance < _TIP_Z_CLEARANCE_MM:
        _tip_z_lift = _TIP_Z_CLEARANCE_MM - _tip_z_clearance
        _lift_vec = np.array([0.0, 0.0, _tip_z_lift])
        surf.vertices                       += _lift_vec
        solid.vertices[:len(surf.vertices)] += _lift_vec
        _tip_z_clearance = float(
            solid.vertices[_tip_idx, 2] - solid.vertices[len(surf.vertices) + _tip_idx, 2]
        )
    if _tip_z_clearance <= 0.0:
        raise AssertionError(
            "leaf surface tip z must be above root oval tip z "
            f"(surface={solid.vertices[_tip_idx, 2]:.6f}, "
            f"root={solid.vertices[len(surf.vertices) + _tip_idx, 2]:.6f}, "
            f"clearance={_tip_z_clearance:.6f}, "
            f"row={row_idx}, col={ci}, layer={_layer})"
        )

    # Commit this leaf's footprint to the shingle grid so later leaves stack
    # above it.  Only written on success — a rejected build leaves the grid
    # untouched, so its layer stays free for the next candidate.
    _shingle_write(ctx.occ, _cells, _layer)

    stats.leaf_float_dists.append(_float_d)
    stats.leaf_buried_depths.append(_burial_d)
    stats.base_positions.append(pt3d.copy())
    stats.base_tangents.append(tangent.copy())
    stats.base_row_idx.append(row_idx)
    stats.root_depths.append(root_depth)
    stats.shingle_layers.append(_layer)
    stats.tip_z_clearances.append(_tip_z_clearance)
    stats.tip_z_lifts.append(_tip_z_lift)
    stats.pull_aways.append(_pull_away)
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
    belly_dip          = _leaf_belly_dip(**leaf_kw)   # (dL, dN); analytic-contact binding point
    ca_cache: dict[int, float] = {}   # shared: leaf_kw is identical for all meshes

    # ── Phase 1: pre-computation for every mesh ───────────────────────────────

    contexts: list[_MeshCtx] = []
    all_rows: list[tuple[float, int, int]] = []   # (z, mesh_idx, per_mesh_row_idx)

    _PROF.clear(); _PROF_N.clear()
    _t_total = time.perf_counter()

    for mi, (mesh, seed, label) in enumerate(zip(meshes, seeds_list, labels_list)):
        z_top       = float(mesh.vertices[:, 2].max())
        cx          = float(mesh.vertices[:, 0].mean())
        cy          = float(mesh.vertices[:, 1].mean())
        centroid_3d = np.array([cx, cy, float(mesh.vertices[:, 2].mean())])
        _tm = time.perf_counter()
        meridians   = _build_meridians(mesh, n_meridians=n_meridians, z_samples=z_samples)
        _pt("phase1.meridians", time.perf_counter() - _tm)
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
        )
        contexts.append(ctx)
        for ri, z in enumerate(row_zs):
            all_rows.append((z, mi, ri))

    # Cross-cluster cull (Option A): give each context the proximity queries of
    # every OTHER cluster so buried-base slots can be skipped before any geometry
    # is built.  Single-cluster runs get an empty list and are unaffected.
    #
    # World-space shingling (Option B): all contexts share one occupancy grid so
    # surviving seam leaves from different clusters can see one another and take
    # distinct standoff layers.
    occ_shared: dict = {}
    _t_prune = time.perf_counter()
    bounds_centers = []
    bounds_radii = []
    for ctx in contexts:
        b = np.asarray(ctx.mesh.bounds, dtype=float)
        c = 0.5 * (b[0] + b[1])
        bounds_centers.append(c)
        bounds_radii.append(float(np.linalg.norm(b[1] - b[0]) * 0.5))

    n_other_links = 0
    for mi, ctx in enumerate(contexts):
        nearby = []
        for oi, other in enumerate(contexts):
            if oi == mi:
                continue
            center_gap = float(np.linalg.norm(bounds_centers[mi] - bounds_centers[oi]))
            if center_gap <= bounds_radii[mi] + bounds_radii[oi] + L:
                nearby.append(other.mesh)
        ctx.other_meshes = nearby
        n_other_links += len(nearby)
        ctx.occ = occ_shared
    _pt("phase1.cross_prune", time.perf_counter() - _t_prune, n_other_links)

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
            _ts = time.perf_counter()
            slots, perim = _collect_row_slots(
                contexts[mi], row_idx, z_row, L=L, W=W, col_step=col_step,
            )
            _pt("phase2.collect_slots", time.perf_counter() - _ts)
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
                _tslot = time.perf_counter()
                att, pl = _place_leaf_slot(
                    slot,
                    contact_candidates=contact_candidates, belly_dip=belly_dip,
                    ca_cache=ca_cache,
                    leaf_kw=leaf_kw,
                    angle_jitter_deg=angle_jitter_deg,
                    pos_jitter=pos_jitter, row_color_fn=row_color_fn,
                    color_row_idx=round(_step * 15 / max(_n_total - 1, 1)),
                )
                _pt("phase2.place_slot", time.perf_counter() - _tslot, att)
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

    _t_total_elapsed = time.perf_counter() - _t_total
    _PROF["TOTAL"] = _t_total_elapsed
    _PROF_N["TOTAL"] = 1
    print("\n── leaf placement timing ──────────────────────────────────────────")
    _ordered = sorted(
        [(k, v) for k, v in _PROF.items() if k != "TOTAL"],
        key=lambda x: -x[1],
    )
    for k, v in _ordered:
        n = _PROF_N.get(k, 1)
        avg_ms = 1000.0 * v / n if n else 0.0
        pct = 100.0 * v / _t_total_elapsed if _t_total_elapsed else 0.0
        print(f"  {k:<40} {v:8.3f}s  {pct:5.1f}%  n={n:5d}  avg={avg_ms:7.2f}ms")
    print(f"  {'TOTAL':<40} {_t_total_elapsed:8.3f}s")
    print()

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
