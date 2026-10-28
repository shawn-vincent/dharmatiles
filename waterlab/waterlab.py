#!/usr/bin/env python3
"""waterlab v0 — fantasy water around a rock and a bank on a DB tile.

One fixed scene (2x2 tile, land rise on the left, one rocklab rock in the
water); several interchangeable water generators.  See waterlab/DESIGN.md.

Usage:
    python waterlab/waterlab.py --mode lapping            # lapping | crescents | legacy
    python waterlab/waterlab.py --mode all --check out.png
"""
from __future__ import annotations

import argparse
import pathlib
import sys
import time

import numpy as np
import trimesh
import manifold3d as m3d
from scipy import ndimage

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'rocklab'))
import rocklab as rl   # noqa: E402

# ── Scene ────────────────────────────────────────────────────────────────────
SQUARE_MM      = 35.0
COLS = ROWS    = 2
CELL_MM        = 0.1
WL_MM          = 2.6          # calm water level above terrain datum
LAND_TOP_MM    = 6.0
SHORE_X_MM     = 17.0         # mean shoreline position (land on the left)
SHORE_WOBBLE_MM = 4.0
BANK_W_MM      = 7.0
ROCK_XY        = (46.0, 38.0)
ROCK_D_MM      = 15.0
ROCK_SINK_MM   = 1.4          # rock base below water level
RIM_FADE_MM    = 4.0
DIST_SMOOTH_MM = 1.0
BED_DEPTH_MM   = 2.2           # underwater bed below calm water (troughs + tide must stay above it)
MESH_SUBSAMPLE = 2            # uniform 0.2 mm top mesh (adaptive mesh faceted the broad swells)

# ── tide: large-scale swell shared by every option ──────────────────────────
TIDE_SIGMA_MM   = 9.0         # interior swell scale
TIDE_RMS_MM     = 0.35
TIDE_EDGE_AMP_MM = 0.6        # shared edge profile (dharmatiles water idea)
TIDE_EDGE_LAMBDA_MM = SQUARE_MM / 2
TIDE_EDGE_FADE_MM = 9.0

# ── direction field (combed, lapped) ─────────────────────────────────────────
DIR_SIGMA_MM    = 12.0
DIR_SWING_DEG   = 70.0
DIR_NEAR_MM     = 7.0         # obstacles deflect the field within ~this

# ── ripple strokes (combed, lapped) ──────────────────────────────────────────
STK_SPACING_MM  = 5.6
STK_LEN_NEAR_MM = (10.0, 20.0)
STK_LEN_FAR_MM  = (7.0, 15.0)
STK_H_NEAR_MM   = (0.75, 1.15)
STK_H_FAR_MM    = (0.3, 0.6)
STK_HW_NEAR_MM  = 1.2         # crest half-width (Gaussian 1/e at q = 1)
STK_HW_FAR_MM   = 1.05
STK_TROUGH      = 0.28        # trough flanking each crest (× crest height)
STK_SOFT_K_MM   = 0.12

# ── rings (lapping v2) ───────────────────────────────────────────────────────
LAP_LAMBDA0_MM = 2.6          # crest spacing at the contact
LAP_GROWTH     = 0.09         # spacing widens with distance: λ(d) = λ0(1 + g d)
LAP_WOBBLE_MM  = 1.2
LAP_PROFILE_P  = 1.6          # >1: narrower crests, broad troughs
LAP_ROCK_A_MM  = 1.0
LAP_ROCK_DECAY = 9.0
LAP_SHORE_A_MM = 0.9
LAP_SHORE_DECAY = 8.0
LAP_BREAK_SIGMA_MM = 3.5      # crest break pattern scale along the obstacle
LAP_BREAK_BIAS = 0.0          # higher → fewer breaks
LIP_MM         = 0.3
LIP_D_MM       = 0.6

# ── sim: linear wave equation on the scene ───────────────────────────────────
SIM_CELL_MM    = 0.25
SIM_STEPS      = 1400
SIM_C          = 0.45          # cells/step in deep water (CFL < 0.7)
SIM_DEPTH_MM   = 1.0           # depth at which c = SIM_C (shallower → slower)
SIM_MIN_DEPTH  = 0.35          # caps shoaling (shorter waves) in the shallows
SIM_LAMBDA_MM  = (2.5, 14.0)   # forcing wavelengths (log-uniform)
SIM_N_FREQ     = 6
SIM_AMP_EXP    = 0.7           # injected amplitude ∝ λ^e (continuous spectrum)
SIM_DAMP       = 0.0025
SIM_SPONGE_MM  = 5.0           # injection band at the water edges
SIM_RMS_MM     = 0.32
SIM_SHARPEN    = 0.9           # crest sharpening (2nd-harmonic style)
SIM_NEAR_BOOST = 0.9           # extra amplitude near obstacles (fantasy)
SIM_NEAR_MM    = 6.0

# ── spectrum ─────────────────────────────────────────────────────────────────
SPEC_N         = 70
SPEC_LAMBDA_MM = (2.5, 22.0)
SPEC_AMP_EXP   = 0.65          # a ∝ λ^e (fantasy: short waves louder than physics)
SPEC_SPREAD_DEG = 75.0         # direction spread about the local wind
SPEC_RMS_MM    = 0.32
SPEC_STEEP     = 0.55          # Stokes 2nd-harmonic strength
SPEC_SHOAL_K   = 0.35           # wave-number gain at obstacles
SPEC_SHOAL_MM  = 7.0

# ── emitted: waves radiate from sources (land strong, rocks weak, edges fallback)
EMIT_CELL_MM    = 0.25        # geodesic-distance grid
EMIT_SOURCES = {              # strength, decay (mm), base λ (mm), λ growth /mm
    'land': (1.00, 34.0, 3.8, 0.022),
    'rock': (0.55, 10.0, 3.0, 0.05),
    'edge': (0.28, 50.0, 5.0, 0.0),
}
EMIT_AMP_MM     = 1.15        # crest height where a source fully dominates
EMIT_PROFILE_P  = 1.7
EMIT_WOBBLE_MM  = 1.0
EMIT_RIDER      = (0.45, 0.55) # same-source swell: wavenumber ×, amplitude ×
EMIT_BREAK_BIAS = 0.55        # crests mostly continuous; breaks are long and rare
EMIT_BREAK_SIGMA_MM = 7.0
EMIT_N_EDGES    = 2
EMIT_SHARP      = 8.0         # blend exponent: higher → crisper source territories

# ── emitted-chaos ────────────────────────────────────────────────────────────
CHAOS_CORNER     = (0.30, 70.0, 4.0, 0.012)   # weak radial corner emitters
CHAOS_WOBBLE_GROW = 0.06      # extra phase wobble (mm) per mm from a source
CHAOS_ORDER_LO   = 0.12       # physical influence below this → full chaos
CHAOS_ORDER_HI   = 0.45       # above this → the physical source rules
CHAOS_FAR_AMP    = 0.7        # amplitude far from physical emitters
CHAOS_BREAK_BIAS = 0.9        # physical crests rarely break (coherence loss does the work)

