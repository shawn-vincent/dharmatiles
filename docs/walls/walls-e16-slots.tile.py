# walls-e16-slots.tile.py — walls campaign: slot system (O6).
#
# Opening(slot=True) is the swap game (ref-08/09 portcullis grooves).
# The wall is built NORMALLY, then a smooth SLOT is sliced out of it
# (a boolean, not a per-brick split); the leaf is modelled to tuck
# into that groove with clearance, so it sits in the slot as a
# separate, removable object in the same STL, "separated by space" —
# print it, drop it in, swap it.  Design: docs/design/walls-doors.md.
#
#   south: cut stone — a slotted PORTCULLIS (iron grid) in the arch.
#          A precision slot wants a dressed surround, so the ROCK gate
#          stays clear of the smooth-cut groove.
#   north: fieldstone LOW wall (16mm) — a full-height arch with a
#          slotted PLANK door.  A wood leaf is its own material group,
#          so it never fuses to the masonry even against rough stone.

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
                # head under the wall top so the arch stays connected
                Opening(at=1.0, width_mm=22.0, head_mm=26.0,
                        slot=True, leaf=Leaf('portcullis')),
            ]),
            FieldstoneWall([(_X0, 1.6), (_X1, 1.6)], seed=303,
                           height_mm=16.0, openings=[
                Opening(at=1.0, width_mm=22.0, head_mm=30.0,
                        slot=True, leaf=Leaf('planks')),
            ]),
        ]),
    ],
)
