"""Private adapter outputs admitted before JSON or NDJSON leaves the runtime."""
from __future__ import annotations

from typing import Annotated, Literal, Self, TypeVar

from pydantic import AfterValidator, AwareDatetime, BaseModel, ConfigDict, Field, TypeAdapter, ValidationError, model_validator

from .world_config import SimulationWorldBindingWire
from .contracts import WireModel
from yarl import URL


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


class Wire(WireModel):
    model_config = ConfigDict(extra="forbid", strict=True, hide_input_in_errors=True)


class EnuVector(Wire):
    eastM: Finite
    northM: Finite
    upM: Finite


class NedVector(Wire):
    northM: Finite
    eastM: Finite
    downM: Finite


class Quaternion(Wire):
    x: Finite
    y: Finite
    z: Finite
    w: Finite


class Position(Wire):
    latitudeDegrees: Annotated[Finite, Field(ge=-90, le=90)]
    longitudeDegrees: Annotated[Finite, Field(ge=-180, le=180)]
    ellipsoidHeightM: Finite


class WorldBinding(SimulationWorldBindingWire):
    georeferenceOrigin: Position


class Direction(Wire):
    east: Annotated[Finite, Field(ge=-1, le=1)]
    north: Annotated[Finite, Field(ge=-1, le=1)]
    up: Annotated[Finite, Field(ge=-1, le=1)]


class RenderPose(Wire):
    positionErrorM: Nonnegative
    forwardErrorDegrees: Annotated[Finite, Field(ge=0, le=180)]
    renderedPositionEnuM: EnuVector
    renderedForwardEnu: Direction


class TileFailure(Wire):
    code: Literal["provider_session_rejected", "credentials_rejected", "asset_unavailable", "quota_exceeded", "provider_unavailable", "transport_failed", "request_failed"]
    loadType: Literal["ion_endpoint", "tileset_json", "tile_content", "unknown"]
    httpStatus: U16
    generation: U64


class TileState(Wire):
    lifecycle: Literal["connecting", "streaming", "ready", "refreshing", "degraded"]
    source: str
    ionAssetId: U64
    residentTiles: U64
    visibleTiles: U64
    loadingTiles: U64
    geometriesLoaded: U64
    geometriesRendered: U64
    materialsLoaded: U64
    providerGeneration: U64
    eventSequence: U64
    refreshCount: U64
    lastFailure: TileFailure | None = None
    diagnostic: str | None = None


class CameraState(Wire):
    vehicleId: Identity
    entityPath: str
    lifecycle: Literal["warming", "ready", "degraded", "failed"]
    width: U32
    height: U32
    frameRateHz: U32
    codec: Literal["h264"]
    encoder: Literal["nvidia_nvenc"]
    transport: Literal["rtsp_rtp"]
    framesObserved: U64
    lastAccessUnitBytes: U64
    lastFrameKeyframe: bool
    renderPose: RenderPose | None = None
    diagnostic: str | None = None


class VehicleState(Wire):
    vehicleId: Identity
    flightState: Literal["initializing", "standby", "armed", "taking_off", "flying", "landing", "landed", "failed"]
    wgs84: Position
    enu: EnuVector
    ned: NedVector
    attitudeXyzw: Quaternion
    linearVelocityEnuMps: EnuVector
    batteryPercent: Annotated[Finite, Field(ge=0, le=100)]
    collisionCount: U64
    px4Connected: bool


class RecordingState(Wire):
    applicationId: str
    recordingKey: Identity
    active: bool
    publisherLifecycle: Literal["connecting", "ready", "degraded", "stopped"]
    queueCapacity: U32
    queuedEvents: U32
    droppedEvents: U64
    diagnostic: str | None = None
    cameraStreams: list[str]
    startedAt: Timestamp