# ── spectral-chaos: a wavelength band per emitter type ──────────────────────
BANDS = {                     # (λ_lo mm, λ_hi mm, spacing growth /mm), components
    'land':   ((3.2, 11.0, 0.015), 4),   # long + medium waves from the shore
    'rock':   ((2.2, 4.8, 0.04), 3),    # short choppy wavelets off the rock
    'corner': ((4.0, 12.0, 0.0), 3),    # weak long swell from open water
}
BAND_AMP_EXP = 1.0            # a ∝ λ^e within a band (longer waves taller)
BAND_SHARPEN = 0.85
BAND_AMP_MM  = 0.42           # per unit-RMS train

# ── swell-chaos (round 7) ────────────────────────────────────────────────────
SWELL_BANDS = {
    'land':   ((5.0, 16.0, 0.012), 4),  # broad rolling swells + medium riders
    'rock':   ((2.0, 4.5, 0.035), 3),   # short chop (≥ 2 mm: printable)
    'corner': ((5.0, 14.0, 0.0), 3),
}
SWELL_AMP_EXP      = 1.4      # long waves dominate
SWELL_LONG_SHARPEN = 0.15     # swells stay round
SWELL_SHORT_SHARPEN = 0.9     # riders get crisp crests
SWELL_AMP_MM       = 0.8      # ≈ 2× spectral-chaos relief
SWELL_ROCK_GAIN    = 1.15
SWELL_WAKE         = 0.9      # extra rock chop on the downstream side
SWELL_TROUGH_MM    = 1.1      # deepest trough below calm water (bed is at −BED_DEPTH)

# ── lapped (rings near obstacles + strokes beyond) ──────────────────────────
LAPPED_NEAR_MM = 8.0
LAPPED_BREAK_BIAS = 0.45       # contact rings mostly continuous here

# ── legacy (dharmatiles port) ────────────────────────────────────────────────
LEG_NOISE_SIGMA_MM = 3.0
LEG_NOISE_RMS_MM   = 0.25
LEG_AMP_MM         = 0.5
LEG_LAMBDA_MM      = 3.0
LEG_SEG_LEN_MM     = (10.0, 20.0)
LEG_SEG_FADE_MM    = 15.0
LEG_OFFSET_MM      = (0.0, 20.0)
LEG_ARC_SMOOTH_MM  = 2.5


def smooth01(t):
    t = np.clip(t, 0, 1)
    return t * t * (3 - 2 * t)


def noise(rng, shape, sigma_px):
    n = ndimage.gaussian_filter(rng.standard_normal(shape), sigma_px, mode='wrap')
    return (n - n.mean()) / (n.std() + 1e-12)


def poisson(rng, x0, y0, x1, y1, r):
    pts = []
    for p in rng.uniform([x0, y0], [x1, y1], size=(int(40 * (x1 - x0) * (y1 - y0) / r ** 2), 2)):
        if all((p[0] - q[0]) ** 2 + (p[1] - q[1]) ** 2 >= r * r for q in pts):
            pts.append(p)
    return np.array(pts)


# ── Scene ────────────────────────────────────────────────────────────────────
# Scenarios: which tile sides are land, and which obstacles stand in the water
# (kind, x, y, size mm).  'bank-rock' is the original scene.
SCENARIOS = {
    'bank-rock':    dict(land='W',    obstacles=[('rock', 46, 38, 15)]),
    'open':         dict(land='',     obstacles=[]),
    'open-rock':    dict(land='',     obstacles=[('rock', 35, 35, 15)]),
    'open-2rocks':  dict(land='',     obstacles=[('rock', 24, 45, 13), ('rock', 47, 23, 11)]),
    'shore-shapes': dict(land='W',    obstacles=[('cube', 42, 49, 10), ('pyramid', 50, 21, 13)]),
    'land-2':       dict(land='WE',   obstacles=[('rock', 35, 37, 13)]),
    'land-3':       dict(land='WES',  obstacles=[('rock', 35, 42, 13)]),
    'land-4':       dict(land='WESN', obstacles=[('rock', 35, 35, 12)]),
}
MULTI_SHORE_MM = 14.0          # shoreline inset when land is on several sides


def make_obstacle(kind, size, rng):
    """A solid standing in the water, base at z = 0 (sunk later)."""
    if kind == 'rock':
        return rl.make_rock(rng, size)[0]
    if kind == 'cube':
        h = size * 0.8
        return m3d.Manifold.cube([size, size, h], center=True).translate([0, 0, h / 2])
    if kind == 'pyramid':
        h = size * 0.75
        b = size / 2
        pts = np.array([[-b, -b, 0], [b, -b, 0], [b, b, 0], [-b, b, 0], [0, 0, h]], np.float32)
        return m3d.Manifold.hull_points(pts)
    raise ValueError(kind)


class Scene:
    def __init__(self, seed, open_water=False, scenario=None):
        scenario = scenario or ('open' if open_water else 'bank-rock')
        self.scenario = scenario
        spec = SCENARIOS[scenario]
        self.open = not spec['land'] and not spec['obstacles']
        rng = np.random.default_rng(seed)
        self.W = COLS * SQUARE_MM
        self.H = ROWS * SQUARE_MM
        n = int(round(self.W / CELL_MM)) + 1
        self.x = np.linspace(0, self.W, n)
        self.y = np.linspace(0, self.H, n)
        X, Y = np.meshgrid(self.x, self.y)          # rows = y, cols = x
        self.X, self.Y = X, Y
        # Land rising from the listed sides (union of per-side banks).
        inset = SHORE_X_MM if len(spec['land']) <= 1 else MULTI_SHORE_MM
        s = np.full(X.shape, -1e3)
        for side in spec['land']:
            dist, along = {'W': (X, Y), 'E': (self.W - X, Y),
                           'S': (Y, X), 'N': (self.H - Y, X)}[side]
            wob = ndimage.gaussian_filter1d(rng.standard_normal(n), 9.0 / CELL_MM, mode='wrap')
            wob /= np.abs(wob).max() + 1e-9
            pos = inset + SHORE_WOBBLE_MM * np.interp(along, self.x, wob)
            s = np.maximum(s, pos - dist)               # > 0 inside land
        bank = smooth01(s / BANK_W_MM)
        self.land = (WL_MM - BED_DEPTH_MM) + (LAND_TOP_MM - WL_MM + BED_DEPTH_MM) * bank
        self.land += 0.25 * noise(rng, X.shape, 6.0 / CELL_MM) * bank
        # Obstacles standing in the water.
        solids = []
        for i, (kind, ox, oy, size) in enumerate(spec['obstacles']):
            ob = make_obstacle(kind, size, np.random.default_rng(seed + 77 + i))
            ob = ob.rotate([0, 0, float(rng.uniform(0, 360))])
            solids.append(ob.translate([ox, oy, WL_MM - ROCK_SINK_MM]))
        self.rock = (m3d.Manifold.batch_boolean(solids, m3d.OpType.Add) if solids else None)
        self.rock_mask = (self._footprint(self.rock, WL_MM) if self.rock is not None
                          else np.zeros(X.shape, bool))
        # Obstacles and distance.
        self.land_mask = self.land > WL_MM
        obst = self.land_mask | self.rock_mask
        if not obst.any():                   # distance from nothing: treat as far
            d = np.full(X.shape, 1e3); iy, ix = np.indices(X.shape)
        else:
            d, (iy, ix) = ndimage.distance_transform_edt(~obst, return_indices=True)
        d = d * CELL_MM
        w = (~obst).astype(float)
        sg = DIST_SMOOTH_MM / CELL_MM
        self.d = np.where(~obst, ndimage.gaussian_filter(d * w, sg)
                          / np.maximum(ndimage.gaussian_filter(w, sg), 1e-6), 0.0)
        self.near_iy, self.near_ix = iy, ix
        self.near_rock = self.rock_mask[iy, ix]
        rim = np.minimum.reduce([X, self.W - X, Y, self.H - Y])
        self.rim = smooth01(rim / RIM_FADE_MM)
        self.water = ~obst
        # Large-scale swell shared by every option (same seed → same tide).
        self.tide = tide_field(np.random.default_rng([seed, 0x71DE]), self)

    def _footprint(self, man, z):
        from matplotlib.path import Path
        sl = man.slice(z)
        mask = np.zeros(self.X.shape, bool)
        pts = np.stack([self.X.ravel(), self.Y.ravel()], 1)
        for poly in sl.to_polygons():
            poly = np.asarray(poly)
            if len(poly) >= 3:
                mask.ravel()[Path(poly).contains_points(pts)] = True
        return mask


