#!/usr/bin/env python3
"""grasslab — flow-ribbon sculpted grass on hilly ground, on a DungeonBlocks tile.

Implements grasslab/DESIGN.md:

  1 hills H0 (rises only) ─► 2 flow field θ (regional angle + in-phase
  meanders + downhill pull) ─► 3 swath envelope (warped anisotropic Voronoi,
  height only) ─► 4 soil lenses; flow wraps them, then rim steering ─►
  5 fronds: strands of streamline-traced blades, each a skirt + domed blade
  + 45° tip lip (+ side ledges on banks), soft-maxed into a mass map T ─►
  6 field F = max(T − z, lip intervals) ─► FDM backstop ─► marching cubes
  ─► 7 ∪ DB base ─► simplify ─► STL

Usage:
    python grasslab/grasslab.py                            # 2x2 with hills, seed 1
    python grasslab/grasslab.py --cols 1 --rows 1 --flat   # S1 flat study
    python grasslab/grasslab.py --seed 3 --check out.png   # + self-check PNGs
"""
from __future__ import annotations

import argparse
import pathlib
import time
from dataclasses import dataclass

import manifold3d as m3d
import numpy as np
import trimesh
from scipy import ndimage

# ── Parameters (DESIGN.md §3), grouped by stage ─────────────────────────────
# tile and grids
SQUARE_MM            = 35.0
FIELD_RES_MM         = 0.25        # ground / flow / swath grids
VOX_MM               = 0.12        # grass voxel (VOX_1X1_MM at 1 × 1)
VOX_1X1_MM           = 0.10
MARGIN_MM            = 10.0        # swath sites beyond the tile so edges are covered

# 1 hills H0
HILL_RELIEF_MM       = 7.0         # raw relief target before the slope cap
GROUND_EDGE_MM       = 3.4         # perimeter level = lowest ground
HILL_SIGMA_LARGE_MM  = 10.0
HILL_SIGMA_MID_MM    = 5.0
HILL_MID_WEIGHT      = 0.35
HILL_PLAIN_FRAC      = 0.4         # share of the interior left at perimeter level
HILL_PLAIN_SOFT      = 0.25        # softness of the plain→hill toe (× noise std)
EDGE_MARGIN_MM       = 9.0         # cosine feather to the edge level
MAX_GROUND_SLOPE_DEG = 34.0
USABLE_SLOPE_DEG     = 12.0        # reported only

# 2 flow field
COHERENCE_MM         = 15.0        # regional angle noise σ
THETA_STD_DEG        = 60.0        # regional angle swing
TERRAIN_FOLLOW       = 0.3         # max downhill blend weight
MEANDER_WAVELENGTH_MM = 15.0
MEANDER_AMP_DEG      = (10.0, 30.0)
MEANDER_AMP_SIGMA_MM = 12.0
MEANDER_PHASE_SIGMA_MM = 14.0

# 3 swath envelope (height only; fronds follow the flow, not swath axes)
SWATH_SPACING_MM     = 11.0
SWATH_LATERAL_ASPECT = 0.65        # lateral distances scaled by 1/aspect
SWATH_WARP_MM        = 2.5
SWATH_WARP_SIGMA_MM  = 3.0
SWATH_AMP_MM         = (1.2, 2.2)
ENV_FLOOR            = 0.25
ENV_EXP              = 1.6
ENV_LATERAL_EXP      = 0.6

# 4 soil lenses, flow deflection, soil relief
SOIL_LENSES_PER_SQ   = (1.3, 1.8)
SOIL_LENS_CANDIDATES = 80
SOIL_LENS_SCORE_NOISE = 0.8        # σ added to the low-ground z-score
SOIL_LENS_HALF_LEN_MM = (5.0, 9.0)
SOIL_LENS_ASPECT     = (0.45, 0.65)  # half-width / half-length
SOIL_LENS_SEP_MM     = 13.0
SOIL_RIM_KEEP_MM     = 7.0         # lens centres at least this far from the rim
SOIL_LENS_WARP_MM    = 0.8
SOIL_LENS_WARP_SIGMA_MM = 1.2
SOIL_EDGE_MM         = 0.5         # mask smoothstep width
DEFLECT_BAND_MM      = (1.5, 4.5)  # per lens: flow bends around it within this band
DEFLECT_STRENGTH     = (0.55, 1.0) # per lens: how strongly the flow hugs it
PARTING_PUSH         = 0.6         # sideways push at a lens's upstream tip
PARTING_SOFT_MM      = 0.8         # tanh width of the parting side choice
SOIL_DIP_MM          = 0.4         # soil sits this far below the ground
SOIL_DIP_SOFT_MM     = 0.8         # blur of the dip/roughness mask
SOIL_ROUGH_MM        = 0.08
SOIL_ROUGH_SIGMA_MM  = 0.4
PEBBLE_DENSITY       = 0.06        # per mm² of bare soil
PEBBLE_R_MM          = (0.45, 1.2) # skewed small (U²)
PEBBLE_GAP_MM        = 1.2

# 4b rim steering of the flow
RIM_NOISE_SIGMA_MM   = 8.0         # smoothness of the per-point rim knobs below
EDGE_FLOW_MM         = (9.0, 15.0) # steering ramp distance from the rim
EDGE_OK_DEG          = (70.0, 88.0)# allowed angle from 'at the edge'
EDGE_PULL            = (0.15, 0.45)# extra lean toward the edge at the rim
RIM_ALONG_MM         = 3.0         # this close to a wall the flow runs alongside it
STEER_SIDE_SIGMA_MM  = 20.0        # smoothing of the lean along the rim loop (1-D, across corners)
STEER_SIDE_HARD_MM   = 2.5         # width of a turn-side flip for inward headings
STEER_SIDE_SOFT_MM   = 8.0         # run-alongside fades (and the flow squares up) over this around a flip
STEER_NATURAL_DEG    = 30.0        # inward headings this close to parallel keep their own side
RIM_SQUARE_PULL      = 0.85        # pull where an edge's side is undecided (s → 0)
RIM_SQUARE_REACH     = 1.0         # …and the ramp reaches (1 + this)× further there
PULL_SOFT_RAD        = 0.35        # pull fades from outward to inward headings over ±this about 90°
STEER_CORNER_MM      = 2.0         # softness of the nearest-edge split at corners
STEER_SMOOTH_MM      = 1.5         # smoothing of the rotation field

# 5 strands and fronds
STRAND_SPACING_MM    = 2.65
STRAND_LEN_MM        = (15.0, 30.0)
STRAND_BACK_FRAC     = (0.2, 0.6)  # strand starts this × its length upstream of its seed
STRAND_MARGIN_MM     = 8.0
STRAND_GAP_FRAC      = 0.5         # roots every this × frond length along a strand
STRAND_LATERAL_MM    = 0.3
TRACE_STEP_MM        = 0.25
FROND_LEN_MM         = (5.5, 9.5)  # × (0.8 + 0.3 s) along the swath
FROND_HALF_W_MM      = (0.55, 0.9)
FROND_H_MM           = (0.65, 1.0)
FROND_TIP_HALF_W_MM  = 0.45
FROND_MIN_LEN_MM     = 1.5
FROND_DELTA_DEG      = 4.0         # heading jitter off the streamline
STRAY_FRAC           = 0.05
STRAY_DELTA_DEG      = (20.0, 35.0)
SPINE_PAD_MM         = 0.3         # spine runs this far past both ends
SPRIG_PROB           = 0.10        # chance a soil-rooted frond becomes a sprig
SPRIG_LEN_MM         = (1.8, 3.0)
SPRIG_HALF_W_MM      = 0.5
SPRIG_H_MM           = 0.5

