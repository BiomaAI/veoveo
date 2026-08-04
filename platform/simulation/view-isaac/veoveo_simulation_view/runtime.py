from __future__ import annotations

import logging
import queue
import uuid
from dataclasses import dataclass
from http import HTTPStatus
from typing import Any

from .camera import CameraPool, livestream_aov_arguments
from .config import RendererConfig
from .contracts import (
    CameraBinding,
    ContractError,
    PoseSourceBinding,
    SceneBinding,
    SessionBinding,
    StreamBinding,
)
from .control import (
    CommandResult,
    ControlCommand,
    ControlServer,
    Readiness,
    ReadinessSlot,
)
from .gpu import verify_nvidia_gpu_and_nvenc
from .interpolation import (
    InterpolationResetReason,
    PoseInterpolator,
    RenderedPoseFrame,
)
from .layers import StreamedWorldManager
from .lighting import DiagnosticScene
from .pose import PoseMirror
from .renderer_setup import (
    CesiumMaterialStatus,
    RendererFailure,
    RendererFailureCode,
    RendererInitializationError,
    configure_headless_cesium_extension,
    disable_render_product_grid,
    ensure_cesium_material_runtime,
    suppress_interactive_cesium_viewport_updates,
)
from .scene import ArtifactStore, SceneManager
from .events import announce_runtime_generation

LOGGER = logging.getLogger("veoveo.simulation_view")


@dataclass(slots=True)
class SessionRuntime:
    binding: SessionBinding
    scene: SceneBinding | None = None
    pose: PoseMirror | None = None
    pose_binding: PoseSourceBinding | None = None
    pose_authorization_floor: int = 0
    interpolation: PoseInterpolator | None = None
    pose_stale: bool = True
    last_applied_pose: tuple[int, int] | None = None


