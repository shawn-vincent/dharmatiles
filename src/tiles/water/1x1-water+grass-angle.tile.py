# water+grass-angle.tile.py
#
# Grass covers two adjacent sides (L-shape, most of the tile); a shallow
# pool sits in the bottom-left corner — a marshy tarn at the edge of a meadow.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, FlatHeight, SurfaceConfig, SpeciesConfig, D
from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter, Water
from dharmatiles.scatter import Rocks, Grass, Grouped, Uniform

_species = SpeciesConfig()

tile = Tile(
    surface=SurfaceConfig(seed=61),
    areas=[
        Region(
            id='pool',
            selector=FloodFill(0.15, 0.15),
            terrain=FlatHeight(3.0),
            layers=[
                Scatter(
                    Rocks(
                        placement=Uniform(count_per_square=3),
                        r=D[2.0:4.0],
                        flat=D[1.5:2.5],
                        n_cuts=2,
                    ),
                ),
                Water(embed_mm=2.5),
            ],
        ),
        Region(
            id='meadow',
            selector=FloodFill(0.75, 0.75),
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
