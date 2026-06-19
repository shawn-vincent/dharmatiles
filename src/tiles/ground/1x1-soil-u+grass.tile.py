# soil-u+grass.tile.py
#
# Grass surrounds a bare-soil clearing carved from the right edge by one
# same-edge boundary.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import SoilCarpet, GrassCarpet
from dharmatiles.scatter import Grass, Grouped

_species = SpeciesConfig()


def _meadow(region_id: str, x: float, y: float) -> Region:
    return Region(
        id=region_id,
        selector=FloodFill(x, y),
        layers=[
            GrassCarpet(species=_species, placement=Grouped(groups_per_square=240)),
            Grass(species=_species, placement=Grouped(groups_per_square=24)),
        ],
    )


tile = Tile(
    surface=SurfaceConfig(seed=102),
    areas=[
        Region(
            id='clearing',
            selector=FloodFill(0.85, 0.5),
            layers=[SoilCarpet()],
        ),
        _meadow('meadow', 0.2, 0.5),
        Boundary(
            id='clearing-loop',
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
        ),
    ],
)
