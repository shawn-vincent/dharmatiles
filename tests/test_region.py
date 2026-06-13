import unittest

import numpy as np

from dharmatiles.core.region import boundary_path_mm, build_region_mask
from dharmatiles.spec import Boundary, Edge, FloodFill, Region, SurfaceConfig, Tile


class BoundaryPathTests(unittest.TestCase):
    def test_organic_boundary_passes_through_waypoint(self):
        surface = SurfaceConfig(square_mm=10.0, cells_per_square=32, seed=1)
        boundary = Boundary(
            id="bend",
            from_anchor=Edge.LEFT(0.5),
            to_anchor=Edge.RIGHT(0.5),
            waypoints=[(0.5, 0.5)],
            path="organic",
            amplitude_mm=2.0,
        )

        path = boundary_path_mm(boundary, surface, n_samples=3)

        np.testing.assert_allclose(path[0], [0.0, 5.0], atol=1e-9)
        np.testing.assert_allclose(path[1], [5.0, 5.0], atol=1e-9)
        np.testing.assert_allclose(path[2], [10.0, 5.0], atol=1e-9)


class SameSideBoundaryMaskTests(unittest.TestCase):
    def test_same_side_boundary_carves_region_between_path_and_edge(self):
        surface = SurfaceConfig(square_mm=10.0, cells_per_square=64, seed=2)
        tile = Tile(
            surface=surface,
            areas=[
                Region(id="pocket", selector=FloodFill(0.85, 0.5)),
                Region(id="surrounding", selector=FloodFill(0.2, 0.5)),
                Boundary(
                    id="loop",
                    from_anchor=Edge.RIGHT(0.2),
                    to_anchor=Edge.RIGHT(0.8),
                    waypoints=[
                        (0.55, 0.2),
                        (0.25, 0.4),
                        (0.30, 0.6),
                        (0.55, 0.8),
                    ],
                    path="straight",
                ),
            ],
        )

        mask = build_region_mask(tile)

        self.assertEqual(mask[int(0.5 * surface.grid_h), int(0.85 * surface.grid_w)], 0)
        self.assertEqual(mask[int(0.5 * surface.grid_h), int(0.2 * surface.grid_w)], 1)
        self.assertIn(-1, set(np.unique(mask)))


if __name__ == "__main__":
    unittest.main()
