"""2x2 grass meadow with one CloudTree (point-cloud-partitioned growth, Bezier tubes)."""

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig, SpeciesConfig, D
from dharmatiles.layers import GrassCarpet, Scatter
from dharmatiles.scatter import CloudTree, Grouped, Uniform

_species = SpeciesConfig(
    blade_length=D[8:14],
    blade_curl=D[0.35:0.65],
    blade_clearance=0.2,
)

tile = Tile(
    surface=SurfaceConfig(cols=2, rows=2, seed=84),
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
                    CloudTree(
                        height_mm=40.0,
                        trunk_height_mm=20.0,
                        crown_radius_mm=18.0,
                        crown_base_radius_mm=18.0,
                        top_pointiness=0.0,
                        placement=Uniform(count_per_square=0.25),
                        debug_attractors=True,
                    ),
                ),
            ],
        ),
    ],
)
