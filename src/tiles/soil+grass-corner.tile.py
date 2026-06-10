# soil+grass-corner.tile.py
#
# A patch of grass in the bottom-left corner; the rest is bare soil.
# Works as a transition tile where a meadow ends at a path corner.

from dharmatiles.core.spec import (
    TileSpec, RegionSpec, LayerSpec, BoundarySpec, SurfaceConfig,
)

tile = TileSpec(
    surface=SurfaceConfig(cols=1, rows=1, seed=99),
    regions=[
        RegionSpec(
            id='patch',
            contains=(0.15, 0.15),
            layers=[
                LayerSpec(type='grass_underlay'),
                LayerSpec(type='grass', params=dict(groups_per_square=240)),
            ],
        ),
        RegionSpec(
            id='floor',
            contains=(0.75, 0.75),
            layers=[
                LayerSpec(type='soil'),
            ],
        ),
    ],
    boundaries=[
        BoundarySpec(
            id='corner-cut',
            from_anchor=('left', 0.5),
            to_anchor=('bottom', 0.5),
            path='organic',
            amplitude_mm=2.5,
            wavelength_mm=8.0,
        ),
    ],
)
