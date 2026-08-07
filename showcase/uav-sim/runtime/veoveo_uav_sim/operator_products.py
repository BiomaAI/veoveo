from __future__ import annotations

import ctypes
import threading
import time
from typing import Any

import numpy as np

from .operator_camera import (
    CameraOptics,
    CameraStreamPolicy,
    OperatorCameraDefinition,
    Pose,
    apply_usd_camera_pose,
    define_usd_camera,
)
from .operator_camera_config import OperatorLiveViewRuntimeConfig
from .operator_health import OperatorProductHealth


RENDER_PRODUCT_PREFIX = "/Render/OmniverseKit/HydraTextures"
RGBA8_TEXTURE_FORMAT = "TextureFormat.RGBA8_UNORM"

_capsule_pointer = ctypes.pythonapi.PyCapsule_GetPointer
_capsule_pointer.argtypes = [ctypes.py_object, ctypes.c_char_p]
_capsule_pointer.restype = ctypes.c_void_p


def operator_product_name(capacity_slot: int) -> str:
    if not 0 <= capacity_slot <= 31:
        raise ValueError("viewer-product capacity slot must be 0-31")
    return f"uav_viewer_slot_{capacity_slot}"


def operator_stream_product_id(capacity_slot: int) -> str:
    if not 0 <= capacity_slot <= 31:
        raise ValueError("viewer-product capacity slot must be 0-31")
    return f"product-slot-{capacity_slot}"


def native_signaling_is_listening(signaling_port: int) -> bool:
    expected_port = f"{signaling_port:04X}"
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


def livestream_aov_product_arguments(
    product_name: str,
    *,
    signaling_port: int,
    media_port: int,
    public_media_ip: str,
    target_fps: int,
) -> list[str]:
    aov = (
        "Render.OmniverseKit.HydraTextures."
        f"{product_name}.LdrColor"
    )
    prefix = f"--/exts/omni.kit.livestream.aov/{aov}/spectatorStream/0"
    settings = {
        "streamType": "webrtc",
        "signalPort": str(signaling_port),
        "streamPort": str(media_port),
        "publicIp": public_media_ip,
        "targetFps": str(target_fps),
        "allowDynamicResize": "false",
        "authenticateBearer": "false",
    }
    return [f"{prefix}/{name}={value}" for name, value in settings.items()]


def livestream_aov_arguments(config: OperatorLiveViewRuntimeConfig) -> list[str]:
    arguments: list[str] = []
    for capacity_slot in range(config.viewer_slot_count):
        arguments.extend(
            livestream_aov_product_arguments(
                operator_product_name(capacity_slot),
                signaling_port=config.signaling_port_base + capacity_slot,
                media_port=config.media_port_base + capacity_slot,
                public_media_ip=config.public_media_ip,
                target_fps=config.viewer_optics.frame_rate_hz,
            )
        )
    return arguments


