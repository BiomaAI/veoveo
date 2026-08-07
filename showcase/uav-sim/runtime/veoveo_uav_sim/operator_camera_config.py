from __future__ import annotations

import json
from collections.abc import Mapping
from dataclasses import dataclass
from typing import Any

from .operator_camera import (
    CameraOptics,
    CameraRigKind,
    CameraStreamPolicy,
    CameraSmoothingProfile,
    OperatorCameraDefinition,
    Pose,
    QuaternionXyzw,
    Vector3,
)
from .operator_camera_rigs import (
    ChaseEntityRig,
    FixedRig,
    FollowEntityRig,
    FormationOverviewRig,
    LookAtRig,
    OrbitRig,
    StabilizedMountedEntityRig,
)


@dataclass(frozen=True, slots=True)
class OperatorLiveViewRuntimeConfig:
    cameras: tuple[OperatorCameraDefinition, ...]
    viewer_slot_count: int
    signaling_port_base: int
    media_port_base: int
    public_media_ip: str

    def __post_init__(self) -> None:
        if not self.cameras or len(self.cameras) > 32:
            raise ValueError("operator live view requires 1-32 configured cameras")
        if all(
            camera.stream_policy is CameraStreamPolicy.DISABLED
            for camera in self.cameras
        ):
            raise ValueError("operator live view requires at least one streamable camera")
        camera_ids = [camera.camera_id for camera in self.cameras]
        if len(camera_ids) != len(set(camera_ids)):
            raise ValueError("operator-camera identities must be unique")
        if not 1 <= self.viewer_slot_count <= 32:
            raise ValueError("operator live view requires 1-32 viewer slots")
        maximum_slot = self.viewer_slot_count - 1
        if not 1 <= self.signaling_port_base <= 65_535 - maximum_slot:
            raise ValueError("viewer-slot signaling port range exceeds 65535")
        if not 1 <= self.media_port_base <= 65_535 - maximum_slot:
            raise ValueError("viewer-slot media port range exceeds 65535")
        if (
            not self.public_media_ip
            or "/" in self.public_media_ip
            or any(character.isspace() for character in self.public_media_ip)
        ):
            raise ValueError("operator-camera public media IP is invalid")
        streamable = tuple(
            camera
            for camera in self.cameras
            if camera.stream_policy is not CameraStreamPolicy.DISABLED
        )
        if any(camera.optics != streamable[0].optics for camera in streamable[1:]):
            raise ValueError(
                "all streamable logical cameras must use the viewer-slot optics profile"
            )

    @property
    def viewer_optics(self) -> CameraOptics:
        return next(
            camera.optics
            for camera in self.cameras
            if camera.stream_policy is not CameraStreamPolicy.DISABLED
        )

    @classmethod
    def from_json(
        cls,
        raw: str,
        *,
        viewer_slot_count: int,
        signaling_port_base: int,
        media_port_base: int,
        public_media_ip: str,
    ) -> "OperatorLiveViewRuntimeConfig":
        try:
            value = json.loads(raw)
        except json.JSONDecodeError as error:
            raise ValueError("UAV_SIM_OPERATOR_CAMERAS_JSON is invalid JSON") from error
        if not isinstance(value, list):
            raise ValueError("UAV_SIM_OPERATOR_CAMERAS_JSON must be a camera array")
        return cls(
            cameras=tuple(_camera(item) for item in value),
            viewer_slot_count=viewer_slot_count,
            signaling_port_base=signaling_port_base,
            media_port_base=media_port_base,
            public_media_ip=public_media_ip,
        )


