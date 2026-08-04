from __future__ import annotations

import ctypes
import math
import threading
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

import numpy as np

from .config import RendererConfig
from .contracts import CameraBinding, ContractError, RenderViewport
from .interpolation import RenderedPoseFrame
from .pose import EntityPose
from .renderer_setup import RendererFailure, RendererFailureCode

RENDER_PRODUCT_PREFIX = "/Render/OmniverseKit/HydraTextures"
RGBA8_TEXTURE_FORMAT = "TextureFormat.RGBA8_UNORM"
READINESS_RENDER_PRODUCT_NAME = "simulation_view_readiness"

_capsule_pointer = ctypes.pythonapi.PyCapsule_GetPointer
_capsule_pointer.argtypes = [ctypes.py_object, ctypes.c_char_p]
_capsule_pointer.restype = ctypes.c_void_p


def render_product_name(slot: int) -> str:
    return f"simulation_view_slot_{slot}"


def render_product_path(name: str) -> str:
    return f"{RENDER_PRODUCT_PREFIX}/{name}"


def livestream_aov_arguments(config: RendererConfig) -> list[str]:
    arguments: list[str] = []
    for slot in range(config.maximum_render_slots):
        aov = f"Render.OmniverseKit.HydraTextures.{render_product_name(slot)}.LdrColor"
        prefix = f"--/exts/omni.kit.livestream.aov/{aov}/spectatorStream/0"
        settings = {
            "streamType": "webrtc",
            "signalPort": str(config.signaling_port_base + slot),
            "streamPort": str(config.media_port_base + slot),
            "publicIp": config.public_media_ip,
            "targetFps": str(config.stream_target_fps),
            "allowDynamicResize": "false",
            "authenticateBearer": "false",
        }
        arguments.extend(f"{prefix}/{name}={value}" for name, value in settings.items())
    return arguments


@dataclass(frozen=True, slots=True)
class FrameHealth:
    sequence: int
    observed_at: float
    observed_at_iso: str
    visible: bool


