"""
Da Vinci pipe-model radius assignment for the unified tree skeleton.

``r_parent^n = Σ r_child^n`` — cross-section is conserved at every bifurcation
when ``n=2`` and can be tuned for heavier or more delicate branching.
Leaf nodes start at *branch_r_tip_mm*; radii accumulate bottom-up.

The root node's radius is then overridden by *r_root_mm* (sampled from
``BarkConfig.r_base_mm`` via ``ScaTreeConfig.bark``), decoupling trunk girth from branching density.

Public API
----------
``assign_radii(parents, r_tip_mm, r_root_mm, exp=2.0)`` → ndarray of shape (N,)
"""
from __future__ import annotations

import numpy as np


def assign_radii(
    parents:    np.ndarray,
    r_tip_mm:   float,
    r_root_mm:  float,
    exp:        float = 2.0,
    include_internal_self: bool = True,
) -> np.ndarray:
    """Assign radii bottom-up; override root to *r_root_mm*.

    Parameters
    ----------
    parents   : (N,) int — parent indices; -1 for the root.
    r_tip_mm  : starting radius at leaf nodes (no children).
    r_root_mm : radius to assign to the root node (node 0) after the
                pipe-model pass, so trunk girth is user-controlled.
    exp       : pipe-model exponent.  ``2.0`` preserves the classic
                area-conserving rule used by ScaTree.
    include_internal_self:
                When true, every internal node contributes one tip-radius
                unit to its parent, preserving the original ScaTree taper.
                When false, internal radii are determined only by children,
                giving constant-radius one-child runs for ConstTree.

    Returns
    -------
    radii : (N,) float
    """
    if exp <= 0.0:
        raise ValueError("pipe-model exponent must be positive")

    N = len(parents)
    if include_internal_self:
        radii = np.full(N, r_tip_mm, dtype=float)
    else:
        child_count = np.zeros(N, dtype=int)
        for i in range(1, N):
            p = int(parents[i])
            if p >= 0:
                child_count[p] += 1
        radii = np.where(child_count == 0, r_tip_mm, 0.0).astype(float)

    # Bottom-up accumulation: leaves → root.
    # Nodes are appended in BFS order, so iterating in reverse gives leaves first.
    for i in range(N - 1, 0, -1):
        p = int(parents[i])
        if p >= 0:
            radii[p] = (radii[p] ** exp + radii[i] ** exp) ** (1.0 / exp)

    # Root override: decouple trunk radius from branching density
    if N > 0:
        radii[0] = r_root_mm

    return radii
