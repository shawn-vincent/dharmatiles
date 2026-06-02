"""
SoilLayer: multi-scale bumpy soil texture baked into terrain_z.

Three octaves of smoothed random noise are summed to produce organic-looking
mounds and ripples — large rolling hills, medium clumps, and fine surface
texture — matching the look of bare compacted soil.

The layer modifies ``scene.terrain_z`` in-place before stones or grass are
placed; it produces no mesh of its own.
"""
from __future__ import annotations

import numpy as np
from scipy.ndimage import gaussian_filter

from ..core.config import SurfaceConfig, SoilConfig
from ..core.tile import TileScene


class SoilLayer:
    """Add multi-scale bumpy displacement to scene.terrain_z."""

    def __init__(self, surface: SurfaceConfig, soil: SoilConfig) -> None:
        self.surface = surface
        self.soil    = soil

    def build(self, scene: TileScene) -> None:
        """Modify *scene.terrain_z* in-place; return nothing."""
        rng  = np.random.default_rng(self.surface.seed ^ 0xC01D_50_11)
        soil = self.soil
        gh, gw = scene.terrain_z.shape

        def _octave(sigma: float, amp: float) -> np.ndarray:
            noise = rng.standard_normal((gh, gw))
            if sigma > 0.5:
                noise = gaussian_filter(noise, sigma=sigma)
            # Normalise to [-1, 1] then scale to amplitude
            peak = np.abs(noise).max()
            if peak > 0:
                noise /= peak
            return noise * (amp * 0.5)   # amp = peak-to-peak → half-amp each side

        bump  = _octave(soil.large_sigma,  soil.large_amp)
        bump += _octave(soil.medium_sigma, soil.medium_amp)
        bump += _octave(soil.small_sigma,  soil.small_amp)

        # ── Edge fade: cosine rolloff to zero at tile borders ─────────────────
        fade_cx = soil.edge_fade_mm / self.surface.cell_w   # fade width in cells X
        fade_cy = soil.edge_fade_mm / self.surface.cell_h   # fade width in cells Y

        ix = np.arange(gw, dtype=float)
        iy = np.arange(gh, dtype=float)
        # distance from nearest edge, clamped to [0, fade_width]
        dx = np.minimum(ix, gw - 1 - ix)
        dy = np.minimum(iy, gh - 1 - iy)
        # cosine ease-in: 0 at edge, 1 in interior
        fx = 0.5 * (1.0 - np.cos(np.pi * np.clip(dx / fade_cx, 0.0, 1.0)))
        fy = 0.5 * (1.0 - np.cos(np.pi * np.clip(dy / fade_cy, 0.0, 1.0)))
        mask = np.minimum(fx[np.newaxis, :], fy[:, np.newaxis])  # (gh, gw)

        scene.terrain_z += bump * mask
