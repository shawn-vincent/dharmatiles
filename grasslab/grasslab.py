#!/usr/bin/env python3
"""grasslab — flow-ribbon grass blades on hilly dirt, on a DungeonBlocks tile.

Implements grasslab/DESIGN.md:

  1 hills H0 (rises only) ─► 2 flow field θ (regional angle + in-phase
  meanders + downhill pull) ─► 3 swaths (strand lengths) ─► 4 soil lenses;
  flow wraps them, then rim steering ─► 5 fronds: strands of streamline-
  traced spines, drawn and shortened ─► 5c blades: print-native strokes
  (0.2 mm layers × whole 0.45 mm lines) smoothed by a loft, seated on H0
  ─► 6 dirt ground ∪ blades ─► 7 ∪ DB base ─► simplify ─► STL

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

import sys as _sys
_sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent / 'dirtlab'))
import dirtlab as _dirt   # noqa: E402  — the shared exposed-ground texture

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

# 5 strands and fronds (where blades go and how long they are)
STRAND_SPACING_MM    = 2.65
STRAND_LEN_MM        = (15.0, 30.0)
STRAND_BACK_FRAC     = (0.2, 0.6)  # strand starts this × its length upstream of its seed
STRAND_MARGIN_MM     = 8.0
STRAND_GAP_FRAC      = 0.5         # roots every this × frond length along a strand
STRAND_LATERAL_MM    = 0.3
TRACE_STEP_MM        = 0.25
FROND_LEN_MM         = (3.5, 6.5)  # × (0.8 + 0.3 s) along the swath
FROND_HALF_W_MM      = (0.8, 1.25) # nominal: sets the blade's width in lines
FROND_H_MM           = (1.5, 2.0)  # crest (tip) height above the ground
FROND_MIN_LEN_MM     = 1.5
FROND_DELTA_DEG      = 4.0         # heading jitter off the streamline
STRAY_FRAC           = 0.05
STRAY_DELTA_DEG      = (20.0, 35.0)
SPRIG_PROB           = 0.10        # chance a soil-rooted frond becomes a sprig
SPRIG_LEN_MM         = (1.8, 3.0)
SPRIG_HALF_W_MM      = 0.5
SPRIG_H_MM           = 0.6

# 5a blade body: print-native strokes (DESIGN.md §5a).  A blade is a stack of
# LAYER_MM layers, each a round-ended stroke along its spine a whole number
# of LINE_W_MM lines wide; then smoothed (a loft through the layer outlines).
LINE_W_MM            = 0.45        # extrusion line width (0.4 nozzle)
LAYER_MM             = 0.2         # print layer height
KEEL_LINES           = 2           # bottom layer width, in lines
TOP_LINES            = 1           # top layer width (the ridge), in lines
MAX_LINES            = (2, 4)      # blade body width range, in lines
LENS_EXP             = 0.6         # cross-section fullness (< 1 = fuller lens)
RAKE_MM              = 0.2         # each layer's front reaches this past the one below (≤ ½ line)
TIP_STEP_MM          = 0.45        # a layer's stroke loses one line every this toward each end
RISE_EXP             = 2.0         # crest 1 − (1 − u/L)^RISE_EXP: convex rise from the root
BLADE_VOX_MM         = 0.05        # blade distance-field voxel (resin resolution)
SMOOTH_CLOSE_MM      = 0.3         # layer outlines closed by this (fills line-step notches)
SMOOTH_SIG_MM        = 0.05        # final blur in plan…
SMOOTH_SIG_Z_MM      = 0.15        # …and across layers (rounds the loft's creases)
EMBED_MM             = 0.3         # blades reach this far below the ground
BLADE_FACES          = 1500        # quadric-decimated to this (≤ ~20 µm off: below resin XY)

# 5b rim and soil endings, banks
RIM_TIP_INSET_MM     = (0.2, 1.5)  # per-frond tip inset from the wall (U²)
SIDE_INNER_MM        = 0.8         # obstacles (meadowlab): frond height falls to 0 this far inside
SOIL_SPILL_MM        = (0.8, 3.2)  # per-frond spill into soil (U²)
WISP_FRAC            = 0.04        # fronds allowed WISP_SPILL_MM over soil
WISP_SPILL_MM        = 4.0
BANK_REACH_MM        = 4.0         # fronds rooted this close to soil are bank fronds
BANK_WANDER_DEG      = 15.0        # extra heading wander for bank fronds
BANK_STRAY_FRAC      = 0.12
BANK_H_SCALE         = (0.65, 1.15)

# 6–7 mesh, base, audit
SIMPLIFY_TOL_MM      = 0.005       # resin resolution
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
    Swaths group frond roots (strand lengths); frond directions come from the flow.
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


_POOL = None


def _pool():
    """Shared worker threads (numpy releases the GIL in its array loops)."""
    global _POOL
    if _POOL is None:
        import os
        from concurrent.futures import ThreadPoolExecutor
        _POOL = ThreadPoolExecutor(max_workers=min(os.cpu_count() or 1, 4))
    return _POOL


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
        ms = ndimage.gaussian_filter(self.soil, SOIL_DIP_SOFT_MM / vox)
        self.H0 = H0 - SOIL_DIP_MM * ms                  # blade seating (no texture)
        # Exposed ground everywhere = dirt (bare patches and between blades);
        # blades seat on plain H0, EMBED_MM deep.  Seed from the ground (no RNG).
        dseed = int(np.abs(ground.data).sum() * 1000) % (1 << 31)
        self.dirt = _dirt.dirt_relief(dseed, grid.nx * vox, grid.ny * vox)
        self.H0_dirt = self.H0 + self.dirt(X, Y)
        self.seat = Field2D(self.H0, grid.xc[0], grid.yc[0], vox)
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
        # Grass beside an obstacle is lower: blade height × side_low at the
        # face, back to full height side_mm out, so blades passing a rock lap
        # against its foot instead of swallowing it.  1.0 = no change.
        self.near = None
        if blocker is not None and side_low < 1.0:
            out = ndimage.distance_transform_edt(~blocker) * vox
            self.near = side_low + (1 - side_low) * _smooth01(out / side_mm)
            # Inside the face (under the rock) it falls to bare ground, so
            # passing blades cannot cover a low rock.
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
    Lmax = FROND_LEN_MM[1] * 1.1 + 0.5
    paths = trace_batch(roots, Lmax, flow, delta=delta)
    return FrondPlan(roots, L0, own, s, l, ri, rj, bank, paths, n_strands)


@dataclass
class Frond:
    """One frond's drawn dimensions after shortening."""
    f: int                 # index into the plan
    sprig: bool
    bank: bool
    L: float               # blade length along its spine
    half_w: float          # nominal half-width (→ width in lines)
    hf: float              # crest height (bank- and shortening-scaled)


