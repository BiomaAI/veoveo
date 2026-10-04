"""Private adapter outputs admitted before JSON or NDJSON leaves the runtime."""
from __future__ import annotations

from typing import Annotated, Literal, Self, TypeVar

from pydantic import AfterValidator, AwareDatetime, BaseModel, ConfigDict, Field, TypeAdapter, ValidationError, model_validator

from .world_config import SimulationWorldBindingWire


def _identity(value: str) -> str:
    if value in {".", ".."}:
        raise ValueError("relative_identity")
    return value


_TIMESTAMP_ADAPTER = TypeAdapter(AwareDatetime)


def _timestamp(value: str) -> str:
    # Pydantic's maintained datetime parser owns calendar and timezone admission.
    # The string field's date prefix excludes its optional UNIX-seconds coercion.
    _TIMESTAMP_ADAPTER.validate_python(value)
    return value


Identity = Annotated[str, Field(min_length=1, max_length=128, pattern=r"^[A-Za-z0-9_.-]+$"), AfterValidator(_identity)]
Timestamp = Annotated[str, Field(pattern=r"^[+-]?\d{4,}-", json_schema_extra={"format": "date-time"}), AfterValidator(_timestamp)]
U16 = Annotated[int, Field(ge=0, le=2**16 - 1)]
U32 = Annotated[int, Field(ge=0, le=2**32 - 1)]
U64 = Annotated[int, Field(ge=0, le=2**64 - 1)]
Finite = Annotated[float, Field(allow_inf_nan=False)]
Nonnegative = Annotated[Finite, Field(ge=0)]
SimulationLifecycle = Literal["unconfigured", "starting", "ready", "running", "paused", "stopping", "stopped", "failed"]


class Wire(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True, hide_input_in_errors=True)


class EnuVector(Wire):
    east_m: Finite
    north_m: Finite
    up_m: Finite


class NedVector(Wire):
    north_m: Finite
    east_m: Finite
    down_m: Finite


class Quaternion(Wire):
    x: Finite
    y: Finite
    z: Finite
    w: Finite


class Position(Wire):
    latitude_degrees: Annotated[Finite, Field(ge=-90, le=90)]
    longitude_degrees: Annotated[Finite, Field(ge=-180, le=180)]
    ellipsoid_height_m: Finite


class WorldBinding(SimulationWorldBindingWire):
    georeference_origin: Position


class Direction(Wire):
    east: Annotated[Finite, Field(ge=-1, le=1)]
    north: Annotated[Finite, Field(ge=-1, le=1)]
    up: Annotated[Finite, Field(ge=-1, le=1)]


class RenderPose(Wire):
    position_error_m: Nonnegative
    forward_error_degrees: Annotated[Finite, Field(ge=0, le=180)]
    rendered_position_enu_m: EnuVector
    rendered_forward_enu: Direction


class TileFailure(Wire):
    code: Literal["provider_session_rejected", "credentials_rejected", "asset_unavailable", "quota_exceeded", "provider_unavailable", "transport_failed", "request_failed"]
    load_type: Literal["ion_endpoint", "tileset_json", "tile_content", "unknown"]
    http_status: U16
    generation: U64


class TileState(Wire):
    lifecycle: Literal["connecting", "streaming", "ready", "refreshing", "degraded"]
    source: str
    ion_asset_id: U64
    resident_tiles: U64
    visible_tiles: U64
    loading_tiles: U64
    geometries_loaded: U64
    geometries_rendered: U64
    materials_loaded: U64
    provider_generation: U64
    event_sequence: U64
    refresh_count: U64
    last_failure: TileFailure | None = None
    diagnostic: str | None = None


class CameraState(Wire):
    vehicle_id: Identity
    entity_path: str
    lifecycle: Literal["warming", "ready", "degraded", "failed"]
    width: U32
    height: U32
    frame_rate_hz: U32
    codec: Literal["h264"]
    encoder: Literal["nvidia_nvenc"]
    transport: Literal["rtsp_rtp"]
    frames_observed: U64
    last_access_unit_bytes: U64
    last_frame_keyframe: bool
    render_pose: RenderPose | None = None
    diagnostic: str | None = None


class VehicleState(Wire):
    vehicle_id: Identity
    flight_state: Literal["initializing", "standby", "armed", "taking_off", "flying", "landing", "landed", "failed"]
    wgs84: Position
    enu: EnuVector
    ned: NedVector
    attitude_xyzw: Quaternion
    linear_velocity_enu_mps: EnuVector
    battery_percent: Annotated[Finite, Field(ge=0, le=100)]
    collision_count: U64
    px4_connected: bool


