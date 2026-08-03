from __future__ import annotations

import logging
import math
import threading
import time
from typing import Protocol

from .config import FleetLoopConfig
from .contracts import Waypoint
from .geo import enu_to_geodetic
from .world_config import GeoreferenceOrigin


LOGGER = logging.getLogger("veoveo.uav_sim.fleet_loop")


class FleetStatus(Protocol):
    flight_state: str


class FleetCommander(Protocol):
    def status(self) -> FleetStatus: ...

    def arm(self) -> None: ...

    def takeoff(self, relative_altitude_m: float) -> None: ...

    def execute_mission(
        self, waypoints: tuple[Waypoint, ...], timeout_seconds: float = 1_800.0
    ) -> int: ...

    def interrupt_mission(self) -> None: ...

    def clear_mission_interrupt(self) -> None: ...


def vehicle_loop_route(
    config: FleetLoopConfig,
    origin: GeoreferenceOrigin,
    vehicle_index: int,
    vehicle_count: int,
) -> tuple[Waypoint, ...]:
    if vehicle_index < 0 or vehicle_index >= vehicle_count:
        raise ValueError("vehicle_index must identify one configured fleet vehicle")
    phase = 2.0 * math.pi * vehicle_index / vehicle_count
    east_radius_m = (
        config.east_radius_m + config.radial_separation_m * vehicle_index
    )
    north_radius_m = (
        config.north_radius_m + config.radial_separation_m * vehicle_index
    )
    altitude_m = (
        config.relative_altitude_m + config.vertical_separation_m * vehicle_index
    )
    route: list[Waypoint] = []
    for waypoint_index in range(config.waypoint_count):
        angle = phase + 2.0 * math.pi * waypoint_index / config.waypoint_count
        latitude, longitude, ellipsoid_height = enu_to_geodetic(
            config.center_east_m + east_radius_m * math.cos(angle),
            config.center_north_m + north_radius_m * math.sin(angle),
            altitude_m,
            origin.latitude_degrees,
            origin.longitude_degrees,
            origin.ellipsoid_height_m,
        )
        route.append(
            Waypoint(
                latitude_degrees=latitude,
                longitude_degrees=longitude,
                ellipsoid_height_m=ellipsoid_height,
                speed_mps=config.speed_mps,
                hold_seconds=config.hold_seconds,
            )
        )
    return tuple(route)


class FleetLoopController:
    def __init__(
        self,
        config: FleetLoopConfig,
        origin: GeoreferenceOrigin,
        commanders: dict[str, FleetCommander],
    ) -> None:
        self._commanders = commanders
        self._routes = {
            vehicle_id: vehicle_loop_route(
                config, origin, index, len(commanders)
            )
            for index, (vehicle_id, _commander) in enumerate(commanders.items())
        }
        self._takeoff_altitudes = {
            vehicle_id: config.relative_altitude_m
            + config.vertical_separation_m * index
            for index, vehicle_id in enumerate(commanders)
        }
        self._takeoff_timeout_seconds = config.takeoff_timeout_seconds
        self._lock = threading.Lock()
        self._stop = threading.Event()
        self._overridden: set[str] = set()
        self._relinquished = {
            vehicle_id: threading.Event() for vehicle_id in commanders
        }
        self._threads: dict[str, threading.Thread] = {}
        self._failure: tuple[str, BaseException] | None = None

    def start(self) -> None:
        with self._lock:
            if self._threads:
                return
            for vehicle_id, commander in self._commanders.items():
                thread = threading.Thread(
                    target=self._run_vehicle,
                    args=(vehicle_id, commander),
                    name=f"fleet-loop-{vehicle_id}",
                    daemon=True,
                )
                self._threads[vehicle_id] = thread
                thread.start()

    def take_control(
        self, vehicle_ids: tuple[str, ...], timeout_seconds: float = 30.0
    ) -> None:
        unknown = sorted(set(vehicle_ids) - set(self._commanders))
        if unknown:
            raise ValueError(f"unknown fleet vehicles: {unknown}")
        with self._lock:
            self._overridden.update(vehicle_ids)
            running = bool(self._threads)
        for vehicle_id in vehicle_ids:
            self._commanders[vehicle_id].interrupt_mission()
        if running:
            deadline = time.monotonic() + timeout_seconds
            for vehicle_id in vehicle_ids:
                remaining = deadline - time.monotonic()
                if remaining <= 0.0 or not self._relinquished[vehicle_id].wait(
                    remaining
                ):
                    raise TimeoutError(
                        f"default fleet loop did not relinquish {vehicle_id}"
                    )
        for vehicle_id in vehicle_ids:
            self._commanders[vehicle_id].clear_mission_interrupt()

    def close(self) -> None:
        self._stop.set()
        for commander in self._commanders.values():
            commander.interrupt_mission()
        for thread in self._threads.values():
            thread.join(timeout=30.0)

    def raise_if_failed(self) -> None:
        with self._lock:
            failure = self._failure
        if failure is not None:
            vehicle_id, error = failure
            raise RuntimeError(
                f"default fleet loop failed for {vehicle_id}"
            ) from error

    def _run_vehicle(
        self, vehicle_id: str, commander: FleetCommander
    ) -> None:
        try:
            if self._is_overridden(vehicle_id):
                return
            status = commander.status()
            if status.flight_state in {"standby", "landed"}:
                commander.arm()
                commander.takeoff(self._takeoff_altitudes[vehicle_id])
            deadline = time.monotonic() + self._takeoff_timeout_seconds
            while not self._stop.is_set() and not self._is_overridden(vehicle_id):
                if commander.status().flight_state == "flying":
                    break
                if time.monotonic() >= deadline:
                    raise TimeoutError(
                        f"{vehicle_id} did not reach its default fleet loop altitude "
                        f"within {self._takeoff_timeout_seconds:g} seconds"
                    )
                time.sleep(0.25)
            while not self._stop.is_set() and not self._is_overridden(vehicle_id):
                commander.execute_mission(self._routes[vehicle_id])
        except BaseException as error:
            if not self._stop.is_set() and not self._is_overridden(vehicle_id):
                with self._lock:
                    self._failure = (vehicle_id, error)
                LOGGER.exception(
                    "default fleet loop failed for %s: %s", vehicle_id, error
                )
        finally:
            self._relinquished[vehicle_id].set()

    def _is_overridden(self, vehicle_id: str) -> bool:
        with self._lock:
            return vehicle_id in self._overridden
