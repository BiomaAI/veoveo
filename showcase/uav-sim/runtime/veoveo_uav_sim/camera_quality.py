from __future__ import annotations

from dataclasses import dataclass

import numpy as np


MIN_MEAN_LUMA = 2.0
MIN_DYNAMIC_RANGE = 8
MIN_NON_BLACK_FRACTION = 0.02


@dataclass(frozen=True, slots=True)
class CameraFrameQuality:
    mean_luma: float
    dynamic_range: int
    non_black_fraction: float
    visible: bool


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
        return np.ascontiguousarray(np.clip(finite, 0.0, 255.0).round().astype(np.uint8))
    return np.ascontiguousarray(np.clip(rgb, 0, 255).astype(np.uint8))


def measure_camera_frame(rgb: np.ndarray) -> CameraFrameQuality:
    """Measure whether the exact RGB8 encoder input contains a visible image."""
    normalized = normalize_rgb_frame(rgb)
    luma = (
        normalized[..., 0].astype(np.float32) * 0.2126
        + normalized[..., 1].astype(np.float32) * 0.7152
        + normalized[..., 2].astype(np.float32) * 0.0722
    )
    mean_luma = float(luma.mean()) if luma.size else 0.0
    dynamic_range = (
        int(round(float(luma.max() - luma.min()))) if luma.size else 0
    )
    non_black_fraction = (
        float(np.count_nonzero(np.any(normalized > MIN_DYNAMIC_RANGE, axis=2)))
        / float(normalized.shape[0] * normalized.shape[1])
        if normalized.shape[0] and normalized.shape[1]
        else 0.0
    )
    visible = (
        mean_luma >= MIN_MEAN_LUMA
        and dynamic_range >= MIN_DYNAMIC_RANGE
        and non_black_fraction >= MIN_NON_BLACK_FRACTION
    )
    return CameraFrameQuality(
        mean_luma=mean_luma,
        dynamic_range=dynamic_range,
        non_black_fraction=non_black_fraction,
        visible=visible,
    )
