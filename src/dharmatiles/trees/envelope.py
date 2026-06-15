"""Crown envelope for the clean-room tree generator."""
from __future__ import annotations

from dataclasses import dataclass

import numpy as np


@dataclass(frozen=True)
class TreeEnvelope:
    """Axisymmetric crown envelope derived from the public tree parameters."""

    cx: float
    cy: float
    terrain_z: float
    height_mm: float
    trunk_height_mm: float
    crown_radius_mm: float
    top_pointiness: float
    top_curve: float
    bottom_pointiness: float
    bottom_curve: float

    @property
    def crown_base_z(self) -> float:
        return self.terrain_z + self.trunk_height_mm

    @property
    def crown_top_z(self) -> float:
        return self.terrain_z + self.height_mm

    @property
    def crown_height(self) -> float:
        return max(0.0, self.height_mm - self.trunk_height_mm)

    def radius_at_t(self, t) -> np.ndarray:
        """Return envelope radius for normalised crown height *t*."""
        t_arr = np.asarray(t, dtype=float)
        clipped = np.clip(t_arr, 0.0, 1.0)
        raw = np.minimum(
            _end_profile(clipped, self.bottom_pointiness, self.bottom_curve),
            _end_profile(1.0 - clipped, self.top_pointiness, self.top_curve),
        )
        raw = np.where((clipped <= 0.0) | (clipped >= 1.0), 0.0, raw)
        peak = self._raw_peak()
        if peak <= 1e-12:
            return np.zeros_like(raw, dtype=float)
        return self.crown_radius_mm * raw / peak

    def radius_at_z(self, z) -> np.ndarray:
        if self.crown_height <= 1e-8:
            return np.zeros_like(np.asarray(z, dtype=float), dtype=float)
        t = (np.asarray(z, dtype=float) - self.crown_base_z) / self.crown_height
        return self.radius_at_t(t)

    def contains(self, p: np.ndarray, *, margin: float = 0.0) -> bool:
        if self.crown_height <= 1e-8:
            return False
        z = float(p[2])
        if z < self.crown_base_z - margin or z > self.crown_top_z + margin:
            return False
        r = float(np.linalg.norm(p[:2] - np.array([self.cx, self.cy])))
        return r <= float(self.radius_at_z(z)) + margin

    def project_inside(self, p: np.ndarray, *, scale: float = 0.98) -> np.ndarray:
        """Softly clamp *p* to the crown surface at the same normalised height."""
        if self.crown_height <= 1e-8:
            return p.copy()
        q = p.copy()
        q[2] = float(np.clip(q[2], self.crown_base_z, self.crown_top_z))
        max_r = float(self.radius_at_z(q[2])) * scale
        off = q[:2] - np.array([self.cx, self.cy])
        r = float(np.linalg.norm(off))
        if r > max_r and r > 1e-9:
            q[:2] = np.array([self.cx, self.cy]) + off / r * max_r
        return q

    def volume_mm3(self) -> float:
        if self.crown_height <= 1e-8:
            return 0.0
        ts = np.linspace(0.0, 1.0, 129)
        rs = self.radius_at_t(ts)
        areas = np.pi * rs * rs
        return float(np.trapezoid(areas, ts) * self.crown_height)

    def _raw_peak(self) -> float:
        ts = np.linspace(0.0, 1.0, 257)
        vals = np.minimum(
            _end_profile(ts, self.bottom_pointiness, self.bottom_curve),
            _end_profile(1.0 - ts, self.top_pointiness, self.top_curve),
        )
        vals[(ts <= 0.0) | (ts >= 1.0)] = 0.0
        return float(vals.max())


def _end_profile(u, pointiness: float, curve: float) -> np.ndarray:
    u_arr = np.clip(np.asarray(u, dtype=float), 0.0, 1.0)
    p = float(np.clip(pointiness, 0.0, 1.0))
    c = max(0.01, float(curve))
    linear = u_arr ** c
    round_arc = np.sqrt(np.clip(1.0 - (1.0 - u_arr) ** 2, 0.0, 1.0))
    return (1.0 - p) * round_arc + p * linear
