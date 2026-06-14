"""
Grass-and-trees tile — one unified deciduous tree on grass.

1×1 only.  One tree is placed per square (count=1).
The trunk emerges from SCA's attractor-free zone below the crown;
trunk and branches are a single bark-surfaced skeleton.
Rocks and 3D grass fill the rest; grass steers around the tree base.
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
                        crown_base_z_mm  = 10,   # effective trunk height
                        r_base_mm        = D[2.5:4.5],
                        crown_rx         = D[7.0:12.0],
                        crown_ry         = D[7.0:12.0],
                        crown_rz         = D[5.0:8.0],
                        n_attractors     = 140,
                        sca_segment_mm   = 2.0,
                        sca_perception_r = 9.0,
                        sca_kill_r       = 3.5,
                        sca_tropism      = 0.30,
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
