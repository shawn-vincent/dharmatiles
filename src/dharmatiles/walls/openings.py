"""
Openings — doors, windows, oculi, floor hatches and wells.

Design: docs/design/walls-doors.md (rev 2).  One concept: an
``Opening`` is a closed 2D profile in the wall's (t, z) plane —
(run-position, height) on a standing wall, (run, plan-depth) on a
``laid_flat`` one.  The layout EXCLUDES cells inside the profile
(trimmed ends become textured 'face', the bond flows around), and the
boundary is LINED with surround units by one rule:

    near-vertical boundary  →  jamb blocks (quoin-style stacks)
    everything else         →  radial VOUSSOIRS (units rotated to the
                               local normal), keystone at the apex

An arch is therefore a special case; a circle (no verticals) becomes
a full voussoir ring — an oculus in a wall, a well on a floor; a
custom polygon gets the generic treatment.  ``head='lintel'``
replaces the arc with one spanning block.
"""
from __future__ import annotations

from dataclasses import dataclass

import numpy as np

_ARC_SAMPLES = 48       # profile arc tessellation


@dataclass
class Opening:
    """A door (sill 0), window (sill > 0), or — on a laid_flat wall —
    a hatch/well.  ``at`` is the centre along the spine in SQUARES.

    On a standing wall the profile lives in (run-position, height):
    ``sill_mm``/``head_mm`` are heights above the wall seat.  On a
    ``laid_flat`` wall (a floor) the same profile lies in the
    pavement plane: ``at`` stays the run position and ``sill_mm``/
    ``head_mm`` become the PLAN-DEPTH extent of the hatch, measured
    from the spine toward the wall's body side."""
    at:        float
    width_mm:  float = 22.0
    sill_mm:   float = 0.0
    head_mm:   float = 36.0          # may exceed the wall height
    head:      str   = 'arch'        # 'arch' | 'lintel'
    rise_mm:   float | None = None   # None = semicircular
    profile:   object = 'auto'       # 'auto' | 'circle' | [(t_rel, z), …]
    #: 'jambs' = the classic door construction (quoin-style jamb
    #: stacks + arch/lintel head + sill slab); 'ring' = line the
    #: profile with a FRAME of small units — the circle's voussoir
    #: ring generalized to a rectangle (rows of small bricks + square
    #: corner blocks).  'auto': hatches in a pavement (laid_flat,
    #: lintel head) take 'ring'; standing walls take 'jambs'.
    surround:  str   = 'auto'        # 'auto' | 'jambs' | 'ring'
    leaf:      object = None         # Leaf | None
    slot:      bool  = False         # O6 (not yet implemented)

    def __post_init__(self):
        if self.head not in ('arch', 'lintel'):
            raise ValueError(f"Opening.head must be 'arch' or 'lintel', "
                             f'got {self.head!r}')
        if self.surround not in ('auto', 'jambs', 'ring'):
            raise ValueError(f"Opening.surround must be 'auto', 'jambs' "
                             f"or 'ring', got {self.surround!r}")
        if not (self.profile == 'auto' or self.profile == 'circle'
                or isinstance(self.profile, (list, tuple, np.ndarray))):
            raise ValueError(f'Opening.profile must be \'auto\', '
                             f'\'circle\' or a point sequence, '
                             f'got {self.profile!r}')
        if self.width_mm <= 0.0:
            raise ValueError('Opening.width_mm must be positive')
        if self.head_mm <= self.sill_mm:
            raise ValueError('Opening.head_mm must exceed sill_mm')
        if self.surround == 'ring' and not (
                self.profile == 'auto' and self.head == 'lintel'):
            raise ValueError(
                "surround='ring' requires profile='auto' with "
                "head='lintel' (a circle/custom profile already gets "
                'a full voussoir ring)')
        if self.slot:
            raise NotImplementedError(
                'Opening.slot is the O6 slot system — not implemented '
                'yet (design: docs/design/walls-doors.md)')


