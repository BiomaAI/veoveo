from __future__ import annotations

import unittest

from veoveo_uav_sim.cesium_camera import CesiumCameraSpec


class CesiumCameraTests(unittest.TestCase):
    def test_camera_spec_requires_absolute_path_and_positive_dimensions(self) -> None:
        self.assertEqual(
            CesiumCameraSpec("/World/camera", 640, 480).camera_path,
            "/World/camera",
        )
        with self.assertRaisesRegex(ValueError, "absolute USD prim path"):
            CesiumCameraSpec("World/camera", 640, 480)
        with self.assertRaisesRegex(ValueError, "dimensions must be positive"):
            CesiumCameraSpec("/World/camera", 0, 480)


if __name__ == "__main__":
    unittest.main()
