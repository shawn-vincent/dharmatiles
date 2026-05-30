"""
Strict geometric intersection checker for placed grass blades.

Uses vectorised Möller–Trumbore segment-triangle tests on blade top-surface
quad strips to detect actual geometric overlaps between blade pairs, then
provides repair utilities that raise the z-floor at hit sites.
"""
from __future__ import annotations

import numpy as np


# ── Möller–Trumbore batch test ────────────────────────────────────────────────

def seg_tri_batch(P: np.ndarray, Q: np.ndarray,
                  A: np.ndarray, B: np.ndarray, C: np.ndarray,
                  eps: float = 1e-8) -> np.ndarray:
    """Vectorised Möller–Trumbore segment-triangle intersection test.

    Parameters
    ----------
    P, Q : (n, 3) — segment endpoints.
    A, B, C : (m, 3) — triangle vertices.

    Returns
    -------
    hit : bool (n, m) — True where segment i intersects triangle j.
    """
    D  = Q - P                                              # (n, 3)
    AB = B - A                                              # (m, 3)
    AC = C - A                                              # (m, 3)

    PV = np.cross(D[:, None, :], AC[None, :, :])           # (n, m, 3)
    DT = np.einsum('mk,nmk->nm', AB, PV)                   # (n, m)

    valid  = np.abs(DT) >= eps
    inv_DT = np.where(valid, 1.0 / np.where(valid, DT, 1.0), 0.0)

    TV = P[:, None, :] - A[None, :, :]                     # (n, m, 3)
    U  = np.einsum('nmk,nmk->nm', TV, PV) * inv_DT        # (n, m)
    QV = np.cross(TV, AB[None, :, :])                      # (n, m, 3)
    V  = np.einsum('nk,nmk->nm', D, QV) * inv_DT          # (n, m)
    T  = np.einsum('mk,nmk->nm', AC, QV) * inv_DT         # (n, m)

    return (valid &
            (U >= -eps) & (U <= 1.0 + eps) &
            (V >= -eps) & (U + V <= 1.0 + eps) &
            (T >= -eps) & (T <= 1.0 + eps))


# ── Blade top-surface intersection ────────────────────────────────────────────

def blade_top_intersections(spine_a, hw_a, up_a, spine_b, hw_b, up_b) -> list:
    """Geometric top-surface intersection test between two blades.

    The top surface of each blade is the strip of quads with vertices:
        V1[i] = spine[i] + hw[i] * up[i]   (right top edge)
        V2[i] = spine[i] − hw[i] * up[i]   (left  top edge)

    Tests each cross-edge of A against every top-face triangle of B, and
    vice versa.

    Returns
    -------
    list of (t_a, t_b) — normalised parameter values [0, 1] along each blade
    where their top surfaces intersect.
    """
    na = len(spine_a);  nb = len(spine_b)

    V1_A = spine_a + hw_a[:, None] * up_a
    V2_A = spine_a - hw_a[:, None] * up_a
    V1_B = spine_b + hw_b[:, None] * up_b
    V2_B = spine_b - hw_b[:, None] * up_b

    # 3-D bounding-box early exit
    a_lo = np.minimum(V1_A.min(axis=0), V2_A.min(axis=0))
    a_hi = np.maximum(V1_A.max(axis=0), V2_A.max(axis=0))
    b_lo = np.minimum(V1_B.min(axis=0), V2_B.min(axis=0))
    b_hi = np.maximum(V1_B.max(axis=0), V2_B.max(axis=0))
    if np.any(a_hi < b_lo) or np.any(b_hi < a_lo):
        return []

    t_scale_a = 1.0 / max(na - 1, 1)
    t_scale_b = 1.0 / max(nb - 1, 1)

    # Triangulate B's top strip: two tris per quad
    #   T1[j] = (V2_B[j], V1_B[j],   V1_B[j+1])
    #   T2[j] = (V2_B[j], V1_B[j+1], V2_B[j+1])
    tA_B = np.concatenate([V2_B[:-1], V2_B[:-1]], axis=0)
    tB_B = np.concatenate([V1_B[:-1], V1_B[1:]],  axis=0)
    tC_B = np.concatenate([V1_B[1:],  V2_B[1:]],  axis=0)
    qi_B = np.concatenate([np.arange(nb - 1), np.arange(nb - 1)])

    # Triangulate A's top strip
    tA_A = np.concatenate([V2_A[:-1], V2_A[:-1]], axis=0)
    tB_A = np.concatenate([V1_A[:-1], V1_A[1:]],  axis=0)
    tC_A = np.concatenate([V1_A[1:],  V2_A[1:]],  axis=0)
    qi_A = np.concatenate([np.arange(na - 1), np.arange(na - 1)])

    results: set = set()

    hit_AB = seg_tri_batch(V2_A, V1_A, tA_B, tB_B, tC_B)
    for ia, itri in zip(*np.where(hit_AB)):
        results.add((int(ia), int(qi_B[itri])))

    hit_BA = seg_tri_batch(V2_B, V1_B, tA_A, tB_A, tC_A)
    for ib, itri in zip(*np.where(hit_BA)):
        results.add((int(qi_A[itri]), int(ib)))

    return sorted(
        [(ia * t_scale_a, ib * t_scale_b) for ia, ib in results],
        key=lambda x: x[0],
    )


