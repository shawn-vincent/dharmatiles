# walls-e12-drystone-floor.tile.py — walls campaign: drystone floor.
#
# The wall/floor unification payoff (E57): a 2x2 courtyard paved with
# FIELDSTONE flagstones — literally FieldstoneWall(laid_flat=True):
# crack-network tessellated stones as pavement rows, proud faces as
# per-flagstone height variation, rubble hearting as chinking in the
# joints — with a standing fieldstone wall on the north edge seating
# ON the pavement.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig
from dharmatiles.layers import SoilCarpet
from dharmatiles.walls import FieldstoneWall

tile = Tile(
    surface=SurfaceConfig(seed=57, cols=2, rows=2),
    areas=[
        Region(id='court', selector=FloodFill(0.5, 0.5), layers=[
            SoilCarpet(),
            # Stone sizes = the fieldstone WALL defaults (Shawn:
            # big flagstones looked weird; the floor should read like
            # the wall's stones, laid flat).
            FieldstoneWall(spine=[(0.0, 0.0), (2.0, 0.0)],
                           laid_flat=True,
                           height_mm=70.0, thickness_mm=7.4,
                           seed=95),
            FieldstoneWall(spine=[(2.0, 2.0), (0.0, 2.0)], seed=96),
        ]),
    ],
)
