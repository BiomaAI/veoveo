from __future__ import annotations

from typing import Any

import numpy as np
import warp as wp

from .camera_quality import CameraFrameQuality, measure_camera_histogram


@wp.kernel
def _reduce_rgba8(
    rgba: wp.array3d(dtype=wp.uint8),
    histogram: wp.array(dtype=wp.int32),
    non_black: wp.array(dtype=wp.int32),
) -> None:
    row, column = wp.tid()
    red = wp.int32(rgba[row, column, 0])
    green = wp.int32(rgba[row, column, 1])
    blue = wp.int32(rgba[row, column, 2])
    luma = (54 * red + 183 * green + 19 * blue + 128) // 256
    wp.atomic_add(histogram, luma, 1)
    if red > 12 or green > 12 or blue > 12:
        wp.atomic_add(non_black, 0, 1)


class GpuCameraQualityReducer:
    """Reduce a CUDA RGBA frame to bounded scalar health evidence."""

    def __init__(self, width: int, height: int, device: str = "cuda:0") -> None:
        if width < 1 or height < 1:
            raise ValueError("GPU camera-quality dimensions must be positive")
        self._width = width
        self._height = height
        self._device = device
        self._histogram = wp.zeros(256, dtype=wp.int32, device=device)
        self._non_black = wp.zeros(1, dtype=wp.int32, device=device)

    def measure(self, rgba: Any) -> CameraFrameQuality:
        shape = tuple(int(value) for value in getattr(rgba, "shape", ()))
        if shape != (self._height, self._width, 4):
            raise RuntimeError(
                "native Isaac CUDA LdrColor shape changed unexpectedly: "
                f"{shape!r}"
            )
        if not str(getattr(rgba, "device", "")).startswith("cuda"):
            raise RuntimeError("native Isaac LdrColor quality input is not CUDA-resident")
        self._histogram.zero_()
        self._non_black.zero_()
        wp.launch(
            _reduce_rgba8,
            dim=(self._height, self._width),
            inputs=[rgba, self._histogram, self._non_black],
            device=self._device,
        )
        # Only 257 reduced integers cross to the CPU. Raw pixels never do.
        histogram = np.asarray(self._histogram.numpy(), dtype=np.int64)
        non_black = int(self._non_black.numpy()[0])
        return measure_camera_histogram(histogram, non_black)
