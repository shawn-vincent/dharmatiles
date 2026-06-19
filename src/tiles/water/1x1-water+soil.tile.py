# water+soil.tile.py
#
# Lower half: water pool (3 mm).
# Upper half: bare soil bank.
# Organic shoreline with a soil slope and waterline pebbles.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, D
from dharmatiles.layers import SoilCarpet, Water
from dharmatiles.scatter import Rocks, Uniform

tile = Tile(
    surface=SurfaceConfig(seed=50),
    areas=[
        Region(
            id='pool',
            selector=FloodFill(0.5, 0.25),
            height_mm=3.0,
            layers=[
                Rocks(
                    placement=Uniform(count_per_square=2),
                    r=D[3.0:5.0],
                    flat=D[1.725:1.86],
                    n_cuts=3,
                ),
                Water(embed_mm=2.5),
            ],
        ),
        Boundary(
            id='shoreline',
            from_anchor=Edge.LEFT(0.48),
            to_anchor=Edge.RIGHT(0.52),
            path='organic',
            amplitude_mm=5.0,
            wavelength_mm=12.0,
            width_mm=2.5,
            layers=[
                SoilCarpet(),
                Rocks(
                    placement=Uniform(count_per_square=60),
                    r=D[0.8:2.2].power(1.5),
                ),
            ],
        ),
        Region(
            id='bank',
            selector=FloodFill(0.5, 0.75),
            layers=[SoilCarpet()],
        ),
    ],
)
