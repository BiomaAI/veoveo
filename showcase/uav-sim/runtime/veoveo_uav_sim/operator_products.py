from __future__ import annotations

import math
import threading
import time
from collections import deque
from dataclasses import dataclass

from .h264 import NativeH264AccessUnit
from .hydra_camera import (
    RtxTiledHydraRenderProduct,
    native_sensor_aov_arguments,
    tcp_listener_is_ready,
)
from .operator_camera import AuthoritativeOperatorCameraCollection, Pose
from .operator_camera_config import OperatorLiveViewRuntimeConfig
from .operator_health import OperatorProductHealth
from .rtsp_h264 import RtspEndpoint, RtspH264Receiver


_FRAME_RING_SIZE = 256
OPERATOR_ATLAS_NAME = "uav_camera_atlas"
OPERATOR_ATLAS_PRODUCT_ID = "camera-atlas"


def operator_aov_arguments(config: OperatorLiveViewRuntimeConfig) -> list[str]:
    """Configure the single tiled CUDA AOV and NVENC stream."""
    cameras = config.streamable_cameras
    if not cameras:
        return []
    _require_uniform_atlas_optics(config)
    return native_sensor_aov_arguments(
        OPERATOR_ATLAS_NAME,
        rtsp_port=config.atlas_rtsp_port,
        target_fps=cameras[0].optics.frame_rate_hz,
    )


@dataclass(frozen=True, slots=True)
class OperatorEncodedFrame:
    sequence: int
    access_unit: NativeH264AccessUnit


@dataclass(frozen=True, slots=True)
class OperatorCameraRegion:
    camera_id: str
    x_px: int
    y_px: int
    width_px: int
    height_px: int

    def as_dict(self) -> dict[str, object]:
        return {
            "cameraId": self.camera_id,
            "xPx": self.x_px,
            "yPx": self.y_px,
            "widthPx": self.width_px,
            "heightPx": self.height_px,
        }