# 5a frond body: blade, skirt, tip lip
DOME_EXP             = 0.9
DROOP                = 0.35        # tip sinks to (1 − DROOP) of the frond crest
SKIRT_FACTOR         = 2.2         # skirt half-width × blade half-width
SKIRT_BASE_CAP       = 0.6         # skirt extends this × skirt width behind the root
SKIRT_TIP_DROP       = 0.45        # skirt height falls by this toward the tip
LIP_MAX_MM           = 2.0
LIP_FRAC             = 0.35        # lip length ≤ this × frond length
LIP_MIN_T_MM         = 0.6
TIP_CARVE            = False       # carve under visible frond tips (shorelab turns this on)
TIP_GAP_MM           = 0.0         # with TIP_CARVE: open air gap under tips (≥ 0.6 printable)
TIP_VIS_TOL_MM       = 0.45        # tips buried this little by neighbours are exposed
SIDE_CARVE_ALL       = False       # side ledges on every blade, not just bank fronds
SIDE_BASE_FRAC       = 0.3         # …except over the first this × length (the base)
SIDE_GAP_MM          = 0.0         # air gap under those side ledges
SOFTMAX_K_MM         = 0.12

# 5b rim and soil endings, banks
RIM_TIP_INSET_MM     = (0.2, 1.5)  # per-frond tip inset from the wall (U²)
SIDE_INNER_MM        = 0.8         # obstacles (meadowlab): frond height falls to 0 this far inside
SOIL_SPILL_MM        = (0.8, 3.2)  # per-frond spill into soil (U²)
WISP_FRAC            = 0.04        # fronds allowed WISP_SPILL_MM over soil
WISP_SPILL_MM        = 4.0
SOIL_BASE_SOFT_MM    = 2.5         # fronds rooted this close to soil get a ramped base
SKIRT_END_MM         = 2.2         # soil-stopped fronds: skirt descends to ~0 over this
BANK_REACH_MM        = 4.0         # fronds rooted this close to soil are bank fronds
BANK_FACE_MM         = 6.0         # columns this close to soil can be soil-facing
BANK_WANDER_DEG      = 15.0        # extra heading wander for bank fronds
BANK_STRAY_FRAC      = 0.12
BANK_H_SCALE         = (0.65, 1.15)
SIDE_LIP_W_MM        = 0.7         # side ledge width (blade edge overhang)

# 6–7 mesh, base, audit
SIMPLIFY_TOL_MM      = 0.015
SLAB_BOX_MM          = 1.0         # exact box under the grass solid (ground ≥ edge level)
OVERHANG_DEG         = 45.0
DB_GRASS_BAND        = (4.4, 5.9)  # mean grass top above datum


# ── Helpers ─────────────────────────────────────────────────────────────────
def _smooth01(t):
    t = np.clip(t, 0, 1)
    return t * t * (3 - 2 * t)


def _wrap(a):
    return (a + np.pi) % (2 * np.pi) - np.pi


def _skewed(rng, lo_hi):
    """lo + (hi − lo)·U²: a draw skewed toward lo."""
    return lo_hi[0] + (lo_hi[1] - lo_hi[0]) * rng.random() ** 2


def smoothmax(a, b, k):
    return np.maximum(a, b) + k * np.log1p(np.exp(-np.abs(a - b) / k))


def _smooth_noise(rng, shape, sigma_px):
    n = ndimage.gaussian_filter(rng.standard_normal(shape), sigma_px, mode='wrap')
    return (n - n.mean()) / (n.std() + 1e-12)


class Field2D:
    """Bilinear sampler of a grid with origin (x0, y0) and spacing res."""

    def __init__(self, data, x0, y0, res):
        self.data, self.x0, self.y0, self.res = data, x0, y0, res

    def __call__(self, x, y):
        ix = (np.asarray(x, float) - self.x0) / self.res
        iy = (np.asarray(y, float) - self.y0) / self.res
        shp = np.shape(ix)
        out = ndimage.map_coordinates(self.data, [np.ravel(ix), np.ravel(iy)],
                                      order=1, mode='nearest')
        return out.reshape(shp)


class Grid:
    """The grass voxel grid: column centres at (i + ½)·vox."""

    def __init__(self, W, H, vox):
        self.vox = vox
        self.nx, self.ny = int(round(W / vox)), int(round(H / vox))
        self.xc = (np.arange(self.nx) + 0.5) * vox
        self.yc = (np.arange(self.ny) + 0.5) * vox


def poisson_sites(rng, x0, y0, x1, y1, r):
    """Dart-throwing Poisson-disk sites with radius r."""
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


# ── 1 Hills H0 ──────────────────────────────────────────────────────────────
def make_ground(rng, W, H, flat=False, res=FIELD_RES_MM):
    """Rises-only hills on an extended grid.  Returns (Field2D, stats)."""
    m = MARGIN_MM + 2.0
    nx, ny = int(np.ceil((W + 2 * m) / res)) + 1, int(np.ceil((H + 2 * m) / res)) + 1
    # Drawn even when flat, so the rest of the RNG stream is the same.
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


# ── 2 Flow field ────────────────────────────────────────────────────────────
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

    Everything up to here is a scalar angle (no singularities); lens
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