class Renderer:
    def __init__(
        self,
        config: RendererConfig,
        stage: Any,
        commands: queue.Queue[ControlCommand],
        diagnostics: DiagnosticScene,
    ) -> None:
        self._config = config
        self._generation = str(uuid.uuid4())
        self._commands = commands
        self._sessions: dict[str, SessionRuntime] = {}
        self._streams: dict[str, StreamBinding] = {}
        self._scenes = SceneManager(
            stage,
            ArtifactStore(config.artifact_directory, config.cache_directory),
            diagnostics,
        )
        self._cameras = CameraPool(stage, config)
        self._layers = StreamedWorldManager(stage, config.layer_catalog)
        self._diagnostics = diagnostics

    @property
    def generation(self) -> str:
        return self._generation

    def tick(self) -> None:
        self._process_commands()
        snapshots: dict[str, RenderedPoseFrame] = {}
        stale_sessions: set[str] = set()
        for session_id, session in self._sessions.items():
            if session.pose is None:
                continue
            while result := session.pose.poll():
                if session.interpolation is not None:
                    session.interpolation.observe(result)
            if session.pose.latest is not None and session.pose.stale:
                if not session.pose_stale and session.interpolation is not None:
                    session.interpolation.reset(InterpolationResetReason.STALE)
                session.pose_stale = True
                stale_sessions.add(session_id)
                self._scenes.mark_pose_stale(session_id)
                continue
            if session.pose.latest is None or session.interpolation is None:
                continue
            session.pose_stale = False
            frame = session.interpolation.render()
            if frame is None:
                continue
            applied_pose = (
                frame.source_sequence,
                frame.simulation_timestamp_ns,
            )
            if applied_pose != session.last_applied_pose:
                self._scenes.apply_pose(frame)
                session.last_applied_pose = applied_pose
            snapshots[session_id] = frame
        self._cameras.tick(snapshots, stale_sessions)
        self._layers.tick(self._cameras.render_viewports())

    def readiness(
        self,
    ) -> tuple[bool, bool, bool, RendererFailure | None]:
        return self._cameras.readiness()

    def streamed_world_ready(self) -> bool:
        return self._layers.ready()

    def governed_lighting_ready(self) -> bool:
        return self._diagnostics.isolated()

    def close(self) -> None:
        for session in self._sessions.values():
            if session.pose is not None:
                session.pose.close()
        self._cameras.close_all()
        self._layers.close_all()

    def _process_commands(self) -> None:
        for _ in range(128):
            try:
                command = self._commands.get_nowait()
            except queue.Empty:
                return
            try:
                result = self._execute(command)
            except ContractError as error:
                result = CommandResult(HTTPStatus.CONFLICT, {"error": str(error)})
            except BaseException:
                LOGGER.exception("renderer command failed: %s", command.operation)
                result = CommandResult(
                    HTTPStatus.INTERNAL_SERVER_ERROR,
                    {"error": "renderer transition failed"},
                )
            command.complete(result)

    def _execute(self, command: ControlCommand) -> CommandResult:
        operation = command.operation
        if operation == "put_session":
            assert isinstance(command.value, SessionBinding)
            existing = self._sessions.get(command.session_id)
            if existing is not None and existing.binding != command.value:
                raise ContractError("renderer session identity is immutable")
            if existing is None:
                self._sessions[command.session_id] = SessionRuntime(
                    binding=command.value
                )
            return _accepted()
        if operation == "delete_session":
            session = self._sessions.get(command.session_id)
            if session is None:
                return CommandResult(HTTPStatus.NO_CONTENT)
            if session.pose is not None:
                session.pose.close()
            self._cameras.close_session(command.session_id)
            self._layers.close(command.session_id)
            self._scenes.close(command.session_id)
            self._streams = {
                stream_id: stream
                for stream_id, stream in self._streams.items()
                if stream.session_id != command.session_id
            }
            self._sessions.pop(command.session_id, None)
            return CommandResult(HTTPStatus.NO_CONTENT)
        session = self._session(command.session_id)
        if operation == "put_scene":
            assert isinstance(command.value, SceneBinding)
            if command.value.epoch_id != session.binding.epoch_id:
                raise ContractError("scene epoch does not match the session")
            if session.scene == command.value:
                return _accepted()
            self._layers.bind(command.value)
            try:
                self._scenes.bind(command.value)
            except BaseException:
                self._layers.close(command.session_id)
                raise
            session.scene = command.value
            session.last_applied_pose = None
            return _accepted()
        if operation == "put_pose_source":
            assert isinstance(command.value, PoseSourceBinding)
            if session.pose_binding == command.value:
                return _accepted()
            if (
                session.pose_binding is not None
                and session.pose_binding.authorization_revision
                == command.value.authorization_revision
            ):
                raise ContractError("pose authorization revision is immutable")
            scene = _scene(session)
            if (
                command.value.epoch_id != session.binding.epoch_id
                or command.value.frame_uri != scene.frame_uri
                or command.value.frame_digest != scene.frame_digest
            ):
                raise ContractError("pose source does not match the renderer scene")
            if command.value.authorization_revision <= session.pose_authorization_floor:
                raise ContractError("pose authorization revision is stale")
            if command.value.revoked:
                session.pose_authorization_floor = command.value.authorization_revision
                if session.pose is not None:
                    session.pose.revoke()
                    session.pose = None
                if session.interpolation is None:
                    session.interpolation = PoseInterpolator(
                        scene.interpolation,
                        command.value.maximum_cadence_hz,
                        command.value.stale_after_ms,
                    )
                session.interpolation.reset(InterpolationResetReason.REVOKED)
                session.pose_stale = True
                session.last_applied_pose = None
                self._scenes.mark_pose_stale(command.session_id)
                session.pose_binding = command.value
                return _accepted()
            mirror = PoseMirror(self._config.pose_directory)
            if session.pose is None:
                mirror.bind(command.value)
                session.pose = mirror
                session.interpolation = PoseInterpolator(
                    scene.interpolation,
                    command.value.maximum_cadence_hz,
                    command.value.stale_after_ms,
                )
            else:
                session.pose.renew(command.value)
                assert session.interpolation is not None
                session.interpolation.reset(
                    InterpolationResetReason.AUTHORIZATION_REVISION_CHANGED
                )
            session.pose_stale = True
            session.last_applied_pose = None
            session.pose_authorization_floor = command.value.authorization_revision - 1
            session.pose_binding = command.value
            return _accepted()
        if operation == "delete_pose_source":
            if session.pose is not None:
                session.pose.revoke()
                session.pose = None
            if session.interpolation is not None:
                session.interpolation.reset(
                    InterpolationResetReason.POSE_SOURCE_CHANGED
                )
                session.interpolation = None
            session.pose_binding = None
            session.pose_stale = True
            session.last_applied_pose = None
            self._scenes.mark_pose_stale(command.session_id)
            return CommandResult(HTTPStatus.NO_CONTENT)
        if operation == "put_camera":
            _scene(session)
            assert isinstance(command.value, CameraBinding)
            self._streams = {
                stream_id: stream
                for stream_id, stream in self._streams.items()
                if stream.camera_id != command.value.camera_id
            }
            return CommandResult(HTTPStatus.OK, self._cameras.upsert(command.value))
        if operation == "get_inventory":
            return CommandResult(
                HTTPStatus.OK,
                {
                    "generation": self._generation,
                    "cameraIds": self._cameras.active_camera_ids(
                        command.session_id
                    ),
                    "streamProductIds": tuple(
                        stream_id
                        for stream_id, stream in sorted(self._streams.items())
                        if stream.session_id == command.session_id
                    ),
                },
            )
        if operation == "get_camera":
            assert command.resource_id is not None
            return CommandResult(
                HTTPStatus.OK,
                self._cameras.status(command.resource_id),
            )
        if operation == "get_pose_source":
            if session.pose_binding is None:
                return CommandResult(HTTPStatus.NO_CONTENT)
            if session.interpolation is None:
                raise ContractError("renderer pose interpolation is unavailable")
            return CommandResult(
                HTTPStatus.OK,
                session.interpolation.diagnostics().response(),
            )
        if operation == "get_layer":
            status = self._layers.status(command.session_id)
            if status is None:
                return CommandResult(HTTPStatus.NO_CONTENT)
            return CommandResult(HTTPStatus.OK, status)
        if operation == "delete_camera":
            assert command.resource_id is not None
            self._cameras.close(command.resource_id)
            self._streams = {
                stream_id: stream
                for stream_id, stream in self._streams.items()
                if stream.camera_id != command.resource_id
            }
            return CommandResult(HTTPStatus.NO_CONTENT)
        if operation == "put_stream":
            assert isinstance(command.value, StreamBinding)
            binding = command.value
            if (
                self._cameras.camera_for_slot(binding.render_slot) != binding.camera_id
                or binding.media_port
                != self._config.media_port_base + binding.render_slot
            ):
                raise ContractError(
                    "stream does not match its camera or physical media slot"
                )
            existing = self._streams.get(binding.stream_product_id)
            if existing is not None and existing != binding:
                raise ContractError("renderer stream identity is immutable")
            self._streams[binding.stream_product_id] = binding
            status = self._cameras.status(binding.camera_id)
            return CommandResult(
                HTTPStatus.OK,
                {
                    "streamProductId": binding.stream_product_id,
                    "ready": status["ready"],
                    "signalPort": (
                        self._config.signaling_port_base + binding.render_slot
                    ),
                    "mediaPort": binding.media_port,
                    "lastPoseSequence": status["lastPoseSequence"],
                    "lastFrameAt": status["lastFrameAt"],
                },
            )
        if operation == "delete_stream":
            assert command.resource_id is not None
            self._streams.pop(command.resource_id, None)
            return CommandResult(HTTPStatus.NO_CONTENT)
        raise ContractError("renderer operation is unsupported")

    def _session(self, session_id: str) -> SessionRuntime:
        session = self._sessions.get(session_id)
        if session is None:
            raise ContractError("renderer session does not exist")
        return session


