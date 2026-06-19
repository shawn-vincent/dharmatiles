# soil-corridor-t+grass.tile.py
#
# Bare soil forms a T junction open to left, right, and top through grass.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import SoilCarpet, GrassCarpet
from dharmatiles.scatter import Grass, Grouped

_species = SpeciesConfig()


tile = Tile(
    surface=SurfaceConfig(seed=104),
    areas=[
        Region(id='path', selector=FloodFill(0.5, 0.5), layers=[SoilCarpet()]),
        Region(
            id='meadow',
            selector=FloodFill((0.18, 0.88), (0.82, 0.88), (0.5, 0.18)),
            layers=[
                GrassCarpet(species=_species, placement=Grouped(groups_per_square=240)),
                Grass(species=_species, placement=Grouped(groups_per_square=24)),
            ],
        ),
        Boundary(
            id='south-margin',
            from_anchor=Edge.LEFT(0.33),
            to_anchor=Edge.RIGHT(0.33),
            path='organic',
            amplitude_mm=3.0,
            wavelength_mm=10.0,
        ),
        Boundary(
            id='northwest-margin',
            from_anchor=Edge.LEFT(0.67),
            to_anchor=Edge.TOP(0.33),
            waypoints=[(0.34, 0.68)],
            path='organic',
            amplitude_mm=2.2,
            wavelength_mm=10.0,
        ),
        Boundary(
            id='northeast-margin',
            from_anchor=Edge.TOP(0.67),
            to_anchor=Edge.RIGHT(0.67),
            waypoints=[(0.66, 0.68)],
            path='organic',
            amplitude_mm=2.2,
            wavelength_mm=10.0,
        ),
    ],
)
