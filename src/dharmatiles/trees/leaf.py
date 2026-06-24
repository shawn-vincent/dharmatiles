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
``build_leaf_mesh(...)`` — returns a list of Trimesh parts (blade body + optional
keel) positioned at *base_pos* and oriented along *tangent*.  A *seed* integer
drives the random roll angle so leaves at the same tip position always look
identical (deterministic) and different edge seeds produce different orientations.

Leaf Attachment Model
---------------------
A leaf can only attach to a surface in the following ways. Any orientation
outside these rules produces a geometrically impossible leaf.

**BASE point** — The centre of the leaf's top surface at the attachment end
(opposite the tip).  ``base_pos`` in the API.  The BASE is the point that sits
on the surface.  All rotations are performed around this point.

**Keel embedding constraint** — The keel (structural ridge on the leaf
underside, in the −N direction from the leaf plane) projects BELOW the base.
The backmost point of the keel (at the base end) MUST ALWAYS be embedded into
the surface.  It is never valid for the base to touch the surface while the keel
back wall floats above it.  This means the leaf's N axis (crease/top direction)
always equals the outward surface normal at the attachment point: the blade
sticks out, the keel sticks in.

**Degrees of freedom** — only three are valid:

1. **Position**: where on the surface the BASE sits.

2. **Twist**: rotation about the base→tip axis.  Controls which direction around
   the surface normal the tip points.  Equivalently, it is the compass bearing
   of the leaf within the surface's tangent plane.

   Tips should always point as close to gravity-down as the surface allows.
   On a vertical surface the tip points straight down (± jitter); on a
   near-horizontal surface gravity has no preferred tangent-plane direction
   so the twist is arbitrary (the contact angle carries the tip downward
   regardless).  Tips never point upward on non-horizontal surfaces.

3. **Lift**: rotation about the lateral axis T through the BASE (the axis
   perpendicular to both the current tangent and the surface normal), in the
   plane of (tangent, surface_normal).  The zero of lift is the *contact
   angle* — the rotation that presses the leaf tip just against the parent
   surface.  ``lift_mm`` in :func:`build_leaf_surface` is measured from that
   zero: positive values raise the tip above surface contact.

   - contact angle → tip just touches the parent surface; this is lift = 0.
   - lift > 0      → tip raised further above the surface.
   - 90°           → tangent points directly into the surface (−surface_normal).

   The contact angle is found by :func:`find_contact_angle_for_sphere` and
   applied as a frame rotation in :func:`place_leaf_on_sphere` before
   ``lift_mm`` is added on top.

**Resulting tangent from surface_normal, twist, and contact angle**::

    T0             = unit vector in surface tangent plane in the twist direction
    contact_angle  = rotation angle in [0°, 90°]
    tangent        = T0 * cos(contact_angle) − surface_normal * sin(contact_angle)
    N (up_hint)    = surface_normal rotated by the same angle toward T0

