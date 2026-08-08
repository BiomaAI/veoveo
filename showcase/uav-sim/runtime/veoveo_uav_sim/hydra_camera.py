from __future__ import annotations

import logging
import threading
from dataclasses import dataclass
from typing import Any

import numpy as np

from .camera_quality import CameraFrameQuality
from .h264 import NativeH264AccessUnit, parse_native_h264_access_unit


LOGGER = logging.getLogger("veoveo.uav_sim.hydra_camera")
RTX_RENDER_PRODUCT_PREFIX = "/Render/OmniverseKit/HydraTextures"
MAX_NATIVE_ENCODER_WARMUP_FRAMES = 16


@dataclass(frozen=True, slots=True)
class HydraRenderedCamera:
    """Camera matrices reported by the exact rendered Hydra product."""

    view: tuple[float, ...]
    width: int
    height: int


@dataclass(frozen=True, slots=True)
class NativeSensorFrame:
    sequence: int
    access_unit: NativeH264AccessUnit
    quality: CameraFrameQuality
    simulation_time_s: float
    physics_step: int
    rendered_camera: HydraRenderedCamera


@dataclass(slots=True)
class SensorCaptureGate:
    """Coalesce physics-driven native encoder submissions."""

    requested: bool = False
    pending: bool = False
    closed: bool = False

    def request(self) -> bool:
        if self.closed or self.requested or self.pending:
            return False
        self.requested = True
        return True

    def begin_render(self) -> bool:
        if self.closed or self.pending or not self.requested:
            return False
        self.requested = False
        self.pending = True
        return True

    def complete(self) -> None:
        self.pending = False

    def close(self) -> None:
        self.closed = True
        self.requested = False
        self.pending = False


def hydra_rendered_camera(frame: dict[str, Any]) -> HydraRenderedCamera:
    view = tuple(float(value) for value in frame.get("view", ()))
    resolution = tuple(int(value) for value in frame.get("resolution", ()))
    if len(view) != 16 or len(resolution) != 2:
        raise RuntimeError("RTX Hydra product returned invalid camera metadata")
    if not all(np.isfinite(value) for value in view):
        raise RuntimeError("RTX Hydra product returned a non-finite view matrix")
    if resolution[0] < 1 or resolution[1] < 1:
        raise RuntimeError("RTX Hydra product returned invalid viewport resolution")
    return HydraRenderedCamera(view=view, width=resolution[0], height=resolution[1])


def render_product_path(name: str) -> str:
    if not name or not all(
        character.isascii()
        and (character.isalnum() or character in {"_", "-"})
        for character in name
    ):
        raise ValueError(
            "RTX Hydra render-product names must be non-empty path components "
            "containing only ASCII letters, digits, underscores, or dashes"
        )
    return f"{RTX_RENDER_PRODUCT_PREFIX}/{name}"


class RtxHydraRenderProduct:
    def __init__(
        self,
        *,
        name: str,
        camera_path: str,
        width: int,
        height: int,
        render_fps: int,
    ) -> None:
        from omni.kit.hydra_texture import create_hydra_texture

        if width < 1 or height < 1 or render_fps < 1:
            raise ValueError(
                "RTX Hydra render-product width, height, and fps must be positive"
            )
        self._path = render_product_path(name)
        self._hydra_texture = create_hydra_texture(
            name,
            width,
            height,
            usd_camera_path=camera_path,
            hydra_engine_name="rtx",
            is_async=True,
            is_async_low_latency=False,
            hydra_tick_rate=render_fps,
        )
        actual_path = self._hydra_texture.get_render_product_path()
        if actual_path != self._path:
            self.close()
            raise RuntimeError(
                "RTX HydraTexture created an unexpected render product: "
                f"{actual_path}"
            )

    @property
    def path(self) -> str:
        return self._path

    @property
    def hydra_texture(self) -> Any:
        return self._hydra_texture

    def set_updates_enabled(self, enabled: bool) -> None:
        self._hydra_texture.updates_enabled = enabled

    def close(self) -> None:
        self.set_updates_enabled(False)


