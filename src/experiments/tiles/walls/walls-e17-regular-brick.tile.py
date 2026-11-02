# walls-e17-regular-brick.tile.py — walls campaign: regular brick.
#
# RegularBrickWall is the tidy counterpart to the worn BrickWall — the
# SAME chassis with the `bond='running'` layout variant: uniform course
# heights, uniform-length bricks, a clean half-brick stagger, near-flush
# dead-level units.  End bricks become closers.
#
#   front: RegularBrickWall with a doorway — the clean running bond, and
#          the bond still flows around an opening surround.
#   back:  BrickWall (worn) for direct comparison — the imperfect grid.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import BrickWall, RegularBrickWall, Opening

_X0, _X1 = 1.9, 0.1

tile = Tile(
    surface=SurfaceConfig(seed=71, cols=2, rows=2),
    areas=[
        Region(id='ground', selector=FloodFill(0.5, 0.05), layers=[
            SoilCarpet(),
            RegularBrickWall([(_X0, 0.6), (_X1, 0.6)], seed=401, openings=[
                Opening(at=1.0, width_mm=20.0, head_mm=26.0),
            ]),
            BrickWall([(_X0, 1.5), (_X1, 1.5)], seed=402),
        ]),
    ],
)
