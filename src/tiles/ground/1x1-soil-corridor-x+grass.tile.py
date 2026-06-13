# soil-corridor-x+grass.tile.py
#
# Bare soil forms a four-way crossing through grass.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter
from dharmatiles.scatter import Grass, Grouped

_species = SpeciesConfig()


tile = Tile(
    surface=SurfaceConfig(seed=105),
    areas=[
        Region(id='path', selector=FloodFill(0.5, 0.5), layers=[SoilCarpet()]),
        Region(
            id='meadow',
            selector=FloodFill((0.08, 0.08), (0.08, 0.92), (0.92, 0.92), (0.92, 0.08)),
            layers=[
                GrassCarpet(species=_species, placement=Grouped(groups_per_square=240)),
                Scatter(Grass(species=_species, placement=Grouped(groups_per_square=24))),
            ],
        ),
        Boundary(id='southwest-margin', from_anchor=Edge.LEFT(0.33), to_anchor=Edge.BOTTOM(0.33), path='organic', amplitude_mm=2.2, wavelength_mm=10.0),
        Boundary(id='northwest-margin', from_anchor=Edge.LEFT(0.67), to_anchor=Edge.TOP(0.33), path='organic', amplitude_mm=2.2, wavelength_mm=10.0),
        Boundary(id='northeast-margin', from_anchor=Edge.TOP(0.67), to_anchor=Edge.RIGHT(0.67), path='organic', amplitude_mm=2.2, wavelength_mm=10.0),
        Boundary(id='southeast-margin', from_anchor=Edge.RIGHT(0.33), to_anchor=Edge.BOTTOM(0.67), path='organic', amplitude_mm=2.2, wavelength_mm=10.0),
    ],
)
