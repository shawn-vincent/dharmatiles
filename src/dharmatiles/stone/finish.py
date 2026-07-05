"""The aged-surface relief pass — the project's signature stone read.

One recipe, used by scatter stones (aged and fresh paths) and the
fieldstone wall, previously three hand-mirrored copies: displace the
surface along TAUBIN-SMOOTHED normals by broad pillowy undulation plus
granular drybrush tooth, modulated by a patchy envelope (calm fields vs
active shoulders) and damped where the surface is tightly curved.

The fixed parts are deliberate, hard-won choices:

- smoothed normals + curvature damping: normal relief folds wherever
  amplitude exceeds the local concave radius (scar rims, crack roots —
  exactly where zero-gap wall stones touch); |p − p_relaxed| is a cheap
  curvature proxy, and the 0.35 mm scale ignores residual voxel
  stairsteps (~0.05 mm) that would otherwise paint z-contour terraces;
- broad undulation at spectral 1.0, grain at 0.7: octave mixes that
  read mineral — narrowband noise reads as an egg carton;
- the 3-wave patchy envelope: uniform texture is its own monotony; the
  references get their gravitas from CONTRAST between calm and incident.
"""
from __future__ import annotations

import numpy as np
import trimesh

from .surface import relief_field


def aged_relief(body: trimesh.Trimesh, rng: np.random.Generator, *,
                broad: tuple[float, float, float] | None = None,
                grain: tuple[float, float, float] | None = None,
                grain_mix: float = 0.5,
                env: tuple[float, float] | None = None,
                hero: tuple[np.ndarray, np.ndarray, float, float]
                      | None = None) -> np.ndarray:
    """Displaced vertex array for *body* (faces unchanged).

    - ``broad``: (amplitude_mm, wl_lo_mm, wl_hi_mm) pillowy undulation.
    - ``grain``: (amplitude_mm, wl_lo_mm, wl_hi_mm) drybrush tooth.
    - ``grain_mix``: grain contribution factor in the sum.
    - ``env``: (floor, footprint_mm) patchy envelope — quiet zones keep
      ``floor`` × the amplitude, active zones 1.0; None = uniform.
    - ``hero``: (face_normal, face_center, calm, width_mm) protected
      hero-face damping — displacement fades near that plane so one
      calm facet survives aging; None = no protected face.

    RNG draw order is fixed (broad → grain → envelope) so a caller's
    stream is reproducible for any parameter subset.
    """
    p = np.asarray(body.vertices)
    relaxed = trimesh.Trimesh(vertices=p.copy(),
                              faces=np.asarray(body.faces).copy(),
                              process=False)
    trimesh.smoothing.filter_taubin(relaxed, iterations=10)
    vn = np.asarray(relaxed.vertex_normals)
    cd = np.linalg.norm(p - np.asarray(relaxed.vertices), axis=1)
    curv_damp = 1.0 / (1.0 + (cd / 0.35) ** 2)

    disp = np.zeros(len(p))
    if broad is not None:
        amp, lo, hi = broad
        disp += amp * 0.35 * relief_field(p, rng, 6, lo, hi, spectral=1.0)
    if grain is not None:
        amp_g, lo, hi = grain
        disp += (amp_g * grain_mix
                 * 0.7 * relief_field(p, rng, 16, lo, hi, spectral=0.7))
    if env is not None:
        floor, foot = env
        e = np.zeros(len(p))
        for _ in range(3):
            d = rng.normal(size=3)
            d /= np.linalg.norm(d) + 1e-12
            ewl = foot / rng.uniform(0.9, 1.4)
            e += np.cos(2.0 * np.pi / ewl * (p @ d)
                        + rng.uniform(0.0, 2.0 * np.pi))
        disp *= floor + (1.0 - floor) * np.clip(0.5 + 0.75 * e / 3.0,
                                                0.0, 1.0)
    if hero is not None:
        n, c, calm, width = hero
        align = np.clip(vn @ n, 0.0, None) ** 2
        dpl   = np.abs((p - c) @ n)
        disp *= 1.0 - calm * align * np.exp(-(dpl / width) ** 2)
    return p + vn * (disp * curv_damp)[:, None]