def operator_atlas_layout(
    config: OperatorLiveViewRuntimeConfig,
) -> tuple[tuple[OperatorCameraRegion, ...], int, int]:
    definitions = config.streamable_cameras
    if not definitions:
        raise ValueError("operator camera atlas requires a streamable camera")
    _require_uniform_atlas_optics(config)
    optics = definitions[0].optics
    columns = math.ceil(math.sqrt(len(definitions)))
    rows = math.ceil(len(definitions) / columns)
    regions = tuple(
        OperatorCameraRegion(
            camera_id=definition.camera_id,
            x_px=(index % columns) * optics.width_px,
            y_px=(index // columns) * optics.height_px,
            width_px=optics.width_px,
            height_px=optics.height_px,
        )
        for index, definition in enumerate(definitions)
    )
    return regions, columns * optics.width_px, rows * optics.height_px


def initial_operator_atlas_state(
    config: OperatorLiveViewRuntimeConfig,
) -> dict[str, object]:
    regions, coded_width_px, coded_height_px = operator_atlas_layout(config)
    return {
        "streamProductId": OPERATOR_ATLAS_PRODUCT_ID,
        "cameraRegions": [region.as_dict() for region in regions],
        "codedWidthPx": coded_width_px,
        "codedHeightPx": coded_height_px,
        "lifecycle": "starting",
        "activeViewers": 0,
        "connectedViewers": 0,
        "nvencSessions": 0,
        "encodedFrames": 0,
        "sourceToRenderSamples": 0,
    }


class OperatorCameraProduct:
    """One tiled RTX/NVENC product shared by all cameras and viewers."""

    def __init__(
        self,
        config: OperatorLiveViewRuntimeConfig,
        cameras: AuthoritativeOperatorCameraCollection,
        *,
        maximum_frame_age_ms: int = 2_000,
    ) -> None:
        definitions = config.streamable_cameras
        if not definitions:
            raise ValueError("operator camera atlas requires a streamable camera")
        _require_uniform_atlas_optics(config)
        camera_by_id = {
            camera.definition.camera_id: camera for camera in cameras.cameras
        }
        missing = [
            definition.camera_id
            for definition in definitions
            if definition.camera_id not in camera_by_id
        ]
        if missing:
            raise ValueError(f"operator camera atlas is missing cameras: {missing}")
        optics = definitions[0].optics
        self.product_id = OPERATOR_ATLAS_PRODUCT_ID
        self._camera_ids = tuple(definition.camera_id for definition in definitions)
        (
            self._regions,
            self._coded_width_px,
            self._coded_height_px,
        ) = operator_atlas_layout(config)
        self._condition = threading.Condition()
        self._closed = False
        self._failure: BaseException | None = None
        self._receiver: RtspH264Receiver | None = None
        self._endpoint = RtspEndpoint("127.0.0.1", config.atlas_rtsp_port)
        self._frames: deque[OperatorEncodedFrame] = deque(maxlen=_FRAME_RING_SIZE)
        self._sequence = 0
        self._source_pose_monotonic_seconds: float | None = None
        self._health = OperatorProductHealth(maximum_frame_age_ms)
        self._health.activate()
        self._render_product = RtxTiledHydraRenderProduct(
            name=OPERATOR_ATLAS_NAME,
            camera_paths=tuple(
                camera_by_id[definition.camera_id].camera_path
                for definition in definitions
            ),
            tile_width=optics.width_px,
            tile_height=optics.height_px,
        )
        if (
            self._render_product.width != self._coded_width_px
            or self._render_product.height != self._coded_height_px
        ):
            self.close()
            raise RuntimeError(
                "Isaac tiled RTX product resolution does not match its camera atlas"
            )

    def observe_source_pose(self, monotonic_seconds: float) -> None:
        if not math.isfinite(monotonic_seconds) or monotonic_seconds < 0.0:
            raise ValueError("operator-camera source time must be finite and non-negative")
        with self._condition:
            self._source_pose_monotonic_seconds = monotonic_seconds

    def observe_render(self) -> None:
        try:
            if not self._render_product.observe_gpu_frame():
                return
            start_receiver = False
            now = time.monotonic()
            with self._condition:
                if self._closed:
                    return
                source = self._source_pose_monotonic_seconds
                if source is not None:
                    self._health.observe_source_to_render(
                        max(0, round((now - source) * 1_000_000))
                    )
                if self._receiver is None and tcp_listener_is_ready(
                    self._endpoint.port
                ):
                    self._receiver = RtspH264Receiver(
                        self._endpoint,
                        self._on_access_unit,
                        self._record_failure,
                    )
                    start_receiver = True
                receiver = self._receiver
            if start_receiver:
                assert receiver is not None
                receiver.start()
        except BaseException as error:
            self._record_failure(error)

    def wait_for_frame(
        self, camera_id: str, after_sequence: int, timeout_seconds: float
    ) -> OperatorEncodedFrame | None:
        if camera_id not in self._camera_ids:
            raise ValueError(f"unknown streamable logical camera {camera_id!r}")
        if after_sequence < 0 or timeout_seconds <= 0.0:
            raise ValueError("operator stream cursor and timeout are invalid")
        deadline = time.monotonic() + timeout_seconds
        with self._condition:
            while True:
                if after_sequence == 0:
                    frame = next(
                        (
                            candidate
                            for candidate in reversed(self._frames)
                            if candidate.access_unit.is_keyframe
                        ),
                        None,
                    )
                else:
                    frame = next(
                        (
                            candidate
                            for candidate in self._frames
                            if candidate.sequence > after_sequence
                        ),
                        None,
                    )
                if frame is not None:
                    return frame
                if self._closed:
                    raise RuntimeError("operator camera atlas is closed")
                if self._failure is not None:
                    raise RuntimeError("operator camera atlas failed") from self._failure
                remaining = deadline - time.monotonic()
                if remaining <= 0.0:
                    return None
                self._condition.wait(remaining)

    def state(self, *, content_ready: bool) -> dict[str, object]:
        with self._condition:
            if self._failure is not None:
                self._health.fail(str(self._failure) or type(self._failure).__name__)
            result = self._health.snapshot(content_ready=content_ready)
        result.update(
            {
                "streamProductId": self.product_id,
                "cameraRegions": [region.as_dict() for region in self._regions],
                "codedWidthPx": self._coded_width_px,
                "codedHeightPx": self._coded_height_px,
                "activeViewers": 0,
                "connectedViewers": 0,
                "nvencSessions": 1,
            }
        )
        return result

    @property
    def camera_ids(self) -> tuple[str, ...]:
        return self._camera_ids

    def close(self) -> None:
        with self._condition:
            if self._closed:
                return
            self._closed = True
            receiver = self._receiver
            self._receiver = None
            self._condition.notify_all()
        if receiver is not None:
            receiver.close()
        self._render_product.close()

    def _on_access_unit(self, access_unit: NativeH264AccessUnit) -> None:
        with self._condition:
            if self._closed:
                return
            self._sequence += 1
            self._frames.append(OperatorEncodedFrame(self._sequence, access_unit))
            self._health.observe_frame()
            self._condition.notify_all()

    def _record_failure(self, error: BaseException) -> None:
        with self._condition:
            if self._failure is None:
                self._failure = error
                self._condition.notify_all()


class OperatorProductCollection:
    def __init__(self, product: OperatorCameraProduct) -> None:
        self._product = product

    @classmethod
    def create(
        cls,
        config: OperatorLiveViewRuntimeConfig,
        cameras: AuthoritativeOperatorCameraCollection,
    ) -> "OperatorProductCollection":
        return cls(OperatorCameraProduct(config, cameras))

    def wait_for_frame(
        self, camera_id: str, after_sequence: int, timeout_seconds: float
    ) -> OperatorEncodedFrame | None:
        return self._product.wait_for_frame(
            camera_id, after_sequence, timeout_seconds
        )

    def sync_camera_poses(
        self,
        camera_poses: dict[str, Pose],
        *,
        source_monotonic_seconds: float,
    ) -> None:
        if any(camera_id in self._product.camera_ids for camera_id in camera_poses):
            self._product.observe_source_pose(source_monotonic_seconds)

    def observe_render(self) -> None:
        self._product.observe_render()

    def state(self, *, content_ready: bool) -> list[dict[str, object]]:
        return [self._product.state(content_ready=content_ready)]

    def active_camera_ids(self) -> tuple[str, ...]:
        return self._product.camera_ids

    def close(self) -> None:
        self._product.close()


def _require_uniform_atlas_optics(config: OperatorLiveViewRuntimeConfig) -> None:
    cameras = config.streamable_cameras
    if not cameras:
        return
    first = cameras[0].optics
    for camera in cameras[1:]:
        optics = camera.optics
        if (
            optics.width_px != first.width_px
            or optics.height_px != first.height_px
            or optics.frame_rate_hz != first.frame_rate_hz
        ):
            raise ValueError(
                "tiled operator cameras must share width, height, and frame rate"
            )
