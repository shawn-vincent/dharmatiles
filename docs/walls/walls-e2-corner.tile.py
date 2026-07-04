# walls-e2-corner.tile.py — walls campaign: the corner read (quoins).
#
# One wall run turning the NW corner: north edge + west edge, DB
# standard height.  Judged for per-course quoin alternation at the
# corner and clean flush cuts at both tile-boundary ends.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import CutStoneWall

tile = Tile(
    surface=SurfaceConfig(seed=12, cols=1, rows=1),
    areas=[
        Region(id='ground', selector=FloodFill(0.6, 0.4), layers=[
            SoilCarpet(),
            # NE → NW → SW, tile interior on the left throughout.
            CutStoneWall(spine=[(1.0, 1.0), (0.0, 1.0), (0.0, 0.0)],
                         seed=5),
        ]),
    ],
)
