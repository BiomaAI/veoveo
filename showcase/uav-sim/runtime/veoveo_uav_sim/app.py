from __future__ import annotations

import concurrent.futures
import logging
import time
from collections.abc import Callable

from .config import RuntimeConfig
from .hydra_camera import native_sensor_aov_arguments
from .operator_products import livestream_aov_arguments
from .physical_camera import physical_camera_product_name

LOGGER = logging.getLogger("veoveo.uav_sim")


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


def _cleanup(name: str, action: Callable[[], None]) -> None:
    try:
        action()
    except BaseException:
        LOGGER.exception("UAV simulation cleanup failed: %s", name)


def run(config: RuntimeConfig) -> None:
    # Isaac requires SimulationApp to exist before importing Kit or simulator modules.
    from isaacsim import SimulationApp

    physical_product_name = physical_camera_product_name(
        config.camera.vehicle_id
    )
    viewport_width = config.camera.width
    viewport_height = config.camera.height
    simulation_app = SimulationApp(
        {
            "headless": True,
            "renderer": "RaytracedLighting",
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
                "omni.kit.livestream.webrtc",
                "--enable",
                "omni.kit.livestream.rtsp",
                *native_sensor_aov_arguments(
                    physical_product_name,
                    rtsp_port=config.camera.rtsp_port,
                    target_fps=config.camera.fps,
                ),
                *livestream_aov_arguments(config.operator_live_view),
                *kit_live_render_arguments(),
                "--portable-root",
                str(config.cache_directory / "kit-portable"),
            ],
        }
    )

    import omni.kit.app
    import omni.timeline
    import omni.usd
    from isaacsim.core.api import World
    from isaacsim.core.api.materials import PhysicsMaterial
    from isaacsim.core.api.objects import GroundPlane
    from pxr import Gf, Usd, UsdGeom, UsdLux

    extension_manager = omni.kit.app.get_app().get_extension_manager()
    extension_manager.add_path(config.extension_directory)
    for extension in (
        "cesium.usd.plugins",
        "cesium.omniverse",
        "isaacsim.core.experimental.prims",
        "omni.kit.livestream.rtsp",
        "omni.kit.livestream.webrtc",
        "pegasus.simulator",
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
    from pegasus.simulator.logic.backends.px4_mavlink_backend import (
        PX4MavlinkBackend,
        PX4MavlinkBackendConfig,
    )
    from pegasus.simulator.logic.interface.pegasus_interface import PegasusInterface
    from pegasus.simulator.logic.sensors import GPS, IMU, Barometer, Magnetometer
    from pegasus.simulator.logic.vehicles.multirotor import Multirotor, MultirotorConfig
    from pegasus.simulator.params import ROBOTS

    from .cesium_camera import current_pose_cesium_viewport
    from .command_queue import MainThreadQueue
    from .fleet_loop import FleetLoopController
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
    from .physics_batch import FleetPhysicsLifecycle
    from .px4 import Px4Commander
    from .realtime import FixedStepCadenceGate, PhysicsRenderSchedule
    from .recording import ImuTelemetry, RecordingPublisher
    from .render_pose import rendered_pose_agreement
    from .runtime_events import notify_adapter_ready, notify_runtime_ready
    from .server import (
        AdapterApplication,
        AdapterServer,
        PreconfigurationApplication,
        TimelineControls,
    )
    from .state import RuntimeState, VehicleTelemetry
    from .vehicle_model import PX4_IRIS_SENSOR_CADENCE, Px4IrisThrustCurve
    from .world_config import WorldConfiguration, WorldConfigurationSlot
    from .world_health import assess_tile_health

    state: RuntimeState | None = None
    world_config: WorldConfiguration | None = None
    world_slot = WorldConfigurationSlot()
    command_queue = MainThreadQueue()
    timeline = omni.timeline.get_timeline_interface()
    recording: RecordingPublisher | None = None
    server: AdapterServer | None = None
    connection_executor: concurrent.futures.ThreadPoolExecutor | None = None
    tileset_path: str | None = None
    world: World | None = None
    physics_step = 0
    simulation_time_s = 0.0
    commanders: dict[str, Px4Commander] = {}
    vehicles: dict[str, Multirotor] = {}
    vehicle_callback_prefixes: dict[str, str] = {}
    camera_sensors: dict[str, NativeH264CameraSensor] = {}
    camera_sensor_sequences: dict[str, int] = {}
    camera_frames_observed: dict[str, int] = {}
    physics_lifecycle: FleetPhysicsLifecycle | None = None
    fleet_loop: FleetLoopController | None = None
    operator_cameras: AuthoritativeOperatorCameraCollection | None = None
    operator_products: OperatorProductCollection | None = None
    simulation_generation = 1

    try:
        preconfiguration = PreconfigurationApplication(config, world_slot)
        server = AdapterServer(config, preconfiguration.application)
        server.start()
        notify_adapter_ready(
            config.runtime_event_socket,
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
        state = RuntimeState(config, world_config)
        recording = RecordingPublisher(config, world_config)
        world = World(
            physics_dt=1.0 / config.physics_hz,
            rendering_dt=1.0 / config.rendering_hz,
            stage_units_in_meters=1.0,
            backend="warp",
            device="cuda:0",
        )
        # This standalone process owns Kit updates through world.step(). Keep
        # the loop in manual mode as required by SimulationContext instead of
        # mixing an automatic Kit run loop with an external update caller.
        # Native AOV/WebRTC otherwise takes automatic loop ownership when a
        # product activates and blocks the authoritative simulation caller.
        from omni.kit.loop import _loop as omni_loop

        loop_runner = omni_loop.acquire_loop_interface()
        loop_runner.set_manual_mode(True)
        launch_surface_material = PhysicsMaterial(
            prim_path="/World/Physics_Materials/uav_launch_surface",
            static_friction=1.0,
            dynamic_friction=0.8,
            restitution=0.0,
        )
        world.scene.add(
            GroundPlane(
                prim_path="/World/uav_launch_surface",
                name="uav_launch_surface",
                size=40.0,
                z_position=0.0,
                visible=False,
                physics_material=launch_surface_material,
            )
        )
        pegasus = PegasusInterface()
        pegasus._world = world
        pegasus.set_global_coordinates(
            world_config.georeference_origin.latitude_degrees,
            world_config.georeference_origin.longitude_degrees,
            world_config.georeference_origin.ellipsoid_height_m,
        )

        stage = omni.usd.get_context().get_stage()
        stage.DefinePrim("/World/Environment", "Xform")
        sky = UsdLux.DomeLight.Define(stage, "/World/Environment/Sky")
        sky.CreateIntensityAttr(1000.0)
        sun = UsdLux.DistantLight.Define(stage, "/World/Environment/Sun")
        sun.CreateIntensityAttr(500.0)
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
            tileset_path = add_tileset_ion(
                "Google_Photorealistic_3D_Tiles",
                config.cesium_ion_asset_id,
                config.cesium_ion_access_token,
            )
            tileset = CesiumTileset.Get(stage, tileset_path)
            if not tileset.GetPrim().IsValid():
                raise RuntimeError("Cesium did not create the governed tileset prim")
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
            tileset.GetForbidHolesAttr().Set(
                config.tile_streaming.forbid_holes
            )
        finally:
            stage.SetEditTarget(previous_target)

        mount_w, mount_x, mount_y, mount_z = config.camera.mount.orientation_wxyz
        sensor_mount_pose = Pose(
            Vector3(*config.camera.mount.translation_xyz_m),
            QuaternionXyzw(mount_x, mount_y, mount_z, mount_w),
        ).normalized()

        physical_cameras = {}
        for index in range(config.vehicle_count):
            vehicle_id = f"uav-{index + 1}"
            vehicle_prim_path = f"/World/uav_{index + 1}"
            multirotor_config = MultirotorConfig()
            multirotor_config.thrust_curve = Px4IrisThrustCurve()
            PX4_IRIS_SENSOR_CADENCE.validate_for_physics(config.physics_hz)
            multirotor_config.sensors = [
                Barometer(
                    {"update_rate": float(PX4_IRIS_SENSOR_CADENCE.barometer_hz)}
                ),
                IMU({"update_rate": float(PX4_IRIS_SENSOR_CADENCE.imu_hz)}),
                Magnetometer(
                    {
                        "update_rate": float(
                            PX4_IRIS_SENSOR_CADENCE.magnetometer_hz
                        )
                    }
                ),
                GPS({"update_rate": float(PX4_IRIS_SENSOR_CADENCE.gps_hz)}),
            ]
            px4_backend = PX4MavlinkBackend(
                PX4MavlinkBackendConfig(
                    {
                        "vehicle_id": index,
                        "px4_autolaunch": True,
                        "px4_dir": config.px4_directory,
                        "px4_vehicle_model": "gazebo-classic_iris",
                        # This process owns the one real-time physics clock.
                        # Waiting serially for four independent PX4 actuator
                        # replies here would multiply their latency and make
                        # native rendering stall the authoritative timeline.
                        "enable_lockstep": False,
                        "update_rate": float(config.physics_hz),
                    }
                )
            )
            multirotor_config.backends = [px4_backend]
            vehicle = Multirotor(
                vehicle_prim_path,
                ROBOTS["Iris"],
                index,
                [float(index * 3), 0.0, 0.07],
                [0.0, 0.0, 0.0, 1.0],
                config=multirotor_config,
            )
            vehicles[vehicle_id] = vehicle
            vehicle_callback_prefixes[vehicle_id] = vehicle_prim_path

            # Pegasus's Iris asset binds two MDL materials over plain HTTP.
            # The UAV geometry remains functional without those cosmetic
            # bindings, and deactivating them keeps the production image
            # self-contained under the chart's HTTPS-only egress policy.
            for looks_path in (
                f"{vehicle_prim_path}/body/Looks",
                *(f"{vehicle_prim_path}/rotor{rotor}/Looks" for rotor in range(4)),
            ):
                looks = stage.GetPrimAtPath(looks_path)
                if looks.IsValid():
                    looks.SetActive(False)

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
            stage,
        )
        extension_manager.set_extension_enabled_immediate(
            "omni.kit.livestream.aov", True
        )
        if not extension_manager.is_extension_enabled("omni.kit.livestream.aov"):
            raise RuntimeError(
                "failed to enable required extension omni.kit.livestream.aov"
            )
        state.update_stream_products(operator_products.state(content_ready=False))

        rigid_body_paths = tuple(
            f"{vehicle_callback_prefixes[vehicle_id]}/{body_name}"
            for vehicle_id in vehicles
            for body_name in ("body", "rotor0", "rotor1", "rotor2", "rotor3")
        )
        recording_cadence = FixedStepCadenceGate(
            config.physics_hz, config.recording.telemetry_hz
        )
        physics_render_schedule = PhysicsRenderSchedule(
            config.physics_hz, config.rendering_hz
        )
        def telemetry_snapshot() -> list[VehicleTelemetry]:
            telemetry: list[VehicleTelemetry] = []
            for vehicle_id, vehicle in vehicles.items():
                px4_status = commanders[vehicle_id].status()
                vehicle_state = vehicle.state
                telemetry.append(
                    VehicleTelemetry(
                        vehicle_id=vehicle_id,
                        position_enu=tuple(
                            float(value) for value in vehicle_state.position
                        ),
                        attitude_xyzw=tuple(
                            float(value) for value in vehicle_state.attitude
                        ),
                        linear_velocity_enu_mps=tuple(
                            float(value) for value in vehicle_state.linear_velocity
                        ),
                        flight_state=px4_status.flight_state,
                        battery_percent=px4_status.battery_percent,
                        px4_connected=px4_status.connected,
                    )
                )
            return telemetry

        def operator_entity_transforms() -> dict[str, EntityTransform]:
            return {
                vehicle_id: EntityTransform(
                    vehicle_id,
                    Pose(
                        Vector3(
                            *(float(value) for value in vehicle.state.position)
                        ),
                        QuaternionXyzw(
                            *(float(value) for value in vehicle.state.attitude)
                        ).normalized(),
                    ),
                )
                for vehicle_id, vehicle in vehicles.items()
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
                            linear_acceleration_mps2=tuple(
                                float(value)
                                for value in vehicle.state.linear_acceleration
                            ),
                            angular_velocity_rps=tuple(
                                float(value) for value in vehicle.state.angular_velocity
                            ),
                        )
                        for vehicle_id, vehicle in vehicles.items()
                    ],
                    simulation_time_s,
                    physics_step,
            )

        physics_lifecycle = FleetPhysicsLifecycle(
            world,
            vehicles,
            vehicle_callback_prefixes,
            rigid_body_paths,
            after_step=advance_physics,
        )
        physics_batch = physics_lifecycle.reset()
        LOGGER.info(
            "UAV fleet physics batch ready: bodies=%d device=%s",
            physics_batch.body_count,
            physics_batch.device,
        )

        def pause() -> None:
            def action() -> None:
                timeline.pause()
                state.set_lifecycle("paused")

            command_queue.submit(action)

        def resume() -> None:
            def action() -> None:
                timeline.play()
                state.set_lifecycle("running")

            command_queue.submit(action)

        def reset() -> None:
            def action() -> None:
                nonlocal physics_step, simulation_time_s, simulation_generation
                assert world is not None
                was_playing = timeline.is_playing()
                assert physics_lifecycle is not None
                physics_lifecycle.reset()
                physics_step = 0
                simulation_time_s = 0.0
                simulation_generation += 1
                recording_cadence.reset()
                physics_render_schedule.reset()
                state.advance(simulation_time_s, physics_step)
                state.set_lifecycle("running" if was_playing else "paused")

            command_queue.submit(action)

        def step(steps: int) -> None:
            def action() -> None:
                nonlocal physics_step, simulation_time_s
                assert world is not None
                timeline.play()
                simulation_app.update()
                for _ in range(steps):
                    world.step(render=False)
                world.render()
                timeline.pause()
                simulation_app.update()
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
            command_queue.submit,
        )
        assert server is not None
        server.close()
        server = AdapterServer(config, application.application)
        server.start()

        timeline.play()
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
            world.step(render=False)
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

        tile_coverage_frames = 0
        tile_absent_since: float | None = time.monotonic()
        tile_failure_latched = False
        tile_recovery_count = 0
        tile_unavailable_reported = False
        runtime_ready_notified = False
        while simulation_app.is_running():
            assert fleet_loop is not None
            fleet_loop.raise_if_failed()
            command_queue.drain()
            if timeline.is_playing():
                render_cycle_started = time.monotonic()
                # Advance the exact fixed-step physics batch first. Rendering
                # is disabled for these substeps, so native AOV activation
                # cannot own or delay the authoritative simulation update.
                for _ in range(physics_render_schedule.next_step_count()):
                    world.step(render=False)
                # Submit the final authoritative camera set before the one
                # render-only Kit update. Hydra and Cesium therefore consume
                # the same current transform instead of racing a physics
                # callback inside app.update().
                update_operator_cameras()
                for sensor in camera_sensors.values():
                    sensor.observe_simulation_time(simulation_time_s, physics_step)
                update_cesium_viewport()
                native_update_started = time.monotonic()
                world.render()
                native_update_wall_seconds = (
                    time.monotonic() - native_update_started
                )
                assert operator_products is not None
                operator_products.observe_render_completion(time.monotonic())
                state.update_stream_products(
                    operator_products.state(
                        content_ready=(
                            state.snapshot()["tiles"]["lifecycle"] == "ready"
                        )
                    )
                )
                render = True
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
                statistics = cesium_interface.get_render_statistics()
                resident = int(statistics.tiles_loaded)
                visible = int(statistics.tiles_rendered)
                loading = int(statistics.tiles_loading_worker) + int(
                    statistics.tiles_loading_main
                )
                now = time.monotonic()
                if visible > 0:
                    tile_absent_since = None
                elif tile_absent_since is None:
                    tile_absent_since = now
                tile_health = assess_tile_health(
                    resident_tiles=resident,
                    visible_tiles=visible,
                    loading_tiles=loading,
                    coverage_frames=tile_coverage_frames,
                    ready_frames=config.tile_ready_frames,
                    absent_seconds=(
                        0.0 if tile_absent_since is None else now - tile_absent_since
                    ),
                    failed_latched=tile_failure_latched,
                )
                tile_coverage_frames = tile_health.coverage_frames
                tile_diagnostic = tile_health.diagnostic
                if tile_health.recovery_required:
                    tile_recovery_count += 1
                    try:
                        assert tileset_path is not None
                        cesium_interface.reload_tileset(tileset_path)
                    except Exception:  # noqa: BLE001 - visual failure is non-authoritative
                        tile_diagnostic = "streamed-world provider reload failed"
                        LOGGER.exception(
                            "streamed-world provider reload failed; simulation continues"
                        )
                tile_failure_latched = tile_health.lifecycle == "failed"
                state.set_tiles(
                    tile_health.lifecycle,
                    resident,
                    visible,
                    loading,
                    tile_recovery_count,
                    diagnostic=tile_diagnostic,
                )
                recording.log_tiles(
                    resident,
                    visible,
                    loading,
                    tile_recovery_count,
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
                )
                if tile_health.lifecycle == "failed" and not tile_unavailable_reported:
                    tile_unavailable_reported = True
                    LOGGER.error(
                        "Google Photorealistic 3D Tiles are unavailable; simulation continues"
                    )
                elif tile_health.lifecycle != "failed":
                    tile_unavailable_reported = False

                state.observe_render_cycle(
                    native_update_wall_seconds,
                    time.monotonic() - render_cycle_started,
                    physics_lifecycle.timing(),
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
                    config.runtime_event_socket,
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
        if tileset_path is not None:

            def clear_ion_token() -> None:
                stage = omni.usd.get_context().get_stage()
                previous_target = stage.GetEditTarget()
                stage.SetEditTarget(Usd.EditTarget(stage.GetSessionLayer()))
                try:
                    tileset = CesiumTileset.Get(stage, tileset_path)
                    if tileset.GetPrim().IsValid():
                        tileset.GetIonAccessTokenAttr().Clear()
                finally:
                    stage.SetEditTarget(previous_target)

            _cleanup("clear Cesium ion token", clear_ion_token)
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
        if timeline.is_playing():
            _cleanup("timeline", timeline.stop)
        for commander in commanders.values():
            _cleanup("PX4 commander", commander.close)
        if recording is not None:
            _cleanup("Recording Hub publisher", recording.close)
            if state is not None:
                state.set_recording_active(False)
        if state is not None:
            state.set_lifecycle("stopped")
        _cleanup("Isaac SimulationApp", simulation_app.close)
