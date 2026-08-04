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
    """Maps a bounded wall-clock timeline onto fixed Isaac physics steps."""

    def __init__(
        self,
        frequency_hz: int,
        *,
        maximum_catchup_seconds: float = 0.5,
        clock: Callable[[], float] = time.monotonic,
    ) -> None:
        if frequency_hz < 1:
            raise ValueError("physics frequency must be positive")
        if not math.isfinite(maximum_catchup_seconds) or maximum_catchup_seconds <= 0:
            raise ValueError("maximum catch-up duration must be positive and finite")
        self._frequency_hz = frequency_hz
        self._maximum_catchup_steps = max(
            1, round(frequency_hz * maximum_catchup_seconds)
        )
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
        if lag > self._maximum_catchup_steps:
            discarded_steps = lag - self._maximum_catchup_steps
            self._discarded_wall_seconds += discarded_steps / self._frequency_hz
            self._rebases += 1
            self._anchor_step = physics_step
            self._anchor_wall = current_wall - (
                self._maximum_catchup_steps / self._frequency_hz
            )
            return self._maximum_catchup_steps
        return max(0, lag)

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
