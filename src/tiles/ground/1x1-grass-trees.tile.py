"""
Grass-and-trees tile — one constructive deciduous tree on grass.

1×1 only.  One tree is placed per square (count=1).
The trunk and branches come from the constructive tree grower.
Rocks and 3D grass fill the rest; grass steers around the tree base.
"""

from dharmatiles.spec import (
    Tile, Region, SurfaceConfig, SpeciesConfig, FloodFill, D,
)
from dharmatiles.layers import GrassCarpet, Scatter
from dharmatiles.scatter import Rocks, Grass, ConstTree
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
                    ConstTree(
                        height_max_mm       = D[24.0:31.0],
                        n_trunk_segs        = 5,
                        n_levels            = 4,
                        n_segs_per_level    = [5, 3, 2, 1],
                        spread_angle_deg    = [35, 22, 14, 8],
                        pipe_model_exp      = 2.3,
                        r_base_mm           = D[2.5:4.5],
                        branch_r_tip_mm     = 0.30,
                        initial_lean_deg    = D[0.0:6.0],
                        wander_deg          = D[2.0:5.0],
                        min_elevation_deg   = 45.0,
                        ridge_amp           = 0.0,
                        wrinkle_amp         = 0.0,
                        placement           = Uniform(count_per_square=1),
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
