# water+grass-corner.tile.py
#
# Water fills most of the tile; a grass meadow grows in the bottom-left
# corner — like a small islet or mossy riverbank.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, SpeciesConfig, D
from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter, Water
from dharmatiles.scatter import Rocks, Grass, Grouped, Uniform

_species = SpeciesConfig()

tile = Tile(
    surface=SurfaceConfig(seed=60),
    areas=[
        Region(
            id='pool',
            selector=FloodFill(0.75, 0.75),
            height_mm=3.0,
            layers=[
                Scatter(
                    Rocks(
                        placement=Uniform(count_per_square=2),
                        r=D[3.0:5.0],
                        flat=D[1.725:1.86],
                        n_cuts=3,
                    ),
                ),
                Water(embed_mm=2.5),
            ],
        ),
        Region(
            id='islet',
            selector=FloodFill(0.15, 0.15),
            layers=[
                GrassCarpet(species=_species, placement=Grouped(groups_per_square=240)),
                Scatter(Grass(species=_species, placement=Grouped(groups_per_square=24))),
            ],
        ),
        Boundary(
            id='shoreline',
            from_anchor=Edge.LEFT(0.5),
            to_anchor=Edge.BOTTOM(0.5),
            path='organic',
            amplitude_mm=2.5,
            wavelength_mm=8.0,
            width_mm=2.5,
            layers=[
                SoilCarpet(),
                Scatter(
                    Rocks(
                        placement=Uniform(count_per_square=60),
                        r=D[0.8:2.2].power(1.5),
                    ),
                ),
            ],
        ),
    ],
)
