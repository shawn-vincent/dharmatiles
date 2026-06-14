"""
Da Vinci pipe-model radius assignment for the unified tree skeleton.

``r_parent² = Σ r_child²`` — cross-section is conserved at every bifurcation.
Leaf nodes start at *branch_r_tip_mm*; radii accumulate bottom-up.

The root node's radius is then overridden by *r_root_mm* (sampled from
``BarkConfig.r_base_mm`` via ``ScaTreeConfig.bark``), decoupling trunk girth from branching density.

Public API
----------
``assign_radii(parents, r_tip_mm, r_root_mm)`` → ndarray of shape (N,)
"""
from __future__ import annotations

import numpy as np


def assign_radii(
    parents:    np.ndarray,
    r_tip_mm:   float,
    r_root_mm:  float,
) -> np.ndarray:
    """Assign radii bottom-up; override root to *r_root_mm*.

    Parameters
    ----------
    parents   : (N,) int — parent indices; -1 for the root.
    r_tip_mm  : starting radius at leaf nodes (no children).
    r_root_mm : radius to assign to the root node (node 0) after the
                pipe-model pass, so trunk girth is user-controlled.

    Returns
    -------
    radii : (N,) float
    """
    N      = len(parents)
    radii  = np.full(N, r_tip_mm, dtype=float)

    # Bottom-up accumulation: leaves → root.
    # Nodes are appended in BFS order, so iterating in reverse gives leaves first.
    for i in range(N - 1, 0, -1):
        p = int(parents[i])
        if p >= 0:
            radii[p] = np.sqrt(radii[p] ** 2 + radii[i] ** 2)

    # Root override: decouple trunk radius from branching density
    if N > 0:
        radii[0] = r_root_mm

    return radii