# ── 3 Swath envelope ────────────────────────────────────────────────────────
class Swaths:
    """Warped anisotropic Voronoi cells, each with a wave height envelope.

    Every point has exactly one owner → total coverage by construction.
    Swaths supply only the skirt HEIGHT; frond directions come from the flow.
    """

    def __init__(self, rng, W, H, flow, res=FIELD_RES_MM):
        m = MARGIN_MM
        self.sites = poisson_sites(rng, -m, -m, W + m, H + m, SWATH_SPACING_MM)
        n = len(self.sites)
        dx, dy = flow.ux(self.sites[:, 0], self.sites[:, 1]), flow.uy(self.sites[:, 0], self.sites[:, 1])
        nd = np.hypot(dx, dy)
        self.axis = np.stack([dx / nd, dy / nd], axis=1)
        self.perp = np.stack([-self.axis[:, 1], self.axis[:, 0]], axis=1)
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
        self.length = np.maximum(self.a_max - self.a_min, 1.0)

    def assign(self, x, y, chunk=100000):
        """Owner cell and its along/across coordinates (a, b) for points."""
        x, y = np.asarray(x, float), np.asarray(y, float)
        own = np.empty(len(x), dtype=int)
        a, b = np.empty(len(x)), np.empty(len(x))
        wp = self.warp(x, y)
        for c0 in range(0, len(x), chunk):
            sl = slice(c0, c0 + chunk)
            dx = x[sl, None] - self.sites[None, :, 0]
            dy = y[sl, None] - self.sites[None, :, 1]
            aa = dx * self.axis[None, :, 0] + dy * self.axis[None, :, 1]
            bb = dx * self.perp[None, :, 0] + dy * self.perp[None, :, 1]
            dist = np.hypot(aa, bb / SWATH_LATERAL_ASPECT) + wp[sl, None] * self.phase[None, :]
            k = dist.argmin(axis=1)
            r = np.arange(len(k))
            own[sl], a[sl], b[sl] = k, aa[r, k], bb[r, k]
        return own, a, b

    def coords(self, own, a, b):
        """Normalised cell coordinates s ∈ [0, 1] (along), l ∈ [−1, 1] (across)."""
        s = np.clip((a - self.a_min[own]) / self.length[own], 0, 1)
        l = np.clip(b / self.b_max[own], -1, 1)
        return s, l

    def env(self, own, s, l):
        """Wave envelope: low upstream and at the sides, crest downstream-centre."""
        return (self.amp[own] * (ENV_FLOOR + (1 - ENV_FLOOR) * s ** ENV_EXP)
                * (1 - l ** 2) ** ENV_LATERAL_EXP)


# ── 4 Soil lenses, flow wrap, rim steering, soil relief ─────────────────────
@dataclass
class Lenses:
    """Eye-shaped bare-soil openings aligned with the local flow (REQ-FL-4).

    Planform |a|/half_len + (b/half_w)² ≤ 1 in each lens's (axis, perp)
    frame, with a shared border warp.  `axis` rows must be unit vectors.
    """
    centre: np.ndarray     # (n, 2)
    axis: np.ndarray       # (n, 2)
    half_len: np.ndarray   # (n,)
    half_w: np.ndarray     # (n,)
    warp: Field2D

    @property
    def perp(self):
        return np.stack([-self.axis[:, 1], self.axis[:, 0]], 1)

    def edge(self, i, X, Y):
        """Approx. distance d (mm, < 0 inside), across coord b, lens-metric ∇f."""
        dx, dy = X - self.centre[i, 0], Y - self.centre[i, 1]
        ax, pp = self.axis[i], self.perp[i]
        a = dx * ax[0] + dy * ax[1]
        b = dx * pp[0] + dy * pp[1]
        f = np.abs(a) / self.half_len[i] + (b / self.half_w[i]) ** 2
        ga = np.sign(a) / self.half_len[i]
        gb = 2 * b / self.half_w[i] ** 2
        nx = ga * ax[0] + gb * pp[0]
        ny = ga * ax[1] + gb * pp[1]
        return (f - 1) * self.half_w[i] / 2, b, nx, ny

    def dist(self, X, Y):
        """Approximate signed distance (mm) to the nearest lens (< 0 inside)."""
        d = np.full(np.shape(X), np.inf)
        for i in range(len(self.centre)):
            d = np.minimum(d, self.edge(i, X, Y)[0])
        return d + self.warp(X, Y)

    def mask(self, X, Y):
        return _smooth01(0.5 - self.dist(X, Y) / SOIL_EDGE_MM)


def place_lenses(rng, ground, flow, W, H, per_sq=SOIL_LENSES_PER_SQ):
    """Lens centres on low ground, ≥ SOIL_LENS_SEP_MM apart, axis = local flow."""
    n_sq = (W / SQUARE_MM) * (H / SQUARE_MM)
    n = max(1, int(round(rng.uniform(*per_sq) * n_sq)))
    k = SOIL_RIM_KEEP_MM
    cand = rng.uniform([k, k], [W - k, H - k], size=(SOIL_LENS_CANDIDATES, 2))
    h = ground(cand[:, 0], cand[:, 1])
    score = -(h - h.mean()) / (h.std() + 1e-9) + rng.normal(0, SOIL_LENS_SCORE_NOISE, len(cand))
    cs = []
    for i in np.argsort(-score):
        if all(np.hypot(*(cand[i] - c)) >= SOIL_LENS_SEP_MM for c in cs):
            cs.append(cand[i])
        if len(cs) == n:
            break
    c = np.array(cs)
    ax = np.stack([flow.ux(c[:, 0], c[:, 1]), flow.uy(c[:, 0], c[:, 1])], 1)
    ax = ax / np.linalg.norm(ax, axis=1, keepdims=True)
    half_len = rng.uniform(*SOIL_LENS_HALF_LEN_MM, len(cs))
    half_w = half_len * rng.uniform(*SOIL_LENS_ASPECT, len(cs))
    nx, ny = flow.vx.shape
    warp = Field2D(SOIL_LENS_WARP_MM * _smooth_noise(
        rng, (nx, ny), SOIL_LENS_WARP_SIGMA_MM / flow.res), flow.x0, flow.y0, flow.res)
    return Lenses(c, ax, half_len, half_w, warp)


def finish_flow(rng, flow, lenses, W, H):
    """Flow wraps around the lenses, then rim steering (REQ-FL-4/5)."""
    X, Y = flow.grid()
    vx, vy = flow.vx.copy(), flow.vy.copy()
    perp = lenses.perp
    for i in range(len(lenses.centre)):
        band = rng.uniform(*DEFLECT_BAND_MM)
        strength = rng.uniform(*DEFLECT_STRENGTH)
        d, b, nx, ny = lenses.edge(i, X, Y)
        nn = np.maximum(np.hypot(nx, ny), 1e-9)
        nx, ny = nx / nn, ny / nn
        w = strength * (1 - _smooth01(d / band))
        inward = np.minimum(vx * nx + vy * ny, 0)
        vx, vy = vx - w * inward * nx, vy - w * inward * ny
        # Parting at the upstream tip: push sideways by the side we're on.
        side = np.tanh(b / PARTING_SOFT_MM)        # smooth: no parting seam upstream
        vx = vx - PARTING_PUSH * w * inward * side * perp[i, 0]
        vy = vy - PARTING_PUSH * w * inward * side * perp[i, 1]
        n = np.maximum(np.hypot(vx, vy), 1e-9)
        vx, vy = vx / n, vy / n
    th = np.arctan2(vy, vx)
    th = steer_rim(rng, th, X, Y, W, H, flow.res)
    return Flow(np.cos(th), np.sin(th), flow.x0, flow.y0, flow.res)


def _rim_sides(lean, res):
    """Turn side around the rim, one value per perimeter sample (CCW loop).

    lean: the band-weighted mean sin φ per sample, in CCW order (side +1 =
    turning counter-clockwise, on every edge).  The side is the sign of the
    lean smoothed at STEER_SIDE_SIGMA_MM along the loop, across corners, so
    it is one side per long stretch and corners continue the adjacent edge.
    Returns (hard, soft): that sign smoothed short (which way inward
    headings turn; a flip is a short parting) and long (how strongly the
    flow runs alongside; ≈ 0 around a flip, where the flow squares up and
    ends at the wall instead of colliding head-on).
    """
    m = ndimage.gaussian_filter1d(lean, STEER_SIDE_SIGMA_MM / res, mode='wrap')
    sg = np.where(m >= 0, 1.0, -1.0)
    hard = ndimage.gaussian_filter1d(sg, STEER_SIDE_HARD_MM / res, mode='wrap')
    soft = ndimage.gaussian_filter1d(sg, STEER_SIDE_SOFT_MM / res, mode='wrap')
    return hard, soft


