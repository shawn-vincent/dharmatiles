"""
Grass-and-trees tile — one canopy-surface SCA-style deciduous tree on grass.

1×1 only.  One tree is placed per square (count=1).
The trunk and branches terminate on evenly spaced canopy surface points.
Rocks and 3D grass fill the rest; grass steers around the tree base.
"""

from dharmatiles.spec import (
    Tile, Region, SurfaceConfig, SpeciesConfig, FloodFill, D,
)
from dharmatiles.layers import GrassCarpet, Scatter
from dharmatiles.scatter import Rocks, Grass, SurfaceScaTree
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
                    SurfaceScaTree(
                        height_max_mm          = 40.0,
                        trunk_height_mm        = 5.0,
                        n_trunk_segs           = 5,
                        n_levels               = 4,
                        crown_radius_mm        = 20.0,
                        top_pointiness         = 0.0,
                        top_curve              = 1.40,
                        bottom_pointiness      = 0.35,
                        bottom_curve           = 0.80,
                        pipe_model_exp         = 2.3,
                        r_base_mm              = D[2.5:4.5],
                        branch_r_tip_mm        = 0.30,
                        initial_lean_deg       = D[0.0:6.0],
                        wander_deg             = D[2.0:5.0],
                        min_elevation_deg      = 45.0,
                        ridge_amp              = 0.0,
                        wrinkle_amp            = 0.0,
                        surface_point_spacing_mm  = 7.0,
                        surface_branch_segment_mm = 2.0,
                        surface_branch_lift_mm    = 0.7,
                        surface_sca_target_bias   = 0.65,
                        surface_sca_tangent_bias  = 0.35,
                        placement              = Uniform(count_per_square=1),
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