# ── Generators ───────────────────────────────────────────────────────────────
def tide_field(rng, sc):
    """Large-scale swell: interior band-limited noise, forced at the tile edges to
    the shared sinusoidal profile (crests at square corners/edge centres, troughs
    between), so any two water tiles butt with the same shape."""
    X, Y, W, H = sc.X, sc.Y, sc.W, sc.H
    inner = TIDE_RMS_MM * noise(rng, X.shape, TIDE_SIGMA_MM / CELL_MM)
    lam = TIDE_EDGE_LAMBDA_MM
    org = SQUARE_MM * 0.5 - lam * 0.25
    px = TIDE_EDGE_AMP_MM * np.sin(2 * np.pi * (X - org) / lam)   # along S/N edges
    py = TIDE_EDGE_AMP_MM * np.sin(2 * np.pi * (Y - org) / lam)   # along W/E edges
    dy = np.minimum(Y, H - Y)
    dx = np.minimum(X, W - X)
    wy, wx = 1 / np.maximum(dy, 1e-6), 1 / np.maximum(dx, 1e-6)
    edge = (wy * px + wx * py) / (wy + wx)
    b = smooth01(1 - np.minimum(dx, dy) / TIDE_EDGE_FADE_MM)
    return inner * (1 - b) + edge * b


def direction_field(rng, sc):
    """Unoriented crest direction: a smooth regional field, deflected to run
    tangent to rock and shore within ~DIR_NEAR_MM (normal component removed),
    so strokes wrap obstacles continuously instead of bursting out of them."""
    th = rng.uniform(0, 2 * np.pi) + np.radians(DIR_SWING_DEG) * noise(
        rng, sc.X.shape, DIR_SIGMA_MM / CELL_MM)
    vx, vy = np.cos(th), np.sin(th)
    gy, gx = np.gradient(sc.d, CELL_MM)
    gn = np.maximum(np.hypot(gx, gy), 1e-9)
    nx, ny = gx / gn, gy / gn
    w = np.exp(-sc.d / DIR_NEAR_MM)
    dot = vx * nx + vy * ny
    tx, ty = -ny, nx
    sgn = np.where(vx * tx + vy * ty >= 0, 1.0, -1.0)
    ux = vx - w * dot * nx + 1e-3 * sgn * tx          # tangent fallback at partings
    uy = vy - w * dot * ny + 1e-3 * sgn * ty
    a = np.arctan2(uy, ux)
    c2 = ndimage.gaussian_filter(np.cos(2 * a), 0.8 / CELL_MM)
    s2 = ndimage.gaussian_filter(np.sin(2 * a), 0.8 / CELL_MM)
    return 0.5 * np.arctan2(s2, c2)


def trace_crest(th, p, length):
    """Centreline through p along the unoriented field, both directions."""
    n = max(int(length / 2 / 0.25), 1)
    out = [np.array(p, float)]
    for sgn in (1.0, -1.0):
        q, prev, seg = np.array(p, float), None, []
        for _ in range(n):
            iy = int(np.clip(q[1] / CELL_MM, 0, th.shape[0] - 1))
            ix = int(np.clip(q[0] / CELL_MM, 0, th.shape[1] - 1))
            v = np.array([np.cos(th[iy, ix]), np.sin(th[iy, ix])])
            if prev is None:
                v *= sgn
            elif v @ prev < 0:
                v = -v
            q = q + 0.25 * v
            prev = v
            seg.append(q.copy())
        out = (seg[::-1] + out) if sgn < 0 else (out + seg)
    return np.array(out)


def ripple_strokes(rng, sc, th, min_d=0.0):
    """Ripple strokes: Gaussian crests (no hard edge) with shallow troughs either
    side, swelling from nothing at each end to full in the middle — ripples that
    fade organically, not tubes ("worms") with blunt ends."""
    pts = poisson(rng, 0, 0, sc.W, sc.H, STK_SPACING_MM)
    pos = np.zeros(sc.X.shape)
    neg = np.zeros(sc.X.shape)
    var = noise(rng, sc.X.shape, 4.0 / CELL_MM)
    k_ = STK_SOFT_K_MM
    for p in pts:
        iy = int(min(p[1] / CELL_MM, sc.d.shape[0] - 1))
        ix = int(min(p[0] / CELL_MM, sc.d.shape[1] - 1))
        if not sc.water[iy, ix] or sc.d[iy, ix] < min_d:
            continue
        near = float(np.exp(-sc.d[iy, ix] / DIR_NEAR_MM))
        L = rng.uniform(*STK_LEN_FAR_MM) * (1 - near) + rng.uniform(*STK_LEN_NEAR_MM) * near
        h = rng.uniform(*STK_H_FAR_MM) * (1 - near) + rng.uniform(*STK_H_NEAR_MM) * near
        hw = STK_HW_FAR_MM * (1 - near) + STK_HW_NEAR_MM * near
        cl = trace_crest(th, p, L)
        b = int((L / 2 + 3.5 * hw + 1) / CELL_MM)
        y0, y1 = max(iy - b, 0), min(iy + b, sc.X.shape[0])
        x0, x1 = max(ix - b, 0), min(ix + b, sc.X.shape[1])
        P = np.stack([sc.X[y0:y1, x0:x1].ravel(), sc.Y[y0:y1, x0:x1].ravel()], 1)
        d2 = ((P[:, None, :] - cl[None, :, :]) ** 2).sum(2)
        k = d2.argmin(1)
        dist = np.sqrt(d2[np.arange(len(k)), k])
        sl = k / max(len(cl) - 1, 1)                          # 0..1 along the stroke
        bell = np.sin(np.pi * sl) ** 1.3                     # height: 0 → 1 → 0
        wid = hw * (0.35 + 0.65 * np.sin(np.pi * sl) ** 0.6)  # width tapers too
        q = dist / wid
        # past the ends the nearest point is an endpoint → fade radially there too
        hh = h * bell * (0.8 + 0.2 * var[y0:y1, x0:x1].ravel())
        crest = (hh * np.exp(-2.2 * q ** 2)).reshape(y1 - y0, x1 - x0)
        trough = (-STK_TROUGH * hh * np.exp(-((q - 1.55) / 0.55) ** 2)).reshape(y1 - y0, x1 - x0)
        a = pos[y0:y1, x0:x1]
        both = (a > 1e-3) & (crest > 1e-3)
        soft = np.maximum(a, crest) + k_ * np.log1p(np.exp(-np.abs(a - crest) / k_)) - k_ * np.log(2)
        pos[y0:y1, x0:x1] = np.where(both, np.maximum(soft, np.maximum(a, crest)),
                                     np.maximum(a, crest))
        neg[y0:y1, x0:x1] = np.minimum(neg[y0:y1, x0:x1], trough)
    # troughs never cut into another crest
    return pos + neg * smooth01(1 - pos / 0.25)


