from __future__ import annotations

import asyncio
import concurrent.futures
import threading
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Callable

from aiohttp import web

from .config import RuntimeConfig
from .contracts import ContractError, DirectCommand, DurableOperation, parse_command, parse_operation
from .px4 import Px4Commander
from .recording import RecordingPublisher
from .state import RuntimeState


def _timestamp() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


@dataclass(frozen=True, slots=True)
class TimelineControls:
    pause: Callable[[], None]
    resume: Callable[[], None]
    reset: Callable[[], None]
    step: Callable[[int], None]


class AdapterApplication:
    def __init__(
        self,
        config: RuntimeConfig,
        state: RuntimeState,
        timeline: TimelineControls,
        commanders: dict[str, Px4Commander],
        recording: RecordingPublisher,
    ) -> None:
        self._config = config
        self._state = state
        self._timeline = timeline
        self._commanders = commanders
        self._recording = recording
        self._app = web.Application(client_max_size=2 * 1024 * 1024)
        self._app.add_routes(
            [
                web.get("/healthz", self._health),
                web.get("/readyz", self._ready),
                web.get("/v1/state", self._get_state),
                web.post("/v1/commands", self._command),
                web.post("/v1/operations", self._operation),
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
        ready = (
            snapshot["lifecycle"] in {"ready", "running", "paused"}
            and snapshot["tiles"]["lifecycle"] == "ready"
            and bool(snapshot["cameras"])
            and all(camera["lifecycle"] == "ready" for camera in snapshot["cameras"])
            and bool(snapshot["vehicles"])
            and all(vehicle["px4_connected"] for vehicle in snapshot["vehicles"])
            and snapshot["recordings"][0]["active"]
        )
        return web.json_response({"ready": ready}, status=200 if ready else 503)

    async def _get_state(self, _request: web.Request) -> web.Response:
        return web.json_response(self._state.snapshot())

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
                "camera/front",
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
        if operation.frame_uri != self._config.frame_uri:
            raise ValueError(
                f"mission frame {operation.frame_uri!r} does not match {self._config.frame_uri!r}"
            )
        vehicle_ids = [mission.vehicle_id for mission in operation.vehicles]
        if len(vehicle_ids) != len(set(vehicle_ids)):
            raise ValueError("a mission may name each vehicle only once")
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
    def __init__(self, config: RuntimeConfig, application: AdapterApplication) -> None:
        self._config = config
        self._application = application
        self._thread: threading.Thread | None = None
        self._loop: asyncio.AbstractEventLoop | None = None
        self._runner: web.AppRunner | None = None
        self._started = threading.Event()
        self._error: BaseException | None = None

    def start(self) -> None:
        self._thread = threading.Thread(target=self._run, name="uav-adapter-http", daemon=True)
        self._thread.start()
        if not self._started.wait(30.0):
            raise TimeoutError("UAV adapter HTTP server did not start")
        if self._error is not None:
            raise RuntimeError("UAV adapter HTTP server failed") from self._error

    def close(self) -> None:
        if self._loop is not None and self._runner is not None:
            future = asyncio.run_coroutine_threadsafe(self._runner.cleanup(), self._loop)
            future.result(timeout=30.0)
            self._loop.call_soon_threadsafe(self._loop.stop)
        if self._thread is not None:
            self._thread.join(timeout=30.0)

    def _run(self) -> None:
        try:
            self._loop = asyncio.new_event_loop()
            asyncio.set_event_loop(self._loop)
            self._runner = web.AppRunner(self._application.application, access_log=None)
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