def _camera(value: Any) -> OperatorCameraDefinition:
    body = _object(value, "operator camera")
    _exact(
        body,
        {
            "cameraId",
            "revision",
            "rig",
            "optics",
            "streamPolicy",
        },
        "operator camera",
    )
    rig = _rig(body["rig"])
    optics_body = _object(body["optics"], "operator camera optics")
    _exact(
        optics_body,
        {
            "widthPx",
            "heightPx",
            "frameRateHz",
            "verticalFovDegrees",
            "nearClipM",
            "farClipM",
        },
        "operator camera optics",
    )
    try:
        stream_policy = CameraStreamPolicy(body["streamPolicy"])
    except (TypeError, ValueError) as error:
        raise ValueError(
            "operator camera streamPolicy must be disabled, on_demand, or continuous"
        ) from error
    return OperatorCameraDefinition(
        camera_id=_identity(body["cameraId"], "operator camera cameraId"),
        revision=_integer(body["revision"], "operator camera revision"),
        rig_kind=_rig_kind(rig),
        rig=rig,
        optics=CameraOptics(
            width_px=_integer(optics_body["widthPx"], "operator camera widthPx"),
            height_px=_integer(optics_body["heightPx"], "operator camera heightPx"),
            frame_rate_hz=_integer(
                optics_body["frameRateHz"], "operator camera frameRateHz"
            ),
            vertical_fov_degrees=_number(
                optics_body["verticalFovDegrees"],
                "operator camera verticalFovDegrees",
            ),
            near_clip_m=_number(
                optics_body["nearClipM"], "operator camera nearClipM"
            ),
            far_clip_m=_number(
                optics_body["farClipM"], "operator camera farClipM"
            ),
        ),
        stream_policy=stream_policy,
    )


def _rig(value: Any) -> object:
    body = _object(value, "operator camera rig")
    kind_raw = body.get("kind")
    try:
        kind = CameraRigKind(kind_raw)
    except (TypeError, ValueError) as error:
        raise ValueError(f"unsupported operator camera rig kind {kind_raw!r}") from error
    if kind is CameraRigKind.FIXED:
        _exact(body, {"kind", "pose"}, "fixed operator camera rig")
        return FixedRig(_pose(body["pose"], "fixed operator camera pose"))
    if kind is CameraRigKind.LOOK_AT:
        _exact(
            body,
            {"kind", "eyeM", "targetM", "smoothing"},
            "look-at operator camera rig",
        )
        return LookAtRig(
            _vector(body["eyeM"], "look-at eyeM"),
            _vector(body["targetM"], "look-at targetM"),
            _smoothing(body["smoothing"]),
        )
    if kind is CameraRigKind.ORBIT:
        _exact(
            body,
            {
                "kind",
                "targetEntityId",
                "radiusM",
                "azimuthDegrees",
                "elevationDegrees",
                "smoothing",
            },
            "orbit operator camera rig",
        )
        radius = _number(body["radiusM"], "orbit radiusM")
        elevation = _number(body["elevationDegrees"], "orbit elevationDegrees")
        if radius <= 0.1 or not -89.9 <= elevation <= 89.9:
            raise ValueError("orbit radius and elevation are outside the admitted range")
        return OrbitRig(
            _identity(body["targetEntityId"], "orbit targetEntityId"),
            radius,
            _number(body["azimuthDegrees"], "orbit azimuthDegrees"),
            elevation,
            _smoothing(body["smoothing"]),
        )
    if kind is CameraRigKind.FOLLOW_ENTITY:
        _exact(
            body,
            {
                "kind",
                "targetEntityId",
                "eyeOffsetFluM",
                "targetOffsetFluM",
                "smoothing",
            },
            "follow operator camera rig",
        )
        return FollowEntityRig(
            _identity(body["targetEntityId"], "follow targetEntityId"),
            _vector(body["eyeOffsetFluM"], "follow eyeOffsetFluM"),
            _vector(body["targetOffsetFluM"], "follow targetOffsetFluM"),
            _smoothing(body["smoothing"]),
        )
    if kind is CameraRigKind.CHASE_ENTITY:
        _exact(
            body,
            {"kind", "targetEntityId", "distanceM", "heightM", "smoothing"},
            "chase operator camera rig",
        )
        distance = _number(body["distanceM"], "chase distanceM")
        if distance <= 0.1:
            raise ValueError("chase distanceM must exceed 0.1")
        return ChaseEntityRig(
            _identity(body["targetEntityId"], "chase targetEntityId"),
            distance,
            _number(body["heightM"], "chase heightM"),
            _smoothing(body["smoothing"]),
        )
    if kind is CameraRigKind.STABILIZED_MOUNTED_ENTITY:
        _exact(
            body,
            {"kind", "targetEntityId", "mount", "smoothing"},
            "stabilized mounted operator camera rig",
        )
        return StabilizedMountedEntityRig(
            _identity(
                body["targetEntityId"], "stabilized mounted targetEntityId"
            ),
            _pose(body["mount"], "stabilized mounted pose"),
            _smoothing(body["smoothing"]),
        )
    if kind is CameraRigKind.FORMATION_OVERVIEW:
        _exact(
            body,
            {"kind", "targetEntityIds", "paddingM", "smoothing"},
            "formation operator camera rig",
        )
        targets = body["targetEntityIds"]
        if not isinstance(targets, list) or not 1 <= len(targets) <= 256:
            raise ValueError("formation targetEntityIds must contain 1-256 identities")
        target_ids = tuple(
            _identity(item, "formation targetEntityId") for item in targets
        )
        if tuple(sorted(set(target_ids))) != target_ids:
            raise ValueError("formation targetEntityIds must be sorted and unique")
        padding = _number(body["paddingM"], "formation paddingM")
        if padding < 0.0:
            raise ValueError("formation paddingM must be non-negative")
        return FormationOverviewRig(
            target_ids,
            padding,
            _smoothing(body["smoothing"]),
        )
    raise AssertionError("exhaustive camera rig parsing")


