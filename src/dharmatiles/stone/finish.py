"""The common stone relief — the project's signature stone read.

ONE surface pass for every rock/block type (floor slabs, cut-stone and
brick blocks, fieldstone pebbles, scatter rocks), design:
docs/design/stone-surface-texture.md.  A calm plateau carved DOWNWARD
into saturating worn recesses (matching the measured official
DungeonBlocks floors: RMS ≈ 0.3 mm, skew ≈ −1, no sub-2 mm power),
plus a gentle dish, an optional patchy calm/incident envelope, and an
optional protected hero face.

Hard-won guardrails that stay regardless of the noise underneath:

- displacement along TAUBIN-SMOOTHED normals with curvature damping:
  normal relief folds wherever amplitude exceeds the local concave
  radius (scar rims, crack roots — exactly where zero-gap wall stones
  touch); |p − p_relaxed| is a cheap curvature proxy, and the 0.35 mm
  scale ignores residual voxel stairsteps that would otherwise paint
  z-contour terraces;
- the noise is isotropic value-noise fBm (stone/noise.py) — plane
  waves are Gaussian and phase-coherent, and on large flat faces they
  read as directional corduroy (the rejected E35 floor texture).
"""
from __future__ import annotations

import numpy as np
import trimesh

from .noise import fbm


def stone_relief(body, rng, *,
                 scale_mm: float = 7.0,
                 carve_mm: float = 0.9,
                 threshold: float = 0.05,
                 band: float = 0.35,
                 dish_mm: float = 0.35,
                 dish_scale_mm: float | None = None,
                 octaves: int = 4,
                 env: tuple[float, float] | None = None,
                 hero: tuple[np.ndarray, np.ndarray, float, float]
                       | None = None,
                 refine: int = 0,
                 base_fade_mm: float | None = None) -> trimesh.Trimesh:
    """The common stone relief (docs/design/stone-surface-texture.md):
    a CALM PLATEAU CARVED DOWNWARD, statistically matched to the
    official DungeonBlocks floors (TS-019: RMS 0.32 mm, p5 −0.80,
    p95 +0.24, skew −0.99 — ours at defaults: 0.35 / −0.82 / +0.20 /
    −0.84).

    - carve: −carve_mm · smoothstep((fbm − threshold)/band) — the
      recesses SATURATE at a worn floor (a bimodal plateau/floor
      surface with rims between), the read of eroded stone.  Sparse
      unsaturated pits came out too spiky (skew −2.3).
    - dish: gentle two-sided fbm at footprint scale so big faces
      aren't dead planes.
    - displaced along Taubin-smoothed normals with curvature damping
      (same guardrail as the aged pass).

    ``refine`` subdivides the mesh that many times before displacing
    (rims turn into angular polylines when mesh edges are coarse
    against the noise scale); ``base_fade_mm`` fades the displacement
    to zero approaching the body's local z-floor (carved patches at a
    buried base leave standing ribs / expose what's behind).  Both
    used to be reimplemented at every call site.

    Returns the displaced MESH.
    """
    for _ in range(max(refine, 0)):
        body = body.subdivide()
    p = np.asarray(body.vertices)
    relaxed = trimesh.Trimesh(vertices=p.copy(),
                              faces=np.asarray(body.faces).copy(),
                              process=False)
    trimesh.smoothing.filter_taubin(relaxed, iterations=10)
    vn = np.asarray(relaxed.vertex_normals)
    cd = np.linalg.norm(p - np.asarray(relaxed.vertices), axis=1)
    curv_damp = 1.0 / (1.0 + (cd / 0.35) ** 2)

    seed = int(rng.integers(0, 2**31))
    # Random domain rotation: the value-noise lattice is axis-aligned,
    # and so are block/slab faces — un-rotated, the carve rims trace
    # lattice-plane level sets and read as straight diagonal scratches
    # (worst on small faces spanning only a few cells).
    q = rng.normal(size=4)
    q /= np.linalg.norm(q) + 1e-12
    w, x, y, z = q
    R = np.array([
        [1 - 2*(y*y + z*z), 2*(x*y - w*z),     2*(x*z + w*y)],
        [2*(x*y + w*z),     1 - 2*(x*x + z*z), 2*(y*z - w*x)],
        [2*(x*z - w*y),     2*(y*z + w*x),     1 - 2*(x*x + y*y)]])
    pr = p @ R.T
    n = fbm(pr, seed, scale_mm, octaves=octaves)
    t = np.clip((n - threshold) / band, 0.0, 1.0)
    carve = -carve_mm * t * t * (3.0 - 2.0 * t)
    if dish_mm > 0.0:
        foot = float(np.ptp(p, axis=0).max())
        dsc = dish_scale_mm if dish_scale_mm is not None else foot / 1.6
        carve = carve + dish_mm * fbm(pr, seed + 7919, dsc, octaves=2)
    if env is not None:
        # Patchy calm/incident contrast (E11 rocks lesson), now on
        # value noise like everything else.
        floor, foot_e = env
        e = fbm(pr, seed + 15013, foot_e / 1.1, octaves=2)
        carve = carve * (floor + (1.0 - floor)
                         * np.clip(0.5 + 1.1 * e, 0.0, 1.0))
    if hero is not None:
        nrm, c, calm, width = hero
        align = np.clip(vn @ nrm, 0.0, None) ** 2
        dpl   = np.abs((p - c) @ nrm)
        carve = carve * (1.0 - calm * align * np.exp(-(dpl / width) ** 2))
    if base_fade_mm is not None:
        zmin = float(p[:, 2].min())
        carve = carve * np.clip((p[:, 2] - zmin) / base_fade_mm, 0.0, 1.0)
    return trimesh.Trimesh(vertices=p + vn * (carve * curv_damp)[:, None],
                           faces=body.faces, process=False)