def _annotator_array(data: Any, name: str) -> Any:
    if isinstance(data, dict):
        data = data.get("data")
    if data is None:
        raise RuntimeError(f"native Isaac {name} annotator returned no data")
    return data


def _create_native_sensor_writer(
    owner: "NativeH264CameraSensor", width: int, height: int
) -> Any:
    """Create the Writer subclass only after SimulationApp initializes Kit."""
    from omni.replicator.core import AnnotatorRegistry, Writer

    from .gpu_camera_quality import GpuCameraQualityReducer

    class NativeSensorWriter(Writer):
        def __init__(self) -> None:
            self._quality = GpuCameraQualityReducer(width, height)
            self.version = "1.0.0"
            self.node_type_id = "VeoVeoNativeH264CameraWriter"
            self._kwargs: dict[str, object] = {}
            encoded = AnnotatorRegistry.get_annotator(
                "LdrColor", init_params={"compression": "h264"}
            )
            quality = AnnotatorRegistry.get_annotator(
                "rgb", device="cuda:0", do_array_copy=False
            )
            # Follow the pinned NVIDIA writer contract exactly. Public-name
            # aliases mutate annotator instances shared by Replicator's
            # registry and are unnecessary because one writer owns one
            # render product.
            self.annotators = [encoded, quality]
            self._annotators = list(self.annotators)

        def write(self, data: dict[str, Any]) -> None:
            try:
                encoded = _annotator_array(data.get("LdrColor"), "H.264")
                if not isinstance(encoded, np.ndarray) or encoded.dtype != np.uint8:
                    raise RuntimeError(
                        "native Isaac H.264 annotator did not return uint8"
                    )
                sample = encoded.tobytes()
                if not sample:
                    owner._retry_native_encoder_warmup()
                    return
                rgba = _annotator_array(
                    data.get("rgb"), "CUDA LdrColor"
                )
                owner._complete_native_frame(
                    parse_native_h264_access_unit(sample),
                    self._quality.measure(rgba),
                )
            except BaseException as error:
                owner._record_failure(error)

    return NativeSensorWriter()


