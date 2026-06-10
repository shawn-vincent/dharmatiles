"""
ScatterLayer: unified placement pipeline for rocks and grass.

Takes a list of ``(prototype, placement_mask)`` pairs and runs them in two
phases:

Phase 0 — ``RockPrototype`` (sort_priority == 0):
  Generate seeds → sort big→small → realise (stamps terrain_support_z +
  rock_mask) — all rock prototypes finish before grass starts.

Phase 1 — ``GrassPrototype`` (sort_priority == 1):
  ``vegetation_support_z`` is synced from the now-complete
  ``terrain_support_z``, then each grass prototype grows its blades
  (reading rock_mask and terrain_support_z).

Additional prototype types can be added in the future by assigning them a
sort_priority and handling them in ``build()``.
"""
from __future__ import annotations

import numpy as np
import trimesh

from .prototype import RockPrototype, GrassPrototype


class ScatterLayer:
    """Place all scatter prototypes in priority order on a TileScene."""

    def __init__(
        self,
        pairs: list[tuple[object, np.ndarray | None]],
    ) -> None:
        """
        Parameters
        ----------
        pairs
            List of ``(prototype, placement_mask)`` — order within each
            priority tier is preserved.
        """
        self.pairs = pairs

    def build(
        self,
        scene,
        *,
        verbose:          bool  = True,
        max_stack_height: float = 2.0,
    ) -> list[trimesh.Trimesh]:
        """Realise all prototypes and return their combined mesh list."""
        surface = scene.config.surface
        parts: list[trimesh.Trimesh] = []

        rock_pairs  = [(p, m) for p, m in self.pairs if isinstance(p, RockPrototype)]
        grass_pairs = [(p, m) for p, m in self.pairs if isinstance(p, GrassPrototype)]

        # ── Phase 0: rocks ────────────────────────────────────────────────────
        if rock_pairs:
            # Pre-compute terrain gradient once; all rock passes share it.
            _cw          = surface.cell_w
            _rock_gz_x   = np.gradient(scene.terrain_z, axis=1) / _cw
            _rock_gz_y   = np.gradient(scene.terrain_z, axis=0) / _cw

            for layer_idx, (proto, pmask) in enumerate(rock_pairs):
                # Each prototype gets its own independent RNG stream.
                rng_seed = (surface.seed
                            ^ 0x726F636B          # "rock"
                            ^ proto.scatter.seed
                            ^ (layer_idx * 65537))
                rng = np.random.default_rng(rng_seed)

                from .distribute import scatter_positions
                n_sq      = surface.cols * surface.rows
                positions = scatter_positions(
                    proto.scatter, n_sq, proto.footprint_mm(),
                    pmask, scene, surface, rng,
                )

                # Create seeds with geometry baked in, then sort big→small.
                seeds = [proto.make_seed(x, y, gd, rng)
                         for x, y, gd in positions]
                seeds.sort(key=lambda s: s.sort_key())

                n_rocks = len(seeds)
                if n_rocks > 0:
                    if verbose:
                        n_sq_total = surface.cols * surface.rows
                        print(f"Building rocks  ({n_rocks} rocks = "
                              f"{n_rocks // max(n_sq_total, 1)}"
                              f"/{n_sq_total} sq, sorted big→small)...")
                    rock_meshes = proto.realize(
                        seeds, scene, surface,
                        layer_idx    = layer_idx,
                        verbose      = False,
                        terrain_gz_x = _rock_gz_x,
                        terrain_gz_y = _rock_gz_y,
                    )
                    parts.extend(rock_meshes)

        # ── Phase 1: grass ────────────────────────────────────────────────────
        if grass_pairs:
            # Sync vegetation_support_z: grass blades ride on top of rocks.
            scene.vegetation_support_z = scene.terrain_support_z.copy()

            if verbose:
                print("Growing grass...")

            global_grass_mask = scene.grass_mask
            for i, (proto, pmask) in enumerate(grass_pairs):
                layer_seed = (surface.seed
                              ^ 0x47524F57        # "GROW"
                              ^ proto.scatter.seed
                              ^ (i * 65537))
                grass_meshes = proto.realize(
                    scene, surface,
                    placement_mask   = pmask,
                    layer_seed       = layer_seed,
                    verbose          = (verbose and i == 0),
                    max_stack_height = max_stack_height,
                )
                parts.extend(grass_meshes)
            scene.grass_mask = global_grass_mask  # restore

        return parts
