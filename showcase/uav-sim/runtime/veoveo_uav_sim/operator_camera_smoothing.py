from __future__ import annotations

import math
from dataclasses import dataclass
from enum import StrEnum

from .operator_camera import CameraSmoothingProfile, Pose, QuaternionXyzw


def half_life_alpha(delta_seconds: float, half_life_ms: int) -> float:
    if not math.isfinite(delta_seconds) or delta_seconds < 0.0:
        raise ValueError("camera smoothing delta must be finite and non-negative")
    if half_life_ms == 0:
        return 1.0
    return 1.0 - math.pow(2.0, -delta_seconds / (half_life_ms / 1_000.0))


def shortest_arc_slerp(
    start: QuaternionXyzw,
    end: QuaternionXyzw,
    alpha: float,
) -> QuaternionXyzw:
    if not math.isfinite(alpha) or not 0.0 <= alpha <= 1.0:
        raise ValueError("quaternion interpolation alpha must be between zero and one")
    first = start.normalized()
    second = end.normalized()
    dot = first.dot(second)
    if dot < 0.0:
        second = second.negated()
        dot = -dot
    dot = min(1.0, max(-1.0, dot))
    if dot > 0.9995:
        return QuaternionXyzw(
            first.x + alpha * (second.x - first.x),
            first.y + alpha * (second.y - first.y),
            first.z + alpha * (second.z - first.z),
            first.w + alpha * (second.w - first.w),
        ).normalized()
    angle = math.acos(dot)
    sine = math.sin(angle)
    start_weight = math.sin((1.0 - alpha) * angle) / sine
    end_weight = math.sin(alpha * angle) / sine
    return QuaternionXyzw(
        start_weight * first.x + end_weight * second.x,
        start_weight * first.y + end_weight * second.y,
        start_weight * first.z + end_weight * second.z,
        start_weight * first.w + end_weight * second.w,
    ).normalized()


class CameraFilterResetReason(StrEnum):
    INITIALIZED = "initialized"
    TARGET_CHANGED = "target_changed"
    CAMERA_REVISION_CHANGED = "camera_revision_changed"
    SIMULATION_RESET = "simulation_reset"
    PHYSICS_DISCONTINUITY = "physics_discontinuity"
    RENDER_GAP = "render_gap"
    TELEPORT = "teleport"


@dataclass(frozen=True, slots=True)
class CameraFilterDiagnostics:
    initialized: bool
    last_reset_reason: CameraFilterResetReason | None
    reset_count: int
    last_physics_step: int | None
    translation_alpha: float
    rotation_alpha: float


class CameraPoseFilter:
    def __init__(self, profile: CameraSmoothingProfile) -> None:
        self._profile = profile
        self._pose: Pose | None = None
        self._last_monotonic_seconds: float | None = None
        self._target_identity: str | None = None
        self._camera_revision: int | None = None
        self._simulation_generation: int | None = None
        self._last_physics_step: int | None = None
        self._last_reset_reason: CameraFilterResetReason | None = None
        self._reset_count = 0
        self._translation_alpha = 1.0
        self._rotation_alpha = 1.0

    @property
    def diagnostics(self) -> CameraFilterDiagnostics:
        return CameraFilterDiagnostics(
            initialized=self._pose is not None,
            last_reset_reason=self._last_reset_reason,
            reset_count=self._reset_count,
            last_physics_step=self._last_physics_step,
            translation_alpha=self._translation_alpha,
            rotation_alpha=self._rotation_alpha,
        )

    def update(
        self,
        desired_pose: Pose,
        *,
        monotonic_seconds: float,
        target_identity: str,
        camera_revision: int,
        simulation_generation: int,
        physics_step: int,
    ) -> Pose:
        if not math.isfinite(monotonic_seconds) or monotonic_seconds < 0.0:
            raise ValueError("camera monotonic time must be finite and non-negative")
        if camera_revision < 1 or simulation_generation < 0 or physics_step < 0:
            raise ValueError("camera revision, generation, and physics step are invalid")
        desired_pose = desired_pose.normalized()
        reset_reason = self._reset_reason(
            desired_pose,
            monotonic_seconds,
            target_identity,
            camera_revision,
            simulation_generation,
            physics_step,
        )
        if reset_reason is not None:
            self._reset(
                desired_pose,
                monotonic_seconds,
                target_identity,
                camera_revision,
                simulation_generation,
                physics_step,
                reset_reason,
            )
            return desired_pose

        assert self._pose is not None
        assert self._last_monotonic_seconds is not None
        delta_seconds = monotonic_seconds - self._last_monotonic_seconds
        self._translation_alpha = half_life_alpha(
            delta_seconds, self._profile.translation_half_life_ms
        )
        self._rotation_alpha = half_life_alpha(
            delta_seconds, self._profile.rotation_half_life_ms
        )
        self._pose = Pose(
            self._pose.position_m.lerp(
                desired_pose.position_m, self._translation_alpha
            ),
            shortest_arc_slerp(
                self._pose.orientation_xyzw,
                desired_pose.orientation_xyzw,
                self._rotation_alpha,
            ),
        )
        self._last_monotonic_seconds = monotonic_seconds
        self._last_physics_step = physics_step
        return self._pose

    def force_reset(self, reason: CameraFilterResetReason) -> None:
        self._pose = None
        self._last_monotonic_seconds = None
        self._target_identity = None
        self._camera_revision = None
        self._simulation_generation = None
        self._last_physics_step = None
        self._last_reset_reason = reason
        self._reset_count += 1

    def _reset_reason(
        self,
        desired_pose: Pose,
        monotonic_seconds: float,
        target_identity: str,
        camera_revision: int,
        simulation_generation: int,
        physics_step: int,
    ) -> CameraFilterResetReason | None:
        if self._pose is None or self._last_monotonic_seconds is None:
            return CameraFilterResetReason.INITIALIZED
        if self._target_identity != target_identity:
            return CameraFilterResetReason.TARGET_CHANGED
        if self._camera_revision != camera_revision:
            return CameraFilterResetReason.CAMERA_REVISION_CHANGED
        if self._simulation_generation != simulation_generation:
            return CameraFilterResetReason.SIMULATION_RESET
        if self._last_physics_step is not None and physics_step < self._last_physics_step:
            return CameraFilterResetReason.PHYSICS_DISCONTINUITY
        delta_seconds = monotonic_seconds - self._last_monotonic_seconds
        if delta_seconds < 0.0:
            return CameraFilterResetReason.RENDER_GAP
        if delta_seconds * 1_000.0 > self._profile.reset_after_gap_ms:
            return CameraFilterResetReason.RENDER_GAP
        if self._pose.position_m.distance(desired_pose.position_m) > self._profile.teleport_distance_m:
            return CameraFilterResetReason.TELEPORT
        return None

    def _reset(
        self,
        desired_pose: Pose,
        monotonic_seconds: float,
        target_identity: str,
        camera_revision: int,
        simulation_generation: int,
        physics_step: int,
        reason: CameraFilterResetReason,
    ) -> None:
        self._pose = desired_pose
        self._last_monotonic_seconds = monotonic_seconds
        self._target_identity = target_identity
        self._camera_revision = camera_revision
        self._simulation_generation = simulation_generation
        self._last_physics_step = physics_step
        self._last_reset_reason = reason
        self._reset_count += 1
        self._translation_alpha = 1.0
        self._rotation_alpha = 1.0
