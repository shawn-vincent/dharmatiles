"""
Grass-and-trees tile — one deciduous tree trunk with branch crown, on grass.

1×1 only.  One tree is placed at a random position on the tile (count=1).
Rocks and 3D grass fill the rest of the meadow; grass steers around the trunk.
"""

from dharmatiles.spec import (
    Tile, Region, SurfaceConfig, SpeciesConfig, FloodFill, D,
)
from dharmatiles.layers import GrassCarpet, Scatter
from dharmatiles.scatter import Rocks, Grass, Trees
from dharmatiles.scatter.config import Uniform, Grouped

species = SpeciesConfig()

tile = Tile(
    surface=SurfaceConfig(seed=82),
    areas=[
        Region(
            id='meadow',
            selector=FloodFill(0.5, 0.5),
            layers=[
                GrassCarpet(
                    species=species,
                    placement=Grouped(groups_per_square=240),
                ),
                Scatter(
                    Trees(
                        height_mm        = D[22.0:38.0],
                        r_base_mm        = D[2.5:4.5],
                        n_stubs          = 0,           # no vestigial stubs; SCA handles all branching
                        grow_branches    = True,
                        crown_rx         = D[7.0:12.0],
                        crown_ry         = D[7.0:12.0],
                        crown_rz         = D[5.0:8.0],
                        n_attractors     = 120,
                        sca_segment_mm   = 2.5,
                        sca_perception_r = 9.0,         # large enough to reach crown from trunk roots
                        sca_kill_r       = 3.0,         # bigger kill zone prevents spiral loops
                        sca_tropism      = 0.35,        # upward bias keeps branches from drooping
                        sca_branch_xy_std  = 0.30,      # trigger branching when XY spread > 0.30
                        sca_min_branch_att = 3,         # each cluster needs ≥ 3 attractors
                        sca_trunk_root_frac = 0.60,     # top 40% of spine used as SCA roots
                        branch_r_tip_mm  = 0.5,
                        branch_min_r_mm  = 0.35,
                        placement        = Uniform(count_per_square=1),
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
