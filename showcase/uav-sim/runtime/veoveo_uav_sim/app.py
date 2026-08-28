from __future__ import annotations

import concurrent.futures
import logging
import time
from collections.abc import Callable
from pathlib import Path

from .config import RuntimeConfig
from .hydra_camera import native_sensor_aov_arguments
from .operator_products import operator_aov_arguments
from .physical_camera import physical_camera_product_name

LOGGER = logging.getLogger("veoveo.uav_sim")

RTX_TONEMAP_IRAY = 7
DAYLIGHT_SKY_INTENSITY = 1000.0
DAYLIGHT_SKY_EXPOSURE_STOPS = -0.5
DAYLIGHT_SKY_TEMPERATURE_K = 6500.0
DAYLIGHT_SUN_INTENSITY = 500.0
DAYLIGHT_SUN_EXPOSURE_STOPS = -0.75
DAYLIGHT_SUN_TEMPERATURE_K = 5500.0


def kit_live_render_arguments() -> list[str]:
    """Use measured native time while decoupling presentation threads."""
    settings = {
        "/app/runLoops/main/rateLimitEnabled": "false",
        "/app/player/useFixedTimeStepping": "false",
        "/app/runLoops/main/syncToPresent": "false",
        "/app/runLoops/rendering_0/syncToPresent": "false",
        "/app/runLoops/rendering_1/syncToPresent": "false",
        "/app/runLoopsGlobal/syncToPresent": "false",
        "/exts/omni.kit.renderer.core/present/presentAfterRendering": "false",
    }
    return [f"--{path}={value}" for path, value in settings.items()]


def kit_newton_arguments() -> list[str]:
    """Register Newton before Kit creates any physics-backed application state."""
    return [
        "--enable",
        "isaacsim.physics.newton",
        "--/exts/isaacsim.core.simulation_manager/default_engine=newton",
    ]


def kit_rtpt_visual_arguments() -> list[str]:
    """Use one stable exposure across every region of the shared camera atlas."""
    settings = {
        "/rtx/post/histogram/enabled": "false",
        "/rtx/post/tonemap/op": str(RTX_TONEMAP_IRAY),
        "/rtx/post/tonemap/irayReinhard/crushBlacks": "0.0",
        "/rtx/post/tonemap/irayReinhard/burnHighlights": "0.35",
        "/rtx/post/tonemap/irayReinhard/burnHighlightsPerComponent": "false",
        "/rtx/post/tonemap/irayReinhard/burnHighlightsMaxComponent": "false",
        "/rtx/post/tonemap/irayReinhard/saturation": "1.0",
        "/rtx/post/tonemap/enableSrgbToGamma": "true",
        "/rtx/rtpt/fireflyFilter/enabled": "true",
    }
    return [f"--{path}={value}" for path, value in settings.items()]


def _cleanup(name: str, action: Callable[[], None]) -> None:
    try:
        action()
    except BaseException:
        LOGGER.exception("UAV simulation cleanup failed: %s", name)