def draw_frond(rng, plan, f, surf, W, H):
    """Draw frond f's dimensions and shorten it to fit (REQ-EDGE-2/3/5).

    Returns None when the frond is not built: rooted under an obstacle, a
    soil root that did not become a sprig, or shortened below FROND_MIN_LEN_MM.
    """
    vox, step = surf.grid.vox, TRACE_STEP_MM
    ri, rj, path = plan.ri[f], plan.rj[f], plan.paths[f]
    if surf.blocker is not None and surf.blocker[ri, rj]:
        return None
    sprig = surf.soil[ri, rj] > 0.5
    if sprig and rng.random() > SPRIG_PROB:
        return None
    if sprig:
        L = rng.uniform(*SPRIG_LEN_MM)
        wmax, hf = SPRIG_HALF_W_MM, SPRIG_H_MM
    else:
        L = plan.L0[f] * (0.8 + 0.3 * plan.s[f])
        wmax = rng.uniform(*FROND_HALF_W_MM)
        hf = rng.uniform(*FROND_H_MM)
    bank = bool(plan.bank[f]) and not sprig
    h_scale = rng.uniform(*BANK_H_SCALE) if bank else 1.0
    hf *= h_scale
    inset = _skewed(rng, RIM_TIP_INSET_MM)
    spill = WISP_SPILL_MM if rng.random() < WISP_FRAC else _skewed(rng, SOIL_SPILL_MM)

    # Shorten (never slice) so the tip ends inside the tile, at most `spill`
    # into bare soil, and at most tuck_mm into an obstacle.
    nL = min(int(np.ceil(L / step)) + 1, path.shape[0])
    cx, cy = path[:nL, 0], path[:nL, 1]
    uu = np.arange(nL) * step
    out = (cx < inset) | (cx > W - inset) | (cy < inset) | (cy > H - inset)
    ci = np.clip((cx / vox).astype(int), 0, surf.grid.nx - 1)
    cj = np.clip((cy / vox).astype(int), 0, surf.grid.ny - 1)
    deep = (surf.soil_depth[ci, cj] > spill) & ~sprig
    if surf.blk_depth is not None:
        out = out | (surf.blk_depth[ci, cj] > surf.tuck_mm)
    stop = np.nonzero(out | deep)[0]
    if len(stop):
        L0, L = L, uu[stop[0]]
        if L < FROND_MIN_LEN_MM:
            return None
        hf *= float(np.clip(L / L0, 0.3, 1.0)) ** 0.8
    return Frond(f, bool(sprig), bank, L, wmax, hf)


