# walls-e11-floor.tile.py — walls campaign: stone slab flooring (E32).
#
# A 2x2 courtyard: dressed slab pavement gridding the ground (one slab
# per DB square, a few missing — bare dirt patches), walled on the
# north edge.  The pavement is the masonry unit kernel laid flat.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import CutStoneWall, StoneFloor

tile = Tile(
    surface=SurfaceConfig(seed=41, cols=2, rows=2),
    areas=[
        Region(id='court', selector=FloodFill(0.5, 0.3), layers=[
            SoilCarpet(),
            StoneFloor(texture='chipped', seed=91),
            CutStoneWall(spine=[(2.0, 2.0), (0.0, 2.0)], seed=92),
        ]),
    ],
)
