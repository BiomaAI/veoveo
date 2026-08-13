from __future__ import annotations

import logging
import threading
from collections import deque
from dataclasses import dataclass
from typing import Any

import numpy as np

from .h264 import NativeH264AccessUnit
from .rtsp_h264 import RtspEndpoint, RtspH264Receiver

LOGGER = logging.getLogger("veoveo.uav_sim.hydra_camera")
RTX_RENDER_PRODUCT_PREFIX = "/Render/OmniverseKit/HydraTextures"
_MAX_UNPAIRED_FRAMES = 16


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
    simulation_time_s: float
    physics_step: int
    rendered_camera: HydraRenderedCamera


@dataclass(frozen=True, slots=True)
class NativeSensorStatus:
    lifecycle: str
    frames_received: int
    diagnostic: str | None


@dataclass(frozen=True, slots=True)
class _RenderedSample:
    simulation_time_s: float
    physics_step: int
    camera: HydraRenderedCamera


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
        character.isascii() and (character.isalnum() or character in {"_", "-"})
        for character in name
    ):
        raise ValueError(
            "RTX Hydra render-product names must be non-empty path components "
            "containing only ASCII letters, digits, underscores, or dashes"
        )
    return f"{RTX_RENDER_PRODUCT_PREFIX}/{name}"


def native_sensor_aov_signal_port(rtsp_port: int) -> int:
    if not 1 <= rtsp_port <= 65_534:
        raise ValueError("native sensor RTSP port must be between 1 and 65534")
    return rtsp_port + 1


def native_sensor_aov_arguments(
    product_name: str,
    *,
    rtsp_port: int,
    target_fps: int,
) -> list[str]:
    """Configure one CUDA AOV-to-NVENC RTSP stream for a sensor product."""
    signal_port = native_sensor_aov_signal_port(rtsp_port)
    if not 1 <= target_fps <= 60:
        raise ValueError("native sensor frame rate must be between 1 and 60")
    aov = f"Render.OmniverseKit.HydraTextures.{product_name}.LdrColor"
    prefix = f"--/exts/omni.kit.livestream.aov/{aov}/spectatorStream/0"
    settings = {
        "streamType": "rtsp",
        # The pinned livestream core gives every server type a default
        # signalPort of 49100. RTSP does not expose that socket, but the AOV
        # manager still reserves the value and would displace the first
        # operator WebRTC product from its locked endpoint. Give the internal
        # RTSP server an explicit, disjoint reservation beside its listener.
        "signalPort": str(signal_port),
        "streamPort": str(rtsp_port),
        "targetFps": str(target_fps),
        "allowDynamicResize": "false",
    }
    return [f"{prefix}/{name}={value}" for name, value in settings.items()]


def tcp_listener_is_ready(port: int) -> bool:
    expected_port = f"{port:04X}"
    for table in ("/proc/self/net/tcp", "/proc/self/net/tcp6"):
        try:
            with open(table, encoding="ascii") as connections:
                for connection in connections:
                    fields = connection.split()
                    if len(fields) < 4 or fields[3] != "0A":
                        continue
                    if fields[1].rsplit(":", 1)[-1].upper() == expected_port:
                        return True
        except OSError:
            continue
    return False


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
                f"RTX HydraTexture created an unexpected render product: {actual_path}"
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


