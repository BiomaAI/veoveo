from __future__ import annotations

import concurrent.futures
import logging
import time
from collections.abc import Callable

from .config import RuntimeConfig


LOGGER = logging.getLogger("veoveo.uav_sim")


def _cleanup(name: str, action: Callable[[], None]) -> None:
    try:
        action()
    except BaseException:
        LOGGER.exception("UAV simulation cleanup failed: %s", name)


def run(config: RuntimeConfig) -> None:
    # Isaac requires SimulationApp to exist before importing Kit or simulator modules.
    from isaacsim import SimulationApp

    simulation_app = SimulationApp(
        {
            "headless": True,
            "renderer": "RaytracedLighting",
            "width": config.camera_width,
            "height": config.camera_height,
            "sync_loads": True,
            # Cesium's native USD schema plugin must be discovered before Kit
            # initializes USD's schema registry. Enabling it after
            # SimulationApp starts leaves the generated attributes untyped.
            # The portable root also keeps every Kit write in the pod's
            # ephemeral runtime-cache volume when running as a non-root user.
            "extra_args": [
                "--ext-folder",
                config.extension_directory,
                "--enable",
                "cesium.usd.plugins",
                "--portable-root",
                "/var/lib/veoveo/.cache/kit-portable",
            ],
        }
    )

    import numpy as np
    import omni.kit.app
    import omni.timeline
    import omni.usd
    from isaacsim.core.api import World
    from isaacsim.sensors.experimental.rtx import CameraSensor, RtxCamera
    from omni.kit.viewport.utility import get_active_viewport
    from pxr import Usd

    extension_manager = omni.kit.app.get_app().get_extension_manager()
    extension_manager.add_path(config.extension_directory)
    for extension in (
        "cesium.usd.plugins",
        "cesium.omniverse",
        "isaacsim.core.experimental.prims",
        "isaacsim.sensors.experimental.rtx",
        "pegasus.simulator",
    ):
        extension_manager.set_extension_enabled_immediate(extension, True)
        if not extension_manager.is_extension_enabled(extension):
            raise RuntimeError(f"failed to enable required extension {extension}")

    from cesium.omniverse.bindings import (
        Viewport as CesiumViewport,
        acquire_cesium_omniverse_interface,
    )
    from cesium.omniverse.usdUtils import (
        add_tileset_ion,
        get_or_create_cesium_data,
        get_or_create_cesium_georeference,
    )
    from cesium.usd.plugins.CesiumUsdSchemas import (
        IonServer as CesiumIonServer,
        Tileset as CesiumTileset,
    )
    from pegasus.simulator.logic.backends.px4_mavlink_backend import (
        PX4MavlinkBackend,
        PX4MavlinkBackendConfig,
    )
    from pegasus.simulator.logic.interface.pegasus_interface import PegasusInterface
    from pegasus.simulator.logic.vehicles.multirotor import Multirotor, MultirotorConfig
    from pegasus.simulator.params import ROBOTS

    from .command_queue import MainThreadQueue
    from .px4 import Px4Commander
    from .recording import RecordingPublisher
    from .server import AdapterApplication, AdapterServer, TimelineControls
    from .state import RuntimeState, VehicleTelemetry

    state = RuntimeState(config)
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
    px4_backends: dict[str, PX4MavlinkBackend] = {}
    camera_sensors: dict[str, CameraSensor] = {}
    primary_camera_path: str | None = None

    try:
        recording = RecordingPublisher(config)
        world = World(
            physics_dt=1.0 / config.physics_hz,
            rendering_dt=1.0 / config.rendering_hz,
            stage_units_in_meters=1.0,
        )
        pegasus = PegasusInterface()
        pegasus._world = world
        pegasus.set_global_coordinates(
            config.origin_latitude_degrees,
            config.origin_longitude_degrees,
            config.origin_ellipsoid_height_m,
        )

        stage = omni.usd.get_context().get_stage()
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
                config.origin_latitude_degrees
            )
            georeference.GetGeoreferenceOriginLongitudeAttr().Set(
                config.origin_longitude_degrees
            )
            georeference.GetGeoreferenceOriginHeightAttr().Set(
                config.origin_ellipsoid_height_m
            )
            tileset_path = add_tileset_ion(
                "Google_Photorealistic_3D_Tiles",
                config.cesium_ion_asset_id,
                config.cesium_ion_access_token,
            )
        finally:
            stage.SetEditTarget(previous_target)

        # Fixed unit quaternion for a forward-facing optical camera whose image
        # axes are right/down and whose optical axis is the vehicle's forward axis.
        camera_rotation_wxyz = np.array([[-0.5, -0.5, 0.5, 0.5]])

        for index in range(config.vehicle_count):
            vehicle_id = f"uav-{index + 1}"
            vehicle_prim_path = f"/World/uav_{index + 1}"
            multirotor_config = MultirotorConfig()
            px4_backend = PX4MavlinkBackend(
                PX4MavlinkBackendConfig(
                    {
                        "vehicle_id": index,
                        "px4_autolaunch": True,
                        "px4_dir": config.px4_directory,
                        "px4_vehicle_model": "gazebo-classic_iris",
                        "enable_lockstep": True,
                        "update_rate": float(config.physics_hz),
                    }
                )
            )
            px4_backends[vehicle_id] = px4_backend
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

            commander = Px4Commander(index, config.origin_ellipsoid_height_m)
            commanders[vehicle_id] = commander

            camera_path = f"{vehicle_prim_path}/body/front_camera"
            camera = RtxCamera(
                camera_path,
                tick_rate=float(config.camera_fps),
                translations=np.array([[0.25, 0.0, 0.05]]),
                orientations=camera_rotation_wxyz,
            )
            camera.camera.set_focal_lengths(18.0)
            camera.camera.set_clipping_ranges(0.05, 100_000.0)
            camera_sensors[vehicle_id] = CameraSensor(
                camera,
                resolution=(config.camera_height, config.camera_width),
                annotators=["rgb"],
            )
            if primary_camera_path is None:
                primary_camera_path = camera_path
            recording.add_camera(vehicle_id)

        viewport = get_active_viewport()
        if viewport is None or primary_camera_path is None:
            raise RuntimeError("Cesium requires an active UAV viewport camera")
        # Cesium for Omniverse drives tile selection from Kit viewports. The
        # RTX sensor render product alone is not a Cesium streaming camera.
        viewport.set_active_camera(primary_camera_path)

        world.reset()

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
                nonlocal physics_step, simulation_time_s
                assert world is not None
                was_playing = timeline.is_playing()
                world.reset()
                physics_step = 0
                simulation_time_s = 0.0
                state.advance(simulation_time_s, physics_step)
                state.set_lifecycle("running" if was_playing else "paused")

            command_queue.submit(action)

        def step(steps: int) -> None:
            def action() -> None:
                nonlocal physics_step, simulation_time_s
                assert world is not None
                timeline.play()
                for offset in range(steps):
                    world.step(render=(offset == steps - 1))
                    physics_step += 1
                    simulation_time_s = physics_step / config.physics_hz
                timeline.pause()
                state.advance(simulation_time_s, physics_step)
                state.set_lifecycle("paused")

            command_queue.submit(action)

        application = AdapterApplication(
            config,
            state,
            TimelineControls(pause=pause, resume=resume, reset=reset, step=step),
            commanders,
            recording,
        )
        server = AdapterServer(config, application)
        server.start()

        timeline.play()
        connection_executor = concurrent.futures.ThreadPoolExecutor(
            max_workers=config.vehicle_count, thread_name_prefix="px4-connect"
        )
        connection_futures = {
            vehicle_id: connection_executor.submit(commander.connect)
            for vehicle_id, commander in commanders.items()
        }

        # The first RTX/Cesium render can compile shaders for longer than the
        # PX4 connection deadline. Advance physics without rendering until the
        # Simulator MAVLink and GCS handshakes are complete, then let the normal
        # loop render Google Photorealistic 3D Tiles and camera frames.
        px4_bootstrap_deadline = time.monotonic() + 120.0
        while not all(future.done() for future in connection_futures.values()):
            world.step(render=False)
            # Pegasus 5.1 registers its backend through Isaac's deprecated
            # callback bridge. Invoke the public backend update here as well
            # during bootstrap because Isaac 6 can defer that callback until a
            # rendered update, which would recreate the startup deadlock.
            for px4_backend in px4_backends.values():
                px4_backend.update(1.0 / config.physics_hz)
            physics_step += 1
            simulation_time_s = physics_step / config.physics_hz
            state.advance(simulation_time_s, physics_step)
            for vehicle_id, future in connection_futures.items():
                if future.done() and future.exception() is not None:
                    raise RuntimeError(
                        f"PX4 connection failed for {vehicle_id}"
                    ) from future.exception()
            if time.monotonic() >= px4_bootstrap_deadline:
                raise TimeoutError("PX4 bootstrap did not complete before rendering")
            time.sleep(0.001)

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
            cesium_viewport = CesiumViewport()
            cesium_viewport.viewMatrix = viewport.view
            cesium_viewport.projMatrix = viewport.projection
            cesium_viewport.width = float(viewport.resolution[0])
            cesium_viewport.height = float(viewport.resolution[1])
            cesium_interface.on_update_frame([cesium_viewport], False)

        tile_resident_frames = 0
        tile_started_at = time.monotonic()
        render_interval = max(1, round(config.physics_hz / config.rendering_hz))
        camera_interval = max(1, round(config.physics_hz / config.camera_fps))

        while simulation_app.is_running():
            command_queue.drain()
            if timeline.is_playing():
                render = physics_step % render_interval == 0
                world.step(render=render)
                physics_step += 1
                simulation_time_s = physics_step / config.physics_hz
                state.advance(simulation_time_s, physics_step)
                update_cesium_viewport()
            else:
                simulation_app.update()
                update_cesium_viewport()
                time.sleep(0.005)
                continue

            telemetry: list[VehicleTelemetry] = []
            for vehicle_id, vehicle in vehicles.items():
                px4_status = commanders[vehicle_id].status()
                vehicle_state = vehicle.state
                telemetry.append(
                    VehicleTelemetry(
                        vehicle_id=vehicle_id,
                        position_enu=tuple(float(value) for value in vehicle_state.position),
                        attitude_xyzw=tuple(float(value) for value in vehicle_state.attitude),
                        linear_velocity_enu_mps=tuple(
                            float(value) for value in vehicle_state.linear_velocity
                        ),
                        flight_state=px4_status.flight_state,
                        battery_percent=px4_status.battery_percent,
                        px4_connected=px4_status.connected,
                    )
                )

            if physics_step % 5 == 0:
                state.update_vehicles(telemetry)
                recording.log_frame(telemetry, simulation_time_s, physics_step)
                for vehicle_id, vehicle in vehicles.items():
                    vehicle_state = vehicle.state
                    recording.log_imu(
                        vehicle_id,
                        tuple(float(value) for value in vehicle_state.linear_acceleration),
                        tuple(float(value) for value in vehicle_state.angular_velocity),
                        simulation_time_s,
                        physics_step,
                    )

            if physics_step % camera_interval == 0:
                for vehicle_id, sensor in camera_sensors.items():
                    pixels, _information = sensor.get_data("rgb")
                    if pixels is not None:
                        rgb = pixels.numpy()[..., :3]
                        recording.camera(vehicle_id).encode(
                            rgb, simulation_time_s, physics_step
                        )

            if render:
                statistics = cesium_interface.get_render_statistics()
                resident = int(statistics.tiles_loaded)
                loading = int(statistics.tiles_loading_worker) + int(
                    statistics.tiles_loading_main
                )
                tile_resident_frames = tile_resident_frames + 1 if resident > 0 else 0
                tile_lifecycle = (
                    "ready"
                    if tile_resident_frames >= config.tile_ready_frames
                    else "streaming"
                    if resident > 0 or loading > 0
                    else "connecting"
                )
                state.set_tiles(tile_lifecycle, resident, loading)
                recording.log_tiles(
                    resident, loading, tile_lifecycle, simulation_time_s, physics_step
                )
                if resident == 0 and time.monotonic() - tile_started_at > 600.0:
                    state.set_tiles(
                        "failed",
                        0,
                        loading,
                        diagnostic="Google Photorealistic 3D Tiles did not become resident",
                    )
                    raise RuntimeError("Google Photorealistic 3D Tiles readiness timed out")

            for vehicle_id, future in connection_futures.items():
                if future.done() and future.exception() is not None:
                    raise RuntimeError(f"PX4 connection failed for {vehicle_id}") from future.exception()

            snapshot = state.snapshot()
            if (
                snapshot["lifecycle"] == "starting"
                and snapshot["tiles"]["lifecycle"] == "ready"
                and snapshot["vehicles"]
                and all(vehicle["px4_connected"] for vehicle in snapshot["vehicles"])
            ):
                state.set_lifecycle("running")
                LOGGER.info(
                    "UAV simulation ready: session=%s vehicles=%d resident_tiles=%d",
                    config.session_id,
                    config.vehicle_count,
                    snapshot["tiles"]["resident_tiles"],
                )

            if (
                config.exit_after_seconds is not None
                and snapshot["lifecycle"] == "running"
                and simulation_time_s >= config.exit_after_seconds
            ):
                LOGGER.info(
                    "batch simulation reached %.3f seconds", config.exit_after_seconds
                )
                break

    except BaseException:
        state.set_lifecycle("failed")
        LOGGER.exception("UAV simulation runtime failed")
        raise
    finally:
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
        if server is not None:
            _cleanup("adapter server", server.close)
        if timeline.is_playing():
            _cleanup("timeline", timeline.stop)
        for commander in commanders.values():
            _cleanup("PX4 commander", commander.close)
        if recording is not None:
            _cleanup(
                "Recording Hub publisher",
                lambda: recording.close(simulation_time_s, physics_step),
            )
        state.set_lifecycle("stopped")
        _cleanup("Isaac SimulationApp", simulation_app.close)
