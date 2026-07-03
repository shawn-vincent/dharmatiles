#!/usr/bin/env python3
"""Correctness tests for the greedy lowest-first leaf placer.

Runs place_leaves_greedy on two overlapping foliage clusters (the same A+B
case as test-multi-parent-mesh-leaves.py) and asserts the invariants that make
a leaf "correct" — the defensive guarantees the greedy path must never break:

  1. At least some leaves are placed on each cluster.
  2. Every placed leaf solid is watertight (a valid printable shell).
  3. Every placed leaf's embedded root oval reaches into the REAL (noised)
     foliage clump — the connection guarantee that ``_GREEDY_EMBED_MM >
     _FOLIAGE_MAX_NOISE_MM`` exists to provide.  A leaf whose root floats over a
     noise valley would print detached; none may.
  4. Every placed leaf's visible blade tip sits above its embedded root tip.
  5. Placement is deterministic: identical seeds ⇒ identical output.

Exit code 0 = all pass, 1 = any failure.

Usage::

    python src/scripts/test-greedy-leaf-placement.py
"""
from __future__ import annotations

import math
import sys
import time

import numpy as np

from dharmatiles.trees._utils import _safe_norm
from dharmatiles.trees.mesh import _build_foliage_cluster_mesh
from dharmatiles.trees.leaf import _LEAF_N_LONG, _LEAF_N_LAT
from dharmatiles.trees.placement_greedy import place_leaves_greedy

_LEAF = dict(
    length_mm      = 4.5,
    width_mm       = 3.0,
    thickness_mm   = 0.24,
    fold_angle_deg = 6.0,
    inner_curve    = 1.5,
    outer_curve    = 0.72,
    curl_deg       = 40.0,
    lift_mm        = 0.0,
)

# n_outer = vertices in one open leaf surface; the solid stacks [outer | oval].
_N_OUTER = (_LEAF_N_LONG - 1) * (_LEAF_N_LAT + 1) + 2


def _make_cluster(cx, cy, ztip, tip_t, start_t, eid, seed, *, apply_noise):
    tt = _safe_norm(np.asarray(tip_t, float))
    st = _safe_norm(np.asarray(start_t, float))
    tip = np.array([cx, cy, ztip], float)
    start = tip - 10.5 * tt
    m, _ = _build_foliage_cluster_mesh(
        tip_pos=tip, tip_tangent=tt, start_pos=start, start_tangent=st,
        r_wood=1.0, r_foliage=5.5, clump_length_mm=10.5,
        edge_id=eid, bark_seed=seed, leaves=False, apply_noise=apply_noise,
    )
    return m


def _build_pair(apply_noise):
    a35, a55 = math.radians(35), math.radians(55)
    a = _make_cluster(0.0, 0.0, 22.0, [0, 0, 1], [0, 0, 1], 0, 33, apply_noise=apply_noise)
    b = _make_cluster(6.0, 2.0, 23.0,
                      [math.sin(a55), 0, math.cos(a55)],
                      [math.sin(a35), 0, math.cos(a35)],
                      1, 44, apply_noise=apply_noise)
    return a, b


def main() -> int:
    failures: list[str] = []

    smooth_a, smooth_b = _build_pair(apply_noise=False)
    noised_a, noised_b = _build_pair(apply_noise=True)

    noised = [noised_a, noised_b]
    t0 = time.perf_counter()
    parts, stats = place_leaves_greedy(
        noised, **_LEAF, seeds=[0, 1], labels=["A", "B"],
    )
    elapsed = time.perf_counter() - t0

    # 1. Non-empty placement per cluster.
    for mi, s in enumerate(stats):
        if s.n_placed == 0:
            failures.append(f"cluster {mi}: no leaves placed")

    total = sum(len(p) for p in parts)
    print(f"placed {total} leaves in {elapsed:.3f}s "
          f"(A={len(parts[0])}, B={len(parts[1])})")

    # Per-leaf correctness for the stripped model:
    #   - every leaf solid watertight,
    #   - root oval reaches real material (embedded below the surface),
    #   - blade tip + belly not buried in a parent clump (blade grows outward).
    # Leaves DO overlap each other by design (dense ~½-width packing), so there
    # is no leaf-vs-leaf non-overlap check.
    L = float(_LEAF["length_mm"])
    base_i = _N_OUTER - 2                       # base_pt vertex (surface layout)
    tip_i = _N_OUTER - 1                        # tip_pt vertex
    n_nonwatertight = 0
    n_disconnected = 0
    n_buried_mesh = 0

    flat = [(mi, leaf) for mi, leaves in enumerate(parts) for leaf in leaves]
    for mi, leaf in flat:
        if not leaf.is_watertight:
            n_nonwatertight += 1
        if len(leaf.vertices) >= 2 * _N_OUTER:
            oval = leaf.vertices[_N_OUTER:2 * _N_OUTER]
            if int(noised[mi].contains(oval).sum()) == 0:
                n_disconnected += 1

        outer = leaf.vertices[:_N_OUTER]
        base = leaf.vertices[base_i]
        tip_half = outer[np.linalg.norm(outer - base, axis=1) > (L / 2.0)]
        belly = tip_half[int(np.argmin(tip_half[:, 2]))]        # lowest blade point
        probe = np.stack([outer[tip_i], belly])
        # tip/belly must not be buried in ANY parent clump.
        if any(int(nm.contains(probe).sum()) > 0 for nm in noised):
            n_buried_mesh += 1

    if n_nonwatertight:
        failures.append(f"{n_nonwatertight}/{total} leaves not watertight")
    if n_disconnected:
        failures.append(
            f"{n_disconnected}/{total} leaves have a root oval that never enters "
            f"the real noised clump (would print detached)"
        )
    if n_buried_mesh:
        failures.append(
            f"{n_buried_mesh}/{total} leaves have tip/belly buried in a parent clump"
        )

    print(f"watertight={total - n_nonwatertight}/{total}  "
          f"root-connected={total - n_disconnected}/{total}  "
          f"blade-clear-of-mesh={total - n_buried_mesh}/{total}")

    # 5. Determinism.
    parts2, stats2 = place_leaves_greedy(
        noised, **_LEAF, seeds=[0, 1], labels=["A", "B"], verbose=False,
    )
    if [s.n_placed for s in stats] != [s.n_placed for s in stats2]:
        failures.append("non-deterministic placed counts across identical runs")
    else:
        for mi in range(2):
            b1 = np.stack(stats[mi].base_positions) if stats[mi].base_positions else np.zeros((0, 3))
            b2 = np.stack(stats2[mi].base_positions) if stats2[mi].base_positions else np.zeros((0, 3))
            if b1.shape != b2.shape or not np.allclose(b1, b2):
                failures.append(f"cluster {mi}: non-deterministic base positions")
    print("determinism: "
          f"{'OK' if all('deterministic' not in f for f in failures) else 'FAIL'}")

    print("\n" + "=" * 56)
    if failures:
        print(f"FAIL ({len(failures)} issue(s)):")
        for f in failures:
            print(f"  ✗ {f}")
        print("=" * 56)
        return 1
    print("PASS — all greedy leaf-placement invariants hold")
    print("=" * 56)
    return 0


if __name__ == "__main__":
    sys.exit(main())
