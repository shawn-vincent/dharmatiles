# grass.tile.py
#
# One full 1×1 grass region. Grass carpet provides embossed ground texture.
#
# A single SpeciesConfig drives both the 2D carpet stamps and the 3D blades,
# guaranteeing the two passes use identical blade geometry.

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import GrassCarpet, Scatter
from dharmatiles.scatter import Grass, Grouped

_meadow = SpeciesConfig(
    group_dir_jitter=0,
    blade_length_min=15,
    blade_length_max=15,
    blade_curl_min=0.5,
    blade_curl_max=0.8,
    blade_clearance=0.2,
)

_placement_carpet = Grouped(groups_per_square=3)
_placement_blades = Grouped(groups_per_square=3)

tile = Tile(
    surface=SurfaceConfig(seed=1),
    areas=[
        Region(
            id='meadow',
            selector=FloodFill(0.5, 0.5),
            layers=[
                GrassCarpet(species=_meadow, placement=_placement_carpet),
                Scatter(
                    Grass(species=_meadow, placement=_placement_blades),
                ),
            ],
        ),
    ],
)