No other axes of freedom exist.  Rotations that don't fit these three (e.g.
tilting the leaf sideways relative to the surface normal, rotating around the
tip, or any combination that lifts the keel back off the surface) produce
nonsensical geometry and must not be introduced.
"""
from __future__ import annotations

from typing import Callable, NamedTuple

import numpy as np
import trimesh

from ._utils import _safe_norm

# ── Constants ──────────────────────────────────────────────────────────────────

_LEAF_N_LONG           = 12    # longitudinal sections (base → tip)
_LEAF_N_LAT            = 10    # lateral sections across the leaf (must be even)
# Fixed vertex index of the tip point in every leaf *surface* mesh
# (as returned by build_leaf_surface — open top face only).
# Layout: top rings | v_base | v_tip  (see build_leaf_surface).
# NB: build_leaf_mesh has an additional bottom-ring block, so its tip index
# is 2*(n_rings*stride)+1; do not confuse the two.
_LEAF_TIP_VERTEX_IDX   = (_LEAF_N_LONG - 1) * (_LEAF_N_LAT + 1) + 1
# Vertex index of the base (midrib start) in the leaf surface mesh.
# The two singular perimeter vertices (tip, base) sit at the axis of symmetry,
# so their centroid-based inward direction is purely axial (±L), not sideways.
# solidify_leaf uses this to identify them; their axial inward is a valid
# in-plane direction and correctly produces root_wall_angle_deg at those points.
_LEAF_BASE_VERTEX_IDX  = (_LEAF_N_LONG - 1) * (_LEAF_N_LAT + 1)
_LEAF_CREASE_SHARPNESS = 10.0  # tanh width of midrib crease (larger = narrower)
# Width profile normalisation: w(s) ∝ s^0.4 × (1-s)^0.8 peaks at s=1/3.
_LEAF_W_PEAK_NORM  = float((1.0 / 3.0) ** 0.4 * (2.0 / 3.0) ** 0.8)
# Thickness-along-length profile: t_long(s) ∝ s^0.5 × (1-s)^1.5, peak at s=0.25.
_LEAF_LONG_T_PEAK  = float(0.25 ** 0.5 * 0.75 ** 1.5)   # ≈ 0.3248

# Default leaf size (mm).  Public so callers can reference them without
# hard-coding magic numbers.
LEAF_LENGTH_MM_DEFAULT = 9.0
LEAF_WIDTH_MM_DEFAULT  = 6.0   # ≈ 2/3 of length

# Tolerance for leaf_placement_from_surface: pos must be within this distance
# of the mesh surface or a ValueError is raised.
_LEAF_SURFACE_MARGIN_MM = 1.0


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

def _build_blade_faces(N_S: int, N_T: int) -> np.ndarray:
    """Build canonical face connectivity for a leaf blade.

    Vertex layout (matches the vectorised ``build_leaf_mesh`` output):
      indices 0 … n_rings*(N_T+1)-1           — top-ring vertices, ring-major
      indices n_rings*(N_T+1) … 2*n*… -1      — bot-ring vertices, ring-major
      index   2*n_rings*(N_T+1)               — v_base
      index   2*n_rings*(N_T+1)+1             — v_tip
    """
    key = (N_S, N_T)
    cached = _BLADE_FACES_CACHE.get(key)
    if cached is not None:
        return cached

    n_rings = N_S - 1
    stride  = N_T + 1
    rt = lambda i: i * stride              # ring i top offset
    rb = lambda i: (n_rings + i) * stride  # ring i bot offset
    v_base = 2 * n_rings * stride
    v_tip  = v_base + 1

    faces: list[list[int]] = []

    # Base fan
    ft0, fb0 = rt(0), rb(0)
    for j in range(N_T):
        j1 = j + 1
        faces.append([v_base, ft0 + j,  ft0 + j1])
        faces.append([v_base, fb0 + j1, fb0 + j ])
    faces.append([v_base, fb0,        ft0       ])
    faces.append([v_base, ft0 + N_T, fb0 + N_T ])

    # Body bands
    for ri in range(n_rings - 1):
        ta, tb = rt(ri), rt(ri + 1)
        ba, bb = rb(ri), rb(ri + 1)
        for j in range(N_T):
            j1 = j + 1
            faces.append([ta + j,  tb + j,  tb + j1])
            faces.append([ta + j,  tb + j1, ta + j1])
            faces.append([ba + j,  ba + j1, bb + j1])
            faces.append([ba + j,  bb + j1, bb + j ])
        faces.append([ta,       ba,       bb      ])
        faces.append([ta,       bb,       tb      ])
        faces.append([ta + N_T, tb + N_T, bb + N_T])
        faces.append([ta + N_T, bb + N_T, ba + N_T])

    # Tip fan
    ftL = rt(n_rings - 1)
    fbL = rb(n_rings - 1)
    for j in range(N_T):
        j1 = j + 1
        faces.append([v_tip, ftL + j1, ftL + j ])
        faces.append([v_tip, fbL + j,  fbL + j1])
    faces.append([v_tip, ftL,        fbL       ])
    faces.append([v_tip, fbL + N_T, ftL + N_T ])

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
    L:       np.ndarray   # (3,) unit growth direction (base → tip)
    T:       np.ndarray   # (3,) unit lateral direction
    N:       np.ndarray   # (3,) unit leaf normal (top/crease faces +N)
    bp:      np.ndarray   # (3,) base position
    v_tip:   np.ndarray   # (3,) tip position  =  bp + length_mm * L
    s_int:   np.ndarray   # (n_rings,) longitudinal stations ∈ (0, 1)
    w_s:     np.ndarray   # (n_rings,) half-widths at each station
    top_pts: np.ndarray   # (n_rings, N_T+1, 3) top-surface vertex grid
    bot_pts: np.ndarray   # (n_rings, N_T+1, 3) bottom-surface vertex grid


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
    L  = _safe_norm(np.asarray(tangent, float))
    bp = np.asarray(base_pos, float)

    world_up = (_safe_norm(np.asarray(up_hint, float)) if up_hint is not None
                else np.array([0.0, 0.0, 1.0]))
    if abs(float(np.dot(L, world_up))) > 0.9:
        world_up = (np.array([1.0, 0.0, 0.0])
                    if abs(float(np.dot(L, np.array([0.0, 0.0, 1.0])))) > 0.9
                    else np.array([0.0, 0.0, 1.0]))
    T0 = np.cross(world_up, L);  T0 /= max(float(np.linalg.norm(T0)), 1e-10)
    N0 = np.cross(L, T0);        N0 /= max(float(np.linalg.norm(N0)), 1e-10)
    T, N = T0, N0

    fold_tan = float(np.tan(np.radians(fold_angle_deg)))
    N_T      = _LEAF_N_LAT
    t_vals   = np.linspace(-1.0, 1.0, N_T + 1)   # (N_T+1,)
    abs_t    = np.abs(t_vals)

    s_int  = np.linspace(0.0, 1.0, _LEAF_N_LONG + 1)[1:-1]        # (n_rings,)
    w_s    = 0.5 * width_mm * _leaf_width_profile(s_int)            # (n_rings,)
    long_t = (s_int ** 0.5 * (1.0 - s_int) ** 1.5) / _LEAF_LONG_T_PEAK

    # ── Compound longitudinal arch + curl ────────────────────────────────────
    # Work as a height profile over the original leaf plane.  The arch is the
    # dominant full-length curve.  The curl integrates a quintic smootherstep
    # slope correction over [CURL_START, 1].  The correction has zero value,
    # slope, and curvature at CURL_START, so it layers onto the arch with a C2
    # join instead of replacing the final part of it.
    curl_start = 2.0 / 3.0          # curl covers the last ⅓ of the leaf
    curl_zone_length = (1.0 - curl_start) * float(length_mm)

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
        """Return arch+curl height and dz/dx for normalized longitudinal stations.

        Lift is NOT computed here — it is applied after all vertex positions
        are built as a rigid rotation of the entire surface around T through
        the base (see the lift block below).
        """
        s = np.asarray(s, float)
        curl_u = np.clip((s - curl_start) / (1.0 - curl_start), 0.0, 1.0)

        arch_z = float(length_mm) * arch_base_slope * s * (1.0 - s)
        arch_dzdx = arch_base_slope * (1.0 - 2.0 * s)

        curl_active = s > curl_start
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

    mid_n, mid_slope = _centerline_profile(s_int)
    midribs = (
        bp[np.newaxis]
        + (s_int[:, np.newaxis] * float(length_mm)) * L[np.newaxis]
        + mid_n[:, np.newaxis] * N[np.newaxis]
    )
    tip_n, _ = _centerline_profile(np.array(1.0))
    v_tip = bp + float(length_mm) * L + float(tip_n.item()) * N

    phi = np.arctan(mid_slope)
    N_local = (-np.sin(phi)[:, np.newaxis] * L[np.newaxis]
               + np.cos(phi)[:, np.newaxis] * N[np.newaxis])

    laterals = (midribs[:, np.newaxis]
                + (t_vals[np.newaxis, :, np.newaxis] * w_s[:, np.newaxis, np.newaxis])
                * T[np.newaxis, np.newaxis])

    tanh_t = np.tanh(abs_t * _LEAF_CREASE_SHARPNESS)
    lobe_profile = _leaf_lobe_profile(abs_t, inner_curve, outer_curve)
    # Fade the crease over the full leaf length.  Smootherstep gives zero value,
    # slope, and curvature at the base and increases continuously toward the
    # tip; long_t then tapers the combined crease smoothly back to zero there.
    crease_fade = (
        6.0 * s_int**5 - 15.0 * s_int**4 + 10.0 * s_int**3
    )
    fold_h = tanh_t[np.newaxis] * (
        w_s[:, np.newaxis]
        * fold_tan
        * long_t[:, np.newaxis]
        * crease_fade[:, np.newaxis]
    )
    lobe_h = (thickness_mm * lobe_profile[np.newaxis]) * long_t[:, np.newaxis]

    # Apply crease + dome offsets along the per-station local normal.
    top_pts = laterals + (fold_h + lobe_h)[:, :, np.newaxis] * N_local[:, np.newaxis, :]
    bot_pts = laterals + fold_h[:, :, np.newaxis]             * N_local[:, np.newaxis, :]

    # ── Lift: rigid rotation of the entire surface around T through bp ────────
    # Applied AFTER arch and curl produce the complete vertex grid; BEFORE
    # walls or root are constructed.  The rotation keeps bp fixed and lifts
    # the tip by approximately lift_mm in the N direction.
    #
    # Axis: T  (lateral axis).
    # Sense: rotates L toward N (tip goes up).  Using Rodrigues around −T with
    #   positive theta achieves R: L → cos*L + sin*N.
    # Angle: arctan(lift_mm / length_mm) — angle that would lift a flat-length
    #   leaf's tip by lift_mm; accurate to within a few percent for arched leaves.
    #
    # Frame vectors L and N are updated so the keel and other downstream
    # builders receive the post-lift normal direction.  T is the rotation axis
    # and is therefore unchanged.
    if abs(float(lift_mm)) > 1e-8:
        theta_lift = float(np.arctan2(float(lift_mm), float(length_mm)))
        c_l = float(np.cos(theta_lift))
        s_l = float(np.sin(theta_lift))

        def _lift_rot(pts: np.ndarray) -> np.ndarray:
            """Rodrigues rotation around −T through bp; lifts L toward N."""
            shape   = pts.shape
            rel     = pts.reshape(-1, 3) - bp[np.newaxis]   # (N, 3)
            T_dot   = (rel @ T)[:, np.newaxis]              # (N, 1)
            T_cross = np.cross(T[np.newaxis], rel)          # (N, 3)  T × rel
            # R(-T, theta)*v = v*c - (T×v)*s + T*(T·v)*(1-c)
            rot     = c_l * rel - s_l * T_cross + (1.0 - c_l) * T_dot * T[np.newaxis]
            return (bp[np.newaxis] + rot).reshape(shape)

        top_pts = _lift_rot(top_pts)
        bot_pts = _lift_rot(bot_pts)
        v_tip   = _lift_rot(v_tip[np.newaxis])[0]

        # Rotate the frame vectors to stay consistent with the new geometry.
        L_prev, N_prev = L.copy(), N.copy()
        L = c_l * L_prev + s_l * N_prev    # L rotates toward N
        N = -s_l * L_prev + c_l * N_prev   # N rotates away from L
        # T is the rotation axis — unchanged.

    return _LeafGeometry(
        L=L, T=T, N=N, bp=bp, v_tip=v_tip,
        s_int=s_int, w_s=w_s, top_pts=top_pts, bot_pts=bot_pts,
    )


# ── Keel prism ─────────────────────────────────────────────────────────────────

def _build_leaf_keel_prism(
    base_vertex: np.ndarray,
    tip_vertex: np.ndarray,
    neg_t_edge: np.ndarray,
    pos_t_edge: np.ndarray,
    N: np.ndarray,
    T: np.ndarray,
    s_values: np.ndarray,
    half_widths: np.ndarray,
    keel_depth_mm: float,
    keel_tip_angle_deg: float,
    all_bot_pts: np.ndarray | None = None,
) -> trimesh.Trimesh:
    """Leaf keel: a V cross-section that meets at a ridge on the midrib.

    Parameters
    ----------
    base_vertex       : (3,)   leaf base point (s=0)
    tip_vertex        : (3,)   leaf tip point  (s=1)
    neg_t_edge        : (n,3)  bottom-surface vertices at t=−1 (−T side), s increasing
    pos_t_edge        : (n,3)  bottom-surface vertices at t=+1 (+T side), s increasing
    N                 : (3,)   leaf normal (toward top surface)
    T                 : (3,)   lateral direction (+T = one leaf edge)
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
    n      = len(pos_t_edge)
    Nu     = N / max(float(np.linalg.norm(N)), 1e-12)
    L_len  = float(np.linalg.norm(tip_vertex - base_vertex))
    D      = float(keel_depth_mm)
    R      = D
    n_st   = n + 2

    # ── Vectorised station geometry ─────────────────────────────────────────
    # s_all: (n+2,) including base (s=0) and tip (s=1)
    s_arr  = np.asarray(s_values, float)
    s_all  = np.concatenate([[0.0], s_arr, [1.0]])          # (n_st,)

    # Ridge depth profile: quarter-circle fillet from tip back over keel_depth_mm.
    x_all  = (1.0 - s_all) * L_len                         # distance from tip
    ridge_depths = np.where(
        x_all >= R,
        D,
        np.sqrt(np.maximum(0.0, R * R - (R - x_all) ** 2))
    )  # (n_st,)

    # topP / topN station points.
    # Endpoints (k=0 and k=n_st-1) pinch to base/tip vertex; interior uses edge verts.
    topP_arr = np.vstack([base_vertex[np.newaxis], pos_t_edge, tip_vertex[np.newaxis]])  # (n_st, 3)
    topN_arr = np.vstack([base_vertex[np.newaxis], neg_t_edge, tip_vertex[np.newaxis]])  # (n_st, 3)

    # Ridge points along the midrib spine.
    mids  = (base_vertex[np.newaxis] +
             s_all[:, np.newaxis] * (tip_vertex - base_vertex)[np.newaxis])  # (n_st, 3)
    ridge_arr = mids - ridge_depths[:, np.newaxis] * Nu[np.newaxis]          # (n_st, 3)

    # A station's ridge collapses to the top point when depth ≈ 0 (tip region).
    degenerate = ridge_depths < 1e-9   # (n_st,) bool — True at tip

    # ── Build vertex list and index arrays ──────────────────────────────────
    # Vertex layout: iTP[k] = k (top-P row)
    #                iTN[k] = k for width-pinch stations, else n_st + (k-1) - offset
    #                iR[k]  = iTP[k] if degenerate, else ...
    verts: list[np.ndarray] = list(topP_arr)  # indices 0 … n_st-1
    n_v = n_st
    iTP = list(range(n_st))

    iTN: list[int] = []
    for k in range(n_st):
        if k == 0 or k == n_st - 1:
            iTN.append(iTP[k])
        else:
            verts.append(topN_arr[k])
            iTN.append(n_v)
            n_v += 1

    iR: list[int] = []
    for k in range(n_st):
        if degenerate[k]:
            iR.append(iTP[k])
        else:
            verts.append(ridge_arr[k])
            iR.append(n_v)
            n_v += 1

    F: list[list[int]] = []

    def _quad(a: int, b: int, c: int, d: int) -> None:
        if a != b and b != c and a != c:
            F.append([a, b, c])
        if a != c and c != d and a != d:
            F.append([a, c, d])

    for k in range(n_st - 1):
        _quad(iTP[k], iTP[k + 1], iR[k + 1], iR[k])
        _quad(iR[k], iR[k + 1], iTN[k + 1], iTN[k])

    # ── Top closure ─────────────────────────────────────────────────────────
    # When all_bot_pts is supplied (the full blade-bottom vertex grid, shape
    # (n_rings, N_T+1, 3)) we triangulate the top as a curved surface that
    # exactly matches the blade underside — this removes the visible flat.
    # When it is None we fall back to the legacy centroid fan.
    if all_bot_pts is not None:
        N_T_top = all_bot_pts.shape[1] - 1   # N_T (number of lateral columns - 1)

        # Build a 2-D index grid: top_grid[row, col] → vertex index in verts.
        # Rows:  0 = base station, 1..n_st-2 = interior rings, n_st-1 = tip.
        # Cols:  0 = neg edge (iTN), N_T_top = pos edge (iTP), 1..N_T-1 = new.
        top_grid = np.empty((n_st, N_T_top + 1), dtype=np.int64)

        # Edge columns: reuse existing keel vertices (no duplicates).
        top_grid[:, 0]       = iTN          # neg edge (t=0)
        top_grid[:, N_T_top] = iTP          # pos edge (t=N_T)

        # Base and tip rows are fully degenerate (single shared vertex each).
        top_grid[0,       :] = iTP[0]       # base vertex
        top_grid[n_st - 1, :] = iTP[n_st - 1]  # tip vertex

        # Interior columns for each interior ring station.
        for k in range(1, n_st - 1):        # k = keel station index
            r = k - 1                        # all_bot_pts ring index
            for t in range(1, N_T_top):
                verts.append(all_bot_pts[r, t])
                top_grid[k, t] = n_v
                n_v += 1

        # Quad-strip triangulation across the grid.
        for row in range(n_st - 1):
            for col in range(N_T_top):
                a = int(top_grid[row,     col    ])
                b = int(top_grid[row,     col + 1])
                c = int(top_grid[row + 1, col + 1])
                d = int(top_grid[row + 1, col    ])
                _quad(a, b, c, d)
    else:
        # Legacy centroid fan (produces a flat face — kept as fallback).
        loop = (iTP + [iTN[k] for k in range(n_st - 2, 0, -1)])
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
        np.array(F, dtype=np.int32),
        (cache_tag, round(L_len, 6), round(D, 6)),
    )