class RuntimeTiming(Wire):
    physicsHz: Annotated[U32, Field(ge=30, le=1000)]
    nativeRenderingHz: Annotated[U32, Field(ge=1, le=120)]
    renderCycles: U64
    physicsSteps: U64
    refreshStatesWallSeconds: Finite
    vehicleUpdateWallSeconds: Finite
    stateUpdateWallSeconds: Finite
    dynamicsUpdateWallSeconds: Finite
    sensorUpdateWallSeconds: Finite
    backendStateWallSeconds: Finite
    flushForcesWallSeconds: Finite
    afterStepWallSeconds: Finite
    nativeUpdateWallSeconds: Finite
    renderCycleWallSeconds: Finite
    maximumPhysicsStepMs: Finite
    maximumNativeUpdateMs: Finite
    maximumRenderCycleMs: Finite


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
    sessionId: Identity
    lifecycle: SimulationLifecycle
    simulationTimeS: Finite
    physicsStep: U64
    timing: RuntimeTiming
    world: WorldBinding | None = None
    tiles: TileState
    cameras: list[CameraState]
    liveCameras: list[LiveCamera]
    streamProducts: list[StreamProduct]
    vehicles: list[VehicleState]
    recordings: list[RecordingState]
    updatedAt: Timestamp

    @model_validator(mode="after")
    def admitted_children(self) -> Self:
        vehicles = {v.vehicleId for v in self.vehicles}
        live = {c.cameraId for c in self.liveCameras}
        if len(vehicles) != len(self.vehicles) or len(live) != len(self.liveCameras):
            raise ValueError("state_duplicate_identity")
        if len({(c.vehicleId, c.entityPath) for c in self.cameras}) != len(self.cameras) or any(c.vehicleId not in vehicles for c in self.cameras):
            raise ValueError("state_camera_parent")
        if any(c.sessionId != self.sessionId for c in self.liveCameras):
            raise ValueError("state_camera_session")
        if len({p.streamProductId for p in self.streamProducts}) != len(self.streamProducts) or any(r.cameraId not in live for p in self.streamProducts for r in p.cameraRegions):
            raise ValueError("state_product_parent")
        if len({r.recordingKey for r in self.recordings}) != len(self.recordings):
            raise ValueError("state_recording_identity")
        return self


def acknowledgement_resource(value: str) -> str:
    uri = URL(value)
    path = uri.parts[1:]
    if (uri.scheme != "uav-sim" or uri.raw_authority != "session" or uri.query_string or uri.fragment
            or str(uri) != value or "%" in value or not path):
        raise ValueError("acknowledgement_resource")
    _IDENTITY_ADAPTER.validate_python(path[0])
    if len(path) == 1 or (len(path) == 2 and path[1] == "world"):
        return value
    if len(path) == 3 and path[1] == "vehicle":
        _IDENTITY_ADAPTER.validate_python(path[2])
        return value
    raise ValueError("acknowledgement_resource")


_IDENTITY_ADAPTER = TypeAdapter(Identity)
AcknowledgementResource = Annotated[str, AfterValidator(acknowledgement_resource)]


def command_resource(session_id: str, *, vehicle_id: str | None = None, world: bool = False) -> str:
    session = _IDENTITY_ADAPTER.validate_python(session_id)
    uri = URL("uav-sim://session") / session
    if vehicle_id is not None:
        if world:
            raise ValueError("acknowledgement_resource")
        uri = uri / "vehicle" / _IDENTITY_ADAPTER.validate_python(vehicle_id)
    elif world:
        uri = uri / "world"
    return acknowledgement_resource(str(uri))


class WorldAcknowledgement(Wire):
    accepted: bool
    world: WorldBinding
    resourceUri: AcknowledgementResource


class CommandAcknowledgement(Wire):
    accepted: bool
    detail: str
    resourceUri: AcknowledgementResource


class ScenarioOutput(Wire):
    sessionId: Identity
    elapsedSeconds: Nonnegative
    finalSimulationTimeS: Nonnegative
    collisionCount: U64
    recordingKeys: list[str]


class MissionOutput(Wire):
    missionId: Identity
    lifecycle: Literal["pending", "running", "completed", "cancelled", "failed"]
    startedAt: Timestamp
    finishedAt: Timestamp
    completedWaypoints: U64
    recordingKeys: list[str]

    @model_validator(mode="after")
    def ordered_completion(self) -> Self:
        if _TIMESTAMP_ADAPTER.validate_python(self.finishedAt) < _TIMESTAMP_ADAPTER.validate_python(self.startedAt):
            raise ValueError("completion_time")
        return self


class CaptureOutput(Wire):
    sessionId: Identity
    elapsedSeconds: Nonnegative
    recordingKeys: list[str]


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
    schemaTag: Literal["veoveo.ai/uav-runtime-event/v2"] = Field(alias="schema")
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
