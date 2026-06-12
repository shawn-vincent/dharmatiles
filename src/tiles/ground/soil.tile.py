# soil.tile.py
#
# Entire surface is bare soil — no grass, no water.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet

tile = Tile(
    surface=SurfaceConfig(seed=13),
    areas=[
        Region(
            id='ground',
            selector=FloodFill(0.5, 0.5),
            layers=[SoilCarpet()],
        ),
    ],
)
