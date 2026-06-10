"""Internal grass blade builder used by ``scatter.Grass``."""

from __future__ import annotations

import numpy as np
import trimesh

from .config import GrassConfig
from .grow import grow_all
from .mesh import build_meshes


class FloppyGrassLayer:
    """Grow grass paths, build blade meshes, and update scene support.

    Not part of the public spec API — ``scatter.Grass.scatter()`` is the
    caller.  Instantiated with a single-species ``GrassConfig``.
    """

    def __init__(self, cfg: GrassConfig) -> None:
        self.cfg = cfg

    def build(self, scene, verbose: bool = True, placement_mask=None) -> list[trimesh.Trimesh]:
        surface = scene.surface
        rng = np.random.default_rng(self.cfg.seed)
        paths = grow_all(scene, surface, self.cfg, rng, verbose=verbose,
                         placement_mask=placement_mask)
        meshes = build_meshes(paths, self.cfg, scene, surface)
        if verbose:
            segs = [len(path.points) - 1 for path in paths]
            if segs:
                avg_len = np.mean([segs[i] * paths[i].seed.blade_segment_length for i in range(len(paths))])
                max_len = max(segs[i] * paths[i].seed.blade_segment_length for i in range(len(paths)))
                print(f"  Built {len(paths)} blades — avg {avg_len:.1f} mm, max {max_len:.1f} mm")
        return meshes
