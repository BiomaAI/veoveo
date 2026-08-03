from __future__ import annotations

import math
import statistics
import time
from collections import deque
from collections.abc import Callable
from dataclasses import dataclass
from enum import Enum

from .contracts import ContractError, InterpolationPolicy
from .pose import EntityPose, PosePollResult, PoseSampleKind, PoseSnapshot

CADENCE_WINDOW_SAMPLES = 16
MAXIMUM_COUNTER = (1 << 64) - 1


class InterpolationRuntimeState(str, Enum):
    RESET = "reset"
    WARMING = "warming"
    HOLD_LATEST = "hold_latest"
    INTERPOLATING = "interpolating"
    HOLDING = "holding"


class InterpolationResetReason(str, Enum):
    POSE_SOURCE_CHANGED = "pose_source_changed"
    AUTHORIZATION_REVISION_CHANGED = "authorization_revision_changed"
    ENTITY_TABLE_CHANGED = "entity_table_changed"
    SEQUENCE_GAP = "sequence_gap"
    SEQUENCE_REPEATED = "sequence_repeated"
    SEQUENCE_REVERSED = "sequence_reversed"
    TIMESTAMP_NOT_INCREASING = "timestamp_not_increasing"
    STALE = "stale"
    REVOKED = "revoked"


@dataclass(frozen=True, slots=True)
class RenderedPoseFrame:
    session_id: str
    epoch_id: str
    source_sequence: int
    simulation_timestamp_ns: int
    frame_uri: str
    frame_digest: str
    entity_table_revision: int
    entity_table_digest: str
    entities: tuple[EntityPose, ...]


@dataclass(frozen=True, slots=True)
class InterpolationDiagnostics:
    policy: InterpolationPolicy
    state: InterpolationRuntimeState
    previous_source_sequence: int | None
    current_source_sequence: int | None
    previous_simulation_timestamp_ns: int | None
    current_simulation_timestamp_ns: int | None
    rendered_simulation_timestamp_ns: int | None
    interpolation_alpha: float | None
    interpolation_delay_ns: int
    discontinuity_reset_count: int
    repeated_source_sample_count: int
    skipped_source_sample_count: int
    last_reset_reason: InterpolationResetReason | None

    def response(self) -> dict[str, object]:
        return {
            "policy": self.policy.value,
            "state": self.state.value,
            "previousSourceSequence": self.previous_source_sequence,
            "currentSourceSequence": self.current_source_sequence,
            "previousSimulationTimestampNs": (self.previous_simulation_timestamp_ns),
            "currentSimulationTimestampNs": (self.current_simulation_timestamp_ns),
            "renderedSimulationTimestampNs": (self.rendered_simulation_timestamp_ns),
            "interpolationAlpha": self.interpolation_alpha,
            "interpolationDelayNs": self.interpolation_delay_ns,
            "discontinuityResetCount": self.discontinuity_reset_count,
            "repeatedSourceSampleCount": (self.repeated_source_sample_count),
            "skippedSourceSampleCount": self.skipped_source_sample_count,
            "lastResetReason": (
                self.last_reset_reason.value
                if self.last_reset_reason is not None
                else None
            ),
        }