def simulation_app_settings(config: RendererConfig) -> dict[str, object]:
    return {
        "headless": True,
        "renderer": "RaytracedLighting",
        "width": config.probe_width,
        "height": config.probe_height,
        # A streamed world is an asynchronous data plane. Waiting for every
        # tile/material load inside SimulationApp.update() stops pose and
        # camera interpolation and turns normal provider latency into visible
        # frame hitches.
        "sync_loads": False,
        "extra_args": [
            "--ext-folder",
            "/opt/veoveo/extensions",
            "--enable",
            "cesium.usd.plugins",
            "--enable",
            "omni.kit.livestream.webrtc",
            "--enable",
            "omni.kit.livestream.aov",
            *livestream_aov_arguments(config),
            "--portable-root",
            str(config.cache_directory / "kit-portable"),
        ],
    }


def run(config: RendererConfig) -> None:
    config.prepare_directories()
    gpu = verify_nvidia_gpu_and_nvenc()
    LOGGER.info(
        "NVIDIA renderer admitted: gpu=%s uuid=%s driver=%s nvenc_api=%s",
        gpu.name,
        gpu.uuid,
        gpu.driver_version,
        gpu.nvenc_api_version,
    )

    from isaacsim import SimulationApp

    simulation_app = SimulationApp(simulation_app_settings(config))

    commands: queue.Queue[ControlCommand] = queue.Queue(maxsize=128)
    readiness = ReadinessSlot()
    server = ControlServer(config, commands, readiness)
    renderer: Renderer | None = None
    material_status: CesiumMaterialStatus | None = None
    server.start()
    try:
        import carb.settings
        import omni.kit.app
        import omni.usd

        settings = carb.settings.get_settings()
        configure_headless_cesium_extension(settings)
        manager = omni.kit.app.get_app().get_extension_manager()
        manager.add_path("/opt/veoveo/extensions")
        for extension in (
            "cesium.usd.plugins",
            "cesium.omniverse",
            "omni.kit.hydra_texture",
            "omni.kit.livestream.webrtc",
            "omni.kit.livestream.aov",
        ):
            manager.set_extension_enabled_immediate(extension, True)
            if not manager.is_extension_enabled(extension):
                raise RendererInitializationError(
                    RendererFailure(
                        RendererFailureCode.REQUIRED_EXTENSION_MISSING,
                        "a required renderer extension is unavailable",
                    )
                )
        disable_render_product_grid(settings)
        suppress_interactive_cesium_viewport_updates(manager)
        material_status = ensure_cesium_material_runtime(settings)
        _verify_stream_settings(settings, config)
        context = omni.usd.get_context()
        context.new_stage()
        simulation_app.update()
        stage = context.get_stage()
        diagnostics = DiagnosticScene(stage)
        renderer = Renderer(config, stage, commands, diagnostics)
        announce_runtime_generation(config, renderer.generation)

        while simulation_app.is_running():
            renderer.tick()
            simulation_app.update()
            (
                product_ready,
                color_pipeline_ready,
                visible,
                pipeline_failure,
            ) = renderer.readiness()
            streamed_world_ready = renderer.streamed_world_ready()
            governed_lighting_ready = renderer.governed_lighting_ready()
            failure = pipeline_failure
            if not governed_lighting_ready and failure is None:
                failure = RendererFailure(
                    RendererFailureCode.DIAGNOSTIC_LIGHT_ISOLATION_FAILED,
                    "diagnostic lighting isolation did not take effect",
                )
            ready = (
                product_ready
                and color_pipeline_ready
                and visible
                and streamed_world_ready
                and governed_lighting_ready
                and failure is None
            )
            readiness.set(
                Readiness(
                    ready=ready,
                    hardware_accelerated=True,
                    nvidia=True,
                    render_product_ready=product_ready,
                    nvenc_ready=True,
                    visible_non_stale_frame=visible,
                    streamed_world_ready=streamed_world_ready,
                    cesium_mdl_ready=(
                        material_status.mdl_assets_ready
                        and material_status.material_search_path_ready
                        and material_status.material_allowlist_ready
                    ),
                    cesium_tangent_frames_ready=(material_status.tangent_frame_ready),
                    governed_lighting_ready=governed_lighting_ready,
                    color_pipeline_ready=color_pipeline_ready,
                    failure=failure,
                )
            )
    except RendererInitializationError as error:
        LOGGER.error("renderer initialization failed: %s", error)
        readiness.set(Readiness(failure=error.failure))
        _serve_failed_runtime(simulation_app)
    except Exception:
        LOGGER.exception("renderer runtime failed")
        readiness.set(
            Readiness(
                failure=RendererFailure(
                    RendererFailureCode.RENDERER_INITIALIZATION_FAILED,
                    "renderer initialization or runtime failed",
                )
            )
        )
        _serve_failed_runtime(simulation_app)
    finally:
        server.close()
        if renderer is not None:
            renderer.close()
        simulation_app.close()


