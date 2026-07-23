from __future__ import annotations

import asyncio
import logging
from collections.abc import Sequence
from dataclasses import dataclass
from typing import Any

from .config import ScreenshotConfig


LOGGER = logging.getLogger("veoveo.uav_sim.screenshot")


@dataclass(slots=True)
class ScreenshotGate:
    settle_rendered_frames: int
    ready_rendered_frames: int = 0
    triggered: bool = False

    def observe(self, *, rendered: bool, ready: bool) -> bool:
        if self.triggered or not rendered:
            return False
        if not ready:
            self.ready_rendered_frames = 0
            return False
        self.ready_rendered_frames += 1
        if self.ready_rendered_frames < self.settle_rendered_frames:
            return False
        self.triggered = True
        return True


class ShowcaseScreenshotCapture:
    CAMERA_PATH = "/World/ShowcaseCamera"

    def __init__(
        self,
        config: ScreenshotConfig,
        viewport: Any,
        camera_transform_op: Any,
        original_camera_path: Any,
    ) -> None:
        self._config = config
        self._viewport = viewport
        self._camera_transform_op = camera_transform_op
        self._original_camera_path = original_camera_path
        self._gate = ScreenshotGate(config.settle_rendered_frames)
        self._capture_task: asyncio.Task[Any] | None = None
        self._completed = False

    @classmethod
    def create(
        cls,
        config: ScreenshotConfig,
        stage: Any,
        viewport: Any,
    ) -> "ShowcaseScreenshotCapture":
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
        capture = cls(config, viewport, transform_op, original_camera_path)
        capture.update_camera((0.0, 0.0, 0.07))
        LOGGER.info(
            "Isaac showcase screenshot armed: path=%s resolution=%dx%d",
            config.output_path,
            config.width,
            config.height,
        )
        return capture

    def update_camera(self, vehicle_position_xyz_m: Sequence[float]) -> None:
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

    def observe(
        self,
        *,
        rendered: bool,
        tiles_ready: bool,
        camera_content_visible: bool,
        vehicle_relative_altitude_m: float,
    ) -> None:
        self._raise_capture_error()
        ready = (
            tiles_ready
            and camera_content_visible
            and vehicle_relative_altitude_m
            >= self._config.minimum_relative_altitude_m
        )
        if not self._gate.observe(rendered=rendered, ready=ready):
            return

        from omni.kit.viewport.utility import capture_viewport_to_file

        self._config.output_path.parent.mkdir(parents=True, exist_ok=True)
        capture = capture_viewport_to_file(
            self._viewport,
            file_path=str(self._config.output_path),
        )
        self._capture_task = asyncio.ensure_future(
            capture.wait_for_result(completion_frames=30)
        )
        LOGGER.info(
            "Capturing Isaac showcase screenshot after %d settled rendered frames",
            self._gate.ready_rendered_frames,
        )

    def poll(self) -> bool:
        self._raise_capture_error()
        if (
            self._completed
            or self._capture_task is None
            or not self._capture_task.done()
        ):
            return self._completed
        if not self._config.output_path.is_file():
            raise RuntimeError(
                f"Isaac screenshot capture produced no file at {self._config.output_path}"
            )
        self._completed = True
        LOGGER.info("Isaac showcase screenshot written: %s", self._config.output_path)
        return True

    def close(self) -> None:
        self._viewport.set_active_camera(self._original_camera_path)

    def _raise_capture_error(self) -> None:
        if self._capture_task is None or not self._capture_task.done():
            return
        error = self._capture_task.exception()
        if error is not None:
            raise RuntimeError("Isaac screenshot capture failed") from error
