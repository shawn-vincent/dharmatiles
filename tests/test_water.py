import unittest

import numpy as np

from dharmatiles.layers.water import WATER_RENDER_LIFT_MM, make_water_volume


class WaterVolumeTests(unittest.TestCase):
    def _top_z(
        self,
        *,
        edge_contacts: tuple[bool, bool, bool, bool],
        water_height: float = 3.0,
        cells: int = 35,
        fade_mm: float = 5.0,
        radius_mm: float | None = None,
        z_disp: np.ndarray | None = None,
    ) -> np.ndarray:
        terrain_z = np.zeros((cells, cells), dtype=float)
        water_mask = np.ones_like(terrain_z, dtype=bool)
        mesh = make_water_volume(
            terrain_z,
            water_mask,
            water_height,
            tile_w=35.0,
            tile_h=35.0,
            z_disp=z_disp,
            tile_edge_profile_edges=edge_contacts,
            tile_edge_profile_square_mm=35.0,
            tile_edge_profile_wavelength_mm=17.5,
            tile_edge_profile_fade_mm=fade_mm,
            tile_edge_profile_radius_mm=radius_mm,
        )
        return mesh.vertices[:(cells + 1) * (cells + 1), 2].reshape(cells + 1, cells + 1)

    def test_tile_edge_profile_forces_active_edge_to_shared_sine(self):
        top_z = self._top_z(edge_contacts=(True, False, False, False))

        x = np.linspace(0.0, 35.0, 36)
        expected = 3.0 + np.sin(2.0 * np.pi * (x - 13.125) / 17.5)

        np.testing.assert_allclose(top_z[0, :], expected, atol=1e-9)

    def test_tile_edge_profile_stops_at_rounded_square_boundary(self):
        top_z = self._top_z(edge_contacts=(True, True, True, True))

        normal_surface = 3.0 + WATER_RENDER_LIFT_MM

        self.assertAlmostEqual(top_z[5, 10], normal_surface)
        self.assertAlmostEqual(top_z[10, 5], normal_surface)
        self.assertAlmostEqual(top_z[18, 18], normal_surface)

    def test_tile_edge_profile_rounds_the_corner_falloff(self):
        top_z = self._top_z(edge_contacts=(True, True, True, True))

        normal_surface = 3.0 + WATER_RENDER_LIFT_MM

        self.assertNotAlmostEqual(top_z[5, 5], normal_surface)
        self.assertAlmostEqual(top_z[6, 6], normal_surface)

    def test_tile_edge_profile_blends_from_raw_texture_to_sine(self):
        z_disp = np.full((35, 35), 0.8, dtype=float)
        top_z = self._top_z(
            edge_contacts=(True, False, False, False),
            z_disp=z_disp,
        )

        x = np.linspace(0.0, 35.0, 36)
        sine_edge = 3.0 + np.sin(2.0 * np.pi * (x - 13.125) / 17.5)
        raw_surface = 3.0 + WATER_RENDER_LIFT_MM + 0.8

        np.testing.assert_allclose(top_z[0, :], sine_edge, atol=1e-9)
        self.assertAlmostEqual(top_z[5, 10], raw_surface)
        self.assertGreater(top_z[4, 10], min(raw_surface, sine_edge[10]))
        self.assertLess(top_z[4, 10], max(raw_surface, sine_edge[10]))

    def test_inactive_tile_edges_are_not_forced(self):
        top_z = self._top_z(edge_contacts=(True, False, False, False))

        normal_surface = 3.0 + WATER_RENDER_LIFT_MM

        np.testing.assert_allclose(top_z[-1, :], normal_surface, atol=1e-9)


if __name__ == "__main__":
    unittest.main()