def lip(sc):
    return LIP_MM * np.exp(-((sc.d - LIP_D_MM) / 0.5) ** 2)


def ring_field(rng, sc, break_bias=LAP_BREAK_BIAS):
    """Broken, wobbling contour crests round rock ∪ shore; returns (relief, near)."""
    d = sc.d
    dw = np.maximum(d + LAP_WOBBLE_MM * noise(rng, d.shape, 3.0 / CELL_MM), 0)
    phase = np.log1p(LAP_GROWTH * dw) / (LAP_GROWTH * LAP_LAMBDA0_MM)
    crest = ((1 + np.cos(2 * np.pi * phase)) / 2) ** LAP_PROFILE_P
    k = np.round(phase)
    gate_field = noise(rng, d.shape, LAP_BREAK_SIGMA_MM / CELL_MM)
    n = d.shape[0]
    sg = 2.5 / CELL_MM
    w = sc.water.astype(float)
    ny = ndimage.gaussian_filter(sc.near_iy * w, sg) / np.maximum(ndimage.gaussian_filter(w, sg), 1e-6)
    nx = ndimage.gaussian_filter(sc.near_ix * w, sg) / np.maximum(ndimage.gaussian_filter(w, sg), 1e-6)
    gy = np.clip((ny + k * 37).astype(int), 0, n - 1)
    gx = np.clip((nx + k * 53).astype(int) % n, 0, n - 1)
    gate = smooth01((gate_field[gy, gx] + break_bias) / 0.5 + 0.5)
    gate *= 0.5 + 0.5 * smooth01(0.5 + 0.5 * noise(rng, d.shape, 5.0 / CELL_MM)[gy, gx])
    gate = np.where(k == 0, 1.0, gate)
    A0 = np.where(sc.near_rock, LAP_ROCK_A_MM, LAP_SHORE_A_MM)
    dec = np.where(sc.near_rock, LAP_ROCK_DECAY, LAP_SHORE_DECAY)
    env = A0 * np.exp(-d / dec)
    return env * crest * gate + lip(sc), np.clip(env / max(LAP_ROCK_A_MM, LAP_SHORE_A_MM), 0, 1)


def gen_combed(rng, sc):
    th = direction_field(rng, sc)
    return ripple_strokes(rng, sc, th) + lip(sc)


def gen_lapped(rng, sc):
    th = direction_field(rng, sc)
    rings, _ = ring_field(rng, sc, break_bias=LAPPED_BREAK_BIAS)
    w_near = 1 - smooth01((sc.d - LAPPED_NEAR_MM) / 3.0)
    return rings * w_near + ripple_strokes(rng, sc, th, min_d=LAPPED_NEAR_MM) * (1 - w_near * 0.7)


def gen_rings(rng, sc):
    rel, _ = ring_field(rng, sc)
    return rel


def gen_legacy(rng, sc):
    """Port of dharmatiles' water: noise + scattered-segment sine ripples."""
    z = LEG_NOISE_RMS_MM * noise(rng, sc.X.shape, LEG_NOISE_SIGMA_MM / CELL_MM)
    sg = LEG_ARC_SMOOTH_MM / CELL_MM
    w = sc.water.astype(float)
    ds = ndimage.gaussian_filter(sc.d * w, sg) / np.maximum(ndimage.gaussian_filter(w, sg), 1e-6)
    nr = ndimage.gaussian_filter(sc.near_iy.astype(float), sg) * CELL_MM
    nc = ndimage.gaussian_filter(sc.near_ix.astype(float), sg) * CELL_MM
    src = np.argwhere(ndimage.binary_dilation(~sc.water) & sc.water)
    n_seg = max(4, int(len(src) * CELL_MM / 6.0))
    for i in rng.integers(0, len(src), n_seg):
        cy, cx = src[i] * CELL_MM
        d0 = rng.uniform(*LEG_OFFSET_MM)
        A = LEG_AMP_MM * max(0.0, 1 - d0 / LEG_OFFSET_MM[1])
        half = rng.uniform(*LEG_SEG_LEN_MM) / 2
        sd = np.hypot(nr - cy, nc - cx)
        tail = sd - half
        fade = np.where(sd <= half, 1.0, np.where(tail <= LEG_SEG_FADE_MM,
                        (0.5 * (1 + np.cos(np.pi * tail / LEG_SEG_FADE_MM))) ** 0.4, 0.0))
        band = (ds >= d0) & (ds <= d0 + LEG_LAMBDA_MM)
        z += np.where(band, A * fade * np.sin(2 * np.pi * (ds - d0) / LEG_LAMBDA_MM), 0.0)
    return z


def _sharpen(u, k):
    """Sharp crests, broad troughs: u + k(u² − <u²>) on unit-RMS u."""
    u = u / (u.std() + 1e-12)
    u = 3.0 * np.tanh(u / 3.0)                     # tame outliers before squaring
    v = u + k * (u * u - 1.0) * 0.5
    return v / (v.std() + 1e-12)


