"""Surface fields and remeshing: the finish half of stone-making."""
from __future__ import annotations

import numpy as np
import trimesh


def relief_field(p: np.ndarray, rng: np.random.Generator, n_waves: int,
                 wl_lo: float, wl_hi: float,
                 spectral: float = 0.7) -> np.ndarray:
    """Broadband random relief: plane waves with LOG-UNIFORM wavelengths
    and amplitude ∝ wl^spectral, normalized to unit RMS.

    Narrowband noise (every wave at one scale) reads as a regular field
    of same-sized bumps — an egg carton, not stone (Shawn's find on the
    doane grain).  Mixing octaves like a 1/f spectrum reads mineral."""
    f, tot = np.zeros(len(p)), 0.0
    for _ in range(n_waves):
        d = rng.normal(size=3)
        d /= np.linalg.norm(d) + 1e-12
        wl = float(np.exp(rng.uniform(np.log(wl_lo), np.log(wl_hi))))
        a  = (wl / wl_hi) ** spectral
        f += a * np.cos(2.0 * np.pi / wl * (p @ d)
                        + rng.uniform(0.0, 2.0 * np.pi))
        tot += a * a
    return f / np.sqrt(tot / 2.0 + 1e-12)


def blur_remesh(body: trimesh.Trimesh, footprint_mm: float,
                sigma: float) -> trimesh.Trimesh | None:
    """Remesh via marching cubes on a Gaussian-blurred occupancy field.

    The single stable-mesh primitive of the pipeline: uniform triangles
    (Laplacian smoothing is unstable on the hull/fillet needle triangles
    — it spikes them into sliver pleats) and a sub-voxel smooth
    isosurface (binary MC quantizes to voxel planes; on flanks nearly
    parallel to a plane family the steps stretch into 1 mm+ terraces no
    smoothing removes).  Returns None when MC fails; caller falls back.
    """
    import scipy.ndimage as _ndi
    from skimage import measure as _measure
    pitch = float(np.clip(footprint_mm / 56.0, 0.18, 0.32))
    try:
        vg  = body.voxelized(pitch).fill()
        mat = np.pad(vg.encoding.dense.astype(np.float32), 4)
        mat = _ndi.gaussian_filter(mat, sigma=sigma)
        mv, mf, _n, _v = _measure.marching_cubes(mat, level=0.5)
        out = trimesh.Trimesh(vertices=mv - 4.0, faces=mf, process=True)
        out.apply_transform(vg.transform)
        # skimage's winding is inverted vs trimesh: volume comes out
        # negative and every downstream boolean refuses ("not a volume").
        out.fix_normals()
        if not out.is_watertight or out.volume <= 0:
            return None
        return out
    except Exception:                               # noqa: BLE001
        return None
