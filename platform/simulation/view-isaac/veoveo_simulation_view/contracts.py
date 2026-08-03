from __future__ import annotations

import math
import re
from dataclasses import dataclass
from datetime import datetime
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from .lighting import GovernedLighting


IDENTITY = re.compile(r"^[A-Za-z0-9_.-]{1,128}$")
SCENE_SCHEMA = "veoveo.io/simulation-view-scene/v1"
POSE_CONTROL_SCHEMA = (
    "veoveo.io/simulation-view-pose-ingress-control/v2"
)


class ContractError(ValueError):
    pass


@dataclass(frozen=True, slots=True)
class RenderViewport:
    view: tuple[float, ...]
    projection: tuple[float, ...]
    width: int
    height: int


def identity(label: str, value: object) -> str:
    if not isinstance(value, str) or IDENTITY.fullmatch(value) is None:
        raise ContractError(f"{label} is not a valid identity")
    return value


def object_with_keys(
    label: str, value: object, required: set[str]
) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != required:
        raise ContractError(
            f"{label} must contain exactly {sorted(required)!r}"
        )
    return value


def positive_integer(label: str, value: object, maximum: int) -> int:
    if (
        not isinstance(value, int)
        or isinstance(value, bool)
        or value < 1
        or value > maximum
    ):
        raise ContractError(f"{label} must be between 1 and {maximum}")
    return value


def _boolean(label: str, value: object) -> bool:
    if not isinstance(value, bool):
        raise ContractError(f"{label} must be a boolean")
    return value


def _nonempty_text(label: str, value: object) -> str:
    if (
        not isinstance(value, str)
        or not value
        or len(value) > 512
        or any(character.isspace() for character in value)
    ):
        raise ContractError(f"{label} is invalid")
    return value


def _spiffe_id(value: object) -> str:
    value = _nonempty_text("spiffeId", value)
    if not value.startswith("spiffe://"):
        raise ContractError("spiffeId must use the spiffe scheme")
    return value


def _timestamp(label: str, value: object) -> str:
    value = _nonempty_text(label, value)
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise ContractError(f"{label} must be RFC 3339") from error
    if parsed.tzinfo is None:
        raise ContractError(f"{label} must include a timezone")
    return value


def nonnegative_integer(label: str, value: object, maximum: int) -> int:
    if (
        not isinstance(value, int)
        or isinstance(value, bool)
        or value < 0
        or value > maximum
    ):
        raise ContractError(f"{label} must be between 0 and {maximum}")
    return value


def finite_number(label: str, value: object) -> float:
    if (
        not isinstance(value, (int, float))
        or isinstance(value, bool)
        or not math.isfinite(value)
    ):
        raise ContractError(f"{label} must be finite")
    return float(value)


@dataclass(frozen=True, slots=True)
class SessionBinding:
    session_id: str
    epoch_id: str

    @classmethod
    def parse(cls, value: object) -> "SessionBinding":
        body = object_with_keys(
            "renderer session", value, {"sessionId", "epochId"}
        )
        return cls(
            session_id=identity("sessionId", body["sessionId"]),
            epoch_id=identity("epochId", body["epochId"]),
        )