def _wave_sim(rng, sc, wind=None):
    """2D damped linear wave equation on the scene.

    Waves are INJECTED in a band at the water edges of the tile (a sum of
    plane waves at several wavelengths, directions spread about the incoming
    direction) and propagate inward.  Speed falls with depth (c ∝ √depth,
    so waves shorten and steepen in the shallows — shoaling); rock and land
    are fixed (reflecting) boundaries, so waves diffract round the rock and
    reflect off rock and bank.  Returns a unit-RMS snapshot on the scene grid.
    """
    f = int(round(SIM_CELL_MM / CELL_MM))
    land = sc.land[::f, ::f]
    rock = sc.rock_mask[::f, ::f]
    depth = np.clip(WL_MM - land, 0, None)
    wet = (depth > 0.02) & ~rock
    c = SIM_C * np.sqrt(np.clip(depth / SIM_DEPTH_MM, SIM_MIN_DEPTH, 1.0))
    c2 = np.where(wet, c * c, 0.0)
    ny, nx = land.shape
    yy, xx = np.mgrid[0:ny, 0:nx] * SIM_CELL_MM
    # Injection band along the water edges (right, top, bottom), fading in.
    band = np.zeros((ny, nx))
    for dist in (sc.W - xx, yy, sc.H - yy):
        band = np.maximum(band, 1 - smooth01(dist / SIM_SPONGE_MM))
    band *= wet
    if wind is None:
        dirs0 = rng.uniform(0, 2 * np.pi, SIM_N_FREQ)        # every which way
        spread = np.radians(60.0)
    else:
        dirs0 = np.full(SIM_N_FREQ, wind)
        spread = np.radians(30.0)
    lam = np.exp(rng.uniform(*np.log(SIM_LAMBDA_MM), SIM_N_FREQ))
    comps = []
    for L, d0 in zip(lam, dirs0):
        for _ in range(3):                                    # 3 sub-directions each
            th = d0 + rng.normal(0, spread)
            k = 2 * np.pi / L                                 # per mm
            w = 2 * np.pi * SIM_C / (L / SIM_CELL_MM)         # per step
            a = (L / SIM_LAMBDA_MM[1]) ** SIM_AMP_EXP
            comps.append((a, k * np.cos(th), k * np.sin(th), w, rng.uniform(0, 2 * np.pi)))
    h = np.zeros((ny, nx))
    hp = np.zeros((ny, nx))
    for t in range(SIM_STEPS):
        lap = (np.roll(h, 1, 0) + np.roll(h, -1, 0) + np.roll(h, 1, 1)
               + np.roll(h, -1, 1) - 4 * h)
        hn = (2 - SIM_DAMP) * h - (1 - SIM_DAMP) * hp + c2 * lap
        target = np.zeros((ny, nx))
        for a, kx, ky, w, ph in comps:
            target += a * np.sin(kx * xx + ky * yy - w * t + ph)
        hn = hn * (1 - 0.3 * band) + 0.3 * band * target
        hn[~wet] = 0.0
        hp, h = h, hn
    from scipy.ndimage import zoom
    full = zoom(h, f, order=3)
    full = np.pad(full, ((0, max(sc.X.shape[0] - full.shape[0], 0)),
                         (0, max(sc.X.shape[1] - full.shape[1], 0))), mode='edge')
    full = full[:sc.X.shape[0], :sc.X.shape[1]] * sc.water
    return full / (full[sc.water].std() + 1e-12)


def _fantasy(u, sc, rms):
    near = np.exp(-sc.d / SIM_NEAR_MM)
    z = _sharpen(u, SIM_SHARPEN) * rms * (1 + SIM_NEAR_BOOST * near)
    return z + lip(sc) * 0.6


def gen_sim(rng, sc):
    return _fantasy(_wave_sim(rng, sc), sc, SIM_RMS_MM)


def gen_sim_wind(rng, sc):
    wind = rng.uniform(0, 2 * np.pi)
    return _fantasy(_wave_sim(rng, sc, wind=wind), sc, SIM_RMS_MM)


def gen_sim_rings(rng, sc):
    """Simulated water, quieter, with the obstacle rings riding on it: the rings
    carry the rock/shore reaction, the simulation supplies related scales."""
    u = _wave_sim(rng, sc)
    rings, near = ring_field(rng, sc, break_bias=0.25)
    calm = 1 - 0.85 * smooth01(1 - sc.d / 7.0)          # chop yields to the rings
    base = _sharpen(u, SIM_SHARPEN) * SIM_RMS_MM * 0.75 * calm
    return base + rings * 1.25


def gen_spectrum(rng, sc):
    """Continuous wave spectrum with Stokes crests, shoaled near obstacles."""
    X, Y = sc.X, sc.Y
    wind = rng.uniform(0, 2 * np.pi)
    kgain = 1 + SPEC_SHOAL_K * np.exp(-sc.d / SPEC_SHOAL_MM)    # shorter near obstacles
    again = 1 + 0.9 * np.exp(-sc.d / SPEC_SHOAL_MM)              # taller near obstacles
    z = np.zeros(X.shape)
    lam = np.exp(rng.uniform(*np.log(SPEC_LAMBDA_MM), SPEC_N))
    for L in lam:
        th = wind + np.radians(rng.normal(0, SPEC_SPREAD_DEG))
        k = 2 * np.pi / L
        a = L ** SPEC_AMP_EXP
        ph = k * kgain * (X * np.cos(th) + Y * np.sin(th)) + rng.uniform(0, 2 * np.pi)
        z += a * (np.cos(ph) + SPEC_STEEP * 0.5 * (k * a * 0.15) * np.cos(2 * ph))
    z = z * sc.water
    z = z / (z[sc.water].std() + 1e-12)
    return _sharpen(z, 0.5) * SPEC_RMS_MM * again + lip(sc) * 0.6


def _geodesic(src, passable, cell):
    """Multi-source shortest-path distance through `passable` cells (8-conn);
    returns (dist mm, index of the nearest source cell) on that grid."""
    from scipy.sparse import coo_matrix
    from scipy.sparse.csgraph import dijkstra
    ny, nx = passable.shape
    idx = np.arange(ny * nx).reshape(ny, nx)
    rows, cols, w = [], [], []
    ok = passable | src
    for dy, dx in ((0, 1), (1, 0), (1, 1), (1, -1)):
        a = ok[max(0, -dy):ny - max(0, dy), max(0, -dx):nx - max(0, dx)]
        b = ok[max(0, dy):, max(0, dx):][:a.shape[0], :a.shape[1]] if dx >= 0 else \
            ok[dy:, :nx + dx][:a.shape[0], :a.shape[1]]
        ia = idx[max(0, -dy):ny - max(0, dy), max(0, -dx):nx - max(0, dx)]
        ib = idx[max(0, dy):, max(0, dx):][:a.shape[0], :a.shape[1]] if dx >= 0 else \
            idx[dy:, :nx + dx][:a.shape[0], :a.shape[1]]
        m = a & b
        rows.append(ia[m]); cols.append(ib[m])
        w.append(np.full(m.sum(), cell * np.hypot(dy, dx)))
    r, c, ww = np.concatenate(rows), np.concatenate(cols), np.concatenate(w)
    G = coo_matrix((np.concatenate([ww, ww]), (np.concatenate([r, c]), np.concatenate([c, r]))),
                   shape=(ny * nx, ny * nx)).tocsr()
    srcs = idx[src]
    d, _, near = dijkstra(G, indices=srcs, min_only=True, return_predecessors=True)
    d = np.where(np.isfinite(d), d, 1e3).reshape(ny, nx)
    return d, near.reshape(ny, nx)