class RecordingState(Wire):
    application_id: str
    recording_key: Identity
    active: bool
    publisher_lifecycle: Literal["connecting", "ready", "degraded", "stopped"]
    queue_capacity: U32
    queued_events: U32
    dropped_events: U64
    diagnostic: str | None = None
    camera_streams: list[str]
    started_at: Timestamp


class RuntimeTiming(Wire):
    physics_hz: Annotated[U32, Field(ge=30, le=1000)]
    native_rendering_hz: Annotated[U32, Field(ge=1, le=120)]
    render_cycles: U64
    physics_steps: U64
    refresh_states_wall_seconds: Finite
    vehicle_update_wall_seconds: Finite
    state_update_wall_seconds: Finite
    dynamics_update_wall_seconds: Finite
    sensor_update_wall_seconds: Finite
    backend_state_wall_seconds: Finite
    flush_forces_wall_seconds: Finite
    after_step_wall_seconds: Finite
    native_update_wall_seconds: Finite
    render_cycle_wall_seconds: Finite
    maximum_physics_step_ms: Finite
    maximum_native_update_ms: Finite
    maximum_render_cycle_ms: Finite


class Vector3(Wire):
    x: Finite
    y: Finite
    z: Finite


class Pose(Wire):
    positionM: Vector3
    orientationXyzw: Quaternion

    @model_validator(mode="after")
    def normalized(self) -> Self:
        q = self.orientationXyzw
        if abs(q.x*q.x + q.y*q.y + q.z*q.z + q.w*q.w - 1.0) > 1e-6:
            raise ValueError("pose_orientation")
        return self


class Smoothing(Wire):
    translationHalfLifeMs: Annotated[U32, Field(le=60_000)]
    rotationHalfLifeMs: Annotated[U32, Field(le=60_000)]
    teleportDistanceMillimetres: Annotated[U32, Field(ge=1, le=100_000_000)]
    resetAfterGapMs: Annotated[U32, Field(ge=1, le=600_000)]


class FixedRig(Wire):
    kind: Literal["fixed"]
    pose: Pose


class LookAtRig(Wire):
    kind: Literal["look_at"]
    eyeM: Vector3
    targetM: Vector3
    smoothing: Smoothing

    @model_validator(mode="after")
    def distinct_points(self) -> Self:
        if self.eyeM == self.targetM:
            raise ValueError("rig_points")
        return self


class OrbitRig(Wire):
    kind: Literal["orbit"]
    targetEntityId: Identity
    radiusM: Annotated[Finite, Field(gt=0.1)]
    azimuthDegrees: Finite
    elevationDegrees: Annotated[Finite, Field(ge=-89.9, le=89.9)]
    smoothing: Smoothing


class FollowRig(Wire):
    kind: Literal["follow_entity"]
    targetEntityId: Identity
    eyeOffsetFluM: Vector3
    targetOffsetFluM: Vector3
    smoothing: Smoothing


class ChaseRig(Wire):
    kind: Literal["chase_entity"]
    targetEntityId: Identity
    distanceM: Annotated[Finite, Field(gt=0.1)]
    heightM: Finite
    smoothing: Smoothing


class MountedRig(Wire):
    kind: Literal["stabilized_mounted_entity"]
    targetEntityId: Identity
    mount: Pose
    smoothing: Smoothing


class FormationRig(Wire):
    kind: Literal["formation_overview"]
    targetEntityIds: Annotated[list[Identity], Field(min_length=1, max_length=256)]
    paddingM: Nonnegative
    smoothing: Smoothing

    @model_validator(mode="after")
    def ordered_entities(self) -> Self:
        if any(left >= right for left, right in zip(self.targetEntityIds, self.targetEntityIds[1:])):
            raise ValueError("rig_entities")
        return self


CameraRig = Annotated[FixedRig | LookAtRig | OrbitRig | FollowRig | ChaseRig | MountedRig | FormationRig, Field(discriminator="kind")]


class LiveCamera(Wire):
    cameraId: Identity
    sessionId: Identity
    revision: Annotated[U64, Field(ge=1)]
    rig: CameraRig
    widthPx: Annotated[U32, Field(ge=1)]
    heightPx: Annotated[U32, Field(ge=1)]
    frameRateMillihertz: Annotated[U32, Field(ge=1)]
    verticalFovDegrees: Annotated[Finite, Field(ge=1, le=160)]
    nearClipM: Annotated[Finite, Field(gt=0)]
    farClipM: Finite
    streamPolicy: Literal["disabled", "continuous"]
    health: Literal["warming", "healthy", "stale", "failed"]
    lastFrameAt: Timestamp | None = None

    @model_validator(mode="after")
    def ordered_clipping(self) -> Self:
        if self.farClipM <= self.nearClipM:
            raise ValueError("camera_clipping")
        return self