class NativeH264CameraSensor:
    """One native RTX AOV encoded once by Isaac and tapped over RTSP."""

    def __init__(
        self,
        *,
        name: str,
        camera_path: str,
        width: int,
        height: int,
        render_fps: int,
        rtsp_port: int,
    ) -> None:
        import omni.hydratexture
        from carb.eventdispatcher import get_eventdispatcher

        self._lock = threading.Lock()
        self._sequence = 0
        self._latest: NativeSensorFrame | None = None
        self._sample_time = (0.0, 0)
        self._rendered_samples: deque[_RenderedSample] = deque()
        self._access_units: deque[NativeH264AccessUnit] = deque()
        self._failure: BaseException | None = None
        self._camera_path = camera_path
        self._rtsp_endpoint = RtspEndpoint("127.0.0.1", rtsp_port)
        self._receiver: RtspH264Receiver | None = None
        self._closed = False
        self._render_product = RtxHydraRenderProduct(
            name=name,
            camera_path=camera_path,
            width=width,
            height=height,
            render_fps=render_fps,
        )
        self._subscription = get_eventdispatcher().observe_event(
            observer_name=f"veoveo_uav_native_h264_{name}",
            event_name=omni.hydratexture.GLOBAL_EVENT_DRAWABLE_CHANGED,
            on_event=self._on_drawable_changed,
            filter=self._render_product.hydra_texture.get_event_key(),
        )
        # This one low-rate sensor product remains active. The native AOV
        # extension transfers its CUDA LdrColor resource directly to the RTSP
        # backend, which performs one NVENC encode shared by all RTSP clients.
        self._render_product.set_updates_enabled(True)

    @property
    def render_product_path(self) -> str:
        return self._render_product.path

    @property
    def camera_path(self) -> str:
        return self._camera_path

    def observe_simulation_time(
        self, simulation_time_s: float, physics_step: int
    ) -> None:
        if not np.isfinite(simulation_time_s) or simulation_time_s < 0.0:
            raise ValueError("sensor simulation time must be finite and non-negative")
        if physics_step < 0:
            raise ValueError("sensor physics step must be non-negative")
        with self._lock:
            self._sample_time = (simulation_time_s, physics_step)

    def latest_frame(self, after_sequence: int = 0) -> NativeSensorFrame | None:
        with self._lock:
            frame = self._latest
        if frame is None or frame.sequence <= after_sequence:
            return None
        return frame

    def status(self) -> NativeSensorStatus:
        with self._lock:
            failure = self._failure
            receiver = self._receiver
            frames = self._sequence
        if failure is not None:
            return NativeSensorStatus(
                "degraded",
                frames,
                _bounded_diagnostic(failure),
            )
        if frames:
            return NativeSensorStatus("ready", frames, None)
        if receiver is not None and receiver.ready:
            return NativeSensorStatus("warming", 0, "native NVENC stream is warming")
        return NativeSensorStatus("warming", 0, "native RTSP tap is starting")

    def close(self) -> None:
        with self._lock:
            self._closed = True
            receiver = self._receiver
            self._receiver = None
        self._subscription = None
        if receiver is not None:
            receiver.close()
        self._render_product.close()

    def _on_drawable_changed(self, event: Any) -> None:
        try:
            rendered = hydra_rendered_camera(
                self._render_product.hydra_texture.get_frame_info(
                    event["result_handle"]
                )
            )
            start_receiver = False
            with self._lock:
                if self._closed:
                    return
                if self._receiver is None and tcp_listener_is_ready(
                    self._rtsp_endpoint.port
                ):
                    self._receiver = RtspH264Receiver(
                        self._rtsp_endpoint,
                        self._on_access_unit,
                        self._on_receiver_error,
                    )
                    start_receiver = True
                receiver = self._receiver
                if receiver is not None and receiver.ready:
                    self._rendered_samples.append(
                        _RenderedSample(*self._sample_time, rendered)
                    )
                    self._bound_queue(self._rendered_samples)
                    self._pair_frames_locked()
            if start_receiver:
                assert receiver is not None
                receiver.start()
        except BaseException as error:
            self._record_failure(error)

    def _on_access_unit(self, access_unit: NativeH264AccessUnit) -> None:
        with self._lock:
            if self._closed:
                return
            self._access_units.append(access_unit)
            self._bound_queue(self._access_units)
            self._pair_frames_locked()

    def _on_receiver_error(self, error: BaseException) -> None:
        self._record_failure(error)

    def _pair_frames_locked(self) -> None:
        while self._rendered_samples and self._access_units:
            rendered = self._rendered_samples.popleft()
            access_unit = self._access_units.popleft()
            self._sequence += 1
            self._latest = NativeSensorFrame(
                sequence=self._sequence,
                access_unit=access_unit,
                simulation_time_s=rendered.simulation_time_s,
                physics_step=rendered.physics_step,
                rendered_camera=rendered.camera,
            )

    @staticmethod
    def _bound_queue(values: deque[object]) -> None:
        while len(values) > _MAX_UNPAIRED_FRAMES:
            values.popleft()

    def _record_failure(self, error: BaseException) -> None:
        first_failure = False
        with self._lock:
            if self._failure is None:
                self._failure = error
                first_failure = True
        if first_failure:
            LOGGER.error(
                "native Isaac RTSP/NVENC sensor degraded; simulation continues",
                exc_info=(type(error), error, error.__traceback__),
            )


def _bounded_diagnostic(error: BaseException) -> str:
    message = str(error).strip() or type(error).__name__
    return message[:512]
