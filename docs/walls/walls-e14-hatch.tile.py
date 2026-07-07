# walls-e14-hatch.tile.py — walls campaign: openings on a laid_flat
# wall (O4).  The same Opening on a floor is a HATCH: a gap in the
# pavement with a slab surround.  The circle profile becomes a WELL —
# a full voussoir ring in the pavement (design: docs/design/
# walls-doors.md; "round holes in the floor").
#
# Floor plan: spine walks the south edge +x, so `at` is the x position
# in squares and (sill, head) span the plan depth northward.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import Leaf, StoneFloor
from dharmatiles.walls.openings import Opening

tile = Tile(
    surface=SurfaceConfig(seed=71, cols=2, rows=2),
    areas=[
        Region(id='court', selector=FloodFill(0.5, 0.3), layers=[
            SoilCarpet(),
            StoneFloor(texture='dressed', seed=93, crack_prob=0.3,
                       missing_prob=0.0, openings=[
                # square hatch: slab surround all round, CLOSED
                # planked trapdoor lid with a ring (O5)
                Opening(at=0.55, width_mm=18.0, sill_mm=9.0,
                        head_mm=27.0, head='lintel',
                        leaf=Leaf('trapdoor')),
                # round well: full voussoir ring
                Opening(at=1.45, profile='circle', width_mm=15.0,
                        sill_mm=37.0, head_mm=53.0),
            ]),
        ]),
    ],
)