class CameraRegion(Wire):
    cameraId: Identity
    xPx: U32
    yPx: U32
    widthPx: Annotated[U32, Field(ge=1)]
    heightPx: Annotated[U32, Field(ge=1)]


class StreamProduct(Wire):
    streamProductId: Identity
    cameraRegions: Annotated[list[CameraRegion], Field(min_length=1)]
    codedWidthPx: Annotated[U32, Field(ge=1)]
    codedHeightPx: Annotated[U32, Field(ge=1)]
    lifecycle: Literal["inactive", "starting", "ready", "failed"]
    activeViewers: U32
    connectedViewers: U32
    nvencSessions: U32
    encodedFrames: U64
    sourceToRenderP95Microseconds: U64 | None = None
    sourceToRenderSamples: U64
    lastFrameAt: Timestamp | None = None
    visible: bool | None = None
    diagnostic: str | None = None

    @model_validator(mode="after")
    def admitted_regions(self) -> Self:
        for index, region in enumerate(self.cameraRegions):
            if region.xPx + region.widthPx > self.codedWidthPx or region.yPx + region.heightPx > self.codedHeightPx:
                raise ValueError("product_region_bounds")
            for other in self.cameraRegions[index+1:]:
                overlaps = (region.xPx < other.xPx + other.widthPx and other.xPx < region.xPx + region.widthPx
                            and region.yPx < other.yPx + other.heightPx and other.yPx < region.yPx + region.heightPx)
                if region.cameraId == other.cameraId or overlaps:
                    raise ValueError("product_region_overlap")
        return self


class SimulationState(Wire):
    session_id: Identity
    lifecycle: SimulationLifecycle
    simulation_time_s: Finite
    physics_step: U64
    timing: RuntimeTiming
    world: WorldBinding | None = None
    tiles: TileState
    cameras: list[CameraState]
    live_cameras: list[LiveCamera]
    stream_products: list[StreamProduct]
    vehicles: list[VehicleState]
    recordings: list[RecordingState]
    updated_at: Timestamp


class WorldAcknowledgement(Wire):
    accepted: bool
    world: WorldBinding
    resource_uri: str


class CommandAcknowledgement(Wire):
    accepted: bool
    detail: str
    resource_uri: str


class ScenarioOutput(Wire):
    session_id: Identity
    elapsed_seconds: Nonnegative
    final_simulation_time_s: Nonnegative
    collision_count: U64
    recording_keys: list[str]


class MissionOutput(Wire):
    mission_id: Identity
    lifecycle: Literal["pending", "running", "completed", "cancelled", "failed"]
    started_at: Timestamp
    finished_at: Timestamp
    completed_waypoints: U64
    recording_keys: list[str]

    @model_validator(mode="after")
    def ordered_completion(self) -> Self:
        if _TIMESTAMP_ADAPTER.validate_python(self.finished_at) < _TIMESTAMP_ADAPTER.validate_python(self.started_at):
            raise ValueError("completion_time")
        return self


class CaptureOutput(Wire):
    session_id: Identity
    elapsed_seconds: Nonnegative
    recording_keys: list[str]


class ScenarioResult(Wire):
    result: Literal["run_scenario"]
    output: ScenarioOutput


class MissionResult(Wire):
    result: Literal["execute_mission"]
    output: MissionOutput


class CaptureResult(Wire):
    result: Literal["capture_dataset"]
    output: CaptureOutput


OperationResult = Annotated[ScenarioResult | MissionResult | CaptureResult, Field(discriminator="result")]
OPERATION_RESULT_ADAPTER = TypeAdapter(OperationResult)


class RuntimeEventWire(Wire):
    schema_tag: Literal["veoveo.ai/uav-runtime-event/v2"] = Field(alias="schema")
    event: Literal["adapter_ready", "ready"]
    sessionId: Identity
    generation: Annotated[U64, Field(ge=1)]


class OutputContractError(RuntimeError):
    pass


Output = TypeVar("Output", bound=BaseModel)


def admit_output(model: type[Output] | TypeAdapter, payload: object) -> dict[str, object]:
    """Preserve absent optional fields; rejection diagnostics expose only validator codes."""
    try:
        value = model.validate_python(payload) if isinstance(model, TypeAdapter) else model.model_validate(payload)
    except ValidationError as error:
        kinds = sorted({item["type"] for item in error.errors(include_input=False, include_context=False, include_url=False)})
        raise OutputContractError("invalid adapter output: " + ", ".join(kinds)) from None
    return value.model_dump(mode="json", by_alias=True, exclude_unset=True)
