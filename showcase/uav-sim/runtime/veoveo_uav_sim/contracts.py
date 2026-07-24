from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping


class ContractError(ValueError):
    pass


def _object(value: Any, context: str) -> dict[str, Any]:
    if not isinstance(value, dict) or not all(isinstance(key, str) for key in value):
        raise ContractError(f"{context} must be a JSON object")
    return value


def _exact_fields(value: Mapping[str, Any], required: set[str], context: str) -> None:
    actual = set(value)
    if actual != required:
        missing = sorted(required - actual)
        unknown = sorted(actual - required)
        raise ContractError(f"{context} fields invalid; missing={missing}, unknown={unknown}")


def _identity(value: Any, field: str) -> str:
    if not isinstance(value, str) or not 1 <= len(value) <= 128:
        raise ContractError(f"{field} must be a 1-128 character identity")
    if not all(character.isascii() and (character.isalnum() or character in "_-.") for character in value):
        raise ContractError(f"{field} contains an invalid character")
    return value


def _number(value: Any, field: str, minimum: float, maximum: float) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ContractError(f"{field} must be a number")
    result = float(value)
    if not minimum <= result <= maximum:
        raise ContractError(f"{field} must be between {minimum} and {maximum}")
    return result


@dataclass(frozen=True, slots=True)
class DirectCommand:
    command: str
    session_id: str
    vehicle_id: str | None = None
    steps: int | None = None
    relative_altitude_m: float | None = None


@dataclass(frozen=True, slots=True)
class Waypoint:
    latitude_degrees: float
    longitude_degrees: float
    ellipsoid_height_m: float
    speed_mps: float
    hold_seconds: float


@dataclass(frozen=True, slots=True)
class VehicleMission:
    vehicle_id: str
    waypoints: tuple[Waypoint, ...]


@dataclass(frozen=True, slots=True)
class DurableOperation:
    operation: str
    session_id: str
    duration_seconds: float | None = None
    parameters: Mapping[str, str] | None = None
    sensors: tuple[str, ...] | None = None
    mission_id: str | None = None
    expected_world_revision_uri: str | None = None
    vehicles: tuple[VehicleMission, ...] | None = None


def parse_command(payload: Any) -> DirectCommand:
    value = _object(payload, "command")
    command = value.get("command")
    if command in {"pause", "resume", "reset"}:
        _exact_fields(value, {"command", "session_id"}, "command")
        return DirectCommand(command, _identity(value["session_id"], "session_id"))
    if command == "step":
        _exact_fields(value, {"command", "session_id", "steps"}, "command")
        steps = value["steps"]
        if isinstance(steps, bool) or not isinstance(steps, int) or not 1 <= steps <= 10_000:
            raise ContractError("steps must be an integer between 1 and 10000")
        return DirectCommand(command, _identity(value["session_id"], "session_id"), steps=steps)
    if command in {"arm", "land"}:
        _exact_fields(value, {"command", "session_id", "vehicle_id"}, "command")
        return DirectCommand(
            command,
            _identity(value["session_id"], "session_id"),
            vehicle_id=_identity(value["vehicle_id"], "vehicle_id"),
        )
    if command == "takeoff":
        _exact_fields(
            value, {"command", "session_id", "vehicle_id", "relative_altitude_m"}, "command"
        )
        return DirectCommand(
            command,
            _identity(value["session_id"], "session_id"),
            vehicle_id=_identity(value["vehicle_id"], "vehicle_id"),
            relative_altitude_m=_number(
                value["relative_altitude_m"], "relative_altitude_m", 0.5, 500.0
            ),
        )
    raise ContractError("command must be pause, resume, reset, step, arm, takeoff, or land")


def _waypoint(value: Any) -> Waypoint:
    value = _object(value, "waypoint")
    _exact_fields(value, {"position", "speed_mps", "hold_seconds"}, "waypoint")
    position = _object(value["position"], "waypoint.position")
    _exact_fields(
        position,
        {"latitude_degrees", "longitude_degrees", "ellipsoid_height_m"},
        "waypoint.position",
    )
    return Waypoint(
        latitude_degrees=_number(position["latitude_degrees"], "latitude_degrees", -90.0, 90.0),
        longitude_degrees=_number(
            position["longitude_degrees"], "longitude_degrees", -180.0, 180.0
        ),
        ellipsoid_height_m=_number(
            position["ellipsoid_height_m"], "ellipsoid_height_m", -1_000.0, 100_000.0
        ),
        speed_mps=_number(value["speed_mps"], "speed_mps", 0.1, 100.0),
        hold_seconds=_number(value["hold_seconds"], "hold_seconds", 0.0, 3_600.0),
    )


def parse_operation(payload: Any) -> DurableOperation:
    envelope = _object(payload, "operation")
    _exact_fields(envelope, {"operation", "input"}, "operation")
    operation = envelope["operation"]
    value = _object(envelope["input"], "operation.input")
    if operation == "run_scenario":
        _exact_fields(value, {"session_id", "duration_seconds", "parameters"}, "run_scenario")
        parameters = value["parameters"]
        if not isinstance(parameters, dict) or not all(
            isinstance(key, str) and isinstance(item, str) for key, item in parameters.items()
        ):
            raise ContractError("parameters must map strings to strings")
        return DurableOperation(
            operation,
            _identity(value["session_id"], "session_id"),
            duration_seconds=_number(value["duration_seconds"], "duration_seconds", 0.1, 86_400.0),
            parameters=parameters,
        )
    if operation == "capture_dataset":
        _exact_fields(value, {"session_id", "duration_seconds", "sensors"}, "capture_dataset")
        sensors = value["sensors"]
        if not isinstance(sensors, list) or not 1 <= len(sensors) <= 128 or not all(
            isinstance(sensor, str) and sensor for sensor in sensors
        ):
            raise ContractError("sensors must contain 1-128 non-empty strings")
        return DurableOperation(
            operation,
            _identity(value["session_id"], "session_id"),
            duration_seconds=_number(value["duration_seconds"], "duration_seconds", 0.1, 86_400.0),
            sensors=tuple(sensors),
        )
    if operation == "execute_mission":
        _exact_fields(
            value,
            {
                "session_id",
                "mission_id",
                "expected_world_revision_uri",
                "vehicles",
            },
            "execute_mission",
        )
        vehicles = value["vehicles"]
        if not isinstance(vehicles, list) or not 1 <= len(vehicles) <= 256:
            raise ContractError("vehicles must contain 1-256 missions")
        parsed_vehicles: list[VehicleMission] = []
        for vehicle in vehicles:
            vehicle = _object(vehicle, "vehicle mission")
            _exact_fields(vehicle, {"vehicle_id", "waypoints"}, "vehicle mission")
            waypoints = vehicle["waypoints"]
            if not isinstance(waypoints, list) or not 1 <= len(waypoints) <= 10_000:
                raise ContractError("waypoints must contain 1-10000 entries")
            parsed_vehicles.append(
                VehicleMission(
                    _identity(vehicle["vehicle_id"], "vehicle_id"),
                    tuple(_waypoint(waypoint) for waypoint in waypoints),
                )
            )
        return DurableOperation(
            operation,
            _identity(value["session_id"], "session_id"),
            mission_id=_identity(value["mission_id"], "mission_id"),
            expected_world_revision_uri=(
                value["expected_world_revision_uri"]
                if isinstance(value["expected_world_revision_uri"], str)
                else ""
            ),
            vehicles=tuple(parsed_vehicles),
        )
    raise ContractError("operation must be run_scenario, execute_mission, or capture_dataset")
