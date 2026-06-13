# soil-corridor-turn+grass.tile.py
#
# Bare soil turns from the left edge to the top edge through grass.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter
from dharmatiles.scatter import Grass, Grouped

_species = SpeciesConfig()


def _meadow() -> Region:
    return Region(
        id='meadow',
        selector=FloodFill((0.82, 0.18), (0.18, 0.88)),
        layers=[
            GrassCarpet(species=_species, placement=Grouped(groups_per_square=240)),
            Scatter(Grass(species=_species, placement=Grouped(groups_per_square=24))),
        ],
    )


tile = Tile(
    surface=SurfaceConfig(seed=103),
    areas=[
        Region(id='path', selector=FloodFill(0.35, 0.55), layers=[SoilCarpet()]),
        _meadow(),
        Boundary(
            id='outer-margin',
            from_anchor=Edge.LEFT(0.33),
            to_anchor=Edge.TOP(0.67),
            waypoints=[(0.64, 0.36)],
            path='organic',
            amplitude_mm=2.2,
            wavelength_mm=10.0,
        ),
        Boundary(
            id='inner-margin',
            from_anchor=Edge.LEFT(0.67),
            to_anchor=Edge.TOP(0.33),
            waypoints=[(0.36, 0.64)],
            path='organic',
            amplitude_mm=2.2,
            wavelength_mm=10.0,
        ),
    ],
)