def _turn(phi, sg, w, wp, ok, pull):
    """Rotation that compresses and pulls φ with inward headings turned to side sg.

    Continuous in φ except at the unwrap cut φ = −sg·π/2, which the caller
    masks (its weight is 0 there)."""
    p = np.where((np.abs(phi) > np.pi / 2) & (np.sign(phi) != sg), phi + 2 * np.pi * sg, phi)
    a0 = np.abs(p)
    a = np.where(a0 > ok, ok + (a0 - ok) * (1 - w), a0)
    # The pull leans flow that already heads outward further out; turned
    # inward headings only run alongside (pulling those too gave an S-bend
    # that read as a herringbone spine parallel to the wall).
    outward = _smooth01((np.pi / 2 + PULL_SOFT_RAD - a0) / (2 * PULL_SOFT_RAD))
    a = a * (1 - pull * wp * outward)
    return np.where(p >= 0, 1.0, -1.0) * a - p


def steer_rim(rng, th, X, Y, W, H, res):
    """Rim rule on the field (REQ-EDGE-7/8): at or along the edge, never away.

    Ramp distance, allowed angle and pull vary smoothly over the tile, so
    the rim is irregular but continuous.  The turn side is chosen once per
    long stretch of the rim loop (`_rim_sides`), not per point; around a
    flip the flow squares up and ends at the wall rather than meeting
    itself in a V; corners take the nearest edge.  Everything is a
    continuous rotation Δ of the input angle (no singularities).
    """
    def nmap(lo, hi):
        return lo + (hi - lo) * _smooth01(0.5 + 0.35 * _smooth_noise(
            rng, th.shape, RIM_NOISE_SIGMA_MM / res))
    flow_mm = nmap(*EDGE_FLOW_MM)
    ok = np.radians(nmap(*EDGE_OK_DEG))
    pull = nmap(*EDGE_PULL)
    xs, ys = X[:, 0], Y[0]
    ix = np.nonzero((xs >= 0) & (xs <= W))[0]
    iy = np.nonzero((ys >= 0) & (ys <= H))[0]
    # Edges in CCW order: (distance, outward normal, along axis, in-tile
    # along indices in CCW direction).  On every edge side +1 = out + ok
    # = counter-clockwise.
    edges = ((Y, -np.pi / 2, 0, ix), (W - X, 0.0, 1, iy),
             (H - Y, np.pi / 2, 0, ix[::-1]), (X, np.pi, 1, iy[::-1]))
    prof, cache = [], []
    for dist, out, along, idx in edges:
        d = np.maximum(dist, 0)
        w = _smooth01(1 - d / flow_mm)
        phi = _wrap(th - out)
        depth = 1 - along
        wt = w * ((X >= 0) & (X <= W) & (Y >= 0) & (Y <= H))
        lean = (wt * np.sin(phi)).sum(depth) / np.maximum(wt.sum(depth), 1e-9)
        prof.append(lean[idx])
        cache.append((d, phi, along, idx))
    hard, soft = _rim_sides(np.concatenate(prof), res)
    deltas, dists = [], []
    k0 = 0
    for d, phi, along, idx in cache:
        k1 = k0 + len(idx)
        # Back onto the edge's along axis (nearest value past the corners).
        n_al = phi.shape[along]
        full_h, full_s = np.empty(n_al), np.empty(n_al)
        full_h[idx], full_s[idx] = hard[k0:k1], soft[k0:k1]
        lo, hi = idx.min(), idx.max()
        full_h[:lo], full_h[hi + 1:] = full_h[lo], full_h[hi]
        full_s[:lo], full_s[hi + 1:] = full_s[lo], full_s[hi]
        s_hard = np.expand_dims(full_h, 1 - along)
        s = np.expand_dims(full_s, 1 - along)
        k0 = k1
        # Around a flip the pull (outward headings only) reaches further.
        wp = _smooth01(1 - d / (flow_mm * (1 + RIM_SQUARE_REACH * (1 - np.abs(s)))))
        w = _smooth01(1 - d / flow_mm)
        # Inward headings turn toward the rim side.  Headings within
        # STEER_NATURAL_DEG of parallel keep their own side, which also
        # masks each branch's unwrap cut at |φ| = 90° (for |φ| ≤ 90° the
        # branches agree, so λ is moot there).
        nat = _smooth01((np.pi / 2 + np.radians(STEER_NATURAL_DEG) - np.abs(phi))
                        / np.radians(STEER_NATURAL_DEG))
        lam = 0.5 * (1 + (1 - nat) * s_hard + nat * np.where(phi >= 0, 1.0, -1.0))
        # Around a flip (flows meeting or splitting at the wall) the pull
        # squares the flow up to the wall: it ends there in parallel
        # instead of funnelling into a V.
        pl = np.maximum(pull, RIM_SQUARE_PULL * (1 - np.abs(s)))
        r = lam * _turn(phi, 1.0, w, wp, ok, pl) + (1 - lam) * _turn(phi, -1.0, w, wp, ok, pl)
        # Run alongside near the wall, on the rim side; faded out around a flip.
        q = _wrap(phi + r)
        wa = _smooth01(1 - d / RIM_ALONG_MM) * np.abs(s)
        r = r + (np.sign(s) * ok - q) * wa
        deltas.append(r)
        dists.append(d)
    # Corners: a smooth nearest-edge partition of unity (far edges have Δ = 0).
    D = np.stack(dists)
    P = np.exp(-(D - D.min(0)) / STEER_CORNER_MM)
    delta = (np.stack(deltas) * P).sum(0) / P.sum(0)
    # Applied as a smoothed SCALAR rotation of the input angle: θ + Δ with
    # continuous Δ cannot create singularities (no starbursts).
    delta = ndimage.gaussian_filter(delta, STEER_SMOOTH_MM / res)
    return th + delta


def soil_relief(rng, m, X, Y, vox):
    """Soil sits lower than the grass, slightly rough, with sparse angular pebbles."""
    # Soft-edged trough: the dip and roughness use a blurred mask so no step
    # outlines the patch (the bank itself is drawn by frond outlines).
    ms = ndimage.gaussian_filter(m, SOIL_DIP_SOFT_MM / vox)
    relief = -SOIL_DIP_MM * ms + SOIL_ROUGH_MM * _smooth_noise(rng, X.shape, SOIL_ROUGH_SIGMA_MM / vox) * ms
    area = float((m > 0.5).sum()) * vox * vox
    n = int(rng.poisson(PEBBLE_DENSITY * area))
    cand = np.argwhere(m > 0.8)
    if n == 0 or len(cand) == 0:
        return relief, 0
    peb = np.zeros(X.shape)
    placed = []
    for idx in cand[rng.permutation(len(cand))[:n * 20]]:
        c = np.array([X[tuple(idx)], Y[tuple(idx)]])
        r = _skewed(rng, PEBBLE_R_MM)
        if any(np.hypot(*(c - q)) < PEBBLE_GAP_MM + r + rq for q, rq in placed):
            continue
        placed.append((c, r))
        # Rounded faceted pebble: a low dome (0.5–0.7 r, elongated 1–1.5×)
        # cut by 7–10 planes at 30–42°, so facets read as stone without flat
        # tops or prism walls.
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