class HydraRenderProductProbe:
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
        from omni.kit.hydra_texture import create_hydra_texture

        self._width = width
        self._height = height
        self._lock = threading.Lock()
        self._capture_pending = False
        self._last_capture_requested = 0.0
        self._closed = False
        self._generation = 0
        self._health: FrameHealth | None = None
        self._last_drawable_at: float | None = None
        self._last_drawable_at_iso: str | None = None
        self._viewport: RenderViewport | None = None
        self._failure: BaseException | None = None
        self._hydra_texture = create_hydra_texture(
            name,
            width,
            height,
            usd_camera_path=camera_path,
            hydra_engine_name="rtx",
            is_async=True,
            is_async_low_latency=True,
            hydra_tick_rate=fps,
        )
        actual = self._hydra_texture.get_render_product_path()
        if actual != render_product_path(name):
            self.close()
            raise RuntimeError(f"RTX HydraTexture returned unexpected path {actual!r}")
        self._capture = omni.kit.renderer_capture.acquire_renderer_capture_interface()
        self._subscription = get_eventdispatcher().observe_event(
            observer_name=f"veoveo_simulation_view_{name}",
            event_name=omni.hydratexture.GLOBAL_EVENT_DRAWABLE_CHANGED,
            on_event=self._on_drawable,
            filter=self._hydra_texture.get_event_key(),
        )

    @property
    def health(self) -> FrameHealth | None:
        with self._lock:
            failure = self._failure
            health = self._health
            last_drawable_at = self._last_drawable_at
            last_drawable_at_iso = self._last_drawable_at_iso
        if failure is not None:
            raise RuntimeError("RTX render-product health probe failed") from failure
        if (
            health is not None
            and last_drawable_at is not None
            and last_drawable_at_iso is not None
        ):
            return FrameHealth(
                sequence=health.sequence,
                observed_at=last_drawable_at,
                observed_at_iso=last_drawable_at_iso,
                visible=health.visible,
            )
        return health

    @property
    def viewport(self) -> RenderViewport | None:
        with self._lock:
            return self._viewport

    def close(self) -> None:
        self.pause()
        if hasattr(self, "_subscription"):
            self._subscription = None

    def pause(self) -> None:
        with self._lock:
            self._closed = True
            self._generation += 1
            self._capture_pending = False
            self._health = None
            self._last_drawable_at = None
            self._last_drawable_at_iso = None
            self._viewport = None
            self._failure = None
        self._hydra_texture.updates_enabled = False

    def reconfigure(
        self,
        *,
        camera_path: str,
        width: int,
        height: int,
        fps: int,
    ) -> None:
        import carb.settings

        if width < 1 or height < 1 or fps < 1:
            raise ValueError(
                "RTX render-product width, height, and fps must be positive"
            )
        self.pause()
        self._hydra_texture.camera_path = camera_path
        self._hydra_texture.width = width
        self._hydra_texture.height = height
        carb.settings.get_settings().set(
            f"{self._hydra_texture.get_settings_path()}hydraTickRate",
            fps,
        )
        with self._lock:
            self._width = width
            self._height = height
            self._closed = False
            self._last_capture_requested = 0.0
        self._hydra_texture.updates_enabled = True

    def _on_drawable(self, event: Any) -> None:
        try:
            aovs = self._hydra_texture.get_aov_info(
                event["result_handle"], "LdrColor", include_texture=True
            )
            if not aovs:
                return
            texture = aovs[0].get("texture")
            resource = texture.get("rp_resource") if isinstance(texture, dict) else None
            if resource is None:
                return
            frame = self._hydra_texture.get_frame_info(event["result_handle"])
            view = tuple(float(value) for value in frame.get("view", ()))
            projection = tuple(float(value) for value in frame.get("projection", ()))
            resolution = tuple(int(value) for value in frame.get("resolution", ()))
            if (
                len(view) != 16
                or len(projection) != 16
                or len(resolution) != 2
                or resolution[0] < 1
                or resolution[1] < 1
            ):
                raise RuntimeError("RTX render-product viewport metadata is invalid")
            viewport = RenderViewport(
                view=view,
                projection=projection,
                width=resolution[0],
                height=resolution[1],
            )
            now = time.monotonic()
            now_iso = _timestamp()
            with self._lock:
                if self._closed:
                    return
                # Drawable events are the authoritative frame-production
                # clock. Pixel visibility validation intentionally runs at a
                # lower cadence, so its completion time must not make a live
                # render product appear stale between readbacks.
                self._last_drawable_at = now
                self._last_drawable_at_iso = now_iso
                self._viewport = viewport
                if self._capture_pending or now - self._last_capture_requested < 0.5:
                    return
                self._capture_pending = True
                self._last_capture_requested = now
                generation = self._generation
            self._capture.capture_next_frame_rp_resource_callback(
                lambda buffer, buffer_size, width, height, pixel_format: (
                    self._on_capture(
                        generation,
                        buffer,
                        buffer_size,
                        width,
                        height,
                        pixel_format,
                    )
                ),
                resource,
            )
        except BaseException as error:
            self._record_failure(error)

    def _on_capture(
        self,
        generation: int,
        buffer: Any,
        buffer_size: int,
        width: int,
        height: int,
        pixel_format: Any,
    ) -> None:
        try:
            with self._lock:
                if generation != self._generation or self._closed:
                    return
                expected_width = self._width
                expected_height = self._height
            expected = expected_width * expected_height * 4
            if (
                width != expected_width
                or height != expected_height
                or buffer_size != expected
                or str(pixel_format) != RGBA8_TEXTURE_FORMAT
            ):
                raise RuntimeError("RTX render-product capture shape or format changed")
            pointer = _capsule_pointer(buffer, None)
            if pointer is None:
                raise RuntimeError("RTX render-product returned a null buffer")
            rgba_buffer = (ctypes.c_uint8 * buffer_size).from_address(pointer)
            rgba = np.ctypeslib.as_array(rgba_buffer).reshape((height, width, 4))
            # TODO(GPU): Replace this low-cadence health-only readback and
            # reduction with a CUDA reduction over the AOV resource. Media
            # remains on the GPU and enters NVENC directly through the NVIDIA
            # AOV extension.
            rgb = rgba[:, :, :3]
            visible = bool(
                int(rgb.max()) - int(rgb.min()) >= 8
                and np.count_nonzero(np.any(rgb > 8, axis=2)) >= width * height * 0.02
            )
            with self._lock:
                if generation != self._generation or self._closed:
                    return
                previous = self._health
                self._health = FrameHealth(
                    sequence=1 if previous is None else previous.sequence + 1,
                    observed_at=time.monotonic(),
                    observed_at_iso=_timestamp(),
                    visible=visible,
                )
                self._capture_pending = False
        except BaseException as error:
            self._record_failure(error, generation)

    def _record_failure(
        self, error: BaseException, generation: int | None = None
    ) -> None:
        with self._lock:
            if generation is not None and generation != self._generation:
                return
            self._capture_pending = False
            if self._failure is None:
                self._failure = error


