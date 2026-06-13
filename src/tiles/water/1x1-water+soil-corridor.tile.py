# water+soil-corridor.tile.py
#
# Two soil banks flank a central water channel running left to right —
# like a river ford where both banks are visible from the crossing.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, FlatHeight, SurfaceConfig, D
from dharmatiles.layers import SoilCarpet, Scatter, Water
from dharmatiles.scatter import Rocks, Uniform

tile = Tile(
    surface=SurfaceConfig(seed=53),
    areas=[
        Region(
            id='bank-top',
            selector=FloodFill(0.5, 0.85),
            layers=[SoilCarpet()],
        ),
        Boundary(
            id='north-shore',
            from_anchor=Edge.LEFT(0.67),
            to_anchor=Edge.RIGHT(0.67),
            path='organic',
            amplitude_mm=4.0,
            wavelength_mm=12.0,
            width_mm=2.5,
            layers=[
                SoilCarpet(),
                Scatter(
                    Rocks(
                        placement=Uniform(count_per_square=40),
                        r=D[0.8:2.0].power(1.5),
                    ),
                ),
            ],
        ),
        Region(
            id='channel',
            selector=FloodFill(0.5, 0.5),
            terrain=FlatHeight(3.0),
            layers=[
                Scatter(
                    Rocks(
                        placement=Uniform(count_per_square=1),
                        r=D[3.0:5.0],
                        flat=D[1.725:1.86],
                        n_cuts=3,
                    ),
                ),
                Water(embed_mm=2.5),
            ],
        ),
        Boundary(
            id='south-shore',
            from_anchor=Edge.LEFT(0.33),
            to_anchor=Edge.RIGHT(0.33),
            path='organic',
            amplitude_mm=4.0,
            wavelength_mm=12.0,
            width_mm=2.5,
            layers=[
                SoilCarpet(),
                Scatter(
                    Rocks(
                        placement=Uniform(count_per_square=40),
                        r=D[0.8:2.0].power(1.5),
                    ),
                ),
            ],
        ),
        Region(
            id='bank-bottom',
            selector=FloodFill(0.5, 0.15),
            layers=[SoilCarpet()],
        ),
    ],
)
