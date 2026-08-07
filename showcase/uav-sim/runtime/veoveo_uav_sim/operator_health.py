from __future__ import annotations

import time
from collections import deque
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
        self._failure_diagnostic: str | None = None
        self._source_to_render_microseconds: deque[int] = deque(maxlen=256)

    def activate(self) -> None:
        self._active = True
        self._failure_diagnostic = None
        self._source_to_render_microseconds.clear()

    def deactivate(self) -> None:
        self._active = False
        self._last_frame = None
        self._failure_diagnostic = None
        self._source_to_render_microseconds.clear()

    def observe_frame(
        self,
        *,
        visible: bool | None = None,
        monotonic_seconds: float | None = None,
        source_to_render_microseconds: int | None = None,
    ) -> None:
        now = time.monotonic() if monotonic_seconds is None else monotonic_seconds
        if source_to_render_microseconds is not None:
            if source_to_render_microseconds < 0:
                raise ValueError("source-to-render latency must not be negative")
            self._source_to_render_microseconds.append(
                source_to_render_microseconds
            )
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

    def fail(self, diagnostic: str) -> None:
        if not diagnostic:
            raise ValueError("operator-product failure diagnostic must not be empty")
        self._failure_diagnostic = diagnostic

    def snapshot(
        self,
        *,
        content_ready: bool,
        monotonic_seconds: float | None = None,
    ) -> dict[str, object]:
        now = time.monotonic() if monotonic_seconds is None else monotonic_seconds
        if not self._active:
            lifecycle = OperatorProductLifecycle.INACTIVE
        elif self._failure_diagnostic is not None:
            lifecycle = OperatorProductLifecycle.FAILED
        elif not content_ready:
            lifecycle = OperatorProductLifecycle.STARTING
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
            "sourceToRenderSamples": len(self._source_to_render_microseconds),
        }
        if self._source_to_render_microseconds:
            ordered = sorted(self._source_to_render_microseconds)
            percentile_index = max(0, (len(ordered) * 95 + 99) // 100 - 1)
            result["sourceToRenderP95Microseconds"] = ordered[percentile_index]
        if self._last_frame is not None:
            result["lastFrameAt"] = self._last_frame.observed_at
            if self._last_frame.visible is not None:
                result["visible"] = self._last_frame.visible
        if self._failure_diagnostic is not None:
            result["diagnostic"] = self._failure_diagnostic
        elif self._active and not content_ready:
            result["diagnostic"] = "streamed world is warming"
        elif lifecycle is OperatorProductLifecycle.FAILED and (
            self._last_frame is not None and self._last_frame.visible is False
        ):
            result["diagnostic"] = "operator camera frame is uniform or blank"
        elif lifecycle is OperatorProductLifecycle.FAILED:
            result["diagnostic"] = "operator camera frame is stale"
        return result
