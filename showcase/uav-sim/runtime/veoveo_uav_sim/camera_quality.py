from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

import numpy as np


MIN_MEAN_LUMA = 2.0
MIN_ROBUST_DYNAMIC_RANGE = 12
MIN_LUMA_STANDARD_DEVIATION = 4.0
MIN_NON_BLACK_FRACTION = 0.02


type CameraContent = Literal["black", "uniform", "visible"]
type CameraDiagnosticCode = Literal["frame_black", "frame_uniform"]


@dataclass(frozen=True, slots=True)
class CameraFrameQuality:
    mean_luma: float
    dynamic_range: int
    robust_dynamic_range: int
    luma_standard_deviation: float
    non_black_fraction: float
    operational: bool
    content: CameraContent

    @property
    def visible(self) -> bool:
        return self.content == "visible"


type CameraLifecycle = Literal["warming", "ready", "degraded"]


@dataclass(frozen=True, slots=True)
class CameraHealth:
    lifecycle: CameraLifecycle
    diagnostic_code: CameraDiagnosticCode | None
    diagnostic: str | None


def assess_camera_health(
    quality: CameraFrameQuality,
    *,
    visible_streak: int,
    unusable_streak_after_tiles: int,
    was_ready: bool,
    prolonged_unusable_threshold: int,
) -> CameraHealth:
    """Classify sensor health without making it simulation authority."""
    if prolonged_unusable_threshold < 1:
        raise ValueError("prolonged unusable threshold must be positive")
    if visible_streak >= 3:
        return CameraHealth("ready", None, None)
    if quality.visible:
        lifecycle: CameraLifecycle = "degraded" if was_ready else "warming"
        return CameraHealth(lifecycle, None, None)
    diagnostic_code: CameraDiagnosticCode = (
        "frame_black" if quality.content == "black" else "frame_uniform"
    )
    diagnostic = (
        "camera RGB frame is black"
        if quality.content == "black"
        else "camera RGB frame is uniform and lacks visible scene detail"
    )
    if unusable_streak_after_tiles >= prolonged_unusable_threshold:
        return CameraHealth(
            "degraded",
            diagnostic_code,
            f"{diagnostic} after streamed-world content became ready",
        )
    lifecycle: CameraLifecycle = "degraded" if was_ready else "warming"
    return CameraHealth(lifecycle, diagnostic_code, diagnostic)


def normalize_rgb_frame(pixels: np.ndarray) -> np.ndarray:
    """Return the camera frame as contiguous RGB8 bytes."""
    if pixels.ndim != 3 or pixels.shape[2] < 3:
        raise ValueError(f"camera RGB frame has invalid shape {pixels.shape!r}")
    rgb = pixels[..., :3]
    if rgb.dtype == np.uint8:
        return np.ascontiguousarray(rgb)
    if np.issubdtype(rgb.dtype, np.floating):
        finite = np.nan_to_num(rgb, nan=0.0, posinf=1.0, neginf=0.0)
        if finite.size and float(finite.max()) <= 1.0:
            finite = finite * 255.0
        return np.ascontiguousarray(
            np.clip(finite, 0.0, 255.0).round().astype(np.uint8)
        )
    return np.ascontiguousarray(np.clip(rgb, 0, 255).astype(np.uint8))


def measure_camera_frame(rgb: np.ndarray) -> CameraFrameQuality:
    """Measure whether the exact RGB8 encoder input contains a visible image.

    TODO(GPU): Move these reductions to CUDA with the recording readback cut.
    """
    normalized = normalize_rgb_frame(rgb)
    luma = (
        normalized[..., 0].astype(np.float32) * 0.2126
        + normalized[..., 1].astype(np.float32) * 0.7152
        + normalized[..., 2].astype(np.float32) * 0.0722
    )
    mean_luma = float(luma.mean()) if luma.size else 0.0
    dynamic_range = int(round(float(luma.max() - luma.min()))) if luma.size else 0
    luma_standard_deviation = float(luma.std()) if luma.size else 0.0
    if luma.size:
        luma_u8 = np.clip(luma.round(), 0.0, 255.0).astype(np.uint8)
        histogram = np.bincount(luma_u8.ravel(), minlength=256)
        cumulative = np.cumsum(histogram)
        pixel_count = int(cumulative[-1])
        lower = int(np.searchsorted(cumulative, pixel_count * 0.05, side="left"))
        upper = int(np.searchsorted(cumulative, pixel_count * 0.95, side="left"))
        robust_dynamic_range = upper - lower
    else:
        robust_dynamic_range = 0
    non_black_fraction = (
        float(np.count_nonzero(np.any(normalized > MIN_ROBUST_DYNAMIC_RANGE, axis=2)))
        / float(normalized.shape[0] * normalized.shape[1])
        if normalized.shape[0] and normalized.shape[1]
        else 0.0
    )
    operational = (
        mean_luma >= MIN_MEAN_LUMA and non_black_fraction >= MIN_NON_BLACK_FRACTION
    )
    visible = (
        operational
        and robust_dynamic_range >= MIN_ROBUST_DYNAMIC_RANGE
        and luma_standard_deviation >= MIN_LUMA_STANDARD_DEVIATION
    )
    content: CameraContent = (
        "visible" if visible else "uniform" if operational else "black"
    )
    return CameraFrameQuality(
        mean_luma=mean_luma,
        dynamic_range=dynamic_range,
        robust_dynamic_range=robust_dynamic_range,
        luma_standard_deviation=luma_standard_deviation,
        non_black_fraction=non_black_fraction,
        operational=operational,
        content=content,
    )


def should_record_camera_frame(quality: CameraFrameQuality) -> bool:
    """Admit only a frame that contains visible scene detail to video encoding."""
    return quality.visible
