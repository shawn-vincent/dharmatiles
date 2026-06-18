"""
SoilCarpet: two-tier random super-Gaussian blob clumps baked into terrain_z.

The bump field is computed at 2× the coarse grid resolution so individual
soil mounds have sub-cell detail; the result is then downsampled back to
the coarse grid and draped onto the terrain surface.

Surface draping
---------------
A plain vertical offset (terrain_z += bump) produces asymmetric blobs on
slopes: the uphill face is very steep (slope + bump gradient reinforce)
and the downhill face is shallow (they oppose).  The fix is to treat the
bump as a texture defined in *surface arc-length space* and resample it at
the arc-length-equivalent position for each world cell.

On a slope with local gradient gx = dz/dx, moving one world-XY unit in X
covers sqrt(gx²+1) units of surface arc.  Cumulating those stretches gives
an arc-length coordinate map.  Sampling the flat bump field at those
coordinates compresses bump footprints in world XY to compensate for the
slope stretch, making every bump appear symmetric when viewed along the
surface normal.

Height is then scaled by nz = 1/sqrt(gx²+gy²+1) to convert the
surface-normal displacement into a world-Z increment.
"""
from __future__ import annotations

_OVERSAMPLE = 2   # internal upscale factor for soil blob detail

import numpy as np
from scipy.ndimage import distance_transform_edt, gaussian_filter, map_coordinates, zoom

from typing import ClassVar

from ..core.color import Material
from ..core.config import SoilConfig
from ..core.tile import TileScene, derive_seed
from ..dist import sample


class SoilCarpet:
    """Bake super-Gaussian soil clumps into scene.terrain_z.

    Accepts flat ``SoilConfig`` kwargs; reads the active ``SurfaceConfig``
    Reads ``scene.surface`` inside ``apply()``.
    """

    height_default_mm: float = 5.0
    terrain_material: ClassVar[Material] = Material.SOIL

    def __init__(self, **soil_kwargs) -> None:
        self.soil = SoilConfig(**soil_kwargs)

    def apply(self, scene: TileScene, *,
              placement_mask: np.ndarray | None = None) -> list:
        """Compute soil bump at 2× resolution, downsample, drape onto surface.

        Ends with a ``terrain_support_z[:] = terrain_z`` sync so subsequent
        scatter layers see the updated surface.
        """
        surface   = scene.surface
        gh, gw    = scene.terrain_z.shape
        cell_mm   = surface.cell_w / _OVERSAMPLE
        n_squares = surface.cols * surface.rows

        hires_bump = _compute_bump_field(
            soil=self.soil,
            seed=derive_seed(surface.seed, 'soil-blobs'),
            gh=gh * _OVERSAMPLE, gw=gw * _OVERSAMPLE,
            cell_mm=cell_mm,
            n_squares=n_squares,
        )
        coarse = zoom(hires_bump, 1.0 / _OVERSAMPLE, order=1)

        # ── Arc-length reparameterisation ─────────────────────────────────────
        gz_y, gz_x = np.gradient(scene.terrain_z,
                                  surface.cell_w,
                                  surface.cell_w)
        nz = 1.0 / np.sqrt(gz_x ** 2 + gz_y ** 2 + 1.0)

        stretch_x = np.sqrt(gz_x ** 2 + 1.0)
        stretch_y = np.sqrt(gz_y ** 2 + 1.0)

        arc_cols = np.zeros((gh, gw), dtype=float)
        arc_rows = np.zeros((gh, gw), dtype=float)
        arc_cols[:, 1:] = np.cumsum(stretch_x[:, :-1], axis=1)
        arc_rows[1:, :] = np.cumsum(stretch_y[:-1, :], axis=0)

        projected = map_coordinates(coarse, [arc_rows, arc_cols],
                                    mode='reflect', order=1)

        displacement = projected * nz

        # ── Placement-mask + tile-edge fade ───────────────────────────────────
        if self.soil.edge_fade_mm > 0.0:
            fade_cells = max(self.soil.edge_fade_mm / surface.cell_w, 1e-6)
            ix = np.arange(gw, dtype=float)
            iy = np.arange(gh, dtype=float)
            dx = np.minimum(ix, gw - 1 - ix)
            dy = np.minimum(iy, gh - 1 - iy)
            tile_dist = np.minimum(dx[np.newaxis, :], dy[:, np.newaxis])
            if placement_mask is not None:
                mask_dist = distance_transform_edt(placement_mask)
                dist = np.minimum(mask_dist, tile_dist)
            else:
                dist = tile_dist
            fade = 0.5 * (1.0 - np.cos(np.pi * np.clip(dist / fade_cells, 0.0, 1.0)))
            displacement = displacement * fade

        scene.displace_terrain(displacement, placement_mask)
        return []


# ── Internal implementation ───────────────────────────────────────────────────

