"""
Grass-and-trees tile — deciduous trunks with bark texture + branch crowns.

Generated sizes: 1×1, 2×2, 3×3 squares.

Tree density: 1 tree per 2×2-square area (count_per_square=0.25 on 2×2 →
approximately 1 tree).  On a 1×1 tile there is a 25% chance of a tree;
on a 3×3 there will be roughly 2.

Placement order in ScatterLayer:
  Trees → Rocks → Grass
so grass blades steer around both rock footprints and trunk footprints.
"""

from dharmatiles.spec import (
    Tile, Region, SurfaceConfig, SpeciesConfig, FloodFill,
    D, repeat_sizes,
)
from dharmatiles.layers import GrassCarpet as GrassCarpetLayer, Scatter as ScatterLayer
from dharmatiles.scatter import Rocks, Grass, Trees
from dharmatiles.scatter.config import Uniform, Grouped

species = SpeciesConfig()

_base = Tile(
    surface=SurfaceConfig(seed=82),
    areas=[
        Region(
            id='meadow',
            selector=FloodFill(0.5, 0.5),
            layers=[
                GrassCarpetLayer(
                    species=species,
                    placement=Grouped(groups_per_square=240),
                ),
                ScatterLayer(
                    Trees(
                        # Trunk geometry
                        height_mm   = D[22.0:38.0],
                        r_base_mm   = D[2.5:4.5],
                        n_stubs     = 3,
                        # Branch crown
                        grow_branches   = True,
                        crown_rx        = D[7.0:12.0],
                        crown_ry        = D[7.0:12.0],
                        crown_rz        = D[5.0:8.0],
                        n_attractors    = 100,
                        sca_segment_mm  = 2.5,
                        sca_tropism     = 0.35,
                        branch_r_tip_mm = 0.5,
                        branch_min_r_mm = 0.35,
                        placement=Uniform(count_per_square=0),  # overridden per size below
                    ),
                    Rocks(r=D[0.8:2.0]),
                    Grass(
                        species=species,
                        placement=Grouped(groups_per_square=24),
                    ),
                ),
            ],
        ),
    ],
)

# Regenerate with a per-size density override so 1×1 gets ~0 trees,
# 2×2 gets ~1, 3×3 gets ~2.  We use separate Tile instances for each size.

def _make(cols, rows, trees_per_sq):
    new_trees = Trees(
        height_mm       = D[22.0:38.0],
        r_base_mm       = D[2.5:4.5],
        n_stubs         = 3,
        grow_branches   = True,
        crown_rx        = D[7.0:12.0],
        crown_ry        = D[7.0:12.0],
        crown_rz        = D[5.0:8.0],
        n_attractors    = 100,
        sca_segment_mm  = 2.5,
        sca_tropism     = 0.35,
        branch_r_tip_mm = 0.5,
        branch_min_r_mm = 0.35,
        placement       = Uniform(count_per_square=trees_per_sq),
    )
    return Tile(
        surface=SurfaceConfig(seed=82, cols=cols, rows=rows),
        areas=[
            Region(
                id='meadow',
                selector=FloodFill(0.5, 0.5),
                layers=[
                    GrassCarpetLayer(
                        species=species,
                        placement=Grouped(groups_per_square=240),
                    ),
                    ScatterLayer(
                        new_trees,
                        Rocks(r=D[0.8:2.0]),
                        Grass(
                            species=species,
                            placement=Grouped(groups_per_square=24),
                        ),
                    ),
                ],
            ),
        ],
    )

tiles = [
    _make(1, 1, 0.25),   # 1×1 — rarely has a tree
    _make(2, 2, 0.30),   # 2×2 — ~1 tree
    _make(3, 3, 0.25),   # 3×3 — ~2 trees
]
