from __future__ import annotations

import unittest

from veoveo_uav_sim.cesium_camera import camera_frustum_at_transform


class _Camera:
    transform = None
    frustum = object()


class _UsdCamera:
    def __init__(self) -> None:
        self.camera = _Camera()

    def GetCamera(self, time_code: object) -> _Camera:
        del time_code
        return self.camera


class CesiumCameraTests(unittest.TestCase):
    def test_frustum_uses_explicit_authoritative_camera_transform(self) -> None:
        usd_camera = _UsdCamera()
        time_code = object()
        transform = object()

        self.assertIs(
            camera_frustum_at_transform(usd_camera, time_code, transform),
            usd_camera.camera.frustum,
        )
        self.assertIs(usd_camera.camera.transform, transform)


if __name__ == "__main__":
    unittest.main()
