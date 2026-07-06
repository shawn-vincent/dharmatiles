"""Isotropic value-noise fBm — the stone-texture noise primitive.

Sum-of-plane-waves noise (``relief_field``) is Gaussian and phase-
coherent: on large flat faces the waves read as directional corduroy /
chop (Shawn's MeshLab find on the floor slabs, 2026-07-05).  Value
noise has no global phase — random lattice values smoothly
interpolated — so it is isotropic and non-repeating by construction,
and evaluating it in 3D object space makes it identical on flat slabs
and curved pebbles.  Design: docs/design/stone-surface-texture.md.
"""
from __future__ import annotations

import numpy as np


def _hash_lattice(ix: np.ndarray, iy: np.ndarray, iz: np.ndarray,
                  seed: int) -> np.ndarray:
    """Deterministic integer-lattice hash → uniform values in [−1, 1]."""
    n = (ix * 73856093) ^ (iy * 19349663) ^ (iz * 83492791) ^ np.int64(seed)
    n &= 0x7FFFFFFFFFFFFFFF
    n = (n ^ (n >> 13)) * 1274126177
    n &= 0x7FFFFFFFFFFFFFFF
    n ^= n >> 16
    return ((n & 0xFFFFFF).astype(np.float64) / 0x7FFFFF) - 1.0


def value_noise(p: np.ndarray, seed: int, scale_mm: float) -> np.ndarray:
    """Smoothstep-interpolated lattice noise at *scale_mm*, in ≈[−1, 1]."""
    q = np.asarray(p, dtype=np.float64) / scale_mm
    i = np.floor(q).astype(np.int64)
    f = q - i
    u = f * f * (3.0 - 2.0 * f)

    def corner(dx, dy, dz):
        return _hash_lattice(i[:, 0] + dx, i[:, 1] + dy, i[:, 2] + dz, seed)

    c000 = corner(0, 0, 0); c100 = corner(1, 0, 0)
    c010 = corner(0, 1, 0); c110 = corner(1, 1, 0)
    c001 = corner(0, 0, 1); c101 = corner(1, 0, 1)
    c011 = corner(0, 1, 1); c111 = corner(1, 1, 1)
    x00 = c000 + (c100 - c000) * u[:, 0]
    x10 = c010 + (c110 - c010) * u[:, 0]
    x01 = c001 + (c101 - c001) * u[:, 0]
    x11 = c011 + (c111 - c011) * u[:, 0]
    y0 = x00 + (x10 - x00) * u[:, 1]
    y1 = x01 + (x11 - x01) * u[:, 1]
    return y0 + (y1 - y0) * u[:, 2]


def fbm(p: np.ndarray, seed: int, scale_mm: float,
        octaves: int = 4, gain: float = 0.5,
        lacunarity: float = 2.0) -> np.ndarray:
    """Fractal sum of value noise, normalized to ≈[−1, 1]."""
    total = np.zeros(len(p))
    amp, sc, norm = 1.0, float(scale_mm), 0.0
    for o in range(octaves):
        total += amp * value_noise(p, seed + 101 * o, sc)
        norm += amp
        amp *= gain
        sc /= lacunarity
    return total / norm
