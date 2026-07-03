"""Single-leaf mesh generator — reusable across any context.

A leaf is an ovate, keeled blade:

* **Outline** (top view): ovate teardrop — peak width at ≈ 1/3 from base,
  tapering to a rounded base and a pointed tip.
* **Cross-section**: a quartic Bézier dome that rises from the midrib and falls
  back to zero at the edge.  Independent ``inner_curve`` and ``outer_curve``
  controls shape the crease-side and edge-side shoulders of one smooth curve.
  The longitudinal thickness scale ``long_t`` peaks at ≈ 25 % from the base
  (``s ≈ 0.25``) and falls steeply toward the tip (``(1−s)^1.5`` decay).
* **Crease**: a narrow tanh fold concentrates the V-indent at the midrib
  (width controlled by ``_LEAF_CREASE_SHARPNESS``).
* **Keel**: a V cross-section prism extruded below the leaf plane, forming a
  structural ridge along the midrib that tapers to the tip.

Public API
----------
``compute_leaf_geometry(...)`` — returns a :class:`_LeafGeometry` NamedTuple with
all vertex arrays and axes; used by :func:`build_leaf_surface`,
:func:`build_leaf_mesh`, and the analytical contact-angle helper in ``mesh.py``.

``build_leaf_surface(...)`` — open top-face mesh (no walls, no keel).  Returns
``(surface, geom)``; the :class:`_LeafGeometry` is needed by :func:`solidify_leaf`.

``build_leaf_mesh(...)`` — returns a list of Trimesh parts (blade body + optional
keel) positioned at *base_pos* and oriented along *tangent*.  For standalone leaf
geometry when a full solid is not needed.

``solidify_leaf(surface, inner_v)`` — closes an open surface into a watertight solid
using a pre-computed oval ``inner_v`` (built by :func:`build_leaf_oval_offsets` during
slot collection).  Wall faces connect the leaf perimeter to the oval perimeter
vertex-for-vertex.

Leaf Attachment Model
---------------------
A leaf can only attach to a surface in the following ways. Any orientation
outside these rules produces a geometrically impossible leaf.

**BASE point** — The centre of the leaf's top surface at the attachment end
(opposite the tip).  ``base_pos`` in the API.  The BASE is the point that sits
on the surface.  All rotations are performed around this point.

**Degrees of freedom** — only three are valid:

1. **Position**: where on the surface the BASE sits.

2. **Twist**: rotation about the base→tip axis.  Controls which direction around
   the surface normal the tip points (the compass bearing in the surface tangent
   plane).  Tips point as close to gravity-down as the surface allows.

3. **Contact angle**: rotation about the lateral axis T through the BASE
   (perpendicular to both tangent and surface normal), in the plane of
   (tangent, surface_normal).  The contact angle is the rotation that presses
   the leaf tip just against the parent surface — the zero point from which
   ``lift_mm`` is measured.

   - contact angle → tip just touches the parent surface (lift = 0).
   - lift > 0      → tip raised further above the surface.

**Frame after contact-angle rotation**::

    T0             = unit vector in surface tangent plane (twist direction)
    ca             = contact angle in [0°, 90°]
    tangent        = T0 * cos(ca) − surface_normal * sin(ca)
    N (up_hint)    = surface_normal * cos(ca) + T0 * sin(ca)

The seat pitch is solved against the real clump surface by
``placement_leaf._seat_oval_tilt``.  See ``docs/design/leaf-placement.md``
for the full algorithm specification.
"""
from __future__ import annotations

import math
from typing import NamedTuple

import numpy as np
import trimesh

from ._utils import _safe_norm

# ── Constants ──────────────────────────────────────────────────────────────────

_LEAF_N_LONG           = 12    # longitudinal sections (base → tip)
_LEAF_N_LAT            = 10    # lateral sections across the leaf (must be even)
_LEAF_CREASE_SHARPNESS = 10.0  # tanh width of midrib crease (larger = narrower)
# Width profile normalisation: w(s) ∝ s^0.4 × (1-s)^0.8 peaks at s=1/3.
_LEAF_W_PEAK_NORM  = float((1.0 / 3.0) ** 0.4 * (2.0 / 3.0) ** 0.8)
# Thickness-along-length profile: t_long(s) ∝ s^0.5 × (1-s)^1.5, peak at s=0.25.
_LEAF_LONG_T_PEAK  = float(0.25 ** 0.5 * 0.75 ** 1.5)   # ≈ 0.3248

# Default leaf size (mm).  Public so callers can reference them without
# hard-coding magic numbers.
LEAF_LENGTH_MM_DEFAULT = 9.0
LEAF_WIDTH_MM_DEFAULT  = 6.0   # ≈ 2/3 of length


# ── Tiny shared helpers (self-contained so this module has no tree imports) ────

# Corrected-winding face cache.  Every leaf blade (and every keel) of a given
# topology shares an identical face-connectivity array, and trimesh's
# ``fix_normals()`` makes winding / outward-flip decisions that are invariant
# under the rigid placement of each leaf.  So the corrected face array is
# identical for all leaves of that topology — we run ``fix_normals()`` once per
# topology and reuse the result, turning ~12k per-leaf calls (the dominant cost
# of leaf generation) into a handful.  Output geometry is byte-identical.
_FACE_CACHE: dict[object, np.ndarray] = {}