def _serve_failed_runtime(simulation_app: Any) -> None:
    while simulation_app.is_running():
        simulation_app.update()


def _verify_stream_settings(settings: Any, config: RendererConfig) -> None:
    for slot in range(config.maximum_render_slots):
        prefix = (
            "/exts/omni.kit.livestream.aov/"
            "Render.OmniverseKit.HydraTextures."
            f"{render_product_name(slot)}.LdrColor/spectatorStream/0"
        )
        expected = {
            "streamType": "webrtc",
            "signalPort": config.signaling_port_base + slot,
            "streamPort": config.media_port_base + slot,
            "publicIp": config.public_media_ip,
        }
        for name, value in expected.items():
            actual = settings.get(f"{prefix}/{name}")
            if actual != value:
                raise RuntimeError(
                    f"NVIDIA AOV setting {prefix}/{name} is {actual!r}, expected {value!r}"
                )


def _scene(session: SessionRuntime) -> SceneBinding:
    if session.scene is None:
        raise ContractError("renderer session has no scene")
    return session.scene


def _accepted() -> CommandResult:
    return CommandResult(HTTPStatus.OK, {"accepted": True})


# Imported late by _verify_stream_settings to keep pre-Kit imports provider-neutral.
from .camera import render_product_name  # noqa: E402
