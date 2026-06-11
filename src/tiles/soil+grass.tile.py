# soil+grass.tile.py
#
# Left half: grass.  Right half: bare soil.
# The organic boundary creates a natural meadow margin.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter
from dharmatiles.scatter import Grass, Grouped

_species = SpeciesConfig()

tile = Tile(
    surface=SurfaceConfig(seed=42),
    areas=[
        Region(
            id='meadow',
            selector=FloodFill(0.25, 0.5),
            layers=[
                GrassCarpet(species=_species, placement=Grouped(groups_per_square=240)),
                Scatter(
                    Grass(species=_species, placement=Grouped(groups_per_square=24)),
                ),
            ],
        ),
        Boundary(
            id='margin',
            from_anchor=Edge.TOP(0.48),
            to_anchor=Edge.BOTTOM(0.52),
            path='organic',
            amplitude_mm=5.0,
            wavelength_mm=10.0,
        ),
        Region(
            id='dirt',
            selector=FloodFill(0.75, 0.5),
            layers=[
                SoilCarpet(),
            ],
        ),
    ],
)
