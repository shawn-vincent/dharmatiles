# water+grass.tile.py
#
# Top half: grass meadow at ground level (5 mm).
# Bottom half: flat water pool (3 mm) — blue placeholder surface.
# Transition: organic shoreline with a 2.5 mm dirt slope between the two levels.

from dharmatiles.core.spec import (
    TileSpec, RegionSpec, LayerSpec, BoundarySpec, BoundaryLayerSpec, SurfaceConfig,
)

tile = TileSpec(
    surface=SurfaceConfig(seed=97),
    sizes=[(1, 1)],
    regions=[
        RegionSpec(
            id='meadow',
            contains=(0.5, 0.75),
            layers=[
                LayerSpec(type='grass_carpet'),
                LayerSpec(type='grass', params=dict(
                    groups_per_square=2,
                    blade_length_min=10,
                    blade_length_max=10,
                )),
            ],
        ),
        RegionSpec(
            id='pool',
            contains=(0.5, 0.25),
            height_mm=3.0,
            layers=[
                LayerSpec(type='water'),
                LayerSpec(type='rocks', params=dict(
                    rocks_per_square=2,
                    r_min=3.0,
                    r_max=5.0,
                    flat_min=2.0,
                    flat_max=2.8,
                    size_power=1.0,
                    n_cuts=3,
                )),
            ],
        ),
    ],
    boundaries=[
        BoundarySpec(
            id='shoreline',
            from_anchor=('left', 0.48),
            to_anchor=('right', 0.52),
            path='organic',
            amplitude_mm=5.0,
            wavelength_mm=12.0,
            width_mm=2.5,
            layers=[
                BoundaryLayerSpec(type='soil_carpet'),
                BoundaryLayerSpec(type='rocks', params=dict(
                    rocks_per_square=60,
                    r_min=0.8,
                    r_max=2.2,
                    size_power=1.5,
                )),
            ],
        ),
    ],
)
