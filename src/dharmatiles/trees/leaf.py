"""Single-leaf mesh generator — reusable across any context.

A leaf is an ovate, keeled blade:

* **Outline** (top view): ovate teardrop — peak width at ≈ 1/3 from base,
  tapering to a rounded base and a pointed tip.
* **Cross-section**: a dome that rises from the midrib, peaks at mid-lateral,
  and falls back to zero at the edge.  Profile:
  ``sin(π × |t|) × thickness_mm × long_t(s)``.
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
"""
from __future__ import annotations

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


# ── Public API ─────────────────────────────────────────────────────────────────

def build_leaf_mesh(
    *,
    base_pos: np.ndarray,
    tangent: np.ndarray,
    length_mm: float,
    width_mm: float,
    thickness_mm: float = 0.24,
    fold_angle_deg: float = 3.0,
    keel_depth_mm: float = 1.0,
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
    thickness_mm    : dome height at peak (s ≈ 0.25).  Default 0.24.
    fold_angle_deg  : midrib crease V-angle.  Default 3.0.
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
    L  = _safe_norm(np.asarray(tangent, float))
    bp = np.asarray(base_pos, float)

    # Orthonormal frame (L, T, N).  N (the leaf's top/crease side) is biased
    # toward *up_hint* — the caller's "which way is away" reference.  Default is
    # world-up (leaves seek the sun); when a leaf grows on the side or underside
    # of a surface the caller passes the outward surface normal instead, so the
    # crease/top faces away from that surface rather than into it.
    world_up = (_safe_norm(np.asarray(up_hint, float)) if up_hint is not None
                else np.array([0.0, 0.0, 1.0]))
    if abs(float(np.dot(L, world_up))) > 0.9:
        # Reference ~parallel to the growth axis → pick an independent fallback.
        world_up = (np.array([1.0, 0.0, 0.0])
                    if abs(float(np.dot(L, np.array([0.0, 0.0, 1.0])))) > 0.9
                    else np.array([0.0, 0.0, 1.0]))
    T0 = np.cross(world_up, L)
    T0 /= max(float(np.linalg.norm(T0)), 1e-10)
    N0 = np.cross(L, T0)
    N0 /= max(float(np.linalg.norm(N0)), 1e-10)

    # N is the up_hint component perpendicular to L (top/crease faces "away").
    T = T0
    N = N0

    fold_tan = float(np.tan(np.radians(fold_angle_deg)))
    N_S = _LEAF_N_LONG
    N_T = _LEAF_N_LAT
    n_rings = N_S - 1
    t_vals = np.linspace(-1.0, 1.0, N_T + 1)   # (N_T+1,)
    abs_t  = np.abs(t_vals)

    s_int = np.linspace(0.0, 1.0, N_S + 1)[1:-1]  # (n_rings,) interior stations

    # ── Vectorised vertex construction ──────────────────────────────────────
    # Compute all ring vertices in one numpy pass (no Python loop per ring).
    w_s    = width_mm * _leaf_width_profile(s_int)                    # (n_rings,)
    long_t = (s_int ** 0.5 * (1.0 - s_int) ** 1.5) / _LEAF_LONG_T_PEAK  # (n_rings,)

    # Midrib positions: (n_rings, 3)
    midribs = bp[np.newaxis, :] + (s_int[:, np.newaxis] * length_mm) * L[np.newaxis, :]

    # Lateral positions: (n_rings, N_T+1, 3)
    laterals = (midribs[:, np.newaxis, :]
                + (t_vals[np.newaxis, :, np.newaxis] * w_s[:, np.newaxis, np.newaxis])
                * T[np.newaxis, np.newaxis, :])

    # Vertical offsets (relative to N axis): (n_rings, N_T+1)
    tanh_t  = np.tanh(abs_t * _LEAF_CREASE_SHARPNESS)   # (N_T+1,)
    sin_t   = np.sin(np.pi * abs_t)                      # (N_T+1,)
    fold_h  = tanh_t[np.newaxis, :] * (w_s[:, np.newaxis] * fold_tan * long_t[:, np.newaxis])
    lobe_h  = (thickness_mm * sin_t[np.newaxis, :]) * long_t[:, np.newaxis]
    z_top   = fold_h + lobe_h   # (n_rings, N_T+1)
    z_bot   = fold_h            # (n_rings, N_T+1)

    # Surface points: (n_rings, N_T+1, 3)
    top_pts = laterals + z_top[:, :, np.newaxis] * N[np.newaxis, np.newaxis, :]
    bot_pts = laterals + z_bot[:, :, np.newaxis] * N[np.newaxis, np.newaxis, :]

    # Vertex array layout: top rings | bot rings | v_base | v_tip
    # Matches the index arithmetic in _build_blade_faces().
    verts = np.concatenate([
        top_pts.reshape(-1, 3),            # (n_rings*(N_T+1), 3)
        bot_pts.reshape(-1, 3),            # (n_rings*(N_T+1), 3)
        bp[np.newaxis, :],                 # v_base
        (bp + length_mm * L)[np.newaxis, :],  # v_tip
    ], axis=0)

    # Extract keel-input arrays from bot_pts (no extra loops needed).
    neg_t_edge_verts = bot_pts[:, 0,    :]  # (n_rings, 3)
    pos_t_edge_verts = bot_pts[:, N_T,  :]  # (n_rings, 3)
    half_width_list  = w_s.tolist()

    # ── Face connectivity ────────────────────────────────────────────────────
    # Canonical face array is built once per (N_S, N_T) topology and cached.
    faces = _build_blade_faces(N_S, N_T)

    # Blade topology depends only on the (constant) longitudinal/lateral
    # section counts, so all blades share one corrected-winding face array.
    mesh  = _mesh_with_fixed_normals(verts, faces, ("blade", N_S, N_T))

    parts: list[trimesh.Trimesh] = [mesh]

    if keel_depth_mm > 1e-6:
        keel = _build_leaf_keel_prism(
            base_vertex=bp,
            tip_vertex=(bp + length_mm * L),
            neg_t_edge=neg_t_edge_verts,   # already ndarray (n_rings, 3)
            pos_t_edge=pos_t_edge_verts,   # already ndarray (n_rings, 3)
            N=N,
            T=T,
            s_values=s_int,                # already ndarray
            half_widths=w_s,               # already ndarray
            keel_depth_mm=keel_depth_mm,
            keel_tip_angle_deg=keel_tip_angle_deg,
            all_bot_pts=bot_pts,           # curved closure — no flat underside
        )
        if len(keel.vertices) > 0:
            parts.append(keel)

    return parts