# Canonical face-connectivity cache (before winding correction).
# The face index array for a blade or keel depends only on (N_S, N_T) or
# the keel station count, not on per-leaf positions.  Building it in Python
# is cheap to do once; the _FACE_CACHE above stores the post-fix_normals
# version.  Pre-building avoids re-constructing the face array 11k times
# when _FACE_CACHE already has the corrected version.
_BLADE_FACES_CACHE: dict[tuple[int, int], np.ndarray] = {}


def _mesh_with_fixed_normals(
    verts: np.ndarray, faces: np.ndarray, cache_key: object
) -> trimesh.Trimesh:
    """Trimesh with outward-consistent winding, reusing fix_normals() per topology."""
    cached = _FACE_CACHE.get(cache_key)
    if cached is None:
        mesh = trimesh.Trimesh(vertices=verts, faces=faces, process=False)
        mesh.fix_normals()
        _FACE_CACHE[cache_key] = mesh.faces.copy()
        return mesh
    return trimesh.Trimesh(vertices=verts, faces=cached.copy(), process=False)


# ── Canonical face array builder ───────────────────────────────────────────────

def _build_blade_faces(long_count: int, lat_count: int) -> np.ndarray:
    """Build canonical face connectivity for a leaf blade.

    Vertex layout (matches the vectorised ``build_leaf_mesh`` output):
      indices 0 … ring_count*(lat_count+1)-1          — upper-grid vertices, ring-major
      indices ring_count*(lat_count+1) … 2*…-1        — lower-grid vertices, ring-major
      index   2*ring_count*(lat_count+1)              — base_pt
      index   2*ring_count*(lat_count+1)+1            — tip_pt
    """
    key = (long_count, lat_count)
    cached = _BLADE_FACES_CACHE.get(key)
    if cached is not None:
        return cached

    ring_count  = long_count - 1
    ring_stride = lat_count + 1
    top_start = lambda i: i * ring_stride
    bot_start = lambda i: (ring_count + i) * ring_stride
    base_idx  = 2 * ring_count * ring_stride
    tip_idx   = base_idx + 1

    faces: list[list[int]] = []

    # Base fan
    first_top_start = top_start(0)
    first_bot_start = bot_start(0)
    for j in range(lat_count):
        j1 = j + 1
        faces.append([base_idx, first_top_start + j,  first_top_start + j1])
        faces.append([base_idx, first_bot_start + j1, first_bot_start + j ])
    faces.append([base_idx, first_bot_start,              first_top_start              ])
    faces.append([base_idx, first_top_start + lat_count,  first_bot_start + lat_count ])

    # Body bands
    for ri in range(ring_count - 1):
        top_cur, top_nxt = top_start(ri), top_start(ri + 1)
        bot_cur, bot_nxt = bot_start(ri), bot_start(ri + 1)
        for j in range(lat_count):
            j1 = j + 1
            faces.append([top_cur + j,  top_nxt + j,  top_nxt + j1])
            faces.append([top_cur + j,  top_nxt + j1, top_cur + j1])
            faces.append([bot_cur + j,  bot_cur + j1, bot_nxt + j1])
            faces.append([bot_cur + j,  bot_nxt + j1, bot_nxt + j ])
        faces.append([top_cur,              bot_cur,              bot_nxt             ])
        faces.append([top_cur,              bot_nxt,              top_nxt             ])
        faces.append([top_cur + lat_count,  top_nxt + lat_count,  bot_nxt + lat_count])
        faces.append([top_cur + lat_count,  bot_nxt + lat_count,  bot_cur + lat_count])

    # Tip fan
    last_top_start = top_start(ring_count - 1)
    last_bot_start = bot_start(ring_count - 1)
    for j in range(lat_count):
        j1 = j + 1
        faces.append([tip_idx, last_top_start + j1, last_top_start + j ])
        faces.append([tip_idx, last_bot_start + j,  last_bot_start + j1])
    faces.append([tip_idx, last_top_start,              last_bot_start             ])
    faces.append([tip_idx, last_bot_start + lat_count,  last_top_start + lat_count])

    arr = np.array(faces, dtype=np.int32)
    _BLADE_FACES_CACHE[key] = arr
    return arr


# ── Profile helpers ────────────────────────────────────────────────────────────

def _leaf_width_profile(s: np.ndarray) -> np.ndarray:
    """Width fraction ∈ [0, 1] at normalised longitudinal position s ∈ [0, 1].

    Broader toward the base (peak at s ≈ 1/3), tapering smoothly to 0 at
    both ends.  This gives an ovate leaf shape — rounded at the base and
    pointed at the tip.
    """
    s = np.asarray(s, float)
    raw = (s ** 0.4) * ((1.0 - s) ** 0.8)
    return raw / _LEAF_W_PEAK_NORM


def _leaf_lobe_profile(
    r: np.ndarray,
    inner_curve: float,
    outer_curve: float,
) -> np.ndarray:
    """Smooth crease-to-edge profile controlled by two Bézier shoulders.

    ``r`` runs from 0 at the crease to 1 at the edge.  The quartic Bézier
    control heights are ``[0, inner_curve, 1, outer_curve, 0]``.  This makes
    the inner and outer attributes shape one continuous curve rather than two
    curve segments joined at an arbitrary boundary.  The result is normalized
    so ``thickness_mm`` remains the actual maximum lobe height.
    """
    r = np.asarray(r, float)
    inner = max(0.0, float(inner_curve))
    outer = max(0.0, float(outer_curve))

    def _evaluate(x: np.ndarray) -> np.ndarray:
        q = 1.0 - x
        return (
            4.0 * inner * q**3 * x
            + 6.0 * q**2 * x**2
            + 4.0 * outer * q * x**3
        )

    profile = _evaluate(r)
    normalizer = float(_evaluate(np.linspace(0.0, 1.0, 257)).max())
    if normalizer <= 1e-12:
        return np.zeros_like(r)
    return profile / normalizer


