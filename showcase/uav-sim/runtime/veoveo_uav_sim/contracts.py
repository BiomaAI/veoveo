from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping, Annotated, Literal
from pydantic import BaseModel, ConfigDict, Field, TypeAdapter, ValidationError, model_validator


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


class WireModel(BaseModel):
    model_config = ConfigDict(hide_input_in_errors=True, extra="forbid", strict=True)

    @model_validator(mode="before")
    @classmethod
    def current_members(cls, value):
        if isinstance(value, dict):
            wire_names = {field.alias or name for name, field in cls.model_fields.items()}
            unknown = set(value) - wire_names
            if unknown:
                raise ValidationError.from_exception_data(cls.__name__, [
                    {"type": "extra_forbidden", "loc": (key,), "input": None}
                    for key in sorted(unknown)
                ])
        return value



class SessionCommand(WireModel):
    command: Literal["pause", "resume", "reset"]
    sessionId: str


class StepCommand(WireModel):
    command: Literal["step"]
    sessionId: str
    steps: int


class VehicleCommand(WireModel):
    command: Literal["arm", "land"]
    sessionId: str
    vehicleId: str


class TakeoffCommand(WireModel):
    command: Literal["takeoff"]
    sessionId: str
    vehicleId: str
    relativeAltitudeM: float


CommandWire = Annotated[SessionCommand | StepCommand | VehicleCommand | TakeoffCommand, Field(discriminator="command")]
COMMAND_ADAPTER = TypeAdapter(CommandWire, config=ConfigDict(hide_input_in_errors=True))


class PositionWire(WireModel):
    latitudeDegrees: float
    longitudeDegrees: float
    ellipsoidHeightM: float


class WaypointWire(WireModel):
    position: PositionWire
    speedMps: float
    holdSeconds: float


class VehicleMissionWire(WireModel):
    vehicleId: str
    waypoints: list[WaypointWire]


class ScenarioInput(WireModel):
    sessionId: str
    durationSeconds: float
    parameters: dict[str, str]


class CaptureInput(WireModel):
    sessionId: str
    durationSeconds: float
    sensors: list[str]


class MissionInput(WireModel):
    sessionId: str
    missionId: str
    expectedWorldRevisionUri: str
    vehicles: list[VehicleMissionWire]


class ScenarioOperation(WireModel):
    operation: Literal["run_scenario"]
    input: ScenarioInput


class CaptureOperation(WireModel):
    operation: Literal["capture_dataset"]
    input: CaptureInput


class MissionOperation(WireModel):
    operation: Literal["execute_mission"]
    input: MissionInput


OperationWire = Annotated[ScenarioOperation | CaptureOperation | MissionOperation, Field(discriminator="operation")]
OPERATION_ADAPTER = TypeAdapter(OperationWire, config=ConfigDict(hide_input_in_errors=True))


def validation_diagnostic(error: ValidationError) -> str:
    """HTTP diagnostics contain validation kinds, never submitted values."""
    kinds = sorted({item["type"] for item in error.errors(include_input=False, include_context=False, include_url=False)})
    return "invalid adapter input: " + ", ".join(kinds)


def _admit(adapter: TypeAdapter, value: Any) -> None:
    try:
        adapter.validate_python(value)
    except ValidationError as error:
        raise ContractError(validation_diagnostic(error)) from error


def parse_command(payload: Any) -> DirectCommand:
    _admit(COMMAND_ADAPTER, payload)
    value = _object(payload, "command")
    command = value.get("command")
    if command in {"pause", "resume", "reset"}:
        _exact_fields(value, {"command", 'sessionId'}, "command")
        return DirectCommand(command, _identity(value['sessionId'], 'sessionId'))
    if command == "step":
        _exact_fields(value, {"command", 'sessionId', "steps"}, "command")
        steps = value["steps"]
        if isinstance(steps, bool) or not isinstance(steps, int) or not 1 <= steps <= 10_000:
            raise ContractError("steps must be an integer between 1 and 10000")
        return DirectCommand(command, _identity(value['sessionId'], 'sessionId'), steps=steps)
    if command in {"arm", "land"}:
        _exact_fields(value, {"command", 'sessionId', 'vehicleId'}, "command")
        return DirectCommand(
            command,
            _identity(value['sessionId'], 'sessionId'),
            vehicle_id=_identity(value['vehicleId'], 'vehicleId'),
        )
    if command == "takeoff":
        _exact_fields(
            value, {"command", 'sessionId', 'vehicleId', 'relativeAltitudeM'}, "command"
        )
        return DirectCommand(
            command,
            _identity(value['sessionId'], 'sessionId'),
            vehicle_id=_identity(value['vehicleId'], 'vehicleId'),
            relative_altitude_m=_number(
                value['relativeAltitudeM'], 'relativeAltitudeM', 0.5, 500.0
            ),
        )
    raise ContractError("command must be pause, resume, reset, step, arm, takeoff, or land")