def run(config: RuntimeConfig) -> None:
    # Isaac requires SimulationApp to exist before importing Kit or simulator modules.
    from isaacsim import SimulationApp

    physical_product_name = physical_camera_product_name(config.camera.vehicle_id)
    viewport_width = config.camera.width
    viewport_height = config.camera.height
    simulation_app = SimulationApp(
        {
            "headless": True,
            # The complete logical-camera set owns one tiled RTX product, so
            # Real-Time 2.0 lights the atlas without multiplying products or
            # encoder sessions. Two total bounces retain direct environment
            # light while bounding the five-region 720p atlas. DLSS preserves
            # native output resolution on the hardware-only renderer path.
            "renderer": "RealTimePathTracing",
            "anti_aliasing": 3,
            "max_bounces": 2,
            "max_specular_transmission_bounces": 1,
            "max_volume_bounces": 0,
            "width": viewport_width,
            "height": viewport_height,
            # A streamed 3D Tiles world never reaches a terminal "all assets
            # loaded" state. Synchronous USD/material loads would therefore
            # suspend live frame submission whenever Cesium admits more tiles.
            "sync_loads": False,
            # Cesium receives exact view/projection matrices from the actual
            # sensor and operator Hydra products. Rendering a separate headless
            # UI viewport would duplicate the nadir camera on the same GPU.
            "disable_viewport_updates": True,
            # Cesium's native USD schema plugin must be discovered before Kit
            # initializes USD's schema registry. Enabling it after
            # SimulationApp starts leaves the generated attributes untyped.
            # The portable root keeps every Kit write in the pod's selected
            # versioned runtime-cache directory when running as a non-root user.
            "extra_args": [
                "--ext-folder",
                config.extension_directory,
                "--enable",
                "cesium.usd.plugins",
                "--/exts/cesium.omniverse/externallyManagedViewports=true",
                "--enable",
                "omni.kit.livestream.rtsp",
                *kit_newton_arguments(),
                *native_sensor_aov_arguments(
                    physical_product_name,
                    rtsp_port=config.camera.rtsp_port,
                    target_fps=config.camera.fps,
                ),
                *operator_aov_arguments(config.operator_live_view),
                (
                    "--/rtx/viewTile/limit="
                    f"{len(config.operator_live_view.streamable_cameras)}"
                ),
                *kit_rtpt_visual_arguments(),
                *kit_live_render_arguments(),
                "--portable-root",
                str(config.cache_directory / "kit-portable"),
            ],
        }
    )

    import isaacsim.physics.newton
    import omni.kit.app
    import omni.timeline
    import omni.usd
    from isaacsim.core.simulation_manager import SimulationManager
    from pxr import Gf, Usd, UsdGeom, UsdLux

    extension_manager = omni.kit.app.get_app().get_extension_manager()
    extension_manager.add_path(config.extension_directory)
    for extension in (
        "cesium.usd.plugins",
        "cesium.omniverse",
        "isaacsim.physics.newton",
        "isaacsim.core.simulation_manager",
        "isaacsim.core.experimental.prims",
        "isaacsim.core.experimental.objects",
        "isaacsim.core.experimental.materials",
        "isaacsim.core.experimental.utils",
        "omni.kit.livestream.rtsp",
    ):
        extension_manager.set_extension_enabled_immediate(extension, True)
        if not extension_manager.is_extension_enabled(extension):
            raise RuntimeError(f"failed to enable required extension {extension}")

    from cesium.omniverse.bindings import (
        Viewport as CesiumViewport,
    )
    from cesium.omniverse.bindings import (
        acquire_cesium_omniverse_interface,
    )
    from cesium.omniverse.usdUtils import (
        add_tileset_ion,
        get_or_create_cesium_data,
        get_or_create_cesium_georeference,
    )
    from cesium.usd.plugins.CesiumUsdSchemas import (
        IonServer as CesiumIonServer,
    )
    from cesium.usd.plugins.CesiumUsdSchemas import (
        Tileset as CesiumTileset,
    )

    from .adapter_server import AdapterServer
    from .cesium_camera import current_pose_cesium_viewport
    from .command_queue import MainThreadQueue
    from .fleet_loop import FleetLoopController
    from .fleet_runtime import WarpFleetRuntime
    from .hydra_camera import NativeH264CameraSensor
    from .operator_camera import (
        AuthoritativeOperatorCameraCollection,
        EntityTransform,
        Pose,
        QuaternionXyzw,
        Vector3,
        compose_pose,
    )
    from .operator_products import OperatorProductCollection
    from .physical_camera import create_physical_rgb_camera, physical_camera_path
    from .px4 import Px4Commander
    from .px4_hil import Px4HilFleet
    from .realtime import FixedStepCadenceGate, MonotonicPhysicsClock
    from .recording import ImuTelemetry, RecordingPublisher
    from .recording_segments import new_recording_key
    from .render_pose import rendered_pose_agreement
    from .runtime_events import (
        RuntimeEventPublisher,
        notify_adapter_ready,
        notify_runtime_ready,
    )
    from .scene import create_fleet_scene
    from .server import (
        AdapterApplication,
        PreconfigurationApplication,
        TimelineControls,
    )
    from .state import RuntimeState, VehicleTelemetry
    from .stream_output import StreamPublicationWorker
    from .tile_lifecycle import (
        NativeTileEventBridge,
        TileLifecycleController,
        TileRenderStatistics,
        begin_provider_session_replacement,
        tile_content_ready,
    )
    from .world_config import WorldConfiguration, WorldConfigurationSlot

    state: RuntimeState | None = None
    world_config: WorldConfiguration | None = None
    world_slot = WorldConfigurationSlot()
    command_queue = MainThreadQueue()
    recording: RecordingPublisher | None = None
    stream_publication: StreamPublicationWorker | None = None
    server: AdapterServer | None = None
    connection_executor: concurrent.futures.ThreadPoolExecutor | None = None
    tileset_path: str | None = None
    tileset_paths: set[str] = set()
    physics_step = 0
    simulation_time_s = 0.0
    commanders: dict[str, Px4Commander] = {}
    vehicle_ids = tuple(f"uav-{index + 1}" for index in range(config.vehicle_count))
    camera_sensors: dict[str, NativeH264CameraSensor] = {}
    camera_sensor_sequences: dict[str, int] = {}
    camera_frames_observed: dict[str, int] = {}
    fleet_runtime: WarpFleetRuntime | None = None
    hil_fleet: Px4HilFleet | None = None
    simulation_running = True
    fleet_loop: FleetLoopController | None = None
    operator_cameras: AuthoritativeOperatorCameraCollection | None = None
    operator_products: OperatorProductCollection | None = None
    simulation_generation = 1
    tile_event_bridge: NativeTileEventBridge | None = None
    tile_controller: TileLifecycleController | None = None
    physics_timeline = None
    runtime_events = RuntimeEventPublisher()

    try:
        preconfiguration = PreconfigurationApplication(
            config, world_slot, runtime_events
        )
        server = AdapterServer(config, preconfiguration.application)
        server.start()
        notify_adapter_ready(
            runtime_events,
            session_id=config.session_id,
            generation=simulation_generation,
        )
        LOGGER.info(
            "UAV simulation session %s is waiting for an immutable Frames world revision",
            config.session_id,
        )
        while simulation_app.is_running() and world_config is None:
            world_config = world_slot.wait(0.05)
            simulation_app.update()
        if world_config is None:
            raise RuntimeError(
                "Isaac SimulationApp stopped before a frame world was configured"
            )
        recording_key = new_recording_key()
        state = RuntimeState(config, world_config, recording_key)
        recording = RecordingPublisher(config, world_config, recording_key)
        if config.stream_publication is not None:
            stream_publication = StreamPublicationWorker(config.stream_publication)
        if not SimulationManager.switch_physics_engine("newton", verbose=True):
            raise RuntimeError("Isaac Sim did not activate the Newton physics engine")
        if SimulationManager.get_active_physics_engine() != "newton":
            raise RuntimeError("Newton is not the active Isaac Sim physics engine")
        SimulationManager.setup_simulation(dt=1.0 / config.physics_hz, device="cuda:0")
        newton_stage = isaacsim.physics.newton.acquire_stage()
        if newton_stage is None:
            raise RuntimeError("Isaac Sim did not expose the active Newton stage")
        newton_stage.cfg.time_step_app = False
        if newton_stage.cfg.solver_cfg.solver_type != "mujoco":
            raise RuntimeError("UAV fleet requires the MuJoCo-Warp Newton solver")
        newton_stage.cfg.num_substeps = 1
        newton_stage.cfg.use_cuda_graph = False
        newton_stage.cfg.solver_cfg.iterations = 1
        newton_stage.cfg.solver_cfg.ls_iterations = 1
        newton_stage.cfg.solver_cfg.integrator = "euler"
        newton_stage.cfg.solver_cfg.disable_contacts = True
        newton_stage.cfg.solver_cfg.use_mujoco_contacts = False
        newton_stage.cfg.solver_cfg.njmax = 1
        newton_stage.cfg.solver_cfg.nconmax = 0
        physics_timeline = omni.timeline.get_timeline_interface()
        # Newton owns the authoritative clock. Kit remains in manual mode and
        # advances only render products and extension work.
        from omni.kit.loop import _loop as omni_loop

        loop_runner = omni_loop.acquire_loop_interface()
        loop_runner.set_manual_mode(True)

        stage = omni.usd.get_context().get_stage()
        fleet_scene = create_fleet_scene(
            stage,
            Path("/opt/veoveo/uav-sim/assets/iris.usda"),
            config.vehicle_count,
        )
        stage.DefinePrim("/World/Environment", "Xform")
        sky = UsdLux.DomeLight.Define(stage, "/World/Environment/Sky")
        sky.CreateIntensityAttr(DAYLIGHT_SKY_INTENSITY)
        sky.CreateExposureAttr(DAYLIGHT_SKY_EXPOSURE_STOPS)
        sky.CreateEnableColorTemperatureAttr(True)
        sky.CreateColorTemperatureAttr(DAYLIGHT_SKY_TEMPERATURE_K)
        sun = UsdLux.DistantLight.Define(stage, "/World/Environment/Sun")
        sun.CreateIntensityAttr(DAYLIGHT_SUN_INTENSITY)
        sun.CreateExposureAttr(DAYLIGHT_SUN_EXPOSURE_STOPS)
        sun.CreateEnableColorTemperatureAttr(True)
        sun.CreateColorTemperatureAttr(DAYLIGHT_SUN_TEMPERATURE_K)
        sun.CreateAngleAttr(0.53)
        UsdGeom.Xformable(sun.GetPrim()).AddRotateXYZOp().Set(
            Gf.Vec3f(-45.0, -45.0, 0.0)
        )

        previous_target = stage.GetEditTarget()
        # The token is authored only into the anonymous session layer required by
        # Cesium's runtime schema. It is cleared on shutdown and never exported.
        stage.SetEditTarget(Usd.EditTarget(stage.GetSessionLayer()))
        try:
            # The interactive Cesium extension normally creates this typed
            # server prim from its USD stage-opened callback. SimulationApp's
            # stage is already open when this headless runtime enables the
            # extension, so author the same official ion endpoint explicitly.
            # Without the binding, Cesium deliberately creates an inert asset-0
            # tileset because its ion API URL is empty.
            ion_server_path = "/CesiumServers/IonOfficial"
            ion_server = CesiumIonServer.Define(stage, ion_server_path)
            ion_server.GetDisplayNameAttr().Set("ion.cesium.com")
            ion_server.GetIonServerUrlAttr().Set("https://ion.cesium.com/")
            ion_server.GetIonServerApiUrlAttr().Set("https://api.cesium.com/")
            ion_server.GetIonServerApplicationIdAttr().Set(413)
            cesium_data = get_or_create_cesium_data()
            cesium_data.GetSelectedIonServerRel().SetTargets([ion_server_path])

            georeference = get_or_create_cesium_georeference()
            georeference.GetGeoreferenceOriginLatitudeAttr().Set(
                world_config.georeference_origin.latitude_degrees
            )
            georeference.GetGeoreferenceOriginLongitudeAttr().Set(
                world_config.georeference_origin.longitude_degrees
            )
            georeference.GetGeoreferenceOriginHeightAttr().Set(
                world_config.georeference_origin.ellipsoid_height_m
            )
        finally:
            stage.SetEditTarget(previous_target)

        def author_google_tileset(name: str) -> str:
            previous_author_target = stage.GetEditTarget()
            stage.SetEditTarget(Usd.EditTarget(stage.GetSessionLayer()))
            try:
                authored_path = add_tileset_ion(
                    name,
                    config.cesium_ion_asset_id,
                    config.cesium_ion_access_token,
                )
                tileset = CesiumTileset.Get(stage, authored_path)
                if not tileset.GetPrim().IsValid():
                    raise RuntimeError(
                        "Cesium did not create the governed tileset prim"
                    )
                tileset.GetMaximumScreenSpaceErrorAttr().Set(
                    config.tile_streaming.maximum_screen_space_error
                )
                tileset.GetMaximumSimultaneousTileLoadsAttr().Set(
                    config.tile_streaming.maximum_simultaneous_loads
                )
                tileset.GetMaximumCachedBytesAttr().Set(
                    config.tile_streaming.maximum_cached_bytes
                )
                tileset.GetPreloadAncestorsAttr().Set(
                    config.tile_streaming.preload_ancestors
                )
                tileset.GetPreloadSiblingsAttr().Set(
                    config.tile_streaming.preload_siblings
                )
                tileset.GetForbidHolesAttr().Set(config.tile_streaming.forbid_holes)
                tileset_paths.add(authored_path)
                return authored_path
            finally:
                stage.SetEditTarget(previous_author_target)

        def retire_google_tileset(retired_path: str) -> None:
            previous_retire_target = stage.GetEditTarget()
            stage.SetEditTarget(Usd.EditTarget(stage.GetSessionLayer()))
            try:
                retired_tileset = CesiumTileset.Get(stage, retired_path)
                if retired_tileset.GetPrim().IsValid():
                    retired_tileset.GetIonAccessTokenAttr().Clear()
                    if not stage.RemovePrim(retired_path):
                        raise RuntimeError(
                            f"failed to retire Cesium tileset {retired_path}"
                        )
                tileset_paths.discard(retired_path)
            finally:
                stage.SetEditTarget(previous_retire_target)

        tileset_path = author_google_tileset("Google_Photorealistic_3D_Tiles")

        mount_w, mount_x, mount_y, mount_z = config.camera.mount.orientation_wxyz
        sensor_mount_pose = Pose(
            Vector3(*config.camera.mount.translation_xyz_m),
            QuaternionXyzw(mount_x, mount_y, mount_z, mount_w),
        ).normalized()

        physical_cameras = {}
        for index in range(config.vehicle_count):
            vehicle_id = f"uav-{index + 1}"
            commander = Px4Commander(
                index, world_config.georeference_origin.ellipsoid_height_m
            )
            commanders[vehicle_id] = commander

            if vehicle_id == config.camera.vehicle_id:
                camera_path = physical_camera_path(vehicle_id)
                physical_cameras[vehicle_id] = create_physical_rgb_camera(
                    stage,
                    path=camera_path,
                    mount_pose=sensor_mount_pose,
                    focal_length_mm=config.camera.focal_length_mm,
                    width_px=config.camera.width,
                    height_px=config.camera.height,
                    clipping_near_m=config.camera.clipping_near_m,
                    clipping_far_m=config.camera.clipping_far_m,
                )
                camera_sensors[vehicle_id] = NativeH264CameraSensor(
                    name=physical_product_name,
                    camera_path=camera_path,
                    width=config.camera.width,
                    height=config.camera.height,
                    render_fps=config.camera.fps,
                    rtsp_port=config.camera.rtsp_port,
                )
                camera_sensor_sequences[vehicle_id] = 0
                camera_frames_observed[vehicle_id] = 0

        if not camera_sensors:
            raise RuntimeError("Cesium requires an authoritative sensor camera")
        operator_cameras = AuthoritativeOperatorCameraCollection.create(
            config.operator_live_view.cameras,
            stage,
        )
        operator_products = OperatorProductCollection.create(
            config.operator_live_view,
            operator_cameras,
        )
        extension_manager.set_extension_enabled_immediate(
            "omni.kit.livestream.aov", True
        )
        if not extension_manager.is_extension_enabled("omni.kit.livestream.aov"):
            raise RuntimeError(
                "failed to enable required extension omni.kit.livestream.aov"
            )
        state.update_stream_products(operator_products.state(content_ready=False))
        physics_timeline.play()
        simulation_app.update()
        SimulationManager.initialize_physics()
        if SimulationManager.get_active_physics_engine() != "newton":
            raise RuntimeError("Newton changed during physics initialization")
        if not physics_timeline.is_playing() or not newton_stage.playing:
            raise RuntimeError("Newton timeline did not enter the playing state")

        recording_cadence = FixedStepCadenceGate(
            config.physics_hz, config.recording.telemetry_hz
        )
        physics_clock = MonotonicPhysicsClock(
            config.physics_hz,
            maximum_steps_per_pass=config.physics_hz,
        )
        render_cadence = FixedStepCadenceGate(config.physics_hz, config.rendering_hz)

        def telemetry_snapshot() -> list[VehicleTelemetry]:
            assert fleet_runtime is not None
            telemetry: list[VehicleTelemetry] = []
            for vehicle_id, vehicle_state in zip(
                vehicle_ids, fleet_runtime.snapshots(), strict=True
            ):
                px4_status = commanders[vehicle_id].status()
                telemetry.append(
                    VehicleTelemetry(
                        vehicle_id=vehicle_id,
                        position_enu=vehicle_state.position_enu_m,
                        attitude_xyzw=vehicle_state.attitude_xyzw,
                        linear_velocity_enu_mps=vehicle_state.linear_velocity_enu_mps,
                        flight_state=px4_status.flight_state,
                        battery_percent=px4_status.battery_percent,
                        px4_connected=px4_status.connected,
                    )
                )
            return telemetry

        def operator_entity_transforms() -> dict[str, EntityTransform]:
            assert fleet_runtime is not None
            return {
                vehicle_id: EntityTransform(
                    vehicle_id,
                    Pose(
                        Vector3(*vehicle_state.position_enu_m),
                        QuaternionXyzw(*vehicle_state.attitude_xyzw).normalized(),
                    ),
                )
                for vehicle_id, vehicle_state in zip(
                    vehicle_ids, fleet_runtime.snapshots(), strict=True
                )
            }

        def update_operator_cameras(now: float | None = None) -> None:
            assert operator_cameras is not None
            assert operator_products is not None
            entities = operator_entity_transforms()
            source_monotonic_seconds = time.monotonic() if now is None else now
            for vehicle_id, physical_camera in physical_cameras.items():
                physical_camera.update(entities[vehicle_id].pose)
            operator_cameras.update(
                entities,
                simulation_generation=simulation_generation,
                physics_step=physics_step,
                monotonic_seconds=source_monotonic_seconds,
            )
            operator_products.sync_camera_poses(
                {
                    camera.definition.camera_id: camera.last_pose
                    for camera in operator_cameras.cameras
                    if camera.last_pose is not None
                },
                source_monotonic_seconds=source_monotonic_seconds,
            )

        def advance_physics(_dt: float) -> None:
            nonlocal physics_step, simulation_time_s
            physics_step += 1
            simulation_time_s = physics_step / config.physics_hz
            state.advance(simulation_time_s, physics_step)
            publish_recording = recording_cadence.due(physics_step)
            if not publish_recording:
                return
            telemetry = telemetry_snapshot()
            state.update_vehicles(telemetry)
            recording.offer_frame(
                telemetry,
                [
                    ImuTelemetry(
                        vehicle_id=vehicle_id,
                        linear_acceleration_mps2=(
                            vehicle_state.linear_acceleration_frd_mps2
                        ),
                        angular_velocity_rps=vehicle_state.angular_velocity_frd_rps,
                    )
                    for vehicle_id, vehicle_state in zip(
                        vehicle_ids, fleet_runtime.snapshots(), strict=True
                    )
                ],
                simulation_time_s,
                physics_step,
            )

        hil_fleet = Px4HilFleet(config.px4_directory, config.vehicle_count)
        fleet_runtime = WarpFleetRuntime(
            fleet_scene.body_paths,
            fleet_scene.initial_positions_enu_m,
            world_config.georeference_origin.latitude_degrees,
            world_config.georeference_origin.longitude_degrees,
            world_config.georeference_origin.ellipsoid_height_m,
            config.physics_hz,
            hil_fleet,
            after_step=advance_physics,
        )
        hil_fleet.start()
        LOGGER.info(
            "Newton CUDA UAV fleet ready: bodies=%d device=%s",
            fleet_runtime.body_count,
            fleet_runtime.device,
        )

        def pause() -> None:
            def action() -> None:
                nonlocal simulation_running
                simulation_running = False
                physics_clock.reset(physics_step)
                state.set_lifecycle("paused")

            command_queue.submit(action)

        def resume() -> None:
            def action() -> None:
                nonlocal simulation_running
                physics_clock.reset(physics_step)
                simulation_running = True
                state.set_lifecycle("running")

            command_queue.submit(action)

        def reset() -> None:
            def action() -> None:
                nonlocal physics_step, simulation_time_s, simulation_generation
                assert fleet_runtime is not None
                was_running = simulation_running
                fleet_runtime.reset()
                physics_step = 0
                simulation_time_s = 0.0
                simulation_generation += 1
                recording_cadence.reset()
                physics_clock.reset(physics_step)
                render_cadence.reset(physics_step)
                state.advance(simulation_time_s, physics_step)
                state.set_lifecycle("running" if was_running else "paused")

            command_queue.submit(action)

        def step(steps: int) -> None:
            def action() -> None:
                nonlocal simulation_running
                assert fleet_runtime is not None
                simulation_running = False
                for _ in range(steps):
                    fleet_runtime.step(physics_step + 1)
                update_operator_cameras()
                simulation_app.update()
                physics_clock.reset(physics_step)
                render_cadence.reset(physics_step)
                state.advance(simulation_time_s, physics_step)
                state.set_lifecycle("paused")

            command_queue.submit(action)

        fleet_loop = FleetLoopController(
            config.fleet_loop,
            world_config.georeference_origin,
            commanders,
        )
        application = AdapterApplication(
            config,
            state,
            TimelineControls(pause=pause, resume=resume, reset=reset, step=step),
            commanders,
            recording,
            world_slot,
            fleet_loop,
            operator_products,
            runtime_events,
            command_queue.submit,
        )
        assert server is not None
        server.close()
        server = AdapterServer(config, application.application)
        server.start()

        connection_executor = concurrent.futures.ThreadPoolExecutor(
            max_workers=config.vehicle_count, thread_name_prefix="px4-connect"
        )
        connection_futures = {
            vehicle_id: connection_executor.submit(
                commander.connect, config.px4_connect_timeout_seconds
            )
            for vehicle_id, commander in commanders.items()
        }

        # The first RTX/Cesium render can compile shaders for longer than the
        # PX4 connection deadline. Advance physics without rendering until the
        # Simulator MAVLink and GCS handshakes are complete, then let the normal
        # loop render Google Photorealistic 3D Tiles and camera frames.
        px4_bootstrap_deadline = (
            time.monotonic() + config.px4_connect_timeout_seconds + 15.0
        )
        while not all(future.done() for future in connection_futures.values()):
            assert fleet_runtime is not None
            fleet_runtime.step(physics_step + 1)
            for vehicle_id, future in connection_futures.items():
                if future.done() and future.exception() is not None:
                    raise RuntimeError(
                        f"PX4 connection failed for {vehicle_id}"
                    ) from future.exception()
            if time.monotonic() >= px4_bootstrap_deadline:
                raise TimeoutError("PX4 bootstrap did not complete before rendering")
            time.sleep(0.001)

        fleet_loop.start()

        cesium_interface = acquire_cesium_omniverse_interface()
        assert tileset_path is not None
        tile_event_bridge = NativeTileEventBridge()
        tile_controller = TileLifecycleController(
            tileset_path=tileset_path,
            ready_frames=config.tile_ready_frames,
            replacement_timeout_frames=config.rendering_hz * 120,
        )
        # The Cesium extension starts before this headless application authors
        # its runtime-only tileset. Rebind the completed stage through Cesium's
        # public lifecycle contract so its native asset registry enumerates the
        # session-layer Google tileset deterministically instead of depending
        # on a UI-era USD notice sequence.
        cesium_interface.on_stage_change(0)
        cesium_interface.on_stage_change(omni.usd.get_context().get_stage_id())

        def update_cesium_viewport() -> None:
            # Cesium's extension enumerates viewport *windows* on every Kit
            # update. Headless Isaac still has an active viewport API for the
            # UAV camera, but no window, so its automatic update submits an
            # empty list. Restore the sensor viewport after every Kit update,
            # using the same native frame contract as the extension.
            cesium_viewports = []
            for vehicle_id, sensor in camera_sensors.items():
                sensor_pose = physical_cameras[vehicle_id].last_pose
                if sensor_pose is None:
                    continue
                cesium_viewports.append(
                    current_pose_cesium_viewport(
                        stage,
                        sensor.camera_path,
                        sensor_pose,
                        config.camera.width,
                        config.camera.height,
                        CesiumViewport,
                    )
                )
            assert operator_products is not None
            assert operator_cameras is not None
            active_operator_cameras = set(operator_products.active_camera_ids())
            for camera in operator_cameras.cameras:
                if camera.definition.camera_id not in active_operator_cameras:
                    continue
                camera_pose = camera.last_pose
                if camera_pose is None:
                    continue
                cesium_viewports.append(
                    current_pose_cesium_viewport(
                        stage,
                        camera.camera_path,
                        camera_pose,
                        camera.definition.optics.width_px,
                        camera.definition.optics.height_px,
                        CesiumViewport,
                    )
                )
            cesium_interface.on_update_frame(cesium_viewports, False)

        runtime_ready_notified = False
        physics_clock.reset(physics_step)
        render_cadence.reset(physics_step)
        while simulation_app.is_running():
            assert fleet_loop is not None
            fleet_loop.raise_if_failed()
            command_queue.drain()
            render = False
            if simulation_running:
                due_steps = physics_clock.due_steps(physics_step)
                for _ in range(due_steps):
                    assert fleet_runtime is not None
                    fleet_runtime.step(physics_step + 1)
                    render = render_cadence.due(physics_step) or render
                if render:
                    render_cycle_started = time.monotonic()
                    # A slow GPU frame can make several fixed physics steps due.
                    # Coalesce their missed render opportunities into one frame
                    # from the newest authoritative state. Rendering therefore
                    # cannot make the simulation clock run slow.
                    update_operator_cameras()
                    for sensor in camera_sensors.values():
                        sensor.observe_simulation_time(simulation_time_s, physics_step)
                    update_cesium_viewport()
                    native_update_started = time.monotonic()
                    simulation_app.update()
                    native_update_wall_seconds = (
                        time.monotonic() - native_update_started
                    )
                    assert operator_products is not None
                    tile_state = state.snapshot()["tiles"]
                    state.update_stream_products(
                        operator_products.state(
                            content_ready=tile_content_ready(
                                lifecycle=tile_state["lifecycle"],
                                visible_tiles=tile_state["visible_tiles"],
                                geometries_rendered=tile_state["geometries_rendered"],
                                materials_loaded=tile_state["materials_loaded"],
                            )
                        )
                    )
                elif due_steps == 0:
                    time.sleep(
                        min(
                            physics_clock.seconds_until_next_step(physics_step),
                            0.005,
                        )
                    )
            else:
                simulation_app.update()
                update_cesium_viewport()
                time.sleep(0.005)
                continue

            if render:
                for vehicle_id, sensor in camera_sensors.items():
                    frame = sensor.latest_frame(
                        after_sequence=camera_sensor_sequences[vehicle_id]
                    )
                    if frame is not None:
                        camera_sensor_sequences[vehicle_id] = frame.sequence
                        entity_transforms = operator_entity_transforms()
                        expected_sensor_pose = compose_pose(
                            entity_transforms[vehicle_id].pose,
                            sensor_mount_pose,
                        )
                        render_pose = rendered_pose_agreement(
                            frame.rendered_camera,
                            expected_sensor_pose,
                        )
                        camera_frames_observed[vehicle_id] += 1
                        state.update_camera(
                            vehicle_id,
                            "ready",
                            camera_frames_observed[vehicle_id],
                            len(frame.access_unit.sample),
                            keyframe=frame.access_unit.is_keyframe,
                            render_pose=render_pose,
                        )
                        recording.offer_camera_access_unit(
                            frame.access_unit,
                            frame.simulation_time_s,
                            frame.physics_step,
                        )
                        if stream_publication is not None:
                            stream_publication.offer(
                                frame.access_unit,
                                frame.simulation_time_s,
                            )
                    else:
                        sensor_status = sensor.status()
                        if sensor_status.lifecycle == "degraded":
                            state.update_camera(
                                vehicle_id,
                                sensor_status.lifecycle,
                                sensor_status.frames_received,
                                0,
                                keyframe=False,
                                diagnostic=sensor_status.diagnostic,
                            )

            if render:
                assert tile_event_bridge is not None
                assert tile_controller is not None
                for tile_event in tile_event_bridge.drain():
                    tile_action = tile_controller.accept(tile_event)
                    if tile_action.report_failure:
                        log = (
                            LOGGER.warning
                            if tile_action.retained_textured_coverage
                            else LOGGER.error
                        )
                        log(
                            (
                                "streamed-world load failed: type=%s "
                                "status=%d generation=%d; simulation and resident "
                                "textured coverage continue=%s"
                            ),
                            tile_event.load_type,
                            tile_event.http_status,
                            tile_event.generation,
                            tile_action.retained_textured_coverage,
                        )
                    if tile_action.begin_replacement:
                        try:
                            replacement_path = begin_provider_session_replacement(
                                author_google_tileset,
                                (
                                    "Google_Photorealistic_3D_Tiles_refresh_"
                                    f"{tile_controller.snapshot().refresh_count}"
                                ),
                            )
                            tile_controller.replacement_started(replacement_path)
                            LOGGER.info(
                                "streamed-world replacement authored: %s; awaiting "
                                "native registration and textured coverage while the "
                                "resident generation remains mounted",
                                replacement_path,
                            )
                        except Exception:
                            tile_controller.mark_refresh_command_failed()
                            LOGGER.exception(
                                "streamed-world replacement creation failed; "
                                "resident generation remains mounted"
                            )
                    if tile_action.retire_tileset_path is not None:
                        try:
                            retire_google_tileset(tile_action.retire_tileset_path)
                            tileset_path = tile_controller.active_tileset_path
                            LOGGER.info(
                                "streamed-world failed replacement retired: %s",
                                tile_action.retire_tileset_path,
                            )
                        except Exception:
                            tile_controller.mark_refresh_command_failed()
                            LOGGER.exception(
                                "streamed-world obsolete generation retirement failed"
                            )
                statistics = cesium_interface.get_render_statistics()
                resident = int(statistics.tiles_loaded)
                visible = int(statistics.tiles_rendered)
                loading = int(statistics.tiles_loading_worker) + int(
                    statistics.tiles_loading_main
                )
                tile_observation = tile_controller.observe_render(
                    TileRenderStatistics(
                        resident_tiles=resident,
                        visible_tiles=visible,
                        loading_tiles=loading,
                        geometries_loaded=int(statistics.geometries_loaded),
                        geometries_rendered=int(statistics.geometries_rendered),
                        materials_loaded=int(statistics.materials_loaded),
                    )
                )
                if tile_observation.action.report_failure:
                    LOGGER.error(
                        "streamed-world replacement failed to prove textured coverage"
                    )
                if tile_observation.action.retire_tileset_path is not None:
                    try:
                        retire_google_tileset(
                            tile_observation.action.retire_tileset_path
                        )
                        tileset_path = tile_controller.active_tileset_path
                        LOGGER.info(
                            "streamed-world textured replacement promoted; retired %s",
                            tile_observation.action.retire_tileset_path,
                        )
                    except Exception:
                        tile_controller.mark_refresh_command_failed()
                        LOGGER.exception("streamed-world generation retirement failed")
                tile_health = tile_observation.snapshot
                state.set_tiles(tile_health)
                recording.log_tiles(
                    resident,
                    visible,
                    loading,
                    tile_health.refresh_count,
                    tile_health.lifecycle,
                    simulation_time_s,
                    physics_step,
                )
                recording_status = recording.status()
                state.update_recording_publisher(
                    recording_status.lifecycle,
                    recording_status.queued_events,
                    recording_status.dropped_events,
                    recording_status.last_error,
                    recording_status.recording_key,
                )
                state.observe_render_cycle(
                    native_update_wall_seconds,
                    time.monotonic() - render_cycle_started,
                    fleet_runtime.timing(),
                )

            for vehicle_id, future in connection_futures.items():
                if future.done() and future.exception() is not None:
                    raise RuntimeError(
                        f"PX4 connection failed for {vehicle_id}"
                    ) from future.exception()

            snapshot = state.snapshot()
            if (
                snapshot["lifecycle"] == "starting"
                and snapshot["vehicles"]
                and all(vehicle["px4_connected"] for vehicle in snapshot["vehicles"])
            ):
                state.set_lifecycle("running")
                LOGGER.info(
                    (
                        "authoritative UAV simulation and live cameras ready: "
                        "session=%s vehicles=%d tile_lifecycle=%s "
                        "camera_lifecycle=%s"
                    ),
                    config.session_id,
                    config.vehicle_count,
                    snapshot["tiles"]["lifecycle"],
                    snapshot["cameras"][0]["lifecycle"],
                )
                snapshot = state.snapshot()
            if (
                not runtime_ready_notified
                and snapshot["lifecycle"] == "running"
                and snapshot["tiles"]["lifecycle"] == "ready"
            ):
                notify_runtime_ready(
                    runtime_events,
                    session_id=config.session_id,
                    generation=simulation_generation,
                )
                runtime_ready_notified = True

    except BaseException:
        if state is not None:
            state.set_lifecycle("failed")
        LOGGER.exception("UAV simulation runtime failed")
        raise
    finally:
        if state is not None:
            state.set_lifecycle("stopping")
        if tileset_paths:

            def clear_ion_token() -> None:
                stage = omni.usd.get_context().get_stage()
                previous_target = stage.GetEditTarget()
                stage.SetEditTarget(Usd.EditTarget(stage.GetSessionLayer()))
                try:
                    for governed_tileset_path in tuple(tileset_paths):
                        tileset = CesiumTileset.Get(stage, governed_tileset_path)
                        if tileset.GetPrim().IsValid():
                            tileset.GetIonAccessTokenAttr().Clear()
                finally:
                    stage.SetEditTarget(previous_target)

            _cleanup("clear Cesium ion token", clear_ion_token)
        if tile_event_bridge is not None:
            _cleanup("Cesium tile lifecycle events", tile_event_bridge.close)
        if connection_executor is not None:
            _cleanup(
                "PX4 connection executor",
                lambda: connection_executor.shutdown(wait=False, cancel_futures=True),
            )
        if fleet_loop is not None:
            _cleanup("default fleet loop", fleet_loop.close)
        if server is not None:
            _cleanup("adapter server", server.close)
        if operator_products is not None:
            _cleanup("operator stream products", operator_products.close)
        for camera_sensor in camera_sensors.values():
            _cleanup("native Isaac H.264 camera sensor", camera_sensor.close)
        if stream_publication is not None:
            _cleanup("native H.264 RTP publication", stream_publication.close)
        if hil_fleet is not None:
            _cleanup("PX4 HIL fleet", hil_fleet.close)
        if physics_timeline is not None:
            _cleanup("Newton timeline", physics_timeline.stop)
        _cleanup("Newton physics", SimulationManager.invalidate_physics)
        for commander in commanders.values():
            _cleanup("PX4 commander", commander.close)
        if recording is not None:
            _cleanup("Recording Hub publisher", recording.close)
            if state is not None:
                state.set_recording_active(False)
        if state is not None:
            state.set_lifecycle("stopped")
        _cleanup("Isaac SimulationApp", simulation_app.close)
