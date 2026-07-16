from __future__ import annotations

import math
import threading
import time
from dataclasses import dataclass

from pymavlink import mavutil

from .contracts import Waypoint


PX4_CUSTOM_MAIN_MODE_AUTO = 4
PX4_CUSTOM_SUB_MODE_AUTO_MISSION = 4


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
        self._absolute_altitude_m = origin_height_m

    def connect(self, timeout_seconds: float = 60.0) -> None:
        deadline = time.monotonic() + timeout_seconds
        with self._lock:
            self._connection = mavutil.mavlink_connection(
                f"udpout:127.0.0.1:{18_570 + self.instance}",
                source_system=255,
                source_component=mavutil.mavlink.MAV_COMP_ID_MISSIONPLANNER,
            )
            while time.monotonic() < deadline:
                self._connection.mav.heartbeat_send(
                    mavutil.mavlink.MAV_TYPE_GCS,
                    mavutil.mavlink.MAV_AUTOPILOT_INVALID,
                    0,
                    0,
                    mavutil.mavlink.MAV_STATE_ACTIVE,
                )
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
        elif (
            self._landed_state == mavutil.mavlink.MAV_LANDED_STATE_ON_GROUND
            and self._has_flown
        ):
            flight_state = "landed"
        elif self._armed:
            flight_state = "armed"
        elif self._connected:
            flight_state = "standby"
        else:
            flight_state = "initializing"
        return Px4Status(self._connected, flight_state, self._battery_percent)

    def arm(self) -> None:
        self._command(mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM, 1.0)

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

    def execute_mission(self, waypoints: tuple[Waypoint, ...], timeout_seconds: float = 1_800.0) -> int:
        with self._lock:
            self._require_connection()
            mission_items, waypoint_sequences = self._mission_items(waypoints)
            self._upload_mission(mission_items)
            self._send_command_locked(
                mavutil.mavlink.MAV_CMD_DO_SET_MODE,
                float(mavutil.mavlink.MAV_MODE_FLAG_CUSTOM_MODE_ENABLED),
                float(PX4_CUSTOM_MAIN_MODE_AUTO),
                float(PX4_CUSTOM_SUB_MODE_AUTO_MISSION),
            )
            if not self._armed:
                self._send_command_locked(mavutil.mavlink.MAV_CMD_COMPONENT_ARM_DISARM, 1.0)

            deadline = time.monotonic() + timeout_seconds
            reached: set[int] = set()
            final_sequence = mission_items[-1]["sequence"]
            while time.monotonic() < deadline:
                message = self._connection.recv_match(blocking=True, timeout=1.0)
                if message is None:
                    continue
                self._consume(message)
                if message.get_type() == "MISSION_ITEM_REACHED":
                    sequence = int(message.seq)
                    if sequence in waypoint_sequences:
                        reached.add(sequence)
                    if sequence == final_sequence:
                        return len(waypoints)
                elif message.get_type() == "MISSION_CURRENT" and int(message.seq) > final_sequence:
                    return len(waypoints)
            raise TimeoutError(f"mission on {self.vehicle_id} did not complete")

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
        self._connection.mav.command_long_send(
            self._target_system,
            self._target_component,
            command,
            0,
            *values[:7],
        )
        deadline = time.monotonic() + 15.0
        while time.monotonic() < deadline:
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

    def _upload_mission(self, mission_items: list[dict[str, float | int]]) -> None:
        self._connection.mav.mission_clear_all_send(
            self._target_system,
            self._target_component,
            mavutil.mavlink.MAV_MISSION_TYPE_MISSION,
        )
        self._connection.mav.mission_count_send(
            self._target_system,
            self._target_component,
            len(mission_items),
            mavutil.mavlink.MAV_MISSION_TYPE_MISSION,
        )
        deadline = time.monotonic() + 60.0
        while time.monotonic() < deadline:
            message = self._connection.recv_match(blocking=True, timeout=1.0)
            if message is None:
                continue
            self._consume(message)
            message_type = message.get_type()
            if message_type in {"MISSION_REQUEST", "MISSION_REQUEST_INT"}:
                sequence = int(message.seq)
                if not 0 <= sequence < len(mission_items):
                    raise RuntimeError(f"PX4 requested invalid mission item {sequence}")
                self._send_mission_item(mission_items[sequence])
            elif message_type == "MISSION_ACK":
                if int(message.type) != mavutil.mavlink.MAV_MISSION_ACCEPTED:
                    raise RuntimeError(f"PX4 rejected mission with result {message.type}")
                return
        raise TimeoutError("PX4 mission upload did not finish")

    def _send_mission_item(self, item: dict[str, float | int]) -> None:
        self._connection.mav.mission_item_int_send(
            self._target_system,
            self._target_component,
            int(item["sequence"]),
            int(item["frame"]),
            int(item["command"]),
            1 if int(item["sequence"]) == 0 else 0,
            1,
            float(item["param1"]),
            float(item["param2"]),
            float(item["param3"]),
            float(item["param4"]),
            int(item["x"]),
            int(item["y"]),
            float(item["z"]),
            mavutil.mavlink.MAV_MISSION_TYPE_MISSION,
        )

    def _mission_items(
        self,
        waypoints: tuple[Waypoint, ...],
    ) -> tuple[list[dict[str, float | int]], set[int]]:
        items: list[dict[str, float | int]] = []
        waypoint_sequences: set[int] = set()
        for index, waypoint in enumerate(waypoints):
            items.append(
                Px4Commander._item(
                    len(items),
                    mavutil.mavlink.MAV_FRAME_MISSION,
                    mavutil.mavlink.MAV_CMD_DO_CHANGE_SPEED,
                    param1=1.0,
                    param2=waypoint.speed_mps,
                )
            )
            command = (
                mavutil.mavlink.MAV_CMD_NAV_TAKEOFF
                if index == 0 and not self._has_flown
                else mavutil.mavlink.MAV_CMD_NAV_WAYPOINT
            )
            sequence = len(items)
            items.append(
                Px4Commander._item(
                    sequence,
                    mavutil.mavlink.MAV_FRAME_GLOBAL_RELATIVE_ALT_INT,
                    command,
                    param1=waypoint.hold_seconds,
                    x=round(waypoint.latitude_degrees * 10_000_000),
                    y=round(waypoint.longitude_degrees * 10_000_000),
                    z=waypoint.ellipsoid_height_m - self._origin_height_m,
                )
            )
            waypoint_sequences.add(sequence)
        return items, waypoint_sequences

    @staticmethod
    def _item(
        sequence: int,
        frame: int,
        command: int,
        *,
        param1: float = 0.0,
        param2: float = 0.0,
        param3: float = 0.0,
        param4: float = math.nan,
        x: int = 0,
        y: int = 0,
        z: float = 0.0,
    ) -> dict[str, float | int]:
        return {
            "sequence": sequence,
            "frame": frame,
            "command": command,
            "param1": param1,
            "param2": param2,
            "param3": param3,
            "param4": param4,
            "x": x,
            "y": y,
            "z": z,
        }

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
            self._absolute_altitude_m = float(message.alt) / 1_000.0

    def _require_connection(self) -> None:
        if self._connection is None or not self._connected:
            raise RuntimeError(f"PX4 is not connected for {self.vehicle_id}")
