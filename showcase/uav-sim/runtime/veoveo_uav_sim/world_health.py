from __future__ import annotations

from dataclasses import dataclass
from typing import Literal


type TileLifecycle = Literal["connecting", "streaming", "ready", "failed"]


@dataclass(frozen=True, slots=True)
class TileHealth:
    lifecycle: TileLifecycle
    resident_frames: int
    diagnostic: str | None


def assess_tile_health(
    *,
    resident_tiles: int,
    loading_tiles: int,
    resident_frames: int,
    ready_frames: int,
    absent_seconds: float,
    unavailable_seconds: float = 600.0,
) -> TileHealth:
    """Classify streamed-world health without stopping simulation physics."""
    if ready_frames < 1:
        raise ValueError("ready frame threshold must be positive")
    if unavailable_seconds <= 0.0:
        raise ValueError("unavailable timeout must be positive")
    if resident_tiles > 0:
        next_resident_frames = resident_frames + 1
        lifecycle: TileLifecycle = (
            "ready" if next_resident_frames >= ready_frames else "streaming"
        )
        return TileHealth(lifecycle, next_resident_frames, None)
    if absent_seconds >= unavailable_seconds:
        return TileHealth(
            "failed",
            0,
            "Google Photorealistic 3D Tiles are unavailable",
        )
    return TileHealth("streaming" if loading_tiles > 0 else "connecting", 0, None)
