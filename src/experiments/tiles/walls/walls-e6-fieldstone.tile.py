# walls-e6-fieldstone.tile.py — walls campaign: the fieldstone family.
#
# A DB-standard drystone fieldstone wall turning the NW corner: rounded
# lumpy stones in rough courses with deep shadow joints, squared quoins
# at the corner, flat coping stones on top.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import FieldstoneWall

tile = Tile(
    surface=SurfaceConfig(seed=17, cols=1, rows=1),
    areas=[
        Region(id='ground', selector=FloodFill(0.6, 0.4), layers=[
            SoilCarpet(),
            FieldstoneWall(spine=[(1.0, 1.0), (0.0, 1.0), (0.0, 0.0)],
                           seed=7),
        ]),
    ],
)
