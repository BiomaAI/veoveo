from __future__ import annotations

import math
import time
from dataclasses import dataclass
from enum import StrEnum
from typing import Any, Mapping


@dataclass(frozen=True, slots=True)
class Vector3:
    x: float
    y: float
    z: float

    def __post_init__(self) -> None:
        if not all(math.isfinite(value) for value in (self.x, self.y, self.z)):
            raise ValueError("camera vector components must be finite")

    def __add__(self, other: "Vector3") -> "Vector3":
        return Vector3(self.x + other.x, self.y + other.y, self.z + other.z)

    def __sub__(self, other: "Vector3") -> "Vector3":
        return Vector3(self.x - other.x, self.y - other.y, self.z - other.z)

    def __mul__(self, scalar: float) -> "Vector3":
        return Vector3(self.x * scalar, self.y * scalar, self.z * scalar)

    def __truediv__(self, scalar: float) -> "Vector3":
        if scalar == 0.0:
            raise ValueError("camera vector divisor must not be zero")
        return self * (1.0 / scalar)

    def dot(self, other: "Vector3") -> float:
        return self.x * other.x + self.y * other.y + self.z * other.z

    def cross(self, other: "Vector3") -> "Vector3":
        return Vector3(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )

    def norm(self) -> float:
        return math.sqrt(self.dot(self))

    def normalized(self) -> "Vector3":
        norm = self.norm()
        if norm <= 1.0e-12:
            raise ValueError("camera vector must have non-zero length")
        return self / norm

    def distance(self, other: "Vector3") -> float:
        return (self - other).norm()

    def lerp(self, other: "Vector3", alpha: float) -> "Vector3":
        return self + (other - self) * alpha

    def as_tuple(self) -> tuple[float, float, float]:
        return (self.x, self.y, self.z)


@dataclass(frozen=True, slots=True)
class QuaternionXyzw:
    x: float
    y: float
    z: float
    w: float

    def __post_init__(self) -> None:
        if not all(
            math.isfinite(value) for value in (self.x, self.y, self.z, self.w)
        ):
            raise ValueError("camera quaternion components must be finite")

    @classmethod
    def identity(cls) -> "QuaternionXyzw":
        return cls(0.0, 0.0, 0.0, 1.0)

    def dot(self, other: "QuaternionXyzw") -> float:
        return (
            self.x * other.x
            + self.y * other.y
            + self.z * other.z
            + self.w * other.w
        )

    def norm(self) -> float:
        return math.sqrt(self.dot(self))

    def normalized(self) -> "QuaternionXyzw":
        norm = self.norm()
        if norm <= 1.0e-12:
            raise ValueError("camera quaternion must have non-zero length")
        return QuaternionXyzw(
            self.x / norm,
            self.y / norm,
            self.z / norm,
            self.w / norm,
        )

    def conjugate(self) -> "QuaternionXyzw":
        return QuaternionXyzw(-self.x, -self.y, -self.z, self.w)

    def __mul__(self, other: "QuaternionXyzw") -> "QuaternionXyzw":
        return QuaternionXyzw(
            self.w * other.x
            + self.x * other.w
            + self.y * other.z
            - self.z * other.y,
            self.w * other.y
            - self.x * other.z
            + self.y * other.w
            + self.z * other.x,
            self.w * other.z
            + self.x * other.y
            - self.y * other.x
            + self.z * other.w,
            self.w * other.w
            - self.x * other.x
            - self.y * other.y
            - self.z * other.z,
        )

    def rotate(self, vector: Vector3) -> Vector3:
        unit = self.normalized()
        value = unit * QuaternionXyzw(vector.x, vector.y, vector.z, 0.0) * unit.conjugate()
        return Vector3(value.x, value.y, value.z)

    def negated(self) -> "QuaternionXyzw":
        return QuaternionXyzw(-self.x, -self.y, -self.z, -self.w)

    def as_tuple(self) -> tuple[float, float, float, float]:
        return (self.x, self.y, self.z, self.w)


@dataclass(frozen=True, slots=True)
class Pose:
    position_m: Vector3
    orientation_xyzw: QuaternionXyzw

    def normalized(self) -> "Pose":
        return Pose(self.position_m, self.orientation_xyzw.normalized())


