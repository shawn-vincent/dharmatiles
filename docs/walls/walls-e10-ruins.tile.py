# walls-e10-ruins.tile.py — walls campaign: ruin states (E31).
#
# Two ruined runs on a 2x2 (hadrians-coursed-rubble.jpg): a ruined
# drystone fieldstone wall and a ruined mortared cut-stone wall.  The
# break line is a slow per-segment height envelope; straddling blocks
# survive at random (ragged steps); the mortar core stays below the
# lowest break so the exposed band shows packed rubble hearting; shed
# shards collect at the foot.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import CutStoneWall, FieldstoneWall

_X0, _X1 = 1.85, 0.15

tile = Tile(
    surface=SurfaceConfig(seed=35, cols=2, rows=2),
    areas=[
        Region(id='ground', selector=FloodFill(0.5, 0.05), layers=[
            SoilCarpet(),
            FieldstoneWall([(_X0, 0.60), (_X1, 0.60)], height_mm=42.0,
                           seed=81, ruin=0.55),
            CutStoneWall([(_X0, 1.55), (_X1, 1.55)], height_mm=49.7,
                         seed=82, ruin=0.6),
        ]),
    ],
)