def _emit_sources(rng, sc):
    """Per source: (strength, decay, λ0, growth, dist mm, nearest y, nearest x)
    on the scene grid.  Land and rock distances are geodesic through water, so
    land waves wrap round the rock; edge sources are straight distances."""
    f = int(round(EMIT_CELL_MM / CELL_MM))
    water = sc.water[::f, ::f]
    land = sc.land_mask[::f, ::f]
    rock = sc.rock_mask[::f, ::f]
    ny, nx = water.shape
    from scipy.ndimage import zoom

    def up(a, order=1):
        out = zoom(a.astype(float), f, order=order)
        out = np.pad(out, ((0, max(sc.X.shape[0] - out.shape[0], 0)),
                           (0, max(sc.X.shape[1] - out.shape[1], 0))), mode='edge')
        return out[:sc.X.shape[0], :sc.X.shape[1]]

    out = []
    for name, src in (('land', land), ('rock', rock)):
        if not src.any():
            continue
        border = src & ndimage.binary_dilation(water)
        d, near = _geodesic(border, water, EMIT_CELL_MM)
        nyy, nxx = np.divmod(np.maximum(near, 0), nx)
        st, dec, lam, g = EMIT_SOURCES[name]
        # Smooth: 8-connected geodesic distance is faceted (octagonal), which
        # shows as stepped crests; smoothing also rounds crests around bumps.
        wm = sc.water.astype(float)

        def wsmooth(a, sigma_mm):          # normalised convolution inside water only
            sg = sigma_mm / CELL_MM
            return ndimage.gaussian_filter(a * wm, sg) / np.maximum(ndimage.gaussian_filter(wm, sg), 1e-6)
        dd = wsmooth(np.minimum(up(d), 200.0), 1.2)
        sm = lambda a: wsmooth(up(a) * f, 2.5)
        out.append((st, dec, lam, g, dd, sm(nyy), sm(nxx)))
    # Fallback: waves come in from EMIT_N_EDGES random tile edges, travelling
    # perpendicular to the edge.  Weak, so they only show where land and rocks
    # don't reach.
    st, dec, lam, g = EMIT_SOURCES['edge']
    X, Y = sc.X, sc.Y
    edges = {'S': (Y, X), 'N': (sc.H - Y, X), 'W': (X, Y), 'E': (sc.W - X, Y)}
    for e in rng.choice(['S', 'N', 'E'], EMIT_N_EDGES, replace=False):   # W is land here
        dist, along = edges[e]
        ii = np.clip(np.round(along / CELL_MM), 0, sc.X.shape[1] - 1)
        out.append((st * rng.uniform(0.85, 1.15), dec, lam * rng.uniform(0.85, 1.2), g, dist, ii, ii))
    return out


def _train(rng, d, ny_, nx_, lam0, g, n, gated=True, wobble_grow=0.0, break_bias=None):
    """Crest train on a distance field: wobbled phase, sharpened crests, breaks
    gated per crest by noise at the crest's source point; plus a shorter
    riding wave (related scale)."""
    # Coherence loss: phase wobble grows with distance from the source, so a
    # source's crests are crisp near it and dissolve into chaos far away.
    wob = EMIT_WOBBLE_MM + wobble_grow * np.minimum(d, 80.0)
    dw = np.maximum(d + wob * noise(rng, d.shape, 4.0 / CELL_MM), 0)
    if g > 0:
        phase = np.log1p(g * dw) / (g * lam0)
    else:
        phase = dw / lam0
    out = np.zeros(d.shape)
    for mult, amp in ((1.0, 1.0), (EMIT_RIDER[0], EMIT_RIDER[1])):
        ph = phase * mult + rng.uniform(0, 1)
        p_ = EMIT_PROFILE_P if mult >= 1 else 1.0          # swell stays round
        crest = ((1 + np.cos(2 * np.pi * ph)) / 2) ** p_
        k = np.round(ph)
        gf = noise(rng, d.shape, EMIT_BREAK_SIGMA_MM / CELL_MM)
        gy = np.clip((ny_ + k * 37).astype(int), 0, n - 1)
        gx = np.clip((nx_ + k * 53).astype(int) % n, 0, n - 1)
        bb = EMIT_BREAK_BIAS if break_bias is None else break_bias
        gate = smooth01((gf[gy, gx] + bb) / 0.5 + 0.5) if (mult >= 1 and gated) else 1.0
        out += amp * crest * gate
    return out


def _band_train(rng, d, band, wobble_grow, n_comp):
    """A source's wave packet: n_comp crest trains at wavelengths spread over the
    source's band (log-spaced, jittered), amplitude ∝ λ^BAND_AMP_EXP, each with
    its own phase and its own coherence wobble; summed, then crest-sharpened
    so the interference forms crisp crests over broad troughs (not blobs)."""
    lo, hi, g = band
    lams = np.exp(np.linspace(np.log(lo), np.log(hi), n_comp))
    lams *= np.exp(rng.uniform(-0.12, 0.12, n_comp))
    out = np.zeros(d.shape)
    for L in lams:
        wob = EMIT_WOBBLE_MM + wobble_grow * np.minimum(d, 80.0) * (L / hi) ** -0.3
        dw = np.maximum(d + wob * noise(rng, d.shape, max(1.2 * L, 2.5) / CELL_MM), 0)
        phase = np.log1p(g * dw) / (g * L) if g > 0 else dw / L
        a = (L / hi) ** BAND_AMP_EXP
        out += a * np.cos(2 * np.pi * phase + rng.uniform(0, 2 * np.pi))
    u = out / (out.std() + 1e-12)
    u = 2.5 * np.tanh(u / 2.5)
    v = u + BAND_SHARPEN * 0.5 * (u * u - 1.0)       # Stokes-like crests
    return (v - v.mean()) / (v.std() + 1e-12)


def gen_spectral_chaos(rng, sc):
    """emitted-chaos with a frequency BAND per emitter type: land emits long
    and medium waves, rocks short choppy wavelets, corners long weak swell."""
    n = sc.X.shape[0]
    phys = [(src, kind) for src, kind in zip(_emit_sources(rng, sc), ('land', 'rock', 'e', 'e'))
            if src[1] < 1e3 and src[0] > 0.4]
    if not sc.land_mask.any():                      # map kinds when land is absent
        phys = [(src, 'rock') for src, _ in phys]
    corners = [(src, 'corner') for src in _corner_sources(rng, sc)]
    num = np.zeros(sc.X.shape); den = np.zeros(sc.X.shape)
    lin = np.zeros(sc.X.shape); w2 = np.zeros(sc.X.shape)
    order = np.zeros(sc.X.shape)
    for (st, dec, _lam, _g, d, ny_, nx_), kind in phys + corners:
        band, ncomp = BANDS[kind]
        w = st * np.exp(-d / dec)
        tr = _band_train(rng, d, band, CHAOS_WOBBLE_GROW * (0.5 if kind == 'corner' else 1.0), ncomp)
        num += w ** EMIT_SHARP * tr
        den += w ** EMIT_SHARP
        lin += w * tr
        w2 += w * w
        if kind != 'corner':
            order = np.maximum(order, w)
    dominant = num / np.maximum(den, 1e-12)
    chaos = lin / np.maximum(np.sqrt(w2), 1e-9)
    c = smooth01((order - CHAOS_ORDER_LO) / (CHAOS_ORDER_HI - CHAOS_ORDER_LO))
    z = (c * dominant + (1 - c) * chaos) * (CHAOS_FAR_AMP + (1 - CHAOS_FAR_AMP) * c)
    z = z - z[sc.water].mean()
    return z * BAND_AMP_MM * sc.water + lip(sc)


