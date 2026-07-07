# walls-e15-leaves.tile.py — walls campaign: integrated leaves (O5).
#
# Door leaf / window leaf / gate across the families (design:
# docs/design/walls-doors.md, "Leaves"):
#   south: cut stone — arched doorway with CLOSED planks door +
#          lintel window with BARS (prison grille)
#   mid:   brick — segmental-arch doorway with planks door OPEN at
#          65° (ref-06's red door) + lintel window with SHUTTERS
#   north: fieldstone LOW wall, full-height arch, planks gate open
#          75° — the walled-garden-gate read
#
# The trapdoor leaf is demoed on walls-e14-hatch.tile.py.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import (BrickWall, CutStoneWall, FieldstoneWall,
                               Leaf, Opening)

_X0, _X1 = 1.9, 0.1

tile = Tile(
    surface=SurfaceConfig(seed=62, cols=2, rows=2),
    areas=[
        Region(id='ground', selector=FloodFill(0.5, 0.05), layers=[
            SoilCarpet(),
            CutStoneWall([(_X0, 0.5), (_X1, 0.5)], seed=201, openings=[
                Opening(at=0.55, width_mm=22.0, head_mm=30.0,
                        leaf=Leaf('planks')),
                Opening(at=1.45, width_mm=12.0, sill_mm=12.0,
                        head_mm=26.0, head='lintel', leaf=Leaf('bars')),
            ]),
            BrickWall([(_X0, 1.2), (_X1, 1.2)], seed=202, openings=[
                Opening(at=0.55, width_mm=20.0, head_mm=28.0,
                        rise_mm=5.0,
                        leaf=Leaf('planks', open_deg=65.0, hinge='left')),
                Opening(at=1.45, width_mm=12.0, sill_mm=12.0,
                        head_mm=24.0, head='lintel',
                        leaf=Leaf('shutters')),
            ]),
            FieldstoneWall([(_X0, 1.9), (_X1, 1.9)], seed=203,
                           height_mm=16.0, openings=[
                Opening(at=1.0, width_mm=22.0, head_mm=36.0,
                        leaf=Leaf('planks', open_deg=75.0,
                                  hinge='right')),
            ]),
        ]),
    ],
)
