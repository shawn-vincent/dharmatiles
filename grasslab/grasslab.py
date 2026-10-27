#!/usr/bin/env python3
"""grasslab v1 — swath → frond → groove sculpted grass on a DungeonBlocks tile.

Implements grasslab/DESIGN.md rev 2:

  H0 hills ─► θ field ─► swath partition (warped anisotropic Voronoi,
  wave envelope per cell) ─► fronds riding their swath's envelope:
  MASS (height field, soft-max) + LIPS (45°-chamfered column intervals)
  ─► continuous field F = max(T − z, lips) ─► FDM backstop ─► marching
  cubes ─► ∪ DB base ─► simplify ─► STL

Usage:
    python grasslab/grasslab.py                  # 2x2 with hills, seed 1
    python grasslab/grasslab.py --cols 1 --rows 1 --flat   # S1 swath study
    python grasslab/grasslab.py --seed 3 --check out.png   # + self-check PNG
"""
from __future__ import annotations

import argparse
import pathlib
import time

import numpy as np
from scipy import ndimage

# ── Parameters (DESIGN.md §3) ────────────────────────────────────────────────
SQUARE_MM            = 35.0

# hills (unchanged from v0)
HILL_RELIEF_MM       = 7.0
GROUND_EDGE_MM       = 3.4
HILL_SIGMA_LARGE_MM  = 10.0
HILL_SIGMA_MID_MM    = 5.0
HILL_MID_WEIGHT      = 0.35
MAX_GROUND_SLOPE_DEG = 34.0
USABLE_SLOPE_DEG     = 12.0
EDGE_MARGIN_MM       = 9.0
HILL_PLAIN_FRAC      = 0.4         # share of the interior left at perimeter level
HILL_PLAIN_SOFT      = 0.25        # softness of the plain→hill toe (× noise std)

# direction field
COHERENCE_MM         = 15.0
THETA_STD_DEG        = 60.0
TERRAIN_FOLLOW       = 0.3

# swaths
SWATH_SPACING_MM     = 11.0
SWATH_LATERAL_ASPECT = 0.65       # lateral distances scaled by 1/aspect
SWATH_WARP_MM        = 2.5
SWATH_WARP_SIGMA_MM  = 3.0
SWATH_AMP_MM         = (1.2, 2.2)
ENV_FLOOR            = 0.25
ENV_EXP              = 1.6
ENV_LATERAL_EXP      = 0.6

# fronds
FROND_LEN_MM         = (5.5, 9.5)
FROND_HALF_W_MM      = (0.55, 0.9)
FROND_H_MM           = (0.65, 1.0)
FROND_TIP_HALF_W_MM  = 0.45
# flow v2: meanders (REQ-FL-2)
MEANDER_WAVELENGTH_MM = 15.0
MEANDER_AMP_DEG      = (10.0, 30.0)
MEANDER_AMP_SIGMA_MM = 12.0
MEANDER_PHASE_SIGMA_MM = 14.0

# strands of streamline fronds (REQ-FL-1/3)
STRAND_SPACING_MM    = 2.65
STRAND_LEN_MM        = (15.0, 30.0)
STRAND_MARGIN_MM     = 8.0
STRAND_GAP_FRAC      = 0.5         # roots every this × frond length along a strand
STRAND_LATERAL_MM    = 0.3
FROND_DELTA_DEG      = 4.0
STRAY_FRAC           = 0.05
STRAY_DELTA_DEG      = (20.0, 35.0)
TRACE_STEP_MM        = 0.25
DOME_EXP             = 0.9
DROOP                = 0.35     # tip sinks to (1 − DROOP) of the frond crest
LIP_MAX_MM           = 2.0
LIP_FRAC             = 0.35
LIP_MIN_T_MM         = 0.6
SOFTMAX_K_MM         = 0.12

# bare soil: lens openings the flow wraps around (REQ-FL-4)
SOIL_LENSES_PER_SQ   = (1.3, 1.8)
SOIL_LENS_HALF_LEN_MM = (5.0, 9.0)
SOIL_LENS_ASPECT     = (0.45, 0.65) # half-width / half-length
SOIL_LENS_SEP_MM     = 13.0
SOIL_LENS_WARP_MM    = 0.8
SOIL_EDGE_MM         = 0.5
SOIL_RIM_KEEP_MM     = 7.0         # lens centres at least this far from the rim
DEFLECT_BAND_MM      = (1.5, 4.5)  # per lens: flow bends around it within this band
DEFLECT_STRENGTH     = (0.55, 1.0) # per lens: how strongly the flow hugs it

# bank piles (REQ-FL-6, DESIGN §2.4d)
BANK_REACH_MM        = 4.0         # fronds rooted this close to soil are bank fronds
BANK_FACE_MM         = 6.0         # columns this close to soil can be soil-facing
BANK_WANDER_DEG      = 15.0        # extra heading wander for bank fronds
BANK_STRAY_FRAC      = 0.12
BANK_H_SCALE         = (0.65, 1.15)
SIDE_LIP_W_MM        = 0.7         # side ledge width (blade edge overhang)
STEER_SIDE_SIGMA_MM  = 14.0         # rim steering: smoothing of the turn-side preference
STEER_SMOOTH_MM      = 1.5         # rim steering: smoothing of the rotation field
SOIL_DIP_MM          = 0.4         # soil sits this far below H0
SOIL_DIP_SOFT_MM     = 0.8         # blur of the dip/roughness mask
SOIL_ROUGH_MM        = 0.08
SOIL_SPILL_MM        = (0.8, 3.2)  # per-frond spill into soil, 0.8 + 2.4·U²
WISP_FRAC            = 0.04        # bank fronds allowed WISP_SPILL_MM over soil
WISP_SPILL_MM        = 4.0
PEBBLE_DENSITY       = 0.06        # per mm² of bare soil
PEBBLE_R_MM          = (0.45, 1.2) # skewed small
PEBBLE_GAP_MM        = 1.2
SPRIG_PROB           = 0.10        # chance a soil-rooted frond becomes a sprig
SPRIG_LEN_MM         = (1.8, 3.0)

# frond-made edges (REQUIREMENTS §2.3b)
SKIRT_FACTOR         = 2.2         # skirt half-width × blade half-width
SKIRT_BASE_CAP       = 0.6         # skirt extends this × skirt width behind the root
SKIRT_TIP_DROP       = 0.45        # skirt height falls by this toward the tip
SKIRT_END_MM         = 2.2         # bank fronds: skirt descends to ~0 over this tip length
SOIL_BASE_SOFT_MM    = 2.5         # fronds rooted this close to soil get a ramped base
FROND_MIN_LEN_MM     = 1.5

# edge flow (REQUIREMENTS REQ-EDGE-7, DESIGN §2.4c)
EDGE_FLOW_MM         = (9.0, 15.0) # per-frond steering ramp distance from the rim
EDGE_OK_DEG          = (70.0, 88.0)# per-frond allowed angle from 'at the edge'
EDGE_PULL            = (0.15, 0.45)# per-frond extra lean toward the edge at the rim
RIM_TIP_INSET_MM     = (0.2, 1.5)  # per-frond tip inset from the wall (skewed small)
RIM_ALONG_MM         = 3.0         # fronds rooted this close to a wall run alongside it

