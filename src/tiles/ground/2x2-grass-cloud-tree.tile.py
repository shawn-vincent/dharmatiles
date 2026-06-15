"""2x2 grass meadow with one CloudTree (point-cloud-partitioned growth, Bezier tubes).

The tree uses attractor groups (group_width_mm / group_height_mm) so the coarse
branching structure is spatially constrained: attractors are pre-partitioned into
~6 Voronoi clusters around the crown, forcing one main branch per cluster before
the fine organic splitting takes over within each cluster.
"""

from dharmatiles.spec import Tile, Region, FloodFill, SurfaceConfig, SpeciesConfig, D
from dharmatiles.layers import GrassCarpet, Scatter
from dharmatiles.scatter import CloudTree, Grouped, Uniform

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
                Scatter(
                    CloudTree(
                        height_mm=40.0,
                        trunk_height_mm=20.0,
                        crown_radius_mm=15.0,
                        crown_base_radius_mm=14.0,
                        top_pointiness=0.0,
                        placement=Uniform(count_per_square=0.25),
                        debug_attractors=True,
                        # Attractor groups: ~6 spatial clusters around the crown
                        # (n_around = round(2π·18/20) = 6, n_tall = round(20/20) = 1).
                        # Each cluster becomes one main branch; fine structure
                        # within each cluster uses the organic PCA splitting.
                        group_width_mm=20.0,
                        group_height_mm=20.0,
                        foliage_bulge_mm=6.0,
                        branchiness=0.5,
                    ),
                ),
            ],
        ),
    ],
)
