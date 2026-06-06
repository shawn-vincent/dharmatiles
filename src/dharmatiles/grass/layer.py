"""Grass layer entry point."""

from __future__ import annotations

import numpy as np
import trimesh

from ..core.config import SceneConfig
from .config import GrassConfig, from_legacy_config
from .grow import grow_all
from .mesh import build_meshes, rasterise_paths_into_support


class GrassLayer:
    """Generate grass paths, build meshes, and update scene support."""

    def __init__(self, cfg: SceneConfig | GrassConfig) -> None:
        if isinstance(cfg, GrassConfig):
            self.surface = None
            self.cfg = cfg
        else:
            self.surface = cfg.surface
            self.cfg = from_legacy_config(
                cfg.grass,
                seed=cfg.surface.seed ^ 0x47524F57,
                max_stack_height=cfg.solver.max_stack_height,
            )

    def build(self, scene, verbose: bool = True) -> list[trimesh.Trimesh]:
        surface = self.surface if self.surface is not None else scene.config.surface
        rng = np.random.default_rng(self.cfg.seed)
        paths = grow_all(scene, surface, self.cfg, rng, verbose=verbose)
        meshes = build_meshes(paths, self.cfg, scene, surface)
        rasterise_paths_into_support(paths, self.cfg, scene, surface)
        if verbose:
            segs = [len(path.points) - 1 for path in paths]
            if segs:
                avg_len = np.mean([segs[i] * paths[i].seed.step_len for i in range(len(paths))])
                max_len = max(segs[i] * paths[i].seed.step_len for i in range(len(paths)))
                print(f"  Built {len(paths)} blades — avg {avg_len:.1f} mm, max {max_len:.1f} mm")
        return meshes


class FloppyGrassLayer(GrassLayer):
    """Compatibility name for the first species: simple floppy grass."""
