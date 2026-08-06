from __future__ import annotations

import time
from dataclasses import dataclass
from datetime import datetime, timezone
from enum import StrEnum


def _timestamp() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


class OperatorProductLifecycle(StrEnum):
    INACTIVE = "inactive"
    STARTING = "starting"
    READY = "ready"
    FAILED = "failed"


@dataclass(frozen=True, slots=True)
class OperatorFrameHealth:
    sequence: int
    observed_monotonic_seconds: float
    observed_at: str
    visible: bool | None


class OperatorProductHealth:
    def __init__(self, maximum_frame_age_ms: int) -> None:
        if not 1 <= maximum_frame_age_ms <= 60_000:
            raise ValueError("operator-product maximum frame age must be 1-60000 ms")
        self._maximum_frame_age_seconds = maximum_frame_age_ms / 1_000.0
        self._active = False
        self._sequence = 0
        self._last_frame: OperatorFrameHealth | None = None
        self._diagnostic: str | None = None

    def activate(self) -> None:
        self._active = True
        self._diagnostic = None

    def deactivate(self) -> None:
        self._active = False
        self._last_frame = None
        self._diagnostic = None

    def observe_frame(
        self,
        *,
        visible: bool | None = None,
        monotonic_seconds: float | None = None,
    ) -> None:
        now = time.monotonic() if monotonic_seconds is None else monotonic_seconds
        retained_visibility = (
            self._last_frame.visible
            if visible is None and self._last_frame is not None
            else visible
        )
        self._sequence += 1
        self._last_frame = OperatorFrameHealth(
            sequence=self._sequence,
            observed_monotonic_seconds=now,
            observed_at=_timestamp(),
            visible=retained_visibility,
        )
        self._diagnostic = (
            "operator camera frame is uniform or blank"
            if retained_visibility is False
            else None
        )

    def fail(self, diagnostic: str) -> None:
        if not diagnostic:
            raise ValueError("operator-product failure diagnostic must not be empty")
        self._diagnostic = diagnostic

    def snapshot(self, monotonic_seconds: float | None = None) -> dict[str, object]:
        now = time.monotonic() if monotonic_seconds is None else monotonic_seconds
        if not self._active:
            lifecycle = OperatorProductLifecycle.INACTIVE
        elif self._diagnostic is not None:
            lifecycle = OperatorProductLifecycle.FAILED
        elif self._last_frame is None:
            lifecycle = OperatorProductLifecycle.STARTING
        elif now - self._last_frame.observed_monotonic_seconds > self._maximum_frame_age_seconds:
            lifecycle = OperatorProductLifecycle.FAILED
        elif self._last_frame.visible is False:
            lifecycle = OperatorProductLifecycle.FAILED
        else:
            lifecycle = OperatorProductLifecycle.READY
        result: dict[str, object] = {
            "lifecycle": lifecycle.value,
            "encodedFrames": self._sequence,
        }
        if self._last_frame is not None:
            result["lastFrameAt"] = self._last_frame.observed_at
            if self._last_frame.visible is not None:
                result["visible"] = self._last_frame.visible
        if self._diagnostic is not None:
            result["diagnostic"] = self._diagnostic
        elif lifecycle is OperatorProductLifecycle.FAILED:
            result["diagnostic"] = "operator camera frame is stale"
        return result
