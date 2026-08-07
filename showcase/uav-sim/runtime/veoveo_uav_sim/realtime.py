from __future__ import annotations

import math
import time
from dataclasses import dataclass
from typing import Callable


@dataclass(frozen=True, slots=True)
class RealtimeClockStatus:
    rebases: int
    discarded_wall_seconds: float


class RealtimePhysicsClock:
    """Paces fixed Isaac steps without replaying missed actuator intervals."""

    def __init__(
        self,
        frequency_hz: int,
        *,
        clock: Callable[[], float] = time.monotonic,
    ) -> None:
        if frequency_hz < 1:
            raise ValueError("physics frequency must be positive")
        self._frequency_hz = frequency_hz
        self._clock = clock
        self._anchor_step = 0
        self._anchor_wall = clock()
        self._last_step = 0
        self._rebases = 0
        self._discarded_wall_seconds = 0.0

    def reset(self, physics_step: int = 0, *, now: float | None = None) -> None:
        if physics_step < 0:
            raise ValueError("physics step cannot be negative")
        self._anchor_step = physics_step
        self._last_step = physics_step
        self._anchor_wall = self._clock() if now is None else now

    def due_steps(self, physics_step: int, *, now: float | None = None) -> int:
        if physics_step < self._last_step:
            raise RuntimeError("physics step moved backward without a clock reset")
        self._last_step = physics_step
        current_wall = self._clock() if now is None else now
        elapsed = max(0.0, current_wall - self._anchor_wall)
        expected_step = self._anchor_step + math.floor(elapsed * self._frequency_hz)
        lag = expected_step - physics_step
        if lag > 1:
            discarded_steps = lag - 1
            self._discarded_wall_seconds += discarded_steps / self._frequency_hz
            self._rebases += 1
            self._anchor_step = physics_step
            self._anchor_wall = current_wall - (1.0 / self._frequency_hz)
            return 1
        return max(0, min(1, lag))

    def seconds_until_next_step(
        self, physics_step: int, *, now: float | None = None
    ) -> float:
        current_wall = self._clock() if now is None else now
        next_step_wall = self._anchor_wall + (
            physics_step + 1 - self._anchor_step
        ) / self._frequency_hz
        return max(0.0, next_step_wall - current_wall)

    def status(self) -> RealtimeClockStatus:
        return RealtimeClockStatus(
            rebases=self._rebases,
            discarded_wall_seconds=self._discarded_wall_seconds,
        )


class PeriodicDeadline:
    """Advances a wall-clock deadline without replaying missed render periods."""

    def __init__(
        self,
        frequency_hz: int,
        *,
        clock: Callable[[], float] = time.monotonic,
    ) -> None:
        if frequency_hz < 1:
            raise ValueError("deadline frequency must be positive")
        self._period = 1.0 / frequency_hz
        self._clock = clock
        self._next = clock()

    def due(self, *, now: float | None = None) -> bool:
        current = self._clock() if now is None else now
        if current < self._next:
            return False
        missed = math.floor((current - self._next) / self._period)
        self._next += (missed + 1) * self._period
        return True

    def seconds_until_due(self, *, now: float | None = None) -> float:
        current = self._clock() if now is None else now
        return max(0.0, self._next - current)

    def reset(self, *, now: float | None = None) -> None:
        self._next = self._clock() if now is None else now


class FixedStepCadenceGate:
    """Select an exact rational output cadence from monotonic physics steps."""

    def __init__(self, physics_hz: int, output_hz: int) -> None:
        if physics_hz < 1 or output_hz < 1 or output_hz > physics_hz:
            raise ValueError("physics/output cadence is invalid")
        self._physics_hz = physics_hz
        self._output_hz = output_hz
        self._last_step = 0

    def due(self, physics_step: int) -> bool:
        if physics_step <= self._last_step:
            raise RuntimeError("physics cadence steps must increase monotonically")
        previous_bucket = ((physics_step - 1) * self._output_hz) // self._physics_hz
        current_bucket = (physics_step * self._output_hz) // self._physics_hz
        self._last_step = physics_step
        return current_bucket > previous_bucket

    def reset(self) -> None:
        self._last_step = 0