SIMPLIFY_TOL_MM      = 0.015
OVERHANG_DEG         = 45.0
DB_GRASS_BAND        = (4.4, 5.9)
MARGIN_MM            = 10.0       # sites/roots beyond the tile so edges are covered


# ── Helpers ──────────────────────────────────────────────────────────────────
def _smooth_noise(rng, shape, sigma_px):
    n = ndimage.gaussian_filter(rng.standard_normal(shape), sigma_px, mode='wrap')
    return (n - n.mean()) / (n.std() + 1e-12)


class Field2D:
    def __init__(self, data, x0, y0, res):
        self.data, self.x0, self.y0, self.res = data, x0, y0, res

    def __call__(self, x, y):
        ix = (np.asarray(x, float) - self.x0) / self.res
        iy = (np.asarray(y, float) - self.y0) / self.res
        shp = np.shape(ix)
        out = ndimage.map_coordinates(self.data, [np.ravel(ix), np.ravel(iy)],
                                      order=1, mode='nearest')
        return out.reshape(shp)


def poisson_sites(rng, x0, y0, x1, y1, r):
    tries = int(30 * (x1 - x0) * (y1 - y0) / (r * r))
    cell = r / np.sqrt(2)
    gw, gh = int((x1 - x0) / cell) + 1, int((y1 - y0) / cell) + 1
    grid = -np.ones((gw, gh), dtype=int)
    pts = []
    for p in rng.uniform([x0, y0], [x1, y1], size=(tries, 2)):
        gi, gj = int((p[0] - x0) / cell), int((p[1] - y0) / cell)
        ok = True
        for i in range(max(gi - 2, 0), min(gi + 3, gw)):
            for j in range(max(gj - 2, 0), min(gj + 3, gh)):
                k = grid[i, j]
                if k >= 0 and (pts[k][0] - p[0]) ** 2 + (pts[k][1] - p[1]) ** 2 < r * r:
                    ok = False
                    break
            if not ok:
                break
        if ok:
            grid[gi, gj] = len(pts)
            pts.append(p)
    return np.array(pts)


def smoothmax(a, b, k):
    return np.maximum(a, b) + k * np.log1p(np.exp(-np.abs(a - b) / k))


# ── Hills H0 ─────────────────────────────────────────────────────────────────
def make_ground(rng, W, H, res, flat):
    m = MARGIN_MM + 2.0
    nx, ny = int(np.ceil((W + 2 * m) / res)) + 1, int(np.ceil((H + 2 * m) / res)) + 1
    raw = (_smooth_noise(rng, (nx, ny), HILL_SIGMA_LARGE_MM / res)
           + HILL_MID_WEIGHT * _smooth_noise(rng, (nx, ny), HILL_SIGMA_MID_MM / res))
    if flat:
        return Field2D(np.full((nx, ny), GROUND_EDGE_MM), -m, -m, res), {}
    xs, ys = -m + np.arange(nx) * res, -m + np.arange(ny) * res
    X, Y = np.meshgrid(xs, ys, indexing='ij')
    inside = (X >= 0) & (X <= W) & (Y >= 0) & (Y <= H)
    # Rises only: the perimeter level is the lowest ground anywhere.  Low
    # ground is softly floored to it (flat plains at edge height) and hills
    # rise from those plains; ~HILL_PLAIN_FRAC of the interior stays plain.
    raw = raw - np.quantile(raw[inside], HILL_PLAIN_FRAC)
    sd = raw[inside].std()
    raw = smoothmax(raw, np.zeros_like(raw), HILL_PLAIN_SOFT * sd) - HILL_PLAIN_SOFT * sd * np.log(2)
    raw = np.maximum(raw, 0.0)
    raw *= HILL_RELIEF_MM / max(raw[inside].max(), 1e-9)
    dist = np.minimum.reduce([X, W - X, Y, H - Y])
    wgt = 0.5 - 0.5 * np.cos(np.pi * np.clip(dist / EDGE_MARGIN_MM, 0, 1))
    h = GROUND_EDGE_MM + wgt * raw
    gx, gy = np.gradient(h, res)
    slope = np.hypot(gx, gy)
    smax = np.tan(np.radians(MAX_GROUND_SLOPE_DEG))
    if slope[inside].max() > smax:
        h = GROUND_EDGE_MM + (h - GROUND_EDGE_MM) * smax / slope[inside].max()
        gx, gy = np.gradient(h, res)
        slope = np.hypot(gx, gy)
    stats = dict(ground_min=float(h[inside].min()), ground_max=float(h[inside].max()),
                 max_slope_deg=float(np.degrees(np.arctan(slope[inside].max()))),
                 usable=float((slope[inside] <= np.tan(np.radians(USABLE_SLOPE_DEG))).mean()))
    return Field2D(h, -m, -m, res), stats


# ── Direction field ──────────────────────────────────────────────────────────
class Flow:
    """Direction field on the ground's extended grid (unit vectors vx, vy)."""

    def __init__(self, vx, vy, x0, y0, res):
        n = np.maximum(np.hypot(vx, vy), 1e-9)
        self.vx, self.vy = vx / n, vy / n
        self.x0, self.y0, self.res = x0, y0, res
        self.ux = Field2D(self.vx, x0, y0, res)
        self.uy = Field2D(self.vy, x0, y0, res)

    def grid(self):
        nx, ny = self.vx.shape
        return np.meshgrid(self.x0 + np.arange(nx) * self.res,
                           self.y0 + np.arange(ny) * self.res, indexing='ij')


def _along_flow_coord(th, res):
    """Scalar s with ∇s ≈ (cos θ, sin θ): least-squares (FFT Poisson) fit.

    Lines of constant s cross the flow, so sin(2π s/λ) makes neighbouring
    streamlines wave in phase.
    """
    cx, cy = np.cos(th), np.sin(th)
    div = np.gradient(cx, res, axis=0) + np.gradient(cy, res, axis=1)
    # Even (mirror) extension keeps the solve well-behaved at the borders.
    D = np.concatenate([div, div[::-1]], 0)
    D = np.concatenate([D, D[:, ::-1]], 1)
    kx = 2 * np.pi * np.fft.fftfreq(D.shape[0], res)
    ky = 2 * np.pi * np.fft.fftfreq(D.shape[1], res)
    K2 = kx[:, None] ** 2 + ky[None, :] ** 2
    K2[0, 0] = 1.0
    S = np.fft.fft2(D) / -K2
    S[0, 0] = 0
    return np.real(np.fft.ifft2(S))[:th.shape[0], :th.shape[1]]