class OperatorRenderProduct:
    def __init__(
        self,
        capacity_slot: int,
        optics: CameraOptics,
        camera_path: str,
        *,
        signaling_port: int,
        maximum_frame_age_ms: int = 1_000,
    ) -> None:
        import omni.hydratexture
        import omni.kit.renderer_capture
        from carb.eventdispatcher import get_eventdispatcher
        from omni.kit.hydra_texture import create_hydra_texture

        self.capacity_slot = capacity_slot
        self.optics = optics
        self.camera_path = camera_path
        self.product_id = operator_stream_product_id(capacity_slot)
        self.name = operator_product_name(capacity_slot)
        self.signaling_port = signaling_port
        self._lock = threading.Lock()
        self._activation = threading.Event()
        self._active = False
        self._closed = False
        self._capture_pending = False
        self._last_capture_requested = 0.0
        self._failure: BaseException | None = None
        self._camera_id: str | None = None
        self._live_view_id: str | None = None
        self._activation_frame_ready = False
        self._signaling_ready = False
        self._source_pose_monotonic_seconds: float | None = None
        self._health = OperatorProductHealth(maximum_frame_age_ms)
        self._hydra_texture = create_hydra_texture(
            self.name,
            optics.width_px,
            optics.height_px,
            usd_camera_path=camera_path,
            hydra_engine_name="rtx",
            is_async=True,
            is_async_low_latency=False,
            hydra_tick_rate=optics.frame_rate_hz,
        )
        actual_path = self._hydra_texture.get_render_product_path()
        expected_path = f"{RENDER_PRODUCT_PREFIX}/{self.name}"
        if actual_path != expected_path:
            self._hydra_texture.updates_enabled = False
            raise RuntimeError(
                "RTX HydraTexture created an unexpected operator product: "
                f"{actual_path!r}"
            )
        self._capture = (
            omni.kit.renderer_capture.acquire_renderer_capture_interface()
        )
        self._subscription = get_eventdispatcher().observe_event(
            observer_name=f"veoveo_uav_operator_{self.name}",
            event_name=omni.hydratexture.GLOBAL_EVENT_DRAWABLE_CHANGED,
            on_event=self._on_drawable,
            filter=self._hydra_texture.get_event_key(),
        )
        self._hydra_texture.updates_enabled = False

    @property
    def active(self) -> bool:
        with self._lock:
            return self._active and not self._closed

    def assign(self, camera_id: str, live_view_id: str) -> None:
        with self._lock:
            if self._closed:
                raise RuntimeError("viewer product is closed")
            if self._active:
                if self._camera_id == camera_id and self._live_view_id == live_view_id:
                    return
                raise RuntimeError("viewer product is already assigned")
            if not camera_id or not live_view_id:
                raise ValueError("viewer product assignment identities are required")
            self._camera_id = camera_id
            self._live_view_id = live_view_id
            self._active = True
            self._failure = None
            self._capture_pending = False
            self._last_capture_requested = 0.0
            self._activation_frame_ready = False
            self._signaling_ready = False
            self._source_pose_monotonic_seconds = None
            self._activation.clear()
            self._health.activate()
        self._hydra_texture.updates_enabled = True
        self._observe_signaling_readiness()

    def wait_until_ready(self, live_view_id: str, timeout_seconds: float) -> None:
        if not self._activation.wait(timeout_seconds):
            raise TimeoutError(
                f"viewer product {self.product_id} did not activate native signaling"
            )
        with self._lock:
            if not self._active or self._live_view_id != live_view_id:
                raise RuntimeError("viewer product assignment ended during activation")
            if self._failure is not None:
                raise RuntimeError("viewer product activation failed") from self._failure
            if not self._activation_frame_ready or not self._signaling_ready:
                raise RuntimeError("viewer product activation completed without readiness")

    def release(self, live_view_id: str) -> None:
        with self._lock:
            if not self._active:
                return
            if self._live_view_id != live_view_id:
                raise RuntimeError("viewer product assignment does not match the lease")
            self._active = False
            self._capture_pending = False
            self._camera_id = None
            self._live_view_id = None
            self._activation_frame_ready = False
            self._signaling_ready = False
            self._source_pose_monotonic_seconds = None
            self._activation.set()
            self._health.deactivate()
        self._hydra_texture.updates_enabled = False

    def release_unconditionally(self) -> None:
        with self._lock:
            self._active = False
            self._capture_pending = False
            self._camera_id = None
            self._live_view_id = None
            self._activation_frame_ready = False
            self._signaling_ready = False
            self._source_pose_monotonic_seconds = None
            self._activation.set()
            self._health.deactivate()
        self._hydra_texture.updates_enabled = False

    def assignment(self) -> tuple[str, str] | None:
        with self._lock:
            if not self._active or self._camera_id is None or self._live_view_id is None:
                return None
            return (self._camera_id, self._live_view_id)

    def close(self) -> None:
        with self._lock:
            self._closed = True
            self._active = False
            self._capture_pending = False
            self._camera_id = None
            self._live_view_id = None
            self._activation_frame_ready = False
            self._signaling_ready = False
            self._source_pose_monotonic_seconds = None
            self._activation.set()
            self._health.deactivate()
        self._hydra_texture.updates_enabled = False
        self._subscription = None

    def observe_source_pose(self, monotonic_seconds: float) -> None:
        if not np.isfinite(monotonic_seconds) or monotonic_seconds < 0.0:
            raise ValueError("operator-camera source time must be finite and non-negative")
        with self._lock:
            if self._active and not self._closed:
                self._source_pose_monotonic_seconds = monotonic_seconds

    def state(self, *, content_ready: bool) -> dict[str, object]:
        with self._lock:
            failure = self._failure
            if failure is not None:
                self._health.fail(str(failure) or type(failure).__name__)
            result = self._health.snapshot(content_ready=content_ready)
            assignment = (
                (self._camera_id, self._live_view_id)
                if self._active
                and self._camera_id is not None
                and self._live_view_id is not None
                else None
            )
            active = self._active and not self._closed
            signaling_ready = self._signaling_ready
        if active and not signaling_ready and result["lifecycle"] != "failed":
            result["lifecycle"] = "starting"
            result["diagnostic"] = "native signaling is starting"
        result.update(
            {
                "streamProductId": self.product_id,
                "capacitySlot": self.capacity_slot,
                "activeViewerLeases": 1 if assignment is not None else 0,
                "connectedViewers": 0,
                "nvencSessions": 1 if active else 0,
            }
        )
        if assignment is not None:
            result["cameraId"] = assignment[0]
            result["liveViewId"] = assignment[1]
        return result

    def _on_drawable(self, event: Any) -> None:
        try:
            with self._lock:
                if self._closed or not self._active:
                    return
            aovs = self._hydra_texture.get_aov_info(
                event["result_handle"], "LdrColor", include_texture=True
            )
            if not aovs:
                return
            texture = aovs[0].get("texture")
            resource = texture.get("rp_resource") if isinstance(texture, dict) else None
            if resource is None:
                return
            now = time.monotonic()
            with self._lock:
                if self._closed or not self._active:
                    return
                source_to_render_microseconds = (
                    max(
                        0,
                        round(
                            (now - self._source_pose_monotonic_seconds) * 1_000_000
                        ),
                    )
                    if self._source_pose_monotonic_seconds is not None
                    else None
                )
                self._health.observe_frame(
                    monotonic_seconds=now,
                    source_to_render_microseconds=source_to_render_microseconds,
                )
                self._activation_frame_ready = True
                if self._capture_pending or now - self._last_capture_requested < 0.5:
                    capture = False
                else:
                    self._capture_pending = True
                    self._last_capture_requested = now
                    capture = True
            self._observe_signaling_readiness()
            if capture:
                self._capture.capture_next_frame_rp_resource_callback(
                    self._on_capture,
                    resource,
                )
        except BaseException as error:
            self._record_failure(error)

    def _on_capture(
        self,
        buffer: Any,
        buffer_size: int,
        width: int,
        height: int,
        pixel_format: Any,
    ) -> None:
        try:
            expected = self.optics.width_px * self.optics.height_px * 4
            if (
                width != self.optics.width_px
                or height != self.optics.height_px
                or buffer_size != expected
                or str(pixel_format) != RGBA8_TEXTURE_FORMAT
            ):
                raise RuntimeError("operator RTX product capture shape changed")
            pointer = _capsule_pointer(buffer, None)
            if pointer is None:
                raise RuntimeError("operator RTX product returned a null buffer")
            rgba_buffer = (ctypes.c_uint8 * buffer_size).from_address(pointer)
            rgba = np.ctypeslib.as_array(rgba_buffer).reshape((height, width, 4))
            # TODO(GPU): Replace this low-cadence health-only readback with a
            # CUDA reduction over the LdrColor resource. Rendering and media
            # stay on the GPU and enter NVENC through the NVIDIA AOV extension.
            rgb = rgba[:, :, :3]
            visible = bool(
                int(rgb.max()) - int(rgb.min()) >= 8
                and np.count_nonzero(np.any(rgb > 8, axis=2))
                >= width * height * 0.02
            )
            with self._lock:
                if self._closed or not self._active:
                    return
                self._capture_pending = False
                self._health.observe_frame(visible=visible)
                self._activation_frame_ready = True
            self._observe_signaling_readiness()
        except BaseException as error:
            self._record_failure(error)

    def _record_failure(self, error: BaseException) -> None:
        with self._lock:
            self._capture_pending = False
            if self._failure is None:
                self._failure = error
            self._activation.set()

    def _observe_signaling_readiness(self) -> None:
        with self._lock:
            if (
                self._closed
                or not self._active
                or self._signaling_ready
                or not self._activation_frame_ready
            ):
                return
        if not native_signaling_is_listening(self.signaling_port):
            return
        with self._lock:
            if self._active and not self._closed and self._activation_frame_ready:
                self._signaling_ready = True
                self._activation.set()


