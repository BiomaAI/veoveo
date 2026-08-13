from __future__ import annotations

import asyncio
import concurrent.futures
import threading
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Callable

from aiohttp import web

from .adapter_auth import authorization_middleware
from .config import RuntimeConfig
from .contracts import (
    ContractError,
    DirectCommand,
    DurableOperation,
    parse_command,
    parse_operation,
)
from .fleet_loop import FleetLoopController
from .operator_camera_config import live_camera_descriptor
from .operator_products import OperatorProductCollection
from .px4 import Px4Commander
from .recording import RecordingPublisher
from .runtime_events import RuntimeEventPublisher
from .state import RuntimeState, initial_runtime_timing
from .world_config import (
    WorldConfiguration,
    WorldConfigurationError,
    WorldConfigurationSlot,
)


def _timestamp() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


@dataclass(frozen=True, slots=True)
class TimelineControls:
    pause: Callable[[], None]
    resume: Callable[[], None]
    reset: Callable[[], None]
    step: Callable[[int], None]


def _world_configuration_response(
    session_id: str, world: WorldConfiguration
) -> dict[str, object]:
    return {
        "accepted": True,
        "world": world.as_dict(),
        "resource_uri": f"uav-sim://session/{session_id}/world",
    }


class PreconfigurationApplication:
    def __init__(
        self,
        config: RuntimeConfig,
        world_slot: WorldConfigurationSlot,
        runtime_events: RuntimeEventPublisher,
    ) -> None:
        self._config = config
        self._world_slot = world_slot
        self._app = web.Application(
            client_max_size=2 * 1024 * 1024,
            middlewares=[authorization_middleware(config.adapter_bearer_token)],
        )
        self._app.add_routes(
            [
                web.get("/healthz", self._health),
                web.get("/readyz", self._ready),
                web.get("/v1/state", self._get_state),
                web.post("/v1/world", self._configure_world),
                web.post(
                    "/v1/live-products/release-all",
                    self._release_all_viewer_slots,
                ),
                web.get("/v1/events", runtime_events.stream),
            ]
        )

    @property
    def application(self) -> web.Application:
        return self._app

    async def _health(self, _request: web.Request) -> web.Response:
        status = "starting" if self._world_slot.get() is not None else "unconfigured"
        return web.json_response({"status": status})

    async def _ready(self, _request: web.Request) -> web.Response:
        status = "starting" if self._world_slot.get() is not None else "unconfigured"
        return web.json_response(
            {
                "ready": True,
                "simulation_ready": False,
                "visual_ready": False,
                "status": status,
            }
        )

    async def _get_state(self, _request: web.Request) -> web.Response:
        now = _timestamp()
        world = self._world_slot.get()
        return web.json_response(
            {
                "session_id": self._config.session_id,
                "lifecycle": "starting" if world is not None else "unconfigured",
                "simulation_time_s": 0.0,
                "physics_step": 0,
                "timing": initial_runtime_timing(self._config),
                "world": world.as_dict() if world is not None else None,
                "tiles": {
                    "lifecycle": "connecting",
                    "source": "google_photorealistic_3d_tiles",
                    "ion_asset_id": self._config.cesium_ion_asset_id,
                    "resident_tiles": 0,
                    "visible_tiles": 0,
                    "loading_tiles": 0,
                    "provider_generation": 0,
                    "event_sequence": 0,
                    "refresh_count": 0,
                },
                "cameras": [],
                "live_cameras": [
                    live_camera_descriptor(self._config.session_id, camera)
                    for camera in self._config.operator_live_view.cameras
                ],
                "stream_products": [
                    {
                        "streamProductId": f"product-slot-{capacity_slot}",
                        "capacitySlot": capacity_slot,
                        "lifecycle": "inactive",
                        "activeViewerLeases": 0,
                        "connectedViewers": 0,
                        "nvencSessions": 0,
                        "encodedFrames": 0,
                        "sourceToRenderSamples": 0,
                    }
                    for capacity_slot in range(
                        self._config.operator_live_view.viewer_slot_count
                    )
                ],
                "vehicles": [],
                "recordings": [],
                "updated_at": now,
            }
        )

    async def _configure_world(self, request: web.Request) -> web.Response:
        try:
            world = WorldConfiguration.from_request(
                await request.json(), self._config.session_id
            )
        except (TypeError, WorldConfigurationError) as error:
            return web.json_response({"error": str(error)}, status=400)
        try:
            configured = self._world_slot.configure(world)
        except WorldConfigurationError as error:
            return web.json_response({"error": str(error)}, status=409)
        return web.json_response(
            _world_configuration_response(self._config.session_id, configured)
        )

    async def _release_all_viewer_slots(
        self, _request: web.Request
    ) -> web.Response:
        return web.json_response({"accepted": True})