def make_direction(rng, ground):
    """Regional angle + in-phase meanders ⊕ bounded downhill pull (REQ-FL-2).

    Everything up to here is a scalar angle (no singularities); obstacle
    partings and rim steering are added in finish_flow().
    """
    h, res = ground.data, ground.res
    th_r = rng.uniform(0, 2 * np.pi) + np.radians(THETA_STD_DEG) * \
        _smooth_noise(rng, h.shape, COHERENCE_MM / res)
    s_al = _along_flow_coord(th_r, res)
    lo, hi = np.radians(MEANDER_AMP_DEG[0]), np.radians(MEANDER_AMP_DEG[1])
    amp = lo + (hi - lo) * _smooth01(0.5 + 0.35 * _smooth_noise(
        rng, h.shape, MEANDER_AMP_SIGMA_MM / res))
    phase = np.pi * _smooth_noise(rng, h.shape, MEANDER_PHASE_SIGMA_MM / res)
    th = th_r + amp * np.sin(2 * np.pi * s_al / MEANDER_WAVELENGTH_MM + phase)
    gx, gy = np.gradient(h, res)
    slope = np.hypot(gx, gy)
    a = TERRAIN_FOLLOW * np.clip(slope / np.tan(np.radians(MAX_GROUND_SLOPE_DEG)), 0, 1)
    safe = np.maximum(slope, 1e-9)
    vx = (1 - a) * np.cos(th) + a * np.where(slope > 1e-9, -gx / safe, 0)
    vy = (1 - a) * np.sin(th) + a * np.where(slope > 1e-9, -gy / safe, 0)
    return Flow(vx, vy, ground.x0, ground.y0, res)


# ── Swath partition ──────────────────────────────────────────────────────────
class Swaths:
    """Warped anisotropic Voronoi cells, each with its own flow axis.

    Every point has exactly one owner → total coverage by construction.
    """

    def __init__(self, rng, W, H, ux, uy, res=0.25):
        m = MARGIN_MM
        self.sites = poisson_sites(rng, -m, -m, W + m, H + m, SWATH_SPACING_MM)
        n = len(self.sites)
        dx, dy = ux(self.sites[:, 0], self.sites[:, 1]), uy(self.sites[:, 0], self.sites[:, 1])
        nd = np.hypot(dx, dy)
        self.d = np.stack([dx / nd, dy / nd], axis=1)
        self.p = np.stack([-self.d[:, 1], self.d[:, 0]], axis=1)
        self.amp = rng.uniform(*SWATH_AMP_MM, size=n)
        nx, ny = int((W + 2 * m) / res) + 1, int((H + 2 * m) / res) + 1
        self.warp = Field2D(SWATH_WARP_MM * _smooth_noise(
            rng, (nx, ny), SWATH_WARP_SIGMA_MM / res), -m, -m, res)
        # Site-dependent warp phase: a common warp would shift every site's
        # distance equally and bend nothing.
        self.phase = rng.uniform(-1, 1, size=n)
        # Per-cell normalisation from a coarse extended grid.
        xs, ys = -m + np.arange(nx) * res, -m + np.arange(ny) * res
        X, Y = np.meshgrid(xs, ys, indexing='ij')
        own, a, b = self.assign(X.ravel(), Y.ravel())
        self.a_min, self.a_max, self.b_max = np.zeros(n), np.ones(n), np.ones(n)
        for i in range(n):
            sel = own == i
            if sel.any():
                self.a_min[i], self.a_max[i] = a[sel].min(), a[sel].max()
                self.b_max[i] = max(np.abs(b[sel]).max(), 0.5)
        self.len = np.maximum(self.a_max - self.a_min, 1.0)

    def assign(self, x, y, chunk=100000):
        x, y = np.asarray(x, float), np.asarray(y, float)
        own = np.empty(len(x), dtype=int)
        a, b = np.empty(len(x)), np.empty(len(x))
        wp = self.warp(x, y)
        for c0 in range(0, len(x), chunk):
            sl = slice(c0, c0 + chunk)
            dx = x[sl, None] - self.sites[None, :, 0]
            dy = y[sl, None] - self.sites[None, :, 1]
            aa = dx * self.d[None, :, 0] + dy * self.d[None, :, 1]
            bb = dx * self.p[None, :, 0] + dy * self.p[None, :, 1]
            dist = np.hypot(aa, bb / SWATH_LATERAL_ASPECT) + wp[sl, None] * self.phase[None, :]
            k = dist.argmin(axis=1)
            r = np.arange(len(k))
            own[sl], a[sl], b[sl] = k, aa[r, k], bb[r, k]
        return own, a, b

    def coords(self, own, a, b):
        s = np.clip((a - self.a_min[own]) / self.len[own], 0, 1)
        l = np.clip(b / self.b_max[own], -1, 1)
        return s, l

    def env(self, own, s, l):
        """Wave envelope: low upstream and at the sides, crest downstream-centre."""
        return (self.amp[own] * (ENV_FLOOR + (1 - ENV_FLOOR) * s ** ENV_EXP)
                * (1 - l ** 2) ** ENV_LATERAL_EXP)


# ── Grass build ──────────────────────────────────────────────────────────────
class Lenses:
    """Eye-shaped bare-soil openings aligned with the local flow (REQ-FL-4)."""

    def __init__(self, rng, ground, flow, W, H):
        n_sq = (W / SQUARE_MM) * (H / SQUARE_MM)
        n = max(1, int(round(rng.uniform(*SOIL_LENSES_PER_SQ) * n_sq)))
        k = SOIL_RIM_KEEP_MM
        cand = rng.uniform([k, k], [W - k, H - k], size=(80, 2))
        h = ground(cand[:, 0], cand[:, 1])
        score = -(h - h.mean()) / (h.std() + 1e-9) + rng.normal(0, 0.8, len(cand))
        cs = []
        for i in np.argsort(-score):
            if all(np.hypot(*(cand[i] - c)) >= SOIL_LENS_SEP_MM for c in cs):
                cs.append(cand[i])
            if len(cs) == n:
                break
        self.c = np.array(cs)
        ax = np.stack([flow.ux(self.c[:, 0], self.c[:, 1]),
                       flow.uy(self.c[:, 0], self.c[:, 1])], 1)
        self.ax = ax / np.linalg.norm(ax, axis=1, keepdims=True)
        self.pp = np.stack([-self.ax[:, 1], self.ax[:, 0]], 1)
        self.l = rng.uniform(*SOIL_LENS_HALF_LEN_MM, len(cs))
        self.b = self.l * rng.uniform(*SOIL_LENS_ASPECT, len(cs))
        nx, ny = flow.vx.shape
        self.warp = Field2D(SOIL_LENS_WARP_MM * _smooth_noise(
            rng, (nx, ny), 1.2 / flow.res), flow.x0, flow.y0, flow.res)

    def metric(self, i, X, Y):
        """Lens metric f (1 on the outline), its along/across coords, ∇f."""
        dx, dy = X - self.c[i, 0], Y - self.c[i, 1]
        a = dx * self.ax[i, 0] + dy * self.ax[i, 1]
        b = dx * self.pp[i, 0] + dy * self.pp[i, 1]
        f = np.abs(a) / self.l[i] + (b / self.b[i]) ** 2
        ga = np.sign(a) / self.l[i]
        gb = 2 * b / self.b[i] ** 2
        nx = ga * self.ax[i, 0] + gb * self.pp[i, 0]
        ny = ga * self.ax[i, 1] + gb * self.pp[i, 1]
        return f, a, b, nx, ny

    def dist(self, X, Y):
        """Approximate signed distance (mm) to the nearest lens (< 0 inside)."""
        d = np.full(np.shape(X), np.inf)
        for i in range(len(self.c)):
            f, *_ = self.metric(i, X, Y)
            d = np.minimum(d, (f - 1) * self.b[i] / 2)
        return d + self.warp(X, Y)

    def mask(self, X, Y):
        t = np.clip(0.5 - self.dist(X, Y) / SOIL_EDGE_MM, 0, 1)
        return t * t * (3 - 2 * t)


