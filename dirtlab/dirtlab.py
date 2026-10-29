#!/usr/bin/env python3
"""dirtlab — plain bare earth (dirt texture and pebbles) on a DungeonBlocks tile.

Implements dirtlab/DESIGN.md.  One height field, built in scale order:

  1 ground   rises-only broad undulation from a fixed edge level
  2 clods    patchy, irregular rounded soil lumps (soft max onto the ground)
  3 grit     fine low-amplitude fBm + sparse sand grains
  4 pebbles  clustered faceted half-sunk stones in the height field
             (small 0.5–1.5 mm, medium 2–4 mm), soft soil fillet at the foot
  5 stones   a very few larger rocklab rocks, bedded in a soil mound
  (6 cracks  optional, off by default)

then uniform heightfield solid ∪ exact slab ∪ rocks ∪ DB base → STL.

Usage:
    python dirtlab/dirtlab.py [--seed N] [--cracks]
    python dirtlab/dirtlab.py --sheet --seeds 1 2 3     # timestamped contact sheet
"""
from __future__ import annotations

import argparse
import datetime
import pathlib
import sys
import tempfile
import time
from dataclasses import dataclass, field

import manifold3d as m3d
import numpy as np
import trimesh
from scipy import ndimage

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'rocklab'))
import rocklab as rl     # noqa: E402

from dharmatiles.stone.noise import fbm   # noqa: E402

# ── Parameters (DESIGN.md §3) ────────────────────────────────────────────────
# tile
SQUARE_MM        = 35.0
CELL_MM          = 0.15          # height-field / mesh resolution (uniform mesh)
EDGE_MM          = 4.0           # tile-edge ground level above the datum (DB dirt band)
LIFT_MM          = 0.3           # interior base lift, so texture never dips below the edge
SLAB_BOX_MM      = 1.0           # exact box under the height-field solid
RIM_FLAT_MM      = 0.4           # flat band at the very edge (tiles butt cleanly)
RIM_TEX_MM       = 2.0           # texture / lift ramps in over this width

# 1 ground undulation (rises only)
UND_SIGMA_MM     = (10.0, 4.0)   # broad + mid noise scales
UND_MID_WEIGHT   = 0.35
UND_PLAIN_FRAC   = 0.35          # share of the interior left at base level
UND_RELIEF_MM    = (0.9, 1.6)    # peak rise, drawn per tile
UND_EDGE_MM      = 6.0           # undulation fades to zero over this rim width
UND_MAX_SLOPE_DEG = 14.0

# 2 clods
CLOD_PATCH_SIGMA_MM = 6.0        # patchiness field scale
CLOD_PATCH_BIAS  = 0.15          # >0: more cloddy area
CLOD_DENSITY     = 0.7           # attempts per mm² at full patch density
CLOD_R_MM        = (0.35, 1.5)   # radius, skewed small
CLOD_R_EXP       = 2.2
CLOD_H_RATIO     = (0.12, 0.3)   # height / radius
CLOD_H_MAX_MM    = 0.45
CLOD_ASPECT      = (1.0, 1.7)
CLOD_LUMP        = 0.22          # angular radius harmonics (irregular outline)
CLOD_OVERLAP     = 0.6           # min centre distance = this × (r1 + r2)
CLOD_SOFT_MM     = 0.06          # soft-max blend into the ground / each other
CLOD_SUNK        = (0.0, 0.5)    # each clod's dome starts this × h below the ground
CLOD_LOBES       = (1, 3)
CLOD_FLAT        = 2.6           # profile exponent: 2 = dome, higher = flatter top
CLOD_TILT        = 0.05          # σ of the top tilt (mm/mm)
SOIL_MID_MM      = 2.2           # low, broad soil roughness under the clods
SOIL_MID_AMP_MM  = 0.07

# 3 grit
GRIT_MM          = 0.45
GRIT_AMP_MM      = 0.045
GRAIN_DENSITY    = 0.12          # sand grains per mm²
GRAIN_R_MM       = (0.15, 0.3)
GRAIN_H_MM       = (0.05, 0.12)

# 4 pebbles (height field)
PEB_CLUSTERS_2X2 = (4, 7)        # loose clusters on a 2×2 tile
PEB_CLUSTER_SIGMA_MM = (3.0, 7.0)
PEB_BACKGROUND   = 0.25          # cluster field floor (some pebbles anywhere)
PEB_LOW_BIAS     = 0.8           # pebbles prefer low ground (washed into hollows)
SMALL_PER_SQ     = (45, 75)      # 0.5–1.5 mm pebbles per square
SMALL_D_MM       = (0.5, 1.5)
MED_PER_SQ       = (5, 9)        # 2–4 mm pebbles per square
MED_D_MM         = (2.0, 4.0)
PEB_D_EXP        = 1.8           # skew toward small
PEB_ASPECT       = (1.0, 1.9)
PEB_H_RATIO      = (0.25, 0.45)  # standing height / width (before sinking)
PEB_SINK         = (0.25, 0.45)  # share of height buried
PEB_ROUND_PROB   = 0.25          # rounded (worn) vs faceted (angular)
PEB_FACETS       = (6, 11)       # medium pebbles: facet planes
PEB_FACETS_SMALL = (4, 7)
PEB_FACET_DROP   = (0.03, 0.18)  # tangent plane lowered by this × height
PEB_ROUND_DROP   = (0.06, 0.15)
PEB_MAX_FACET_DEG = 65.0
PEB_SKIRT_DEG    = 58.0
PEB_SKIRT_SIDES  = (5, 8)
PEB_FACET_TILT_DEG = 12.0       # σ of each facet plane's tilt off the tangent
PEB_FLAT         = (2.0, 3.5)    # cap profile exponent: 2 = ellipsoid, higher = slab-like
PEB_GAP_MM       = 0.25
PEB_SOFT_MM      = (0.08, 0.22)  # soil fillet at the foot (soft max), small → medium
PEB_MOUND        = 0.2           # medium pebbles: soil heaped round the foot (× height)