# ── 5c Blades: print-native strokes, smoothed ───────────────────────────────
def blade_layers(path, L, half_w, height):
    """One blade as [(layer k, shapely polygon)]: spine `path` (sampled every
    TRACE_STEP_MM from the root), length L, nominal half-width, crest height.

    Each layer is a stroke along the spine, a whole number of LINE_W_MM lines
    wide (REQ-B-1):
      * cross-section: a lens of line counts — KEEL_LINES at the bottom,
        widest in the middle, TOP_LINES on top — growing ≤ 1 line per layer;
      * profile: a convex rise — each layer starts where the crest
        1 − (1 − u/L)^RISE_EXP reaches it;
      * tip: only the top layer reaches the tip; each layer below ends
        RAKE_MM shorter (the raked bow / undercut, to the last layer);
      * ends: within a layer the stroke loses one line every TIP_STEP_MM
        toward each end (points made of line ends).
    """
    from shapely.geometry import LineString
    from shapely.ops import unary_union
    N = max(int(round(height / LAYER_MM)), 2)
    n_max = int(np.clip(round(2 * half_w / LINE_W_MM), *MAX_LINES))
    n = [max(1, int(round(n_max * np.sin(np.pi * (k + 0.5) / N) ** LENS_EXP)))
         for k in range(N)]
    n[0] = max(n[0], KEEL_LINES)
    n[-1] = TOP_LINES
    for k in range(1, N):
        n[k] = min(n[k], n[k - 1] + 1)
    us = np.arange(len(path)) * TRACE_STEP_MM

    def seg(u0, u1):
        m = (us > u0) & (us < u1)
        a = [np.interp(u0, us, path[:, 0]), np.interp(u0, us, path[:, 1])]
        b = [np.interp(u1, us, path[:, 0]), np.interp(u1, us, path[:, 1])]
        return LineString(np.vstack([a, path[m], b]))

    out = []
    for k in range(N):
        u_back = L * (1 - (1 - (k + 0.5) / N) ** (1 / RISE_EXP))
        u_front = L - (N - 1 - k) * RAKE_MM
        parts = []
        for j in range(n[k]):                       # n[k] lines → 1 line
            u0 = u_back + (n[k] - 1 - j) * TIP_STEP_MM
            u1 = u_front - (n[k] - 1 - j) * TIP_STEP_MM
            if u1 - u0 > 1e-3:
                parts.append(seg(u0, u1).buffer((n[k] - j) * LINE_W_MM / 2, quad_segs=8))
        if parts:
            out.append((k, unary_union(parts)))
    return out


