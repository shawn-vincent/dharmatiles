# 1x1-floor-stone.tile.py
#
# Dressed stone slab pavement gridding a 1x1 (one slab per DB square).

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import StoneFloor

tile = Tile(
    surface=SurfaceConfig(seed=71),
    areas=[
        Region(id='court', selector=FloodFill(0.5, 0.3), layers=[
            SoilCarpet(),
            StoneFloor(texture='dressed', seed=21),
        ]),
    ],
)