class AdapterApplication:
    def __init__(
        self,
        config: RuntimeConfig,
        state: RuntimeState,
        timeline: TimelineControls,
        commanders: dict[str, Px4Commander],
        recording: RecordingPublisher,
        world_slot: WorldConfigurationSlot,
        fleet_loop: FleetLoopController,
        operator_products: OperatorProductCollection,
        runtime_events: RuntimeEventPublisher,
        submit_main_thread: Callable[[Callable[[], object]], object],
    ) -> None:
        self._config = config
        self._state = state
        self._timeline = timeline
        self._commanders = commanders
        self._recording = recording
        self._world_slot = world_slot
        self._fleet_loop = fleet_loop
        self._operator_products = operator_products
        self._submit_main_thread = submit_main_thread
        self._app = web.Application(
            client_max_size=2 * 1024 * 1024,
            middlewares=[authorization_middleware(config.adapter_bearer_token)],
        )
        self._app.add_routes(
            [
                web.get("/healthz", self._health),
                web.get("/readyz", self._ready),
                web.get("/v1/state", self._get_state),
                web.post("/v1/world", self._configure_world),
                web.post("/v1/commands", self._command),
                web.post("/v1/operations", self._operation),
                web.put(
                    "/v1/live-products/{capacity_slot}/assignment",
                    self._assign_product,
                ),
                web.delete(
                    "/v1/live-products/{capacity_slot}/assignment",
                    self._release_product,
                ),
                web.post(
                    "/v1/live-products/release-all",
                    self._release_all_viewer_slots,
                ),
                web.get("/v1/events", runtime_events.stream),
            ]
        )

    @property
    def application(self) -> web.Application:
        return self._app

    async def _health(self, _request: web.Request) -> web.Response:
        lifecycle = self._state.snapshot()["lifecycle"]
        status = 503 if lifecycle == "failed" else 200
        return web.json_response({"status": lifecycle}, status=status)

    async def _ready(self, _request: web.Request) -> web.Response:
        snapshot = self._state.snapshot()
        simulation_ready = (
            snapshot["lifecycle"] in {"ready", "running", "paused"}
            and bool(snapshot["vehicles"])
            and all(vehicle["px4_connected"] for vehicle in snapshot["vehicles"])
        )
        visual_ready = (
            snapshot["tiles"]["lifecycle"] == "ready"
            and bool(snapshot["cameras"])
            and all(camera["lifecycle"] == "ready" for camera in snapshot["cameras"])
        )
        return web.json_response(
            {
                "ready": snapshot["lifecycle"] != "failed",
                "simulation_ready": simulation_ready,
                "visual_ready": visual_ready,
                "status": snapshot["lifecycle"],
            },
            status=503 if snapshot["lifecycle"] == "failed" else 200,
        )

    async def _get_state(self, _request: web.Request) -> web.Response:
        return web.json_response(self._state.snapshot())

    async def _configure_world(self, request: web.Request) -> web.Response:
        try:
            world = WorldConfiguration.from_request(
                await request.json(), self._config.session_id
            )
        except (TypeError, WorldConfigurationError) as error:
            return web.json_response({"error": str(error)}, status=400)
        try:
            configured = self._world_slot.configure(world)
        except WorldConfigurationError as error:
            return web.json_response({"error": str(error)}, status=409)
        return web.json_response(
            _world_configuration_response(self._config.session_id, configured)
        )

    async def _command(self, request: web.Request) -> web.Response:
        try:
            command = parse_command(await request.json())
            result = await asyncio.to_thread(self._execute_command, command)
            return web.json_response(result)
        except (ContractError, ValueError) as error:
            return web.json_response({"error": str(error)}, status=400)
        except (RuntimeError, TimeoutError) as error:
            return web.json_response({"error": str(error)}, status=409)

    async def _operation(self, request: web.Request) -> web.Response:
        try:
            operation = parse_operation(await request.json())
            result = await asyncio.to_thread(self._execute_operation, operation)
            return web.json_response(result)
        except (ContractError, ValueError) as error:
            return web.json_response({"error": str(error)}, status=400)
        except (RuntimeError, TimeoutError) as error:
            return web.json_response({"error": str(error)}, status=409)

    async def _release_all_viewer_slots(
        self, _request: web.Request
    ) -> web.Response:
        try:
            await asyncio.to_thread(
                self._submit_main_thread,
                self._operator_products.release_all,
            )
            self._state.update_stream_products(
                self._operator_products.state(
                    content_ready=self._stream_content_ready()
                )
            )
            return web.json_response({"accepted": True})
        except (RuntimeError, TimeoutError) as error:
            return web.json_response({"error": str(error)}, status=409)

    async def _assign_product(self, request: web.Request) -> web.Response:
        try:
            capacity_slot = int(request.match_info["capacity_slot"])
            body = await request.json()
            if not isinstance(body, dict) or set(body) != {"cameraId", "liveViewId"}:
                raise ValueError("viewer-product assignment requires cameraId and liveViewId")
            camera_id = body["cameraId"]
            live_view_id = body["liveViewId"]
            if not isinstance(camera_id, str) or not isinstance(live_view_id, str):
                raise ValueError("viewer-product assignment identities must be strings")
            await asyncio.to_thread(
                self._submit_main_thread,
                lambda: self._operator_products.assign(
                    capacity_slot,
                    camera_id,
                    live_view_id,
                    content_ready=self._stream_content_ready(),
                ),
            )
            try:
                result = await asyncio.to_thread(
                    self._operator_products.wait_until_ready,
                    capacity_slot,
                    live_view_id,
                    timeout_seconds=(
                        self._config.operator_live_view.activation_timeout_seconds
                    ),
                    content_ready=self._stream_content_ready(),
                )
            except (RuntimeError, TimeoutError):
                await asyncio.to_thread(
                    self._submit_main_thread,
                    lambda: self._operator_products.release(
                        capacity_slot,
                        live_view_id,
                        content_ready=self._stream_content_ready(),
                    ),
                )
                raise
            self._state.update_stream_products(
                self._operator_products.state(
                    content_ready=self._stream_content_ready()
                )
            )
            return web.json_response(result)
        except (TypeError, ValueError) as error:
            return web.json_response({"error": str(error)}, status=400)
        except (RuntimeError, TimeoutError) as error:
            return web.json_response({"error": str(error)}, status=409)

    async def _release_product(self, request: web.Request) -> web.Response:
        try:
            capacity_slot = int(request.match_info["capacity_slot"])
            body = await request.json()
            if not isinstance(body, dict) or set(body) != {"liveViewId"}:
                raise ValueError("viewer-product release requires liveViewId")
            live_view_id = body["liveViewId"]
            if not isinstance(live_view_id, str):
                raise ValueError("viewer-product liveViewId must be a string")
            result = await asyncio.to_thread(
                self._submit_main_thread,
                lambda: self._operator_products.release(
                    capacity_slot,
                    live_view_id,
                    content_ready=self._stream_content_ready(),
                ),
            )
            self._state.update_stream_products(
                self._operator_products.state(
                    content_ready=self._stream_content_ready()
                )
            )
            return web.json_response(result)
        except (TypeError, ValueError) as error:
            return web.json_response({"error": str(error)}, status=400)
        except (RuntimeError, TimeoutError) as error:
            return web.json_response({"error": str(error)}, status=409)

    def _stream_content_ready(self) -> bool:
        tiles = self._state.snapshot()["tiles"]
        return tiles["lifecycle"] == "ready" or (
            tiles["lifecycle"] == "refreshing" and tiles["visible_tiles"] > 0
        )

    def _execute_command(self, command: DirectCommand) -> dict[str, object]:
        self._state.require_session(command.session_id)
        if command.command == "pause":
            self._timeline.pause()
            detail = "simulation paused"
            resource_uri = f"uav-sim://session/{command.session_id}"
        elif command.command == "resume":
            self._timeline.resume()
            detail = "simulation resumed"
            resource_uri = f"uav-sim://session/{command.session_id}"
        elif command.command == "reset":
            snapshot = self._state.snapshot()
            if any(
                vehicle["flight_state"] not in {"standby", "landed"}
                for vehicle in snapshot["vehicles"]
            ):
                raise RuntimeError("all vehicles must be landed before reset")
            self._timeline.reset()
            detail = "simulation reset"
            resource_uri = f"uav-sim://session/{command.session_id}"
        elif command.command == "step":
            assert command.steps is not None
            if self._state.snapshot()["lifecycle"] != "paused":
                raise RuntimeError("simulation must be paused before stepping")
            self._timeline.step(command.steps)
            detail = f"advanced {command.steps} physics step(s)"
            resource_uri = f"uav-sim://session/{command.session_id}/world"
        else:
            assert command.vehicle_id is not None
            self._fleet_loop.take_control((command.vehicle_id,))
            commander = self._commander(command.vehicle_id)
            if command.command == "arm":
                commander.arm()
                detail = "vehicle armed"
            elif command.command == "takeoff":
                assert command.relative_altitude_m is not None
                if commander.status().flight_state != "armed":
                    raise RuntimeError("vehicle must be armed before takeoff")
                commander.takeoff(command.relative_altitude_m)
                detail = "vehicle takeoff accepted"
            elif command.command == "land":
                commander.land()
                detail = "vehicle landing accepted"
            else:
                raise AssertionError("validated command was not handled")
            resource_uri = (
                f"uav-sim://session/{command.session_id}/vehicle/{command.vehicle_id}"
            )
        return {"accepted": True, "detail": detail, "resource_uri": resource_uri}

    def _execute_operation(self, operation: DurableOperation) -> dict[str, object]:
        self._state.require_session(operation.session_id)
        if operation.operation == "run_scenario":
            if operation.parameters:
                raise ValueError("this scenario accepts no runtime parameter overrides")
            duration = self._duration(operation)
            final_time = self._state.wait_for_simulation_delta(
                duration, timeout_seconds=max(120.0, duration * 20.0)
            )
            snapshot = self._state.snapshot()
            output = {
                "session_id": operation.session_id,
                "elapsed_seconds": duration,
                "final_simulation_time_s": final_time,
                "collision_count": sum(
                    vehicle["collision_count"] for vehicle in snapshot["vehicles"]
                ),
                "recording_keys": self._state.recording_keys(),
            }
            return {"result": "run_scenario", "output": output}
        if operation.operation == "capture_dataset":
            duration = self._duration(operation)
            supported = {
                "camera/down",
                "imu",
                "pose",
                "vehicle_state",
                "tile_metrics",
            }
            unknown = sorted(set(operation.sensors or ()) - supported)
            if unknown:
                raise ValueError(f"unsupported capture sensors: {unknown}")
            self._state.wait_for_simulation_delta(
                duration, timeout_seconds=max(120.0, duration * 20.0)
            )
            return {
                "result": "capture_dataset",
                "output": {
                    "session_id": operation.session_id,
                    "elapsed_seconds": duration,
                    "recording_keys": self._state.recording_keys(),
                },
            }
        if operation.operation == "execute_mission":
            return self._execute_mission(operation)
        raise AssertionError("validated operation was not handled")

    def _execute_mission(self, operation: DurableOperation) -> dict[str, object]:
        assert operation.mission_id is not None
        assert operation.vehicles is not None
        world_revision_uri = self._state.snapshot()["world"]["revision_uri"]
        if operation.expected_world_revision_uri != world_revision_uri:
            raise ValueError(
                "mission expected world revision "
                f"{operation.expected_world_revision_uri!r} does not match "
                f"{world_revision_uri!r}"
            )
        vehicle_ids = [mission.vehicle_id for mission in operation.vehicles]
        if len(vehicle_ids) != len(set(vehicle_ids)):
            raise ValueError("a mission may name each vehicle only once")
        self._fleet_loop.take_control(tuple(vehicle_ids))
        started_at = _timestamp()
        self._recording.log_mission(
            operation.mission_id, "running", {"vehicle_ids": vehicle_ids}
        )
        try:
            with concurrent.futures.ThreadPoolExecutor(
                max_workers=len(operation.vehicles), thread_name_prefix="px4-mission"
            ) as executor:
                futures = [
                    executor.submit(
                        self._commander(mission.vehicle_id).execute_mission,
                        mission.waypoints,
                    )
                    for mission in operation.vehicles
                ]
                completed_waypoints = sum(future.result() for future in futures)
        except BaseException as error:
            self._recording.log_mission(
                operation.mission_id, "failed", {"error": str(error)}
            )
            raise
        finished_at = _timestamp()
        self._recording.log_mission(
            operation.mission_id,
            "completed",
            {"completed_waypoints": completed_waypoints},
        )
        return {
            "result": "execute_mission",
            "output": {
                "mission_id": operation.mission_id,
                "lifecycle": "completed",
                "started_at": started_at,
                "finished_at": finished_at,
                "completed_waypoints": completed_waypoints,
                "recording_keys": self._state.recording_keys(),
            },
        }

    def _commander(self, vehicle_id: str) -> Px4Commander:
        try:
            return self._commanders[vehicle_id]
        except KeyError as error:
            raise ValueError(f"unknown vehicle {vehicle_id!r}") from error

    @staticmethod
    def _duration(operation: DurableOperation) -> float:
        assert operation.duration_seconds is not None
        return operation.duration_seconds


