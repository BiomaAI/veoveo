from __future__ import annotations

import logging
import queue
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
from .pose import PoseMirror, PoseSnapshot
from .scene import ArtifactStore, SceneManager


LOGGER = logging.getLogger("veoveo.simulation_view")


@dataclass(slots=True)
class SessionRuntime:
    binding: SessionBinding
    scene: SceneBinding | None = None
    pose: PoseMirror | None = None


class Renderer:
    def __init__(
        self,
        config: RendererConfig,
        stage: Any,
        commands: queue.Queue[ControlCommand],
    ) -> None:
        self._config = config
        self._commands = commands
        self._sessions: dict[str, SessionRuntime] = {}
        self._streams: dict[str, StreamBinding] = {}
        self._scenes = SceneManager(
            stage,
            ArtifactStore(
                config.artifact_directory, config.cache_directory
            ),
        )
        self._cameras = CameraPool(stage, config)

    def tick(self) -> None:
        self._process_commands()
        snapshots: dict[str, PoseSnapshot] = {}
        stale_sessions: set[str] = set()
        for session_id, session in self._sessions.items():
            if session.pose is None:
                continue
            snapshot = session.pose.poll()
            if snapshot is not None:
                self._scenes.apply_pose(snapshot)
            if session.pose.latest is not None and session.pose.stale:
                stale_sessions.add(session_id)
                self._scenes.mark_pose_stale(session_id)
            elif session.pose.latest is not None:
                snapshots[session_id] = session.pose.latest
        self._cameras.tick(snapshots, stale_sessions)

    def readiness(self) -> tuple[bool, bool]:
        return self._cameras.readiness()

    def close(self) -> None:
        for session in self._sessions.values():
            if session.pose is not None:
                session.pose.close()
        self._cameras.close_all()

    def _process_commands(self) -> None:
        for _ in range(128):
            try:
                command = self._commands.get_nowait()
            except queue.Empty:
                return
            try:
                result = self._execute(command)
            except ContractError as error:
                result = CommandResult(
                    HTTPStatus.CONFLICT, {"error": str(error)}
                )
            except BaseException:
                LOGGER.exception(
                    "renderer command failed: %s", command.operation
                )
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
            session = self._sessions.pop(command.session_id, None)
            if session is None:
                return CommandResult(HTTPStatus.NO_CONTENT)
            if session.pose is not None:
                session.pose.close()
            self._cameras.close_session(command.session_id)
            self._scenes.close(command.session_id)
            self._streams = {
                stream_id: stream
                for stream_id, stream in self._streams.items()
                if stream.session_id != command.session_id
            }
            return CommandResult(HTTPStatus.NO_CONTENT)
        session = self._session(command.session_id)
        if operation == "put_scene":
            assert isinstance(command.value, SceneBinding)
            if command.value.epoch_id != session.binding.epoch_id:
                raise ContractError("scene epoch does not match the session")
            self._scenes.bind(command.value)
            session.scene = command.value
            return _accepted()
        if operation == "put_pose_source":
            assert isinstance(command.value, PoseSourceBinding)
            scene = _scene(session)
            if (
                command.value.epoch_id != session.binding.epoch_id
                or command.value.frame_uri != scene.frame_uri
                or command.value.frame_digest != scene.frame_digest
            ):
                raise ContractError(
                    "pose source does not match the renderer scene"
                )
            mirror = PoseMirror(self._config.pose_directory)
            mirror.bind(command.value)
            if session.pose is not None:
                session.pose.close()
            session.pose = mirror
            return _accepted()
        if operation == "delete_pose_source":
            if session.pose is not None:
                session.pose.revoke()
                session.pose = None
            return CommandResult(HTTPStatus.NO_CONTENT)
        if operation == "put_camera":
            _scene(session)
            assert isinstance(command.value, CameraBinding)
            self._streams = {
                stream_id: stream
                for stream_id, stream in self._streams.items()
                if stream.camera_id != command.value.camera_id
            }
            return CommandResult(
                HTTPStatus.OK, self._cameras.upsert(command.value)
            )
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
                self._cameras.camera_for_slot(binding.render_slot)
                != binding.camera_id
                or binding.media_port
                != self._config.media_port_base + binding.render_slot
            ):
                raise ContractError(
                    "stream does not match its camera or physical media slot"
                )
            existing = self._streams.get(binding.live_view_id)
            if existing is not None and existing != binding:
                raise ContractError("renderer stream identity is immutable")
            self._streams[binding.live_view_id] = binding
            status = self._cameras.status(binding.camera_id)
            return CommandResult(
                HTTPStatus.OK,
                {
                    "liveViewId": binding.live_view_id,
                    "ready": status["ready"],
                    "signalPort": (
                        self._config.signaling_port_base
                        + binding.render_slot
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

    simulation_app = SimulationApp(
        {
            "headless": True,
            "renderer": "RaytracedLighting",
            "width": config.probe_width,
            "height": config.probe_height,
            "sync_loads": True,
            "extra_args": [
                "--enable",
                "omni.kit.livestream.webrtc",
                "--enable",
                "omni.kit.livestream.aov",
                *livestream_aov_arguments(config),
                "--portable-root",
                str(config.cache_directory / "kit-portable"),
            ],
        }
    )

    commands: queue.Queue[ControlCommand] = queue.Queue(maxsize=128)
    readiness = ReadinessSlot()
    server = ControlServer(config, commands, readiness)
    renderer: Renderer | None = None
    try:
        import carb.settings
        import omni.kit.app
        import omni.usd

        manager = omni.kit.app.get_app().get_extension_manager()
        for extension in (
            "omni.kit.hydra_texture",
            "omni.kit.livestream.webrtc",
            "omni.kit.livestream.aov",
        ):
            manager.set_extension_enabled_immediate(extension, True)
            if not manager.is_extension_enabled(extension):
                raise RuntimeError(
                    f"required renderer extension {extension} is unavailable"
                )
        _verify_stream_settings(carb.settings.get_settings(), config)
        context = omni.usd.get_context()
        context.new_stage()
        simulation_app.update()
        stage = context.get_stage()
        _create_diagnostic_scene(stage)
        renderer = Renderer(config, stage, commands)
        server.start()

        while simulation_app.is_running():
            renderer.tick()
            simulation_app.update()
            product_ready, visible = renderer.readiness()
            ready = product_ready and visible
            readiness.set(
                Readiness(
                    ready=ready,
                    hardware_accelerated=True,
                    nvidia=True,
                    render_product_ready=product_ready,
                    nvenc_ready=True,
                    visible_non_stale_frame=visible,
                )
            )
    except BaseException:
        readiness.set(Readiness())
        raise
    finally:
        server.close()
        if renderer is not None:
            renderer.close()
        simulation_app.close()


def _create_diagnostic_scene(stage: Any) -> None:
    from pxr import Gf, UsdGeom, UsdLux

    stage.DefinePrim("/World/SimulationView", "Xform")
    stage.DefinePrim("/World/SimulationView/Sessions", "Scope")
    stage.DefinePrim("/World/SimulationView/Cameras", "Scope")
    stage.DefinePrim("/World/SimulationView/Diagnostics", "Xform")
    cube = UsdGeom.Cube.Define(
        stage, "/World/SimulationView/Diagnostics/Cube"
    )
    cube.CreateSizeAttr(2.0)
    cube.CreateDisplayColorAttr([Gf.Vec3f(0.04, 0.65, 0.85)])
    ground = UsdGeom.Cube.Define(
        stage, "/World/SimulationView/Diagnostics/Ground"
    )
    ground.CreateSizeAttr(1.0)
    ground_xform = UsdGeom.Xformable(ground.GetPrim())
    ground_xform.AddScaleOp().Set(Gf.Vec3f(18.0, 18.0, 0.1))
    ground_xform.AddTranslateOp().Set(Gf.Vec3d(0.0, 0.0, -1.05))
    ground.CreateDisplayColorAttr([Gf.Vec3f(0.08, 0.1, 0.14)])
    sun = UsdLux.DistantLight.Define(
        stage, "/World/SimulationView/Diagnostics/Sun"
    )
    sun.CreateIntensityAttr(3_000.0)
    UsdGeom.Xformable(sun.GetPrim()).AddRotateXYZOp().Set(
        Gf.Vec3f(35.0, -25.0, -30.0)
    )
    dome = UsdLux.DomeLight.Define(
        stage, "/World/SimulationView/Diagnostics/Dome"
    )
    dome.CreateIntensityAttr(500.0)


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
