from __future__ import annotations

import copy
import threading
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Callable

from .config import RuntimeConfig
from .geo import enu_to_geodetic
from .camera_quality import CameraFrameQuality
from .operator_camera import CameraStreamPolicy
from .operator_camera_config import live_camera_descriptor
from .physics_batch import FleetPhysicsTiming
from .world_config import WorldConfiguration


def _timestamp() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


@dataclass(frozen=True, slots=True)
class VehicleTelemetry:
    vehicle_id: str
    position_enu: tuple[float, float, float]
    attitude_xyzw: tuple[float, float, float, float]
    linear_velocity_enu_mps: tuple[float, float, float]
    flight_state: str
    battery_percent: float
    px4_connected: bool
    collision_count: int = 0


def initial_runtime_timing(config: RuntimeConfig) -> dict[str, int | float]:
    return {
        "physics_hz": config.physics_hz,
        "native_rendering_hz": config.rendering_hz,
        "render_cycles": 0,
        "physics_steps": 0,
        "refresh_states_wall_seconds": 0.0,
        "vehicle_update_wall_seconds": 0.0,
        "flush_forces_wall_seconds": 0.0,
        "after_step_wall_seconds": 0.0,
        "native_update_wall_seconds": 0.0,
        "render_cycle_wall_seconds": 0.0,
        "maximum_physics_step_ms": 0.0,
        "maximum_native_update_ms": 0.0,
        "maximum_render_cycle_ms": 0.0,
    }