class NativeH264CameraSensor:
    """One physics-gated RTX frame encoded in-place by Isaac NVENC."""

    def __init__(
        self,
        *,
        name: str,
        camera_path: str,
        width: int,
        height: int,
        render_fps: int,
    ) -> None:
        import omni.hydratexture
        from carb.eventdispatcher import get_eventdispatcher

        self._lock = threading.Lock()
        self._capture = SensorCaptureGate()
        self._sequence = 0
        self._latest: NativeSensorFrame | None = None
        self._rendered_camera: HydraRenderedCamera | None = None
        self._pending_access_unit: NativeH264AccessUnit | None = None
        self._pending_quality: CameraFrameQuality | None = None
        self._sample_time: tuple[float, int] | None = None
        self._encoder_warmup_frames = 0
        self._failure: BaseException | None = None
        self._camera_path = camera_path
        self._render_product = RtxHydraRenderProduct(
            name=name,
            camera_path=camera_path,
            width=width,
            height=height,
            render_fps=render_fps,
        )
        self._writer = _create_native_sensor_writer(self, width, height)
        # The HydraTexture itself is the capture gate. An on-frame writer is
        # only evaluated while this product has updates enabled, avoiding the
        # separate WriterOrchestrator graph used by manual schedule events.
        self._writer.attach(self._render_product.path)
        self._subscription = get_eventdispatcher().observe_event(
            observer_name=f"veoveo_uav_native_h264_{name}",
            event_name=omni.hydratexture.GLOBAL_EVENT_DRAWABLE_CHANGED,
            on_event=self._on_drawable_changed,
            filter=self._render_product.hydra_texture.get_event_key(),
        )
        self._render_product.set_updates_enabled(False)

    @property
    def render_product_path(self) -> str:
        return self._render_product.path

    @property
    def camera_path(self) -> str:
        return self._camera_path

    def request_capture(self) -> None:
        with self._lock:
            self._capture.request()

    def prepare_render(self, simulation_time_s: float, physics_step: int) -> None:
        with self._lock:
            if not self._capture.begin_render():
                return
            self._sample_time = (simulation_time_s, physics_step)
            self._rendered_camera = None
            self._pending_access_unit = None
            self._pending_quality = None
            self._encoder_warmup_frames = 0
        self._render_product.set_updates_enabled(True)

    def latest_frame(self, after_sequence: int = 0) -> NativeSensorFrame | None:
        with self._lock:
            failure = self._failure
            frame = self._latest
        if failure is not None:
            raise RuntimeError("native Isaac H.264 sensor failed") from failure
        if frame is None or frame.sequence <= after_sequence:
            return None
        return frame

    def _retry_native_encoder_warmup(self) -> None:
        error: RuntimeError | None = None
        with self._lock:
            if self._capture.closed:
                return
            if not self._capture.pending:
                error = RuntimeError(
                    "native Isaac H.264 encoder returned data without a pending capture"
                )
            else:
                self._encoder_warmup_frames += 1
                if (
                    self._encoder_warmup_frames
                    > MAX_NATIVE_ENCODER_WARMUP_FRAMES
                ):
                    error = RuntimeError(
                        "native Isaac H.264 encoder did not produce an access unit "
                        f"within {MAX_NATIVE_ENCODER_WARMUP_FRAMES} rendered frames"
                    )
        if error is not None:
            self._record_failure(error)
            return
        # The product remains enabled while the hardware encoder warms. Its
        # next render drives the on-frame writer without a timer or a manual
        # WriterOrchestrator event.

    def close(self) -> None:
        with self._lock:
            self._capture.close()
        self._subscription = None
        self._writer.detach()
        self._render_product.close()

    def _on_drawable_changed(self, event: Any) -> None:
        try:
            rendered = hydra_rendered_camera(
                self._render_product.hydra_texture.get_frame_info(
                    event["result_handle"]
                )
            )
            with self._lock:
                if not self._capture.closed:
                    self._rendered_camera = rendered
                complete = self._finalize_frame_locked()
            if complete:
                self._render_product.set_updates_enabled(False)
        except BaseException as error:
            self._record_failure(error)

    def _complete_native_frame(
        self,
        access_unit: NativeH264AccessUnit,
        quality: CameraFrameQuality,
    ) -> None:
        with self._lock:
            self._pending_access_unit = access_unit
            self._pending_quality = quality
            complete = self._finalize_frame_locked()
        if complete:
            self._render_product.set_updates_enabled(False)

    def _finalize_frame_locked(self) -> bool:
        sample_time = self._sample_time
        rendered_camera = self._rendered_camera
        access_unit = self._pending_access_unit
        quality = self._pending_quality
        if (
            sample_time is None
            or rendered_camera is None
            or access_unit is None
            or quality is None
        ):
            return False
        if not self._capture.closed:
            self._sequence += 1
            self._latest = NativeSensorFrame(
                sequence=self._sequence,
                access_unit=access_unit,
                quality=quality,
                simulation_time_s=sample_time[0],
                physics_step=sample_time[1],
                rendered_camera=rendered_camera,
            )
        self._capture.complete()
        self._sample_time = None
        self._pending_access_unit = None
        self._pending_quality = None
        self._encoder_warmup_frames = 0
        return True

    def _record_failure(self, error: BaseException) -> None:
        first_failure = False
        with self._lock:
            self._capture.complete()
            self._sample_time = None
            self._pending_access_unit = None
            self._pending_quality = None
            self._encoder_warmup_frames = 0
            if self._failure is None:
                self._failure = error
                first_failure = True
        self._render_product.set_updates_enabled(False)
        if first_failure:
            LOGGER.error(
                "native Isaac H.264 sensor failed",
                exc_info=(type(error), error, error.__traceback__),
            )
