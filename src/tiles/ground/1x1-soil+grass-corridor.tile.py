# soil+grass-corridor.tile.py
#
# Grass strips run along the top and bottom edges; bare soil cuts across
# the middle as a path or open clearing.  Two organic boundaries divide
# the tile into three horizontal bands.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter
from dharmatiles.scatter import Grass, Grouped

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
    surface=SurfaceConfig(seed=101),
    areas=[
        _meadow('meadow-top', 0.5, 0.85),
        Boundary(
            id='top-margin',
            from_anchor=Edge.LEFT(0.67),
            to_anchor=Edge.RIGHT(0.67),
            path='organic',
            amplitude_mm=3.0,
            wavelength_mm=10.0,
        ),
        Region(
            id='path',
            selector=FloodFill(0.5, 0.5),
            layers=[SoilCarpet()],
        ),
        Boundary(
            id='bottom-margin',
            from_anchor=Edge.LEFT(0.33),
            to_anchor=Edge.RIGHT(0.33),
            path='organic',
            amplitude_mm=3.0,
            wavelength_mm=10.0,
        ),
        _meadow('meadow-bottom', 0.5, 0.15),
    ],
)
