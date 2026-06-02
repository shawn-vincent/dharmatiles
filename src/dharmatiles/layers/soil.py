"""
SoilLayer: two-tier random super-Gaussian blob clumps baked into terrain_z.

The bump field is computed at 2× the coarse grid resolution so individual
soil mounds have sub-cell detail; the result is then downsampled back to
the coarse grid and added to scene.terrain_z in-place.
"""
from __future__ import annotations

_OVERSAMPLE = 2   # internal upscale factor for soil blob detail

import numpy as np
from scipy.ndimage import gaussian_filter, zoom

from ..core.config import SurfaceConfig, SoilConfig
from ..core.tile import TileScene


class SoilLayer:
    """Scatter random super-Gaussian soil clumps into scene.terrain_z."""

    def __init__(self, surface: SurfaceConfig, soil: SoilConfig) -> None:
        self.surface = surface
        self.soil    = soil

    def build(self, scene: TileScene) -> None:
        """Compute soil bump at 2× resolution, downsample, add to terrain_z."""
        gh, gw    = scene.terrain_z.shape
        cell_mm   = self.surface.cell_w / _OVERSAMPLE
        n_squares = self.surface.cols * self.surface.rows

        hires_bump = _compute_bump_field(
            soil=self.soil,
            seed=self.surface.seed,
            gh=gh * _OVERSAMPLE, gw=gw * _OVERSAMPLE,
            cell_mm=cell_mm,
            n_squares=n_squares,
        )

        coarse = zoom(hires_bump, 1.0 / _OVERSAMPLE, order=1)
        scene.terrain_z += coarse


# ── Internal implementation ───────────────────────────────────────────────────

def _compute_bump_field(soil: SoilConfig, seed: int,
                        gh: int, gw: int, cell_mm: float,
                        n_squares: int) -> np.ndarray:
    """Compute the full soil bump array at (gh × gw) resolution."""
    rng  = np.random.default_rng(seed ^ 0xC01D_50_11)
    bump = np.zeros((gh, gw), dtype=float)
    xi   = np.arange(gw, dtype=float)
    yi   = np.arange(gh, dtype=float)

    # ── Pre-compute per-tile organic perturbation fields ──────────────────────
    warp_str   = soil.blob_warp_str_mm / cell_mm
    warp_sigma = max(1.0, warp_str * 2.5)

    def _nf(sigma: float) -> np.ndarray:
        n = rng.standard_normal((gh, gw))
        n = gaussian_filter(n, sigma=sigma)
        s = n.std()
        return n / s if s > 0 else n

    wx  = _nf(warp_sigma) * warp_str
    wy  = _nf(warp_sigma) * warp_str
    tex = _nf(max(1.0, 1.5))   # fine surface texture, unit-std

    # ── Tier definitions ──────────────────────────────────────────────────────
    tiers = [
        (soil.n_blobs * n_squares,
         soil.blob_sigma_min_mm, soil.blob_sigma_max_mm,
         soil.blob_h_min,        soil.blob_h_max,
         True),    # primary: elliptical + warp + texture
        (soil.n_small * n_squares,
         soil.small_sigma_min_mm, soil.small_sigma_max_mm,
         soil.small_h_min,        soil.small_h_max,
         False),   # fine grain: circular, no extra perturbation
    ]

    for (n, sig_min, sig_max, h_min, h_max, perturb) in tiers:
        if perturb and soil.blob_cluster_count > 0:
            # Cluster process: place blobs around random cluster centres
            spread_cells = soil.blob_cluster_spread_mm / cell_mm
            centres_x = rng.uniform(0.0, gw, soil.blob_cluster_count)
            centres_y = rng.uniform(0.0, gh, soil.blob_cluster_count)
            which     = rng.integers(0, soil.blob_cluster_count, n)
            cx = np.clip(centres_x[which] + rng.normal(0, spread_cells, n), 0, gw - 1)
            cy = np.clip(centres_y[which] + rng.normal(0, spread_cells, n), 0, gh - 1)
        elif perturb and soil.blob_jitter < 1.0:
            # Jittered grid: divide surface into n cells, one blob per cell
            n_cols = max(1, int(round(np.sqrt(n * gw / gh))))
            n_rows = max(1, int(round(n / n_cols)))
            n      = n_cols * n_rows           # actual count may differ slightly
            cw_    = gw / n_cols;  ch_ = gh / n_rows
            base_x = (np.arange(n_cols) + 0.5) * cw_
            base_y = (np.arange(n_rows) + 0.5) * ch_
            bx, by = np.meshgrid(base_x, base_y)
            cx = (bx + soil.blob_jitter * (rng.uniform(-0.5, 0.5, bx.shape) * cw_)).ravel()
            cy = (by + soil.blob_jitter * (rng.uniform(-0.5, 0.5, by.shape) * ch_)).ravel()
        else:
            cx = rng.uniform(0.0, gw, n)
            cy = rng.uniform(0.0, gh, n)
        if perturb and soil.blob_sigma_mode_mm >= sig_min:
            sigma_mm = rng.triangular(sig_min, soil.blob_sigma_mode_mm, sig_max, n)
        else:
            sigma_mm = rng.uniform(sig_min, sig_max, n)
        sigma    = sigma_mm / cell_mm                        # mm → cells
        if perturb:
            # Height scales with blob size; larger blobs biased toward scale_max
            t        = (sigma_mm - sig_min) / (sig_max - sig_min + 1e-8)   # 0→1
            span     = soil.blob_h_scale_max - soil.blob_h_scale_min
            bias     = soil.blob_h_size_bias
            scale_lo = soil.blob_h_scale_min + span * t * bias
            scale_hi = soil.blob_h_scale_min + span * (t * bias + (1.0 - bias))
            scale_hi = np.maximum(scale_lo, scale_hi)
            scale    = rng.uniform(0.0, 1.0, n) * (scale_hi - scale_lo) + scale_lo
            h = sigma_mm * scale
        else:
            h = rng.uniform(h_min, h_max, n)
        if perturb:
            aspect = rng.uniform(soil.blob_aspect_min, soil.blob_aspect_max, n)
            angle  = rng.uniform(0.0, np.pi, n)
        else:
            aspect = np.ones(n)
            angle  = np.zeros(n)

        # Per-blob angular shape noise
        if perturb and soil.blob_shape_noise_amp > 0.0:
            n_harm  = soil.blob_shape_noise_harmonics
            s_amp   = rng.uniform(0.0, soil.blob_shape_noise_amp, (n, n_harm))
            s_phase = rng.uniform(0.0, 2.0 * np.pi,              (n, n_harm))
            s_freq  = np.arange(2, 2 + n_harm, dtype=float)
        else:
            s_amp = s_phase = s_freq = None

        for i in range(n):
            _accumulate_blob(
                bump, xi, yi, gw, gh,
                cx[i], cy[i], sigma[i], h[i],
                aspect[i], angle[i],
                soil.blob_power, soil.blob_cutoff,
                wx if perturb else None,
                wy if perturb else None,
                tex if perturb else None,
                soil.blob_texture_amp if perturb else 0.0,
                s_amp[i]   if s_amp   is not None else None,
                s_phase[i] if s_phase is not None else None,
                s_freq,
            )

    # ── Overall surface texture ───────────────────────────────────────────────
    if soil.surface_texture_amp > 0.0:
        sigma_cells = max(1.0, soil.surface_texture_scale_mm / cell_mm)
        tex_noise   = rng.standard_normal((gh, gw))
        tex_noise   = gaussian_filter(tex_noise, sigma=sigma_cells)
        s = tex_noise.std()
        if s > 0:
            tex_noise /= s
        bump += soil.surface_texture_amp * tex_noise

    if soil.surface_texture2_amp > 0.0:
        sigma_cells2 = max(1.0, soil.surface_texture2_scale_mm / cell_mm)
        tex_noise2   = rng.standard_normal((gh, gw))
        tex_noise2   = gaussian_filter(tex_noise2, sigma=sigma_cells2)
        s = tex_noise2.std()
        if s > 0:
            tex_noise2 /= s
        bump += soil.surface_texture2_amp * tex_noise2

    np.maximum(bump, 0.0, out=bump)   # don't dip below ground

    # ── Edge fade ─────────────────────────────────────────────────────────────
    fade_cx = soil.edge_fade_mm / cell_mm
    fade_cy = soil.edge_fade_mm / cell_mm
    ix   = np.arange(gw, dtype=float)
    iy   = np.arange(gh, dtype=float)
    dx_e = np.minimum(ix, gw - 1 - ix)
    dy_e = np.minimum(iy, gh - 1 - iy)
    fx   = 0.5 * (1.0 - np.cos(np.pi * np.clip(dx_e / fade_cx, 0.0, 1.0)))
    fy   = 0.5 * (1.0 - np.cos(np.pi * np.clip(dy_e / fade_cy, 0.0, 1.0)))
    bump *= np.minimum(fx[np.newaxis, :], fy[:, np.newaxis])

    return bump


