# walls-e11-floor.tile.py — walls campaign: stone slab flooring (E32).
#
# A 2x2 courtyard: slab pavement gridding the ground (one slab per DB
# square), walled on the north edge.  The floor IS the terrain (soil
# drops to a thin dirt bed; slabs run full depth from the tile bottom);
# cracked slabs use the standard rock crack (stone/cracks.py).

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import CutStoneWall, StoneFloor

tile = Tile(
    surface=SurfaceConfig(seed=41, cols=2, rows=2),
    areas=[
        Region(id='court', selector=FloodFill(0.5, 0.3), layers=[
            SoilCarpet(),
            # Wall first: it seats on the original soil; the floor then
            # drops the soil to the dirt bed and its slabs lap the wall
            # base — pavement laid against a standing wall.
            CutStoneWall(spine=[(2.0, 2.0), (0.0, 2.0)], seed=92),
            StoneFloor(texture='chipped', seed=91, crack_prob=0.6),
        ]),
    ],
)
