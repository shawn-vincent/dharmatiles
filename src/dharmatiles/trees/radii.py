"""Printable pipe-model radius assignment for tree skeletons."""
from __future__ import annotations

import numpy as np


def assign_radii(nodes: np.ndarray, parents: np.ndarray, terrain_z: float, height_mm: float) -> np.ndarray:
    """Assign printable branch radii to every skeleton node."""
    n = len(nodes)
    children: list[list[int]] = [[] for _ in range(n)]
    for i, p in enumerate(parents):
        if p >= 0:
            children[int(p)].append(i)

    radii = np.zeros(n, dtype=float)
    order = sorted(range(n), key=lambda i: nodes[i, 2], reverse=True)
    for i in order:
        min_r = _structural_min_radius(nodes[i, 2], terrain_z, height_mm)
        if not children[i]:
            radii[i] = max(0.45, min_r)
        else:
            child_rs = np.array([radii[c] for c in children[i]], dtype=float)
            pipe = float(np.sum(child_rs ** 2.25) ** (1.0 / 2.25))
            radii[i] = max(min_r, pipe)
    radii[0] = float(np.clip(radii[0], 1.25, 0.14 * max(height_mm, 1.0)))
    return radii


def _structural_min_radius(z: float, terrain_z: float, height_mm: float) -> float:
    f = (z - terrain_z) / max(height_mm, 1e-8)
    u = np.clip((f - 0.35) / (0.95 - 0.35), 0.0, 1.0)
    s = u * u * (3.0 - 2.0 * u)
    return float((1.0 - s) * 0.75 + s * 0.42)
