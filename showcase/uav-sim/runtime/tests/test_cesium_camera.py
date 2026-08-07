from __future__ import annotations

import unittest

import numpy as np

from veoveo_uav_sim.cesium_camera import normalized_hydra_matrices
from veoveo_uav_sim.hydra_camera import HydraRenderViewport


def _serialized(matrix: np.ndarray) -> tuple[float, ...]:
    return tuple(float(value) for value in matrix.reshape(16))


def _projection() -> np.ndarray:
    projection = np.zeros((4, 4), dtype=np.float64)
    projection[0, 0] = 2.0
    projection[1, 1] = 3.0
    projection[2, 2] = -1.001
    projection[2, 3] = -1.0
    projection[3, 2] = -0.1
    return projection


class CesiumCameraTests(unittest.TestCase):
    def test_preserves_row_major_hydra_matrices(self) -> None:
        view = np.identity(4)
        view[3, :3] = (-100.0, -200.0, -300.0)
        projection = _projection()
        viewport = HydraRenderViewport(
            _serialized(view),
            _serialized(projection),
            640,
            480,
        )

        actual_view, actual_projection = normalized_hydra_matrices(
            viewport,
            (100.0, 200.0, 300.0),
        )

        self.assertEqual(actual_view, _serialized(view))
        self.assertEqual(actual_projection, _serialized(projection))

    def test_transposes_column_major_hydra_serialization(self) -> None:
        view = np.identity(4)
        view[3, :3] = (-100.0, -200.0, -300.0)
        projection = _projection()
        viewport = HydraRenderViewport(
            _serialized(view.transpose()),
            _serialized(projection.transpose()),
            640,
            480,
        )

        actual_view, actual_projection = normalized_hydra_matrices(
            viewport,
            (100.0, 200.0, 300.0),
        )

        self.assertEqual(actual_view, _serialized(view))
        self.assertEqual(actual_projection, _serialized(projection))

    def test_normalizes_projection_independently_from_view(self) -> None:
        view = np.identity(4)
        view[3, :3] = (-100.0, -200.0, -300.0)
        projection = _projection()
        viewport = HydraRenderViewport(
            _serialized(view),
            _serialized(projection.transpose()),
            640,
            480,
        )

        actual_view, actual_projection = normalized_hydra_matrices(
            viewport,
            (100.0, 200.0, 300.0),
        )

        self.assertEqual(actual_view, _serialized(view))
        self.assertEqual(actual_projection, _serialized(projection))

    def test_rejects_matrix_that_disagrees_with_camera_authority(self) -> None:
        identity = _serialized(np.identity(4))
        viewport = HydraRenderViewport(identity, identity, 640, 480)

        with self.assertRaisesRegex(RuntimeError, "authoritative camera pose"):
            normalized_hydra_matrices(viewport, (100.0, 200.0, 300.0))


if __name__ == "__main__":
    unittest.main()