def finish_flow(rng, flow, lenses, W, H):
    """Flow wraps around soil lenses, then rim steering (REQ-FL-4/5)."""
    X, Y = flow.grid()
    vx, vy = flow.vx.copy(), flow.vy.copy()
    for i in range(len(lenses.c)):
        band = rng.uniform(*DEFLECT_BAND_MM)
        strength = rng.uniform(*DEFLECT_STRENGTH)
        f, a, b, nx, ny = lenses.metric(i, X, Y)
        nn = np.maximum(np.hypot(nx, ny), 1e-9)
        nx, ny = nx / nn, ny / nn
        d = (f - 1) * lenses.b[i] / 2
        w = strength * (1 - _smooth01(d / band))
        inward = np.minimum(vx * nx + vy * ny, 0)
        vx, vy = vx - w * inward * nx, vy - w * inward * ny
        # Parting at the upstream tip: push sideways by the side we're on.
        side = np.tanh(b / 0.8)               # smooth: no parting seam upstream
        vx = vx - 0.6 * w * inward * side * lenses.pp[i, 0]
        vy = vy - 0.6 * w * inward * side * lenses.pp[i, 1]
        n = np.maximum(np.hypot(vx, vy), 1e-9)
        vx, vy = vx / n, vy / n
    th = np.arctan2(vy, vx)
    th = steer_field(rng, th, X, Y, W, H, flow.res)
    return Flow(np.cos(th), np.sin(th), flow.x0, flow.y0, flow.res)


def steer_field(rng, th, X, Y, W, H, res):
    """Rim rule on the field (REQ-EDGE-7/8): at or along the edge, never away.

    Ramp distance, allowed angle and pull vary smoothly over the tile, so
    the rim is irregular but continuous.
    """
    def nmap(lo, hi):
        return lo + (hi - lo) * _smooth01(0.5 + 0.35 * _smooth_noise(rng, th.shape, 8.0 / res))
    flow_mm = nmap(*EDGE_FLOW_MM)
    ok = np.radians(nmap(*EDGE_OK_DEG))
    pull = nmap(*EDGE_PULL)
    th_in = th.copy()
    for dist, out in ((X, np.pi), (W - X, 0.0), (Y, -np.pi / 2), (H - Y, np.pi / 2)):
        w = _smooth01(1 - np.maximum(dist, 0) / flow_mm)
        phi = _wrap(th - out)
        # Turn side from a SMOOTHED preference, so whole stretches of rim turn
        # the same way and flips are rare.
        pref = ndimage.gaussian_filter(np.sin(phi), STEER_SIDE_SIGMA_MM / res)
        sgn = np.where(pref >= 0, 1.0, -1.0)
        phi = np.where((np.abs(phi) > np.pi / 2) & (np.sign(phi) != sgn), phi + 2 * np.pi * sgn, phi)
        a = np.abs(phi)
        a = np.where(a > ok, ok + (a - ok) * (1 - w), a)
        a = a * (1 - pull * w)
        wa = _smooth01(1 - np.maximum(dist, 0) / RIM_ALONG_MM)
        a = a + (ok - a) * wa
        th = np.where(w > 0, out + sgn * a, th)
    # Apply the steering as a smoothed SCALAR rotation of the input angle:
    # θ + Δ with continuous Δ cannot create singularities (no starbursts).
    # Where the turn side flips, Δ passes through 0 and leaves a short
    # parting at the wall instead.
    delta = ndimage.gaussian_filter(_wrap(th - th_in), STEER_SMOOTH_MM / res)
    return th_in + delta


def soil_relief(rng, m, X, Y, vox):
    """Soil sits lower than the grass, slightly rough, with sparse angular pebbles."""
    # Soft-edged trough: the dip and roughness use a blurred mask so no step
    # outlines the patch (the bank itself is drawn by frond outlines).
    ms = ndimage.gaussian_filter(m, SOIL_DIP_SOFT_MM / vox)
    relief = -SOIL_DIP_MM * ms + SOIL_ROUGH_MM * _smooth_noise(rng, X.shape, 0.4 / vox) * ms
    area = float((m > 0.5).sum()) * vox * vox
    n = int(rng.poisson(PEBBLE_DENSITY * area))
    cand = np.argwhere(m > 0.8)
    if n == 0 or len(cand) == 0:
        return relief, 0
    peb = np.zeros(X.shape)
    placed = []
    for idx in cand[rng.permutation(len(cand))[:n * 20]]:
        c = np.array([X[tuple(idx)], Y[tuple(idx)]])
        r = PEBBLE_R_MM[0] + (PEBBLE_R_MM[1] - PEBBLE_R_MM[0]) * rng.random() ** 2
        if any(np.hypot(*(c - q)) < PEBBLE_GAP_MM + r + rq for q, rq in placed):
            continue
        placed.append((c, r))
        # Rounded faceted pebble: a low dome cut by K tilted planes, so facets
        # read as stone without flat tops or prism walls.
        K = int(rng.integers(7, 11))
        th = np.sort(rng.uniform(0, 2 * np.pi, K))
        off = r * rng.uniform(0.75, 1.0, K)
        el = rng.uniform(1.0, 1.5)
        ang = rng.uniform(0, np.pi)
        hp = r * rng.uniform(0.5, 0.7)
        slope = np.tan(np.radians(rng.uniform(30, 42)))
        rr = int(np.ceil(1.7 * r / vox)) + 1
        i, j = idx
        ia, ib = max(i - rr, 0), min(i + rr + 1, X.shape[0])
        ja, jb = max(j - rr, 0), min(j + rr + 1, X.shape[1])
        px, py = X[ia:ib, ja:jb] - c[0], Y[ia:ib, ja:jb] - c[1]
        qa = (px * np.cos(ang) + py * np.sin(ang)) / el
        qb = -px * np.sin(ang) + py * np.cos(ang)
        h = hp * np.sqrt(np.clip(1 - (qa ** 2 + qb ** 2) / r ** 2, 0, None))
        for t_, o_ in zip(th, off):
            h = np.minimum(h, slope * (o_ - (qa * np.cos(t_) + qb * np.sin(t_))))
        peb[ia:ib, ja:jb] = np.maximum(peb[ia:ib, ja:jb], np.clip(h, 0, None))
        if len(placed) >= n:
            break
    return relief + peb * m, len(placed)


