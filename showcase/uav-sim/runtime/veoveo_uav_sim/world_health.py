from __future__ import annotations

from dataclasses import dataclass
from typing import Literal


type TileLifecycle = Literal["connecting", "streaming", "ready", "failed"]


@dataclass(frozen=True, slots=True)
class TileHealth:
    lifecycle: TileLifecycle
    coverage_frames: int
    recovery_required: bool
    diagnostic: str | None


def assess_tile_health(
    *,
    resident_tiles: int,
    visible_tiles: int,
    loading_tiles: int,
    coverage_frames: int,
    ready_frames: int,
    absent_seconds: float,
    failed_latched: bool = False,
    unavailable_seconds: float = 30.0,
) -> TileHealth:
    """Classify streamed-world health without stopping simulation physics."""
    if ready_frames < 1:
        raise ValueError("ready frame threshold must be positive")
    if unavailable_seconds <= 0.0:
        raise ValueError("unavailable timeout must be positive")
    if visible_tiles > 0:
        next_coverage_frames = coverage_frames + 1
        lifecycle: TileLifecycle = (
            "ready" if next_coverage_frames >= ready_frames else "streaming"
        )
        return TileHealth(lifecycle, next_coverage_frames, False, None)
    if failed_latched:
        return TileHealth(
            "failed",
            0,
            False,
            "streamed-world coverage is unavailable for the current camera viewport",
        )
    if absent_seconds >= unavailable_seconds:
        return TileHealth(
            "failed",
            0,
            True,
            "streamed-world coverage is unavailable for the current camera viewport",
        )
    lifecycle: TileLifecycle = (
        "streaming" if resident_tiles > 0 or loading_tiles > 0 else "connecting"
    )
    return TileHealth(lifecycle, 0, False, None)
