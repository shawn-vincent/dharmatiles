"""Attractor sampling for envelope-driven space colonization trees."""
from __future__ import annotations

import numpy as np

from .envelope import TreeEnvelope


def sample_attractors(env: TreeEnvelope, rng: np.random.Generator) -> np.ndarray:
    """Return biased attractor points inside *env*."""
    volume = env.volume_mm3()
    n = int(np.clip(round(volume / 55.0), 90, 420))
    if env.crown_height <= 1e-8 or env.crown_radius_mm <= 1e-8:
        return np.empty((0, 3), dtype=float)

    points: list[list[float]] = []
    max_attempts = n * 80
    attempts = 0
    while len(points) < n and attempts < max_attempts:
        attempts += 1
        t = float(rng.uniform(0.0, 1.0))
        r_max = float(env.radius_at_t(t))
        if r_max <= 1e-8:
            continue
        rho_frac = float(np.sqrt(rng.uniform(0.0, 1.0)))
        density_weight = 0.65 + 0.35 * rho_frac ** 1.7
        height_weight = 0.85 + 0.30 * _smoothstep(0.25, 0.90, t)
        if rng.random() > min(1.0, density_weight * height_weight):
            continue
        theta = float(rng.uniform(0.0, 2.0 * np.pi))
        r = rho_frac * r_max
        z = env.crown_base_z + t * env.crown_height
        points.append([env.cx + r * np.cos(theta), env.cy + r * np.sin(theta), z])

    pts = np.array(points, dtype=float)
    if len(pts) == 0:
        return pts
    return _add_asymmetry(pts, env, rng, n)


def _add_asymmetry(
    pts: np.ndarray,
    env: TreeEnvelope,
    rng: np.random.Generator,
    target_n: int,
) -> np.ndarray:
    """Create a subtle dominant-limb bias while staying inside the envelope."""
    if len(pts) < 20:
        return pts
    theta0 = float(rng.uniform(0.0, 2.0 * np.pi))
    off = pts[:, :2] - np.array([env.cx, env.cy])
    angles = np.arctan2(off[:, 1], off[:, 0])
    diff = (angles - theta0 + np.pi) % (2.0 * np.pi) - np.pi
    t = (pts[:, 2] - env.crown_base_z) / max(env.crown_height, 1e-8)
    remove_mask = (np.abs(diff) < 0.45) & (t < 0.55)
    candidates = np.where(remove_mask)[0]
    n_remove = min(len(candidates), int(round(len(pts) * rng.uniform(0.05, 0.12))))
    if n_remove <= 0:
        return pts
    keep = np.ones(len(pts), dtype=bool)
    keep[rng.choice(candidates, n_remove, replace=False)] = False
    out = pts[keep]

    # Refill on the opposite upper side.
    while len(out) < target_n:
        t_new = float(rng.uniform(0.45, 0.95))
        r_max = float(env.radius_at_t(t_new))
        if r_max <= 1e-8:
            break
        theta = theta0 + np.pi + float(rng.normal(0.0, 0.35))
        r = np.sqrt(float(rng.uniform(0.35, 1.0))) * r_max
        out = np.vstack([
            out,
            [env.cx + r * np.cos(theta), env.cy + r * np.sin(theta),
             env.crown_base_z + t_new * env.crown_height],
        ])
    return out


def _smoothstep(edge0: float, edge1: float, x: float) -> float:
    if edge1 <= edge0:
        return 1.0 if x >= edge1 else 0.0
    u = np.clip((x - edge0) / (edge1 - edge0), 0.0, 1.0)
    return float(u * u * (3.0 - 2.0 * u))
