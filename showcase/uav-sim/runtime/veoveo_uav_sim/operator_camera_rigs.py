from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Mapping, TypeAlias

from .operator_camera import (
    CameraRigKind,
    CameraSmoothingProfile,
    EntityTransform,
    Pose,
    QuaternionXyzw,
    Vector3,
    compose_pose,
)


@dataclass(frozen=True, slots=True)
class FixedRig:
    pose: Pose


@dataclass(frozen=True, slots=True)
class LookAtRig:
    eye_m: Vector3
    target_m: Vector3
    smoothing: CameraSmoothingProfile


@dataclass(frozen=True, slots=True)
class OrbitRig:
    target_entity_id: str
    radius_m: float
    azimuth_degrees: float
    elevation_degrees: float
    smoothing: CameraSmoothingProfile


@dataclass(frozen=True, slots=True)
class FollowEntityRig:
    target_entity_id: str
    eye_offset_flu_m: Vector3
    target_offset_flu_m: Vector3
    smoothing: CameraSmoothingProfile


@dataclass(frozen=True, slots=True)
class ChaseEntityRig:
    target_entity_id: str
    distance_m: float
    height_m: float
    smoothing: CameraSmoothingProfile


@dataclass(frozen=True, slots=True)
class StabilizedMountedEntityRig:
    target_entity_id: str
    mount: Pose
    smoothing: CameraSmoothingProfile


@dataclass(frozen=True, slots=True)
class FormationOverviewRig:
    target_entity_ids: tuple[str, ...]
    padding_m: float
    smoothing: CameraSmoothingProfile


CameraRig: TypeAlias = (
    FixedRig
    | LookAtRig
    | OrbitRig
    | FollowEntityRig
    | ChaseEntityRig
    | StabilizedMountedEntityRig
    | FormationOverviewRig
)


def rig_kind(rig: CameraRig) -> CameraRigKind:
    if isinstance(rig, FixedRig):
        return CameraRigKind.FIXED
    if isinstance(rig, LookAtRig):
        return CameraRigKind.LOOK_AT
    if isinstance(rig, OrbitRig):
        return CameraRigKind.ORBIT
    if isinstance(rig, FollowEntityRig):
        return CameraRigKind.FOLLOW_ENTITY
    if isinstance(rig, ChaseEntityRig):
        return CameraRigKind.CHASE_ENTITY
    if isinstance(rig, StabilizedMountedEntityRig):
        return CameraRigKind.STABILIZED_MOUNTED_ENTITY
    if isinstance(rig, FormationOverviewRig):
        return CameraRigKind.FORMATION_OVERVIEW
    raise TypeError(f"unsupported operator-camera rig {type(rig)!r}")


def smoothing_profile(rig: CameraRig) -> CameraSmoothingProfile | None:
    return None if isinstance(rig, FixedRig) else rig.smoothing


def target_identity(rig: CameraRig) -> str:
    if isinstance(rig, (FixedRig, LookAtRig)):
        return rig_kind(rig).value
    if isinstance(rig, FormationOverviewRig):
        return ":".join(rig.target_entity_ids)
    return rig.target_entity_id


