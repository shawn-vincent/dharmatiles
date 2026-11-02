# walls-e4-meadow.tile.py — walls campaign: full-tile composition.
#
# A DB-standard wall along the north edge of a grassy 2x1 with scatter
# stones — judged for seating (soil at the wall base), grass steering
# around the footprint, and the wall reading alongside the shipped
# grass/stone systems.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig, \
    SpeciesConfig
from dharmatiles.layers import SoilCarpet, GrassCarpet
from dharmatiles.scatter import Grass
from dharmatiles.scatter.stones import StoneField
from dharmatiles.walls import CutStoneWall

species = SpeciesConfig()
tile = Tile(
    surface=SurfaceConfig(seed=21, cols=2, rows=1),
    areas=[
        Region(id='meadow', selector=FloodFill(0.5, 0.5), layers=[
            SoilCarpet(),
            GrassCarpet(species=species),
            CutStoneWall(spine=[(2.0, 1.0), (0.0, 1.0)], seed=9),
            StoneField(),
            Grass(species=species),
        ]),
    ],
)
