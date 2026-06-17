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

# ── Constants ──────────────────────────────────────────────────────────────────

_LEAF_N_LONG           = 20    # longitudinal sections (base → tip)
_LEAF_N_LAT            = 16    # lateral sections across the leaf (must be even)
_LEAF_CREASE_SHARPNESS = 10.0  # tanh width of midrib crease (larger = narrower)
# Width profile normalisation: w(s) ∝ s^0.4 × (1-s)^0.8 peaks at s=1/3.
_LEAF_W_PEAK_NORM  = float((1.0 / 3.0) ** 0.4 * (2.0 / 3.0) ** 0.8)
# Thickness-along-length profile: t_long(s) ∝ s^0.5 × (1-s)^1.5, peak at s=0.25.
_LEAF_LONG_T_PEAK  = float(0.25 ** 0.5 * 0.75 ** 1.5)   # ≈ 0.3248


# ── Tiny shared helpers (self-contained so this module has no tree imports) ────

def _safe_norm(v: np.ndarray) -> np.ndarray:
    n = float(np.linalg.norm(v))
    return v / n if n > 1e-12 else v


def _hash01(*parts: object) -> float:
    """Deterministic float in [0, 1) from arbitrary hashable parts."""
    h = 1469598103934665603
    for part in parts:
        for byte in str(part).encode("utf-8"):
            h ^= byte
            h  = (h * 1099511628211) & 0xFFFFFFFFFFFFFFFF
        h ^= 0xFF
        h  = (h * 1099511628211) & 0xFFFFFFFFFFFFFFFF
    return h / float(2 ** 64)


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

    def _ridge_depth(s: float) -> float:
        x = (1.0 - s) * L_len
        if x >= R:
            return D
        return float(np.sqrt(max(0.0, R * R - (R - x) ** 2)))

    s_all = [0.0] + [float(s_values[i]) for i in range(n)] + [1.0]
    topP, topN, ridge = [], [], []
    for k, s in enumerate(s_all):
        if k == 0:
            tp = tn = base_vertex
        elif k == n + 1:
            tp = tn = tip_vertex
        else:
            tp, tn = pos_t_edge[k - 1], neg_t_edge[k - 1]
        mid = base_vertex + s * (tip_vertex - base_vertex)
        topP.append(tp); topN.append(tn)
        ridge.append(mid - _ridge_depth(s) * Nu)

    n_st = n + 2

    verts: list[np.ndarray] = []

    def _add(p: np.ndarray) -> int:
        verts.append(np.asarray(p, float))
        return len(verts) - 1

    iTP, iTN, iR = [], [], []
    for k in range(n_st):
        width_pinch = (k == 0 or k == n_st - 1)
        itp = _add(topP[k])
        iTP.append(itp)
        iTN.append(itp if width_pinch else _add(topN[k]))
        iR.append(itp if bool(np.allclose(ridge[k], topP[k])) else _add(ridge[k]))

    F: list[list[int]] = []

    def _quad(a: int, b: int, c: int, d: int) -> None:
        if a != b and b != c and a != c:
            F.append([a, b, c])
        if a != c and c != d and a != d:
            F.append([a, c, d])

    for k in range(n_st - 1):
        _quad(iTP[k], iTP[k + 1], iR[k + 1], iR[k])
        _quad(iR[k], iR[k + 1], iTN[k + 1], iTN[k])

    loop = ([iTP[k] for k in range(n_st)] +
            [iTN[k] for k in range(n_st - 2, 0, -1)])
    c_top = _add(np.mean([verts[i] for i in loop], axis=0))
    for a in range(len(loop)):
        b = (a + 1) % len(loop)
        F.append([c_top, loop[a], loop[b]])

    mesh = trimesh.Trimesh(
        vertices=np.array(verts),
        faces=np.array(F, dtype=np.int32),
        process=False,
    )
    mesh.fix_normals()
    return mesh


# ── Public API ─────────────────────────────────────────────────────────────────

