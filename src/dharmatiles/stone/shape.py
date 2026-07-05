"""Shape primitives: the body half of stone-making."""
from __future__ import annotations

import numpy as np
import trimesh

# Rubble-stone character (hearting fill, ruin scatter): a cheap ROUGH
# hull — crisp facets, strong radial variance, no remesh/relief/
# roundover.  Smooth rounded rubble read as MORTAR through the wall
# cracks (fieldstone E26, Shawn: "smooth rubbles no good.  should be
# lots of little rocks --- very rough"); sharp jittered shards read as
# packed stone chips.
_RUBBLE_DIR_JITTER = 0.30
_RUBBLE_LUMP       = (0.70, 1.05)

_ROUND_JITTER = (0.55, 1.6)   # roundover randomization range —
                              # uniform fillets read CNC, not geology;
                              # the floor keeps every edge VISIBLY
                              # rounded on weathered stones
_ROUND_EDGE_STEP_MM = 1.2     # ball spacing along edges: the radius is
                              # re-rolled at each sample, so the fillet
                              # wobbles ALONG an edge (per-corner-only
                              # radii read as rounded dice)


def fibonacci_sphere(n: int) -> np.ndarray:
    """*n* unit directions in a golden-angle spiral — evenly spread, so
    hull facet size is bounded from below by construction (R3)."""
    i     = np.arange(n) + 0.5
    phi   = np.arccos(1.0 - 2.0 * i / n)           # polar
    theta = np.pi * (1.0 + np.sqrt(5.0)) * i        # golden-angle azimuth
    return np.stack([np.sin(phi) * np.cos(theta),
                     np.sin(phi) * np.sin(theta),
                     np.cos(phi)], axis=1)


def rubble_stone(lx: float, ly: float, lz: float,
                 rng: np.random.Generator) -> trimesh.Trimesh:
    """Cheap ROUGH hull stone: strongly jittered fibonacci directions,
    an ellipsoid↔box radial blend (per-stone blockiness), heavy lump
    variance, crisp un-rounded facets — a sharp little stone chip that
    costs nothing to build.  Used by the fieldstone rubble hearting;
    the shape primitive for any future ruin scatter / core fill."""
    M = int(rng.integers(10, 15))
    d = fibonacci_sphere(M)
    d += rng.normal(0.0, _RUBBLE_DIR_JITTER, d.shape)
    d[:, 1] *= rng.uniform(0.5, 0.8)
    d /= np.linalg.norm(d, axis=1, keepdims=True) + 1e-12
    half = np.array([lx / 2.0, ly / 2.0, lz / 2.0])
    blockiness = rng.uniform(0.15, 0.50)
    r_ell = 1.0 / np.sqrt(((d / half) ** 2).sum(axis=1))
    r_box = 1.0 / (np.abs(d) / half).max(axis=1)
    r = ((1.0 - blockiness) * r_ell + blockiness * r_box)
    r *= rng.uniform(*_RUBBLE_LUMP, M)
    p = d * r[:, None] + half
    hull = trimesh.convex.convex_hull(p)
    return trimesh.Trimesh(vertices=np.asarray(hull.vertices),
                           faces=np.asarray(hull.faces), process=False)


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