class PoseInterpolator:
    def __init__(
        self,
        policy: InterpolationPolicy,
        maximum_cadence_hz: int,
        stale_after_ms: int,
        clock_ns: Callable[[], int] = time.monotonic_ns,
    ) -> None:
        if maximum_cadence_hz < 1 or stale_after_ms < 1:
            raise ContractError("pose interpolation limits are invalid")
        self._policy = policy
        self._minimum_delay_ns = max(1, 1_000_000_000 // maximum_cadence_hz)
        self._maximum_delay_ns = stale_after_ms * 1_000_000
        self._clock_ns = clock_ns
        self._cadences_ns: deque[int] = deque(maxlen=CADENCE_WINDOW_SAMPLES)
        self._previous: PoseSnapshot | None = None
        self._current: PoseSnapshot | None = None
        self._current_received_at_ns: int | None = None
        self._rendered_timestamp_ns: int | None = None
        self._alpha: float | None = None
        self._state = InterpolationRuntimeState.RESET
        self._reset_count = 0
        self._repeated_count = 0
        self._skipped_count = 0
        self._last_reset_reason: InterpolationResetReason | None = None

    @property
    def policy(self) -> InterpolationPolicy:
        return self._policy

    def observe(self, result: PosePollResult) -> None:
        if result.kind == PoseSampleKind.REPEATED:
            self._repeated_count = _increment(self._repeated_count)
            self.reset(
                InterpolationResetReason.SEQUENCE_REPEATED,
                seed=self._current,
            )
            return
        if result.kind == PoseSampleKind.SEQUENCE_REVERSED:
            self.reset(
                InterpolationResetReason.SEQUENCE_REVERSED,
                seed=self._current,
            )
            return
        if result.kind == PoseSampleKind.TIMESTAMP_NOT_INCREASING:
            self.reset(
                InterpolationResetReason.TIMESTAMP_NOT_INCREASING,
                seed=self._current,
            )
            return
        snapshot = result.snapshot
        if result.kind != PoseSampleKind.ACCEPTED or snapshot is None:
            raise ContractError("pose poll result is invalid")
        if result.skipped_samples:
            self._skipped_count = _increment(
                self._skipped_count, result.skipped_samples
            )
            self.reset(
                InterpolationResetReason.SEQUENCE_GAP,
                seed=snapshot,
            )
            return
        self._accept(snapshot)

    def reset(
        self,
        reason: InterpolationResetReason,
        seed: PoseSnapshot | None = None,
    ) -> None:
        self._reset_count = _increment(self._reset_count)
        self._last_reset_reason = reason
        self._cadences_ns.clear()
        self._previous = None
        self._current = seed
        self._current_received_at_ns = self._clock_ns() if seed is not None else None
        self._rendered_timestamp_ns = None
        self._alpha = None
        self._state = InterpolationRuntimeState.RESET

    def render(self) -> RenderedPoseFrame | None:
        current = self._current
        if current is None:
            self._rendered_timestamp_ns = None
            self._alpha = None
            return None
        if self._policy == InterpolationPolicy.HOLD_LATEST:
            self._state = InterpolationRuntimeState.HOLD_LATEST
            self._alpha = 1.0
            self._rendered_timestamp_ns = current.simulation_timestamp_ns
            return _source_frame(current)
        previous = self._previous
        received_at = self._current_received_at_ns
        if previous is None or received_at is None:
            self._state = InterpolationRuntimeState.WARMING
            self._alpha = 1.0
            self._rendered_timestamp_ns = current.simulation_timestamp_ns
            return _source_frame(current)
        delay_ns = self._delay_ns()
        elapsed_ns = max(0, self._clock_ns() - received_at)
        alpha = min(1.0, elapsed_ns / delay_ns)
        rendered_timestamp_ns = round(
            previous.simulation_timestamp_ns
            + (current.simulation_timestamp_ns - previous.simulation_timestamp_ns)
            * alpha
        )
        rendered_timestamp_ns = max(
            previous.simulation_timestamp_ns,
            min(current.simulation_timestamp_ns, rendered_timestamp_ns),
        )
        self._alpha = alpha
        self._rendered_timestamp_ns = rendered_timestamp_ns
        self._state = (
            InterpolationRuntimeState.HOLDING
            if alpha >= 1.0
            else InterpolationRuntimeState.INTERPOLATING
        )
        return RenderedPoseFrame(
            session_id=current.session_id,
            epoch_id=current.epoch_id,
            source_sequence=current.sequence,
            simulation_timestamp_ns=rendered_timestamp_ns,
            frame_uri=current.frame_uri,
            frame_digest=current.frame_digest,
            entity_table_revision=current.entity_table_revision,
            entity_table_digest=current.entity_table_digest,
            entities=tuple(
                _interpolate_entity(left, right, alpha)
                for left, right in zip(previous.entities, current.entities, strict=True)
            ),
        )

    def diagnostics(self) -> InterpolationDiagnostics:
        previous = self._previous
        current = self._current
        return InterpolationDiagnostics(
            policy=self._policy,
            state=self._state,
            previous_source_sequence=(
                previous.sequence if previous is not None else None
            ),
            current_source_sequence=(current.sequence if current is not None else None),
            previous_simulation_timestamp_ns=(
                previous.simulation_timestamp_ns if previous is not None else None
            ),
            current_simulation_timestamp_ns=(
                current.simulation_timestamp_ns if current is not None else None
            ),
            rendered_simulation_timestamp_ns=self._rendered_timestamp_ns,
            interpolation_alpha=self._alpha,
            interpolation_delay_ns=self._delay_ns(),
            discontinuity_reset_count=self._reset_count,
            repeated_source_sample_count=self._repeated_count,
            skipped_source_sample_count=self._skipped_count,
            last_reset_reason=self._last_reset_reason,
        )

    def _accept(self, snapshot: PoseSnapshot) -> None:
        current = self._current
        if current is None:
            self._current = snapshot
            self._current_received_at_ns = self._clock_ns()
            self._state = InterpolationRuntimeState.WARMING
            return
        if not _same_pose_identity(current, snapshot):
            self.reset(
                InterpolationResetReason.ENTITY_TABLE_CHANGED,
                seed=snapshot,
            )
            return
        if snapshot.sequence != current.sequence + 1:
            skipped = max(0, snapshot.sequence - current.sequence - 1)
            self._skipped_count = _increment(self._skipped_count, skipped)
            self.reset(
                InterpolationResetReason.SEQUENCE_GAP,
                seed=snapshot,
            )
            return
        delta_ns = snapshot.simulation_timestamp_ns - current.simulation_timestamp_ns
        if delta_ns <= 0:
            self.reset(
                InterpolationResetReason.TIMESTAMP_NOT_INCREASING,
                seed=current,
            )
            return
        self._cadences_ns.append(
            max(
                self._minimum_delay_ns,
                min(self._maximum_delay_ns, delta_ns),
            )
        )
        self._previous = current
        self._current = snapshot
        self._current_received_at_ns = self._clock_ns()

    def _delay_ns(self) -> int:
        if not self._cadences_ns:
            return self._minimum_delay_ns
        return int(statistics.median(self._cadences_ns))


def _source_frame(snapshot: PoseSnapshot) -> RenderedPoseFrame:
    return RenderedPoseFrame(
        session_id=snapshot.session_id,
        epoch_id=snapshot.epoch_id,
        source_sequence=snapshot.sequence,
        simulation_timestamp_ns=snapshot.simulation_timestamp_ns,
        frame_uri=snapshot.frame_uri,
        frame_digest=snapshot.frame_digest,
        entity_table_revision=snapshot.entity_table_revision,
        entity_table_digest=snapshot.entity_table_digest,
        entities=snapshot.entities,
    )


def _same_pose_identity(left: PoseSnapshot, right: PoseSnapshot) -> bool:
    return (
        left.session_id == right.session_id
        and left.epoch_id == right.epoch_id
        and left.frame_uri == right.frame_uri
        and left.frame_digest == right.frame_digest
        and left.entity_table_revision == right.entity_table_revision
        and left.entity_table_digest == right.entity_table_digest
        and tuple(entity.entity_id for entity in left.entities)
        == tuple(entity.entity_id for entity in right.entities)
    )


def _interpolate_entity(
    previous: EntityPose, current: EntityPose, alpha: float
) -> EntityPose:
    if previous.entity_id != current.entity_id:
        raise ContractError("pose entity table changed during interpolation")
    return EntityPose(
        entity_id=current.entity_id,
        position_enu_m=tuple(
            start + (end - start) * alpha
            for start, end in zip(
                previous.position_enu_m,
                current.position_enu_m,
                strict=True,
            )
        ),
        orientation_xyzw=_slerp(
            previous.orientation_xyzw,
            current.orientation_xyzw,
            alpha,
        ),
        active=(current.active if alpha >= 1.0 else previous.active),
        visible=(current.visible if alpha >= 1.0 else previous.visible),
    )


def _slerp(
    previous: tuple[float, float, float, float],
    current: tuple[float, float, float, float],
    alpha: float,
) -> tuple[float, float, float, float]:
    left = _normalize_quaternion(previous)
    right = _normalize_quaternion(current)
    dot = sum(start * end for start, end in zip(left, right, strict=True))
    if dot < 0.0:
        right = tuple(-component for component in right)
        dot = -dot
    dot = max(-1.0, min(1.0, dot))
    if dot > 0.9995:
        return _normalize_quaternion(
            tuple(
                start + (end - start) * alpha
                for start, end in zip(left, right, strict=True)
            )
        )
    angle = math.acos(dot)
    sine = math.sin(angle)
    if abs(sine) < 1e-12:
        return left
    left_weight = math.sin((1.0 - alpha) * angle) / sine
    right_weight = math.sin(alpha * angle) / sine
    return _normalize_quaternion(
        tuple(
            start * left_weight + end * right_weight
            for start, end in zip(left, right, strict=True)
        )
    )


def _normalize_quaternion(
    value: tuple[float, float, float, float],
) -> tuple[float, float, float, float]:
    magnitude = math.sqrt(sum(component * component for component in value))
    if not math.isfinite(magnitude) or magnitude < 1e-12:
        raise ContractError("pose entity quaternion cannot be normalized")
    return tuple(component / magnitude for component in value)


def _increment(value: int, amount: int = 1) -> int:
    return min(MAXIMUM_COUNTER, value + max(0, amount))
