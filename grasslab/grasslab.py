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
GROUND_EDGE_MM       = 4.2
HILL_SIGMA_LARGE_MM  = 10.0
HILL_SIGMA_MID_MM    = 5.0
HILL_MID_WEIGHT      = 0.35
MAX_GROUND_SLOPE_DEG = 34.0
USABLE_SLOPE_DEG     = 12.0
EDGE_MARGIN_MM       = 9.0
GROUND_FLOOR_MM      = 1.0

# direction field
COHERENCE_MM         = 25.0
THETA_STD_DEG        = 35.0
TERRAIN_FOLLOW       = 0.3

# swaths
SWATH_SPACING_MM     = 13.0
SWATH_LATERAL_ASPECT = 0.65       # lateral distances scaled by 1/aspect
SWATH_WARP_MM        = 2.5
SWATH_WARP_SIGMA_MM  = 3.0
SWATH_AMP_MM         = (1.2, 2.2)
ENV_FLOOR            = 0.25
ENV_EXP              = 1.6
ENV_LATERAL_EXP      = 0.6

# fronds
FROND_SPACING_MM     = 1.6
FROND_LEN_MM         = (5.0, 9.0)
FROND_HALF_W_MM      = (0.6, 1.0)
FROND_H_MM           = (0.7, 1.1)
FROND_TIP_HALF_W_MM  = 0.45
FAN_DEG              = 12.0
JITTER_DEG           = 4.0
CURVATURE_PER_MM     = 0.03
DOME_EXP             = 0.9
DROOP                = 0.35     # tip sinks to (1 − DROOP) of the frond crest
LIP_MAX_MM           = 2.0
LIP_FRAC             = 0.35
LIP_MIN_T_MM         = 0.6
SOFTMAX_K_MM         = 0.12

# bare soil: breaks in the grass along chosen swath seams (REQUIREMENTS §2.3a)
SOIL_SEAMS_PER_SQ    = (2.5, 3.0)  # opened seams per DB square
SOIL_MIN_SEAM_MM     = 5.0         # ignore seams shorter than this
SOIL_LANE_SEAMS      = 3           # seams chained into one winding lane
SOIL_LANE_SEP_MM     = 18.0        # new lanes start this far from existing ones
SOIL_MIN_HALF_W_MM   = 1.1         # narrower stretches close (no slits/cracks)
SOIL_HALF_W_MM       = 5.8         # max corridor half-width
SOIL_WIDTH_SIGMA_MM  = 3.5         # along-seam width modulation scale
SOIL_WIDTH_BIAS      = 0.9        # higher → more continuous corridors
SOIL_EDGE_MM         = 0.5         # bank transition width
SOIL_WOBBLE_MM       = 0.35        # bank-line roughness (amplitude)
SOIL_RIM_KEEP_MM     = 4.0         # no soil within this of the rim
SOIL_DIP_MM          = 0.4         # soil sits this far below H0
SOIL_DIP_SOFT_MM     = 0.8         # blur of the dip/roughness mask
SOIL_ROUGH_MM        = 0.08
SOIL_SPILL_MM        = 1.5         # frond tips reach at most this far into soil
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
RIM_TIP_INSET_MM     = 0.4         # fronds are shortened to end this far inside the rim
FROND_MIN_LEN_MM     = 1.5

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
    raw = raw - raw[inside].mean()
    raw *= HILL_RELIEF_MM / (raw[inside].max() - raw[inside].min())
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
    # Soft floor: keep the slab ≥ ~GROUND_FLOOR_MM thick over the base in the
    # deepest dips without flattening them (soft-max, k = 0.3 mm).
    h = smoothmax(h, np.full_like(h, GROUND_FLOOR_MM), 0.3)
    stats = dict(ground_min=float(h[inside].min()), ground_max=float(h[inside].max()),
                 max_slope_deg=float(np.degrees(np.arctan(slope[inside].max()))),
                 usable=float((slope[inside] <= np.tan(np.radians(USABLE_SLOPE_DEG))).mean()))
    return Field2D(h, -m, -m, res), stats


