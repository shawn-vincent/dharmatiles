"""
Flow vector field: drives blade lean direction and lateral curl across the tile.

The field is a unit-vector field over the GRID_RES × GRID_RES tile, expressed
as a bearing angle (atan2(fx, fy), 0 = +Y / north, π/2 = +X / east).

Construction
────────────
  1. Analytic base field — one of: swirl / linear / radial / drain / dipole / curl.
  2. Blend with divergence-free curl noise for organic variation.
  3. Derive angle field θ = atan2(fx, fy).
  4. Derive signed curvature κ = ∇θ · f̂, normalised to [−1, 1].

The curvature field drives blade curl: κ ≈ +1 → tight CW bend, κ ≈ −1 → tight CCW.
"""
from __future__ import annotations

import numpy as np

from .tile import TileConfig


def build_flow_field(cfg: TileConfig, x_grid: np.ndarray, y_grid: np.ndarray):
    """Build (angle_field, curv_field) on the tile's GRID_RES × GRID_RES grid.

    Parameters
    ----------
    cfg     : TileConfig — uses ``seed``, ``flow_type``, ``flow_curl_noise``,
              ``gx``, ``gy``.
    x_grid  : (GRID_RES, GRID_RES) — world X coords in mm (from make_xy_grids).
    y_grid  : (GRID_RES, GRID_RES) — world Y coords in mm.

    Returns
    -------
    angle_field : (GRID_RES, GRID_RES) float — bearing angle in radians.
    curv_field  : (GRID_RES, GRID_RES) float in [−1, 1] — signed curvature.
    """
    frng = np.random.default_rng(cfg.seed ^ 0x464C4F57)   # independent from blade rng

    # Normalised tile coords in [−0.5, 0.5]²
    xn = (x_grid / cfg.tile_w - 0.5).astype(float)
    yn = (y_grid / cfg.tile_h - 0.5).astype(float)

    # ── 1. Base field ──────────────────────────────────────────────────────────
    ft = cfg.flow_type

    if ft == 'swirl':
        cx_n = frng.uniform(-0.15, 0.15)
        cy_n = frng.uniform(-0.15, 0.15)
        sign = frng.choice([-1.0, 1.0])   # CW or CCW, per-seed
        dx = xn - cx_n;  dy = yn - cy_n
        r  = np.sqrt(dx**2 + dy**2) + 1e-9
        bfx =  sign * dy / r
        bfy = -sign * dx / r

    elif ft == 'linear':
        angle = frng.uniform(0, 2 * np.pi)
        bfx = np.full_like(xn, np.sin(angle))
        bfy = np.full_like(xn, np.cos(angle))

    elif ft == 'radial':
        cx_n = frng.uniform(-0.15, 0.15)
        cy_n = frng.uniform(-0.15, 0.15)
        dx = xn - cx_n;  dy = yn - cy_n
        r  = np.sqrt(dx**2 + dy**2) + 1e-9
        bfx = dx / r;  bfy = dy / r

    elif ft == 'drain':
        cx_n = frng.uniform(-0.15, 0.15)
        cy_n = frng.uniform(-0.15, 0.15)
        dx = xn - cx_n;  dy = yn - cy_n
        r  = np.sqrt(dx**2 + dy**2) + 1e-9
        bfx = -dx / r;  bfy = -dy / r

    elif ft == 'dipole':
        sep  = frng.uniform(0.15, 0.25)
        ang  = frng.uniform(0, 2 * np.pi)
        cx1, cy1 =  np.cos(ang) * sep,  np.sin(ang) * sep
        cx2, cy2 = -cx1, -cy1
        r1sq = (xn - cx1)**2 + (yn - cy1)**2 + 1e-4
        r2sq = (xn - cx2)**2 + (yn - cy2)**2 + 1e-4
        bfx = (xn - cx1) / r1sq - (xn - cx2) / r2sq
        bfy = (yn - cy1) / r1sq - (yn - cy2) / r2sq

    else:  # 'curl' — pure curl noise; weak +Y bias for orientation
        bfx = np.zeros_like(xn)
        bfy = np.ones_like(xn)

    mag = np.sqrt(bfx**2 + bfy**2) + 1e-9
    bfx, bfy = bfx / mag, bfy / mag

    # ── 2. Curl noise: divergence-free perturbation ────────────────────────────
    # Stream-function P = Σ sinusoids.  curl(P) = (∂P/∂y, −∂P/∂x) is always
    # divergence-free by construction.
    P = np.zeros_like(xn)
    for _ in range(4):
        fx_ = frng.uniform(1.5, 4.0)
        fy_ = frng.uniform(1.5, 4.0)
        phx = frng.uniform(0, 2 * np.pi)
        phy = frng.uniform(0, 2 * np.pi)
        amp = frng.uniform(0.3, 1.0)
        P  += amp * np.sin(fx_ * 2 * np.pi * xn + phx) * np.cos(fy_ * 2 * np.pi * yn + phy)

    dPdy, dPdx = np.gradient(P, cfg.gy, cfg.gx)   # ∂P/∂y axis-0, ∂P/∂x axis-1
    cnx, cny = dPdy, -dPdx
    cmag = np.sqrt(cnx**2 + cny**2) + 1e-9
    cnx /= cmag;  cny /= cmag

    s  = cfg.flow_curl_noise
    fx = (1 - s) * bfx + s * cnx
    fy = (1 - s) * bfy + s * cny
    mag = np.sqrt(fx**2 + fy**2) + 1e-9
    fx /= mag;  fy /= mag

    # ── 3. Angle field ─────────────────────────────────────────────────────────
    angle_field = np.arctan2(fx, fy)   # 0 = +Y (north), π/2 = +X (east)

    # ── 4. Signed curvature κ = ∇θ · f̂ ───────────────────────────────────────
    # Positive = streamline bends CW (increasing azimuth), negative = CCW.
    dθdy, dθdx = np.gradient(angle_field, cfg.gy, cfg.gx)
    kappa = dθdx * fx + dθdy * fy
    scale = np.percentile(np.abs(kappa), 95) + 1e-9
    curv_field = np.clip(kappa / scale, -1.0, 1.0)

    return angle_field.astype(float), curv_field.astype(float)
