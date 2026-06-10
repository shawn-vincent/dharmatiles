# soil+grass.tile.py
#
# Left half: grass.  Right half: bare soil.
# The organic boundary creates a natural meadow margin.
#
# Grass region:  grass_underlay (embossed 2D texture) + grass (3D blades).
# Soil region:   soil (blob texture), masked to this region only.
# Boundary:      zero-width (same elevation both sides — no slope needed).

from dharmatiles.core.spec import (
    TileSpec, RegionSpec, LayerSpec, BoundarySpec, SurfaceConfig,
)

tile = TileSpec(
    surface=SurfaceConfig(seed=42),
    regions=[
        RegionSpec(
            id='meadow',
            contains=(0.25, 0.5),
            layers=[
                LayerSpec(type='grass_underlay', params=dict(groups_per_square=240)),
                LayerSpec(type='grass', params=dict(groups_per_square=24)),
            ],
        ),
        RegionSpec(
            id='dirt',
            contains=(0.75, 0.5),
            layers=[
                LayerSpec(type='soil'),
            ],
        ),
    ],
    boundaries=[
        BoundarySpec(
            id='margin',
            from_anchor=('top', 0.48),
            to_anchor=('bottom', 0.52),
            path='organic',
            amplitude_mm=5.0,
            wavelength_mm=10.0,
        ),
    ],
)
