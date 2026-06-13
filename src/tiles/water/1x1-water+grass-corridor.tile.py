# water+grass-corridor.tile.py
#
# Grass meadows run along the top and bottom edges; a water channel flows
# left-to-right through the center — like a stream where you can see both
# grassy banks at once.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, FlatHeight, SurfaceConfig, SpeciesConfig, D
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


tile = Tile(
    surface=SurfaceConfig(seed=62),
    areas=[
        _meadow('meadow-top', 0.5, 0.85),
        Boundary(
            id='north-shore',
            from_anchor=Edge.LEFT(0.67),
            to_anchor=Edge.RIGHT(0.67),
            path='organic',
            amplitude_mm=4.0,
            wavelength_mm=12.0,
            width_mm=2.5,
            layers=[
                SoilCarpet(),
                Scatter(
                    Rocks(
                        placement=Uniform(count_per_square=40),
                        r=D[0.8:2.0].power(1.5),
                    ),
                ),
            ],
        ),
        Region(
            id='channel',
            selector=FloodFill(0.5, 0.5),
            terrain=FlatHeight(3.0),
            layers=[
                Scatter(
                    Rocks(
                        placement=Uniform(count_per_square=1),
                        r=D[3.0:5.0],
                        flat=D[1.725:1.86],
                        n_cuts=3,
                    ),
                ),
                Water(embed_mm=2.5),
            ],
        ),
        Boundary(
            id='south-shore',
            from_anchor=Edge.LEFT(0.33),
            to_anchor=Edge.RIGHT(0.33),
            path='organic',
            amplitude_mm=4.0,
            wavelength_mm=12.0,
            width_mm=2.5,
            layers=[
                SoilCarpet(),
                Scatter(
                    Rocks(
                        placement=Uniform(count_per_square=40),
                        r=D[0.8:2.0].power(1.5),
                    ),
                ),
            ],
        ),
        _meadow('meadow-bottom', 0.5, 0.15),
    ],
)
