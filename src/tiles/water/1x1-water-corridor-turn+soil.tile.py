# water-corridor-turn+soil.tile.py
#
# Water turns from the left edge to the top edge through soil.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, FlatHeight, SurfaceConfig, D
from dharmatiles.layers import SoilCarpet, Scatter, Water
from dharmatiles.scatter import Rocks, Uniform


def _shore() -> list:
    return [
        SoilCarpet(),
        Scatter(Rocks(placement=Uniform(count_per_square=40), r=D[0.8:2.0].power(1.5))),
    ]


tile = Tile(
    surface=SurfaceConfig(seed=55),
    areas=[
        Region(
            id='channel',
            selector=FloodFill(0.35, 0.55),
            terrain=FlatHeight(3.0),
            layers=[
                Scatter(Rocks(placement=Uniform(count_per_square=1), r=D[3.0:5.0], flat=D[1.725:1.86], n_cuts=3)),
                Water(embed_mm=2.5),
            ],
        ),
        Region(id='bank', selector=FloodFill((0.82, 0.18), (0.18, 0.88)), layers=[SoilCarpet()]),
        Boundary(id='outer-shore', from_anchor=Edge.LEFT(0.33), to_anchor=Edge.TOP(0.67), waypoints=[(0.64, 0.36)], path='organic', amplitude_mm=3.0, wavelength_mm=12.0, width_mm=2.5, layers=_shore()),
        Boundary(id='inner-shore', from_anchor=Edge.LEFT(0.67), to_anchor=Edge.TOP(0.33), waypoints=[(0.36, 0.64)], path='organic', amplitude_mm=3.0, wavelength_mm=12.0, width_mm=2.5, layers=_shore()),
    ],
)
