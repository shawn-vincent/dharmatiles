# grass-carpet.tile.py
#
# Full-square grass carpet only — no 3D blades, no soil texture.
# Use this to inspect the embossed 2D grass-carpet layer in isolation.

from dharmatiles.spec import Tile, Region, SurfaceConfig
from dharmatiles.layers import GrassCarpetLayer

tile = Tile(
    surface=SurfaceConfig(seed=42),
    regions=[
        Region(
            id='meadow',
            contains=(0.5, 0.5),
            layers=[
                GrassCarpetLayer(),
            ],
        ),
    ],
)
