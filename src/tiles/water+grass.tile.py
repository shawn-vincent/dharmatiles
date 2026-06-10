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

from dharmatiles.spec import Tile, Region, Boundary, SurfaceConfig, SpeciesConfig
from dharmatiles.layers import SoilCarpet, GrassCarpet, Scatter, Water
from dharmatiles.scatter import Rocks, Grass

tile = Tile(
    surface=SurfaceConfig(seed=97),
    regions=[
        Region(
            id='pool',
            contains=(0.5, 0.25),
            height_mm=3.0,
            layers=[
                Scatter(
                    Rocks(
                        rocks_per_square=2,
                        r_min=3.0,
                        r_max=5.0,
                        flat_min=2.0,
                        flat_max=2.8,
                        size_power=1.0,
                        n_cuts=3,
                    ),
                ),
                Water(embed_mm=2.5),
            ],
        ),
        Region(
            id='meadow',
            contains=(0.5, 0.75),
            layers=[
                GrassCarpet(),
                Scatter(
                    Grass(species=SpeciesConfig(
                        groups_per_square=2,
                        blade_length_min=10,
                        blade_length_max=10,
                    )),
                ),
            ],
        ),
    ],
    boundaries=[
        Boundary(
            id='shoreline',
            from_anchor=('left', 0.48),
            to_anchor=('right', 0.52),
            path='organic',
            amplitude_mm=5.0,
            wavelength_mm=12.0,
            width_mm=2.5,
            layers=[
                SoilCarpet(),
                Scatter(
                    Rocks(
                        rocks_per_square=60,
                        r_min=0.8,
                        r_max=2.2,
                        size_power=1.5,
                    ),
                ),
            ],
        ),
    ],
)