def _compute_bump_field(soil: SoilConfig, seed: int,
                        gh: int, gw: int, cell_mm: float,
                        n_squares: int) -> np.ndarray:
    """Compute the full soil bump array at (gh × gw) resolution."""
    rng  = np.random.default_rng(seed)   # seed already derived by caller
    bump = np.zeros((gh, gw), dtype=float)
    xi   = np.arange(gw, dtype=float)
    yi   = np.arange(gh, dtype=float)

    # ── Pre-compute per-tile organic perturbation fields ──────────────────────
    # These are optional: skip allocation entirely when the corresponding
    # parameter is disabled (zero) to avoid spending RNG state and memory.
    warp_str = soil.blob_warp_str_mm / cell_mm

    def _nf(sigma: float) -> np.ndarray:
        n = rng.standard_normal((gh, gw))
        n = gaussian_filter(n, sigma=sigma)
        s = n.std()
        return n / s if s > 0 else n

    if warp_str > 0.0:
        warp_sigma = max(1.0, warp_str * 2.5)
        wx = _nf(warp_sigma) * warp_str
        wy = _nf(warp_sigma) * warp_str
    else:
        wx = None
        wy = None

    tex = _nf(max(1.0, 1.5)) if soil.blob_texture_amp > 0.0 else None

    blob_jitter = float(sample(soil.blob_jitter, rng))
    blob_cluster_count = max(0, int(round(sample(soil.blob_cluster_count, rng))))
    blob_cluster_spread_mm = float(sample(soil.blob_cluster_spread_mm, rng))

    # ── Tier definitions ──────────────────────────────────────────────────────
    # Each tier: (count, sigma distribution, height distribution, perturb).
    # perturb=True → height is sigma_mm × sampled height scale.
    # perturb=False → height is sampled directly.
    tiers = [
        (max(0, int(round(sample(soil.n_blobs, rng)))) * n_squares,
         soil.blob_sigma,
         soil.blob_h_scale,
         True),    # primary: elliptical + warp + texture
        (max(0, int(round(sample(soil.n_small, rng)))) * n_squares,
         soil.small_sigma,
         soil.small_h,
         False),   # fine grain: circular, no extra perturbation
    ]

    for (n, sigma_dist, height_dist, perturb) in tiers:
        if n <= 0:
            continue
        if perturb and blob_cluster_count > 0:
            # Cluster process: place blobs around random cluster centres
            spread_cells = blob_cluster_spread_mm / cell_mm
            centres_x = rng.uniform(0.0, gw, blob_cluster_count)
            centres_y = rng.uniform(0.0, gh, blob_cluster_count)
            which     = rng.integers(0, blob_cluster_count, n)
            cx = np.clip(centres_x[which] + rng.normal(0, spread_cells, n), 0, gw - 1)
            cy = np.clip(centres_y[which] + rng.normal(0, spread_cells, n), 0, gh - 1)
        elif perturb and blob_jitter < 1.0:
            # Jittered grid: divide surface into n cells, one blob per cell
            n_cols = max(1, int(round(np.sqrt(n * gw / gh))))
            n_rows = max(1, int(round(n / n_cols)))
            n      = n_cols * n_rows           # actual count may differ slightly
            cw_    = gw / n_cols;  ch_ = gh / n_rows
            base_x = (np.arange(n_cols) + 0.5) * cw_
            base_y = (np.arange(n_rows) + 0.5) * ch_
            bx, by = np.meshgrid(base_x, base_y)
            cx = (bx + blob_jitter * (rng.uniform(-0.5, 0.5, bx.shape) * cw_)).ravel()
            cy = (by + blob_jitter * (rng.uniform(-0.5, 0.5, by.shape) * ch_)).ravel()
        else:
            cx = rng.uniform(0.0, gw, n)
            cy = rng.uniform(0.0, gh, n)
        sigma_mm = sample(sigma_dist, rng, n)
        sigma    = sigma_mm / cell_mm                        # mm → cells
        if perturb:
            h = sigma_mm * sample(height_dist, rng, n)
        else:
            h = sample(height_dist, rng, n)
        if perturb:
            aspect = sample(soil.blob_aspect, rng, n)
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

        # g_R and norm depend only on cutoff/power (tile constants); pre-compute
        # once per tier rather than recomputing inside every _accumulate_blob call.
        _g_R  = float(np.exp(-(soil.blob_cutoff ** soil.blob_power) * 0.5))
        _norm = 1.0 - _g_R
        if _norm < 1e-9:
            continue   # cutoff so tight nothing survives — skip whole tier

        for i in range(n):
            _accumulate_blob(
                bump, xi, yi, gw, gh,
                cx[i], cy[i], sigma[i], h[i],
                aspect[i], angle[i],
                soil.blob_power, soil.blob_cutoff,
                _g_R, _norm,
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
                     power, cutoff, g_R, norm,
                     wx, wy, tex, tex_amp,
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
    g     = np.exp(-dp * 0.5)
    shape = np.maximum(0.0, (g - g_R) / norm)

    if tex is not None:
        shape *= np.maximum(0.0, 1.0 + tex_amp * tex[y_lo:y_hi + 1, x_lo:x_hi + 1])

    np.maximum(bump[y_lo:y_hi + 1, x_lo:x_hi + 1], h * shape,
               out=bump[y_lo:y_hi + 1, x_lo:x_hi + 1])