# 5 stones (rocklab rocks)
STONES_2X2       = (2, 4)
STONE_D_MM       = (4.5, 7.5)
STONE_SINK       = 0.4           # share of height buried (plus 0.25 mm below the lowest ground)
STONE_Z_SCALE    = 0.65          # rocklab rocks are boulders; flatten to bedded stones
STONE_MOUND_MM   = 0.3           # soil bank around each stone
STONE_RIM_MM     = 2.0
STONE_GAP_MM     = 4.0

# 6 cracks (off by default)
CRACKS_2X2       = 5
CRACK_W_MM       = 0.35
CRACK_DEPTH_MM   = 0.25

OVERHANG_DEG     = 45.0


# ── Helpers ──────────────────────────────────────────────────────────────────
def smooth01(t):
    t = np.clip(t, 0, 1)
    return t * t * (3 - 2 * t)


def smax(a, b, k):
    """Polynomial smooth max: a soft fillet of width ~k where a and b meet."""
    if np.isscalar(k) and k <= 0:
        return np.maximum(a, b)
    h = np.clip(0.5 + 0.5 * (b - a) / k, 0, 1)
    return a * (1 - h) + b * h + k * h * (1 - h)


def noise(rng, shape, sigma_px):
    n = ndimage.gaussian_filter(rng.normal(size=shape), sigma_px, mode='reflect')
    return n / max(n.std(), 1e-12)


def skewed(rng, lo_hi, exp):
    lo, hi = lo_hi
    return lo + (hi - lo) * rng.random() ** exp


@dataclass
class Field:
    W: float
    H: float
    x: np.ndarray                 # (nx,)
    y: np.ndarray                 # (ny,)
    X: np.ndarray                 # (ny, nx) — z[row = y, col = x]
    Y: np.ndarray
    dist: np.ndarray              # distance to the tile edge
    z: np.ndarray = None
    pebble: np.ndarray = None     # bool: cell belongs to a stone (paint)
    stats: dict = field(default_factory=dict)

    def window(self, cx, cy, r):
        """Index slices of the cells within r of (cx, cy) (clipped)."""
        ia, ib = max(int((cx - r) / CELL_MM), 0), min(int((cx + r) / CELL_MM) + 2, len(self.x))
        ja, jb = max(int((cy - r) / CELL_MM), 0), min(int((cy + r) / CELL_MM) + 2, len(self.y))
        return slice(ja, jb), slice(ia, ib)


def make_field(W, H):
    nx, ny = int(round(W / CELL_MM)) + 1, int(round(H / CELL_MM)) + 1
    x, y = np.linspace(0, W, nx), np.linspace(0, H, ny)
    X, Y = np.meshgrid(x, y)
    dist = np.minimum.reduce([X, W - X, Y, H - Y])
    return Field(W, H, x, y, X, Y, dist)


# ── 1 Ground ─────────────────────────────────────────────────────────────────
def ground_undulation(rng, f):
    """Rises-only broad undulation (≥ 0), zero on the rim."""
    sh = f.X.shape
    raw = noise(rng, sh, UND_SIGMA_MM[0] / CELL_MM) + UND_MID_WEIGHT * noise(rng, sh, UND_SIGMA_MM[1] / CELL_MM)
    inside = f.dist > UND_EDGE_MM
    raw -= np.quantile(raw[inside], UND_PLAIN_FRAC)
    sd = raw[inside].std()
    raw = smax(raw, np.zeros_like(raw), 0.6 * sd)
    raw -= raw[inside].min()
    raw *= rng.uniform(*UND_RELIEF_MM) / max(raw[inside].max(), 1e-9)
    w = 0.5 - 0.5 * np.cos(np.pi * np.clip((f.dist - RIM_FLAT_MM) / UND_EDGE_MM, 0, 1))
    u = raw * w
    gy, gx = np.gradient(u, CELL_MM)
    smax_ = np.tan(np.radians(UND_MAX_SLOPE_DEG))
    s = np.hypot(gx, gy).max()
    if s > smax_:
        u *= smax_ / s
    return u