def _accumulate_blob(bump, xi, yi, gw, gh,
                     cx, cy, sigma, h, aspect, angle,
                     power, cutoff, wx, wy, tex, tex_amp,
                     s_amp=None, s_phase=None, s_freq=None):
    R    = cutoff * sigma
    r    = int(R) + 1
    x_lo = max(0,      int(cx) - r)
    x_hi = min(gw - 1, int(cx) + r)
    y_lo = max(0,      int(cy) - r)
    y_hi = min(gh - 1, int(cy) + r)
    if x_lo > x_hi or y_lo > y_hi:
        return

    dx = xi[x_lo:x_hi + 1][np.newaxis, :] - cx
    dy = yi[y_lo:y_hi + 1][:, np.newaxis] - cy

    if wx is not None:
        dx = dx + wx[y_lo:y_hi + 1, x_lo:x_hi + 1]
        dy = dy + wy[y_lo:y_hi + 1, x_lo:x_hi + 1]

    ca = np.cos(angle);  sa = np.sin(angle)
    dx_n = ( ca * dx + sa * dy) / sigma
    dy_n = (-sa * dx + ca * dy) / (sigma * aspect)

    if s_amp is not None:
        # Per-blob angular harmonics: modulate radius by angle
        theta        = np.arctan2(dy_n, dx_n)
        radial_scale = np.ones_like(theta)
        for k in range(len(s_freq)):
            radial_scale += s_amp[k] * np.cos(s_freq[k] * theta + s_phase[k])
        radial_scale = np.maximum(0.3, radial_scale)
        dp = ((dx_n * dx_n + dy_n * dy_n) * radial_scale * radial_scale) ** (power * 0.5)
    else:
        dp = (dx_n * dx_n + dy_n * dy_n) ** (power * 0.5)
    g    = np.exp(-dp * 0.5)
    g_R  = float(np.exp(-(cutoff ** power) * 0.5))
    norm = 1.0 - g_R
    if norm < 1e-9:
        return
    shape = np.maximum(0.0, (g - g_R) / norm)

    if tex is not None:
        shape *= np.maximum(0.0, 1.0 + tex_amp * tex[y_lo:y_hi + 1, x_lo:x_hi + 1])

    np.maximum(bump[y_lo:y_hi + 1, x_lo:x_hi + 1], h * shape,
               out=bump[y_lo:y_hi + 1, x_lo:x_hi + 1])
