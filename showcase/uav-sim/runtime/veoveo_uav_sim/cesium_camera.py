from __future__ import annotations

from typing import Protocol, TypeVar

import numpy as np

from .hydra_camera import HydraRenderViewport


CesiumViewportT = TypeVar("CesiumViewportT")
Matrix4dT = TypeVar("Matrix4dT")


class CesiumViewportFactory(Protocol[CesiumViewportT]):
    def __call__(self) -> CesiumViewportT: ...


class Matrix4dFactory(Protocol[Matrix4dT]):
    def __call__(self, *values: float) -> Matrix4dT: ...


def _matrix(values: tuple[float, ...], name: str) -> np.ndarray:
    if len(values) != 16 or not all(np.isfinite(value) for value in values):
        raise RuntimeError(f"RTX Hydra product returned an invalid {name} matrix")
    return np.asarray(values, dtype=np.float64).reshape((4, 4))


def _camera_position(view: np.ndarray) -> np.ndarray:
    try:
        inverse = np.linalg.inv(view)
    except np.linalg.LinAlgError as error:
        raise RuntimeError("RTX Hydra product returned a singular view matrix") from error
    # Gf.Matrix4d uses row vectors and stores translation in the fourth row.
    return inverse[3, :3]


def _perspective_layout_score(projection: np.ndarray) -> float:
    # Gf.Matrix4d uses row vectors. Perspective division therefore places its
    # unit coefficient in row 2, column 3; serialized GPU matrices commonly
    # carry the transposed form instead.
    return float(
        abs(abs(projection[2, 3]) - 1.0)
        + abs(projection[0, 3])
        + abs(projection[1, 3])
        + abs(projection[3, 3])
    )


def normalized_hydra_matrices(
    viewport: HydraRenderViewport,
    expected_camera_position_m: tuple[float, float, float],
    *,
    maximum_position_error_m: float = 20.0,
) -> tuple[tuple[float, ...], tuple[float, ...]]:
    """Normalize Hydra's serialized matrix layout against camera authority."""
    if maximum_position_error_m <= 0.0:
        raise ValueError("maximum camera position error must be positive")
    expected = np.asarray(expected_camera_position_m, dtype=np.float64)
    if expected.shape != (3,) or not np.all(np.isfinite(expected)):
        raise ValueError("expected camera position must contain three finite values")

    view = _matrix(viewport.view, "view")
    projection = _matrix(viewport.projection, "projection")
    selected_view = min(
        (view, view.transpose()),
        key=lambda candidate: float(
            np.linalg.norm(_camera_position(candidate) - expected)
        ),
    )
    position_error_m = float(
        np.linalg.norm(_camera_position(selected_view) - expected)
    )
    if position_error_m > maximum_position_error_m:
        raise RuntimeError(
            "RTX Hydra camera matrix disagrees with authoritative camera pose: "
            f"position error {position_error_m:.3f} m"
        )
    selected_projection = min(
        (projection, projection.transpose()),
        key=_perspective_layout_score,
    )
    projection_score = _perspective_layout_score(selected_projection)
    if projection_score > 0.01:
        raise RuntimeError(
            "RTX Hydra product returned an unsupported projection matrix layout"
        )
    return (
        tuple(float(value) for value in selected_view.reshape(16)),
        tuple(float(value) for value in selected_projection.reshape(16)),
    )


def current_cesium_viewport(
    viewport: HydraRenderViewport,
    expected_camera_position_m: tuple[float, float, float],
    viewport_type: CesiumViewportFactory[CesiumViewportT],
    matrix_type: Matrix4dFactory[Matrix4dT],
) -> CesiumViewportT:
    """Project one rendered camera into Cesium's native viewport contract."""
    view, projection = normalized_hydra_matrices(
        viewport,
        expected_camera_position_m,
    )
    cesium_viewport = viewport_type()
    cesium_viewport.viewMatrix = matrix_type(*view)
    cesium_viewport.projMatrix = matrix_type(*projection)
    cesium_viewport.width = float(viewport.width)
    cesium_viewport.height = float(viewport.height)
    return cesium_viewport