class Grid:
    def __init__(self, W, H, vox):
        self.vox = vox
        self.nx, self.ny = int(round(W / vox)), int(round(H / vox))
        self.xc = (np.arange(self.nx) + 0.5) * vox
        self.yc = (np.arange(self.ny) + 0.5) * vox


def _rise(u, L, Lt):
    """Frond crest profile: flush at the base, full by 0.45 L, drooping to the tip."""
    tt = np.clip(np.asarray(u) / (0.45 * L), 0, 1)
    up = tt * tt * (3 - 2 * tt)
    td = np.clip((np.asarray(u) - 0.55 * L) / (Lt - 0.55 * L), 0, 1)
    return up * (1 - DROOP * td * td * (3 - 2 * td))


def _smooth01(t):
    t = np.clip(t, 0, 1)
    return t * t * (3 - 2 * t)


def _wrap(a):
    return (a + np.pi) % (2 * np.pi) - np.pi


def trace_batch(P, length, flow, delta=None, sign=1.0, step=TRACE_STEP_MM):
    """RK2 streamlines of the flow from many starts at once → (N, n+1, 2).

    `delta` rotates each streamline's heading by a constant offset (radians).
    Steps are unit-speed, so sample k sits at arc length k·step.
    """
    P = np.asarray(P, float)
    N = len(P)
    n = max(int(np.ceil(length / step)), 1)
    c = np.cos(delta) if delta is not None else np.ones(N)
    sn = np.sin(delta) if delta is not None else np.zeros(N)
    pts = np.empty((N, n + 1, 2))
    pts[:, 0] = P

    def d(Q):
        vx, vy = flow.ux(Q[:, 0], Q[:, 1]), flow.uy(Q[:, 0], Q[:, 1])
        return sign * np.stack([c * vx - sn * vy, sn * vx + c * vy], 1)

    for k in range(n):
        q = pts[:, k]
        pts[:, k + 1] = q + step * d(q + 0.5 * step * d(q))
    return pts


def project_spine(P, pts, us):
    """Arc length u and signed lateral offset v of points P on a polyline."""
    A = pts[:-1]
    D = pts[1:] - pts[:-1]
    dd = np.maximum((D * D).sum(1), 1e-12)
    R = P[:, None, :] - A[None, :, :]
    t = np.clip((R * D[None]).sum(2) / dd[None], 0, 1)
    C = R - t[..., None] * D[None]
    dist2 = (C * C).sum(2)
    k = dist2.argmin(1)
    r = np.arange(len(P))
    u = us[k] + t[r, k] * (us[k + 1] - us[k])
    cross = D[k, 0] * R[r, k, 1] - D[k, 1] * R[r, k, 0]
    v = np.sign(cross) * np.sqrt(dist2[r, k])
    return u, v


def make_strand_roots(rng, flow, W, H):
    """Frond roots along strands (REQ-FL-3): long streamlines, fronds shingled.

    Returns roots (M, 2) and per-root frond lengths (before swath scaling).
    """
    m = STRAND_MARGIN_MM
    seeds = poisson_sites(rng, -m, -m, W + m, H + m, STRAND_SPACING_MM)
    n = len(seeds)
    slen = rng.uniform(*STRAND_LEN_MM, n)
    back = slen * rng.uniform(0.2, 0.6, n)
    bp = trace_batch(seeds, back.max(), flow, sign=-1.0)
    start = bp[np.arange(n), np.minimum((back / TRACE_STEP_MM).astype(int), bp.shape[1] - 1)]
    fp = trace_batch(start, slen.max(), flow)
    roots, lens = [], []
    for i in range(n):
        u = 0.0
        while u < slen[i]:
            L = rng.uniform(*FROND_LEN_MM)
            k = min(int(u / TRACE_STEP_MM), fp.shape[1] - 2)
            p, t = fp[i, k], fp[i, k + 1] - fp[i, k]
            t = t / max(np.linalg.norm(t), 1e-9)
            roots.append(p + rng.uniform(-1, 1) * STRAND_LATERAL_MM * np.array([-t[1], t[0]]))
            lens.append(L)
            u += STRAND_GAP_FRAC * L * rng.uniform(0.8, 1.2)
    return np.array(roots), np.array(lens), n


