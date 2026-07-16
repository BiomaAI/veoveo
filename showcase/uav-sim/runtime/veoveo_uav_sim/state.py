from __future__ import annotations

import copy
import threading
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Callable

from .config import RuntimeConfig
from .geo import enu_to_geodetic


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


class RuntimeState:
    def __init__(self, config: RuntimeConfig) -> None:
        self._config = config
        self._condition = threading.Condition()
        started_at = _timestamp()
        self._state: dict[str, Any] = {
            "session_id": config.session_id,
            "lifecycle": "starting",
            "simulation_time_s": 0.0,
            "physics_step": 0,
            "frame_uri": config.frame_uri,
            "georeference_origin": {
                "latitude_degrees": config.origin_latitude_degrees,
                "longitude_degrees": config.origin_longitude_degrees,
                "ellipsoid_height_m": config.origin_ellipsoid_height_m,
            },
            "tiles": {
                "lifecycle": "connecting",
                "source": "google_photorealistic_3d_tiles",
                "ion_asset_id": config.cesium_ion_asset_id,
                "resident_tiles": 0,
                "loading_tiles": 0,
                "failed_tiles": 0,
            },
            "vehicles": [],
            "recordings": [
                {
                    "application_id": "veoveo-uav-sim",
                    "recording_key": str(config.recording_key),
                    "active": True,
                    "camera_streams": [
                        f"/world/uav-sim/{config.session_id}/vehicle/uav-{index + 1}/camera/front"
                        for index in range(config.vehicle_count)
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
        loading_tiles: int,
        failed_tiles: int = 0,
        diagnostic: str | None = None,
    ) -> None:
        with self._condition:
            tiles = self._state["tiles"]
            tiles.update(
                lifecycle=lifecycle,
                resident_tiles=max(0, resident_tiles),
                loading_tiles=max(0, loading_tiles),
                failed_tiles=max(0, failed_tiles),
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

    def update_vehicles(self, vehicles: list[VehicleTelemetry]) -> None:
        with self._condition:
            self._state["vehicles"] = [self._vehicle_state(vehicle) for vehicle in vehicles]
            self._touch()

    def set_recording_active(self, active: bool) -> None:
        with self._condition:
            self._state["recordings"][0]["active"] = active
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
            self._config.origin_latitude_degrees,
            self._config.origin_longitude_degrees,
            self._config.origin_ellipsoid_height_m,
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
