# 1x1-floor-stone-wall.tile.py
#
# Chipped slab pavement with a cut-stone wall on the north edge.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import CutStoneWall, StoneFloor

tile = Tile(
    surface=SurfaceConfig(seed=72),
    areas=[
        Region(id='court', selector=FloodFill(0.5, 0.3), layers=[
            SoilCarpet(),
            CutStoneWall(spine=[(1.0, 1.0), (0.0, 1.0)], seed=23),
            StoneFloor(texture='chipped', seed=22),
        ]),
    ],
)
