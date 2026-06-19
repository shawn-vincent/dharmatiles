# water-corridor-t+grass.tile.py
#
# Water forms a T junction open to left, right, and top through grass.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, SpeciesConfig, D
from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter, Water
from dharmatiles.scatter import Rocks, Grass, Grouped, Uniform

_species = SpeciesConfig()


def _shore() -> list:
    return [
        SoilCarpet(),
        Scatter(Rocks(placement=Uniform(count_per_square=40), r=D[0.8:2.0].power(1.5))),
    ]


tile = Tile(
    surface=SurfaceConfig(seed=65),
    areas=[
        Region(
            id='channel',
            selector=FloodFill(0.5, 0.5),
            height_mm=3.0,
            layers=[
                Scatter(Rocks(placement=Uniform(count_per_square=1), r=D[3.0:5.0], flat=D[1.725:1.86], n_cuts=3)),
                Water(embed_mm=2.5),
            ],
        ),
        Region(
            id='meadow',
            selector=FloodFill((0.18, 0.88), (0.82, 0.88), (0.5, 0.18)),
            layers=[
                GrassCarpet(species=_species, placement=Grouped(groups_per_square=240)),
                Scatter(Grass(species=_species, placement=Grouped(groups_per_square=24))),
            ],
        ),
        Boundary(id='south-shore', from_anchor=Edge.LEFT(0.33), to_anchor=Edge.RIGHT(0.33), path='organic', amplitude_mm=4.0, wavelength_mm=12.0, width_mm=2.5, layers=_shore()),
        Boundary(id='northwest-shore', from_anchor=Edge.LEFT(0.67), to_anchor=Edge.TOP(0.33), waypoints=[(0.34, 0.68)], path='organic', amplitude_mm=3.0, wavelength_mm=12.0, width_mm=2.5, layers=_shore()),
        Boundary(id='northeast-shore', from_anchor=Edge.TOP(0.67), to_anchor=Edge.RIGHT(0.67), waypoints=[(0.66, 0.68)], path='organic', amplitude_mm=3.0, wavelength_mm=12.0, width_mm=2.5, layers=_shore()),
    ],
)