# ── Direction field ──────────────────────────────────────────────────────────
def make_direction(rng, ground):
    """Unit-vector components of a scalar angle field ⊕ bounded downhill pull.

    A continuous scalar angle has zero winding → no crowns or vortices; the
    downhill weight is ≤ 0.3 < 0.5, so the blend never vanishes.
    """
    h, res = ground.data, ground.res
    th = rng.uniform(0, 2 * np.pi) + np.radians(THETA_STD_DEG) * \
        _smooth_noise(rng, h.shape, COHERENCE_MM / res)
    gx, gy = np.gradient(h, res)
    slope = np.hypot(gx, gy)
    a = TERRAIN_FOLLOW * np.clip(slope / np.tan(np.radians(MAX_GROUND_SLOPE_DEG)), 0, 1)
    safe = np.maximum(slope, 1e-9)
    vx = (1 - a) * np.cos(th) + a * np.where(slope > 1e-9, -gx / safe, 0)
    vy = (1 - a) * np.sin(th) + a * np.where(slope > 1e-9, -gy / safe, 0)
    n = np.hypot(vx, vy)
    return (Field2D(vx / n, ground.x0, ground.y0, res),
            Field2D(vy / n, ground.x0, ground.y0, res))


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
def make_soil(rng, own, E, H0, X, Y, W, H, vox):
    """Bare-soil mask m ∈ [0, 1] as breaks in the grass along swath seams.

    A few seams between neighbouring swaths are opened — preferring low ground
    and seams where both envelopes are already low (lateral/upstream edges) —
    into corridors whose width is modulated along the seam so they break into
    a few winding, discontinuous exposures elongated with the flow.  The bank
    edge is sharp (SOIL_EDGE_MM): grass keeps its height up to it.
    """
    own = own.reshape(X.shape)
    n_sites = int(own.max()) + 1
    rim = np.minimum.reduce([X, W - X, Y, H - Y])
    pair = np.full(X.shape, -1, dtype=np.int64)
    for sl0, sl1 in (((slice(None, -1), slice(None)), (slice(1, None), slice(None))),
                     ((slice(None), slice(None, -1)), (slice(None), slice(1, None)))):
        a_, b_ = own[sl0], own[sl1]
        diff = a_ != b_
        pid = np.minimum(a_, b_) * n_sites + np.maximum(a_, b_)
        view = pair[sl0]
        view[diff] = pid[diff]
    on = (pair >= 0) & (rim > SOIL_RIM_KEEP_MM)
    ids, inv, cnt = np.unique(pair[on], return_inverse=True, return_counts=True)
    ok = cnt * vox >= SOIL_MIN_SEAM_MM
    if not ok.any():
        return np.zeros(X.shape), 0
    def zs(v):
        return (v - v.mean()) / (v.std() + 1e-9)
    h_mean = np.bincount(inv, H0[on]) / cnt
    e_mean = np.bincount(inv, E[on]) / cnt
    score = -zs(h_mean) - zs(e_mean) + rng.normal(0, 0.8, len(ids))
    score[~ok] = -np.inf
    n_sq = (W / SQUARE_MM) * (H / SQUARE_MM)
    k = max(1, int(round(rng.uniform(*SOIL_SEAMS_PER_SQ) * n_sq)))
    # Chain seams into winding lanes: seams (a,b) and (b,c) meet at a triple
    # junction when (a,c) is also a seam.  Start a lane at the best free seam
    # and extend it through junctions toward the best-scoring continuation.
    sc = dict(zip(ids.tolist(), score.tolist()))
    cen = {q: (np.bincount(inv, X[on])[n_] / cnt[n_], np.bincount(inv, Y[on])[n_] / cnt[n_])
           for n_, q in enumerate(ids.tolist())}
    def key(a_, b_):
        return min(a_, b_) * n_sites + max(a_, b_)
    chosen = []
    while len(chosen) < k:
        free = [q for q in sc if q not in chosen and np.isfinite(sc[q])
                and all(np.hypot(cen[q][0] - cen[c][0], cen[q][1] - cen[c][1])
                        >= SOIL_LANE_SEP_MM for c in chosen)]
        if not free:
            break
        lane = [max(free, key=sc.get)]
        while len(lane) < SOIL_LANE_SEAMS and len(chosen) + len(lane) < k:
            ends = set()
            for q in lane:
                ends |= {q // n_sites, q % n_sites}
            nxt = []
            for q in sc:
                if q in lane or q in chosen or not np.isfinite(sc[q]):
                    continue
                a_, b_ = q // n_sites, q % n_sites
                for q2 in lane:
                    c_, d_ = q2 // n_sites, q2 % n_sites
                    shared = {a_, b_} & {c_, d_}
                    if len(shared) == 1:
                        x_ = ({a_, b_} - shared).pop()
                        y_ = ({c_, d_} - shared).pop()
                        if key(x_, y_) in sc:
                            nxt.append(q)
                            break
            if not nxt:
                break
            lane.append(max(nxt, key=sc.get))
        chosen += lane
    chosen = np.array(chosen, dtype=np.int64)
    seam = on & np.isin(pair, chosen)
    dist, (bi, bj) = ndimage.distance_transform_edt(~seam, return_indices=True)
    dist = dist * vox
    nw = _smooth_noise(rng, X.shape, SOIL_WIDTH_SIGMA_MM / vox)[bi, bj]
    hw = SOIL_HALF_W_MM * np.clip((nw + SOIL_WIDTH_BIAS) / 1.3, 0, 1) ** 0.8
    # Close stretches too narrow to read as bare ground (they read as cracks).
    hw = np.where(hw < SOIL_MIN_HALF_W_MM, -1.0, hw)
    d_eff = dist + SOIL_WOBBLE_MM * _smooth_noise(rng, X.shape, 0.9 / vox)
    t = np.clip(0.5 + (hw - d_eff) / SOIL_EDGE_MM, 0, 1)
    m = t * t * (3 - 2 * t)
    t = np.clip((rim - SOIL_RIM_KEEP_MM) / 2.0, 0, 1)
    m *= t * t * (3 - 2 * t)
    return m, len(chosen)


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


def build_grass(rng, grid, ground, sw, W, H):
    """Return H0, mass map T, lip intervals [(I, J, bottom, top), …], soil, stats.

    The grass volume is MADE OF FRONDS (REQ-EDGE-1): T starts at the bare
    ground and every bit of grass height comes from some frond's own body —
    a broad skirt carrying its swath's envelope plus the domed blade on top.
    Where no frond grows there is no grass, so every boundary (soil banks,
    tile rim, swath borders) is the outline of the last individual fronds.
    """
    vox = grid.vox
    X, Y = np.meshgrid(grid.xc, grid.yc, indexing='ij')
    H0 = ground(X, Y)
    own, a, b = sw.assign(X.ravel(), Y.ravel())
    s, l = sw.coords(own, a, b)
    E = sw.env(own, s, l).reshape(X.shape)
    soil, n_seams = make_soil(rng, own, E, H0, X, Y, W, H, vox)
    rel, n_pebbles = soil_relief(rng, soil, X, Y, vox)
    H0 = H0 + rel
    T = H0.copy()                          # no free-standing grass layer
    soil_depth = ndimage.distance_transform_edt(soil > 0.5) * vox
    soil_dist = ndimage.distance_transform_edt(soil <= 0.5) * vox   # grass → soil

    def h0_at(px, py):
        ii = min(max(int(px / vox), 0), grid.nx - 1)
        jj = min(max(int(py / vox), 0), grid.ny - 1)
        return float(H0[ii, jj])

    lips = []
    m = FROND_LEN_MM[1]
    roots = poisson_sites(rng, -m, -m, W + m, H + m, FROND_SPACING_MM)
    r_own, r_a, r_b = sw.assign(roots[:, 0], roots[:, 1])
    r_s, r_l = sw.coords(r_own, r_a, r_b)
    wt = FROND_TIP_HALF_W_MM
    inset = RIM_TIP_INSET_MM
    n_built = 0
    for p0, i, sb, lb in zip(roots, r_own, r_s, r_l):
        in_tile = (0 <= p0[0] < W) and (0 <= p0[1] < H)
        if not in_tile:
            continue                       # rim = fronds that grow inside it
        sprig = soil[int(p0[0] / vox), int(p0[1] / vox)] > 0.5
        if sprig and rng.random() > SPRIG_PROB:
            continue
        if sprig:
            L = rng.uniform(*SPRIG_LEN_MM)
            wmax, hf, wsk = 0.5, 0.5, 0.0
        else:
            L = rng.uniform(*FROND_LEN_MM) * (0.75 + 0.4 * sb)
            wmax = rng.uniform(*FROND_HALF_W_MM)
            hf = rng.uniform(*FROND_H_MM)
            wsk = SKIRT_FACTOR * wmax
        ang = (np.arctan2(sw.d[i, 1], sw.d[i, 0])
               + np.radians(lb * FAN_DEG + rng.uniform(-JITTER_DEG, JITTER_DEG)))
        d = np.array([np.cos(ang), np.sin(ang)])
        pp = np.array([-d[1], d[0]])
        kap = rng.uniform(-CURVATURE_PER_MM, CURVATURE_PER_MM)

        # Shorten (never slice) fronds so their tips end inside the tile and
        # lap at most SOIL_SPILL_MM into bare soil — every frond keeps its
        # own tip shape (REQ-EDGE-2/3).
        uu = np.linspace(0, L + wt, 48)
        cx = p0[0] + uu * d[0] + kap * uu ** 2 * pp[0]
        cy = p0[1] + uu * d[1] + kap * uu ** 2 * pp[1]
        out = (cx < inset) | (cx > W - inset) | (cy < inset) | (cy > H - inset)
        ci = np.clip((cx / vox).astype(int), 0, grid.nx - 1)
        cj = np.clip((cy / vox).astype(int), 0, grid.ny - 1)
        deep = (soil_depth[ci, cj] > SOIL_SPILL_MM) & ~sprig
        stop = np.nonzero(out | deep)[0]
        k_h = 1.0
        # Bank fronds (cut by soil / rooted beside it) get soft ends; all
        # other fronds keep the accepted interior/rim shape unchanged.
        tip_soft = bool(len(stop)) and bool(deep[stop[0]])
        base_soft = (not sprig) and soil_dist[int(p0[0] / vox), int(p0[1] / vox)] < SOIL_BASE_SOFT_MM
        if len(stop):
            L0, L = L, uu[stop[0]] - wt
            if L < FROND_MIN_LEN_MM:
                continue
            # A frond cut short at a bank or the rim is also lower, so the
            # fringe steps down instead of ending in tall stubs.
            k_h = float(np.clip(L / L0, 0.3, 1.0)) ** 0.8
            hf *= k_h
        Lt = L + wt
        cb = SKIRT_BASE_CAP * wsk           # skirt extends behind the root
        u_c, u_h = (Lt - cb) / 2, (Lt + cb) / 2

        reach = max(wmax, wsk)
        uu = np.linspace(-cb, Lt, 14)
        cx = p0[0] + uu * d[0] + kap * uu ** 2 * pp[0]
        cy = p0[1] + uu * d[1] + kap * uu ** 2 * pp[1]
        i0 = max(int((cx.min() - reach) / vox), 0)
        i1 = min(int((cx.max() + reach) / vox) + 2, grid.nx)
        j0 = max(int((cy.min() - reach) / vox), 0)
        j1 = min(int((cy.max() + reach) / vox) + 2, grid.ny)
        if i0 >= i1 or j0 >= j1:
            continue
        I, J = np.meshgrid(np.arange(i0, i1), np.arange(j0, j1), indexing='ij')
        I, J = I.ravel(), J.ravel()
        dx, dy = grid.xc[I] - p0[0], grid.yc[J] - p0[1]
        u = dx * d[0] + dy * d[1]
        v = dx * pp[0] + dy * pp[1] - kap * np.clip(u, 0, None) ** 2
        # Blade planform (pointed lens, round tip cap).
        w = wt + (wmax - wt) * np.sin(np.pi * (0.25 + 0.75 * np.clip(u / L, 0, 1)))
        w = np.where(u > L, np.sqrt(np.clip(wt ** 2 - (u - L) ** 2, 0, None)), w)
        in_blade = (u >= 0) & (u <= Lt) & (np.abs(v) < w)
        # Skirt planform: a lens from −cb to Lt (rounded base, tapering tip).
        ws = wsk * np.clip(1 - ((u - u_c) / u_h) ** 2, 0, 1) ** 0.6
        in_skirt = (np.abs(v) < ws) & (u >= -cb) & (u <= Lt)
        inside = in_blade | in_skirt
        if not inside.any():
            continue
        I, J, u, v, w, ws, in_blade = (I[inside], J[inside], u[inside], v[inside],
                                       w[inside], ws[inside], in_blade[inside])
        # Skirt: carries this swath's envelope (continued past its border),
        # falling off with the frond's own outline and dropping toward the tip.
        su = np.minimum(sb + np.clip(u, 0, None) / sw.len[i], 1.0)
        env = 0.0 if sprig else k_h * sw.env(np.full(len(u), i), su, np.full(len(u), lb))
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
        mass = ~lp
        T[I[mass], J[mass]] = smoothmax(T[I[mass], J[mass]], top[mass], SOFTMAX_K_MM)
        n_built += 1
        if lp.any():
            # 45° underside measured back from the drooped tip crest.
            s_tip = min(sb + Lt / sw.len[i], 1.0)
            env_t = 0.0 if sprig else \
                k_h * sw.env(np.array([i]), np.array([s_tip]), np.array([lb]))[0]
            g_t = h0_at(p0[0] + Lt * d[0], p0[1] + Lt * d[1])
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
    gaps = float(((T - H0) < 0.3)[soil < 0.5].mean())
    return H0, T, lips, n_built, soil, (n_seams, n_pebbles, gaps)


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
    ux, uy = make_direction(rng, ground)
    sw = Swaths(rng, W, H, ux, uy)
    tm['setup'] = time.perf_counter() - t0

    t = time.perf_counter()
    H0, T, lips, n_fronds, soil, n_patches = build_grass(rng, grid, ground, sw, W, H)
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
    print(f"  bare soil: {n_patches[0]} seams opened, {(soil > 0.5).mean():.1%} of the tile, "
          f"{n_patches[1]} pebbles; gaps between fronds (grass <0.3 mm, off-soil) "
          f"{n_patches[2]:.1%}")
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