@dataclass(frozen=True, slots=True)
class SceneBinding:
    session_id: str
    epoch_id: str
    frame_uri: str
    frame_digest: str
    maximum_pose_age_ms: int
    layer_id: str | None
    lighting: GovernedLighting
    declaration: dict[str, Any]

    @classmethod
    def parse(cls, value: object) -> "SceneBinding":
        declaration = object_with_keys(
            "scene declaration", value, {"body", "digest"}
        )
        _digest(declaration["digest"])
        body = declaration["body"]
        if not isinstance(body, dict):
            raise ContractError("scene body must be an object")
        required = {
            "schemaVersion",
            "sessionId",
            "epochId",
            "frameRevision",
            "simulationFrame",
            "geospatialLayerId",
            "environment",
            "prototypes",
            "entities",
            "allowedCameraKinds",
            "lighting",
            "quality",
            "attribution",
        }
        optional = {"geospatialLayerId"}
        if (
            not required - optional <= set(body)
            or set(body) - required
            or body.get("schemaVersion") != SCENE_SCHEMA
        ):
            raise ContractError("scene body schema is unsupported")
        frame = object_with_keys(
            "frameRevision", body["frameRevision"], {"uri", "digest"}
        )
        quality = object_with_keys(
            "quality",
            body["quality"],
            {
                "renderer",
                "maximumTextureDimension",
                "maximumAssetBytes",
                "interpolation",
                "maximumPoseAgeMs",
            },
        )
        if quality["renderer"] != "raytraced_lighting":
            raise ContractError("only RaytracedLighting is supported")
        from .lighting import GovernedLighting

        lighting = GovernedLighting.parse(body["lighting"])
        prototypes = body["prototypes"]
        entities = body["entities"]
        if (
            not isinstance(prototypes, list)
            or not prototypes
            or not isinstance(entities, list)
            or not entities
        ):
            raise ContractError("scene prototypes and entities must be non-empty")
        return cls(
            session_id=identity("sessionId", body["sessionId"]),
            epoch_id=identity("epochId", body["epochId"]),
            frame_uri=_frame_uri(frame["uri"]),
            frame_digest=_digest(frame["digest"]),
            maximum_pose_age_ms=positive_integer(
                "maximumPoseAgeMs", quality["maximumPoseAgeMs"], 60_000
            ),
            layer_id=(
                identity("geospatialLayerId", body["geospatialLayerId"])
                if "geospatialLayerId" in body
                else None
            ),
            lighting=lighting,
            declaration=declaration,
        )


@dataclass(frozen=True, slots=True)
class PoseSourceBinding:
    session_id: str
    epoch_id: str
    frame_uri: str
    frame_digest: str
    entity_table_revision: int
    entity_table_digest: str
    maximum_entities: int
    maximum_message_bytes: int
    stale_after_ms: int
    producer_id: str
    producer_spiffe_id: str
    authorization_revision: int
    expires_at: str
    revoked: bool

    @classmethod
    def parse(cls, value: object) -> "PoseSourceBinding":
        body = object_with_keys(
            "pose-source binding",
            value,
            {
                "schemaVersion",
                "sessionId",
                "epochId",
                "frameRevision",
                "entityTableRevision",
                "entityTableDigest",
                "limits",
                "producer",
            },
        )
        if body["schemaVersion"] != POSE_CONTROL_SCHEMA:
            raise ContractError("pose control schema is unsupported")
        frame = object_with_keys(
            "frameRevision", body["frameRevision"], {"uri", "digest"}
        )
        limits = object_with_keys(
            "pose limits",
            body["limits"],
            {
                "maximumEntities",
                "maximumMessageBytes",
                "maximumCadenceHz",
                "staleAfterMs",
            },
        )
        producer = object_with_keys(
            "producer",
            body["producer"],
            {
                "producerId",
                "spiffeId",
                "authorizationRevision",
                "expiresAt",
                "revoked",
            },
        )
        return cls(
            session_id=identity("sessionId", body["sessionId"]),
            epoch_id=identity("epochId", body["epochId"]),
            frame_uri=_frame_uri(frame["uri"]),
            frame_digest=_digest(frame["digest"]),
            entity_table_revision=positive_integer(
                "entityTableRevision",
                body["entityTableRevision"],
                (1 << 63) - 1,
            ),
            entity_table_digest=_digest(body["entityTableDigest"]),
            maximum_entities=positive_integer(
                "maximumEntities", limits["maximumEntities"], 1_000_000
            ),
            maximum_message_bytes=positive_integer(
                "maximumMessageBytes",
                limits["maximumMessageBytes"],
                64 * 1024 * 1024,
            ),
            stale_after_ms=positive_integer(
                "staleAfterMs", limits["staleAfterMs"], 60_000
            ),
            producer_id=identity("producerId", producer["producerId"]),
            producer_spiffe_id=_spiffe_id(producer["spiffeId"]),
            authorization_revision=positive_integer(
                "authorizationRevision",
                producer["authorizationRevision"],
                (1 << 63) - 1,
            ),
            expires_at=_timestamp("expiresAt", producer["expiresAt"]),
            revoked=_boolean("revoked", producer["revoked"]),
        )