class OperatorProductCollection:
    def __init__(
        self,
        products: tuple[OperatorRenderProduct, ...],
        cameras: tuple[OperatorCameraDefinition, ...],
        transforms: dict[int, Any],
    ) -> None:
        self._products = {product.capacity_slot: product for product in products}
        if len(self._products) != len(products):
            raise ValueError("viewer-product capacity slots must be unique")
        self._cameras = {
            camera.camera_id: camera
            for camera in cameras
            if camera.stream_policy is not CameraStreamPolicy.DISABLED
        }
        self._transforms = transforms

    @classmethod
    def create(
        cls,
        config: OperatorLiveViewRuntimeConfig,
        stage: Any,
    ) -> "OperatorProductCollection":
        transforms: dict[int, Any] = {}
        products = []
        for capacity_slot in range(config.viewer_slot_count):
            camera_path = f"/World/OperatorViewerCameras/slot_{capacity_slot}"
            transforms[capacity_slot] = define_usd_camera(
                stage, camera_path, config.viewer_optics
            )
            products.append(
                OperatorRenderProduct(
                    capacity_slot,
                    config.viewer_optics,
                    camera_path,
                    signaling_port=config.signaling_port_base + capacity_slot,
                )
            )
        return cls(tuple(products), config.cameras, transforms)

    def assign(
        self,
        capacity_slot: int,
        camera_id: str,
        live_view_id: str,
        *,
        content_ready: bool,
    ) -> dict[str, object]:
        if camera_id not in self._cameras:
            raise ValueError(f"unknown streamable logical camera {camera_id!r}")
        product = self._require(capacity_slot)
        product.assign(camera_id, live_view_id)
        return product.state(content_ready=content_ready)

    def wait_until_ready(
        self,
        capacity_slot: int,
        live_view_id: str,
        *,
        timeout_seconds: float,
        content_ready: bool,
    ) -> dict[str, object]:
        product = self._require(capacity_slot)
        product.wait_until_ready(live_view_id, timeout_seconds)
        return product.state(content_ready=content_ready)

    def release(
        self,
        capacity_slot: int,
        live_view_id: str,
        *,
        content_ready: bool,
    ) -> dict[str, object]:
        product = self._require(capacity_slot)
        product.release(live_view_id)
        return product.state(content_ready=content_ready)

    def release_all(self) -> None:
        for product in self._products.values():
            product.release_unconditionally()

    def sync_camera_poses(
        self,
        camera_poses: dict[str, Pose],
        *,
        source_monotonic_seconds: float,
    ) -> None:
        for capacity_slot, product in self._products.items():
            assignment = product.assignment()
            if assignment is None:
                continue
            camera_id, _ = assignment
            pose = camera_poses.get(camera_id)
            if pose is not None:
                apply_usd_camera_pose(self._transforms[capacity_slot], pose)
                product.observe_source_pose(source_monotonic_seconds)

    def state(self, *, content_ready: bool) -> list[dict[str, object]]:
        return [
            product.state(content_ready=content_ready)
            for product in sorted(
                self._products.values(),
                key=lambda product: product.capacity_slot,
            )
        ]

    def active_camera_ids(self) -> tuple[str, ...]:
        return tuple(
            assignment[0]
            for product in sorted(
                self._products.values(),
                key=lambda product: product.capacity_slot,
            )
            if (assignment := product.assignment()) is not None
        )

    def close(self) -> None:
        for product in self._products.values():
            product.close()

    def _require(self, capacity_slot: int) -> OperatorRenderProduct:
        try:
            return self._products[capacity_slot]
        except KeyError as error:
            raise ValueError(
                f"unknown viewer-product capacity slot {capacity_slot}"
            ) from error
