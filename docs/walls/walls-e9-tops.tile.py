# walls-e9-tops.tile.py — walls campaign: wall tops (E29/E30).
#
# Two parallel runs on a 2x2: a fieldstone wall finished with a
# vertical coping course (thin stones on edge, leaning together —
# drystone refs 02/05) and a crenellated cut-stone wall (merlon/crenel
# parapet, corner merlons — the city-wall archetype).

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import CutStoneWall, FieldstoneWall

_X0, _X1 = 1.85, 0.15          # walk -x so the body (left side) faces south

tile = Tile(
    surface=SurfaceConfig(seed=35, cols=2, rows=2),
    areas=[
        Region(id='ground', selector=FloodFill(0.5, 0.05), layers=[
            SoilCarpet(),
            FieldstoneWall([(_X0, 0.60), (_X1, 0.60)], height_mm=38.0,
                           seed=71, coping='vertical'),
            CutStoneWall([(_X0, 1.55), (_X1, 1.55)], height_mm=42.0,
                         seed=72, crenellated=True),
        ]),
    ],
)
