"""Crown envelope for the clean-room tree generator."""
from __future__ import annotations

from dataclasses import dataclass
from functools import cached_property

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
    crown_base_radius_mm: float
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
        raw = self._raw_radius(clipped)
        raw = np.where((clipped <= 0.0) | (clipped >= 1.0), 0.0, raw)
        raw = np.where(clipped <= 0.0, self._raw_base_radius, raw)
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

    def outward_normal_at(self, pts: np.ndarray) -> np.ndarray:
        """Compute outward unit normals on the crown surface at *pts*.

        For the surface of revolution r(z), the outward normal at a point
        (x, y, z) on the surface is:

            n = (cos θ, sin θ, −dr/dz) / ‖…‖

        where θ = atan2(y − cy, x − cx) and dr/dz is evaluated numerically.

        Points at or near the symmetry axis fall back to (0, 0, 1).
        """
        pts_arr = np.atleast_2d(np.asarray(pts, dtype=float))
        dx = pts_arr[:, 0] - self.cx
        dy = pts_arr[:, 1] - self.cy
        z  = pts_arr[:, 2]

        # Central-difference dr/dz; step = 0.1 % of crown height (min 0.01 mm).
        dz_step = max(self.crown_height * 0.001, 0.01)
        dr_dz = (
            np.asarray(self.radius_at_z(z + dz_step), dtype=float)
            - np.asarray(self.radius_at_z(z - dz_step), dtype=float)
        ) / (2.0 * dz_step)

        rxy  = np.hypot(dx, dy)
        safe = rxy > 1e-9
        cos_t = np.where(safe, dx / np.where(safe, rxy, 1.0), 0.0)
        sin_t = np.where(safe, dy / np.where(safe, rxy, 1.0), 0.0)

        norms = np.sqrt(cos_t ** 2 + sin_t ** 2 + dr_dz ** 2)
        norms = np.maximum(norms, 1e-9)
        return np.column_stack([
            cos_t / norms,
            sin_t / norms,
            -dr_dz / norms,
        ])

    def volume_mm3(self) -> float:
        if self.crown_height <= 1e-8:
            return 0.0
        ts = np.linspace(0.0, 1.0, 129)
        rs = self.radius_at_t(ts)
        areas = np.pi * rs * rs
        return float(np.trapezoid(areas, ts) * self.crown_height)

    @cached_property
    def _raw_base_radius(self) -> float:
        if self.crown_radius_mm <= 1e-12:
            return 0.0
        target = float(np.clip(self.crown_base_radius_mm / self.crown_radius_mm, 0.0, 1.0))
        if target <= 1e-12:
            return 0.0
        lo = 0.0
        hi = min(1.0, target * 1.25 + 0.25)
        for _ in range(32):
            mid = 0.5 * (lo + hi)
            peak = self._raw_peak_for_base(mid)
            ratio = mid / peak if peak > 1e-12 else 0.0
            if ratio < target:
                lo = mid
            else:
                hi = mid
        return hi

    def _raw_radius(self, t) -> np.ndarray:
        bottom = self._raw_base_radius + (1.0 - self._raw_base_radius) * _end_profile(
            t,
            self.bottom_pointiness,
            self.bottom_curve,
        )
        top = _end_profile(1.0 - t, self.top_pointiness, self.top_curve)
        return _smooth_min(bottom, top, 0.08)

    def _raw_peak(self) -> float:
        return self._raw_peak_for_base(self._raw_base_radius)

    def _raw_peak_for_base(self, base_radius: float) -> float:
        ts = np.linspace(0.0, 1.0, 257)
        bottom = base_radius + (1.0 - base_radius) * _end_profile(
            ts,
            self.bottom_pointiness,
            self.bottom_curve,
        )
        top = _end_profile(1.0 - ts, self.top_pointiness, self.top_curve)
        vals = _smooth_min(bottom, top, 0.08)
        vals[(ts <= 0.0) | (ts >= 1.0)] = 0.0
        vals[ts <= 0.0] = base_radius
        return float(vals.max())


def _end_profile(u, pointiness: float, curve: float) -> np.ndarray:
    u_arr = np.clip(np.asarray(u, dtype=float), 0.0, 1.0)
    p = float(np.clip(pointiness, 0.0, 1.0))
    c = max(0.01, float(curve))
    linear = u_arr ** c
    round_arc = np.sqrt(np.clip(1.0 - (1.0 - u_arr) ** 2, 0.0, 1.0))
    return (1.0 - p) * round_arc + p * linear


def _smooth_min(a, b, width: float) -> np.ndarray:
    a_arr = np.asarray(a, dtype=float)
    b_arr = np.asarray(b, dtype=float)
    k = max(1e-9, float(width))
    h = np.maximum(k - np.abs(a_arr - b_arr), 0.0) / k
    return np.minimum(a_arr, b_arr) - h * h * k * 0.25
