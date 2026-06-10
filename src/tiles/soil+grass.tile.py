# soil+grass.tile.py
#
# Left half: grass.  Right half: bare soil.
# The organic boundary creates a natural meadow margin.
#
# Grass region:  grass_carpet (embossed 2D texture) + grass (3D blades).
# Soil region:   soil_carpet (blob texture), masked to this region only.
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
                # grass_carpet: dense 2D footprint field (10× the 3D count).
                # Carpet and 3D seeds are placed independently (different RNG),
                # so their positions don't align — that's intentional.  The
                # carpet gives the impression of a trampled ground; the 3D blades
                # stand up through it at sparser intervals.
                LayerSpec(type='grass_carpet', params=dict(groups_per_square=240)),
                LayerSpec(type='grass', params=dict(groups_per_square=24)),
            ],
        ),
        RegionSpec(
            id='dirt',
            contains=(0.75, 0.5),
            layers=[
                LayerSpec(type='soil_carpet'),
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
