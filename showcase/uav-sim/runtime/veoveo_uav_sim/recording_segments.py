from __future__ import annotations

from dataclasses import dataclass


@dataclass(slots=True)
class RecordingSegmentBudget:
    """Conservative encoded-payload and wall-time budget for one Rerun recording."""

    maximum_bytes: int
    maximum_seconds: int
    opened_monotonic_s: float
    used_bytes: int = 64 * 1024

    def __post_init__(self) -> None:
        if self.maximum_bytes <= self.used_bytes:
            raise ValueError("recording maximum bytes must exceed static context budget")
        if self.maximum_seconds <= 0:
            raise ValueError("recording maximum seconds must be positive")

    def should_rotate_before(self, payload_bytes: int, now_monotonic_s: float) -> bool:
        if payload_bytes < 0:
            raise ValueError("recording payload bytes must not be negative")
        if now_monotonic_s < self.opened_monotonic_s:
            raise ValueError("recording segment clock moved backwards")
        return (
            self.used_bytes + payload_bytes > self.maximum_bytes
            or now_monotonic_s - self.opened_monotonic_s >= self.maximum_seconds
        )

    def account(self, payload_bytes: int) -> None:
        if payload_bytes < 0:
            raise ValueError("recording payload bytes must not be negative")
        self.used_bytes += payload_bytes