def desired_camera_pose(
    rig: CameraRig,
    entities: Mapping[str, EntityTransform],
) -> Pose:
    if isinstance(rig, FixedRig):
        return rig.pose.normalized()
    if isinstance(rig, LookAtRig):
        return Pose(rig.eye_m, look_at_orientation(rig.eye_m, rig.target_m))
    if isinstance(rig, OrbitRig):
        target = _entity(entities, rig.target_entity_id).pose.position_m
        azimuth = math.radians(rig.azimuth_degrees)
        elevation = math.radians(rig.elevation_degrees)
        horizontal = rig.radius_m * math.cos(elevation)
        eye = target + Vector3(
            horizontal * math.cos(azimuth),
            horizontal * math.sin(azimuth),
            rig.radius_m * math.sin(elevation),
        )
        return Pose(eye, look_at_orientation(eye, target))
    if isinstance(rig, FollowEntityRig):
        target = _entity(entities, rig.target_entity_id).pose.normalized()
        eye = target.position_m + target.orientation_xyzw.rotate(rig.eye_offset_flu_m)
        aim = target.position_m + target.orientation_xyzw.rotate(
            rig.target_offset_flu_m
        )
        return Pose(eye, look_at_orientation(eye, aim))
    if isinstance(rig, ChaseEntityRig):
        target = _entity(entities, rig.target_entity_id).pose.normalized()
        eye = target.position_m + target.orientation_xyzw.rotate(
            Vector3(-rig.distance_m, 0.0, rig.height_m)
        )
        return Pose(eye, look_at_orientation(eye, target.position_m))
    if isinstance(rig, StabilizedMountedEntityRig):
        return compose_pose(
            _entity(entities, rig.target_entity_id).pose.normalized(), rig.mount
        )
    if isinstance(rig, FormationOverviewRig):
        if not rig.target_entity_ids:
            raise ValueError("formation operator camera requires at least one target")
        positions = [
            _entity(entities, entity_id).pose.position_m
            for entity_id in rig.target_entity_ids
        ]
        count = float(len(positions))
        centroid = Vector3(
            sum(position.x for position in positions) / count,
            sum(position.y for position in positions) / count,
            sum(position.z for position in positions) / count,
        )
        radius = max((position.distance(centroid) for position in positions), default=0.0)
        distance = max(5.0, radius + rig.padding_m)
        eye = centroid + Vector3(-distance, -distance, max(5.0, distance * 0.75))
        return Pose(eye, look_at_orientation(eye, centroid))
    raise TypeError(f"unsupported operator-camera rig {type(rig)!r}")


def look_at_orientation(
    eye_m: Vector3,
    target_m: Vector3,
    world_up: Vector3 = Vector3(0.0, 0.0, 1.0),
) -> QuaternionXyzw:
    forward = (target_m - eye_m).normalized()
    try:
        right = forward.cross(world_up).normalized()
    except ValueError:
        fallback_up = Vector3(0.0, 1.0, 0.0)
        right = forward.cross(fallback_up).normalized()
    up = right.cross(forward).normalized()
    backward = forward * -1.0
    return _quaternion_from_rotation_columns(right, up, backward)


def _quaternion_from_rotation_columns(
    x_axis: Vector3,
    y_axis: Vector3,
    z_axis: Vector3,
) -> QuaternionXyzw:
    m00, m01, m02 = x_axis.x, y_axis.x, z_axis.x
    m10, m11, m12 = x_axis.y, y_axis.y, z_axis.y
    m20, m21, m22 = x_axis.z, y_axis.z, z_axis.z
    trace = m00 + m11 + m22
    if trace > 0.0:
        scale = math.sqrt(trace + 1.0) * 2.0
        quaternion = QuaternionXyzw(
            (m21 - m12) / scale,
            (m02 - m20) / scale,
            (m10 - m01) / scale,
            0.25 * scale,
        )
    elif m00 > m11 and m00 > m22:
        scale = math.sqrt(1.0 + m00 - m11 - m22) * 2.0
        quaternion = QuaternionXyzw(
            0.25 * scale,
            (m01 + m10) / scale,
            (m02 + m20) / scale,
            (m21 - m12) / scale,
        )
    elif m11 > m22:
        scale = math.sqrt(1.0 + m11 - m00 - m22) * 2.0
        quaternion = QuaternionXyzw(
            (m01 + m10) / scale,
            0.25 * scale,
            (m12 + m21) / scale,
            (m02 - m20) / scale,
        )
    else:
        scale = math.sqrt(1.0 + m22 - m00 - m11) * 2.0
        quaternion = QuaternionXyzw(
            (m02 + m20) / scale,
            (m12 + m21) / scale,
            0.25 * scale,
            (m10 - m01) / scale,
        )
    return quaternion.normalized()


def _entity(
    entities: Mapping[str, EntityTransform], entity_id: str
) -> EntityTransform:
    try:
        return entities[entity_id]
    except KeyError as error:
        raise ValueError(f"operator-camera target {entity_id!r} is unavailable") from error
