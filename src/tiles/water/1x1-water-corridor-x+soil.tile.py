# water-corridor-x+soil.tile.py
#
# Water forms a four-way crossing through soil.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, D
from dharmatiles.layers import SoilCarpet, Scatter, Water
from dharmatiles.scatter import Rocks, Uniform


def _shore() -> list:
    return [
        SoilCarpet(),
        Scatter(Rocks(placement=Uniform(count_per_square=40), r=D[0.8:2.0].power(1.5))),
    ]


tile = Tile(
    surface=SurfaceConfig(seed=57),
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
        Region(id='bank', selector=FloodFill((0.08, 0.08), (0.08, 0.92), (0.92, 0.92), (0.92, 0.08)), layers=[SoilCarpet()]),
        Boundary(id='southwest-shore', from_anchor=Edge.LEFT(0.33), to_anchor=Edge.BOTTOM(0.33), path='straight', width_mm=2.5, layers=_shore()),
        Boundary(id='northwest-shore', from_anchor=Edge.LEFT(0.67), to_anchor=Edge.TOP(0.33), path='straight', width_mm=2.5, layers=_shore()),
        Boundary(id='northeast-shore', from_anchor=Edge.TOP(0.67), to_anchor=Edge.RIGHT(0.67), path='straight', width_mm=2.5, layers=_shore()),
        Boundary(id='southeast-shore', from_anchor=Edge.RIGHT(0.33), to_anchor=Edge.BOTTOM(0.67), path='straight', width_mm=2.5, layers=_shore()),
    ],
)
