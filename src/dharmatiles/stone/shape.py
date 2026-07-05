"""Shape primitives: the body half of stone-making."""
from __future__ import annotations

import numpy as np
import trimesh

_ROUND_JITTER = (0.55, 1.6)   # roundover randomization range —
                              # uniform fillets read CNC, not geology;
                              # the floor keeps every edge VISIBLY
                              # rounded on weathered stones
_ROUND_EDGE_STEP_MM = 1.2     # ball spacing along edges: the radius is
                              # re-rolled at each sample, so the fillet
                              # wobbles ALONG an edge (per-corner-only
                              # radii read as rounded dice)


def round_edges(verts: np.ndarray, faces: np.ndarray,
                roundover_mm: float, rng: np.random.Generator,
                ) -> tuple[np.ndarray, np.ndarray]:
    """Weathering fillet: a rolling ball whose radius is re-rolled at every
    corner AND at ~1 mm intervals along every edge, eroded inward and
    hulled.  The silhouette stays; the fillet radius wobbles along each
    edge — one edge can stay sharp at one end and round over in the middle
    (constant-radius fillets read as rounded dice, per Shawn)."""
    m   = trimesh.Trimesh(vertices=verts, faces=faces, process=False)
    cap = 0.3 * float(m.extents.min())

    cores, rhos = [], []
    # Corner balls.
    vn = np.asarray(m.vertex_normals)
    rv = np.minimum(roundover_mm * rng.uniform(*_ROUND_JITTER, len(verts)),
                    cap)
    cores.append(verts - vn * rv[:, None])
    rhos.append(rv)
    # Edge balls, radius independently re-rolled per sample.
    fn = m.face_normals
    for (f1, f2), (a, b) in zip(m.face_adjacency, m.face_adjacency_edges):
        va, vb = verts[a], verts[b]
        L = float(np.linalg.norm(vb - va))
        k = int(L / _ROUND_EDGE_STEP_MM)
        if k < 1:
            continue
        nd = fn[f1] + fn[f2]
        nd = nd / (np.linalg.norm(nd) + 1e-12)
        ts = (np.arange(1, k + 1) + rng.uniform(-0.3, 0.3, k)) / (k + 1)
        ts = np.clip(ts, 0.08, 0.92)
        p  = va[None, :] + ts[:, None] * (vb - va)[None, :]
        r  = np.minimum(roundover_mm * rng.uniform(*_ROUND_JITTER, k), cap)
        cores.append(p - nd[None, :] * r[:, None])
        rhos.append(r)

    core = np.vstack(cores)
    rho  = np.concatenate(rhos)
    # subdivisions=2 (162 dirs): coarser balls turn fillets into single
    # chamfer strips — reads as a bevelled box, not weathering.
    dirs = trimesh.creation.icosphere(subdivisions=2, radius=1.0).vertices
    pts  = (core[:, None, :] + rho[:, None, None] * dirs[None, :, :])
    h = trimesh.convex.convex_hull(pts.reshape(-1, 3))
    return np.asarray(h.vertices), np.asarray(h.faces)
