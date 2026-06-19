# water-u+grass.tile.py
#
# A water cove indents from the right edge; grass meadow surrounds it.
# One same-edge shoreline boundary carves out the cove.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, SpeciesConfig, D
from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter, Water
from dharmatiles.scatter import Rocks, Grass, Grouped, Uniform

_species = SpeciesConfig()


def _meadow(region_id: str, x: float, y: float) -> Region:
    return Region(
        id=region_id,
        selector=FloodFill(x, y),
        layers=[
            GrassCarpet(species=_species, placement=Grouped(groups_per_square=240)),
            Scatter(Grass(species=_species, placement=Grouped(groups_per_square=24))),
        ],
    )


_shore_rocks = lambda: Scatter(Rocks(placement=Uniform(count_per_square=50), r=D[0.8:2.0].power(1.5)))

tile = Tile(
    surface=SurfaceConfig(seed=63),
    areas=[
        Region(
            id='cove',
            selector=FloodFill(0.85, 0.5),
            height_mm=3.0,
            layers=[
                Scatter(
                    Rocks(
                        placement=Uniform(count_per_square=2),
                        r=D[2.5:4.5],
                        flat=D[1.665:1.81],
                        n_cuts=3,
                    ),
                ),
                Water(embed_mm=2.5),
            ],
        ),
        _meadow('meadow', 0.2, 0.5),
        Boundary(
            id='cove-shore',
            from_anchor=Edge.RIGHT(0.33),
            to_anchor=Edge.RIGHT(0.67),
            waypoints=[
                (0.56, 0.20),
                (0.28, 0.38),
                (0.34, 0.62),
                (0.58, 0.80),
            ],
            path='organic',
            amplitude_mm=1.6,
            wavelength_mm=8.0,
            width_mm=2.5,
            layers=[SoilCarpet(), _shore_rocks()],
        ),
    ],
)