def _waypoint(value: Any) -> Waypoint:
    value = _object(value, "waypoint")
    _exact_fields(value, {"position", 'speedMps', 'holdSeconds'}, "waypoint")
    position = _object(value["position"], "waypoint.position")
    _exact_fields(
        position,
        {'latitudeDegrees', 'longitudeDegrees', 'ellipsoidHeightM'},
        "waypoint.position",
    )
    return Waypoint(
        latitude_degrees=_number(position['latitudeDegrees'], 'latitudeDegrees', -90.0, 90.0),
        longitude_degrees=_number(
            position['longitudeDegrees'], 'longitudeDegrees', -180.0, 180.0
        ),
        ellipsoid_height_m=_number(
            position['ellipsoidHeightM'], 'ellipsoidHeightM', -1_000.0, 100_000.0
        ),
        speed_mps=_number(value['speedMps'], 'speedMps', 0.1, 100.0),
        hold_seconds=_number(value['holdSeconds'], 'holdSeconds', 0.0, 3_600.0),
    )


def parse_operation(payload: Any) -> DurableOperation:
    _admit(OPERATION_ADAPTER, payload)
    envelope = _object(payload, "operation")
    _exact_fields(envelope, {"operation", "input"}, "operation")
    operation = envelope["operation"]
    value = _object(envelope["input"], "operation.input")
    if operation == "run_scenario":
        _exact_fields(value, {'sessionId', 'durationSeconds', "parameters"}, "run_scenario")
        parameters = value["parameters"]
        if not isinstance(parameters, dict) or not all(
            isinstance(key, str) and isinstance(item, str) for key, item in parameters.items()
        ):
            raise ContractError("parameters must map strings to strings")
        return DurableOperation(
            operation,
            _identity(value['sessionId'], 'sessionId'),
            duration_seconds=_number(value['durationSeconds'], 'durationSeconds', 0.1, 86_400.0),
            parameters=parameters,
        )
    if operation == "capture_dataset":
        _exact_fields(value, {'sessionId', 'durationSeconds', "sensors"}, "capture_dataset")
        sensors = value["sensors"]
        if not isinstance(sensors, list) or not 1 <= len(sensors) <= 128 or not all(
            isinstance(sensor, str) and sensor for sensor in sensors
        ):
            raise ContractError("sensors must contain 1-128 non-empty strings")
        return DurableOperation(
            operation,
            _identity(value['sessionId'], 'sessionId'),
            duration_seconds=_number(value['durationSeconds'], 'durationSeconds', 0.1, 86_400.0),
            sensors=tuple(sensors),
        )
    if operation == "execute_mission":
        _exact_fields(
            value,
            {
                'sessionId',
                'missionId',
                'expectedWorldRevisionUri',
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
            _exact_fields(vehicle, {'vehicleId', "waypoints"}, "vehicle mission")
            waypoints = vehicle["waypoints"]
            if not isinstance(waypoints, list) or not 1 <= len(waypoints) <= 10_000:
                raise ContractError("waypoints must contain 1-10000 entries")
            parsed_vehicles.append(
                VehicleMission(
                    _identity(vehicle['vehicleId'], 'vehicleId'),
                    tuple(_waypoint(waypoint) for waypoint in waypoints),
                )
            )
        return DurableOperation(
            operation,
            _identity(value['sessionId'], 'sessionId'),
            mission_id=_identity(value['missionId'], 'missionId'),
            expected_world_revision_uri=(
                value['expectedWorldRevisionUri']
                if isinstance(value['expectedWorldRevisionUri'], str)
                else ""
            ),
            vehicles=tuple(parsed_vehicles),
        )
    raise ContractError("operation must be run_scenario, execute_mission, or capture_dataset")
