# walls-e13-openings.tile.py — walls campaign: openings (O1-O3).
#
# Three walls, three families, every opening kind (design:
# docs/design/walls-doors.md):
#   south: cut stone — arched doorway + round OCULUS window
#   mid:   brick — segmental-arch doorway + flat-lintel window
#   north: fieldstone LOW wall (16mm) with a full-height arch rising
#          ABOVE the wall top ("low walls imply tall walls")

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import BrickWall, CutStoneWall, FieldstoneWall
from dharmatiles.walls.openings import Opening

_X0, _X1 = 1.9, 0.1

tile = Tile(
    surface=SurfaceConfig(seed=61, cols=2, rows=2),
    areas=[
        Region(id='ground', selector=FloodFill(0.5, 0.05), layers=[
            SoilCarpet(),
            CutStoneWall([(_X0, 0.5), (_X1, 0.5)], seed=101, openings=[
                Opening(at=0.55, width_mm=22.0, head_mm=30.0),
                Opening(at=1.45, profile='circle', width_mm=13.0,
                        sill_mm=15.0, head_mm=28.0),
            ]),
            BrickWall([(_X0, 1.2), (_X1, 1.2)], seed=102, openings=[
                Opening(at=0.55, width_mm=20.0, head_mm=28.0,
                        rise_mm=5.0),
                Opening(at=1.45, width_mm=12.0, sill_mm=12.0,
                        head_mm=24.0, head='lintel'),
            ]),
            FieldstoneWall([(_X0, 1.9), (_X1, 1.9)], seed=103,
                           height_mm=16.0, openings=[
                Opening(at=1.0, width_mm=22.0, head_mm=36.0),
            ]),
        ]),
    ],
)