@dataclass(slots=True)
class CameraRuntime:
    binding: CameraBinding
    camera_path: str
    transform_operation: Any
    probe: HydraRenderProductProbe
    smoothed_eye: tuple[float, float, float] | None = None
    last_update: float = 0.0
    last_pose_sequence: int | None = None
    pose_stale: bool = False

    def status(self) -> dict[str, object]:
        health = self.probe.health
        return {
            "cameraId": self.binding.camera_id,
            "ready": bool(health and health.visible and not self.pose_stale),
            "lastPoseSequence": self.last_pose_sequence,
            "lastFrameAt": (health.observed_at_iso if health is not None else None),
        }


@dataclass(slots=True)
class ReadinessProbeRuntime:
    probe: HydraRenderProductProbe


class CameraPool:
    def __init__(self, stage: Any, config: RendererConfig) -> None:
        self._stage = stage
        self._config = config
        self._cameras: dict[str, CameraRuntime] = {}
        self._slots: dict[int, str] = {}
        self._idle: dict[int, CameraRuntime] = {}
        # Readiness owns a low-resolution RTX product outside the admitted
        # media slots. A live slot must never be reconfigured back into a
        # diagnostic product during teardown: Kit applies that transition on
        # its next update and can block the renderer control loop while the
        # AOV encoder is draining.
        self._probe = self._create_diagnostic_probe()

    def upsert(self, binding: CameraBinding) -> dict[str, object]:
        existing_camera = self._slots.get(binding.render_slot)
        if existing_camera is not None and existing_camera != binding.camera_id:
            raise ContractError("renderer slot is already assigned")
        existing = self._cameras.get(binding.camera_id)
        if existing is not None:
            if existing.binding.render_slot != binding.render_slot:
                raise ContractError("renderer camera slot is immutable")
            if existing.binding == binding:
                return existing.status()
            self._configure_camera(existing, binding)
            return existing.status()
        runtime = self._idle.pop(binding.render_slot, None)
        if runtime is None:
            runtime = self._create_camera(binding)
        else:
            self._configure_camera(runtime, binding)
        self._cameras[binding.camera_id] = runtime
        self._slots[binding.render_slot] = binding.camera_id
        return runtime.status()

    def close(self, camera_id: str) -> None:
        runtime = self._cameras.pop(camera_id, None)
        if runtime is None:
            return
        self._slots.pop(runtime.binding.render_slot, None)
        runtime.probe.pause()
        runtime.smoothed_eye = None
        runtime.last_update = 0.0
        runtime.last_pose_sequence = None
        runtime.pose_stale = False
        self._idle[runtime.binding.render_slot] = runtime

    def close_session(self, session_id: str) -> None:
        for camera_id in [
            camera_id
            for camera_id, runtime in self._cameras.items()
            if runtime.binding.session_id == session_id
        ]:
            self.close(camera_id)

    def tick(
        self,
        snapshots: dict[str, RenderedPoseFrame],
        stale_sessions: set[str],
    ) -> None:
        for runtime in self._cameras.values():
            snapshot = snapshots.get(runtime.binding.session_id)
            runtime.pose_stale = _rig_requires_pose(
                runtime.binding.definition["rig"]
            ) and (snapshot is None or runtime.binding.session_id in stale_sessions)
            self._update_camera(runtime, snapshot)

    def camera_for_slot(self, slot: int) -> str | None:
        return self._slots.get(slot)

    def active_camera_paths(self) -> tuple[str, ...]:
        return tuple(
            runtime.camera_path
            for _, runtime in sorted(
                self._cameras.items(),
                key=lambda item: item[1].binding.render_slot,
            )
        )

    def active_camera_ids(self, session_id: str) -> tuple[str, ...]:
        return tuple(
            camera_id
            for camera_id, runtime in sorted(self._cameras.items())
            if runtime.binding.session_id == session_id
        )

    def render_viewports(self) -> tuple[RenderViewport, ...]:
        viewports: list[RenderViewport] = []
        for _, runtime in sorted(
            self._cameras.items(),
            key=lambda item: item[1].binding.render_slot,
        ):
            viewport = runtime.probe.viewport
            if viewport is not None:
                viewports.append(viewport)
        return tuple(viewports)

    def status(self, camera_id: str) -> dict[str, object]:
        runtime = self._cameras.get(camera_id)
        if runtime is None:
            raise ContractError("renderer camera does not exist")
        return runtime.status()

    def readiness(
        self,
    ) -> tuple[bool, bool, bool, RendererFailure | None]:
        runtimes = [*self._cameras.values(), self._probe]
        now = time.monotonic()
        product_ready = bool(runtimes)
        try:
            health = [runtime.probe.health for runtime in runtimes]
        except RuntimeError:
            return (
                False,
                False,
                False,
                RendererFailure(
                    RendererFailureCode.LDR_COLOR_PIPELINE_FAILED,
                    "RTX LdrColor render-product validation failed",
                ),
            )
        color_pipeline_ready = any(item is not None for item in health)
        visible = any(
            item is not None
            and item.visible
            and (now - item.observed_at) * 1000.0 <= self._config.frame_stale_after_ms
            for item in health
        )
        return (
            product_ready,
            color_pipeline_ready,
            visible,
            None,
        )

    def close_all(self) -> None:
        for runtime in self._cameras.values():
            runtime.probe.close()
        for runtime in self._idle.values():
            runtime.probe.close()
        self._cameras.clear()
        self._slots.clear()
        self._idle.clear()
        self._probe.probe.close()

    def _configure_camera(self, runtime: CameraRuntime, binding: CameraBinding) -> None:
        from pxr import Gf, UsdGeom

        definition = binding.definition
        camera = UsdGeom.Camera.Define(self._stage, runtime.camera_path)
        aspect = float(definition["widthPx"]) / float(definition["heightPx"])
        vertical_aperture = 24.0
        focal = vertical_aperture / (
            2.0 * math.tan(math.radians(float(definition["verticalFovDegrees"])) / 2.0)
        )
        camera.CreateVerticalApertureAttr().Set(vertical_aperture)
        camera.CreateHorizontalApertureAttr().Set(vertical_aperture * aspect)
        camera.CreateFocalLengthAttr().Set(focal)
        camera.CreateClippingRangeAttr().Set(
            Gf.Vec2f(
                float(definition["nearClipM"]),
                float(definition["farClipM"]),
            )
        )
        runtime.binding = binding
        runtime.smoothed_eye = None
        runtime.last_update = 0.0
        runtime.last_pose_sequence = None
        runtime.pose_stale = False
        runtime.probe.reconfigure(
            camera_path=runtime.camera_path,
            width=int(definition["widthPx"]),
            height=int(definition["heightPx"]),
            fps=max(
                1,
                int(definition["frameRateMillihertz"]) // 1000,
            ),
        )
        self._update_camera(runtime, None)

    def _create_camera(self, binding: CameraBinding) -> CameraRuntime:
        from pxr import Gf, UsdGeom

        definition = binding.definition
        path = f"/World/SimulationView/Cameras/slot_{binding.render_slot}"
        camera = UsdGeom.Camera.Define(self._stage, path)
        aspect = float(definition["widthPx"]) / float(definition["heightPx"])
        vertical_aperture = 24.0
        focal = vertical_aperture / (
            2.0 * math.tan(math.radians(float(definition["verticalFovDegrees"])) / 2.0)
        )
        camera.CreateVerticalApertureAttr(vertical_aperture)
        camera.CreateHorizontalApertureAttr(vertical_aperture * aspect)
        camera.CreateFocalLengthAttr(focal)
        camera.CreateClippingRangeAttr(
            Gf.Vec2f(
                float(definition["nearClipM"]),
                float(definition["farClipM"]),
            )
        )
        xform = UsdGeom.Xformable(camera.GetPrim())
        xform.ClearXformOpOrder()
        operation = xform.AddTransformOp(precision=UsdGeom.XformOp.PrecisionDouble)
        runtime = CameraRuntime(
            binding=binding,
            camera_path=path,
            transform_operation=operation,
            probe=HydraRenderProductProbe(
                name=render_product_name(binding.render_slot),
                camera_path=path,
                width=int(definition["widthPx"]),
                height=int(definition["heightPx"]),
                fps=max(
                    1,
                    int(definition["frameRateMillihertz"]) // 1000,
                ),
            ),
        )
        self._update_camera(runtime, None)
        return runtime

    def _create_diagnostic_probe(self) -> ReadinessProbeRuntime:
        from pxr import Gf, UsdGeom

        path = "/World/SimulationView/Cameras/diagnostic"
        camera = UsdGeom.Camera.Define(self._stage, path)
        camera.CreateFocalLengthAttr(35.0)
        camera.CreateVerticalApertureAttr(24.0)
        camera.CreateHorizontalApertureAttr(36.0)
        camera.CreateClippingRangeAttr(Gf.Vec2f(0.1, 1_000.0))
        xform = UsdGeom.Xformable(camera.GetPrim())
        operation = xform.AddTransformOp(precision=UsdGeom.XformOp.PrecisionDouble)
        operation.Set(
            Gf.Matrix4d()
            .SetLookAt(
                Gf.Vec3d(6.0, -6.0, 4.0),
                Gf.Vec3d(0.0, 0.0, 0.5),
                Gf.Vec3d(0.0, 0.0, 1.0),
            )
            .GetInverse()
        )
        return ReadinessProbeRuntime(
            probe=HydraRenderProductProbe(
                name=READINESS_RENDER_PRODUCT_NAME,
                camera_path=path,
                width=self._config.probe_width,
                height=self._config.probe_height,
                fps=self._config.probe_fps,
            ),
        )

    def _update_camera(
        self,
        runtime: CameraRuntime,
        snapshot: RenderedPoseFrame | None,
    ) -> None:
        from pxr import Gf

        rig = runtime.binding.definition["rig"]
        if (
            snapshot is None
            and _rig_requires_pose(rig)
            and runtime.last_pose_sequence is not None
        ):
            return
        entity_map = (
            {entity.entity_id: entity for entity in snapshot.entities}
            if snapshot is not None
            else {}
        )
        matrix, eye = _rig_matrix(rig, entity_map, runtime.smoothed_eye)
        smoothing = float(rig.get("smoothingSeconds", 0.0))
        now = time.monotonic()
        if eye is not None and runtime.smoothed_eye is not None and smoothing > 0:
            delta = max(0.0, now - runtime.last_update)
            alpha = 1.0 - math.exp(-delta / smoothing)
            smoothed = tuple(
                previous + (current - previous) * alpha
                for previous, current in zip(runtime.smoothed_eye, eye)
            )
            target = _target_for_rig(rig, entity_map)
            matrix = _look_at(smoothed, target)
            runtime.smoothed_eye = smoothed
        elif eye is not None:
            runtime.smoothed_eye = eye
        runtime.transform_operation.Set(Gf.Matrix4d(matrix))
        runtime.last_update = now
        if snapshot is not None:
            runtime.last_pose_sequence = snapshot.source_sequence


