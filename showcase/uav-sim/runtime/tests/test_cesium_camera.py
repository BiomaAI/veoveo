from __future__ import annotations

import unittest

from veoveo_uav_sim.cesium_camera import CesiumCameraSpec, camera_frustum


class _Camera:
    transform = None
    frustum = object()


class _UsdCamera:
    def __init__(self) -> None:
        self.camera = _Camera()

    def GetCamera(self, time_code: object) -> _Camera:
        del time_code
        return self.camera

    def ComputeLocalToWorldTransform(self, time_code: object) -> object:
        return time_code


class CesiumCameraTests(unittest.TestCase):
    def test_camera_frustum_uses_current_world_transform(self) -> None:
        usd_camera = _UsdCamera()
        time_code = object()

        self.assertIs(camera_frustum(usd_camera, time_code), usd_camera.camera.frustum)
        self.assertIs(usd_camera.camera.transform, time_code)

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
