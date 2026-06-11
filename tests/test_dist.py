import unittest

import numpy as np

from dharmatiles.core.config import RocksConfig, SpeciesConfig
from dharmatiles.dist import D, bounds, sample


class DistributionTests(unittest.TestCase):
    def test_fixed_uniform_power_and_weighted_syntax(self):
        rng = np.random.default_rng(1)

        self.assertEqual(sample(1.0, rng), 1.0)
        self.assertEqual(bounds(D[0.8:2.2]), (0.8, 2.2))
        self.assertEqual(bounds(D[0.8:2.2].power(1.5)), (0.8, 2.2))
        self.assertEqual(bounds(D[5:2, 10:1]), (5, 10))

    def test_legacy_species_ranges_coerce_to_distribution(self):
        species = SpeciesConfig(blade_length_min=10, blade_length_max=15)

        self.assertEqual(bounds(species.blade_length), (10.0, 15.0))

    def test_legacy_rock_ranges_coerce_to_distribution(self):
        rocks = RocksConfig(r_min=0.8, r_max=2.2, size_power=1.5)

        self.assertEqual(bounds(rocks.r), (0.8, 2.2))


if __name__ == "__main__":
    unittest.main()