# ── Surface-query placement helpers ───────────────────────────────────────────

def leaf_placement_from_surface(
    mesh: trimesh.Trimesh,
    pos: np.ndarray,
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Compute ``(base_pos, tangent, up_hint)`` from a point on a mesh surface.

    Derives all three placement inputs to :func:`build_leaf_mesh` /
    :func:`build_leaf_surface` from the parent mesh geometry.  The returned
    values are ready to pass directly to either builder.

    Parameters
    ----------
    mesh : Parent ``trimesh.Trimesh`` — the surface the leaf grows from.
    pos  : Approximate world position.  Snapped to the nearest point on
           ``mesh``; the returned ``base_pos`` is the snapped point.
           Raises ``ValueError`` if ``pos`` is farther than
           ``_LEAF_SURFACE_MARGIN_MM`` from the surface.

    Returns
    -------
    (base_pos, tangent, up_hint)

    Notes
    -----
    *up_hint* is the **interpolated vertex normal** at the snapped point
    (barycentric blend of the triangle's three vertex normals), giving smooth
    results on curved surfaces regardless of mesh resolution.

    *tangent* is gravity-down projected onto the tangent plane — the leaf tip
    points as far downward as the surface allows.  Callers that need a
    different orientation can derive their own tangent from the returned frame::

        tangent = T0 * cos(θ) − up_hint * sin(θ)

    where ``T0`` is the returned tangent (θ = 0) and θ is the contact angle
    computed by :func:`find_contact_angle_for_sphere`.
    """
    pos = np.asarray(pos, float).ravel()[:3]

    # 1. Snap to nearest surface point.
    pts, dists, face_ids = trimesh.proximity.closest_point(mesh, pos[np.newaxis])
    base_pos = pts[0]
    dist = float(dists[0])
    if dist > _LEAF_SURFACE_MARGIN_MM:
        raise ValueError(
            f"pos is {dist:.3f} mm from the mesh surface "
            f"(limit: _LEAF_SURFACE_MARGIN_MM = {_LEAF_SURFACE_MARGIN_MM})"
        )

    # 2. Interpolated vertex normal via barycentric coordinates.
    # Clamp bary to [0, 1] before blending: floating-point rounding in
    # closest_point can place base_pos just outside the triangle (on a shared
    # edge or vertex), causing points_to_barycentric to return a small negative
    # weight.  Clamping + renormalising ensures we interpolate rather than
    # extrapolate vertex normals.
    face_id   = int(face_ids[0])
    tri_verts = mesh.vertices[mesh.faces[face_id]]          # (3, 3)
    bary      = trimesh.triangles.points_to_barycentric(
        tri_verts[np.newaxis], base_pos[np.newaxis]
    )[0]                                                    # (3,)
    bary      = np.clip(bary, 0.0, 1.0)
    bary_sum  = float(bary.sum())
    if bary_sum > 1e-10:
        bary /= bary_sum
    v_normals = mesh.vertex_normals[mesh.faces[face_id]]    # (3, 3)
    up_hint   = _safe_norm(bary @ v_normals)                # (3,)

    # 3. Gravity-down projected onto the tangent plane → tangent.
    gravity_down = np.array([0.0, 0.0, -1.0])
    grav_proj    = gravity_down - float(np.dot(gravity_down, up_hint)) * up_hint
    grav_len     = float(np.linalg.norm(grav_proj))
    if grav_len < 1e-6:
        # Surface is nearly horizontal — no gravity preference; pick arbitrary tangent.
        arb = np.array([1.0, 0.0, 0.0])
        if abs(float(np.dot(arb, up_hint))) > 0.9:
            arb = np.array([0.0, 1.0, 0.0])
        tangent = _safe_norm(np.cross(up_hint, arb))
    else:
        tangent = grav_proj / grav_len

    return base_pos, tangent, up_hint


def build_leaf_on_surface(
    mesh: trimesh.Trimesh,
    pos: np.ndarray,
    *,
    length_mm: float = LEAF_LENGTH_MM_DEFAULT,
    width_mm:  float = LEAF_WIDTH_MM_DEFAULT,
    **leaf_kwargs,
) -> list[trimesh.Trimesh]:
    """Build a leaf mesh placed on a mesh surface point.

    Convenience wrapper combining :func:`leaf_placement_from_surface` and
    :func:`build_leaf_mesh` in one call.  All placement inputs (``base_pos``,
    ``tangent``, ``up_hint``) are derived automatically from the mesh geometry;
    the leaf tip points gravity-down in the surface's tangent plane.

    Parameters
    ----------
    mesh       : Parent ``trimesh.Trimesh`` the leaf grows from.
    pos        : Approximate world position (snapped to nearest surface point).
    length_mm  : Leaf length.  Default ``LEAF_LENGTH_MM_DEFAULT``.
    width_mm   : Maximum leaf width.  Default ``LEAF_WIDTH_MM_DEFAULT``.
    **leaf_kwargs : Forwarded to :func:`build_leaf_mesh`
                   (``thickness_mm``, ``keel_depth_mm``, etc.)

    Returns
    -------
    list[trimesh.Trimesh]
        ``[blade]`` when ``keel_depth_mm <= 0``, ``[blade, keel]`` otherwise.
    """
    base_pos, tangent, up_hint = leaf_placement_from_surface(mesh, pos)
    return build_leaf_mesh(
        base_pos=base_pos, tangent=tangent, up_hint=up_hint,
        length_mm=length_mm, width_mm=width_mm, **leaf_kwargs
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
) -> trimesh.Trimesh:
    """Open leaf surface — the top face only, with all visible geometry.

    Returns a single open ``trimesh.Trimesh`` (disk topology).  The mesh has
    one boundary loop running along the two lateral edges from base to tip —
    it can be stitched directly to a branchlet loft to form a single closed
    solid with no overlapping parts.

    All geometry of the top face is present: the ovate teardrop outline,
    the dome-shaped lobes (two humps rising from the midrib crease), the
    V-shaped crease along the midrib, and the compound longitudinal curve.
    Face normals point outward (+N at the base, rotating with the centerline).

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

    n_rings = _LEAF_N_LONG - 1
    N_T     = _LEAF_N_LAT
    stride  = N_T + 1
    v_base_i = n_rings * stride
    v_tip_i  = n_rings * stride + 1

    verts = np.concatenate([
        g.top_pts.reshape(-1, 3),
        g.bp[np.newaxis],
        g.v_tip[np.newaxis],
    ], axis=0)

    faces: list[list[int]] = []

    # Base fan — v_base fans into ring 0 of the top surface.
    # Winding [v_base, j, j+1] gives +N outward normals. ✓
    for j in range(N_T):
        faces.append([v_base_i, j, j + 1])

    # Body — quad strip across consecutive top rings.
    # [a, d, c] and [a, c, b] give +N outward normals. ✓
    for ri in range(n_rings - 1):
        for j in range(N_T):
            a = ri * stride + j
            b = ri * stride + j + 1
            c = (ri + 1) * stride + j + 1
            d = (ri + 1) * stride + j
            faces.append([a, d, c])
            faces.append([a, c, b])

    # Tip fan — v_tip fans into the last ring of the top surface.
    # Winding [v_tip, last+j+1, last+j] gives +N outward normals. ✓
    last = (n_rings - 1) * stride
    for j in range(N_T):
        faces.append([v_tip_i, last + j + 1, last + j])

    # curl_deg is in the cache key because winding decisions differ between
    # curl=0 (flat tip) and curl>0 (tip raised).  lift_mm is excluded: it
    # rotates all vertices uniformly and does not change which side is outward,
    # so the same winding applies for all lift values at a given curl.
    return _mesh_with_fixed_normals(
        verts,
        np.array(faces, dtype=np.int32),
        ("leaf_surface", _LEAF_N_LONG, N_T, round(float(curl_deg), 4)),
    )


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

    N_S = _LEAF_N_LONG
    N_T = _LEAF_N_LAT

    # Vertex array layout: top rings | bot rings | v_base | v_tip
    # Matches the index arithmetic in _build_blade_faces().
    verts = np.concatenate([
        g.top_pts.reshape(-1, 3),
        g.bot_pts.reshape(-1, 3),
        g.bp[np.newaxis],
        g.v_tip[np.newaxis],
    ], axis=0)

    faces = _build_blade_faces(N_S, N_T)
    mesh  = _mesh_with_fixed_normals(verts, faces, ("blade", N_S, N_T))
    parts: list[trimesh.Trimesh] = [mesh]

    if keel_depth_mm > 1e-6:
        keel = _build_leaf_keel_prism(
            base_vertex=g.bp,
            tip_vertex=g.v_tip,
            neg_t_edge=g.bot_pts[:, 0,   :],
            pos_t_edge=g.bot_pts[:, N_T, :],
            N=g.N, T=g.T,
            s_values=g.s_int,
            half_widths=g.w_s,
            keel_depth_mm=keel_depth_mm,
            keel_tip_angle_deg=keel_tip_angle_deg,
            all_bot_pts=g.bot_pts,
        )
        if len(keel.vertices) > 0:
            parts.append(keel)

    return parts


# ── Solidification and FDM analysis ───────────────────────────────────────────

# Root-embedding depth: how far the root ring extends past the parent
# surface after the per-vertex inward raycast.  When no parent mesh is
# supplied (or a ray misses), falls back to a flat projection this deep
# below the perimeter vertex.  Public so callers can reference it without
# hard-coding the literal.
LEAF_ROOT_EMBED_MM: float = 0.75

# Maximum raycast hit distance accepted during root embedding.  Hits beyond
# this are discarded and fall back to the angled-offset default.  Prevents
# spike vertices when a perimeter vertex ends up inside the parent mesh (e.g.
# from large roll jitter), causing the ray to hit the far side of the mesh.
_LEAF_ROOT_MAX_HIT_MM: float = 10.0

# Angle (degrees) between the leaf surface plane and the root wall,
# measured at the perimeter edge (the "face angle" or "taper angle").
# 90° = wall perpendicular to leaf surface (ray straight along -n, no taper).
# Smaller values undercut inward: the root ring narrows toward the perimeter
# centroid, giving beefy anchoring especially at sharp corners (tip, base).
# Public so callers can reference it without hard-coding the literal.
LEAF_ROOT_WALL_ANGLE_DEG: float = 50.0

# FDM printability floor: faces whose downward slope exceeds this are overhangs.
_LEAF_FDM_FLOOR_DEG: float = 45.0

# Contact tolerance for support-mesh queries (compensates for faceted surfaces).
_LEAF_FDM_SUPPORT_TOLERANCE_MM: float = 0.05

def find_contact_angle(
    base_pos:  np.ndarray,
    T0:        np.ndarray,
    up_hint:   np.ndarray,
    is_clear:  Callable[[np.ndarray], bool],
    **leaf_kwargs,
) -> float:
    """Find the contact angle (radians) using a generic collision-free predicate.

    The *contact angle* is the rotation around the lateral axis T through
    ``base_pos`` that presses the leaf tip just against the parent surface —
    it establishes the zero point from which ``lift_mm`` is measured.

    Builds a flat leaf (contact angle = 0), identifies *risky* vertices (those
    currently outside the obstacle that could penetrate it as the angle
    increases), then binary-searches for the largest rotation that keeps all
    risky vertices clear.

    Rotation formula: ``tangent = T0 * cos(θ) - up_hint * sin(θ)``.

    Parameters
    ----------
    base_pos  : Leaf base — pivot point for the rotation.
    T0        : Flat (angle = 0) tangent direction.
    up_hint   : Outward surface normal at base_pos.
    is_clear  : Collision-free predicate.  Called with an (N, 3) vertex array;
                must return ``True`` iff **all** supplied points are in the
                free region.  For a sphere of radius *r* centred at the origin::

                    is_clear = lambda pts: np.all(np.linalg.norm(pts, axis=1) >= r)

    **leaf_kwargs : Passed to :func:`build_leaf_surface`
                   (``length_mm``, ``width_mm``, ``fold_angle_deg``, etc.)
                   ``lift_mm`` is **ignored** here (forced to 0.0): the contact
                   angle is the zero-lift position, so the search must be run
                   without lift applied — ``lift_mm`` is added on top afterward
                   by :func:`place_leaf_on_sphere`.

    Returns
    -------
    float
        Contact angle in radians.  Returns 0.0 if no vertices are clear at
        angle = 0 (leaf fully inside the parent shape — degenerate placement).
    """
    # Search at lift_mm=0: the contact angle is the zero-lift reference.
    # Lift is applied on top afterward; including it here would cause the search
    # to compensate for it, making the lift invisible in the final geometry.
    flat_kwargs = {**leaf_kwargs, 'lift_mm': 0.0}
    flat = build_leaf_surface(base_pos=base_pos, tangent=T0, up_hint=up_hint, **flat_kwargs)

    # Identify *risky* vertices: those clear at angle = 0 that could become
    # unclear as the angle increases.  Per-vertex calls happen once at setup,
    # not inside the hot bisection loop.
    risky_mask  = np.array([is_clear(v[np.newaxis]) for v in flat.vertices])
    risky_verts = flat.vertices[risky_mask]
    if len(risky_verts) == 0:
        return 0.0

    # Rodrigues rotation axis: rotating T0 around cross(-T0, up_hint) by θ
    # gives tangent = T0*cos(θ) − up_hint*sin(θ).
    axis = _safe_norm(np.cross(-T0, up_hint))

    def _rotate(pts: np.ndarray, theta: float) -> np.ndarray:
        c, s = np.cos(theta), np.sin(theta)
        rel  = pts - base_pos
        return (base_pos
                + rel * c
                + np.cross(axis, rel) * s
                + axis * (rel @ axis)[:, np.newaxis] * (1.0 - c))

    # Check upper bound: if never penetrates even at full π, return π.
    lo, hi = 0.0, np.pi
    if is_clear(_rotate(risky_verts, hi)):
        return hi

    # Bisect between lo (ok) and hi (penetrates).
    # 48 iterations gives ~4e-15 rad precision.
    for _ in range(48):
        mid = 0.5 * (lo + hi)
        if is_clear(_rotate(risky_verts, mid)):
            lo = mid
        else:
            hi = mid
    return lo


def find_contact_angle_for_sphere(
    base_pos:      np.ndarray,
    T0:            np.ndarray,
    up_hint:       np.ndarray,
    sphere_radius: float,
    *,
    clearance_mm:  float = 0.0,
    **leaf_kwargs,
) -> float:
    """Find the contact angle (radians) that presses a leaf against a sphere.

    The contact angle is the rotation around the lateral axis T through
    ``base_pos`` that places the leaf tip just against the sphere surface —
    the zero point from which ``lift_mm`` is measured.

    Convenience wrapper around :func:`find_contact_angle` for a sphere centred
    at the origin.  Equivalent to::

        min_dist = sphere_radius + clearance_mm
        find_contact_angle(
            base_pos, T0, up_hint,
            lambda pts: np.all(np.linalg.norm(pts, axis=1) >= min_dist),
            **leaf_kwargs,
        )

    Parameters
    ----------
    base_pos      : Leaf base — pivot point for the rotation.
    T0            : Flat (angle = 0) tangent direction.
    up_hint       : Outward surface normal at base_pos.
    sphere_radius : Radius of the sphere (centred at origin).
    clearance_mm  : Minimum clearance from the sphere surface (default 0).
    **leaf_kwargs : Passed to :func:`build_leaf_surface`
                   (``length_mm``, ``width_mm``, ``fold_angle_deg``, etc.)

    Returns
    -------
    float
        Contact angle in radians.  Returns 0.0 if no vertices are outside the
        sphere at angle = 0 (leaf fully inside — shouldn't normally occur).
    """
    min_dist: float = sphere_radius + clearance_mm
    is_clear: Callable[[np.ndarray], bool] = (
        lambda pts: bool(np.all(np.linalg.norm(pts, axis=1) >= min_dist))
    )
    return find_contact_angle(base_pos, T0, up_hint, is_clear, **leaf_kwargs)


def boundary_loop(mesh: trimesh.Trimesh) -> list[int]:
    """Return the perimeter vertex indices of an open mesh as an ordered loop.

    Each boundary edge appears exactly once in an undirected sense; the loop
    is walked by following the unique chain of boundary adjacencies.

    Parameters
    ----------
    mesh : An open ``trimesh.Trimesh`` (one boundary loop).

    Returns
    -------
    list[int]
        Vertex indices forming the boundary loop, in order.  The loop is
        implicitly closed: the last vertex connects back to the first.
    """
    edges_sorted = np.sort(mesh.edges, axis=1)
    unique, counts = np.unique(edges_sorted, axis=0, return_counts=True)
    bnd_edges = unique[counts == 1]

    adj: dict[int, list[int]] = {}
    for a, b in bnd_edges.tolist():
        adj.setdefault(a, []).append(b)
        adj.setdefault(b, []).append(a)

    start = int(bnd_edges[0, 0])
    loop, prev, curr = [start], -1, start
    while True:
        a, b = adj[curr]
        nxt = b if a == prev else a
        if nxt == start:
            break
        loop.append(nxt)
        prev, curr = curr, nxt
    return loop


def solidify_leaf(
    surface:              trimesh.Trimesh,
    up_hint:              np.ndarray,
    embed_mm:             float = LEAF_ROOT_EMBED_MM,
    *,
    parent_mesh:          trimesh.Trimesh | None = None,
    root_wall_angle_deg:  float = LEAF_ROOT_WALL_ANGLE_DEG,
) -> tuple[trimesh.Trimesh, range]:
    """Close an open leaf surface into a watertight solid.

    For each boundary vertex, casts a ray at ``root_wall_angle_deg`` from
    the leaf plane — angled inward toward the perimeter centroid — to find
    where it crosses the parent surface, then places the root vertex
    ``embed_mm`` further along that same ray.  This embeds a consistent
    depth past the parent surface regardless of how far arched, curled, or
    tilted the leaf is from the surface.

    ``root_wall_angle_deg`` is the angle between the root wall and the leaf
    surface plane, measured at the perimeter edge — the same angle a
    machinist would call the *face angle* or *taper angle*:

    * **90°** — wall is perpendicular to the leaf surface.  Ray goes straight
      along ``-up_hint``.  Root ring is the same shape as the perimeter.
    * **< 90°** — wall undercuts inward: the root ring converges toward the
      perimeter centroid, giving a tapered mortise grip.  Sharp corners
      (tip, base) converge most, producing "beefy" anchoring geometry there.
    * **50°** (default) — wall makes a 50° angle with the leaf surface plane
      (40° from perpendicular).  ``sin 50° ≈ 0.77`` along ``-n``,
      ``cos 50° ≈ 0.64`` toward the centroid.

    When ``parent_mesh`` is ``None`` or a ray misses the surface, the vertex
    falls back to moving ``embed_mm`` along the same angled direction from
    the perimeter position (no raycast, fixed offset).

    Quad walls bridge the perimeter to the root ring; a centroid fan caps
    the buried end.

    The tip vertex is handled the same as every other perimeter vertex —
    the angled raycast finds the parent surface under it, and ``embed_mm``
    is applied past the hit.  No special-case override is needed.

    Parameters
    ----------
    surface             : Open leaf surface from :func:`build_leaf_surface`.
    up_hint             : Leaf plane normal (outward from the parent surface).
    embed_mm            : Distance along the ray direction past the parent
                          surface hit for every root vertex.  Also used as
                          the angled fallback offset when a ray misses.
    parent_mesh         : Mesh to raycast against for per-vertex embed depth.
                          Pass the full support mesh (sphere + trunk, etc.).
                          ``None`` disables raycasting and uses the angled
                          fallback for all vertices.
    root_wall_angle_deg : Angle between the root wall and the leaf surface
                          plane, measured at the perimeter edge.  90° = wall
                          perpendicular to the leaf surface (no taper).
                          Smaller values undercut inward — sharp corners
                          become beefier.  Default ``LEAF_ROOT_WALL_ANGLE_DEG``
                          (70°).

    Returns
    -------
    (solid, wall_face_range)
        *solid* is a closed ``trimesh.Trimesh``.  *wall_face_range* is the
        ``range`` of face indices belonging to the wall quads only (not the
        original surface faces or the bottom cap), suitable for external
        per-face FDM printability analysis.
    """
    n    = _safe_norm(np.asarray(up_hint, float))
    loop = boundary_loop(surface)
    NP   = len(loop)

    perim = surface.vertices[loop]     # (NP, 3)

    # ── Per-vertex ray directions ─────────────────────────────────────────────
    # root_wall_angle_deg is measured relative to the LOCAL leaf surface at
    # each boundary vertex, not the global up_hint direction.  Arch, curl, and
    # lift all rotate the local surface normal away from up_hint — especially
    # at the tip (phi_tip = curl_deg + arch correction).  Using the global n
    # for every vertex produces the wrong angle at the tip and base where the
    # deviation is largest.
    #
    # Fix: take per-vertex local normals directly from the open surface mesh.
    # trimesh's vertex_normals are area-weighted face-normal averages; they
    # track the curved leaf surface closely (dot ≈ 0.998 vs. the analytic
    # N_local in tests) without any API changes to this function.
    #
    # n (global up_hint) is still used below for the cap-plane projection,
    # where an approximate plane through the buried root ring is sufficient.
    local_n = surface.vertex_normals[np.array(loop)]   # (NP, 3)  — pre-normalised by trimesh

    centroid   = perim.mean(axis=0)                                   # (3,)
    raw_inward = centroid[np.newaxis] - perim                         # (NP, 3)
    # Project each inward vector onto its vertex's OWN local tangent plane.
    dot_ln     = np.einsum('ij,ij->i', raw_inward, local_n)           # (NP,)
    raw_inward -= dot_ln[:, np.newaxis] * local_n                     # (NP, 3)
    inward_norms = np.linalg.norm(raw_inward, axis=1, keepdims=True)
    inward = np.where(inward_norms > 1e-8,
                      raw_inward / np.where(inward_norms > 1e-8, inward_norms, 1.0),
                      0.0)                                            # (NP, 3)

    # ray = sin(angle)*(-local_n) + cos(angle)*inward
    # → unit vector at root_wall_angle_deg from the local leaf surface.
    # At 90° this reduces to pure -local_n (no taper).
    angle_rad = float(np.radians(root_wall_angle_deg))
    sin_a     = float(np.sin(angle_rad))
    cos_a     = float(np.cos(angle_rad))
    ray_dirs  = sin_a * (-local_n) + cos_a * inward                  # (NP, 3)

    # Fallback root: move embed_mm along the ray direction from each perim vertex.
    root = perim + float(embed_mm) * ray_dirs                         # (NP, 3)

    if parent_mesh is not None:
        # Raycast each vertex along its angled direction to find the parent
        # surface, then embed embed_mm past the hit along the same ray.
        # Offset origins slightly along the LOCAL outward normal so a vertex
        # sitting exactly on the parent surface doesn't self-intersect.
        origins = perim + _LEAF_FDM_SUPPORT_TOLERANCE_MM * local_n   # (NP, 3)
        locs, ray_idx, _ = parent_mesh.ray.intersects_location(
            ray_origins=origins,
            ray_directions=ray_dirs,
            multiple_hits=True,
        )
        if len(locs) > 0:
            for ri in np.unique(ray_idx).tolist():
                mask  = ray_idx == ri
                hits  = locs[mask]
                dists = np.linalg.norm(hits - origins[int(ri)], axis=1)
                near  = dists <= _LEAF_ROOT_MAX_HIT_MM
                if near.any():
                    hit = hits[near][int(np.argmin(dists[near]))]
                    root[int(ri)] = hit + float(embed_mm) * ray_dirs[int(ri)]
        # Perimeter vertices whose ray missed or hit too far keep the fallback.

    # Project the ring mean onto the cap plane (normal = n, through root[0])
    # so the centroid stays inside the ring even when the ring is non-planar
    # (e.g. a leaf placed at a large contact angle on a small sphere).
    raw_center = root.mean(axis=0)
    center     = raw_center - float(np.dot(raw_center - root[0], n)) * n
    n_surf    = len(surface.vertices)
    root_base = n_surf
    cap_ctr   = n_surf + NP

    all_verts = np.vstack([surface.vertices, root, center[np.newaxis]])

    wall_faces: list[list[int]] = []
    for i in range(NP):
        j    = (i + 1) % NP
        a, b = loop[i], loop[j]
        d, c = root_base + i, root_base + j
        wall_faces += [[a, b, c], [a, c, d]]

    cap_faces = [
        [cap_ctr, root_base + (i + 1) % NP, root_base + i]
        for i in range(NP)
    ]

    wall_start = len(surface.faces)
    wall_end   = wall_start + len(wall_faces)

    all_faces = np.vstack([
        surface.faces,
        np.array(wall_faces, dtype=np.int32),
        np.array(cap_faces,  dtype=np.int32),
    ])

    solid = trimesh.Trimesh(vertices=all_verts, faces=all_faces, process=False)
    solid.fix_normals()
    return solid, range(wall_start, wall_end)


def place_leaf_on_sphere(
    base_pos:      np.ndarray,
    T0:            np.ndarray,
    up_hint:       np.ndarray,
    sphere_radius: float,
    parent_mesh:   trimesh.Trimesh,
    *,
    contact_angle_rad:   float | None = None,
    clearance_mm:        float = 0.0,
    embed_mm:            float = LEAF_ROOT_EMBED_MM,
    root_wall_angle_deg: float = LEAF_ROOT_WALL_ANGLE_DEG,
    **leaf_kwargs,
) -> tuple[trimesh.Trimesh, range]:
    """Build and solidify a leaf placed on a sphere surface.

    Single primitive covering the full placement pipeline:

    1. Find contact angle — rotation around the lateral axis T that presses
       the leaf tip just against the sphere (skipped when *contact_angle_rad*
       is supplied).  This is the zero point: ``lift_mm = 0`` in
       *leaf_kwargs* means the tip sits at sphere contact; positive values
       raise it above.
    2. Apply the contact angle — rotate the frame (tangent and up_hint) so
       that ``lift_mm`` in :func:`build_leaf_surface` is measured from
       sphere contact.
    3. :func:`build_leaf_surface` — open surface mesh (``lift_mm`` applied
       here as an offset above contact).
    4. :func:`solidify_leaf` — per-vertex inward raycast against
       *parent_mesh* to find each perimeter vertex's true surface distance,
       then embed ``embed_mm`` past the surface at ``root_wall_angle_deg``
       from the leaf surface plane.  The tip vertex is handled identically
       to every other perimeter vertex.

    Parameters
    ----------
    base_pos           : Leaf base position on the sphere surface.
    T0                 : Flat tangent direction (angle = 0) — gravity-down in
                         the surface tangent plane, as returned by
                         :func:`leaf_placement_from_surface`.
    up_hint            : Outward surface normal at *base_pos*.
    sphere_radius      : Radius of the sphere centred at the origin (used only
                         for the contact-angle search).
    parent_mesh        : Full support mesh (sphere + trunk, etc.) used for
                         per-vertex embed raycasts in :func:`solidify_leaf`.
    contact_angle_rad  : Override the auto-computed contact angle (radians).
                         ``None`` (default) → computed automatically via
                         :func:`find_contact_angle_for_sphere`.
    clearance_mm       : Minimum clearance from the sphere for the contact
                         angle search.
    embed_mm             : How far past the parent surface each root vertex is
                           placed.  Passed to :func:`solidify_leaf`.
    root_wall_angle_deg  : Angle between the root wall and the leaf surface
                           plane at the perimeter edge.  90° = perpendicular
                           (no taper); smaller values undercut inward, giving
                           beefy corners at the tip and base.  Default
                           ``LEAF_ROOT_WALL_ANGLE_DEG`` (70°).  Passed to
                           :func:`solidify_leaf`.
    **leaf_kwargs        : Passed to :func:`build_leaf_surface` and (for the
                           contact angle search) to
                           :func:`find_contact_angle_for_sphere`.  Typical keys:
                           ``length_mm``, ``width_mm``, ``fold_angle_deg``,
                           ``curl_deg``, ``lift_mm``.

    Returns
    -------
    (solid, wall_face_range)
        *solid* is a closed watertight ``trimesh.Trimesh``.
        *wall_face_range* is the ``range`` of wall face indices suitable for
        FDM printability analysis (see :func:`solidify_leaf`).
    """
    if contact_angle_rad is None:
        contact_angle_rad = find_contact_angle_for_sphere(
            base_pos, T0, up_hint, sphere_radius,
            clearance_mm=clearance_mm,
            **leaf_kwargs,
        )

    # Apply the contact angle: rotate the frame so lift_mm=0 → sphere contact.
    c, s      = float(np.cos(contact_angle_rad)), float(np.sin(contact_angle_rad))
    tangent   = _safe_norm(np.asarray(T0, float) * c - np.asarray(up_hint, float) * s)
    up_placed = _safe_norm(np.asarray(up_hint, float) * c + np.asarray(T0, float) * s)

    leaf_surf = build_leaf_surface(
        base_pos=base_pos, tangent=tangent, up_hint=up_placed, **leaf_kwargs,
    )

    return solidify_leaf(
        leaf_surf, up_placed, embed_mm,
        parent_mesh=parent_mesh,
        root_wall_angle_deg=root_wall_angle_deg,
    )


