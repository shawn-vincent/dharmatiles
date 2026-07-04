# walls-e1-straight.tile.py — walls campaign E1: the core read.
#
# One DB-standard cut-stone wall (49.7 mm × 7 mm slab, flush to the
# north tile edge) on a bare soil square.  Judged side-by-side against
# the commercial RR-095-Wall render.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import CutStoneWall

tile = Tile(
    surface=SurfaceConfig(seed=11, cols=1, rows=1),
    areas=[
        Region(id='ground', selector=FloodFill(0.5, 0.5), layers=[
            SoilCarpet(),
            # Spine = outer face, walked with the tile interior on the
            # left: along the north edge from NE to NW.
            CutStoneWall(spine=[(1.0, 1.0), (0.0, 1.0)], seed=3),
        ]),
    ],
)
