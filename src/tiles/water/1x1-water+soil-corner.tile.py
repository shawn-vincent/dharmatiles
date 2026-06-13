# water+soil-corner.tile.py
#
# Water fills most of the tile; a soil bank juts into the bottom-left
# corner — like a small peninsula or mudflat.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, FlatHeight, SurfaceConfig, D
from dharmatiles.layers import SoilCarpet, Scatter, Water
from dharmatiles.scatter import Rocks, Uniform

tile = Tile(
    surface=SurfaceConfig(seed=51),
    areas=[
        Region(
            id='pool',
            selector=FloodFill(0.75, 0.75),
            terrain=FlatHeight(3.0),
            layers=[
                Scatter(
                    Rocks(
                        placement=Uniform(count_per_square=2),
                        r=D[3.0:5.0],
                        flat=D[2.0:2.8],
                        n_cuts=3,
                    ),
                ),
                Water(embed_mm=2.5),
            ],
        ),
        Region(
            id='bank',
            selector=FloodFill(0.15, 0.15),
            layers=[SoilCarpet()],
        ),
        Boundary(
            id='shoreline',
            from_anchor=Edge.LEFT(0.5),
            to_anchor=Edge.BOTTOM(0.5),
            path='organic',
            amplitude_mm=2.5,
            wavelength_mm=8.0,
            width_mm=2.0,
            layers=[
                SoilCarpet(),
                Scatter(
                    Rocks(
                        placement=Uniform(count_per_square=60),
                        r=D[0.8:2.2].power(1.5),
                    ),
                ),
            ],
        ),
    ],
)