# ── 5 Fronds ────────────────────────────────────────────────────────────────
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


def _rise(u, L, Lt):
    """Frond crest profile: flush at the base, full by 0.45 L, drooping to the tip."""
    up = _smooth01(np.asarray(u) / (0.45 * L))
    td = np.clip((np.asarray(u) - 0.55 * L) / (Lt - 0.55 * L), 0, 1)
    return up * (1 - DROOP * td * td * (3 - 2 * td))    # (operand order is load-bearing)


def make_strand_roots(rng, flow, W, H):
    """Frond roots along strands (REQ-FL-3): long streamlines, fronds shingled.

    Returns roots (M, 2), per-root frond lengths (before swath scaling) and
    the strand count.
    """
    m = STRAND_MARGIN_MM
    seeds = poisson_sites(rng, -m, -m, W + m, H + m, STRAND_SPACING_MM)
    n = len(seeds)
    slen = rng.uniform(*STRAND_LEN_MM, n)
    back = slen * rng.uniform(*STRAND_BACK_FRAC, n)
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


class Surface:
    """What the fronds read per grass column: ground H0 (with soil relief),
    the soil mask and distances, and optional hard obstacles."""

    def __init__(self, rng, grid, ground, lenses, blocker=None, tuck_mm=0.5,
                 side_low=1.0, side_mm=1.0):
        vox = grid.vox
        X, Y = np.meshgrid(grid.xc, grid.yc, indexing='ij')
        H0 = ground(X, Y)
        self.soil = lenses.mask(X, Y)
        rel, self.n_pebbles = soil_relief(rng, self.soil, X, Y, vox)
        self.H0 = H0 + rel
        self.grid = grid
        self.soil_depth = ndimage.distance_transform_edt(self.soil > 0.5) * vox
        self.soil_dist = ndimage.distance_transform_edt(self.soil <= 0.5) * vox  # grass → soil
        self.gsx, self.gsy = np.gradient(self.soil_dist, vox)
        # Optional hard obstacles (rock footprints, used by meadowlab): no
        # rooting inside; spines stop once `tuck_mm` inside (pressed against
        # the obstacle face).  None = grasslab's own behaviour.
        self.blocker, self.tuck_mm = blocker, tuck_mm
        self.blk_depth = (ndimage.distance_transform_edt(blocker) * vox
                          if blocker is not None else None)
        # Grass beside an obstacle is lower: frond height × side_low at the
        # face, back to full height side_mm out, so fronds passing a rock lap
        # against its foot instead of swallowing it.  1.0 = no change.
        self.near = None
        if blocker is not None and side_low < 1.0:
            out = ndimage.distance_transform_edt(~blocker) * vox
            self.near = side_low + (1 - side_low) * _smooth01(out / side_mm)
            # Inside the face (under the rock) it falls to bare ground, so
            # skirts of passing fronds cannot fill over a low rock.
            self.near *= 1 - _smooth01(self.blk_depth / SIDE_INNER_MM)

    def cell(self, px, py):
        g = self.grid
        return (min(max(int(px / g.vox), 0), g.nx - 1),
                min(max(int(py / g.vox), 0), g.ny - 1))


@dataclass
class FrondPlan:
    """Per-root data decided before sculpting."""
    roots: np.ndarray      # (N, 2)
    L0: np.ndarray         # strand frond lengths
    own: np.ndarray        # swath owner
    s: np.ndarray          # swath coords at the root
    l: np.ndarray
    ri: np.ndarray         # root cell
    rj: np.ndarray
    bank: np.ndarray       # rooted near (not in) soil
    paths: np.ndarray      # traced spines (N, n, 2)
    n_strands: int


def plan_fronds(rng, surf, sw, flow, W, H):
    """Roots along strands, bank flags, heading offsets, traced spines."""
    grid, vox = surf.grid, surf.grid.vox
    roots, L0, n_strands = make_strand_roots(rng, flow, W, H)
    keep = (roots[:, 0] >= 0) & (roots[:, 0] < W) & (roots[:, 1] >= 0) & (roots[:, 1] < H)
    roots, L0 = roots[keep], L0[keep]
    N = len(roots)
    own, a, b = sw.assign(roots[:, 0], roots[:, 1])
    s, l = sw.coords(own, a, b)
    ri = np.clip((roots[:, 0] / vox).astype(int), 0, grid.nx - 1)
    rj = np.clip((roots[:, 1] / vox).astype(int), 0, grid.ny - 1)
    bank = (surf.soil_dist[ri, rj] < BANK_REACH_MM) & (surf.soil[ri, rj] <= 0.5)
    stray = rng.random(N) < np.where(bank, BANK_STRAY_FRAC, STRAY_FRAC)
    delta = np.radians(np.where(stray, rng.choice([-1, 1], N) * rng.uniform(*STRAY_DELTA_DEG, N),
                                rng.uniform(-FROND_DELTA_DEG, FROND_DELTA_DEG, N))
                       + np.where(bank, rng.uniform(-BANK_WANDER_DEG, BANK_WANDER_DEG, N), 0.0))
    Lmax = FROND_LEN_MM[1] * 1.1 + FROND_TIP_HALF_W_MM + 0.5
    paths = trace_batch(roots, Lmax, flow, delta=delta)
    return FrondPlan(roots, L0, own, s, l, ri, rj, bank, paths, n_strands)


@dataclass
class Frond:
    """One frond's drawn dimensions after shortening."""
    f: int                 # index into the plan
    sprig: bool
    bank: bool
    L: float               # blade length (the tip cap adds FROND_TIP_HALF_W_MM)
    Lt: float              # total length
    wmax: float            # blade half-width
    wsk: float             # skirt half-width (0 for sprigs)
    hf: float              # blade height, × h_scale × k_h
    h_scale: float         # bank height scale
    k_h: float             # shortening height factor
    tip_soft: bool         # spine stopped by soil
    base_soft: bool        # rooted close to soil