def blade_mesh(layers, seat):
    """Smooth closed mesh of one blade, seated on the ground `seat`(x, y).

    A loft through the layer outlines in height-above-ground h (REQ-B-2): each
    outline, closed by SMOOTH_CLOSE_MM, becomes a 2-D signed distance field;
    between layer centres the field blends linearly, so the surface runs
    straight from one outline to the next (no layer steps) while the plan
    shape — outline, keel, point — is kept.  Above the top layer it closes
    over half a layer (a rounded ridge); below the bottom layer it continues
    EMBED_MM into the ground.  A light blur rounds the loft's creases, then
    marching cubes; vertices are lifted by the ground height under them;
    quadric decimation to BLADE_FACES.  Returns a trimesh, or None.
    """
    import shapely
    from shapely.ops import unary_union
    from skimage.measure import marching_cubes
    if not layers:
        return None
    v = BLADE_VOX_MM
    P = int(np.ceil(4 * SMOOTH_SIG_Z_MM / v)) + 3
    polys = {k: unary_union([p]).buffer(SMOOTH_CLOSE_MM, quad_segs=16)
             .buffer(-SMOOTH_CLOSE_MM, quad_segs=16) for k, p in layers}
    x0, y0, x1, y1 = unary_union(list(polys.values())).bounds
    pad = LINE_W_MM
    xs = np.arange(x0 - pad, x1 + pad, v) + v / 2
    ys = np.arange(y0 - pad, y1 + pad, v) + v / 2
    N = max(polys) + 1
    hs = np.arange(-EMBED_MM - 2 * v, (N + 1) * LAYER_MM, v) + v / 2
    X, Y = np.meshgrid(xs, ys, indexing='ij')
    D = []
    for k in range(N):
        m = shapely.contains_xy(polys[k], X, Y) if k in polys else np.zeros(X.shape, bool)
        D.append((ndimage.distance_transform_edt(m) - ndimage.distance_transform_edt(~m)) * v)
    close = LINE_W_MM / LAYER_MM              # field closes ½ line over ½ layer above the top
    sd = np.empty((len(xs), len(ys), len(hs)), np.float32)
    for iz, h in enumerate(hs):
        t = h / LAYER_MM - 0.5                  # in layer-centre units
        if t <= 0:
            f = np.minimum(D[0], h + EMBED_MM)
        elif t >= N - 1:
            f = D[-1] - close * (t - (N - 1)) * LAYER_MM
        else:
            k0 = int(t)
            f = D[k0] + (t - k0) * (D[k0 + 1] - D[k0])
        sd[:, :, iz] = f
    sd = np.pad(sd, P, constant_values=-1.0)
    sd = ndimage.gaussian_filter(sd, (SMOOTH_SIG_MM / v, SMOOTH_SIG_MM / v, SMOOTH_SIG_Z_MM / v))
    if sd.max() <= 0:
        return None
    verts, faces, _, _ = marching_cubes(sd, 0.0, spacing=(v, v, v))
    verts += [xs[0] - P * v, ys[0] - P * v, hs[0] - P * v]
    verts[:, 2] += seat(verts[:, 0], verts[:, 1])
    mesh = trimesh.Trimesh(verts, faces, process=True)
    if mesh.volume < 0:
        mesh.invert()
    if len(mesh.faces) > BLADE_FACES:
        mesh = mesh.simplify_quadric_decimation(face_count=BLADE_FACES)
    return mesh


def build_blade(fr, plan, surf):
    """Frond → (blade trimesh or None, its layers).  Height × the obstacle
    side factor (meadowlab) at its tip, where it presses against a rock."""
    path = plan.paths[fr.f]
    h = fr.hf
    if surf.near is not None:
        tip = path[min(int(fr.L / TRACE_STEP_MM), len(path) - 1)]
        h *= float(surf.near[surf.cell(tip[0], tip[1])])
    if h < 2 * LAYER_MM:
        return None, []
    layers = blade_layers(path, fr.L, fr.half_w, h)
    return blade_mesh(layers, surf.seat), layers


