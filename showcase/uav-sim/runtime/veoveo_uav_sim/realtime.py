from __future__ import annotations

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


class PhysicsRenderSchedule:
    """Partition exact fixed physics steps across fixed render updates."""

    def __init__(self, physics_hz: int, rendering_hz: int) -> None:
        if physics_hz < 1 or rendering_hz < 1 or rendering_hz > physics_hz:
            raise ValueError("physics/render cadence is invalid")
        self._physics_hz = physics_hz
        self._rendering_hz = rendering_hz
        self._remainder = 0

    def next_step_count(self) -> int:
        self._remainder += self._physics_hz
        step_count, self._remainder = divmod(
            self._remainder, self._rendering_hz
        )
        if step_count < 1:
            raise RuntimeError("render update omitted its authoritative physics step")
        return step_count

    def reset(self) -> None:
        self._remainder = 0
