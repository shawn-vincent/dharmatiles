"""2x2 grass meadow with one Tree (space-colonisation growth, Bezier tubes)."""

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig, SpeciesConfig, D
from dharmatiles.layers import GrassCarpet
from dharmatiles.scatter import Tree, Grouped, Uniform

_species = SpeciesConfig(
    blade_length=D[8:14],
    blade_curl=D[0.35:0.65],
    blade_clearance=0.2,
)

tile = Tile(
    surface=SurfaceConfig(cols=2, rows=2, seed=84),
    areas=[
        Region(
            id='meadow',
            selector=FloodFill(0.5, 0.5),
            layers=[
                GrassCarpet(
                    species=_species,
                    placement=Grouped(groups_per_square=3),
                ),
                Tree(
                    placement=Uniform(count_per_square=0.25),
                    debug_attractors=False,
                    foliage_clusters=True,
                    leaf_placement="organic",
                    n_attractors=32,
                    leaf_length_mm=4.5,
                    leaf_width_mm=3.0,
                    leaf_h_overlap=0.1,
                    leaf_v_overlap=0.25,
                    leaf_angle_jitter_deg=10.0,
                    leaf_pos_jitter=0.10,
                ),
            ],
        ),
    ],
)
