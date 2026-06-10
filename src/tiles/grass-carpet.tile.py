# grass-carpet.tile.py
#
# Full-square grass carpet only — no 3D blades, no soil texture.
# Use this to inspect the embossed 2D grass-carpet layer in isolation.

from dharmatiles.core.spec import TileSpec, RegionSpec, LayerSpec, SurfaceConfig

tile = TileSpec(
    surface=SurfaceConfig(seed=42),
    regions=[
        RegionSpec(
            id='meadow',
            contains=(0.5, 0.5),
            layers=[
                LayerSpec(type='grass_carpet', params=dict(groups_per_square=240)),
            ],
        ),
    ],
)