# ── 2 Clods ──────────────────────────────────────────────────────────────────
def clod_layer(rng, f, base):
    """Irregular rounded soil lumps, patchy, soft-maxed onto *base*.
    Returns (new surface, clod count)."""
    patch = noise(rng, f.X.shape, CLOD_PATCH_SIGMA_MM / CELL_MM)
    dens = smooth01(0.5 + 0.6 * (patch + CLOD_PATCH_BIAS))      # 0 = tamped, 1 = cloddy
    area = f.W * f.H
    n_try = int(rng.poisson(CLOD_DENSITY * area))
    xs = rng.uniform(0, f.W, n_try)
    ys = rng.uniform(0, f.H, n_try)
    rs = np.array([skewed(rng, CLOD_R_MM, CLOD_R_EXP) for _ in range(n_try)])
    order = np.argsort(-rs)                                      # big first
    keep_r = RIM_TEX_MM
    grid = {}                                                    # spatial hash of placed
    cs = 1.5 * CLOD_R_MM[1]
    out = base.copy()
    n = 0
    for k in order:
        cx, cy, r = xs[k], ys[k], rs[k]
        ix, iy = min(int(cx / CELL_MM), len(f.x) - 1), min(int(cy / CELL_MM), len(f.y) - 1)
        if rng.random() > dens[iy, ix] or min(cx, f.W - cx, cy, f.H - cy) < keep_r + 0.5 * r:
            continue
        gk = (int(cx / cs), int(cy / cs))
        ok = True
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                for (qx, qy, qr) in grid.get((gk[0] + dx, gk[1] + dy), ()):
                    if np.hypot(cx - qx, cy - qy) < CLOD_OVERLAP * (r + qr):
                        ok = False
                        break
        if not ok:
            continue
        grid.setdefault(gk, []).append((cx, cy, r))
        h = min(r * rng.uniform(*CLOD_H_RATIO), CLOD_H_MAX_MM)
        sunk = rng.uniform(*CLOD_SUNK) * h
        tilt = rng.normal(0, CLOD_TILT, 2)                       # tilted top (mm/mm)
        sl = f.window(cx, cy, 2.0 * r + CELL_MM)
        px, py = f.X[sl] - cx, f.Y[sl] - cy
        top = np.full(px.shape, -1e9)
        # A crumb is 1–3 overlapping lumpy lobes, not one round blister.
        for li in range(int(rng.integers(*CLOD_LOBES, endpoint=True))):
            lr = r * (1.0 if li == 0 else rng.uniform(0.45, 0.8))
            off = 0.0 if li == 0 else rng.uniform(0.4, 0.9) * r
            oa = rng.uniform(0, 2 * np.pi)
            lx, ly = px - off * np.cos(oa), py - off * np.sin(oa)
            el = rng.uniform(*CLOD_ASPECT)
            ang = rng.uniform(0, np.pi)
            qa = (lx * np.cos(ang) + ly * np.sin(ang)) / el
            qb = -lx * np.sin(ang) + ly * np.cos(ang)
            th = np.arctan2(qb, qa)
            rr = np.ones_like(th)
            for k in (2, 3, 5):
                rr += rng.normal(0, CLOD_LUMP / k ** 0.5) * np.cos(k * th + rng.uniform(0, 2 * np.pi))
            rho = np.sqrt(qa ** 2 + qb ** 2) / (lr * rr)
            lh = h * (1.0 if li == 0 else rng.uniform(0.6, 1.0))
            prof = lh * np.clip(1 - rho ** CLOD_FLAT, 0, None) ** 0.8   # flat-ish top, soft shoulder
            lobe = np.where(rho < 1, base[sl] - sunk + prof + tilt[0] * px + tilt[1] * py, -1e9)
            top = smax(top, lobe, CLOD_SOFT_MM)
        out[sl] = smax(out[sl], top, CLOD_SOFT_MM)
        n += 1
    return out, n


# ── 3 Grit ───────────────────────────────────────────────────────────────────
def grit(rng, f):
    seed = int(rng.integers(1 << 30))
    pts = np.c_[f.X.ravel(), f.Y.ravel(), np.zeros(f.X.size)]
    g = GRIT_AMP_MM * fbm(pts, seed, GRIT_MM, octaves=2).reshape(f.X.shape)
    # sparse sand grains (tiny bumps)
    n = int(rng.poisson(GRAIN_DENSITY * f.W * f.H))
    bumps = np.zeros_like(g)
    for _ in range(n):
        cx, cy = rng.uniform(0, f.W), rng.uniform(0, f.H)
        r, h = rng.uniform(*GRAIN_R_MM), rng.uniform(*GRAIN_H_MM)
        sl = f.window(cx, cy, r)
        d2 = ((f.X[sl] - cx) ** 2 + (f.Y[sl] - cy) ** 2) / r ** 2
        bumps[sl] = np.maximum(bumps[sl], h * np.clip(1 - d2, 0, None) ** 0.7)
    return g + bumps


# ── 4 Pebbles ────────────────────────────────────────────────────────────────
def cluster_field(rng, f, ground):
    n_sq = (f.W / SQUARE_MM) * (f.H / SQUARE_MM)
    k = int(round(rng.integers(*PEB_CLUSTERS_2X2, endpoint=True) * n_sq / 4))
    c = np.full(f.X.shape, PEB_BACKGROUND)
    for _ in range(k):
        cx, cy = rng.uniform(4, f.W - 4), rng.uniform(4, f.H - 4)
        s = rng.uniform(*PEB_CLUSTER_SIGMA_MM)
        c += np.exp(-((f.X - cx) ** 2 + (f.Y - cy) ** 2) / (2 * s * s))
    low = ndimage.gaussian_filter(ground, 3.0 / CELL_MM)
    lo, hi = np.quantile(low, [0.05, 0.95])
    c *= 1 + PEB_LOW_BIAS * (1 - np.clip((low - lo) / max(hi - lo, 1e-9), 0, 1))
    return c / c.max()