@dataclass(frozen=True, slots=True)
class EntityTransform:
    entity_id: str
    pose: Pose

    def __post_init__(self) -> None:
        if not self.entity_id or len(self.entity_id) > 128:
            raise ValueError("camera target entity identity is invalid")


@dataclass(frozen=True, slots=True)
class CameraSmoothingProfile:
    translation_half_life_ms: int
    rotation_half_life_ms: int
    teleport_distance_m: float
    reset_after_gap_ms: int

    def __post_init__(self) -> None:
        if not 0 <= self.translation_half_life_ms <= 60_000:
            raise ValueError("translation camera half-life must be 0-60000 ms")
        if not 0 <= self.rotation_half_life_ms <= 60_000:
            raise ValueError("rotation camera half-life must be 0-60000 ms")
        if not math.isfinite(self.teleport_distance_m) or not (
            0.001 <= self.teleport_distance_m <= 100_000.0
        ):
            raise ValueError("camera teleport distance must be 0.001-100000 metres")
        if not 1 <= self.reset_after_gap_ms <= 600_000:
            raise ValueError("camera reset gap must be 1-600000 ms")


class CameraRigKind(StrEnum):
    FIXED = "fixed"
    LOOK_AT = "look_at"
    ORBIT = "orbit"
    FOLLOW_ENTITY = "follow_entity"
    CHASE_ENTITY = "chase_entity"
    STABILIZED_MOUNTED_ENTITY = "stabilized_mounted_entity"
    FORMATION_OVERVIEW = "formation_overview"


class CameraStreamPolicy(StrEnum):
    DISABLED = "disabled"
    ON_DEMAND = "on_demand"
    CONTINUOUS = "continuous"


@dataclass(frozen=True, slots=True)
class CameraOptics:
    width_px: int
    height_px: int
    frame_rate_hz: int
    vertical_fov_degrees: float
    near_clip_m: float
    far_clip_m: float

    def __post_init__(self) -> None:
        if not 64 <= self.width_px <= 7_680 or not 64 <= self.height_px <= 4_320:
            raise ValueError("operator-camera resolution is outside the admitted range")
        if not 1 <= self.frame_rate_hz <= 120:
            raise ValueError("operator-camera cadence must be 1-120 Hz")
        if not math.isfinite(self.vertical_fov_degrees) or not (
            1.0 <= self.vertical_fov_degrees <= 160.0
        ):
            raise ValueError("operator-camera vertical FOV must be 1-160 degrees")
        if (
            not math.isfinite(self.near_clip_m)
            or not math.isfinite(self.far_clip_m)
            or self.near_clip_m <= 0.0
            or self.far_clip_m <= self.near_clip_m
        ):
            raise ValueError("operator-camera clipping range is invalid")


@dataclass(frozen=True, slots=True)
class OperatorCameraDefinition:
    camera_id: str
    rig_kind: CameraRigKind
    rig: object
    optics: CameraOptics
    stream_policy: CameraStreamPolicy
    revision: int = 1

    def __post_init__(self) -> None:
        if not self.camera_id or len(self.camera_id) > 128:
            raise ValueError("operator-camera identity is invalid")
        if self.revision < 1:
            raise ValueError("operator-camera revision must be positive")


def define_usd_camera(stage: Any, camera_path: str, optics: CameraOptics) -> Any:
    from pxr import Gf, UsdGeom

    camera = UsdGeom.Camera.Define(stage, camera_path)
    vertical_aperture_mm = 20.25
    horizontal_aperture_mm = vertical_aperture_mm * (
        optics.width_px / optics.height_px
    )
    camera.CreateHorizontalApertureAttr(horizontal_aperture_mm)
    focal_length_mm = (vertical_aperture_mm * 0.5) / math.tan(
        math.radians(optics.vertical_fov_degrees) * 0.5
    )
    camera.CreateVerticalApertureAttr(vertical_aperture_mm)
    camera.CreateFocalLengthAttr(focal_length_mm)
    camera.CreateClippingRangeAttr(
        Gf.Vec2f(optics.near_clip_m, optics.far_clip_m)
    )
    return UsdGeom.Xformable(camera.GetPrim()).AddTransformOp(
        precision=UsdGeom.XformOp.PrecisionDouble
    )