def _swell_train(rng, d, band, wobble_grow, n_comp):
    """Like _band_train, but scale-aware: the long components stay ROUND
    (broad rolling swells), only the short ones are crest-sharpened, and the
    long ones dominate (amplitude ∝ λ^SWELL_AMP_EXP)."""
    lo, hi, g = band
    lams = np.exp(np.linspace(np.log(lo), np.log(hi), n_comp))
    lams *= np.exp(rng.uniform(-0.1, 0.1, n_comp))
    split = np.sqrt(lo * hi)
    long_, short_ = np.zeros(d.shape), np.zeros(d.shape)
    for L in lams:
        wob = EMIT_WOBBLE_MM + wobble_grow * np.minimum(d, 80.0) * (L / hi) ** -0.3
        dw = np.maximum(d + wob * noise(rng, d.shape, max(1.2 * L, 2.5) / CELL_MM), 0)
        phase = np.log1p(g * dw) / (g * L) if g > 0 else dw / L
        a = (L / hi) ** SWELL_AMP_EXP
        comp = a * np.cos(2 * np.pi * phase + rng.uniform(0, 2 * np.pi))
        if L >= split:
            long_ += comp
        else:
            short_ += comp
    def sharp(u, k):
        sd = u.std() + 1e-12
        u = u / sd
        u = 2.5 * np.tanh(u / 2.5)
        return (u + k * 0.5 * (u * u - 1.0)) * sd
    out = sharp(long_, SWELL_LONG_SHARPEN) + sharp(short_, SWELL_SHORT_SHARPEN)
    return (out - out.mean()) / (out.std() + 1e-12)


def gen_swell_chaos(rng, sc):
    """spectral-chaos, round 7: longer land band dominated by broad round
    swells with sharper short waves riding on them; ~2× relief; stronger rock
    chop with a downstream wake; same order→chaos hand-over and corners."""
    n = sc.X.shape[0]
    srcs = _emit_sources(rng, sc)
    kinds = []
    for src in srcs:
        if src[1] >= 1e3 or src[0] <= 0.4:
            continue
        kinds.append((src, 'land' if (sc.land_mask.any() and src[0] >= 0.99) else 'rock'))
    corners = [(src, 'corner') for src in _corner_sources(rng, sc)]
    # Downstream of the rock = away from the land (land waves travel outward).
    land_src = [s_ for s_, k in kinds if k == 'land']
    num = np.zeros(sc.X.shape); den = np.zeros(sc.X.shape)
    lin = np.zeros(sc.X.shape); w2 = np.zeros(sc.X.shape)
    order = np.zeros(sc.X.shape)
    for (st, dec, _lam, _g, d, ny_, nx_), kind in kinds + corners:
        band, ncomp = SWELL_BANDS[kind]
        w = st * np.exp(-d / dec)
        if kind == 'rock':
            w = w * SWELL_ROCK_GAIN
            if land_src:
                dl = land_src[0][4]
                gy, gx = np.gradient(dl, CELL_MM)          # direction of land-wave travel
                ry, rx = np.gradient(d, CELL_MM)           # away from the rock
                down = np.clip(gx * rx + gy * ry, 0, 1)     # 1 = downstream side
                w = w * (1 + SWELL_WAKE * down * np.exp(-d / (2.5 * dec)))
        tr = _swell_train(rng, d, band, CHAOS_WOBBLE_GROW * (0.5 if kind == 'corner' else 1.0), ncomp)
        num += w ** EMIT_SHARP * tr
        den += w ** EMIT_SHARP
        lin += w * tr
        w2 += w * w
        if kind != 'corner':
            order = np.maximum(order, np.minimum(w, 1.0))
    dominant = num / np.maximum(den, 1e-12)
    chaos = lin / np.maximum(np.sqrt(w2), 1e-9)
    c = smooth01((order - CHAOS_ORDER_LO) / (CHAOS_ORDER_HI - CHAOS_ORDER_LO))
    z = (c * dominant + (1 - c) * chaos) * (CHAOS_FAR_AMP + (1 - CHAOS_FAR_AMP) * c)
    z = z - z[sc.water].mean()
    z = z * SWELL_AMP_MM
    # Soft floor: troughs deeper than the underwater bed (WL − 1 mm) would be
    # clipped flat by max(land, water) → terraces.  Compress them smoothly.
    fl, k = -SWELL_TROUGH_MM, 0.25
    z = fl + k * np.log1p(np.exp((z - fl) / k))
    return z * sc.water + lip(sc)


def gen_emitted(rng, sc, mode='blend'):
    """Waves radiate from sources.  'blend': a partition of unity by source
    influence, so each region is ruled by its dominant source and only a few
    directions cross; 'sum': trains add (more interference)."""
    n = sc.X.shape[0]
    num = np.zeros(sc.X.shape)
    den = np.zeros(sc.X.shape)
    tot = np.zeros(sc.X.shape)
    for st, dec, lam, g, d, ny_, nx_ in _emit_sources(rng, sc):
        w = st * np.exp(-d / dec)
        tr = _train(rng, d, ny_, nx_, lam, g, n, gated=dec < 1e3 and st > 0.4)
        if mode == 'blend':
            wq = w ** EMIT_SHARP                 # dominant source rules; short hand-over
            num += wq * tr
            den += wq
            tot = np.maximum(tot, w)
        else:
            num += w * tr
    if mode == 'blend':
        z = num / np.maximum(den, 1e-9) * (0.35 + 0.65 * np.sqrt(tot))   # far waves quieter, still readable
    else:
        z = num
    z = z - z[sc.water].mean()
    return z * EMIT_AMP_MM * sc.water + lip(sc)


def _corner_sources(rng, sc):
    """Weak radial emitters at the open-water tile corners (corners not on land).
    On an all-water tile that's all four → non-directional interference."""
    out = []
    st, dec, lam, g = CHAOS_CORNER
    for cx, cy in ((0, 0), (sc.W, 0), (0, sc.H), (sc.W, sc.H)):
        iy = int(min(cy / CELL_MM, sc.X.shape[0] - 1))
        ix = int(min(cx / CELL_MM, sc.X.shape[1] - 1))
        if not sc.water[max(iy - 20, 0):iy + 20, max(ix - 20, 0):ix + 20].mean() > 0.5:
            continue
        d = np.hypot(sc.X - cx, sc.Y - cy)
        out.append((st * rng.uniform(0.8, 1.2), dec, lam * rng.uniform(0.8, 1.25), g, d,
                    np.full(d.shape, iy), np.full(d.shape, ix)))
    return out