def draw_frond(rng, plan, f, surf, W, H):
    """Draw frond f's dimensions and shorten it to fit (REQ-EDGE-2/3/5).

    Returns None when the frond is not built: rooted under an obstacle, a
    soil root that did not become a sprig, or shortened below FROND_MIN_LEN_MM.
    """
    vox, step, wt = surf.grid.vox, TRACE_STEP_MM, FROND_TIP_HALF_W_MM
    ri, rj, path = plan.ri[f], plan.rj[f], plan.paths[f]
    if surf.blocker is not None and surf.blocker[ri, rj]:
        return None
    sprig = surf.soil[ri, rj] > 0.5
    if sprig and rng.random() > SPRIG_PROB:
        return None
    if sprig:
        L = rng.uniform(*SPRIG_LEN_MM)
        wmax, hf, wsk = SPRIG_HALF_W_MM, SPRIG_H_MM, 0.0
    else:
        L = plan.L0[f] * (0.8 + 0.3 * plan.s[f])
        wmax = rng.uniform(*FROND_HALF_W_MM)
        hf = rng.uniform(*FROND_H_MM)
        wsk = SKIRT_FACTOR * wmax
    bank = bool(plan.bank[f]) and not sprig
    h_scale = rng.uniform(*BANK_H_SCALE) if bank else 1.0
    hf *= h_scale
    inset = _skewed(rng, RIM_TIP_INSET_MM)
    spill = WISP_SPILL_MM if rng.random() < WISP_FRAC else _skewed(rng, SOIL_SPILL_MM)

    # Shorten (never slice) so the tip ends inside the tile, at most `spill`
    # into bare soil, and at most tuck_mm into an obstacle.
    nL = min(int(np.ceil((L + wt) / step)) + 1, path.shape[0])
    cx, cy = path[:nL, 0], path[:nL, 1]
    uu = np.arange(nL) * step
    out = (cx < inset) | (cx > W - inset) | (cy < inset) | (cy > H - inset)
    ci = np.clip((cx / vox).astype(int), 0, surf.grid.nx - 1)
    cj = np.clip((cy / vox).astype(int), 0, surf.grid.ny - 1)
    deep = (surf.soil_depth[ci, cj] > spill) & ~sprig
    if surf.blk_depth is not None:
        out = out | (surf.blk_depth[ci, cj] > surf.tuck_mm)
    stop = np.nonzero(out | deep)[0]
    k_h = 1.0
    tip_soft = bool(len(stop)) and bool(deep[stop[0]])
    base_soft = (not sprig) and surf.soil_dist[ri, rj] < SOIL_BASE_SOFT_MM
    if len(stop):
        L0, L = L, uu[stop[0]] - wt
        if L < FROND_MIN_LEN_MM:
            return None
        k_h = float(np.clip(L / L0, 0.3, 1.0)) ** 0.8
        hf *= k_h
    return Frond(f, bool(sprig), bank, L, L + wt, wmax, wsk, hf, h_scale, k_h,
                 tip_soft, bool(base_soft))


def frond_spine(plan, fr):
    """Traced streamline to Lt + pad, plus a straight cap behind the root."""
    step, path, p0 = TRACE_STEP_MM, plan.paths[fr.f], plan.roots[fr.f]
    cb = SKIRT_BASE_CAP * fr.wsk
    nf = min(int(np.ceil((fr.Lt + SPINE_PAD_MM) / step)) + 1, path.shape[0])
    fwd = path[:nf:2]
    uf = np.arange(0, nf, 2) * step
    t0 = path[1] - path[0]
    t0 = t0 / max(np.linalg.norm(t0), 1e-9)
    ub = np.linspace(-cb - SPINE_PAD_MM, 0, 4)[:-1]
    return np.concatenate([p0 + ub[:, None] * t0, fwd]), np.concatenate([ub, uf])


def sculpt_frond(fr, plan, surf, sw, T, lips, ledges):
    """Rasterise one frond: skirt + blade into T; tip lip and bank side
    ledges as column intervals.  Returns False if it covers no column."""
    grid, step, wt = surf.grid, TRACE_STEP_MM, FROND_TIP_HALF_W_MM
    vox, f, path = grid.vox, fr.f, plan.paths[fr.f]
    i, sb, lb = plan.own[f], plan.s[f], plan.l[f]
    L, Lt, wmax, wsk, hf = fr.L, fr.Lt, fr.wmax, fr.wsk, fr.hf
    cb = SKIRT_BASE_CAP * wsk              # skirt extends behind the root
    u_c, u_h = (Lt - cb) / 2, (Lt + cb) / 2
    sp, su_ = frond_spine(plan, fr)

    reach = max(wmax, wsk)
    i0 = max(int((sp[:, 0].min() - reach) / vox), 0)
    i1 = min(int((sp[:, 0].max() + reach) / vox) + 2, grid.nx)
    j0 = max(int((sp[:, 1].min() - reach) / vox), 0)
    j1 = min(int((sp[:, 1].max() + reach) / vox) + 2, grid.ny)
    if i0 >= i1 or j0 >= j1:
        return False
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
    if fr.bank:
        tans = np.diff(path, axis=0)
        tans = tans / np.maximum(np.linalg.norm(tans, axis=1, keepdims=True), 1e-9)
        kk = np.clip((u / step).astype(int), 0, len(tans) - 1)
        sv = np.sign(v)
        ox, oy = -tans[kk, 1] * sv, tans[kk, 0] * sv
        facing = (((ox * -surf.gsx[I, J] + oy * -surf.gsy[I, J]) > 0.3)
                  & (surf.soil_dist[I, J] < BANK_FACE_MM))
        in_skirt &= ~(facing & (np.abs(v) >= w))   # nothing fills under the ledge
    side_all = SIDE_CARVE_ALL and not fr.sprig
    if side_all:
        # Every blade gets side ledges along both edges, except near its base,
        # so the whole blade is defined all round (shorelab).
        facing = facing | (u > SIDE_BASE_FRAC * L)
    inside = in_blade | in_skirt
    if not inside.any():
        return False
    I, J, u, v, w, ws, in_blade, facing = (I[inside], J[inside], u[inside], v[inside],
                                           w[inside], ws[inside], in_blade[inside],
                                           facing[inside])
    # Skirt: carries this swath's envelope (continued past its border),
    # falling off with the frond's own outline and dropping toward the tip.
    su = np.minimum(sb + np.clip(u, 0, None) / sw.length[i], 1.0)
    env = 0.0 if fr.sprig else fr.k_h * fr.h_scale * sw.env(np.full(len(u), i), su,
                                                             np.full(len(u), lb))
    # Height: plateau across the lens, falling off at the sides; along the
    # frond it ramps up over the rounded base cap and drops toward the tip
    # (soil-stopped fronds: to ~0 over the last SKIRT_END_MM) — a pointed,
    # lowering tip, never a "sawn log" face and never a rounded scallop.
    qs = np.clip(np.abs(v) / np.maximum(ws, 1e-6), 0, 1)
    taper = 1 - SKIRT_TIP_DROP * _smooth01((u - 0.5 * L) / (Lt - 0.5 * L))
    if fr.tip_soft:
        t_end = min(SKIRT_END_MM, 0.4 * Lt)
        taper = taper * (1 - _smooth01((u - (Lt - t_end)) / t_end))
    if fr.base_soft:
        taper = taper * _smooth01((u + cb) / max(1.5 * cb, 1e-6))
    hs = env * taper * (1 - qs ** 3) ** 1.5
    # Blade on top.
    q = np.clip(np.abs(v) / np.maximum(w, 1e-6), 0, 1)
    blade = np.where(in_blade, hf * _rise(u, L, Lt) * (1 - q ** 2) ** DOME_EXP, 0.0)
    if surf.near is not None:
        k = surf.near[I, J]
        hs, blade = hs * k, blade * k
    top = surf.H0[I, J] + hs + blade

    lip_len = min(LIP_MAX_MM, LIP_FRAC * L)
    lp = in_blade & (u >= Lt - lip_len)
    # Side ledge on the soil-facing edge of bank fronds: the outer
    # SIDE_LIP_W of the blade overhangs a recessed body with a 45°
    # underside, so stacked blades show undercut ridges (REQ-FL-6).
    e = w - np.abs(v)
    ledge = facing & in_blade & (e < SIDE_LIP_W_MM) & ~lp
    mass = ~lp & ~ledge
    T[I[mass], J[mass]] = smoothmax(T[I[mass], J[mass]], top[mass], SOFTMAX_K_MM)
    if ledge.any():
        # Underside: LIP_MIN_T below the ledge's own top at the edge,
        # descending inward at 45° — a shallow wedge just under the blade.
        entry = (I[ledge], J[ledge], top[ledge] - LIP_MIN_T_MM - e[ledge], top[ledge])
        ledges.append(entry + ((SIDE_GAP_MM,) if side_all else ()))
    if lp.any():
        # 45° underside measured back from the drooped tip crest.
        s_tip = min(sb + Lt / sw.length[i], 1.0)
        env_t = 0.0 if fr.sprig else \
            fr.k_h * sw.env(np.array([i]), np.array([s_tip]), np.array([lb]))[0]
        tp = path[min(int(Lt / step), path.shape[0] - 1)]
        g_t = float(surf.H0[surf.cell(tp[0], tp[1])])
        tip = g_t + (0.0 if fr.tip_soft else env_t * (1 - SKIRT_TIP_DROP)) \
            + hf * float(_rise(Lt, L, Lt))
        bot = tip - LIP_MIN_T_MM - (Lt - u[lp])
        keep = (top[lp] - bot) >= LIP_MIN_T_MM
        if keep.any():
            # TIP_CARVE: like bank ledges, carve the mass that neighbouring
            # skirts left under a visible tip, so every tip casts a shadow.
            if TIP_CARVE:
                ledges.append((I[lp][keep], J[lp][keep], bot[keep], top[lp][keep], TIP_GAP_MM))
            else:
                lips.append((I[lp][keep], J[lp][keep], bot[keep], top[lp][keep]))
        # Lip side columns too thin to undercut stay solid (fused blade
        # edge) instead of being dropped — dropping them sawtoothed tips.
        thin = np.nonzero(lp)[0][~keep]
        T[I[thin], J[thin]] = smoothmax(T[I[thin], J[thin]], top[thin], SOFTMAX_K_MM)
    return True


