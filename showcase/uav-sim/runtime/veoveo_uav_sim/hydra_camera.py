from __future__ import annotations

import ctypes
import logging
import threading
from dataclasses import dataclass
from typing import Any

import numpy as np


LOGGER = logging.getLogger("veoveo.uav_sim.hydra_camera")
RTX_RENDER_PRODUCT_PREFIX = "/Render/OmniverseKit/HydraTextures"
RGBA8_TEXTURE_FORMAT = "TextureFormat.RGBA8_UNORM"

_py_capsule_get_pointer = ctypes.pythonapi.PyCapsule_GetPointer
_py_capsule_get_pointer.argtypes = [ctypes.py_object, ctypes.c_char_p]
_py_capsule_get_pointer.restype = ctypes.c_void_p


@dataclass(frozen=True, slots=True)
class RgbFrame:
    sequence: int
    pixels: np.ndarray


@dataclass(frozen=True, slots=True)
class HydraRenderViewport:
    view: tuple[float, ...]
    projection: tuple[float, ...]
    width: int
    height: int


def hydra_render_viewport(frame: dict[str, Any]) -> HydraRenderViewport:
    view = tuple(float(value) for value in frame.get("view", ()))
    projection = tuple(float(value) for value in frame.get("projection", ()))
    resolution = tuple(int(value) for value in frame.get("resolution", ()))
    if len(view) != 16 or len(projection) != 16 or len(resolution) != 2:
        raise RuntimeError("RTX Hydra product returned invalid viewport metadata")
    if resolution[0] < 1 or resolution[1] < 1:
        raise RuntimeError("RTX Hydra product returned invalid viewport resolution")
    return HydraRenderViewport(
        view=view,
        projection=projection,
        width=resolution[0],
        height=resolution[1],
    )


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


def _copy_rgba8_capture(
    buffer: Any,
    buffer_size: int,
    width: int,
    height: int,
    pixel_format: Any,
) -> np.ndarray:
    expected_size = width * height * 4
    if width < 1 or height < 1 or buffer_size != expected_size:
        raise RuntimeError(
            "RTX LdrColor capture has an unexpected byte shape: "
            f"{width}x{height} buffer_size={buffer_size}"
        )
    if str(pixel_format) != RGBA8_TEXTURE_FORMAT:
        raise RuntimeError(
            "RTX LdrColor capture has an unsupported texture format: "
            f"{pixel_format}"
        )
    pointer = _py_capsule_get_pointer(buffer, None)
    if pointer is None:
        raise RuntimeError("RTX LdrColor capture returned a null buffer")
    rgba_buffer = (ctypes.c_uint8 * buffer_size).from_address(pointer)
    rgba = np.ctypeslib.as_array(rgba_buffer).reshape((height, width, 4))
    # TODO(GPU): Replace this CPU readback with direct CUDA/NVENC packet
    # fan-out once Recording Hub accepts the canonical pre-encoded stream.
    return np.ascontiguousarray(rgba[:, :, :3])


class RtxHydraRenderProduct:
    def __init__(
        self,
        *,
        name: str,
        camera_path: str,
        width: int,
        height: int,
        fps: int,
    ) -> None:
        from omni.kit.hydra_texture import create_hydra_texture

        if width < 1 or height < 1 or fps < 1:
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
            hydra_tick_rate=fps,
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

    def close(self) -> None:
        self._hydra_texture.updates_enabled = False


class HydraRgbCameraSensor:
    def __init__(
        self,
        *,
        name: str,
        camera_path: str,
        width: int,
        height: int,
        fps: int,
    ) -> None:
        import omni.hydratexture
        import omni.kit.renderer_capture
        from carb.eventdispatcher import get_eventdispatcher

        self._width = width
        self._height = height
        self._lock = threading.Lock()
        self._capture_pending = False
        self._closed = False
        self._sequence = 0
        self._latest_pixels: np.ndarray | None = None
        self._viewport: HydraRenderViewport | None = None
        self._failure: BaseException | None = None
        self._render_product = RtxHydraRenderProduct(
            name=name,
            camera_path=camera_path,
            width=width,
            height=height,
            fps=fps,
        )
        self._renderer_capture = (
            omni.kit.renderer_capture.acquire_renderer_capture_interface()
        )
        self._subscription = get_eventdispatcher().observe_event(
            observer_name=f"veoveo_uav_rgb_{name}",
            event_name=omni.hydratexture.GLOBAL_EVENT_DRAWABLE_CHANGED,
            on_event=self._on_drawable_changed,
            filter=self._render_product.hydra_texture.get_event_key(),
        )

    @property
    def render_product_path(self) -> str:
        return self._render_product.path

    @property
    def viewport(self) -> HydraRenderViewport | None:
        with self._lock:
            return self._viewport

    def latest_frame(self, after_sequence: int = 0) -> RgbFrame | None:
        with self._lock:
            failure = self._failure
            sequence = self._sequence
            pixels = self._latest_pixels
        if failure is not None:
            raise RuntimeError("RTX Hydra RGB capture failed") from failure
        if pixels is None or sequence <= after_sequence:
            return None
        return RgbFrame(sequence=sequence, pixels=pixels)

    def close(self) -> None:
        with self._lock:
            self._closed = True
        self._subscription = None
        self._render_product.close()

    def _on_drawable_changed(self, event: Any) -> None:
        try:
            aov_info = self._render_product.hydra_texture.get_aov_info(
                event["result_handle"],
                "LdrColor",
                include_texture=True,
            )
            if not aov_info:
                return
            texture = aov_info[0].get("texture")
            if not isinstance(texture, dict):
                return
            resource = texture.get("rp_resource")
            if resource is None:
                return
            viewport = hydra_render_viewport(
                self._render_product.hydra_texture.get_frame_info(
                    event["result_handle"]
                )
            )
            with self._lock:
                if self._closed:
                    return
                self._viewport = viewport
                if self._capture_pending:
                    return
                self._capture_pending = True
            try:
                self._renderer_capture.capture_next_frame_rp_resource_callback(
                    self._on_capture,
                    resource,
                )
            except BaseException:
                with self._lock:
                    self._capture_pending = False
                raise
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
            if width != self._width or height != self._height:
                raise RuntimeError(
                    "RTX Hydra RGB capture resolution changed unexpectedly: "
                    f"{width}x{height}"
                )
            pixels = _copy_rgba8_capture(
                buffer,
                buffer_size,
                width,
                height,
                pixel_format,
            )
            with self._lock:
                if not self._closed:
                    self._sequence += 1
                    self._latest_pixels = pixels
                self._capture_pending = False
        except BaseException as error:
            self._record_failure(error)

    def _record_failure(self, error: BaseException) -> None:
        first_failure = False
        with self._lock:
            self._capture_pending = False
            if self._failure is None:
                self._failure = error
                first_failure = True
        if first_failure:
            LOGGER.error(
                "RTX Hydra RGB capture failed",
                exc_info=(type(error), error, error.__traceback__),
            )