@dataclass
class Grass:
    H0: np.ndarray         # blade seating ground, on the grass grid
    T: np.ndarray          # grass top height map (ground where no blade)
    solid: object          # Manifold: dirt ground (from z = 0) ∪ every blade
    soil: np.ndarray       # soil mask
    n_fronds: int
    n_strands: int
    gaps: float            # off-soil share of ground with no blade over it


def ground_solid(surf, W, H):
    """The dirt ground as a watertight heightfield solid from z = 0."""
    from dharmatiles.core.mesh import make_heightmap_solid
    return to_manifold(make_heightmap_solid(surf.H0_dirt.T, W, H, base_h=0.0))


def build_grass(rng, grid, ground, sw, lenses, flow, W, H, blocker=None, tuck_mm=0.5,
                side_low=1.0, side_mm=1.0):
    """Grass = separate blades standing on the dirt (REQ-EDGE-1, REQ-B-*).
    Blades follow streamlines of the flow (REQ-FL-1), gathered into strands;
    each is built from print-native strokes and smoothed (blade_layers,
    blade_mesh).  Drawing uses the RNG; meshing does not, so blades are
    meshed on worker threads."""
    import shapely
    surf = Surface(rng, grid, ground, lenses, blocker, tuck_mm, side_low, side_mm)
    plan = plan_fronds(rng, surf, sw, flow, W, H)
    frs = [draw_frond(rng, plan, f, surf, W, H) for f in range(len(plan.roots))]
    frs = [fr for fr in frs if fr is not None]
    built = list(_pool().map(lambda fr: build_blade(fr, plan, surf), frs))
    # Grass top map (for seating rocks, the DB band check and gap stats).
    T = surf.H0_dirt.copy()
    covered = np.zeros(T.shape, bool)
    X, Y = np.meshgrid(grid.xc, grid.yc, indexing='ij')
    for _, layers in built:
        for k, poly in layers:
            x0, y0, x1, y1 = poly.bounds
            i0, i1 = max(int(x0 / grid.vox), 0), min(int(x1 / grid.vox) + 2, grid.nx)
            j0, j1 = max(int(y0 / grid.vox), 0), min(int(y1 / grid.vox) + 2, grid.ny)
            if i0 >= i1 or j0 >= j1:
                continue
            m = shapely.contains_xy(poly, X[i0:i1, j0:j1], Y[i0:i1, j0:j1])
            sub = T[i0:i1, j0:j1]
            sub[m] = np.maximum(sub[m], surf.H0[i0:i1, j0:j1][m] + (k + 1) * LAYER_MM)
            covered[i0:i1, j0:j1] |= m
    meshes = [to_manifold(m) for m, _ in built if m is not None]
    solid = m3d.Manifold.batch_boolean([ground_solid(surf, W, H)] + meshes, m3d.OpType.Add)
    gaps = float((~covered)[surf.soil < 0.5].mean())
    return Grass(surf.H0, T, solid, surf.soil, len(meshes), plan.n_strands, gaps)


def to_manifold(mesh, original_id=None):
    """trimesh → Manifold.  `original_id` (from Manifold.reserve_ids) tags the
    mesh with a pre-reserved ID instead of the next free one, so a mesh built
    on a worker thread keeps the ID — hence the triangle order of every later
    boolean — that it would have had if built in sequence."""
    tv = np.asarray(mesh.faces, dtype=np.uint32)
    kw = {} if original_id is None else dict(
        run_index=np.array([0, tv.size], np.uint32),
        run_original_id=np.array([original_id], np.uint32))
    return m3d.Manifold(mesh=m3d.Mesh(
        vert_properties=np.asarray(mesh.vertices, dtype=np.float32), tri_verts=tv, **kw))


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


