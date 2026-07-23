from __future__ import annotations

import asyncio
import logging
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
    def __init__(
        self,
        config: ScreenshotConfig,
        viewport: Any,
    ) -> None:
        self._config = config
        self._viewport = viewport
        self._gate = ScreenshotGate(config.settle_rendered_frames)
        self._capture_task: asyncio.Task[Any] | None = None
        self._completed = False

    @classmethod
    def create(
        cls,
        config: ScreenshotConfig,
        viewport: Any,
    ) -> "ShowcaseScreenshotCapture":
        capture = cls(config, viewport)
        LOGGER.info(
            "Isaac showcase screenshot armed: path=%s",
            config.output_path,
        )
        return capture

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

    def _raise_capture_error(self) -> None:
        if self._capture_task is None or not self._capture_task.done():
            return
        error = self._capture_task.exception()
        if error is not None:
            raise RuntimeError("Isaac screenshot capture failed") from error
