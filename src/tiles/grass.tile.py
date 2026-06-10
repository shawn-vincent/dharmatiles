# grass.tile.py
#
# One full 1×1 grass region. Grass carpet provides embossed ground texture.
#
# A single SpeciesConfig drives both the 2D carpet stamps and the 3D blades,
# guaranteeing the two passes use identical blade geometry.

from dharmatiles.core.spec import (
    TileSpec, RegionSpec, LayerSpec, SurfaceConfig, SpeciesConfig,
)

# Non-default blade geometry for this meadow.
# All omitted fields use SpeciesConfig defaults.
_meadow = SpeciesConfig(
    groups_per_square=3,
    group_dir_jitter=0,
    blade_length_min=15,
    blade_length_max=15,
    blade_curl_min=0.5,
    blade_curl_max=0.8,
    blade_clearance=0.2,
)

tile = TileSpec(
    surface=SurfaceConfig(seed=1),
    sizes=[(1, 1)],
    regions=[
        RegionSpec(
            id='meadow',
            contains=(0.5, 0.5),
            layers=[
                LayerSpec(type='grass_carpet', params=dict(species=_meadow)),
                LayerSpec(type='grass',          params=dict(species=_meadow)),
            ],
        ),
    ],
)