def build_leaf_mesh(
    *,
    base_pos: np.ndarray,
    tangent: np.ndarray,
    length_mm: float,
    width_mm: float,
    thickness_mm: float = 0.24,
    fold_angle_deg: float = 5.0,
    keel_depth_mm: float = 1.0,
    keel_tip_angle_deg: float = 45.0,
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
    fold_angle_deg  : midrib crease V-angle.  Default 5.0.
    keel_depth_mm   : maximum depth of the structural keel on the underside.
                      Pass 0 to omit the keel.  Default 1.0.
    keel_tip_angle_deg : reserved for future use.  Default 45.0.
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

    # Orthonormal frame (L, T, N) with N biased toward world-up.
    world_up = np.array([0.0, 0.0, 1.0])
    if abs(float(np.dot(L, world_up))) > 0.9:
        world_up = np.array([1.0, 0.0, 0.0])
    T0 = np.cross(world_up, L)
    T0 /= max(float(np.linalg.norm(T0)), 1e-10)
    N0 = np.cross(L, T0)
    N0 /= max(float(np.linalg.norm(N0)), 1e-10)

    # Deterministic random roll around the branch tangent.
    roll  = _hash01(seed, "leaf-roll") * 2.0 * np.pi
    cr, sr = float(np.cos(roll)), float(np.sin(roll))
    T = cr * T0 + sr * N0
    N = -sr * T0 + cr * N0

    fold_tan = float(np.tan(np.radians(fold_angle_deg)))
    N_S = _LEAF_N_LONG
    N_T = _LEAF_N_LAT
    t_vals = np.linspace(-1.0, 1.0, N_T + 1)
    abs_t  = np.abs(t_vals)

    s_interior = np.linspace(0.0, 1.0, N_S + 1)[1:-1]

    verts_acc: list[np.ndarray] = []
    faces_acc: list[list[int]]  = []
    n_v = 0

    def _add(arr: np.ndarray) -> int:
        nonlocal n_v
        off = n_v
        a = arr.reshape(-1, 3)
        verts_acc.append(a)
        n_v += len(a)
        return off

    ring_top: list[int] = []
    ring_bot: list[int] = []
    neg_t_edge_verts: list[np.ndarray] = []
    pos_t_edge_verts: list[np.ndarray] = []
    half_width_list:  list[float]      = []

    for s in s_interior:
        w_s    = width_mm * float(_leaf_width_profile(np.array([s]))[0])
        long_t = float((s ** 0.5) * ((1.0 - s) ** 1.5)) / _LEAF_LONG_T_PEAK

        midrib  = bp + s * length_mm * L
        lateral = midrib[np.newaxis, :] + t_vals[:, np.newaxis] * w_s * T

        fold_h  = np.tanh(abs_t * _LEAF_CREASE_SHARPNESS) * w_s * fold_tan * long_t
        lobe_h  = thickness_mm * np.sin(np.pi * abs_t) * long_t

        z_top = fold_h + lobe_h
        z_bot = fold_h

        top_pts = lateral + z_top[:, np.newaxis] * N
        bot_pts = lateral + z_bot[:, np.newaxis] * N

        ring_top.append(_add(top_pts))
        ring_bot.append(_add(bot_pts))

        neg_t_edge_verts.append(bot_pts[0].copy())
        pos_t_edge_verts.append(bot_pts[N_T].copy())
        half_width_list.append(w_s)

    v_base = _add(bp[np.newaxis])
    v_tip  = _add((bp + length_mm * L)[np.newaxis])
    n_rings = len(s_interior)

    # Base fan
    ft0, fb0 = ring_top[0], ring_bot[0]
    for j in range(N_T):
        j1 = j + 1
        faces_acc.append([v_base, ft0 + j,  ft0 + j1])
        faces_acc.append([v_base, fb0 + j1, fb0 + j ])
    faces_acc.append([v_base, fb0,        ft0       ])
    faces_acc.append([v_base, ft0 + N_T, fb0 + N_T ])

    # Body bands
    for ri in range(n_rings - 1):
        ta, tb = ring_top[ri], ring_top[ri + 1]
        ba, bb = ring_bot[ri], ring_bot[ri + 1]
        for j in range(N_T):
            j1 = j + 1
            faces_acc.append([ta + j,  tb + j,  tb + j1])
            faces_acc.append([ta + j,  tb + j1, ta + j1])
            faces_acc.append([ba + j,  ba + j1, bb + j1])
            faces_acc.append([ba + j,  bb + j1, bb + j ])
        faces_acc.append([ta,       ba,       bb      ])
        faces_acc.append([ta,       bb,       tb      ])
        faces_acc.append([ta + N_T, tb + N_T, bb + N_T])
        faces_acc.append([ta + N_T, bb + N_T, ba + N_T])

    # Tip fan
    ftL, fbL = ring_top[-1], ring_bot[-1]
    for j in range(N_T):
        j1 = j + 1
        faces_acc.append([v_tip, ftL + j1, ftL + j ])
        faces_acc.append([v_tip, fbL + j,  fbL + j1])
    faces_acc.append([v_tip, ftL,        fbL       ])
    faces_acc.append([v_tip, fbL + N_T, ftL + N_T ])

    verts = np.vstack(verts_acc)
    faces = np.array(faces_acc, dtype=np.int32)
    mesh  = trimesh.Trimesh(vertices=verts, faces=faces, process=False)
    mesh.fix_normals()

    parts: list[trimesh.Trimesh] = [mesh]

    if keel_depth_mm > 1e-6:
        keel = _build_leaf_keel_prism(
            base_vertex=bp,
            tip_vertex=(bp + length_mm * L),
            neg_t_edge=np.array(neg_t_edge_verts),
            pos_t_edge=np.array(pos_t_edge_verts),
            N=N,
            T=T,
            s_values=s_interior,
            half_widths=np.array(half_width_list),
            keel_depth_mm=keel_depth_mm,
            keel_tip_angle_deg=keel_tip_angle_deg,
        )
        if len(keel.vertices) > 0:
            parts.append(keel)

    return parts
