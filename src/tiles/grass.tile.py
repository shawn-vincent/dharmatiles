# grass.tile.py
#
# One full 1×1 grass region. Grass carpet provides embossed ground texture.
#
# A single SpeciesConfig drives both the 2D carpet stamps and the 3D blades,
# guaranteeing the two passes use identical blade geometry.

from dharmatiles.spec import Tile, Region, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import GrassCarpetLayer, ScatterLayer
from dharmatiles.scatter import Grass

_meadow = SpeciesConfig(
    groups_per_square=3,
    group_dir_jitter=0,
    blade_length_min=15,
    blade_length_max=15,
    blade_curl_min=0.5,
    blade_curl_max=0.8,
    blade_clearance=0.2,
)

tile = Tile(
    surface=SurfaceConfig(seed=1),
    regions=[
        Region(
            id='meadow',
            contains=(0.5, 0.5),
            layers=[
                GrassCarpetLayer(species=_meadow),
                ScatterLayer(
                    Grass(species=_meadow),
                ),
            ],
        ),
    ],
)