# ── Shared geometry computation ───────────────────────────────────────────────

class _LeafGeometry(NamedTuple):
    """Raw arrays produced once and shared by build_leaf_surface / build_leaf_mesh."""
    along_axis:  np.ndarray   # (3,) unit growth direction (base → tip)
    across_axis: np.ndarray   # (3,) unit lateral direction
    normal_axis: np.ndarray   # (3,) unit leaf normal (top/crease faces outward)
    base_pt:     np.ndarray   # (3,) base position (attachment point, s=0)
    tip_pt:      np.ndarray   # (3,) tip position (pointed end, s=1)
    s_int:       np.ndarray   # (n_rings,) longitudinal stations ∈ (0, 1)
    w_s:         np.ndarray   # (n_rings,) half-widths at each station
    upper_grid:  np.ndarray   # (n_rings, N_T+1, 3) top-surface vertex grid
    lower_grid:  np.ndarray   # (n_rings, N_T+1, 3) bottom-surface vertex grid


def compute_leaf_geometry(
    *,
    base_pos:       np.ndarray,
    tangent:        np.ndarray,
    length_mm:      float = LEAF_LENGTH_MM_DEFAULT,
    width_mm:       float = LEAF_WIDTH_MM_DEFAULT,
    thickness_mm:   float = 0.24,
    fold_angle_deg: float = 6.0,
    inner_curve:    float = 1.5,
    outer_curve:    float = 0.72,
    arch_deg:       float = 30.0,
    curl_deg:       float = 15.0,
    lift_mm:        float = 1.5,
    up_hint:        np.ndarray | None = None,
    seed:           int = 0,
) -> _LeafGeometry:
    """Compute all leaf geometry arrays (frame, vertex grids) shared across builders.

    The midrib is a smooth compound curve in the L-N plane:

    * *arch_deg* creates the dominant base-to-tip hump across the whole leaf.
    * *curl_deg* applies a correction over the tip third (last ⅓) of the leaf
      and sets the final tangent angle above the leaf plane.

    Both values are positive curve magnitudes and remain independently visible.
    The default 30° arch ensures leaves are arched even when
    curl is explicitly disabled.

    *lift_mm* is applied **after** arch and curl as a rigid rotation of the
    entire surface around the lateral axis (T) through the base point.  The
    whole leaf tilts so the tip rises by approximately *lift_mm* in the N
    direction while the base stays fixed.  This is NOT a tip-only z-offset.

    *inner_curve* and *outer_curve* are dimensionless Bézier control heights
    for each mirrored crease-to-edge cross-section.  They shape one continuous
    convex curve on each side: inner controls the crease-side shoulder and
    outer controls the edge-side shoulder.
    """
    along_axis = _safe_norm(np.asarray(tangent, float))
    base_pt    = np.asarray(base_pos, float)

    world_up = (_safe_norm(np.asarray(up_hint, float)) if up_hint is not None
                else np.array([0.0, 0.0, 1.0]))
    if abs(float(np.dot(along_axis, world_up))) > 0.9:
        world_up = (np.array([1.0, 0.0, 0.0])
                    if abs(float(np.dot(along_axis, np.array([0.0, 0.0, 1.0])))) > 0.9
                    else np.array([0.0, 0.0, 1.0]))
    across_axis = np.cross(world_up, along_axis)
    across_axis /= max(float(np.linalg.norm(across_axis)), 1e-10)
    normal_axis = np.cross(along_axis, across_axis)
    normal_axis /= max(float(np.linalg.norm(normal_axis)), 1e-10)

    fold_slope = float(np.tan(np.radians(fold_angle_deg)))
    lat_count  = _LEAF_N_LAT
    lat_pos    = np.linspace(-1.0, 1.0, lat_count + 1)   # lateral positions −1→+1
    abs_lat    = np.abs(lat_pos)

    s_int            = np.linspace(0.0, 1.0, _LEAF_N_LONG + 1)[1:-1]   # interior stations
    w_s              = 0.5 * width_mm * _leaf_width_profile(s_int)     # half-widths
    thickness_taper  = (s_int ** 0.5 * (1.0 - s_int) ** 1.5) / _LEAF_LONG_T_PEAK

    # ── Compound longitudinal arch + curl ────────────────────────────────────
    # Work as a height profile over the original leaf plane.  The arch is the
    # dominant full-length curve.  The curl integrates a quintic smootherstep
    # slope correction over [CURL_START, 1].  The correction has zero value,
    # slope, and curvature at CURL_START, so it layers onto the arch with a C2
    # join instead of replacing the final part of it.
    curl_start_s     = 2.0 / 3.0          # curl covers the last ⅓ of the leaf
    curl_zone_length = (1.0 - curl_start_s) * float(length_mm)

    # f(s)=s(1-s) spans the entire leaf, leaves the base immediately at
    # arch_deg, reaches its hump at mid-leaf, and returns to the tip plane.
    arch_base_slope = np.tan(np.radians(abs(float(arch_deg))))
    # The curl slope uses smootherstep(u), which starts with zero slope and
    # curvature and settles close to its final value before the tip.  This
    # makes the final mesh segment visibly point upward at approximately the
    # requested angle instead of reaching that angle only at the mathematical
    # endpoint.
    curl_tip_slope = np.tan(np.radians(abs(float(curl_deg))))

    def _centerline_profile(s: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
        """Return arch+curl height and slope for normalized longitudinal stations.

        Lift is NOT computed here — it is applied after all vertex positions
        are built as a rigid rotation around across_axis through base_pt
        (see the lift block below).
        """
        s = np.asarray(s, float)
        curl_u = np.clip((s - curl_start_s) / (1.0 - curl_start_s), 0.0, 1.0)

        arch_z = float(length_mm) * arch_base_slope * s * (1.0 - s)
        arch_dzdx = arch_base_slope * (1.0 - 2.0 * s)

        curl_active = s > curl_start_s
        curl_shape = 2.5 * curl_u**4 - 3.0 * curl_u**5 + curl_u**6
        curl_slope_shape = 10.0 * curl_u**3 - 15.0 * curl_u**4 + 6.0 * curl_u**5
        # The full-length arch points downward at the tip.  Curl corrects that
        # existing slope to the requested upward angle rather than merely
        # adding curl_deg to it.
        curl_slope_correction = (
            curl_tip_slope + arch_base_slope
            if abs(float(curl_deg)) > 1e-8
            else 0.0
        )
        curl_z = np.where(
            curl_active,
            curl_zone_length * curl_slope_correction * curl_shape,
            0.0,
        )
        curl_dzdx = np.where(
            curl_active,
            curl_slope_correction * curl_slope_shape,
            0.0,
        )

        return arch_z + curl_z, arch_dzdx + curl_dzdx

    arch_height, arch_slope = _centerline_profile(s_int)
    midrib_pts = (
        base_pt[np.newaxis]
        + (s_int[:, np.newaxis] * float(length_mm)) * along_axis[np.newaxis]
        + arch_height[:, np.newaxis] * normal_axis[np.newaxis]
    )
    tip_arch_height, _ = _centerline_profile(np.array(1.0))
    tip_pt = base_pt + float(length_mm) * along_axis + float(tip_arch_height.item()) * normal_axis

    tilt_angle     = np.arctan(arch_slope)
    station_normal = (-np.sin(tilt_angle)[:, np.newaxis] * along_axis[np.newaxis]
                      + np.cos(tilt_angle)[:, np.newaxis] * normal_axis[np.newaxis])

    spread_pts = (midrib_pts[:, np.newaxis]
                  + (lat_pos[np.newaxis, :, np.newaxis] * w_s[:, np.newaxis, np.newaxis])
                  * across_axis[np.newaxis, np.newaxis])

    crease_shape = np.tanh(abs_lat * _LEAF_CREASE_SHARPNESS)
    dome_shape   = _leaf_lobe_profile(abs_lat, inner_curve, outer_curve)
    # Smootherstep fades the crease in from base to tip; thickness_taper
    # then brings it back to zero at the tip.
    crease_ramp   = 6.0 * s_int**5 - 15.0 * s_int**4 + 10.0 * s_int**3
    crease_height = crease_shape[np.newaxis] * (
        w_s[:, np.newaxis]
        * fold_slope
        * thickness_taper[:, np.newaxis]
        * crease_ramp[:, np.newaxis]
    )
    dome_height = (thickness_mm * dome_shape[np.newaxis]) * thickness_taper[:, np.newaxis]

    # Apply crease + dome offsets along the per-station surface normal.
    upper_grid = spread_pts + (crease_height + dome_height)[:, :, np.newaxis] * station_normal[:, np.newaxis, :]
    lower_grid = spread_pts + crease_height[:, :, np.newaxis]                 * station_normal[:, np.newaxis, :]

    # ── Lift: rigid rotation around across_axis through base_pt ──────────────
    # Applied AFTER arch and curl produce the complete vertex grid; BEFORE
    # walls or root are constructed.  Rotates along_axis toward normal_axis
    # so the tip rises by approximately lift_mm.
    # Frame vectors along_axis and normal_axis are updated afterward;
    # across_axis is the rotation axis and is unchanged.
    if abs(float(lift_mm)) > 1e-8:
        lift_angle = float(np.arctan2(float(lift_mm), float(length_mm)))
        lift_cos   = float(np.cos(lift_angle))
        lift_sin   = float(np.sin(lift_angle))

        def _lift_rot(pts: np.ndarray) -> np.ndarray:
            """Rodrigues rotation around −across_axis through base_pt."""
            shape   = pts.shape
            rel     = pts.reshape(-1, 3) - base_pt[np.newaxis]
            ax_dot  = (rel @ across_axis)[:, np.newaxis]
            ax_cross = np.cross(across_axis[np.newaxis], rel)
            rot     = lift_cos * rel - lift_sin * ax_cross + (1.0 - lift_cos) * ax_dot * across_axis[np.newaxis]
            return (base_pt[np.newaxis] + rot).reshape(shape)

        upper_grid = _lift_rot(upper_grid)
        lower_grid = _lift_rot(lower_grid)
        tip_pt     = _lift_rot(tip_pt[np.newaxis])[0]

        # Rotate the frame vectors to stay consistent with the new geometry.
        along_prev, normal_prev = along_axis.copy(), normal_axis.copy()
        along_axis  = lift_cos * along_prev + lift_sin * normal_prev
        normal_axis = -lift_sin * along_prev + lift_cos * normal_prev
        # across_axis is the rotation axis — unchanged.

    return _LeafGeometry(
        along_axis=along_axis, across_axis=across_axis, normal_axis=normal_axis,
        base_pt=base_pt, tip_pt=tip_pt,
        s_int=s_int, w_s=w_s, upper_grid=upper_grid, lower_grid=lower_grid,
    )


# ── Keel prism ─────────────────────────────────────────────────────────────────

def _build_leaf_keel_prism(
    base_pt: np.ndarray,
    tip_pt: np.ndarray,
    left_pts: np.ndarray,
    right_pts: np.ndarray,
    normal_axis: np.ndarray,
    across_axis: np.ndarray,
    s_values: np.ndarray,
    half_widths: np.ndarray,
    keel_depth_mm: float,
    keel_tip_angle_deg: float,
    all_bot_pts: np.ndarray | None = None,
) -> trimesh.Trimesh:
    """Leaf keel: a V cross-section that meets at a ridge on the midrib.

    Parameters
    ----------
    base_pt           : (3,)   leaf base point (s=0)
    tip_pt            : (3,)   leaf tip point  (s=1)
    left_pts          : (n,3)  bottom-surface vertices at t=−1 (left edge), s increasing
    right_pts         : (n,3)  bottom-surface vertices at t=+1 (right edge), s increasing
    normal_axis       : (3,)   leaf normal (toward top surface)
    across_axis       : (3,)   lateral direction (one leaf edge at +across_axis)
    s_values          : (n,)   s parameter for each interior ring
    half_widths       : (n,)   leaf half-width w_s at each interior ring (unused)
    keel_depth_mm     : maximum keel depth below the leaf plane
    keel_tip_angle_deg: unused — the tip descent is a quarter circle, not a bevel
    all_bot_pts       : (n_rings, N_T+1, 3) full blade-bottom vertex array used for
                        the top-closure surface.  When supplied the top is triangulated
                        to match the blade's curved underside (no flat); when None the
                        closure falls back to a centroid fan (legacy behaviour).

    Construction
    ------------
    The **keel bottom edge** (the ridge along the midrib) is an explicit
    longitudinal depth profile, independent of leaf width.  A quarter-circle
    fillet of radius ``keel_depth_mm`` runs from the tip back to the flat
    full-depth section:

        x = distance back from the tip
        depth(x) = sqrt(R² − (R − x)²)   for x < R
        depth(x) = R                     for x ≥ R

    The **side walls** are built straight from each leaf edge down to the ridge
    at that station.  The cross-section is a V meeting at the midrib ridge; the
    tip pinches to a point.
    """
    leaf_len      = float(np.linalg.norm(tip_pt - base_pt))
    max_depth     = float(keel_depth_mm)
    fillet_radius = max_depth
    station_count = len(right_pts) + 2   # interior rings + base + tip

    # ── Vectorised station geometry ─────────────────────────────────────────
    station_s = np.concatenate([[0.0], np.asarray(s_values, float), [1.0]])  # (station_count,)

    # Keel depth profile: flat at max_depth, quarter-circle fillet near tip.
    tip_dist      = (1.0 - station_s) * leaf_len
    station_depth = np.where(
        tip_dist >= fillet_radius,
        max_depth,
        np.sqrt(np.maximum(0.0, fillet_radius**2 - (fillet_radius - tip_dist)**2))
    )  # (station_count,)

    # Edge positions per station — interior uses left_pts/right_pts;
    # endpoints pinch to base_pt / tip_pt.
    right_station_pts = np.vstack([base_pt[np.newaxis], right_pts, tip_pt[np.newaxis]])
    left_station_pts  = np.vstack([base_pt[np.newaxis], left_pts,  tip_pt[np.newaxis]])

    # Keel ridge: midrib spine offset downward by station_depth.
    midrib_pts = (base_pt[np.newaxis] +
                  station_s[:, np.newaxis] * (tip_pt - base_pt)[np.newaxis])
    keel_pts   = midrib_pts - station_depth[:, np.newaxis] * normal_axis[np.newaxis]

    # Stations where depth ≈ 0 collapse to the top edge (tip region).
    pinched_mask = station_depth < 1e-9   # (station_count,) bool

    # ── Build vertex list and per-station index lists ───────────────────────
    verts: list[np.ndarray] = list(right_station_pts)   # right_idx[k] = k
    vert_count = station_count
    right_idx  = list(range(station_count))

    left_idx: list[int] = []
    for k in range(station_count):
        if k == 0 or k == station_count - 1:
            left_idx.append(right_idx[k])          # pinch to right at endpoints
        else:
            verts.append(left_station_pts[k])
            left_idx.append(vert_count)
            vert_count += 1

    ridge_idx: list[int] = []
    for k in range(station_count):
        if pinched_mask[k]:
            ridge_idx.append(right_idx[k])
        else:
            verts.append(keel_pts[k])
            ridge_idx.append(vert_count)
            vert_count += 1

    F: list[list[int]] = []

    def _quad(a: int, b: int, c: int, d: int) -> None:
        if a != b and b != c and a != c:
            F.append([a, b, c])
        if a != c and c != d and a != d:
            F.append([a, c, d])

    for k in range(station_count - 1):
        _quad(right_idx[k], right_idx[k + 1], ridge_idx[k + 1], ridge_idx[k])
        _quad(ridge_idx[k], ridge_idx[k + 1], left_idx[k + 1],  left_idx[k])

    # ── Top closure ─────────────────────────────────────────────────────────
    # When all_bot_pts is supplied (the full lower_grid, shape
    # (ring_count, lat_count+1, 3)) we triangulate the top as a curved surface
    # matching the blade underside — this removes the visible flat.
    # When it is None we fall back to the legacy centroid fan.
    if all_bot_pts is not None:
        lat_count = all_bot_pts.shape[1] - 1

        # closure_idx_grid[row, col] → vertex index in verts.
        # Rows:  0 = base station, 1..station_count-2 = interior, last = tip.
        # Cols:  0 = left edge (left_idx), lat_count = right edge (right_idx).
        closure_idx_grid = np.empty((station_count, lat_count + 1), dtype=np.int64)

        closure_idx_grid[:, 0]         = left_idx
        closure_idx_grid[:, lat_count] = right_idx
        closure_idx_grid[0,  :]        = right_idx[0]               # base row
        closure_idx_grid[-1, :]        = right_idx[station_count - 1]  # tip row

        for k in range(1, station_count - 1):
            r = k - 1                        # all_bot_pts ring index
            for t in range(1, lat_count):
                verts.append(all_bot_pts[r, t])
                closure_idx_grid[k, t] = vert_count
                vert_count += 1

        for row in range(station_count - 1):
            for col in range(lat_count):
                a = int(closure_idx_grid[row,     col    ])
                b = int(closure_idx_grid[row,     col + 1])
                c = int(closure_idx_grid[row + 1, col + 1])
                d = int(closure_idx_grid[row + 1, col    ])
                _quad(a, b, c, d)
    else:
        # Legacy centroid fan (produces a flat face — kept as fallback).
        loop  = (right_idx + [left_idx[k] for k in range(station_count - 2, 0, -1)])
        c_top = len(verts)
        verts.append(np.mean([verts[i] for i in loop], axis=0))
        for a in range(len(loop)):
            b = (a + 1) % len(loop)
            F.append([c_top, loop[a], loop[b]])

    # Topology (vertex sharing + degenerate-triangle culling) is fully
    # determined by the leaf length, keel depth, and top-closure mode.
    cache_tag = "keel_grid" if all_bot_pts is not None else "keel"
    return _mesh_with_fixed_normals(
        np.array(verts, dtype=float),
        np.array(F,     dtype=np.int32),
        (cache_tag, round(leaf_len, 6), round(max_depth, 6)),
    )


# ── Public API ─────────────────────────────────────────────────────────────────

def build_leaf_surface(
    *,
    base_pos:       np.ndarray,
    tangent:        np.ndarray,
    length_mm:      float = LEAF_LENGTH_MM_DEFAULT,
    width_mm:       float = LEAF_WIDTH_MM_DEFAULT,
    thickness_mm:   float = 0.24,
    fold_angle_deg: float = 6.0,
    inner_curve:    float = 1.5,
    outer_curve:    float = 0.72,
    arch_deg:       float = 30.0,
    curl_deg:       float = 15.0,
    lift_mm:        float = 1.5,
    up_hint:        np.ndarray | None = None,
    seed:           int = 0,
) -> tuple[trimesh.Trimesh, _LeafGeometry]:
    """Open leaf surface — the top face only, with all visible geometry.

    Returns ``(surface, geom)`` where *surface* is an open ``trimesh.Trimesh``
    (disk topology) and *geom* is the :class:`_LeafGeometry` used to build it.
    The geometry is needed by :func:`solidify_leaf` to build the back dome.

    The surface has one boundary loop running along the two lateral edges
    from base to tip.  Face normals point outward (+N at the base).

    *inner_curve*, *outer_curve*, *arch_deg*, and *curl_deg* — see
    :func:`compute_leaf_geometry`.

    Parameters otherwise mirror :func:`build_leaf_mesh` minus keel params.
    """
    g = compute_leaf_geometry(
        base_pos=base_pos, tangent=tangent,
        length_mm=length_mm, width_mm=width_mm,
        thickness_mm=thickness_mm, fold_angle_deg=fold_angle_deg,
        inner_curve=inner_curve, outer_curve=outer_curve,
        arch_deg=arch_deg, curl_deg=curl_deg, lift_mm=lift_mm,
        up_hint=up_hint, seed=seed,
    )

    ring_count  = _LEAF_N_LONG - 1
    lat_count   = _LEAF_N_LAT
    ring_stride = lat_count + 1
    base_idx    = ring_count * ring_stride
    tip_idx     = ring_count * ring_stride + 1

    verts = np.concatenate([
        g.upper_grid.reshape(-1, 3),
        g.base_pt[np.newaxis],
        g.tip_pt[np.newaxis],
    ], axis=0)

    faces: list[list[int]] = []

    # Base fan — base_idx fans into ring 0 of the upper grid.
    # Winding [base_idx, j, j+1] gives outward normals. ✓
    for j in range(lat_count):
        faces.append([base_idx, j, j + 1])

    # Body — quad strip across consecutive upper-grid rings.
    # [a, d, c] and [a, c, b] give outward normals. ✓
    for ri in range(ring_count - 1):
        for j in range(lat_count):
            a = ri * ring_stride + j
            b = ri * ring_stride + j + 1
            c = (ri + 1) * ring_stride + j + 1
            d = (ri + 1) * ring_stride + j
            faces.append([a, d, c])
            faces.append([a, c, b])

    # Tip fan — tip_idx fans into the last ring of the upper grid.
    # Winding [tip_idx, last+j+1, last+j] gives outward normals. ✓
    last_ring_start = (ring_count - 1) * ring_stride
    for j in range(lat_count):
        faces.append([tip_idx, last_ring_start + j + 1, last_ring_start + j])

    # curl_deg is in the cache key because winding decisions differ between
    # curl=0 (flat tip) and curl>0 (tip raised).  lift_mm is excluded: it
    # rotates all vertices uniformly and does not change which side is outward,
    # so the same winding applies for all lift values at a given curl.
    mesh = _mesh_with_fixed_normals(
        verts,
        np.array(faces, dtype=np.int32),
        ("leaf_surface", _LEAF_N_LONG, lat_count, round(float(curl_deg), 4)),
    )
    return mesh, g


def build_leaf_mesh(
    *,
    base_pos: np.ndarray,
    tangent: np.ndarray,
    length_mm: float = LEAF_LENGTH_MM_DEFAULT,
    width_mm: float = LEAF_WIDTH_MM_DEFAULT,
    thickness_mm: float = 0.24,
    fold_angle_deg: float = 6.0,
    inner_curve: float = 1.5,
    outer_curve: float = 0.72,
    arch_deg: float = 30.0,
    curl_deg: float = 15.0,
    lift_mm: float = 1.5,
    keel_depth_mm: float = 1.5,
    keel_tip_angle_deg: float = 45.0,
    up_hint: np.ndarray | None = None,
    seed: int = 0,
) -> list[trimesh.Trimesh]:
    """Build a single leaf mesh positioned at *base_pos*, oriented along *tangent*.

    Parameters
    ----------
    base_pos        : (3,) world position of the leaf base.
    tangent         : (3,) growth direction (will be normalised).
    length_mm       : leaf length from base to tip.
    width_mm        : maximum leaf width (at ≈ 1/3 from base).
    thickness_mm    : dome height at peak (s ≈ 0.25).  Default 0.16.
    fold_angle_deg  : midrib crease V-angle.  Default 6.0.
    inner_curve     : crease-side Bézier shoulder height.  Default 1.5.
    outer_curve     : edge-side Bézier shoulder height.  Default 0.72.
    arch_deg        : upward tangent angle at the base of the arch.  Default 30.0.
    curl_deg        : concave tangent turn over the tip third.  Default 15.0.
    lift_mm         : tip lift in mm.  The entire arch+curl surface is rotated
                      rigidly around the lateral axis (T) through the base,
                      lifting the tip by approximately this amount in the N
                      direction while keeping the base fixed.  Applied after
                      arch and curl, before walls and root are built.  Default 1.0.
    keel_depth_mm   : maximum depth of the structural keel on the underside.
                      Pass 0 to omit the keel.  Default 1.0.
    keel_tip_angle_deg : reserved for future use.  Default 45.0.
    up_hint         : (3,) optional "away" reference for the leaf's top/crease
                      side.  Defaults to world-up; pass the outward surface
                      normal so leaves on a side/underside face their crease
                      away from the surface instead of into it.
    seed            : integer seed for the random roll angle.  Different seeds
                      produce differently-oriented leaves; the same seed always
                      produces the same orientation.  Default 0.

    Returns
    -------
    list[trimesh.Trimesh]
        One or two parts: [blade, keel] (keel absent if keel_depth_mm ≤ 0).
    """
    g = compute_leaf_geometry(
        base_pos=base_pos, tangent=tangent,
        length_mm=length_mm, width_mm=width_mm,
        thickness_mm=thickness_mm, fold_angle_deg=fold_angle_deg,
        inner_curve=inner_curve, outer_curve=outer_curve,
        arch_deg=arch_deg, curl_deg=curl_deg, lift_mm=lift_mm,
        up_hint=up_hint, seed=seed,
    )

    long_count = _LEAF_N_LONG
    lat_count  = _LEAF_N_LAT

    # Vertex array layout: upper rings | lower rings | base_pt | tip_pt
    # Matches the index arithmetic in _build_blade_faces().
    verts = np.concatenate([
        g.upper_grid.reshape(-1, 3),
        g.lower_grid.reshape(-1, 3),
        g.base_pt[np.newaxis],
        g.tip_pt[np.newaxis],
    ], axis=0)

    faces = _build_blade_faces(long_count, lat_count)
    mesh  = _mesh_with_fixed_normals(verts, faces, ("blade", long_count, lat_count))
    parts: list[trimesh.Trimesh] = [mesh]

    if keel_depth_mm > 1e-6:
        keel = _build_leaf_keel_prism(
            base_pt=g.base_pt,
            tip_pt=g.tip_pt,
            left_pts=g.lower_grid[:, 0,         :],
            right_pts=g.lower_grid[:, lat_count, :],
            normal_axis=g.normal_axis,
            across_axis=g.across_axis,
            s_values=g.s_int,
            half_widths=g.w_s,
            keel_depth_mm=keel_depth_mm,
            keel_tip_angle_deg=keel_tip_angle_deg,
            all_bot_pts=g.lower_grid,
        )
        if len(keel.vertices) > 0:
            parts.append(keel)

    return parts


# ── Oval builder (called during slot collection, before leaf geometry exists) ──

def build_leaf_oval_offsets(
    n_hat:    np.ndarray,
    T_along:  np.ndarray,
    across:   np.ndarray,
    L:        float,
    W:        float,
    embed_mm: float = 0.75,
) -> np.ndarray:
    """Build oval vertex offsets (relative to the leaf base point) for solidification.

    Returns an ``(n_outer, 3)`` array where
    ``n_outer = (_LEAF_N_LONG − 1) × (_LEAF_N_LAT + 1) + 2``.
    Add the actual ``base_pt`` / ``pt3d`` to get world-space positions.

    Parameters
    ----------
    n_hat    : (3,) outward mesh surface normal at the attachment point.
    T_along  : (3,) in-plane growth direction (normalised; world-down ⟂ n_hat).
    across   : (3,) lateral direction (normalised; cross(n_hat, T_along)).
    L        : leaf length in mm.
    W        : leaf maximum width in mm.
    embed_mm : depth to push the oval into the parent mesh surface.
    """
    ring_count = _LEAF_N_LONG - 1
    lat_count  = _LEAF_N_LAT

    s_int   = np.linspace(0.0, 1.0, _LEAF_N_LONG + 1)[1:-1]   # (ring_count,)
    lat_pos = np.linspace(-1.0, 1.0, lat_count + 1)            # (lat_count+1,)
    # Half-size, bottom-aligned: spans [L/2, L] along T_along, W/2 wide.
    oval_hw = (W / 4.0) * np.sin(np.pi * s_int)                # (ring_count,)

    embed_v = -float(embed_mm) * np.asarray(n_hat, float)      # (3,)
    T       = np.asarray(T_along, float)

    oval_grid = (
        embed_v[np.newaxis, np.newaxis, :]
        + ((0.5 + 0.5 * s_int)[:, np.newaxis] * float(L))[:, :, np.newaxis]
          * T[np.newaxis, np.newaxis, :]
        + (lat_pos[np.newaxis, :] * oval_hw[:, np.newaxis])[:, :, np.newaxis]
          * np.asarray(across, float)[np.newaxis, np.newaxis, :]
    )   # (ring_count, lat_count+1, 3)

    oval_base_off = 0.5 * float(L) * T + embed_v
    oval_tip_off  = float(L) * T + embed_v

    return np.concatenate([
        oval_grid.reshape(-1, 3),
        oval_base_off[np.newaxis],
        oval_tip_off[np.newaxis],
    ], axis=0)   # (n_outer, 3)


# ── Solidification ────────────────────────────────────────────────────────────

def solidify_leaf(
    surface: trimesh.Trimesh,
    inner_v: np.ndarray,
) -> tuple[trimesh.Trimesh, range]:
    """Close an open leaf surface into a watertight solid.

    Parameters
    ----------
    surface : Open leaf surface from :func:`build_leaf_surface`.
    inner_v : ``(n_outer, 3)`` oval vertices pre-computed by
              :func:`build_leaf_oval_offsets` during slot collection, translated
              to world space by adding the leaf's ``pt3d`` base position.
              Must have the same vertex count and index layout as *surface*.

    Returns
    -------
    (solid, wall_face_range)
        *solid* is a closed ``trimesh.Trimesh``.  *wall_face_range* is always
        an empty ``range`` for API compat.
    """
    ring_count  = _LEAF_N_LONG - 1
    lat_count   = _LEAF_N_LAT
    ring_stride = lat_count + 1
    base_idx    = ring_count * ring_stride
    tip_idx     = base_idx + 1
    n_outer     = tip_idx + 1   # == len(surface.vertices)

    outer_v = surface.vertices   # (n_outer, 3)

    all_verts = np.vstack([outer_v, inner_v])

    # ── Faces ────────────────────────────────────────────────────────────────
    outer_faces = surface.faces                         # (F, 3)
    inner_faces = (outer_faces + n_outer)[:, ::-1]     # same topology, reversed winding

    # Wall faces: perimeter loop connecting outer rim to inner oval rim.
    # Perimeter in CCW order when viewed from +normal_axis:
    #   base_idx → left lateral edge (j=0, rings 0…N-1) → tip_idx
    #            → right lateral edge (j=lat_count, rings N-1…0) → base_idx
    left_edge  = [i * ring_stride              for i in range(ring_count)]
    right_edge = [i * ring_stride + lat_count  for i in range(ring_count - 1, -1, -1)]
    perimeter  = [base_idx] + left_edge + [tip_idx] + right_edge

    wall_faces: list[list[int]] = []
    NP = len(perimeter)
    for k in range(NP):
        i0 = perimeter[k]
        i1 = perimeter[(k + 1) % NP]
        j0 = n_outer + i0
        j1 = n_outer + i1
        wall_faces.append([i0, i1, j1])
        wall_faces.append([i0, j1, j0])

    all_faces = np.vstack([
        outer_faces,
        inner_faces,
        np.array(wall_faces, dtype=np.int32),
    ])

    solid = trimesh.Trimesh(vertices=all_verts, faces=all_faces, process=False)
    return solid, range(0, 0)