def _rig_matrix(
    rig: dict[str, Any],
    entities: dict[str, EntityPose],
    previous_eye: tuple[float, float, float] | None,
) -> tuple[Any, tuple[float, float, float] | None]:
    from pxr import Gf

    kind = rig["kind"]
    if kind == "fixed":
        pose = rig["pose"]
        return _pose_matrix(
            _xyz(pose["positionM"]), _xyzw(pose["orientationXyzw"])
        ), None
    if kind == "look_at":
        eye = _xyz(rig["eyeM"])
        return _look_at(eye, _xyz(rig["targetM"])), eye
    if kind == "formation_overview":
        targets = [_entity(entities, value) for value in rig["targetEntities"]]
        points = [target.position_enu_m for target in targets]
        center = tuple(sum(values) / len(points) for values in zip(*points))
        radius = max(math.dist(center, point) for point in points) + float(
            rig["paddingM"]
        )
        eye = (
            center[0],
            center[1] - max(radius * 2.0, 2.0),
            center[2] + max(radius, 2.0),
        )
        return _look_at(eye, center), eye

    target = _entity(entities, rig["targetEntity"])
    position = target.position_enu_m
    if kind == "mounted_entity":
        mount = rig["mount"]
        return _transform_matrix(mount) * _pose_matrix(
            position, target.orientation_xyzw
        ), None
    if kind == "orbit":
        azimuth = math.radians(float(rig["azimuthDegrees"]))
        elevation = math.radians(float(rig["elevationDegrees"]))
        radius = float(rig["radiusM"])
        eye = (
            position[0] + radius * math.cos(elevation) * math.cos(azimuth),
            position[1] + radius * math.cos(elevation) * math.sin(azimuth),
            position[2] + radius * math.sin(elevation),
        )
        return _look_at(eye, position), eye
    rotation = Gf.Rotation(
        Gf.Quatd(
            target.orientation_xyzw[3],
            Gf.Vec3d(*target.orientation_xyzw[:3]),
        )
    )
    if kind == "follow_entity":
        offset = rotation.TransformDir(Gf.Vec3d(*_xyz(rig["offsetFluM"])))
        eye = tuple(position[index] + offset[index] for index in range(3))
        return _look_at(eye, position), eye
    if kind == "chase_entity":
        backward = rotation.TransformDir(
            Gf.Vec3d(-float(rig["distanceM"]), 0.0, float(rig["heightM"]))
        )
        eye = tuple(position[index] + backward[index] for index in range(3))
        return _look_at(eye, position), eye
    raise ContractError("camera rig is unsupported")


