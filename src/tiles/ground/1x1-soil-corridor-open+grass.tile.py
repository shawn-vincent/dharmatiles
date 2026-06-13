# soil-corridor-open+grass.tile.py
#
# Bare soil enters as a corridor on the left and opens into a broad side.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter
from dharmatiles.scatter import Grass, Grouped

_species = SpeciesConfig()


tile = Tile(
    surface=SurfaceConfig(seed=106),
    areas=[
        Region(id='path', selector=FloodFill(0.75, 0.5), layers=[SoilCarpet()]),
        Region(
            id='meadow',
            selector=FloodFill((0.08, 0.08), (0.08, 0.92)),
            layers=[
                GrassCarpet(species=_species, placement=Grouped(groups_per_square=240)),
                Scatter(Grass(species=_species, placement=Grouped(groups_per_square=24))),
            ],
        ),
        Boundary(
            id='south-margin',
            from_anchor=Edge.LEFT(0.33),
            to_anchor=Edge.BOTTOM(0.50),
            path='organic',
            amplitude_mm=2.5,
            wavelength_mm=10.0,
        ),
        Boundary(
            id='north-margin',
            from_anchor=Edge.LEFT(0.67),
            to_anchor=Edge.TOP(0.50),
            path='organic',
            amplitude_mm=2.5,
            wavelength_mm=10.0,
        ),
    ],
)
