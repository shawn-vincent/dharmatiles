"""Shared utility functions for the trees subpackage.

These helpers are tiny but appear in multiple trees/ modules.  Centralising
them here avoids copy-paste drift and the circular-import risk of importing
from the heavier mesh / skeleton modules.
"""
from __future__ import annotations

import numpy as np

# Canonical "world up" vector — used for frame construction.
_WUP_VEC = np.array([0.0, 0.0, 1.0])


def _safe_norm(v: np.ndarray) -> np.ndarray:
    """Return *v* normalised; return *v* unchanged if it is near-zero."""
    n = float(np.linalg.norm(v))
    return v / n if n > 1e-12 else v


def _hash01(*parts: object) -> float:
    """Deterministic float in [0, 1) from arbitrary hashable parts.

    Uses FNV-1a over the UTF-8 bytes of each part, followed by a murmur3
    fmix64 avalanche step.  Without the finalizer, FNV barely diffuses the
    trailing bytes, so varying only the last argument (e.g. a column index)
    leaves the high bits almost unchanged — collapsing per-leaf jitter into a
    visible grid.  The finalizer spreads every input bit across all 64 output
    bits, giving a centred, full-range uniform.
    """
    h = 1469598103934665603
    for part in parts:
        for byte in str(part).encode("utf-8"):
            h ^= byte
            h  = (h * 1099511628211) & 0xFFFFFFFFFFFFFFFF
        h ^= 0xFF
        h  = (h * 1099511628211) & 0xFFFFFFFFFFFFFFFF
    # murmur3 fmix64 finalizer
    h ^= h >> 33
    h  = (h * 0xFF51AFD7ED558CCD) & 0xFFFFFFFFFFFFFFFF
    h ^= h >> 33
    h  = (h * 0xC4CEB9FE1A85EC53) & 0xFFFFFFFFFFFFFFFF
    h ^= h >> 33
    return h / float(2 ** 64)
