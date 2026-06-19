# water+grass.tile.py
#
# Top half: grass meadow at ground level (5 mm).
# Bottom half: flat water pool (3 mm) — blue placeholder surface.
# Transition: organic shoreline with a 2.5 mm dirt slope between the two levels.
#
# Region/boundary order matters globally: layers run in spec order, so
# anything that should steer 3D grass blades must appear before the region
# that grows the grass.  Pool comes before meadow so its rocks are stamped
# into terrain_support_z before meadow blades plan their growth.

from dharmatiles.spec import Tile, Region, Boundary, Edge, FloodFill, SurfaceConfig, SpeciesConfig, D
from dharmatiles.layers import SoilCarpet, GrassCarpet, Water
from dharmatiles.scatter import Rocks, Grass, Grouped, Uniform

tile = Tile(
    surface=SurfaceConfig(seed=97),
    areas=[
        Region(
            id='pool',
            selector=FloodFill(0.5, 0.25),
            height_mm=3.0,
            layers=[
                Rocks(
                    placement=Uniform(count_per_square=2),
                    r=D[3.0:5.0],
                    flat=D[1.725:1.86],
                    n_cuts=3,
                ),
                Water(embed_mm=2.5),
            ],
        ),
        Boundary(
            id='shoreline',
            from_anchor=Edge.LEFT(0.48),
            to_anchor=Edge.RIGHT(0.52),
            path='organic',
            amplitude_mm=5.0,
            wavelength_mm=12.0,
            width_mm=2.5,
            layers=[
                SoilCarpet(),
                Rocks(
                    placement=Uniform(count_per_square=60),
                    r=D[0.8:2.2].power(1.5),
                ),
            ],
        ),
        Region(
            id='meadow',
            selector=FloodFill(0.5, 0.75),
            layers=[
                GrassCarpet(placement=Grouped(groups_per_square=2)),
                Grass(
                    species=SpeciesConfig(
                        blade_length=10,
                    ),
                    placement=Grouped(groups_per_square=2),
                ),
            ],
        ),
    ],
)