def build_grass(rng, grid, ground, sw, lenses, flow, W, H, blocker=None, tuck_mm=0.5):
    """Return H0, mass map T, lip intervals [(I, J, bottom, top), …], soil, stats.

    The grass volume is MADE OF FRONDS (REQ-EDGE-1): T starts at the bare
    ground and every bit of grass height comes from some frond's own body —
    a broad skirt carrying its swath's envelope plus the domed blade on top.
    Fronds are streamlines of the flow (REQ-FL-1), gathered into strands.
    """
    vox = grid.vox
    X, Y = np.meshgrid(grid.xc, grid.yc, indexing='ij')
    H0 = ground(X, Y)
    soil = lenses.mask(X, Y)
    rel, n_pebbles = soil_relief(rng, soil, X, Y, vox)
    H0 = H0 + rel
    T = H0.copy()                          # no free-standing grass layer
    soil_depth = ndimage.distance_transform_edt(soil > 0.5) * vox
    soil_dist = ndimage.distance_transform_edt(soil <= 0.5) * vox   # grass → soil
    # Optional hard obstacles (e.g. rock footprints, used by meadowlab):
    # no rooting inside; spines stop once `tuck_mm` inside (pressed against
    # the obstacle face).  None = grasslab's own behaviour, unchanged.
    blk_depth = (ndimage.distance_transform_edt(blocker) * vox
                 if blocker is not None else None)

    def h0_at(px, py):
        ii = min(max(int(px / vox), 0), grid.nx - 1)
        jj = min(max(int(py / vox), 0), grid.ny - 1)
        return float(H0[ii, jj])

    roots, Ls0, n_strands = make_strand_roots(rng, flow, W, H)
    keep = (roots[:, 0] >= 0) & (roots[:, 0] < W) & (roots[:, 1] >= 0) & (roots[:, 1] < H)
    roots, Ls0 = roots[keep], Ls0[keep]
    N = len(roots)
    r_own, r_a, r_b = sw.assign(roots[:, 0], roots[:, 1])
    r_s, r_l = sw.coords(r_own, r_a, r_b)
    ri = np.clip((roots[:, 0] / vox).astype(int), 0, grid.nx - 1)
    rj = np.clip((roots[:, 1] / vox).astype(int), 0, grid.ny - 1)
    bank = (soil_dist[ri, rj] < BANK_REACH_MM) & (soil[ri, rj] <= 0.5)
    stray = rng.random(N) < np.where(bank, BANK_STRAY_FRAC, STRAY_FRAC)
    delta = np.radians(np.where(stray, rng.choice([-1, 1], N) * rng.uniform(*STRAY_DELTA_DEG, N),
                                rng.uniform(-FROND_DELTA_DEG, FROND_DELTA_DEG, N))
                       + np.where(bank, rng.uniform(-BANK_WANDER_DEG, BANK_WANDER_DEG, N), 0.0))
    gsx, gsy = np.gradient(soil_dist, vox)
    wt = FROND_TIP_HALF_W_MM
    Lmax = FROND_LEN_MM[1] * 1.1 + wt + 0.5
    paths = trace_batch(roots, Lmax, flow, delta=delta)
    step = TRACE_STEP_MM

    lips = []
    ledges = []
    n_built = n_ledges = 0
    for f in range(N):
        p0, i, sb, lb = roots[f], r_own[f], r_s[f], r_l[f]
        path = paths[f]
        if blocker is not None and blocker[int(p0[0] / vox), int(p0[1] / vox)]:
            continue
        sprig = soil[int(p0[0] / vox), int(p0[1] / vox)] > 0.5
        if sprig and rng.random() > SPRIG_PROB:
            continue
        if sprig:
            L = rng.uniform(*SPRIG_LEN_MM)
            wmax, hf, wsk = 0.5, 0.5, 0.0
        else:
            L = Ls0[f] * (0.8 + 0.3 * sb)
            wmax = rng.uniform(*FROND_HALF_W_MM)
            hf = rng.uniform(*FROND_H_MM)
            wsk = SKIRT_FACTOR * wmax
        bank_f = bool(bank[f]) and not sprig
        h_scale = rng.uniform(*BANK_H_SCALE) if bank_f else 1.0
        hf *= h_scale
        inset = RIM_TIP_INSET_MM[0] + (RIM_TIP_INSET_MM[1] - RIM_TIP_INSET_MM[0]) * rng.random() ** 2
        spill = WISP_SPILL_MM if rng.random() < WISP_FRAC else \
            SOIL_SPILL_MM[0] + (SOIL_SPILL_MM[1] - SOIL_SPILL_MM[0]) * rng.random() ** 2

        # Shorten (never slice) fronds so their tips end inside the tile and
        # lap at most `spill` into bare soil (REQ-EDGE-2/3).
        nL = min(int(np.ceil((L + wt) / step)) + 1, path.shape[0])
        cx, cy = path[:nL, 0], path[:nL, 1]
        uu = np.arange(nL) * step
        out = (cx < inset) | (cx > W - inset) | (cy < inset) | (cy > H - inset)
        ci = np.clip((cx / vox).astype(int), 0, grid.nx - 1)
        cj = np.clip((cy / vox).astype(int), 0, grid.ny - 1)
        deep = (soil_depth[ci, cj] > spill) & ~sprig
        if blk_depth is not None:
            out = out | (blk_depth[ci, cj] > tuck_mm)
        stop = np.nonzero(out | deep)[0]
        k_h = 1.0
        tip_soft = bool(len(stop)) and bool(deep[stop[0]])
        base_soft = (not sprig) and soil_dist[int(p0[0] / vox), int(p0[1] / vox)] < SOIL_BASE_SOFT_MM
        if len(stop):
            L0, L = L, uu[stop[0]] - wt
            if L < FROND_MIN_LEN_MM:
                continue
            k_h = float(np.clip(L / L0, 0.3, 1.0)) ** 0.8
            hf *= k_h
        Lt = L + wt
        cb = SKIRT_BASE_CAP * wsk           # skirt extends behind the root
        u_c, u_h = (Lt - cb) / 2, (Lt + cb) / 2

        # Spine = traced streamline (to Lt + 0.3) + straight cap behind the root.
        nf = min(int(np.ceil((Lt + 0.3) / step)) + 1, path.shape[0])
        fwd = path[:nf:2]
        uf = np.arange(0, nf, 2) * step
        t0 = path[1] - path[0]
        t0 = t0 / max(np.linalg.norm(t0), 1e-9)
        ub = np.linspace(-cb - 0.3, 0, 4)[:-1]
        sp = np.concatenate([p0 + ub[:, None] * t0, fwd])
        su_ = np.concatenate([ub, uf])

        reach = max(wmax, wsk)
        i0 = max(int((sp[:, 0].min() - reach) / vox), 0)
        i1 = min(int((sp[:, 0].max() + reach) / vox) + 2, grid.nx)
        j0 = max(int((sp[:, 1].min() - reach) / vox), 0)
        j1 = min(int((sp[:, 1].max() + reach) / vox) + 2, grid.ny)
        if i0 >= i1 or j0 >= j1:
            continue
        I, J = np.meshgrid(np.arange(i0, i1), np.arange(j0, j1), indexing='ij')
        I, J = I.ravel(), J.ravel()
        u, v = project_spine(np.stack([grid.xc[I], grid.yc[J]], 1), sp, su_)
        # Blade planform (pointed lens, round tip cap).
        w = wt + (wmax - wt) * np.sin(np.pi * (0.25 + 0.75 * np.clip(u / L, 0, 1)))
        w = np.where(u > L, np.sqrt(np.clip(wt ** 2 - (u - L) ** 2, 0, None)), w)
        in_blade = (u >= 0) & (u <= Lt) & (np.abs(v) < w)
        # Skirt planform: a lens from −cb to Lt (rounded base, tapering tip).
        ws = wsk * np.clip(1 - ((u - u_c) / u_h) ** 2, 0, 1) ** 0.6
        in_skirt = (np.abs(v) < ws) & (u >= -cb) & (u <= Lt)
        # Bank fronds: which columns are on the soil-facing side (REQ-FL-6).
        facing = np.zeros(len(u), bool)
        if bank_f:
            tans = np.diff(path, axis=0)
            tans = tans / np.maximum(np.linalg.norm(tans, axis=1, keepdims=True), 1e-9)
            kk = np.clip((u / step).astype(int), 0, len(tans) - 1)
            sv = np.sign(v)
            ox, oy = -tans[kk, 1] * sv, tans[kk, 0] * sv
            facing = ((ox * -gsx[I, J] + oy * -gsy[I, J]) > 0.3) & (soil_dist[I, J] < BANK_FACE_MM)
            in_skirt &= ~(facing & (np.abs(v) >= w))   # nothing fills under the ledge
        inside = in_blade | in_skirt
        if not inside.any():
            continue
        I, J, u, v, w, ws, in_blade, facing = (I[inside], J[inside], u[inside], v[inside],
                                               w[inside], ws[inside], in_blade[inside],
                                               facing[inside])
        # Skirt: carries this swath's envelope (continued past its border),
        # falling off with the frond's own outline and dropping toward the tip.
        su = np.minimum(sb + np.clip(u, 0, None) / sw.len[i], 1.0)
        env = 0.0 if sprig else k_h * h_scale * sw.env(np.full(len(u), i), su, np.full(len(u), lb))
        # Height: plateau across the lens, falling off at the sides; along the
        # frond it ramps up over the rounded base cap and descends gradually
        # to ~0 over the last SKIRT_END_MM — a pointed, lowering tip, never a
        # "sawn log" face and never a rounded scallop (fish scales).
        qs = np.clip(np.abs(v) / np.maximum(ws, 1e-6), 0, 1)
        taper = 1 - SKIRT_TIP_DROP * _smooth01((u - 0.5 * L) / (Lt - 0.5 * L))
        if tip_soft:
            t_end = min(SKIRT_END_MM, 0.4 * Lt)
            taper = taper * (1 - _smooth01((u - (Lt - t_end)) / t_end))
        if base_soft:
            taper = taper * _smooth01((u + cb) / max(1.5 * cb, 1e-6))
        hs = env * taper * (1 - qs ** 3) ** 1.5
        # Blade on top.
        q = np.clip(np.abs(v) / np.maximum(w, 1e-6), 0, 1)
        blade = np.where(in_blade, hf * _rise(u, L, Lt) * (1 - q ** 2) ** DOME_EXP, 0.0)
        top = H0[I, J] + hs + blade

        lip_len = min(LIP_MAX_MM, LIP_FRAC * L)
        u_lip = Lt - lip_len
        lp = in_blade & (u >= u_lip)
        # Side ledge on the soil-facing edge of bank fronds: the outer
        # SIDE_LIP_W of the blade overhangs a recessed body with a 45°
        # underside, so stacked blades show undercut ridges (REQ-FL-6).
        e = w - np.abs(v)
        ledge = facing & in_blade & (e < SIDE_LIP_W_MM) & ~lp
        mass = ~lp & ~ledge
        T[I[mass], J[mass]] = smoothmax(T[I[mass], J[mass]], top[mass], SOFTMAX_K_MM)
        n_built += 1
        if ledge.any():
            # Underside: LIP_MIN_T below the ledge's own top at the edge,
            # descending inward at 45° — a shallow wedge just under the blade.
            bot_l = top[ledge] - LIP_MIN_T_MM - e[ledge]
            ledges.append((I[ledge], J[ledge], bot_l, top[ledge]))
            n_ledges += 1
        if lp.any():
            # 45° underside measured back from the drooped tip crest.
            s_tip = min(sb + Lt / sw.len[i], 1.0)
            env_t = 0.0 if sprig else \
                k_h * sw.env(np.array([i]), np.array([s_tip]), np.array([lb]))[0]
            tp = path[min(int(Lt / step), path.shape[0] - 1)]
            g_t = h0_at(tp[0], tp[1])
            tip = g_t + (0.0 if tip_soft else env_t * (1 - SKIRT_TIP_DROP)) \
                + hf * float(_rise(Lt, L, Lt))
            bot = tip - LIP_MIN_T_MM - (Lt - u[lp])
            keep = (top[lp] - bot) >= LIP_MIN_T_MM
            if keep.any():
                lips.append((I[lp][keep], J[lp][keep], bot[keep], top[lp][keep]))
            # Lip side columns too thin to undercut stay solid (fused blade
            # edge) instead of being dropped — dropping them sawtoothed tips.
            thin = np.nonzero(lp)[0][~keep]
            T[I[thin], J[thin]] = smoothmax(T[I[thin], J[thin]], top[thin], SOFTMAX_K_MM)
    # Open the undercuts: neighbouring skirts fill in under every ledge, so
    # after all fronds are built, cut the mass beneath each VISIBLE ledge
    # (its top is the surface there) down to its 45° underside — a wedge
    # under the blade edge, never a slot (REQ-FL-6, REQ-FDM-3).
    for I, J, bot, top in ledges:
        vis = top >= T[I, J] - 0.05
        if not vis.any():
            continue
        T[I[vis], J[vis]] = np.minimum(T[I[vis], J[vis]], bot[vis])
        lips.append((I[vis], J[vis], bot[vis], top[vis]))
    T = np.maximum(T, H0)
    gaps = float(((T - H0) < 0.3)[soil < 0.5].mean())
    return H0, T, lips, n_built, soil, (n_strands, n_pebbles, gaps, n_ledges)