@dataclass(frozen=True, slots=True)
class CameraBinding:
    session_id: str
    camera_id: str
    revision: int
    render_slot: int
    definition: dict[str, Any]

    @classmethod
    def parse(cls, value: object, maximum_slots: int) -> "CameraBinding":
        body = object_with_keys(
            "camera binding",
            value,
            {
                "sessionId",
                "cameraId",
                "revision",
                "renderSlot",
                "definition",
            },
        )
        definition = _camera_definition(body["definition"])
        slot = nonnegative_integer(
            "renderSlot", body["renderSlot"], maximum_slots - 1
        )
        return cls(
            session_id=identity("sessionId", body["sessionId"]),
            camera_id=identity("cameraId", body["cameraId"]),
            revision=positive_integer(
                "revision", body["revision"], (1 << 63) - 1
            ),
            render_slot=slot,
            definition=definition,
        )


@dataclass(frozen=True, slots=True)
class StreamBinding:
    session_id: str
    camera_id: str
    live_view_id: str
    render_slot: int
    media_port: int

    @classmethod
    def parse(cls, value: object, maximum_slots: int) -> "StreamBinding":
        body = object_with_keys(
            "stream binding",
            value,
            {
                "sessionId",
                "cameraId",
                "liveViewId",
                "renderSlot",
                "mediaPort",
            },
        )
        slot = nonnegative_integer(
            "renderSlot", body["renderSlot"], maximum_slots - 1
        )
        return cls(
            session_id=identity("sessionId", body["sessionId"]),
            camera_id=identity("cameraId", body["cameraId"]),
            live_view_id=identity("liveViewId", body["liveViewId"]),
            render_slot=slot,
            media_port=positive_integer(
                "mediaPort", body["mediaPort"], 65535
            ),
        )


def _digest(value: object) -> str:
    if (
        not isinstance(value, str)
        or not value.startswith("sha256:")
        or len(value) != 71
        or any(character not in "0123456789abcdef" for character in value[7:])
    ):
        raise ContractError("SHA-256 digest is invalid")
    return value


def _frame_uri(value: object) -> str:
    if (
        not isinstance(value, str)
        or not value.startswith("frames://world/")
        or len(value) > 512
    ):
        raise ContractError("Frames revision URI is invalid")
    return value


def _vector(label: str, value: object) -> tuple[float, float, float]:
    body = object_with_keys(label, value, {"x", "y", "z"})
    return tuple(
        finite_number(f"{label}.{axis}", body[axis])
        for axis in ("x", "y", "z")
    )


def _camera_definition(value: object) -> dict[str, Any]:
    body = object_with_keys(
        "camera definition",
        value,
        {
            "rig",
            "widthPx",
            "heightPx",
            "frameRateMillihertz",
            "verticalFovDegrees",
            "nearClipM",
            "farClipM",
            "streamPolicy",
            "recordingPolicy",
        },
    )
    positive_integer("widthPx", body["widthPx"], 16_384)
    positive_integer("heightPx", body["heightPx"], 16_384)
    positive_integer(
        "frameRateMillihertz", body["frameRateMillihertz"], 240_000
    )
    fov = finite_number("verticalFovDegrees", body["verticalFovDegrees"])
    near = finite_number("nearClipM", body["nearClipM"])
    far = finite_number("farClipM", body["farClipM"])
    if not 1.0 <= fov <= 160.0 or near <= 0.0 or far <= near:
        raise ContractError("camera optics are invalid")
    if body["streamPolicy"] not in {"disabled", "on_demand", "continuous"}:
        raise ContractError("camera stream policy is invalid")
    if body["recordingPolicy"] not in {
        "disabled",
        "on_capture",
        "continuous",
    }:
        raise ContractError("camera recording policy is invalid")
    _camera_rig(body["rig"])
    return body


