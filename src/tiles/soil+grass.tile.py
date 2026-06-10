# soil+grass.tile.py
#
# Left half: grass.  Right half: bare soil.
# The organic boundary creates a natural meadow margin.

from dataclasses import replace
from dharmatiles.spec import Tile, Region, Boundary, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter
from dharmatiles.scatter import Grass

_species = SpeciesConfig()

tile = Tile(
    surface=SurfaceConfig(seed=42),
    regions=[
        Region(
            id='meadow',
            contains=(0.25, 0.5),
            layers=[
                GrassCarpet(species=replace(_species, groups_per_square=240)),
                Scatter(
                    Grass(species=replace(_species, groups_per_square=24)),
                ),
            ],
        ),
        Region(
            id='dirt',
            contains=(0.75, 0.5),
            layers=[
                SoilCarpet(),
            ],
        ),
    ],
    boundaries=[
        Boundary(
            id='margin',
            from_anchor=('top', 0.48),
            to_anchor=('bottom', 0.52),
            path='organic',
            amplitude_mm=5.0,
            wavelength_mm=10.0,
        ),
    ],
)