def _rig_requires_pose(rig: dict[str, Any]) -> bool:
    return rig["kind"] not in {"fixed", "look_at"}


def _target_for_rig(
    rig: dict[str, Any], entities: dict[str, EntityPose]
) -> tuple[float, float, float]:
    if rig["kind"] == "look_at":
        return _xyz(rig["targetM"])
    return _entity(entities, rig["targetEntity"]).position_enu_m


def _entity(entities: dict[str, EntityPose], entity_id: str) -> EntityPose:
    entity = entities.get(entity_id)
    if entity is None:
        return EntityPose(
            entity_id=entity_id,
            position_enu_m=(0.0, 0.0, 0.0),
            orientation_xyzw=(0.0, 0.0, 0.0, 1.0),
            active=True,
            visible=True,
        )
    return entity


def _look_at(
    eye: tuple[float, float, float],
    target: tuple[float, float, float],
) -> Any:
    from pxr import Gf

    return (
        Gf.Matrix4d()
        .SetLookAt(
            Gf.Vec3d(*eye),
            Gf.Vec3d(*target),
            Gf.Vec3d(0.0, 0.0, 1.0),
        )
        .GetInverse()
    )


def _pose_matrix(
    position: tuple[float, float, float],
    orientation: tuple[float, float, float, float],
) -> Any:
    from pxr import Gf

    transform = Gf.Transform()
    transform.SetTranslation(Gf.Vec3d(*position))
    transform.SetRotation(
        Gf.Rotation(Gf.Quatd(orientation[3], Gf.Vec3d(*orientation[:3])))
    )
    return transform.GetMatrix()


def _transform_matrix(value: dict[str, Any]) -> Any:
    from pxr import Gf

    transform = Gf.Transform()
    transform.SetTranslation(Gf.Vec3d(*_xyz(value["translationM"])))
    orientation = _xyzw(value["orientationXyzw"])
    transform.SetRotation(
        Gf.Rotation(Gf.Quatd(orientation[3], Gf.Vec3d(*orientation[:3])))
    )
    transform.SetScale(Gf.Vec3d(*_xyz(value["scale"])))
    return transform.GetMatrix()


def _xyz(value: dict[str, object]) -> tuple[float, float, float]:
    return (float(value["x"]), float(value["y"]), float(value["z"]))


def _xyzw(value: dict[str, object]) -> tuple[float, float, float, float]:
    return (
        float(value["x"]),
        float(value["y"]),
        float(value["z"]),
        float(value["w"]),
    )


def _timestamp() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
