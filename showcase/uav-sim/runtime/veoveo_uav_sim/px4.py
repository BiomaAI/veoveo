from __future__ import annotations

import math
import threading
import time
from dataclasses import dataclass

from pymavlink import mavutil

from .contracts import Waypoint
from .geo import horizontal_distance_m


GCS_HEARTBEAT_INTERVAL_SECONDS = 1.0
WAYPOINT_HORIZONTAL_TOLERANCE_M = 1.0
WAYPOINT_VERTICAL_TOLERANCE_M = 0.75


@dataclass(frozen=True, slots=True)
class Px4Status:
    connected: bool
    flight_state: str
    battery_percent: float


class Px4Commander:
    def __init__(self, instance: int, origin_height_m: float) -> None:
        self.instance = instance
        self.vehicle_id = f"uav-{instance + 1}"
        self._origin_height_m = origin_height_m
        self._lock = threading.Lock()
        self._connection = None
        self._target_system = instance + 1
        self._target_component = 1
        self._connected = False
        self._armed = False
        self._has_flown = False
        self._landed_state = mavutil.mavlink.MAV_LANDED_STATE_UNDEFINED
        self._battery_percent = 100.0
        self._latitude_degrees: float | None = None
        self._longitude_degrees: float | None = None
        self._absolute_altitude_m = origin_height_m
        self._last_gcs_heartbeat_at = 0.0
        self._mission_interrupt = threading.Event()

    def connect(self, timeout_seconds: float = 60.0) -> None:
        deadline = time.monotonic() + timeout_seconds
        with self._lock:
            self._connection = mavutil.mavlink_connection(
                f"udpin:127.0.0.1:{14_550 + self.instance}",
                source_system=255,
                source_component=mavutil.mavlink.MAV_COMP_ID_MISSIONPLANNER,
            )
            # PX4's GCS channel binds 18570+instance and expects the GCS on
            # 14550+instance. Pymavlink's input socket learns peers from
            # received datagrams, but PX4 also needs the first heartbeat to
            # learn this return path, so seed the pinned local endpoint.
            self._connection.clients.add(("127.0.0.1", 18_570 + self.instance))
            while time.monotonic() < deadline:
                self._send_gcs_heartbeat_locked()
                message = self._connection.recv_match(type="HEARTBEAT", blocking=True, timeout=1.0)
                if message is not None and message.get_srcSystem() == self._target_system:
                    self._target_component = message.get_srcComponent()
                    self._consume(message)
                    self._connected = True
                    return
        raise TimeoutError(f"PX4 instance {self.instance} did not publish a heartbeat")

    def status(self) -> Px4Status:
        if self._lock.acquire(blocking=False):
            try:
                if self._connection is not None:
                    self._send_gcs_heartbeat_locked_if_due()
                    for _ in range(64):
                        message = self._connection.recv_match(blocking=False)
                        if message is None:
                            break
                        self._consume(message)
            finally:
                self._lock.release()
        if self._landed_state == mavutil.mavlink.MAV_LANDED_STATE_TAKEOFF:
            flight_state = "taking_off"
        elif self._landed_state == mavutil.mavlink.MAV_LANDED_STATE_LANDING:
            flight_state = "landing"
        elif self._landed_state == mavutil.mavlink.MAV_LANDED_STATE_IN_AIR:
            flight_state = "flying"
        elif self._armed:
            flight_state = "armed"
        elif (
            self._landed_state == mavutil.mavlink.MAV_LANDED_STATE_ON_GROUND
            and self._has_flown
        ):
            flight_state = "landed"
        elif self._connected:
            flight_state = "standby"
        else:
            flight_state = "initializing"
        return Px4Status(self._connected, flight_state, self._battery_percent)

    def arm(self) -> None:
        self._command(mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM, 1.0)
        deadline = time.monotonic() + 15.0
        with self._lock:
            while time.monotonic() < deadline:
                self._send_gcs_heartbeat_locked_if_due()
                message = self._connection.recv_match(blocking=True, timeout=1.0)
                if message is not None:
                    self._consume(message)
                if self._armed:
                    return
        raise TimeoutError(f"PX4 did not report {self.vehicle_id} armed")

    def takeoff(self, relative_altitude_m: float) -> None:
        target_altitude = max(self._absolute_altitude_m, self._origin_height_m) + relative_altitude_m
        self._command(
            mavutil.mavlink.MAV_CMD_NAV_TAKEOFF,
            math.nan,
            0.0,
            0.0,
            math.nan,
            math.nan,
            math.nan,
            target_altitude,
        )

    def land(self) -> None:
        self._mission_interrupt.set()
        try:
            self._command(
                mavutil.mavlink.MAV_CMD_NAV_LAND,
                0.0,
                0.0,
                0.0,
                math.nan,
                math.nan,
                math.nan,
                math.nan,
            )
        finally:
            self._mission_interrupt.clear()

    def execute_mission(self, waypoints: tuple[Waypoint, ...], timeout_seconds: float = 1_800.0) -> int:
        with self._lock:
            self._require_connection()
            if self._mission_interrupt.is_set():
                raise RuntimeError(f"mission on {self.vehicle_id} was interrupted")
            if not self._armed:
                self._send_command_locked(mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM, 1.0)

            deadline = time.monotonic() + timeout_seconds
            completed = 0
            for waypoint in waypoints:
                self._send_reposition_locked(waypoint)
                reached_at: float | None = None
                while time.monotonic() < deadline:
                    if self._mission_interrupt.is_set():
                        raise RuntimeError(f"mission on {self.vehicle_id} was interrupted")
                    self._send_gcs_heartbeat_locked_if_due()
                    message = self._connection.recv_match(blocking=True, timeout=1.0)
                    if message is not None:
                        self._consume(message)
                    if not self._waypoint_reached_locked(waypoint):
                        reached_at = None
                        continue
                    reached_at = reached_at or time.monotonic()
                    if time.monotonic() - reached_at >= waypoint.hold_seconds:
                        completed += 1
                        break
                else:
                    raise TimeoutError(f"mission on {self.vehicle_id} did not complete")
            return completed

    def close(self) -> None:
        with self._lock:
            if self._connection is not None:
                self._connection.close()
                self._connection = None
            self._connected = False

    def _command(self, command: int, *parameters: float) -> None:
        with self._lock:
            self._require_connection()
            self._send_command_locked(command, *parameters)

    def _send_command_locked(self, command: int, *parameters: float) -> None:
        values = list(parameters) + [0.0] * (7 - len(parameters))
        self._send_gcs_heartbeat_locked()
        self._connection.mav.command_long_send(
            self._target_system,
            self._target_component,
            command,
            0,
            *values[:7],
        )
        self._await_command_ack_locked(command)

    def _send_reposition_locked(self, waypoint: Waypoint) -> None:
        self._send_gcs_heartbeat_locked()
        self._connection.mav.command_int_send(
            self._target_system,
            self._target_component,
            mavutil.mavlink.MAV_FRAME_GLOBAL_INT,
            mavutil.mavlink.MAV_CMD_DO_REPOSITION,
            0,
            0,
            waypoint.speed_mps,
            float(mavutil.mavlink.MAV_DO_REPOSITION_FLAGS_CHANGE_MODE),
            0.0,
            math.nan,
            round(waypoint.latitude_degrees * 10_000_000),
            round(waypoint.longitude_degrees * 10_000_000),
            waypoint.ellipsoid_height_m,
        )
        self._await_command_ack_locked(mavutil.mavlink.MAV_CMD_DO_REPOSITION)

    def _await_command_ack_locked(self, command: int) -> None:
        deadline = time.monotonic() + 15.0
        while time.monotonic() < deadline:
            self._send_gcs_heartbeat_locked_if_due()
            message = self._connection.recv_match(blocking=True, timeout=1.0)
            if message is None:
                continue
            self._consume(message)
            if message.get_type() != "COMMAND_ACK" or int(message.command) != command:
                continue
            if int(message.result) == mavutil.mavlink.MAV_RESULT_IN_PROGRESS:
                continue
            if int(message.result) != mavutil.mavlink.MAV_RESULT_ACCEPTED:
                raise RuntimeError(
                    f"PX4 rejected MAVLink command {command} with result {message.result}"
                )
            return
        raise TimeoutError(f"PX4 did not acknowledge MAVLink command {command}")

    def _waypoint_reached_locked(self, waypoint: Waypoint) -> bool:
        if self._latitude_degrees is None or self._longitude_degrees is None:
            return False
        horizontal_error = horizontal_distance_m(
            self._latitude_degrees,
            self._longitude_degrees,
            waypoint.latitude_degrees,
            waypoint.longitude_degrees,
        )
        vertical_error = abs(self._absolute_altitude_m - waypoint.ellipsoid_height_m)
        return (
            horizontal_error <= WAYPOINT_HORIZONTAL_TOLERANCE_M
            and vertical_error <= WAYPOINT_VERTICAL_TOLERANCE_M
        )

    def _consume(self, message) -> None:
        message_type = message.get_type()
        if message_type == "HEARTBEAT" and message.get_srcSystem() == self._target_system:
            self._armed = bool(
                int(message.base_mode) & mavutil.mavlink.MAV_MODE_FLAG_SAFETY_ARMED
            )
            self._connected = True
        elif message_type == "EXTENDED_SYS_STATE":
            self._landed_state = int(message.landed_state)
            if self._landed_state == mavutil.mavlink.MAV_LANDED_STATE_IN_AIR:
                self._has_flown = True
        elif message_type == "SYS_STATUS" and int(message.battery_remaining) >= 0:
            self._battery_percent = float(message.battery_remaining)
        elif message_type == "GLOBAL_POSITION_INT":
            self._latitude_degrees = float(message.lat) / 10_000_000.0
            self._longitude_degrees = float(message.lon) / 10_000_000.0
            self._absolute_altitude_m = float(message.alt) / 1_000.0

    def _send_gcs_heartbeat_locked(self) -> None:
        self._connection.mav.heartbeat_send(
            mavutil.mavlink.MAV_TYPE_GCS,
            mavutil.mavlink.MAV_AUTOPILOT_INVALID,
            0,
            0,
            mavutil.mavlink.MAV_STATE_ACTIVE,
        )
        self._last_gcs_heartbeat_at = time.monotonic()

    def _send_gcs_heartbeat_locked_if_due(self) -> None:
        if time.monotonic() - self._last_gcs_heartbeat_at >= GCS_HEARTBEAT_INTERVAL_SECONDS:
            self._send_gcs_heartbeat_locked()

    def _require_connection(self) -> None:
        if self._connection is None or not self._connected:
            raise RuntimeError(f"PX4 is not connected for {self.vehicle_id}")
