"""1x1 grass meadow with one CloudTree, a few big rocks, and 3D grass blades."""

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig, SpeciesConfig, D
from dharmatiles.layers import GrassCarpet, Scatter
from dharmatiles.scatter import Rocks, Grass, CloudTree, Grouped, Uniform

_species = SpeciesConfig(
    blade_length=D[8:14],
    blade_curl=D[0.35:0.65],
    blade_clearance=0.2,
)

tile = Tile(
    surface=SurfaceConfig(seed=42),
    areas=[
        Region(
            id='meadow',
            selector=FloodFill(0.5, 0.5),
            layers=[
                GrassCarpet(
                    species=_species,
                    placement=Grouped(groups_per_square=3),
                ),
                Scatter(
                    # Rocks first — stamp obstacle_mask so grass steers around them
                    Rocks(
                        r=D[2.5:4.5],
                        placement=Uniform(count_per_square=3),
                    ),
                    # Tree next — trunk footprint stamped before grass is planted
                    CloudTree(
                        height_mm=35.0,
                        crown_radius_mm=12.0,
                        crown_base_radius_mm=11.0,
                        placement=Uniform(count_per_square=1),
                        n_attraction=32,
                        leaf_clumps=True,
                    ),
                    # Grass last — grows around rocks and tree trunk
                    Grass(
                        species=_species,
                        placement=Grouped(groups_per_square=3),
                    ),
                ),
            ],
        ),
    ],
)