def _separate_pinches(mesh):
    """Same result as dharmatiles.stone.solidify.separate_pinches (see there),
    but fast on big meshes: the close pairs are found first and only they are
    checked for adjacency (the reference builds a Python set of every edge,
    ~1 s on a tile), and vertex normals are computed only if a pair exists.
    Same pairs, same order, same nudges → identical vertices."""
    from scipy.spatial import cKDTree
    v = mesh.vertices.view(np.ndarray)
    n = len(v)
    vn = edge_keys = None
    for _ in range(4):
        pairs = cKDTree(v).query_pairs(2e-4, output_type='ndarray')
        if not len(pairs):
            break
        if vn is None:
            vn = np.asarray(mesh.vertex_normals)
            f = mesh.faces.view(np.ndarray).astype(np.int64)
            e = np.sort(f[:, [0, 1, 1, 2, 2, 0]].reshape(-1, 2), axis=1)   # every face edge
            edge_keys = np.sort(e[:, 0] * n + e[:, 1])
        q = pairs.min(1).astype(np.int64) * n + pairs.max(1).astype(np.int64)
        k = np.minimum(np.searchsorted(edge_keys, q), len(edge_keys) - 1)
        adj = edge_keys[k] == q
        moved = False
        for (i, j), a in zip(pairs, adj):
            if a and float(np.linalg.norm(v[i] - v[j])) > 2e-5:
                continue                     # connected and not a sliver: leave it
            v[i] -= vn[i] * 1.5e-4
            v[j] -= vn[j] * 3.0e-4
            moved = True
        if not moved:
            break
    mesh.vertices = v


def stl_safe(mesh):
    """Export guarantee: the mesh as a slicer will see it.

    STL stores float32 positions and readers merge vertices by position, so
    a mesh that is manifold in memory can open up on reload (e.g. zero-volume
    back-to-back "fins" a union leaves at a tangency, or two sheets pinched
    along an edge).  Separate pinches (as dharmatiles.stone.solidify), apply
    that merge here, drop zero-volume components, and refuse to export
    anything that is not then a single watertight body.
    """
    mesh = trimesh.Trimesh(mesh.vertices.copy(), np.asarray(mesh.faces), process=False)
    _separate_pinches(mesh)             # coincident-but-unconnected vertices → apart
    v32 = np.asarray(mesh.vertices, dtype=np.float32).astype(np.float64)
    m = trimesh.Trimesh(v32, np.asarray(mesh.faces), process=True)
    # Count components first (what Trimesh.split does); only split if > 1.
    comps = trimesh.graph.connected_components(
        edges=m.face_adjacency, nodes=np.arange(len(m.faces)), min_len=1)
    if len(comps) > 1:
        parts = m.split(only_watertight=False)
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
    tm['blades'] = time.perf_counter() - t

    t = time.perf_counter()
    mesh, peg_h, crumbs = attach_base(g.solid, args.cols, args.rows, g.solid.bounding_box()[5])
    tm['base'] = time.perf_counter() - t

    topz = g.T
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
    print(f"  grid {vox:.4f} mm, blade voxel {BLADE_VOX_MM} mm; {len(sw.sites)} swath sites, "
          f"{g.n_fronds} blades")
    if gstats:
        print(f"  ground {gstats['ground_min']:.2f}–{gstats['ground_max']:.2f} mm, "
              f"max slope {gstats['max_slope_deg']:.1f}°, usable {gstats['usable']:.0%}")
    print(f"  {g.n_strands} strands; bare soil: {len(lenses.centre)} lenses, "
          f"{(g.soil > 0.5).mean():.1%} of the tile; dirt showing between blades "
          f"(off-soil) {g.gaps:.1%}")
    print(f"  grass above ground: mean {gh.mean():.2f}, p95 {np.percentile(gh, 95):.2f}, "
          f"max {gh.max():.2f} mm")
    print(f"  mean grass top {mean_top:.2f} mm above datum "
          f"(DB band {DB_GRASS_BAND[0]}–{DB_GRASS_BAND[1]}: {'OK' if band else 'OUT'})")
    print(f"  >{OVERHANG_DEG:.0f}°(+3) overhang {ov_area:.1f} mm² ({ov_frac:.2%})")
    print("  timing: " + ", ".join(f"{k} {v:.1f}s" for k, v in tm.items())
          + f"; total {time.perf_counter() - t0:.1f}s")
    if args.check:
        self_check(mesh, args.check, W, H)
        print(f"  self-check → {args.check}")


if __name__ == '__main__':
    main()
