from __future__ import annotations

import math
import time
from collections.abc import Callable


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

    def reset(self, physics_step: int = 0) -> None:
        if physics_step < 0:
            raise ValueError("physics step cannot be negative")
        self._last_step = physics_step


class MonotonicPhysicsClock:
    """Select fixed physics work from elapsed monotonic time without dropping debt."""

    def __init__(
        self,
        physics_hz: int,
        *,
        maximum_steps_per_pass: int,
        clock: Callable[[], float] = time.monotonic,
    ) -> None:
        if physics_hz < 1:
            raise ValueError("physics cadence must be positive")
        if maximum_steps_per_pass < 1:
            raise ValueError("maximum physics steps per pass must be positive")
        self._physics_hz = physics_hz
        self._maximum_steps_per_pass = maximum_steps_per_pass
        self._clock = clock
        self._anchor_step = 0
        self._anchor_wall_seconds = clock()
        self._last_step = 0

    def reset(self, physics_step: int = 0, *, now: float | None = None) -> None:
        if physics_step < 0:
            raise ValueError("physics step cannot be negative")
        self._anchor_step = physics_step
        self._anchor_wall_seconds = self._clock() if now is None else now
        self._last_step = physics_step

    def due_steps(self, physics_step: int, *, now: float | None = None) -> int:
        if physics_step < self._last_step:
            raise RuntimeError("physics step moved backward without a clock reset")
        self._last_step = physics_step
        current_wall_seconds = self._clock() if now is None else now
        elapsed_seconds = max(
            0.0, current_wall_seconds - self._anchor_wall_seconds
        )
        expected_step = self._anchor_step + math.floor(
            elapsed_seconds * self._physics_hz
        )
        return min(
            max(0, expected_step - physics_step),
            self._maximum_steps_per_pass,
        )

    def seconds_until_next_step(
        self, physics_step: int, *, now: float | None = None
    ) -> float:
        current_wall_seconds = self._clock() if now is None else now
        next_step_wall_seconds = self._anchor_wall_seconds + (
            physics_step + 1 - self._anchor_step
        ) / self._physics_hz
        return max(
            0.0, next_step_wall_seconds - current_wall_seconds
        )