def make_field(grid, T, lips, top_z):
    """Continuous field: vertical signed distance, clamped to a narrow band."""
    vox = grid.vox
    nz = int(np.ceil(top_z / vox)) + 2
    zc = ((np.arange(nz) + 0.5) * vox).astype(np.float32)
    cl = 2.0 * vox
    F = np.clip(T[:, :, None].astype(np.float32) - zc[None, None, :], -cl, cl)
    for I, J, bot, top in lips:
        z0 = max(int(bot.min() / vox) - 1, 0)
        z1 = min(int(top.max() / vox) + 2, nz)
        if z1 <= z0:
            continue
        zi = np.arange(z0, z1)[None, :]
        z = zc[z0:z1][None, :]
        f = np.clip(np.minimum(z - bot[:, None], top[:, None] - z), -cl, cl)
        F[I[:, None], J[:, None], zi] = np.maximum(F[I[:, None], J[:, None], zi], f)
    return F


# ── FDM backstop ─────────────────────────────────────────────────────────────
def fdm_backstop(F, vox):
    """Support filter + hole fill + bed connectivity on occupancy (should be ~no-op)."""
    before = F > 0
    M = ndimage.binary_fill_holes(np.pad(before, ((0, 0), (0, 0), (0, 1))))[:, :, :-1]
    for _ in range(4):
        for z in range(1, M.shape[2]):
            b = M[:, :, z - 1]
            sup = b.copy()
            sup[1:, :] |= b[:-1, :]
            sup[:-1, :] |= b[1:, :]
            sup[:, 1:] |= b[:, :-1]
            sup[:, :-1] |= b[:, 1:]
            M[:, :, z] &= sup
        lab, _ = ndimage.label(M)
        keep = np.unique(lab[:, :, 0])
        conn = np.isin(lab, keep[keep > 0])
        if (conn == M).all():
            break
        M &= conn
    added, removed = M & ~before, before & ~M
    F[added] = vox
    F[removed] = -vox
    return int(added.sum()), int(removed.sum())


# ── Mesh + base ──────────────────────────────────────────────────────────────
def mesh_field(F, vox):
    from skimage.measure import marching_cubes
    import trimesh
    p = 2
    V = np.pad(F, p, mode='constant', constant_values=-2.0 * vox)
    verts, faces, _, _ = marching_cubes(V, 0.0, spacing=(vox, vox, vox))
    verts -= (p - 0.5) * vox
    mesh = trimesh.Trimesh(verts, faces, process=True)
    if mesh.volume < 0:
        mesh.invert()
    return mesh


def attach_base(top_mesh, cols, rows, tile_max_z):
    import manifold3d as m3d
    import trimesh
    from dharmatiles.core.config import BaseConfig, SurfaceConfig
    from dharmatiles.bases import dungeonblocks as db

    surface = SurfaceConfig(cols=cols, rows=rows, square_mm=SQUARE_MM)
    base_cfg = BaseConfig()
    peg_h = db.select_peg_height(np.zeros((2, 2)), base_cfg, tile_max_z)
    base = db.make_base(surface, peg_h, base_cfg)

    def to_m(t):
        return m3d.Manifold(mesh=m3d.Mesh(
            vert_properties=np.asarray(t.vertices, dtype=np.float32),
            tri_verts=np.asarray(t.faces, dtype=np.uint32)))

    solid = (to_m(top_mesh) + to_m(base)).simplify(SIMPLIFY_TOL_MM)
    parts = solid.decompose()
    vols = [q.volume() for q in parts]
    k = int(np.argmax(vols))
    crumbs = (len(parts) - 1, float(sum(vols) - vols[k]))
    solid = parts[k]
    out = solid.to_mesh()
    mesh = trimesh.Trimesh(out.vert_properties[:, :3], out.tri_verts, process=False)
    return mesh, len(solid.decompose()), peg_h, crumbs


