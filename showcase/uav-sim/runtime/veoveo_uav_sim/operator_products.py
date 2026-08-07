from __future__ import annotations

import ctypes
import threading
import time
from typing import Any, Protocol, TypeVar

import numpy as np

from .hydra_camera import HydraRenderViewport, hydra_render_viewport
from .operator_camera import CameraStreamPolicy, OperatorCameraDefinition
from .operator_camera_config import OperatorLiveViewRuntimeConfig
from .operator_health import OperatorProductHealth


RENDER_PRODUCT_PREFIX = "/Render/OmniverseKit/HydraTextures"
RGBA8_TEXTURE_FORMAT = "TextureFormat.RGBA8_UNORM"

Matrix4dT = TypeVar("Matrix4dT")


class Matrix4dFactory(Protocol[Matrix4dT]):
    def __call__(self, *values: float) -> Matrix4dT: ...


def materialize_matrix4d(
    values: tuple[float, ...], matrix_type: Matrix4dFactory[Matrix4dT]
) -> Matrix4dT:
    """Restore the native matrix type stripped by Hydra frame serialization."""
    if len(values) != 16:
        raise ValueError("operator RTX viewport matrix must contain 16 values")
    if not all(np.isfinite(value) for value in values):
        raise ValueError("operator RTX viewport matrix values must be finite")
    return matrix_type(*values)

_capsule_pointer = ctypes.pythonapi.PyCapsule_GetPointer
_capsule_pointer.argtypes = [ctypes.py_object, ctypes.c_char_p]
_capsule_pointer.restype = ctypes.c_void_p


def operator_product_name(camera_id: str) -> str:
    if not camera_id or not all(
        character.isascii()
        and (character.isalnum() or character in {"_", "-"})
        for character in camera_id
    ):
        raise ValueError("operator-camera product identity is invalid")
    return f"uav_operator_{camera_id.replace('-', '_')}"


def operator_stream_product_id(camera_id: str) -> str:
    return f"product-{camera_id}"


def livestream_aov_arguments(config: OperatorLiveViewRuntimeConfig) -> list[str]:
    arguments: list[str] = []
    for camera in config.cameras:
        if camera.stream_policy is CameraStreamPolicy.DISABLED:
            continue
        aov = (
            "Render.OmniverseKit.HydraTextures."
            f"{operator_product_name(camera.camera_id)}.LdrColor"
        )
        prefix = f"--/exts/omni.kit.livestream.aov/{aov}/spectatorStream/0"
        settings = {
            "streamType": "webrtc",
            "signalPort": str(config.signaling_port_base + camera.physical_slot),
            "streamPort": str(config.media_port_base + camera.physical_slot),
            "publicIp": config.public_media_ip,
            "targetFps": str(camera.optics.frame_rate_hz),
            "allowDynamicResize": "false",
            "authenticateBearer": "false",
        }
        arguments.extend(
            f"{prefix}/{name}={value}" for name, value in settings.items()
        )
    return arguments