def open_ledges(T, ledges, lips):
    """Neighbouring skirts fill in under every ledge, so after all fronds are
    built, cut the mass beneath each VISIBLE ledge column (its top is the
    surface there) down to its 45° underside — a wedge under the blade
    edge, never a slot (REQ-FL-6, REQ-FDM-3)."""
    for I, J, bot, top, *gap in ledges:
        # Tips (gap given) count as visible even when a neighbour's skirt
        # covers them by up to TIP_VIS_TOL_MM — that skirt is cut back, so
        # the tip is exposed and stands on its own.
        tol = TIP_VIS_TOL_MM if gap else 0.05
        vis = top >= T[I, J] - tol
        if not vis.any():
            continue
        # Optional air gap under the ledge (tips): the floor is cut `gap`
        # below the 45° underside, so the tip stands as separate geometry.
        g_ = gap[0] if gap else 0.0
        T[I[vis], J[vis]] = np.minimum(T[I[vis], J[vis]], bot[vis] - g_)
        if gap:
            # the tip's own column top wins over the cut-back neighbour
            T[I[vis], J[vis]] = np.minimum(T[I[vis], J[vis]], bot[vis] - g_)
        lips.append((I[vis], J[vis], bot[vis], top[vis]))


@dataclass
class Grass:
    H0: np.ndarray         # ground incl. soil relief, on the grass grid
    T: np.ndarray          # mass map (height field)
    lips: list             # column intervals [(I, J, bottom, top), …]
    soil: np.ndarray       # soil mask
    n_fronds: int
    n_strands: int
    n_pebbles: int
    n_ledges: int          # bank fronds with side ledges
    gaps: float            # off-soil share with < 0.3 mm grass


def build_grass(rng, grid, ground, sw, lenses, flow, W, H, blocker=None, tuck_mm=0.5,
                side_low=1.0, side_mm=1.0):
    """The grass volume is MADE OF FRONDS (REQ-EDGE-1): T starts at the bare
    ground and every bit of grass height comes from some frond's own body —
    a broad skirt carrying its swath's envelope plus the domed blade on top.
    Fronds are streamlines of the flow (REQ-FL-1), gathered into strands.
    """
    surf = Surface(rng, grid, ground, lenses, blocker, tuck_mm, side_low, side_mm)
    plan = plan_fronds(rng, surf, sw, flow, W, H)
    T = surf.H0.copy()                     # no free-standing grass layer
    lips, ledges = [], []
    n_built = 0
    for f in range(len(plan.roots)):
        fr = draw_frond(rng, plan, f, surf, W, H)
        if fr is not None and sculpt_frond(fr, plan, surf, sw, T, lips, ledges):
            n_built += 1
    open_ledges(T, ledges, lips)
    T = np.maximum(T, surf.H0)
    gaps = float(((T - surf.H0) < 0.3)[surf.soil < 0.5].mean())
    return Grass(surf.H0, T, lips, surf.soil, n_built, plan.n_strands, surf.n_pebbles,
                 len(ledges), gaps)


# ── 6 Field, FDM backstop, mesh ─────────────────────────────────────────────
def make_field(grid, T, lips):
    """Continuous field: vertical signed distance, clamped to a narrow band."""
    vox = grid.vox
    top_z = max(T.max(), max((tp.max() for *_, tp in lips), default=0)) + 0.3
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


def to_manifold(mesh):
    return m3d.Manifold(mesh=m3d.Mesh(
        vert_properties=np.asarray(mesh.vertices, dtype=np.float32),
        tri_verts=np.asarray(mesh.faces, dtype=np.uint32)))


