"""Surface fields and remeshing: the finish half of stone-making."""
from __future__ import annotations

import numpy as np
import trimesh


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