class OperatorRenderProduct:
    def __init__(
        self,
        definition: OperatorCameraDefinition,
        camera_path: str,
        *,
        maximum_frame_age_ms: int = 1_000,
    ) -> None:
        import omni.hydratexture
        import omni.kit.renderer_capture
        from carb.eventdispatcher import get_eventdispatcher
        from omni.kit.hydra_texture import create_hydra_texture

        self.definition = definition
        self.camera_path = camera_path
        self.product_id = operator_stream_product_id(definition.camera_id)
        self.name = operator_product_name(definition.camera_id)
        self._lock = threading.Lock()
        self._active = False
        self._closed = False
        self._capture_pending = False
        self._last_capture_requested = 0.0
        self._viewport: HydraRenderViewport | None = None
        self._failure: BaseException | None = None
        self._health = OperatorProductHealth(maximum_frame_age_ms)
        self._hydra_texture = create_hydra_texture(
            self.name,
            definition.optics.width_px,
            definition.optics.height_px,
            usd_camera_path=camera_path,
            hydra_engine_name="rtx",
            is_async=True,
            is_async_low_latency=True,
            hydra_tick_rate=definition.optics.frame_rate_hz,
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
        if definition.stream_policy is CameraStreamPolicy.CONTINUOUS:
            self.activate()
        else:
            self._hydra_texture.updates_enabled = False

    @property
    def active(self) -> bool:
        with self._lock:
            return self._active and not self._closed

    @property
    def viewport(self) -> HydraRenderViewport | None:
        with self._lock:
            return self._viewport

    def activate(self) -> None:
        with self._lock:
            if self._closed:
                raise RuntimeError("operator-camera product is closed")
            if self._active:
                return
            self._active = True
            self._failure = None
            self._viewport = None
            self._capture_pending = False
            self._last_capture_requested = 0.0
            self._health.activate()
        self._hydra_texture.updates_enabled = True

    def deactivate(self) -> None:
        if self.definition.stream_policy is CameraStreamPolicy.CONTINUOUS:
            return
        with self._lock:
            if not self._active:
                return
            self._active = False
            self._viewport = None
            self._capture_pending = False
            self._health.deactivate()
        self._hydra_texture.updates_enabled = False

    def close(self) -> None:
        with self._lock:
            self._closed = True
            self._active = False
            self._viewport = None
            self._capture_pending = False
            self._health.deactivate()
        self._hydra_texture.updates_enabled = False
        self._subscription = None

    def state(self, *, content_ready: bool) -> dict[str, object]:
        with self._lock:
            failure = self._failure
        if failure is not None:
            self._health.fail(str(failure) or type(failure).__name__)
        result = self._health.snapshot(content_ready=content_ready)
        result.update(
            {
                "cameraId": self.definition.camera_id,
                "streamProductId": self.product_id,
                "physicalSlot": self.definition.physical_slot,
                "activeViewerLeases": 0,
                "connectedViewers": 0,
                "nvencSessions": 1 if self.active else 0,
            }
        )
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
            viewport = hydra_render_viewport(
                self._hydra_texture.get_frame_info(event["result_handle"])
            )
            now = time.monotonic()
            with self._lock:
                if self._closed or not self._active:
                    return
                self._viewport = viewport
                self._health.observe_frame(monotonic_seconds=now)
                if self._capture_pending or now - self._last_capture_requested < 0.5:
                    return
                self._capture_pending = True
                self._last_capture_requested = now
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
            expected = self.definition.optics.width_px * self.definition.optics.height_px * 4
            if (
                width != self.definition.optics.width_px
                or height != self.definition.optics.height_px
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
        except BaseException as error:
            self._record_failure(error)

    def _record_failure(self, error: BaseException) -> None:
        with self._lock:
            self._capture_pending = False
            if self._failure is None:
                self._failure = error


class OperatorProductCollection:
    def __init__(self, products: tuple[OperatorRenderProduct, ...]) -> None:
        self._products = {product.definition.camera_id: product for product in products}
        if len(self._products) != len(products):
            raise ValueError("operator product camera identities must be unique")

    @classmethod
    def create(
        cls,
        definitions: tuple[OperatorCameraDefinition, ...],
        camera_paths: dict[str, str],
    ) -> "OperatorProductCollection":
        return cls(
            tuple(
                OperatorRenderProduct(
                    definition,
                    camera_paths[definition.camera_id],
                )
                for definition in definitions
                if definition.stream_policy is not CameraStreamPolicy.DISABLED
            )
        )

    def activate(self, camera_id: str, *, content_ready: bool) -> dict[str, object]:
        product = self._require(camera_id)
        product.activate()
        return product.state(content_ready=content_ready)

    def deactivate(self, camera_id: str, *, content_ready: bool) -> dict[str, object]:
        product = self._require(camera_id)
        product.deactivate()
        return product.state(content_ready=content_ready)

    def deactivate_all_on_demand(self) -> None:
        for product in self._products.values():
            product.deactivate()

    def state(self, *, content_ready: bool) -> list[dict[str, object]]:
        return [
            product.state(content_ready=content_ready)
            for product in sorted(
                self._products.values(),
                key=lambda product: product.definition.physical_slot,
            )
        ]

    def active_viewports(self) -> tuple[OperatorRenderViewport, ...]:
        return tuple(
            viewport
            for product in sorted(
                self._products.values(),
                key=lambda product: product.definition.physical_slot,
            )
            if product.active and (viewport := product.viewport) is not None
        )

    def close(self) -> None:
        for product in self._products.values():
            product.close()

    def _require(self, camera_id: str) -> OperatorRenderProduct:
        try:
            return self._products[camera_id]
        except KeyError as error:
            raise ValueError(f"unknown operator-camera product {camera_id!r}") from error
