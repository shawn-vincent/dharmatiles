#!/usr/bin/env python3
"""Detect geometric artifacts in stone meshes, distinct from intended detail.

Two artifact signatures, both invisible to watertight/genus checks:

1. KNIFE EDGES — adjacent faces folding back (normal deviation above a
   threshold) across a CONVEX edge.  Intended convex edges (facet arrises,
   groove rims, scar rims) deviate < ~80 deg; a pleat/fold deviates > 100.
   Sharp CONCAVE edges are intentional (crack/seam apexes) and ignored.
2. LAYERED SKIN — vertex pairs closer than a tolerance that are NOT
   topological neighbours (within 2 rings).  A fold is two layers of
   surface nearly touching; no intended feature produces that.

Usage:
    python src/extras/stone_audit.py stl/test/hero-letipea-db.stl [...]
Reports counts and cluster locations (mm) for each rock-like component.
"""
import sys

import numpy as np
import trimesh
from scipy.spatial import cKDTree
from scipy.sparse import csr_matrix

KNIFE_DEG   = 100.0   # convex normal deviation above this = fold
NEAR_MM     = 0.08    # non-neighbour vertices closer than this = layered skin
RING_EXCL   = 2       # topological rings considered "neighbours"


def audit(mesh: trimesh.Trimesh) -> dict:
    out = {}

    ang  = mesh.face_adjacency_angles
    conv = mesh.face_adjacency_convex
    knife = (ang > np.radians(KNIFE_DEG)) & conv
    out['knife_edges'] = int(knife.sum())
    if knife.any():
        e = mesh.face_adjacency_edges[knife]
        out['knife_locs'] = np.asarray(mesh.vertices)[e].mean(axis=1)

    v = np.asarray(mesh.vertices)
    n = len(v)
    ii, jj = mesh.edges_unique.T
    adj = csr_matrix((np.ones(len(ii)), (ii, jj)), shape=(n, n))
    adj = adj + adj.T
    ring = adj
    for _ in range(RING_EXCL - 1):
        ring = ring @ adj
    ring = ring.tocsr()

    pairs = cKDTree(v).query_pairs(NEAR_MM, output_type='ndarray')
    if len(pairs):
        near = ~np.asarray(
            [ring[a, b] != 0 for a, b in pairs], dtype=bool)
        pairs = pairs[near]
    out['layered_pairs'] = int(len(pairs))
    if len(pairs):
        out['layered_locs'] = v[pairs[:, 0]]
    return out


def _clusters(locs: np.ndarray, r: float = 1.5) -> list:
    """Greedy cluster of artifact locations for compact reporting."""
    locs = list(map(np.asarray, locs))
    out = []
    while locs:
        seed = locs.pop()
        members = [seed]
        locs2 = []
        for q in locs:
            (members if np.linalg.norm(q - seed) < r else locs2).append(q)
        locs = locs2
        c = np.mean(members, axis=0)
        out.append((len(members), c))
    return sorted(out, reverse=True, key=lambda t: t[0])


def main():
    for path in sys.argv[1:]:
        m = trimesh.load(path)
        print(f'== {path}')
        for c in m.split(only_watertight=False):
            vv = np.asarray(c.vertices)
            if len(c.faces) < 100 or vv[:, 2].min() < -1.0:
                continue
            if (vv.max(axis=0) - vv.min(axis=0)).max() > 33.0:
                continue        # terrain sheet
            r = audit(c)
            flag = ('ARTIFACTS' if r['knife_edges'] or r['layered_pairs']
                    else 'clean')
            print(f'  stone at ({vv[:,0].mean():5.1f},{vv[:,1].mean():5.1f})'
                  f'  faces={len(c.faces):6d}  knife={r["knife_edges"]:4d}'
                  f'  layered={r["layered_pairs"]:4d}  {flag}')
            for key in ('knife_locs', 'layered_locs'):
                if key in r:
                    for cnt, ctr in _clusters(list(r[key]))[:4]:
                        print(f'      {key[:-5]} x{cnt:<4d} near '
                              f'({ctr[0]:.1f},{ctr[1]:.1f},{ctr[2]:.1f})')


if __name__ == '__main__':
    main()
