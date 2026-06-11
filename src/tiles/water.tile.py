# water.tile.py
#
# Entire surface is a water pool at 3 mm (2 mm below the 5 mm ground level).

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import Water

tile = Tile(
    surface=SurfaceConfig(seed=17),
    areas=[
        Region(
            id='pool',
            selector=FloodFill(0.5, 0.5),
            height_mm=3.0,
            layers=[
                Water(),
            ],
        ),
    ],
)