class RuntimeState:
    def __init__(self, config: RuntimeConfig, world: WorldConfiguration) -> None:
        self._config = config
        self._world = world
        self._condition = threading.Condition()
        started_at = _timestamp()
        self._state: dict[str, Any] = {
            "session_id": config.session_id,
            "lifecycle": "starting",
            "simulation_time_s": 0.0,
            "physics_step": 0,
            "timing": initial_runtime_timing(config),
            "world": world.as_dict(),
            "tiles": {
                "lifecycle": "connecting",
                "source": "google_photorealistic_3d_tiles",
                "ion_asset_id": config.cesium_ion_asset_id,
                "resident_tiles": 0,
                "visible_tiles": 0,
                "loading_tiles": 0,
                "recovery_count": 0,
            },
            "cameras": [
                {
                    "vehicle_id": config.camera.vehicle_id,
                    "entity_path": (
                        f"/world/uav-sim/{config.session_id}/vehicle/"
                        f"{config.camera.vehicle_id}/camera/down"
                    ),
                    "lifecycle": "warming",
                    "width": config.camera.width,
                    "height": config.camera.height,
                    "codec": "h264",
                    "encoder": "nvidia_nvenc",
                    "frames_observed": 0,
                    "mean_luma": 0.0,
                    "dynamic_range": 0,
                    "robust_dynamic_range": 0,
                    "luma_standard_deviation": 0.0,
                    "non_black_fraction": 0.0,
                    "content": "black",
                }
            ],
            "live_cameras": [
                live_camera_descriptor(config.session_id, camera)
                for camera in config.operator_live_view.cameras
            ],
            "stream_products": [
                {
                    "streamProductId": f"product-{camera.camera_id}",
                    "cameraId": camera.camera_id,
                    "physicalSlot": camera.physical_slot,
                    "lifecycle": "inactive",
                    "activeViewerLeases": 0,
                    "connectedViewers": 0,
                    "nvencSessions": 0,
                    "encodedFrames": 0,
                }
                for camera in config.operator_live_view.cameras
                if camera.stream_policy is not CameraStreamPolicy.DISABLED
            ],
            "vehicles": [],
            "recordings": [
                {
                    "application_id": "veoveo-uav-sim",
                    "recording_key": str(config.recording_key),
                    "active": True,
                    "publisher_lifecycle": "connecting",
                    "queue_capacity": config.recording.queue_capacity,
                    "queued_events": 0,
                    "dropped_events": 0,
                    "camera_streams": [
                        f"/world/uav-sim/{config.session_id}/vehicle/"
                        f"{config.camera.vehicle_id}/camera/down"
                    ],
                    "started_at": started_at,
                }
            ],
            "updated_at": started_at,
        }

    def snapshot(self) -> dict[str, Any]:
        with self._condition:
            return copy.deepcopy(self._state)

    def require_session(self, session_id: str) -> None:
        if session_id != self._config.session_id:
            raise ValueError(f"unknown simulation session {session_id!r}")

    def set_lifecycle(self, lifecycle: str) -> None:
        with self._condition:
            self._state["lifecycle"] = lifecycle
            self._touch()

    def set_tiles(
        self,
        lifecycle: str,
        resident_tiles: int,
        visible_tiles: int,
        loading_tiles: int,
        recovery_count: int,
        diagnostic: str | None = None,
    ) -> None:
        with self._condition:
            tiles = self._state["tiles"]
            tiles.update(
                lifecycle=lifecycle,
                resident_tiles=max(0, resident_tiles),
                visible_tiles=max(0, visible_tiles),
                loading_tiles=max(0, loading_tiles),
                recovery_count=max(0, recovery_count),
            )
            if diagnostic:
                tiles["diagnostic"] = diagnostic
            else:
                tiles.pop("diagnostic", None)
            self._touch()

    def advance(self, simulation_time_s: float, physics_step: int) -> None:
        with self._condition:
            self._state["simulation_time_s"] = simulation_time_s
            self._state["physics_step"] = physics_step
            self._touch()

    def observe_render_cycle(
        self,
        native_update_wall_seconds: float,
        render_cycle_wall_seconds: float,
        physics_timing: FleetPhysicsTiming,
    ) -> None:
        if native_update_wall_seconds < 0.0 or render_cycle_wall_seconds < 0.0:
            raise ValueError("render timing cannot be negative")
        if render_cycle_wall_seconds < native_update_wall_seconds:
            raise ValueError("render-cycle timing cannot be shorter than native update")
        with self._condition:
            timing = self._state["timing"]
            timing["render_cycles"] += 1
            timing["physics_steps"] = physics_timing.physics_steps
            timing["refresh_states_wall_seconds"] = (
                physics_timing.refresh_states_wall_seconds
            )
            timing["vehicle_update_wall_seconds"] = (
                physics_timing.vehicle_update_wall_seconds
            )
            timing["flush_forces_wall_seconds"] = (
                physics_timing.flush_forces_wall_seconds
            )
            timing["after_step_wall_seconds"] = physics_timing.after_step_wall_seconds
            timing["native_update_wall_seconds"] += native_update_wall_seconds
            timing["render_cycle_wall_seconds"] += render_cycle_wall_seconds
            timing["maximum_physics_step_ms"] = physics_timing.maximum_physics_step_ms
            timing["maximum_native_update_ms"] = max(
                timing["maximum_native_update_ms"],
                native_update_wall_seconds * 1_000.0,
            )
            timing["maximum_render_cycle_ms"] = max(
                timing["maximum_render_cycle_ms"],
                render_cycle_wall_seconds * 1_000.0,
            )
            self._touch()

    def update_camera(
        self,
        vehicle_id: str,
        lifecycle: str,
        frames_observed: int,
        quality: CameraFrameQuality,
        diagnostic_code: str | None = None,
        diagnostic: str | None = None,
    ) -> None:
        with self._condition:
            for camera in self._state["cameras"]:
                if camera["vehicle_id"] == vehicle_id:
                    camera.update(
                        lifecycle=lifecycle,
                        frames_observed=max(0, frames_observed),
                        mean_luma=quality.mean_luma,
                        dynamic_range=quality.dynamic_range,
                        robust_dynamic_range=quality.robust_dynamic_range,
                        luma_standard_deviation=quality.luma_standard_deviation,
                        non_black_fraction=quality.non_black_fraction,
                        content=quality.content,
                    )
                    if diagnostic_code:
                        camera["diagnostic_code"] = diagnostic_code
                    else:
                        camera.pop("diagnostic_code", None)
                    if diagnostic:
                        camera["diagnostic"] = diagnostic
                    else:
                        camera.pop("diagnostic", None)
                    self._touch()
                    return
            raise ValueError(f"unknown camera vehicle {vehicle_id!r}")

    def update_stream_products(self, products: list[dict[str, object]]) -> None:
        by_camera = {str(product["cameraId"]): product for product in products}
        with self._condition:
            self._state["stream_products"] = copy.deepcopy(products)
            for camera in self._state["live_cameras"]:
                product = by_camera.get(str(camera["cameraId"]))
                if product is None:
                    camera["health"] = "healthy"
                    camera.pop("lastFrameAt", None)
                    continue
                lifecycle = product["lifecycle"]
                camera["health"] = {
                    "inactive": "healthy",
                    "starting": "warming",
                    "ready": "healthy",
                    "failed": "failed",
                }[str(lifecycle)]
                if "lastFrameAt" in product:
                    camera["lastFrameAt"] = product["lastFrameAt"]
                else:
                    camera.pop("lastFrameAt", None)
            self._touch()

    def update_vehicles(self, vehicles: list[VehicleTelemetry]) -> None:
        with self._condition:
            self._state["vehicles"] = [self._vehicle_state(vehicle) for vehicle in vehicles]
            self._touch()

    def set_recording_active(self, active: bool) -> None:
        with self._condition:
            self._state["recordings"][0]["active"] = active
            self._touch()

    def update_recording_publisher(
        self,
        lifecycle: str,
        queued_events: int,
        dropped_events: int,
        diagnostic: str | None,
    ) -> None:
        with self._condition:
            recording = self._state["recordings"][0]
            recording.update(
                publisher_lifecycle=lifecycle,
                queued_events=max(0, queued_events),
                dropped_events=max(0, dropped_events),
            )
            if diagnostic:
                recording["diagnostic"] = diagnostic
            else:
                recording.pop("diagnostic", None)
            self._touch()

    def wait_for_simulation_delta(self, duration_seconds: float, timeout_seconds: float) -> float:
        with self._condition:
            start = float(self._state["simulation_time_s"])
            target = start + duration_seconds
            if not self._condition.wait_for(
                lambda: float(self._state["simulation_time_s"]) >= target
                or self._state["lifecycle"] in {"failed", "stopped"},
                timeout_seconds,
            ):
                raise TimeoutError("simulation did not advance for the requested duration")
            if self._state["lifecycle"] in {"failed", "stopped"}:
                raise RuntimeError(f"simulation entered {self._state['lifecycle']}")
            return float(self._state["simulation_time_s"])

    def mutate_vehicle(self, vehicle_id: str, callback: Callable[[dict[str, Any]], None]) -> None:
        with self._condition:
            for vehicle in self._state["vehicles"]:
                if vehicle["vehicle_id"] == vehicle_id:
                    callback(vehicle)
                    self._touch()
                    return
            raise ValueError(f"unknown vehicle {vehicle_id!r}")

    def recording_keys(self) -> list[str]:
        with self._condition:
            return [item["recording_key"] for item in self._state["recordings"]]

    def _vehicle_state(self, telemetry: VehicleTelemetry) -> dict[str, Any]:
        east, north, up = telemetry.position_enu
        latitude, longitude, height = enu_to_geodetic(
            east,
            north,
            up,
            self._world.georeference_origin.latitude_degrees,
            self._world.georeference_origin.longitude_degrees,
            self._world.georeference_origin.ellipsoid_height_m,
        )
        x, y, z, w = telemetry.attitude_xyzw
        velocity_east, velocity_north, velocity_up = telemetry.linear_velocity_enu_mps
        return {
            "vehicle_id": telemetry.vehicle_id,
            "flight_state": telemetry.flight_state,
            "wgs84": {
                "latitude_degrees": latitude,
                "longitude_degrees": longitude,
                "ellipsoid_height_m": height,
            },
            "enu": {"east_m": east, "north_m": north, "up_m": up},
            "ned": {"north_m": north, "east_m": east, "down_m": -up},
            "attitude_xyzw": {"x": x, "y": y, "z": z, "w": w},
            "linear_velocity_enu_mps": {
                "east_m": velocity_east,
                "north_m": velocity_north,
                "up_m": velocity_up,
            },
            "battery_percent": max(0.0, min(100.0, telemetry.battery_percent)),
            "collision_count": max(0, telemetry.collision_count),
            "px4_connected": telemetry.px4_connected,
        }

    def _touch(self) -> None:
        self._state["updated_at"] = _timestamp()
        self._condition.notify_all()
