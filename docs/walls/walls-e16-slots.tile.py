# walls-e16-slots.tile.py — walls campaign: slot system (O6).
#
# `Opening(slot=True)` splits the surround front/back around a
# mid-thickness CHANNEL; the leaf drops in as a SEPARATE, removable
# solid (undersized by the slot clearance, so it never fuses to the
# masonry — a swappable object in the same STL, "separated by space").
# Design: docs/design/walls-doors.md, "Slot system".
#
#   south: cut stone full wall — arched doorway with a slotted PLANK
#          door sitting in its channel (swap it for anything).
#   north: fieldstone LOW wall (16mm) — a full-height arch rising
#          ABOVE the wall top with a slotted PORTCULLIS: the channel
#          mouth is proud of the top, so the iron gate drops straight
#          in (the marquee slot leaf; ref-08/09 portcullis grooves).

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import CutStoneWall, FieldstoneWall, Leaf, Opening

_X0, _X1 = 1.9, 0.1

tile = Tile(
    surface=SurfaceConfig(seed=63, cols=2, rows=2),
    areas=[
        Region(id='ground', selector=FloodFill(0.5, 0.05), layers=[
            SoilCarpet(),
            CutStoneWall([(_X0, 0.6), (_X1, 0.6)], seed=301, openings=[
                Opening(at=1.0, width_mm=22.0, head_mm=30.0,
                        slot=True, leaf=Leaf('planks')),
            ]),
            FieldstoneWall([(_X0, 1.6), (_X1, 1.6)], seed=303,
                           height_mm=16.0, openings=[
                Opening(at=1.0, width_mm=22.0, head_mm=36.0,
                        slot=True, leaf=Leaf('portcullis')),
            ]),
        ]),
    ],
)