# ── Collection and repair ─────────────────────────────────────────────────────

def collect_strict_hits(spine, hw, up_locs, placed: list, strict_base_t: float) -> list:
    """Return all top-surface hits between *spine* and every blade in *placed*.

    Parameters
    ----------
    placed : list of (blade_idx, spine, hw, up_locs) tuples.
    strict_base_t : hits at t_a ≤ this are suppressed (blade eruption zone).

    Returns
    -------
    list of (prev_blade_idx, prev_spine, t_a, t_b)
    """
    hits_out = []
    for prev_idx, prev_spine, prev_hw, prev_up in placed:
        for t_a, t_b in blade_top_intersections(spine, hw, up_locs,
                                                 prev_spine, prev_hw, prev_up):
            if t_a > strict_base_t:
                hits_out.append((prev_idx, prev_spine, t_a, t_b))
    return hits_out


def log_strict_hits(blade_idx: int, base_x: float, base_y: float,
                    spine: np.ndarray, strict_hits: list,
                    max_report: int = 8) -> int:
    """Print intersection diagnostics to stdout.  Returns number of hits logged."""
    reported = 0
    for prev_idx, prev_spine, t_a, t_b in strict_hits:
        ia = round(t_a * (len(spine) - 1))
        ib = round(t_b * (len(prev_spine) - 1))
        print(
            f"  STRICT blade {blade_idx} (base {base_x:.1f},{base_y:.1f}) "
            f"t={t_a:.2f} ↔ blade {prev_idx} t={t_b:.2f} "
            f"@ ({spine[ia, 0]:.1f},{spine[ia, 1]:.1f})  "
            f"z_new={spine[ia, 2]:.2f}  z_old={prev_spine[ib, 2]:.2f}  "
            f"TOP-SURFACE geometric hit"
        )
        reported += 1
        if reported >= max_report:
            print("  STRICT   ... (more hits suppressed)")
            return reported
    return reported


def add_collision_repairs(repair_floor: np.ndarray, spine: np.ndarray,
                          strict_hits: list, clearance: float) -> None:
    """Raise *repair_floor* at hit sites to force the blade above collisions.

    Applies a 5-point weighted plateau (rather than a single-sample spike) to
    give the z-solver a smooth target rather than a discontinuity.
    """
    n = len(spine)
    for _prev_idx, prev_spine, t_a, t_b in strict_hits:
        ia = int(np.clip(round(t_a * (n - 1)), 0, n - 1))
        ib = int(np.clip(round(t_b * (len(prev_spine) - 1)), 0, len(prev_spine) - 1))
        required_z = float(max(prev_spine[ib, 2] + clearance,
                               spine[ia, 2] + clearance))
        delta = required_z - spine[ia, 2]
        for offset, weight in ((-2, 0.35), (-1, 0.70), (0, 1.0), (1, 0.70), (2, 0.35)):
            j = ia + offset
            if 0 <= j < n:
                repair_floor[j] = max(repair_floor[j], spine[j, 2] + delta * weight)
