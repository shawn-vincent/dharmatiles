# water-u+soil.tile.py
#
# A water cove indents from the right edge through dry soil.
# One same-edge shoreline boundary carves out the cove.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, D
from dharmatiles.layers import SoilCarpet, Scatter, Water
from dharmatiles.scatter import Rocks, Uniform

_shore_rocks = lambda: Scatter(Rocks(placement=Uniform(count_per_square=50), r=D[0.8:2.0].power(1.5)))

tile = Tile(
    surface=SurfaceConfig(seed=54),
    areas=[
        Region(
            id='cove',
            selector=FloodFill(0.85, 0.5),
            height_mm=3.0,
            layers=[
                Scatter(
                    Rocks(
                        placement=Uniform(count_per_square=2),
                        r=D[2.5:4.5],
                        flat=D[1.665:1.81],
                        n_cuts=3,
                    ),
                ),
                Water(embed_mm=2.5),
            ],
        ),
        Region(id='bank', selector=FloodFill(0.2, 0.5), layers=[SoilCarpet()]),
        Boundary(
            id='cove-shore',
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
            width_mm=2.0,
            layers=[SoilCarpet(), _shore_rocks()],
        ),
    ],
)
