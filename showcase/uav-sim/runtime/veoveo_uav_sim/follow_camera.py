from __future__ import annotations

from collections.abc import Sequence
from typing import Any

from .config import FollowCameraConfig


class FollowCamera:
    CAMERA_PATH = "/World/FollowCamera"

    def __init__(
        self,
        config: FollowCameraConfig,
        viewport: Any,
        camera_transform_op: Any,
        original_camera_path: Any,
    ) -> None:
        self._config = config
        self._viewport = viewport
        self._camera_transform_op = camera_transform_op
        self._original_camera_path = original_camera_path

    @classmethod
    def create(
        cls,
        config: FollowCameraConfig,
        stage: Any,
        viewport: Any,
    ) -> "FollowCamera":
        from pxr import Gf, UsdGeom

        camera = UsdGeom.Camera.Define(stage, cls.CAMERA_PATH)
        camera.CreateFocalLengthAttr(config.focal_length_mm)
        camera.CreateHorizontalApertureAttr(36.0)
        camera.CreateClippingRangeAttr(Gf.Vec2f(0.1, 100_000.0))
        transform_op = UsdGeom.Xformable(camera.GetPrim()).AddTransformOp(
            precision=UsdGeom.XformOp.PrecisionDouble
        )
        original_camera_path = viewport.camera_path
        viewport.resolution = (config.width, config.height)
        viewport.set_active_camera(cls.CAMERA_PATH)
        follow_camera = cls(config, viewport, transform_op, original_camera_path)
        follow_camera.update((0.0, 0.0, 0.07))
        return follow_camera

    def update(self, vehicle_position_xyz_m: Sequence[float]) -> None:
        from pxr import Gf

        eye = Gf.Vec3d(
            *(
                float(vehicle_position_xyz_m[index])
                + self._config.eye_offset_xyz_m[index]
                for index in range(3)
            )
        )
        target = Gf.Vec3d(
            *(
                float(vehicle_position_xyz_m[index])
                + self._config.target_offset_xyz_m[index]
                for index in range(3)
            )
        )
        camera_transform = Gf.Matrix4d().SetLookAt(
            eye, target, Gf.Vec3d(0.0, 0.0, 1.0)
        )
        self._camera_transform_op.Set(camera_transform.GetInverse())

    def close(self) -> None:
        self._viewport.set_active_camera(self._original_camera_path)
