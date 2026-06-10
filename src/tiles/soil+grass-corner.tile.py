# soil+grass-corner.tile.py
#
# A patch of grass in the bottom-left corner; the rest is bare soil.
# Works as a transition tile where a meadow ends at a path corner.

from dharmatiles.spec import Tile, Region, Boundary, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter
from dharmatiles.scatter import Grass

tile = Tile(
    surface=SurfaceConfig(cols=1, rows=1, seed=99),
    regions=[
        Region(
            id='patch',
            contains=(0.15, 0.15),
            layers=[
                GrassCarpet(),
                Scatter(
                    Grass(species=SpeciesConfig(groups_per_square=240)),
                ),
            ],
        ),
        Region(
            id='floor',
            contains=(0.75, 0.75),
            layers=[
                SoilCarpet(),
            ],
        ),
    ],
    boundaries=[
        Boundary(
            id='corner-cut',
            from_anchor=('left', 0.5),
            to_anchor=('bottom', 0.5),
            path='organic',
            amplitude_mm=2.5,
            wavelength_mm=8.0,
        ),
    ],
)
