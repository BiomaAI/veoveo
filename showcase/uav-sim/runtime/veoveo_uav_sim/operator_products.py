from __future__ import annotations

import math
import threading
import time
from collections import deque
from dataclasses import dataclass
from typing import Any

from .h264 import NativeH264AccessUnit
from .hydra_camera import (
    RtxHydraRenderProduct,
    native_sensor_aov_arguments,
    tcp_listener_is_ready,
)
from .operator_camera import (
    AuthoritativeOperatorCameraCollection,
    OperatorCameraDefinition,
    Pose,
)
from .operator_camera_config import OperatorLiveViewRuntimeConfig
from .operator_health import OperatorProductHealth
from .rtsp_h264 import RtspEndpoint, RtspH264Receiver


_FRAME_RING_SIZE = 256


def operator_product_name(product_index: int) -> str:
    if not 0 <= product_index <= 31:
        raise ValueError("operator-camera product index must be 0-31")
    return f"uav_camera_product_{product_index}"


def operator_stream_product_id(product_index: int) -> str:
    if not 0 <= product_index <= 31:
        raise ValueError("operator-camera product index must be 0-31")
    return f"camera-product-{product_index}"


def operator_aov_arguments(config: OperatorLiveViewRuntimeConfig) -> list[str]:
    """Configure one native RTSP/NVENC product per logical camera."""
    arguments: list[str] = []
    for product_index, camera in enumerate(config.streamable_cameras):
        arguments.extend(
            native_sensor_aov_arguments(
                operator_product_name(product_index),
                rtsp_port=config.rtsp_port(product_index),
                target_fps=camera.optics.frame_rate_hz,
            )
        )
    return arguments


@dataclass(frozen=True, slots=True)
class OperatorEncodedFrame:
    sequence: int
    access_unit: NativeH264AccessUnit


class OperatorCameraProduct:
    """One continuously encoded camera product shared by every viewer."""

    def __init__(
        self,
        product_index: int,
        definition: OperatorCameraDefinition,
        camera_path: str,
        *,
        rtsp_port: int,
        maximum_frame_age_ms: int = 2_000,
    ) -> None:
        import omni.hydratexture
        from carb.eventdispatcher import get_eventdispatcher

        self.product_index = product_index
        self.definition = definition
        self.product_id = operator_stream_product_id(product_index)
        self._condition = threading.Condition()
        self._closed = False
        self._failure: BaseException | None = None
        self._receiver: RtspH264Receiver | None = None
        self._endpoint = RtspEndpoint("127.0.0.1", rtsp_port)
        self._frames: deque[OperatorEncodedFrame] = deque(maxlen=_FRAME_RING_SIZE)
        self._sequence = 0
        self._source_pose_monotonic_seconds: float | None = None
        self._health = OperatorProductHealth(maximum_frame_age_ms)
        self._health.activate()
        self._render_product = RtxHydraRenderProduct(
            name=operator_product_name(product_index),
            camera_path=camera_path,
            width=definition.optics.width_px,
            height=definition.optics.height_px,
            render_fps=definition.optics.frame_rate_hz,
        )
        self._subscription = get_eventdispatcher().observe_event(
            observer_name=f"veoveo_uav_operator_{self.product_id}",
            event_name=omni.hydratexture.GLOBAL_EVENT_DRAWABLE_CHANGED,
            on_event=self._on_drawable,
            filter=self._render_product.hydra_texture.get_event_key(),
        )
        self._render_product.set_updates_enabled(True)

    def observe_source_pose(self, monotonic_seconds: float) -> None:
        if not math.isfinite(monotonic_seconds) or monotonic_seconds < 0.0:
            raise ValueError("operator-camera source time must be finite and non-negative")
        with self._condition:
            self._source_pose_monotonic_seconds = monotonic_seconds

    def wait_for_frame(
        self, after_sequence: int, timeout_seconds: float
    ) -> OperatorEncodedFrame | None:
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
                    raise RuntimeError("operator camera product is closed")
                if self._failure is not None:
                    raise RuntimeError("operator camera product failed") from self._failure
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
                "cameraId": self.definition.camera_id,
                "activeViewers": 0,
                "connectedViewers": 0,
                "nvencSessions": 1,
            }
        )
        return result

    def close(self) -> None:
        with self._condition:
            if self._closed:
                return
            self._closed = True
            receiver = self._receiver
            self._receiver = None
            self._condition.notify_all()
        self._subscription = None
        if receiver is not None:
            receiver.close()
        self._render_product.close()

    def _on_drawable(self, _event: Any) -> None:
        try:
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
    def __init__(self, products: tuple[OperatorCameraProduct, ...]) -> None:
        self._products = {product.definition.camera_id: product for product in products}
        if len(self._products) != len(products):
            raise ValueError("operator-camera product identities must be unique")

    @classmethod
    def create(
        cls,
        config: OperatorLiveViewRuntimeConfig,
        cameras: AuthoritativeOperatorCameraCollection,
    ) -> "OperatorProductCollection":
        camera_by_id = {
            camera.definition.camera_id: camera for camera in cameras.cameras
        }
        return cls(
            tuple(
                OperatorCameraProduct(
                    product_index,
                    definition,
                    camera_by_id[definition.camera_id].camera_path,
                    rtsp_port=config.rtsp_port(product_index),
                )
                for product_index, definition in enumerate(
                    config.streamable_cameras
                )
            )
        )

    def wait_for_frame(
        self, camera_id: str, after_sequence: int, timeout_seconds: float
    ) -> OperatorEncodedFrame | None:
        return self._require(camera_id).wait_for_frame(
            after_sequence, timeout_seconds
        )

    def sync_camera_poses(
        self,
        camera_poses: dict[str, Pose],
        *,
        source_monotonic_seconds: float,
    ) -> None:
        for camera_id in camera_poses:
            product = self._products.get(camera_id)
            if product is not None:
                product.observe_source_pose(source_monotonic_seconds)

    def state(self, *, content_ready: bool) -> list[dict[str, object]]:
        return [
            product.state(content_ready=content_ready)
            for product in sorted(
                self._products.values(), key=lambda product: product.product_index
            )
        ]

    def active_camera_ids(self) -> tuple[str, ...]:
        return tuple(sorted(self._products))

    def close(self) -> None:
        for product in self._products.values():
            product.close()

    def _require(self, camera_id: str) -> OperatorCameraProduct:
        try:
            return self._products[camera_id]
        except KeyError as error:
            raise ValueError(
                f"unknown streamable logical camera {camera_id!r}"
            ) from error
