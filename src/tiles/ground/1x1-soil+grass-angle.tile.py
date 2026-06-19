# soil+grass-angle.tile.py
#
# Grass covers two adjacent sides (L-shape, most of the tile); a patch of
# bare soil is tucked in the bottom-left corner.  Same diagonal boundary as
# soil+grass-corner, but terrain assignments are swapped — grass is the large
# L region, soil is the small corner.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import SoilCarpet, GrassCarpet
from dharmatiles.scatter import Grass, Grouped

_species = SpeciesConfig()

tile = Tile(
    surface=SurfaceConfig(seed=100),
    areas=[
        Region(
            id='dirt',
            selector=FloodFill(0.15, 0.15),
            layers=[SoilCarpet()],
        ),
        Region(
            id='meadow',
            selector=FloodFill(0.75, 0.75),
            layers=[
                GrassCarpet(species=_species, placement=Grouped(groups_per_square=240)),
                Grass(species=_species, placement=Grouped(groups_per_square=24)),
            ],
        ),
        Boundary(
            id='corner-cut',
            from_anchor=Edge.LEFT(0.5),
            to_anchor=Edge.BOTTOM(0.5),
            path='organic',
            amplitude_mm=2.5,
            wavelength_mm=8.0,
        ),
    ],
)