def _rig_kind(rig: object) -> CameraRigKind:
    mapping = {
        FixedRig: CameraRigKind.FIXED,
        LookAtRig: CameraRigKind.LOOK_AT,
        OrbitRig: CameraRigKind.ORBIT,
        FollowEntityRig: CameraRigKind.FOLLOW_ENTITY,
        ChaseEntityRig: CameraRigKind.CHASE_ENTITY,
        StabilizedMountedEntityRig: CameraRigKind.STABILIZED_MOUNTED_ENTITY,
        FormationOverviewRig: CameraRigKind.FORMATION_OVERVIEW,
    }
    try:
        return mapping[type(rig)]
    except KeyError as error:
        raise TypeError(f"unsupported operator camera rig {type(rig)!r}") from error


def _smoothing(value: Any) -> CameraSmoothingProfile:
    body = _object(value, "operator camera smoothing")
    _exact(
        body,
        {
            "translationHalfLifeMs",
            "rotationHalfLifeMs",
            "teleportDistanceM",
            "resetAfterGapMs",
        },
        "operator camera smoothing",
    )
    return CameraSmoothingProfile(
        _integer(
            body["translationHalfLifeMs"], "smoothing translationHalfLifeMs"
        ),
        _integer(body["rotationHalfLifeMs"], "smoothing rotationHalfLifeMs"),
        _number(body["teleportDistanceM"], "smoothing teleportDistanceM"),
        _integer(body["resetAfterGapMs"], "smoothing resetAfterGapMs"),
    )


def _pose(value: Any, context: str) -> Pose:
    body = _object(value, context)
    _exact(body, {"positionM", "orientationXyzw"}, context)
    quaternion_body = _object(body["orientationXyzw"], f"{context} orientation")
    _exact(quaternion_body, {"x", "y", "z", "w"}, f"{context} orientation")
    quaternion = QuaternionXyzw(
        _number(quaternion_body["x"], f"{context} orientation x"),
        _number(quaternion_body["y"], f"{context} orientation y"),
        _number(quaternion_body["z"], f"{context} orientation z"),
        _number(quaternion_body["w"], f"{context} orientation w"),
    ).normalized()
    return Pose(_vector(body["positionM"], f"{context} position"), quaternion)


def _vector(value: Any, context: str) -> Vector3:
    body = _object(value, context)
    _exact(body, {"x", "y", "z"}, context)
    return Vector3(
        _number(body["x"], f"{context} x"),
        _number(body["y"], f"{context} y"),
        _number(body["z"], f"{context} z"),
    )


def _object(value: Any, context: str) -> Mapping[str, Any]:
    if not isinstance(value, dict) or not all(isinstance(key, str) for key in value):
        raise ValueError(f"{context} must be an object")
    return value


def _exact(value: Mapping[str, Any], fields: set[str], context: str) -> None:
    actual = set(value)
    if actual != fields:
        raise ValueError(
            f"{context} fields invalid; missing={sorted(fields - actual)}, "
            f"unknown={sorted(actual - fields)}"
        )


