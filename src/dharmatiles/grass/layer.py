"""Grass layer entry point."""

from __future__ import annotations

import numpy as np
import trimesh

from .config import GrassConfig
from .grow import grow_all
from .mesh import build_meshes, rasterise_paths_into_support


class GrassLayer:
    """Generate grass paths, build meshes, and update scene support."""

    def __init__(self, cfg: GrassConfig) -> None:
        self.cfg = cfg

    def build(self, scene, verbose: bool = True) -> list[trimesh.Trimesh]:
        surface = scene.config.surface
        rng = np.random.default_rng(self.cfg.seed)
        paths = grow_all(scene, surface, self.cfg, rng, verbose=verbose)
        meshes = build_meshes(paths, self.cfg, scene, surface)
        rasterise_paths_into_support(paths, self.cfg, scene, surface)
        if verbose:
            segs = [len(path.points) - 1 for path in paths]
            if segs:
                avg_len = np.mean([segs[i] * paths[i].seed.blade_segment_length for i in range(len(paths))])
                max_len = max(segs[i] * paths[i].seed.blade_segment_length for i in range(len(paths)))
                print(f"  Built {len(paths)} blades — avg {avg_len:.1f} mm, max {max_len:.1f} mm")
        return meshes


class FloppyGrassLayer(GrassLayer):
    """Compatibility name for the first species: simple floppy grass."""
