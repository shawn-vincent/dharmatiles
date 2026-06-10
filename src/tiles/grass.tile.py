# grass.tile.py
#
# One full 1×1 grass region. Grass underlay provides embossed ground texture.

from dharmatiles.core.spec import (
    TileSpec, RegionSpec, LayerSpec, SurfaceConfig,
)

tile = TileSpec(
    surface=SurfaceConfig(seed=1),
    sizes=[(1, 1)],
    regions=[
        RegionSpec(
            id='meadow',
            contains=(0.5, 0.5),
            layers=[
                LayerSpec(type='grass_underlay'),
                LayerSpec(type='grass', params=dict(
                    groups_per_square=3,
                    group_dir_jitter=0,
                    blade_width_min=1.2,
                    blade_width_max=1.2,
                    blade_length_min=15,
                    blade_length_max=15,
                    blade_segment_length=0.5,
                    blade_curl_min=0.5,
                    blade_curl_max=0.8,
                    blade_smooth=0.9,
                    blade_rise_cap=2.0,
                    blade_clearance=0.2,
                    blade_top_facets=6,
                    blade_thickness=0.6,
                    blade_taper=1,
                    blade_base_width=1.0,
                    blade_base_taper=0,
                    keel_fraction=0.6,
                )),
            ],
        ),
    ],
)