def _identity(value: Any, context: str) -> str:
    if (
        not isinstance(value, str)
        or not 1 <= len(value) <= 128
        or not all(
            character.isascii()
            and (character.isalnum() or character in {"_", "-", "."})
            for character in value
        )
    ):
        raise ValueError(f"{context} must be a canonical identity")
    return value


def _integer(value: Any, context: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise ValueError(f"{context} must be an integer")
    return value


def _number(value: Any, context: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"{context} must be a number")
    result = float(value)
    if result != result or result in {float("inf"), float("-inf")}:
        raise ValueError(f"{context} must be finite")
    return result


def live_camera_descriptor(
    session_id: str,
    camera: OperatorCameraDefinition,
) -> dict[str, object]:
    return {
        "cameraId": camera.camera_id,
        "sessionId": session_id,
        "revision": camera.revision,
        "rig": _rig_json(camera.rig),
        "widthPx": camera.optics.width_px,
        "heightPx": camera.optics.height_px,
        "frameRateMillihertz": camera.optics.frame_rate_hz * 1_000,
        "verticalFovDegrees": camera.optics.vertical_fov_degrees,
        "nearClipM": camera.optics.near_clip_m,
        "farClipM": camera.optics.far_clip_m,
        "streamPolicy": camera.stream_policy.value,
        "health": "warming",
    }


def _rig_json(rig: object) -> dict[str, object]:
    if isinstance(rig, FixedRig):
        return {"kind": "fixed", "pose": _pose_json(rig.pose)}
    if isinstance(rig, LookAtRig):
        return {
            "kind": "look_at",
            "eyeM": _vector_json(rig.eye_m),
            "targetM": _vector_json(rig.target_m),
            "smoothing": _smoothing_json(rig.smoothing),
        }
    if isinstance(rig, OrbitRig):
        return {
            "kind": "orbit",
            "targetEntityId": rig.target_entity_id,
            "radiusM": rig.radius_m,
            "azimuthDegrees": rig.azimuth_degrees,
            "elevationDegrees": rig.elevation_degrees,
            "smoothing": _smoothing_json(rig.smoothing),
        }
    if isinstance(rig, FollowEntityRig):
        return {
            "kind": "follow_entity",
            "targetEntityId": rig.target_entity_id,
            "eyeOffsetFluM": _vector_json(rig.eye_offset_flu_m),
            "targetOffsetFluM": _vector_json(rig.target_offset_flu_m),
            "smoothing": _smoothing_json(rig.smoothing),
        }
    if isinstance(rig, ChaseEntityRig):
        return {
            "kind": "chase_entity",
            "targetEntityId": rig.target_entity_id,
            "distanceM": rig.distance_m,
            "heightM": rig.height_m,
            "smoothing": _smoothing_json(rig.smoothing),
        }
    if isinstance(rig, StabilizedMountedEntityRig):
        return {
            "kind": "stabilized_mounted_entity",
            "targetEntityId": rig.target_entity_id,
            "mount": _pose_json(rig.mount),
            "smoothing": _smoothing_json(rig.smoothing),
        }
    if isinstance(rig, FormationOverviewRig):
        return {
            "kind": "formation_overview",
            "targetEntityIds": list(rig.target_entity_ids),
            "paddingM": rig.padding_m,
            "smoothing": _smoothing_json(rig.smoothing),
        }
    raise TypeError(f"unsupported operator-camera rig {type(rig)!r}")


def _vector_json(vector: Vector3) -> dict[str, float]:
    return {"x": vector.x, "y": vector.y, "z": vector.z}


def _pose_json(pose: Pose) -> dict[str, object]:
    orientation = pose.orientation_xyzw.normalized()
    return {
        "positionM": _vector_json(pose.position_m),
        "orientationXyzw": {
            "x": orientation.x,
            "y": orientation.y,
            "z": orientation.z,
            "w": orientation.w,
        },
    }


def _smoothing_json(profile: CameraSmoothingProfile) -> dict[str, int]:
    return {
        "translationHalfLifeMs": profile.translation_half_life_ms,
        "rotationHalfLifeMs": profile.rotation_half_life_ms,
        "teleportDistanceMillimetres": int(round(profile.teleport_distance_m * 1_000.0)),
        "resetAfterGapMs": profile.reset_after_gap_ms,
    }