def pebble_shape(rng, qx, qy, D):
    """Height (≥0) of one pebble over local coords (qx, qy), centred, pre-sink.

    The pebble is a flattened ellipsoid cap; facets are its tangent planes
    at random points, each lowered a little (rocklab's one-facet-per-seed
    idea in 2.5D).  Faceted: the min of the planes alone (crisp ridges, a
    circumscribed polytope).  Rounded: the cap itself with a few shallow
    flats.  A height field cannot overhang, so any facet set is printable."""
    a = D / 2
    el = rng.uniform(*PEB_ASPECT)
    ang = rng.uniform(0, np.pi)
    qa = (qx * np.cos(ang) + qy * np.sin(ang)) / el
    qb = -qx * np.sin(ang) + qy * np.cos(ang)
    hp = D * rng.uniform(*PEB_H_RATIO) / el ** 0.3
    rounded = rng.random() < PEB_ROUND_PROB
    rho2 = (qa ** 2 + qb ** 2) / a ** 2
    pf = rng.uniform(*PEB_FLAT)
    cap = hp * np.sqrt(np.clip(1 - rho2 ** (pf / 2), 0, None))
    K = int(rng.integers(*PEB_FACETS, endpoint=True)) if D >= MED_D_MM[0] else int(rng.integers(*PEB_FACETS_SMALL, endpoint=True))
    if rounded:
        K = max(3, K // 3)
    # facet seed points: stratified in angle, radius ∝ √U (area-uniform), plus one near the top
    th = (np.arange(K) + rng.uniform(0, 1, K)) * 2 * np.pi / K + rng.uniform(0, 2 * np.pi)
    rs = np.sqrt(rng.uniform(0.05, 0.85, K))
    rs[0] = rng.uniform(0.0, 0.35)
    h = cap if rounded else np.full(qa.shape, np.inf)
    for t_, r_ in zip(th, rs):
        pa, pb = a * r_ * np.cos(t_), a * r_ * np.sin(t_)
        s = np.sqrt(max(1 - r_ ** pf, 1e-3))
        z0 = hp * s
        dz = hp * pf / 2 * r_ ** (pf - 2) / (a * a * s)        # −∂cap/∂q ÷ q
        tl = np.tan(np.radians(PEB_FACET_TILT_DEG))
        ga, gb = -dz * pa + rng.normal(0, tl), -dz * pb + rng.normal(0, tl)
        gn = np.hypot(ga, gb)
        cap_g = np.tan(np.radians(PEB_MAX_FACET_DEG))
        if gn > cap_g:
            ga, gb = ga * cap_g / gn, gb * cap_g / gn
        drop = hp * rng.uniform(*(PEB_ROUND_DROP if rounded else PEB_FACET_DROP))
        h = np.minimum(h, z0 - drop + ga * (qa - pa) + gb * (qb - pb))
    # Skirt: no side steeper than PEB_SKIRT_DEG near the footprint edge
    # (slab caps otherwise end in a vertical, grid-aliased wall).
    # The skirt is POLYGONAL (5–8 sides), so outlines are angular, not discs.
    M = int(rng.integers(*PEB_SKIRT_SIDES, endpoint=True))
    ts = (np.arange(M) + rng.uniform(-0.3, 0.3, M)) * 2 * np.pi / M + rng.uniform(0, 2 * np.pi)
    sk = np.tan(np.radians(PEB_SKIRT_DEG))
    for t_ in ts:
        off = a * rng.uniform(0.82, 1.05) * (1.1 if rounded else 1.0)
        h = np.minimum(h, sk * (off - (qa * np.cos(t_) + qb * np.sin(t_))))
    return np.clip(h, 0, None), hp, rounded


def place_pebbles(rng, f, z, ground, avoid):
    """Clustered half-sunk pebbles stamped into z.  avoid = [(x, y, r)] (stones).
    Returns (z, pebble mask, counts)."""
    n_sq = (f.W / SQUARE_MM) * (f.H / SQUARE_MM)
    C = cluster_field(rng, f, ground)
    sizes = ([skewed(rng, MED_D_MM, PEB_D_EXP) for _ in range(int(rng.integers(*MED_PER_SQ, endpoint=True) * n_sq))]
             + [skewed(rng, SMALL_D_MM, PEB_D_EXP) for _ in range(int(rng.integers(*SMALL_PER_SQ, endpoint=True) * n_sq))])
    placed = list(avoid)
    mask = np.zeros(z.shape, bool)
    counts = dict(small=0, medium=0, rounded=0)
    gsm = ndimage.gaussian_filter(z, 0.5 / CELL_MM)               # seat on smoothed soil
    for D in sizes:
        R = 0.5 * D * PEB_ASPECT[1]
        for _ in range(60):
            cx = rng.uniform(RIM_TEX_MM + R, f.W - RIM_TEX_MM - R)
            cy = rng.uniform(RIM_TEX_MM + R, f.H - RIM_TEX_MM - R)
            iy, ix = int(cy / CELL_MM), int(cx / CELL_MM)
            if rng.random() > C[iy, ix]:
                continue
            if any(np.hypot(cx - qx, cy - qy) < PEB_GAP_MM + 0.5 * D + qr for qx, qy, qr in placed):
                continue
            break
        else:
            continue
        sl = f.window(cx, cy, R * 1.6 + 0.3)
        h, hp, rounded = pebble_shape(rng, f.X[sl] - cx, f.Y[sl] - cy, D)
        med = D >= MED_D_MM[0]
        sink = rng.uniform(*PEB_SINK) * hp
        g0 = float(gsm[iy, ix])
        if med:                                                   # soil heaped round the foot
            d2 = ((f.X[sl] - cx) ** 2 + (f.Y[sl] - cy) ** 2) / (0.5 * D * 1.25) ** 2
            z[sl] += PEB_MOUND * hp * np.exp(-d2 * 1.4)
        top = np.where(h > 0, g0 - sink + h, -1e9)
        k = PEB_SOFT_MM[1] if med else PEB_SOFT_MM[0]
        new = smax(z[sl], top, k)
        mask[sl] |= top > z[sl] + 0.03
        z[sl] = new
        placed.append((cx, cy, 0.5 * D))
        counts['medium' if med else 'small'] += 1
        counts['rounded'] += rounded
    return z, mask, counts


# ── 5 Stones (rocklab) ───────────────────────────────────────────────────────
def make_stones(rng, f):
    """A very few larger faceted rocks (rocklab), dart-thrown.  Returns
    [(x, y, R, solid at origin, height)]."""
    n_sq = (f.W / SQUARE_MM) * (f.H / SQUARE_MM)
    n = int(round(rng.integers(*STONES_2X2, endpoint=True) * n_sq / 4))
    out = []
    for _ in range(n):
        D = skewed(rng, STONE_D_MM, 1.5)
        rock, _info = rl.make_rock(rng, D)
        rock = rock.scale([1.0, 1.0, STONE_Z_SCALE]).rotate([0.0, 0.0, float(rng.uniform(0, 360))])
        bb = rock.bounding_box()
        R = 0.5 * max(bb[3] - bb[0], bb[4] - bb[1])
        R = max(R, np.hypot(bb[0], bb[1]), np.hypot(bb[3], bb[4]))
        for _ in range(300):
            x = rng.uniform(STONE_RIM_MM + R, f.W - STONE_RIM_MM - R)
            y = rng.uniform(STONE_RIM_MM + R, f.H - STONE_RIM_MM - R)
            if all(np.hypot(x - q[0], y - q[1]) > STONE_GAP_MM + R + q[2] for q in out):
                out.append((x, y, R, rock, bb[5]))
                break
    return out


def bed_stones(f, z, stones):
    """Soil bank around each stone; seat it below the lowest ground under it.
    Returns (z, [posed Manifold])."""
    posed = []
    for x, y, R, rock, h in stones:
        d = np.hypot(f.X - x, f.Y - y)
        z += STONE_MOUND_MM * np.exp(-np.clip(d - 0.7 * R, 0, None) ** 2 / (2 * (0.5 * R) ** 2)) \
            * smooth01((f.dist - RIM_TEX_MM) / 1.0)
        under = z[d < 0.85 * R]
        z0 = min(float(under.min()) - 0.25, float(np.quantile(under, 0.5)) - STONE_SINK * h)
        posed.append(rock.translate([x, y, z0]))
    return z, posed


# ── 6 Cracks (optional) ──────────────────────────────────────────────────────
def crack_layer(rng, f):
    n_sq = (f.W / SQUARE_MM) * (f.H / SQUARE_MM)
    line = np.zeros(f.X.shape, bool)
    for _ in range(int(round(CRACKS_2X2 * n_sq / 4))):
        p = rng.uniform([6, 6], [f.W - 6, f.H - 6])
        th = rng.uniform(0, 2 * np.pi)
        for _ in range(int(rng.integers(20, 60))):
            th += rng.normal(0, 0.35)
            p = p + 0.3 * np.array([np.cos(th), np.sin(th)])
            if not (4 < p[0] < f.W - 4 and 4 < p[1] < f.H - 4):
                break
            line[int(p[1] / CELL_MM), int(p[0] / CELL_MM)] = True
    d = ndimage.distance_transform_edt(~line) * CELL_MM
    return -CRACK_DEPTH_MM * np.clip(1 - d / CRACK_W_MM, 0, None) ** 1.5


# ── Build ────────────────────────────────────────────────────────────────────
def build_field(seed, cols=2, rows=2, cracks=False):
    rng = np.random.default_rng(seed)
    W, H = cols * SQUARE_MM, rows * SQUARE_MM
    f = make_field(W, H)
    t0 = time.perf_counter()
    und = ground_undulation(rng, f)
    w_tex = smooth01((f.dist - RIM_FLAT_MM) / (RIM_TEX_MM - RIM_FLAT_MM))
    seed_mid = int(rng.integers(1 << 30))
    pts = np.c_[f.X.ravel(), f.Y.ravel(), np.zeros(f.X.size)]
    mid = SOIL_MID_AMP_MM * fbm(pts, seed_mid, SOIL_MID_MM, octaves=3).reshape(f.X.shape)
    ground = EDGE_MM + w_tex * LIFT_MM + und + w_tex * mid
    stones = make_stones(rng, f)
    t1 = time.perf_counter()
    soil, n_clods = clod_layer(rng, f, ground)
    soil = ground + w_tex * (soil - ground)
    t2 = time.perf_counter()
    z = soil + w_tex * grit(rng, f)
    if cracks:
        z += w_tex * crack_layer(rng, f)
    t3 = time.perf_counter()
    z, posed = bed_stones(f, z, stones)
    z, mask, counts = place_pebbles(rng, f, z, ground, [(s[0], s[1], s[2]) for s in stones])
    t4 = time.perf_counter()
    z[f.dist <= 1e-9] = EDGE_MM                                  # exact edge level
    z = np.maximum(z, EDGE_MM)                                   # rises only (REQ-H-3)
    f.z, f.pebble = z, mask
    f.stats = dict(clods=n_clods, **counts, stones=len(posed),
                   ground=(float(z.min()), float(np.median(z)), float(z.max())),
                   t=dict(ground=t1 - t0, clods=t2 - t1, grit=t3 - t2, pebbles=t4 - t3))
    return f, posed


def stl_safe(mesh):
    """Export guarantee (as grasslab.stl_safe): the mesh as a slicer sees it."""
    from dharmatiles.stone.solidify import separate_pinches
    mesh = trimesh.Trimesh(mesh.vertices.copy(), np.asarray(mesh.faces), process=False)
    separate_pinches(mesh)
    v32 = np.asarray(mesh.vertices, dtype=np.float32).astype(np.float64)
    m = trimesh.Trimesh(v32, np.asarray(mesh.faces), process=True)
    parts = m.split(only_watertight=False)
    if len(parts) > 1:
        m = max(parts, key=lambda q: len(q.faces))
        m = trimesh.Trimesh(m.vertices, m.faces, process=True)
    if not m.is_watertight:
        raise RuntimeError('mesh is not watertight after STL-style merge')
    return m


def assemble(f, posed, cols=2, rows=2):
    from dharmatiles.core.mesh import make_heightmap_solid
    from dharmatiles.core.config import BaseConfig, SurfaceConfig
    from dharmatiles.bases import dungeonblocks as db
    top = rl.to_manifold(make_heightmap_solid(f.z, f.W, f.H, base_h=0.0, subsample=1))
    slab = m3d.Manifold.cube([f.W, f.H, SLAB_BOX_MM])
    solid = m3d.Manifold.batch_boolean([top, slab] + posed, m3d.OpType.Add)
    zmax = solid.bounding_box()[5]
    surface = SurfaceConfig(cols=cols, rows=rows, square_mm=SQUARE_MM)
    base_cfg = BaseConfig()
    peg = db.select_peg_height(np.zeros((2, 2)), base_cfg, zmax)
    solid = solid + rl.to_manifold(db.make_base(surface, peg, base_cfg))
    bodies = len(solid.decompose())
    return stl_safe(rl.to_trimesh(solid)), peg, zmax, bodies


def audit_overhang(mesh):
    n = mesh.face_normals
    bad = (n[:, 2] < -np.sin(np.radians(OVERHANG_DEG + 3.0))) & (mesh.triangles_center[:, 2] > 0.05)
    return float(mesh.area_faces[bad].sum())


def build(seed, cols=2, rows=2, cracks=False):
    t0 = time.perf_counter()
    f, posed = build_field(seed, cols, rows, cracks)
    mesh, peg, zmax, bodies = assemble(f, posed, cols, rows)
    f.stats.update(peg=peg, zmax=zmax, bodies=bodies, overhang=audit_overhang(mesh),
                   build_s=time.perf_counter() - t0)
    return f, mesh


# ── Painted preview / contact sheet ──────────────────────────────────────────
EARTH = [(52, 36, 24), (88, 62, 40), (128, 96, 64), (170, 138, 100), (205, 180, 140)]
PEBBLE = [(96, 96, 100), (132, 130, 128), (168, 164, 158)]
SIDE = (42, 42, 46)


def _ramp(t, stops):
    stops = np.array(stops, float)
    i = np.clip(t * (len(stops) - 1), 0, len(stops) - 1 - 1e-9)
    k = i.astype(int)
    fr = (i - k)[:, None]
    return stops[k] * (1 - fr) + stops[k + 1] * fr


def painted(f, mesh):
    """Per-face paint: earth by local relief (a drybrush), stones grey."""
    m = trimesh.Trimesh(mesh.vertices, mesh.faces, process=False)
    c = m.triangles_center
    ix = np.clip(np.round(c[:, 0] / CELL_MM).astype(int), 0, f.z.shape[1] - 1)
    iy = np.clip(np.round(c[:, 1] / CELL_MM).astype(int), 0, f.z.shape[0] - 1)
    z = f.z
    rel = z - ndimage.gaussian_filter(z, 0.8 / CELL_MM)
    rel2 = z - ndimage.gaussian_filter(z, 4.0 / CELL_MM)
    t = np.clip(0.5 + 0.7 * rel[iy, ix] / 0.35 + 0.3 * rel2[iy, ix] / 1.0, 0, 1)
    col = _ramp(t, EARTH)
    zs = z[iy, ix]
    stone = f.pebble[iy, ix] | (c[:, 2] > zs + 0.06)
    tp = np.clip(0.5 + 0.5 * m.face_normals[:, 2] - 0.4 + 0.6 * rel[iy, ix] / 0.5, 0, 1)
    col[stone] = _ramp(tp[stone], PEBBLE)
    side = (np.abs(m.face_normals[:, 2]) < 0.35) & (c[:, 2] < zs - 0.3) | (c[:, 2] < EDGE_MM - 0.3)
    col[side] = SIDE
    m.visual.face_colors = np.c_[col, np.full(len(col), 255)].astype(np.uint8)
    return m


def crop(mesh, x0, x1, y0, y1, zmin=0.5):
    """Faces whose centres fall in the box (keeps per-face paint)."""
    c = mesh.triangles_center
    keep = (c[:, 0] > x0) & (c[:, 0] < x1) & (c[:, 1] > y0) & (c[:, 1] < y1) & (c[:, 2] > zmin)
    return mesh.submesh([np.nonzero(keep)[0]], append=True)


def top_heightmap(f, mesh):
    """The finished tile's top surface on the field grid (rays straight down),
    so the fallback renderer also shows the rocklab stones."""
    o = np.c_[f.X.ravel(), f.Y.ravel(), np.full(f.X.size, 100.0)]
    o[:, 0] = np.clip(o[:, 0], 1e-3, f.W - 1e-3)
    o[:, 1] = np.clip(o[:, 1], 1e-3, f.H - 1e-3)
    d = np.tile([0.0, 0.0, -1.0], (len(o), 1))
    loc, idx, _ = mesh.ray.intersects_location(o, d, multiple_hits=False)
    z = f.z.ravel().copy()
    z[idx] = loc[:, 2]
    return z.reshape(f.z.shape)


def paint_grid(f, Z):
    """Per-cell paint for the fallback renderer (same palette as painted())."""
    rel = Z - ndimage.gaussian_filter(Z, 0.8 / CELL_MM)
    rel2 = Z - ndimage.gaussian_filter(Z, 4.0 / CELL_MM)
    t = np.clip(0.5 + 0.7 * rel / 0.35 + 0.3 * rel2 / 1.0, 0, 1)
    col = _ramp(t.ravel(), EARTH)
    stone = (f.pebble | (Z > f.z + 0.06)).ravel()
    gy, gx = np.gradient(Z, CELL_MM)
    nz = 1 / np.sqrt(1 + gx ** 2 + gy ** 2)
    tp = np.clip(0.5 + 0.5 * nz.ravel() - 0.4 + 0.6 * rel.ravel() / 0.5, 0, 1)
    col[stone] = _ramp(tp[stone], PEBBLE)
    return col.reshape(Z.shape + (3,)) / 255.0


def render_fallback(f, Z, rgb, path, elev, azim, box=None, res=(1200, 950)):
    """Software preview (matplotlib, lit height field) for when pyrender has
    no screen (e.g. a locked macOS session).  Surface only, no tile walls."""
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    X, Y = f.X, f.Y
    st = 3
    if box is not None:
        x0, x1, y0, y1 = box
        sx = slice(int(x0 / CELL_MM), int(x1 / CELL_MM) + 1)
        sy = slice(int(y0 / CELL_MM), int(y1 / CELL_MM) + 1)
        X, Y, Z, rgb = X[sy, sx], Y[sy, sx], Z[sy, sx], rgb[sy, sx]
        st = 1
    gy, gx = np.gradient(Z, CELL_MM)
    n = np.stack([-gx, -gy, np.ones_like(Z)], -1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    la, le = np.radians(azim + 50), np.radians(40)
    lit = np.array([np.cos(le) * np.cos(la), np.cos(le) * np.sin(la), np.sin(le)])
    shaded = np.clip(rgb * (0.3 + 0.85 * np.clip(n @ lit, 0, None))[..., None], 0, 1)
    fig = plt.figure(figsize=(res[0] / 100, res[1] / 100), dpi=100)
    ax = fig.add_subplot(projection='3d')
    ax.plot_surface(X[::st, ::st], Y[::st, ::st], Z[::st, ::st], facecolors=shaded[::st, ::st],
                    rstride=1, cstride=1, linewidth=0, antialiased=False, shade=False)
    w = X.max() - X.min()
    ax.set_box_aspect((1, (Y.max() - Y.min()) / w, (Z.max() - EDGE_MM + 1.0) / w), zoom=1.45)
    ax.set_zlim(EDGE_MM - 1.0, Z.max())
    ax.view_init(elev=elev, azim=azim)
    ax.set_axis_off()
    ax.set_facecolor((0.85, 0.88, 0.92))
    fig.subplots_adjust(0, 0, 1, 1)
    fig.savefig(path, facecolor=(0.85, 0.88, 0.92))
    plt.close(fig)
    from PIL import Image, ImageChops
    im = Image.open(path).convert('RGB')
    bg = Image.new('RGB', im.size, (217, 224, 235))
    bb = ImageChops.difference(im, bg).convert('L').point(lambda v: 255 if v > 12 else 0).getbbox()
    if bb:
        im.crop((max(bb[0] - 10, 0), max(bb[1] - 10, 0), bb[2] + 10, bb[3] + 10)).save(path)


VIEWS = [('painted', 40, -120), ('oblique', 40, -120), ('top', 89, -90), ('low', 14, -150),
         ('close (painted, 20 mm)', 38, -125)]


def contact_sheet(seeds, cracks=False):
    from PIL import Image, ImageDraw, ImageFont
    from dharmatiles.render import render
    cell = (760, 600)
    try:
        font = ImageFont.truetype('/System/Library/Fonts/Helvetica.ttc', 26)
    except OSError:
        font = ImageFont.load_default()
    head, cap = 46, 44
    row_h = head + cell[1] + cap
    sheet = Image.new('RGB', (cell[0] * len(VIEWS), row_h * len(seeds)), (245, 245, 245))
    dr = ImageDraw.Draw(sheet)
    fallback = False
    with tempfile.TemporaryDirectory() as td:
        tmp = pathlib.Path(td)
        for r, seed in enumerate(seeds):
            f, mesh = build(seed, cracks=cracks)
            st = f.stats
            y = r * row_h
            dr.rectangle([0, y, sheet.width, y + head - 6], fill=(225, 228, 235))
            dr.text((14, y + 8),
                    f"dirt s{seed}: {st['clods']} clods, {st['small']} small + {st['medium']} medium pebbles "
                    f"({st['rounded']} rounded), {st['stones']} stones; ground {st['ground'][0]:.2f}–"
                    f"{st['ground'][2]:.2f} mm; {len(mesh.faces):,} faces, watertight={mesh.is_watertight}, "
                    f"overhang {st['overhang']:.2f} mm², build {st['build_s']:.1f}s",
                    fill=(25, 25, 25), font=font)
            clay = mesh.copy()
            clay.visual.face_colors = np.tile([150, 150, 150, 255], (len(clay.faces), 1))
            topv = clay.slice_plane([0, 0, 0.5], [0, 0, 1], cap=False)
            pm = painted(f, mesh)
            c0 = f.W / 2 - 10
            Z = rgb = None
            for k, (name, elev, azim) in enumerate(VIEWS):
                p = tmp / f's{seed}-{k}.png'
                if name == 'painted':
                    meshes = [pm]
                elif name == 'oblique':
                    meshes = [clay]
                elif name.startswith('close'):
                    meshes = [crop(pm, c0, c0 + 20, c0, c0 + 20)]
                else:
                    meshes = [topv]
                try:
                    if fallback:
                        raise RuntimeError('fallback')
                    render(meshes, p, elev=elev, azim=azim, resolution=(1200, 950), quiet=True, smooth=False)
                except Exception as e:                       # no screen for pyrender
                    if not fallback:
                        print(f"  pyrender failed ({type(e).__name__}: {e}); using the matplotlib fallback")
                        fallback = True
                    if Z is None:
                        Z = top_heightmap(f, mesh)
                        rgb = paint_grid(f, Z)
                    tint = rgb if name in ('painted',) or name.startswith('close') else np.full(rgb.shape, 0.5)
                    box = (c0, c0 + 20, c0, c0 + 20) if name.startswith('close') else None
                    render_fallback(f, Z, tint, p, elev, azim if name != 'top' else -90, box)
                im = Image.open(p).convert('RGB')
                im.thumbnail((cell[0] - 20, cell[1] - 20))
                x = k * cell[0]
                sheet.paste(im, (x + (cell[0] - im.width) // 2, y + head + (cell[1] - im.height) // 2))
                dr.text((x + 18, y + head + cell[1] + 8), f"s{seed} · {name}" + (" (fallback renderer)" if fallback else ""), fill=(40, 40, 40), font=font)
            print(f"  seed {seed}: {st}")
    out = pathlib.Path(__file__).parent / 'out'
    out.mkdir(exist_ok=True)
    path = out / f"{datetime.datetime.now():%Y-%m-%d-%H%M%S}-contact-sheet.png"
    sheet.save(path)
    print(f"→ {path}")
    return path


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--cols', type=int, default=2)
    ap.add_argument('--rows', type=int, default=2)
    ap.add_argument('--cracks', action='store_true', help='add a few fine cracks (off by default)')
    ap.add_argument('--sheet', action='store_true')
    ap.add_argument('--seeds', type=int, nargs='*', default=[1, 2, 3])
    args = ap.parse_args()
    if args.sheet:
        contact_sheet(args.seeds, cracks=args.cracks)
        return
    f, mesh = build(args.seed, args.cols, args.rows, args.cracks)
    out = pathlib.Path(__file__).parent / 'out'
    out.mkdir(exist_ok=True)
    path = out / f"{args.cols}x{args.rows}-dirt-s{args.seed}{'-cracks' if args.cracks else ''}.stl"
    mesh.export(path)
    st = f.stats
    print(f"→ {path}  ({path.stat().st_size / 1e6:.1f} MB) {len(mesh.vertices):,} verts, "
          f"{len(mesh.faces):,} faces, watertight={mesh.is_watertight}, bodies={st['bodies']}, "
          f"peg={st['peg']} mm, top {st['zmax']:.2f} mm")
    print(f"  {st}")


if __name__ == '__main__':
    main()