def apply_usd_camera_pose(transform_operation: Any, pose: Pose) -> None:
    from pxr import Gf

    orientation = pose.orientation_xyzw.normalized()
    rotation = Gf.Quatd(
        orientation.w,
        Gf.Vec3d(orientation.x, orientation.y, orientation.z),
    )
    matrix = Gf.Matrix4d().SetRotate(rotation)
    matrix.SetTranslateOnly(Gf.Vec3d(*pose.position_m.as_tuple()))
    transform_operation.Set(matrix)


def compose_pose(parent: Pose, local: Pose) -> Pose:
    parent_orientation = parent.orientation_xyzw.normalized()
    return Pose(
        parent.position_m + parent_orientation.rotate(local.position_m),
        (parent_orientation * local.orientation_xyzw.normalized()).normalized(),
    )


class AuthoritativeOperatorCamera:
    CAMERA_ROOT = "/World/OperatorCameras"

    def __init__(
        self,
        definition: OperatorCameraDefinition,
        camera_path: str,
        transform_operation: Any,
    ) -> None:
        from .operator_camera_rigs import smoothing_profile
        from .operator_camera_smoothing import CameraPoseFilter

        self.definition = definition
        self.camera_path = camera_path
        self._transform_operation = transform_operation
        profile = smoothing_profile(definition.rig)  # type: ignore[arg-type]
        self._filter = CameraPoseFilter(profile) if profile is not None else None
        self._last_pose: Pose | None = None

    @classmethod
    def create(
        cls,
        definition: OperatorCameraDefinition,
        stage: Any,
    ) -> "AuthoritativeOperatorCamera":
        camera_path = f"{cls.CAMERA_ROOT}/{definition.camera_id}"
        transform = define_usd_camera(stage, camera_path, definition.optics)
        return cls(definition, camera_path, transform)

    @property
    def last_pose(self) -> Pose | None:
        return self._last_pose

    @property
    def smoothing_diagnostics(self) -> object | None:
        return self._filter.diagnostics if self._filter is not None else None

    def update(
        self,
        entities: Mapping[str, EntityTransform],
        *,
        simulation_generation: int,
        physics_step: int,
        monotonic_seconds: float | None = None,
    ) -> Pose:
        from .operator_camera_rigs import desired_camera_pose, target_identity

        desired = desired_camera_pose(  # type: ignore[arg-type]
            self.definition.rig,
            entities,
        )
        if self._filter is None:
            pose = desired
        else:
            pose = self._filter.update(
                desired,
                monotonic_seconds=(
                    time.monotonic()
                    if monotonic_seconds is None
                    else monotonic_seconds
                ),
                target_identity=target_identity(self.definition.rig),  # type: ignore[arg-type]
                camera_revision=self.definition.revision,
                simulation_generation=simulation_generation,
                physics_step=physics_step,
            )
        self._apply_pose(pose)
        self._last_pose = pose
        return pose

    def _apply_pose(self, pose: Pose) -> None:
        apply_usd_camera_pose(self._transform_operation, pose)


class AuthoritativeOperatorCameraCollection:
    def __init__(self, cameras: tuple[AuthoritativeOperatorCamera, ...]) -> None:
        self._cameras = {camera.definition.camera_id: camera for camera in cameras}
        if len(self._cameras) != len(cameras):
            raise ValueError("authoritative operator-camera identities must be unique")

    @classmethod
    def create(
        cls,
        definitions: tuple[OperatorCameraDefinition, ...],
        stage: Any,
    ) -> "AuthoritativeOperatorCameraCollection":
        return cls(
            tuple(
                AuthoritativeOperatorCamera.create(definition, stage)
                for definition in definitions
            )
        )

    @property
    def cameras(self) -> tuple[AuthoritativeOperatorCamera, ...]:
        return tuple(
            sorted(
                self._cameras.values(),
                key=lambda camera: camera.definition.camera_id,
            )
        )

    def update(
        self,
        entities: Mapping[str, EntityTransform],
        *,
        simulation_generation: int,
        physics_step: int,
        monotonic_seconds: float | None = None,
    ) -> None:
        for camera in self.cameras:
            camera.update(
                entities,
                simulation_generation=simulation_generation,
                physics_step=physics_step,
                monotonic_seconds=monotonic_seconds,
            )