def gen_emitted_chaos(rng, sc):
    """Physical emitters (land strong, rock medium) rule near themselves with
    crisp crests; their coherence decays with distance (phase wobble grows),
    and weak open-water corner emitters take over, summing into
    non-directional interference far from anything physical."""
    n = sc.X.shape[0]
    phys = [src for src in _emit_sources(rng, sc) if src[1] < 1e3 and src[0] > 0.4]
    srcs = [(s_, True) for s_ in phys] + [(s_, False) for s_ in _corner_sources(rng, sc)]
    num = np.zeros(sc.X.shape); den = np.zeros(sc.X.shape)
    lin = np.zeros(sc.X.shape); w2 = np.zeros(sc.X.shape)
    order = np.zeros(sc.X.shape)
    for (st, dec, lam, g, d, ny_, nx_), physical in srcs:
        w = st * np.exp(-d / dec)
        tr = _train(rng, d, ny_, nx_, lam, g, n, gated=physical,
                    wobble_grow=CHAOS_WOBBLE_GROW if physical else CHAOS_WOBBLE_GROW * 0.5,
                    break_bias=CHAOS_BREAK_BIAS)
        tr = tr - tr.mean()
        num += w ** EMIT_SHARP * tr
        den += w ** EMIT_SHARP
        lin += w * tr
        w2 += w * w
        if physical:
            order = np.maximum(order, w)
    dominant = num / np.maximum(den, 1e-12)
    chaos = lin / np.maximum(np.sqrt(w2), 1e-9)          # interference, amplitude-preserving
    c = smooth01((order - CHAOS_ORDER_LO) / (CHAOS_ORDER_HI - CHAOS_ORDER_LO))
    amp = CHAOS_FAR_AMP + (1 - CHAOS_FAR_AMP) * c        # physical sources louder
    z = (c * dominant + (1 - c) * chaos) * amp
    z = z - z[sc.water].mean()
    return z * EMIT_AMP_MM * sc.water + lip(sc)


def gen_emitted_sum(rng, sc):
    return gen_emitted(rng, sc, mode='sum') * 0.8


GENERATORS = {'swell-chaos': gen_swell_chaos, 'spectral-chaos': gen_spectral_chaos, 'emitted-chaos': gen_emitted_chaos, 'emitted': gen_emitted, 'emitted-sum': gen_emitted_sum,
              'sim-rings': gen_sim_rings, 'sim': gen_sim, 'sim-wind': gen_sim_wind, 'spectrum': gen_spectrum,
              'combed': gen_combed, 'lapped': gen_lapped, 'rings': gen_rings,
              'legacy': gen_legacy}


# ── Build ────────────────────────────────────────────────────────────────────
def build(sc, relief, tide=None):
    from dharmatiles.core.mesh import make_heightmap_solid
    from dharmatiles.core.config import BaseConfig, SurfaceConfig
    from dharmatiles.bases import dungeonblocks as db
    # Fade relief at banks (into the shallows) and at the rim.
    shore_fade = smooth01(sc.d / 0.4)
    water_z = WL_MM + relief * sc.rim * np.where(sc.water, 1.0, 0.0) * shore_fade
    if tide is not None:
        water_z = water_z + tide
    z = np.maximum(sc.land, water_z)
    z = ndimage.gaussian_filter(z, 0.6)          # remove grid-scale kinks (≤0.1 mm)
    top = make_heightmap_solid(z, sc.W, sc.H, base_h=0.0, subsample=MESH_SUBSAMPLE)
    solid = rl.to_manifold(top) + sc.rock if sc.rock is not None else rl.to_manifold(top)
    zmax = solid.bounding_box()[5]
    surface = SurfaceConfig(cols=COLS, rows=ROWS, square_mm=SQUARE_MM)
    base_cfg = BaseConfig()
    peg = db.select_peg_height(np.zeros((2, 2)), base_cfg, zmax)
    solid = solid + rl.to_manifold(db.make_base(surface, peg, base_cfg))
    parts = solid.decompose()
    solid = max(parts, key=lambda p: p.volume())
    mesh = rl.to_trimesh(solid)
    chk = trimesh.Trimesh(np.asarray(mesh.vertices, dtype=np.float32).astype(np.float64),
                          np.asarray(mesh.faces), process=True)
    if not chk.is_watertight:
        raise RuntimeError('mesh is not watertight after STL-style merge')
    return chk, z


def mode_rng(seed, mode):
    """Per-option RNG keyed by the option's NAME (stable across reorderings)."""
    import zlib
    return np.random.default_rng([seed, zlib.crc32(mode.encode())])


def stats(sc, relief, z):
    r = relief[sc.water & (sc.rim > 0.99)]
    gy, gx = np.gradient(z, CELL_MM)
    sl = np.degrees(np.arctan(np.hypot(gx, gy)))[sc.water]
    return dict(relief_min=float(r.min()), relief_max=float(r.max()),
                relief_rms=float(r.std()), max_slope=float(sl.max()))


def self_check(mesh, path):
    from dharmatiles.render import render
    m = mesh.copy()
    m.visual.face_colors = np.tile([150, 150, 150, 255], (len(m.faces), 1))
    render([m], path, elev=40, azim=-120, resolution=(1400, 1100), quiet=True)
    s = m.slice_plane([0, 0, 0.5], [0, 0, 1], cap=False)
    render([s], path.with_name(path.stem + '-top.png'), elev=89, azim=-90,
           resolution=(1200, 1200), quiet=True)
    render([s], path.with_name(path.stem + '-low.png'), elev=18, azim=-150,
           resolution=(1400, 900), quiet=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--mode', default='lapping', choices=list(GENERATORS) + ['all'])
    ap.add_argument('--seed', type=int, default=1)
    ap.add_argument('--check', type=pathlib.Path, default=None)
    ap.add_argument('--open', action='store_true', help='all-water tile (no land, no rock)')
    ap.add_argument('--scene', default=None, choices=list(SCENARIOS))
    args = ap.parse_args()
    modes = list(GENERATORS) if args.mode == 'all' else [args.mode]
    sc = Scene(args.seed, open_water=args.open, scenario=args.scene)
    out = pathlib.Path(__file__).parent / 'out'
    out.mkdir(exist_ok=True)
    for mode in modes:
        t0 = time.perf_counter()
        relief = GENERATORS[mode](mode_rng(args.seed, mode), sc)
        mesh, z = build(sc, relief, sc.tide)
        path = out / f"2x2-water-{mode}{'' if sc.scenario == 'bank-rock' else '-' + sc.scenario}-s{args.seed}.stl"
        mesh.export(path)
        st = stats(sc, relief, z)
        print(f"→ {path}  ({path.stat().st_size / 1e6:.1f} MB) {len(mesh.faces):,} faces, "
              f"watertight={mesh.is_watertight}; relief {st['relief_min']:+.2f}…{st['relief_max']:+.2f} mm "
              f"(rms {st['relief_rms']:.2f}), max water slope {st['max_slope']:.0f}°; "
              f"{time.perf_counter() - t0:.1f}s")
        if args.check:
            p = args.check.with_name(f"{args.check.stem}-{mode}.png")
            self_check(mesh, p)


if __name__ == '__main__':
    main()