def audit_overhang(mesh):
    n = mesh.face_normals
    cz = mesh.triangles_center[:, 2]
    top = cz > 0.05
    bad = (n[:, 2] < -np.sin(np.radians(OVERHANG_DEG + 3.0))) & top
    a = mesh.area_faces
    return float(a[bad].sum()), float(a[bad].sum() / max(a[top].sum(), 1e-9))


def self_check(mesh, path, W, H):
    """Quick monochrome renders for Claude's own filtering (REQ-DIAG-2)."""
    from dharmatiles.render import render
    m = mesh.copy()
    m.visual.face_colors = np.tile([150, 150, 150, 255], (len(m.faces), 1))
    render([m], path, elev=35, azim=-125, resolution=(1400, 1100), quiet=True)
    render([m], path.with_name(path.stem + '-top.png'), elev=89, azim=-90,
           resolution=(1200, 1200), quiet=True)
    # Close-up: the central 18 mm, low light angle.
    c = np.array([W / 2, H / 2])
    sub = m.slice_plane([c[0] - 9, 0, 0], [1, 0, 0], cap=False)
    sub = sub.slice_plane([c[0] + 9, 0, 0], [-1, 0, 0], cap=False)
    sub = sub.slice_plane([0, c[1] - 9, 0], [0, 1, 0], cap=False)
    sub = sub.slice_plane([0, c[1] + 9, 0], [0, -1, 0], cap=False)
    sub = sub.slice_plane([0, 0, 0.5], [0, 0, 1], cap=False)
    sub.visual.face_colors = np.tile([150, 150, 150, 255], (len(sub.faces), 1))
    render([sub], path.with_name(path.stem + '-close.png'), elev=30, azim=-125,
           resolution=(1400, 1100), quiet=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--cols', type=int, default=2)
    ap.add_argument('--rows', type=int, default=2)
    ap.add_argument('--flat', action='store_true', help='flat ground (S1 swath study)')
    ap.add_argument('--vox', type=float, default=None)
    ap.add_argument('--check', type=pathlib.Path, default=None,
                    help='write quick monochrome self-check PNGs here')
    args = ap.parse_args()

    t0 = time.perf_counter()
    tm = {}
    rng = np.random.default_rng(args.seed)
    W, H = args.cols * SQUARE_MM, args.rows * SQUARE_MM
    vox_t = args.vox or (0.1 if args.cols * args.rows == 1 else 0.12)
    grid = Grid(W, H, W / round(W / vox_t))
    vox = grid.vox

    ground, gstats = make_ground(rng, W, H, 0.25, args.flat)
    flow0 = make_direction(rng, ground)
    sw = Swaths(rng, W, H, flow0.ux, flow0.uy)
    lenses = Lenses(rng, ground, flow0, W, H)
    flow = finish_flow(rng, flow0, lenses, W, H)
    tm['setup'] = time.perf_counter() - t0

    t = time.perf_counter()
    H0, T, lips, n_fronds, soil, n_patches = build_grass(rng, grid, ground, sw, lenses, flow, W, H)
    tm['fronds'] = time.perf_counter() - t

    t = time.perf_counter()
    top_z = max(T.max(), max((tp.max() for *_, tp in lips), default=0)) + 0.3
    F = make_field(grid, T, lips, top_z)
    added, removed = fdm_backstop(F, vox)
    tm['field+fdm'] = time.perf_counter() - t

    t = time.perf_counter()
    top_mesh = mesh_field(F, vox)
    tm['mc'] = time.perf_counter() - t

    t = time.perf_counter()
    mesh, n_bodies, peg_h, crumbs = attach_base(top_mesh, args.cols, args.rows,
                                                float(top_mesh.vertices[:, 2].max()))
    tm['base'] = time.perf_counter() - t

    occ = F > 0
    topz = (occ.shape[2] - np.argmax(occ[:, :, ::-1], axis=2)) * vox
    gh = topz - H0
    ov_area, ov_frac = audit_overhang(mesh)

    out_dir = pathlib.Path(__file__).parent / 'out'
    out_dir.mkdir(exist_ok=True)
    path = out_dir / (f"{args.cols}x{args.rows}-sculpted-grass"
                      f"{'-flat' if args.flat else ''}-s{args.seed}.stl")
    mesh.export(path)

    mean_top = float(topz.mean())
    band = DB_GRASS_BAND[0] <= mean_top <= DB_GRASS_BAND[1]
    print(f"→ {path}  ({path.stat().st_size / 1e6:.1f} MB)")
    print(f"  mesh: {len(mesh.vertices):,} verts, {len(mesh.faces):,} faces, "
          f"watertight={mesh.is_watertight}, bodies={n_bodies}, peg={peg_h} mm, "
          f"crumbs dropped {crumbs[0]} ({crumbs[1]:.3f} mm³)")
    print(f"  vox {vox:.4f} mm; {len(sw.sites)} swath sites, {n_fronds} fronds, "
          f"{len(lips)} lips")
    if gstats:
        print(f"  ground {gstats['ground_min']:.2f}–{gstats['ground_max']:.2f} mm, "
              f"max slope {gstats['max_slope_deg']:.1f}°, usable {gstats['usable']:.0%}")
    print(f"  {n_patches[0]} strands; bare soil: {len(lenses.c)} lenses, {(soil > 0.5).mean():.1%} of the tile, "
          f"{n_patches[1]} pebbles; gaps between fronds (grass <0.3 mm, off-soil) "
          f"{n_patches[2]:.1%}; {n_patches[3]} bank fronds with side ledges")
    print(f"  grass above ground: mean {gh.mean():.2f}, p95 {np.percentile(gh, 95):.2f}, "
          f"max {gh.max():.2f} mm")
    print(f"  mean grass top {mean_top:.2f} mm above datum "
          f"(DB band {DB_GRASS_BAND[0]}–{DB_GRASS_BAND[1]}: {'OK' if band else 'OUT'})")
    print(f"  FDM backstop: +{added:,} / −{removed:,} voxels; "
          f">{OVERHANG_DEG:.0f}°(+3) overhang {ov_area:.1f} mm² ({ov_frac:.2%})")
    print("  timing: " + ", ".join(f"{k} {v:.1f}s" for k, v in tm.items())
          + f"; total {time.perf_counter() - t0:.1f}s")
    if args.check:
        self_check(mesh, args.check, W, H)
        print(f"  self-check → {args.check}")


if __name__ == '__main__':
    main()