def arch_arc(op: Opening, tc: float) -> np.ndarray:
    """The head arc polyline in absolute (t, z), right → left (CCW):
    a segmental arc through the springings (±w2, z_spring) with apex
    (tc, head) — circle of radius R = (w2² + rise²) / (2·rise)."""
    w2 = op.width_mm / 2.0
    rise = min(op.rise_mm if op.rise_mm is not None else w2,
               w2 * 1.0 + 1e-9, op.head_mm - op.sill_mm)
    z_spring = op.head_mm - rise
    R = (w2 * w2 + rise * rise) / (2.0 * rise)
    zc = op.head_mm - R
    a1 = np.arctan2(z_spring - zc, +w2)
    a0 = np.arctan2(z_spring - zc, -w2)
    a = np.linspace(a1, a0, _ARC_SAMPLES // 2)
    return np.column_stack([tc + R * np.cos(a), zc + R * np.sin(a)])


def build_profile(op: Opening, tc: float) -> np.ndarray:
    """Closed CCW polygon (N, 2) in absolute (t, z) for the opening."""
    w2 = op.width_mm / 2.0
    if op.profile == 'circle':
        r = min(op.width_mm, op.head_mm - op.sill_mm) / 2.0
        zc = (op.sill_mm + op.head_mm) / 2.0
        a = np.linspace(0.0, 2.0 * np.pi, _ARC_SAMPLES, endpoint=False)
        return np.column_stack([tc + r * np.cos(a), zc + r * np.sin(a)])
    if isinstance(op.profile, (list, tuple, np.ndarray)):
        P = np.asarray(op.profile, dtype=float)
        P = P + np.array([tc, 0.0])
        # ensure CCW
        Q = np.roll(P, -1, axis=0)
        area = float((P[:, 0] * Q[:, 1] - P[:, 1] * Q[:, 0]).sum())
        return P if area > 0 else P[::-1]
    # 'auto': rectangle with arch or flat head
    if op.head == 'lintel':
        return np.array([[tc - w2, op.sill_mm], [tc + w2, op.sill_mm],
                         [tc + w2, op.head_mm], [tc - w2, op.head_mm]])
    return np.vstack([
        np.array([[tc - w2, op.sill_mm], [tc + w2, op.sill_mm]]),
        arch_arc(op, tc),
    ])


def band_extent(P: np.ndarray, z0: float, z1: float):
    """Horizontal extent (t_lo, t_hi) of the polygon within the z-band,
    or None if it does not reach the band."""
    ts = []
    N = len(P)
    for i in range(N):
        a, b = P[i], P[(i + 1) % N]
        lo, hi = min(a[1], b[1]), max(a[1], b[1])
        if hi < z0 or lo > z1:
            continue
        for p in (a, b):
            if z0 <= p[1] <= z1:
                ts.append(p[0])
        for zc in (z0, z1):
            if lo <= zc <= hi and abs(b[1] - a[1]) > 1e-9:
                u = (zc - a[1]) / (b[1] - a[1])
                if 0.0 <= u <= 1.0:
                    ts.append(a[0] + u * (b[0] - a[0]))
    if not ts:
        return None
    return min(ts), max(ts)


def boundary_units(P: np.ndarray, spacing: float, *,
                   closed: bool = True, offset: float = 0.0,
                   force_odd: bool = False):
    """Sample a boundary polyline into surround-unit poses.

    The units are placed on the curve OFFSET outward by ``offset``
    (the ring centreline) and the pitch is measured THERE — sizing by
    the inner boundary leaves radial wedge gaps that grow with the
    ring depth.  ``closed`` walks the polygon loop (oculus / well /
    custom profile); open walks end-to-end (an arch's arc, so the end
    units land exactly at the springings).  ``force_odd`` guarantees a
    single apex unit — the keystone.

    Returns [(centre(t,z), normal(t,z), angle, arclen_here, dtheta), …]
    where angle rotates the block so its height axis lies along the
    outward normal and dtheta is the normal's swing across the unit's
    span — the wedge angle a rectangular unit can't cover (radial
    joints gape at the outer radius; the caller tapers the unit by
    dtheta × ring depth)."""
    P = np.asarray(P, dtype=float)
    N = len(P)
    n_edges = N if closed else N - 1
    a = P
    b = np.roll(P, -1, axis=0) if closed else P[1:]
    ev = (b - a[:n_edges])
    eL = np.hypot(ev[:, 0], ev[:, 1])
    good = eL > 1e-9
    en = np.zeros_like(ev)
    en[good] = np.column_stack([ev[good, 1], -ev[good, 0]]) \
        / eL[good, None]                                # CCW → outward
    # vertex normals: average of adjacent edge normals (ends of an
    # open curve take their single edge's normal)
    vn = np.zeros_like(P)
    for i in range(n_edges):
        vn[i] += en[i]
        vn[(i + 1) % N if closed else i + 1] += en[i]
    vL = np.hypot(vn[:, 0], vn[:, 1])
    vn[vL > 1e-9] /= vL[vL > 1e-9, None]
    Q = P + offset * vn
    qa = Q
    qb = np.roll(Q, -1, axis=0) if closed else Q[1:]
    qv = qb - qa[:n_edges]
    qL = np.hypot(qv[:, 0], qv[:, 1])
    s0 = np.concatenate([[0.0], np.cumsum(qL)])
    total = float(s0[-1])
    n_units = max(int(round(total / spacing)), 3 if closed else 1)
    if force_odd and n_units % 2 == 0:
        n_units += 1
    step = total / n_units
    def _normal_at(s: float) -> np.ndarray:
        i = min(int(np.searchsorted(s0[1:], s + 1e-9)), n_edges - 1)
        return en[i]

    out = []
    for k in range(n_units):
        s = (k + 0.5) * step
        i = min(int(np.searchsorted(s0[1:], s + 1e-9)), n_edges - 1)
        u = (s - s0[i]) / max(qL[i], 1e-9)
        p = qa[i] + u * qv[i]
        n = en[i]
        ang = float(np.arctan2(n[0], n[1]))
        na = _normal_at((s - 0.5 * step) % total if closed
                        else max(s - 0.5 * step, 0.0))
        nb = _normal_at((s + 0.5 * step) % total if closed
                        else min(s + 0.5 * step, total - 1e-9))
        dth = float(np.arccos(np.clip(np.dot(na, nb), -1.0, 1.0)))
        out.append((p, n, ang, step, dth))
    return out


def point_inside(P: np.ndarray, t: float, z: float) -> bool:
    """Ray-cast point-in-polygon (for keeping hearting chips out of
    the passage)."""
    inside = False
    N = len(P)
    for i in range(N):
        a, b = P[i], P[(i + 1) % N]
        if (a[1] > z) != (b[1] > z):
            x = a[0] + (z - a[1]) / (b[1] - a[1]) * (b[0] - a[0])
            if x > t:
                inside = not inside
    return inside