def mesh_field(F, vox):
    """Marching cubes on F → (grass Manifold, its max z).

    Field values ≈ 0 at grid nodes can make marching-cubes vertices
    coincide; trimesh's vertex merge then pinches edges and manifold3d
    rejects the mesh (an empty union downstream).  On that failure only,
    near-zero values are nudged off the level set and the field re-meshed,
    so meshes that were already valid are unchanged.
    """
    from skimage.measure import marching_cubes
    for _ in range(2):
        p = 2
        V = np.pad(F, p, mode='constant', constant_values=-2.0 * vox)
        verts, faces, _, _ = marching_cubes(V, 0.0, spacing=(vox, vox, vox))
        verts -= (p - 0.5) * vox
        mesh = trimesh.Trimesh(verts, faces, process=True)
        if mesh.volume < 0:
            mesh.invert()
        solid = to_manifold(mesh)
        if solid.status() == m3d.Error.NoError:
            return solid, float(mesh.vertices[:, 2].max())
        eps = 1e-3 * vox
        F = np.where(np.abs(F) < eps, np.where(F < 0, -eps, eps), F).astype(F.dtype)
    raise RuntimeError(f"grass mesh is not manifold: {solid.status()}")


# ── 7 Base, audit, self-check ───────────────────────────────────────────────
def attach_base(grass, cols, rows, tile_max_z):
    """grass ∪ DB base → simplify → keep the largest body.

    Returns (trimesh, peg height, (crumbs dropped, their volume mm³)).
    """
    from dharmatiles.core.config import BaseConfig, SurfaceConfig
    from dharmatiles.bases import dungeonblocks as db

    surface = SurfaceConfig(cols=cols, rows=rows, square_mm=SQUARE_MM)
    base_cfg = BaseConfig()
    peg_h = db.select_peg_height(np.zeros((2, 2)), base_cfg, tile_max_z)
    base = db.make_base(surface, peg_h, base_cfg)
    # Exact slab box under the marching-cubes solid: the base must meet a
    # clean planar face.  Meeting the MC bottom directly left 4 sliver edges
    # at the peg-groove apexes that collapse on STL reload (non-watertight).
    W, H = cols * SQUARE_MM, rows * SQUARE_MM
    slab = m3d.Manifold.cube([W, H, SLAB_BOX_MM])
    solid = (grass + slab + to_manifold(base)).simplify(SIMPLIFY_TOL_MM)
    parts = solid.decompose()
    vols = [q.volume() for q in parts]
    k = int(np.argmax(vols))
    crumbs = (len(parts) - 1, float(sum(vols) - vols[k]))
    out = parts[k].to_mesh()
    mesh = trimesh.Trimesh(out.vert_properties[:, :3], out.tri_verts, process=False)
    return mesh, peg_h, crumbs


def stl_safe(mesh):
    """Export guarantee: the mesh as a slicer will see it.

    STL stores float32 positions and readers merge vertices by position, so
    a mesh that is manifold in memory can open up on reload (e.g. zero-volume
    back-to-back "fins" a union leaves at a tangency, or two sheets pinched
    along an edge).  Separate pinches (dharmatiles.stone.solidify), apply
    that merge here, drop zero-volume components, and refuse to export
    anything that is not then a single watertight body.
    """
    from dharmatiles.stone.solidify import separate_pinches
    mesh = trimesh.Trimesh(mesh.vertices.copy(), np.asarray(mesh.faces), process=False)
    separate_pinches(mesh)              # coincident-but-unconnected vertices → apart
    v32 = np.asarray(mesh.vertices, dtype=np.float32).astype(np.float64)
    m = trimesh.Trimesh(v32, np.asarray(mesh.faces), process=True)
    parts = m.split(only_watertight=False)
    if len(parts) > 1:
        m = max(parts, key=lambda q: len(q.faces))
        m = trimesh.Trimesh(m.vertices, m.faces, process=True)
    if not m.is_watertight:
        raise RuntimeError('mesh is not watertight after STL-style merge')
    return m


def audit_overhang(mesh):
    """Area (mm²) and share of faces overhanging past OVERHANG_DEG + 3°."""
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
    ap.add_argument('--flat', action='store_true', help='flat ground (S1 study)')
    ap.add_argument('--vox', type=float, default=None)
    ap.add_argument('--check', type=pathlib.Path, default=None,
                    help='write quick monochrome self-check PNGs here')
    args = ap.parse_args()

    t0 = time.perf_counter()
    tm = {}
    rng = np.random.default_rng(args.seed)
    W, H = args.cols * SQUARE_MM, args.rows * SQUARE_MM
    vox_t = args.vox or (VOX_1X1_MM if args.cols * args.rows == 1 else VOX_MM)
    grid = Grid(W, H, W / round(W / vox_t))
    vox = grid.vox

    ground, gstats = make_ground(rng, W, H, args.flat)
    flow0 = make_direction(rng, ground)
    sw = Swaths(rng, W, H, flow0)
    lenses = place_lenses(rng, ground, flow0, W, H)
    flow = finish_flow(rng, flow0, lenses, W, H)
    tm['setup'] = time.perf_counter() - t0

    t = time.perf_counter()
    g = build_grass(rng, grid, ground, sw, lenses, flow, W, H)
    tm['fronds'] = time.perf_counter() - t

    t = time.perf_counter()
    F = make_field(grid, g.T, g.lips)
    added, removed = fdm_backstop(F, vox)
    tm['field+fdm'] = time.perf_counter() - t

    t = time.perf_counter()
    solid, zmax = mesh_field(F, vox)
    tm['mc'] = time.perf_counter() - t

    t = time.perf_counter()
    mesh, peg_h, crumbs = attach_base(solid, args.cols, args.rows, zmax)
    tm['base'] = time.perf_counter() - t

    occ = F > 0
    topz = (occ.shape[2] - np.argmax(occ[:, :, ::-1], axis=2)) * vox
    gh = topz - g.H0
    ov_area, ov_frac = audit_overhang(mesh)

    out_dir = pathlib.Path(__file__).parent / 'out'
    out_dir.mkdir(exist_ok=True)
    path = out_dir / (f"{args.cols}x{args.rows}-sculpted-grass"
                      f"{'-flat' if args.flat else ''}-s{args.seed}.stl")
    mesh = stl_safe(mesh)
    mesh.export(path)

    mean_top = float(topz.mean())
    band = DB_GRASS_BAND[0] <= mean_top <= DB_GRASS_BAND[1]
    print(f"→ {path}  ({path.stat().st_size / 1e6:.1f} MB)")
    print(f"  mesh: {len(mesh.vertices):,} verts, {len(mesh.faces):,} faces, "
          f"watertight={mesh.is_watertight}, peg={peg_h} mm, "
          f"crumbs dropped {crumbs[0]} ({crumbs[1]:.3f} mm³)")
    print(f"  vox {vox:.4f} mm; {len(sw.sites)} swath sites, {g.n_fronds} fronds, "
          f"{len(g.lips)} lips")
    if gstats:
        print(f"  ground {gstats['ground_min']:.2f}–{gstats['ground_max']:.2f} mm, "
              f"max slope {gstats['max_slope_deg']:.1f}°, usable {gstats['usable']:.0%}")
    print(f"  {g.n_strands} strands; bare soil: {len(lenses.centre)} lenses, "
          f"{(g.soil > 0.5).mean():.1%} of the tile, {g.n_pebbles} pebbles; gaps between "
          f"fronds (grass <0.3 mm, off-soil) {g.gaps:.1%}; {g.n_ledges} bank fronds with side ledges")
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
