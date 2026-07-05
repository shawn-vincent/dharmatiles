# walls-e8-brick.tile.py — walls campaign: the brick family (family 2).
#
# A DB-standard worn brick wall turning the NW corner (same plan as the
# fieldstone e6 scene for direct comparison): running bond of small
# near-uniform units, eroded mortar recesses, spalled bricks and the
# odd missing one breaking the grid.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import BrickWall

tile = Tile(
    surface=SurfaceConfig(seed=17, cols=1, rows=1),
    areas=[
        Region(id='ground', selector=FloodFill(0.6, 0.4), layers=[
            SoilCarpet(),
            BrickWall(spine=[(1.0, 1.0), (0.0, 1.0), (0.0, 0.0)],
                      seed=7),
        ]),
    ],
)
