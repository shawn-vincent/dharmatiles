# 1x1-wall-fieldstone.tile.py
#
# Drystone fieldstone wall run along the north edge of a grassy 1x1.
# Wall body extends to the LEFT of the spine walk (into the tile);
# outer face flush to the tile edge — the DungeonBlocks slab standard.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig, \
    SpeciesConfig
from dharmatiles.layers import SoilCarpet, GrassCarpet
from dharmatiles.scatter import Grass
from dharmatiles.walls import FieldstoneWall

_species = SpeciesConfig()

tile = Tile(
    surface=SurfaceConfig(seed=63),
    areas=[
        Region(id='meadow', selector=FloodFill(0.5, 0.35), layers=[
            SoilCarpet(),
            GrassCarpet(species=_species),
            FieldstoneWall(spine=[(1.0, 1.0), (0.0, 1.0)], seed=13),
            Grass(species=_species),
        ]),
    ],
)
