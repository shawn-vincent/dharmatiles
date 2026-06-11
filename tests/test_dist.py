import unittest

import numpy as np

from dharmatiles.core.config import RocksConfig, SoilConfig, SpeciesConfig
from dharmatiles.dist import D, bounds, sample
from dharmatiles.scatter.config import Grouped, Uniform
from dharmatiles.scatter.distribute import scaled_voronoi_group_count


class DistributionTests(unittest.TestCase):
    def test_fixed_uniform_power_and_weighted_syntax(self):
        rng = np.random.default_rng(1)

        self.assertEqual(sample(1.0, rng), 1.0)
        self.assertEqual(bounds(D[0.8:2.2]), (0.8, 2.2))
        self.assertEqual(bounds(D[0.8:2.2].power(1.5)), (0.8, 2.2))
        self.assertEqual(bounds(D[5:2, 10:1]), (5, 10))
        self.assertEqual(bounds(D.normal(0.0, 0.1, clamp=(-0.2, 0.2))), (-0.2, 0.2))

    def test_legacy_species_ranges_coerce_to_distribution(self):
        species = SpeciesConfig(blade_length_min=10, blade_length_max=15)

        self.assertEqual(bounds(species.blade_length), (10.0, 15.0))

    def test_legacy_rock_ranges_coerce_to_distribution(self):
        rocks = RocksConfig(r_min=0.8, r_max=2.2, size_power=1.5)

        self.assertEqual(bounds(rocks.r), (0.8, 2.2))

    def test_legacy_soil_ranges_coerce_to_distribution(self):
        soil = SoilConfig(
            blob_sigma_min_mm=0.1,
            blob_sigma_mode_mm=0.2,
            blob_sigma_max_mm=0.3,
            small_h_min=0.004,
            small_h_max=0.010,
        )

        self.assertEqual(bounds(soil.blob_sigma), (0.1, 0.3))
        self.assertEqual(bounds(soil.small_h), (0.004, 0.010))

    def test_placement_accepts_distributions(self):
        uniform = Uniform(count_per_square=D[4:4], gap_mm=D[0.1:0.2])
        grouped = Grouped(groups_per_square=D[2:2])

        self.assertEqual(bounds(uniform.gap_mm), (0.1, 0.2))
        surface = type("Surface", (), {"grid_w": 1, "grid_h": 1, "cols": 1, "rows": 1})()
        self.assertEqual(
            scaled_voronoi_group_count(grouped.groups_per_square, None, surface, np.random.default_rng(1)),
            2,
        )


if __name__ == "__main__":
    unittest.main()