def _camera_rig(value: object) -> None:
    if not isinstance(value, dict):
        raise ContractError("camera rig must be an object")
    kind = value.get("kind")
    keys = {
        "fixed": {"kind", "pose"},
        "look_at": {"kind", "eyeM", "targetM"},
        "orbit": {
            "kind",
            "targetEntity",
            "radiusM",
            "azimuthDegrees",
            "elevationDegrees",
        },
        "follow_entity": {
            "kind",
            "targetEntity",
            "offsetFluM",
            "smoothingSeconds",
        },
        "chase_entity": {
            "kind",
            "targetEntity",
            "distanceM",
            "heightM",
            "smoothingSeconds",
        },
        "mounted_entity": {"kind", "targetEntity", "mount"},
        "formation_overview": {"kind", "targetEntities", "paddingM"},
    }
    if kind not in keys or set(value) != keys[kind]:
        raise ContractError("camera rig kind or fields are invalid")
    for field in ("targetEntity",):
        if field in value:
            identity(field, value[field])
    if kind == "look_at":
        if _vector("eyeM", value["eyeM"]) == _vector(
            "targetM", value["targetM"]
        ):
            raise ContractError("look-at eye and target must differ")
    elif kind == "follow_entity":
        _vector("offsetFluM", value["offsetFluM"])
        if finite_number(
            "smoothingSeconds", value["smoothingSeconds"]
        ) < 0.0:
            raise ContractError("smoothingSeconds cannot be negative")
    elif kind == "fixed":
        _local_pose("pose", value["pose"])
    elif kind == "orbit":
        radius = finite_number("radiusM", value["radiusM"])
        elevation = finite_number(
            "elevationDegrees", value["elevationDegrees"]
        )
        finite_number("azimuthDegrees", value["azimuthDegrees"])
        if radius <= 0.1 or not -89.9 <= elevation <= 89.9:
            raise ContractError("orbit rig is invalid")
    elif kind == "chase_entity":
        if (
            finite_number("distanceM", value["distanceM"]) <= 0.1
            or finite_number(
                "smoothingSeconds", value["smoothingSeconds"]
            )
            < 0.0
        ):
            raise ContractError("chase rig is invalid")
        finite_number("heightM", value["heightM"])
    elif kind == "mounted_entity":
        _transform("mount", value["mount"])
    elif kind == "formation_overview":
        targets = value["targetEntities"]
        if (
            not isinstance(targets, list)
            or not targets
            or len(targets) > 256
        ):
            raise ContractError(
                "formation targets must be sorted and unique"
            )
        for target in targets:
            identity("targetEntity", target)
        if targets != sorted(set(targets)):
            raise ContractError(
                "formation targets must be sorted and unique"
            )
        if finite_number("paddingM", value["paddingM"]) < 0.0:
            raise ContractError("formation padding cannot be negative")


def _quaternion(label: str, value: object) -> tuple[float, float, float, float]:
    body = object_with_keys(label, value, {"x", "y", "z", "w"})
    quaternion = tuple(
        finite_number(f"{label}.{axis}", body[axis])
        for axis in ("x", "y", "z", "w")
    )
    if abs(sum(component * component for component in quaternion) - 1.0) > 1e-3:
        raise ContractError(f"{label} must be normalized")
    return quaternion


def _local_pose(label: str, value: object) -> None:
    body = object_with_keys(
        label, value, {"positionM", "orientationXyzw"}
    )
    _vector(f"{label}.positionM", body["positionM"])
    _quaternion(f"{label}.orientationXyzw", body["orientationXyzw"])


def _transform(label: str, value: object) -> None:
    body = object_with_keys(
        label, value, {"translationM", "orientationXyzw", "scale"}
    )
    _vector(f"{label}.translationM", body["translationM"])
    _quaternion(f"{label}.orientationXyzw", body["orientationXyzw"])
    scale = _vector(f"{label}.scale", body["scale"])
    if any(component <= 0.0 for component in scale):
        raise ContractError(f"{label}.scale must be positive")