class AdapterServer:
    def __init__(self, config: RuntimeConfig, application: web.Application) -> None:
        self._config = config
        self._application = application
        self._thread: threading.Thread | None = None
        self._loop: asyncio.AbstractEventLoop | None = None
        self._runner: web.AppRunner | None = None
        self._started = threading.Event()
        self._error: BaseException | None = None

    def start(self) -> None:
        self._thread = threading.Thread(
            target=self._run, name="uav-adapter-http", daemon=True
        )
        self._thread.start()
        if not self._started.wait(30.0):
            raise TimeoutError("UAV adapter HTTP server did not start")
        if self._error is not None:
            raise RuntimeError("UAV adapter HTTP server failed") from self._error

    def close(self) -> None:
        if self._loop is not None and self._runner is not None:
            future = asyncio.run_coroutine_threadsafe(
                self._runner.cleanup(), self._loop
            )
            future.result(timeout=30.0)
            self._loop.call_soon_threadsafe(self._loop.stop)
        if self._thread is not None:
            self._thread.join(timeout=30.0)

    def _run(self) -> None:
        try:
            self._loop = asyncio.new_event_loop()
            asyncio.set_event_loop(self._loop)
            self._runner = web.AppRunner(self._application, access_log=None)
            self._loop.run_until_complete(self._runner.setup())
            site = web.TCPSite(
                self._runner, self._config.adapter_host, self._config.adapter_port
            )
            self._loop.run_until_complete(site.start())
            self._started.set()
            self._loop.run_forever()
        except BaseException as error:
            self._error = error
            self._started.set()


def _identity(field: str, value: object) -> str:
    if not isinstance(value, str) or not 1 <= len(value) <= 128:
        raise ValueError(f"{field} must be a 1-128 character identity")
    if not all(
        character.isascii() and (character.isalnum() or character in {"_", "-", "."})
        for character in value
    ):
        raise ValueError(
            f"{field} must contain only ASCII letters, digits, underscore, dash, or dot"
        )
    return value
