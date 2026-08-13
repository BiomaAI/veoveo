from __future__ import annotations

import math
import unittest

import numpy as np
from veoveo_uav_sim.hydra_camera import (
    HydraRenderedCamera,
    hydra_rendered_camera,
)
from veoveo_uav_sim.operator_camera import (
    Pose,
    QuaternionXyzw,
    Vector3,
)
from veoveo_uav_sim.render_pose import rendered_pose_agreement


def _rendered(view: np.ndarray) -> HydraRenderedCamera:
    return HydraRenderedCamera(
        view=tuple(float(value) for value in view.reshape(16)),
        width=640,
        height=480,
    )


class RenderPoseTests(unittest.TestCase):
    def test_rejects_incomplete_hydra_metadata(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "invalid camera metadata"):
            hydra_rendered_camera({"view": [1.0] * 15, "resolution": [640, 480]})

    def test_exact_row_vector_view_agrees_with_authority(self) -> None:
        camera_to_world = np.identity(4)
        camera_to_world[3, :3] = (10.0, 20.0, 30.0)
        view = np.linalg.inv(camera_to_world)

        agreement = rendered_pose_agreement(
            _rendered(view),
            Pose(Vector3(10.0, 20.0, 30.0), QuaternionXyzw.identity()),
        )

        self.assertAlmostEqual(agreement.position_error_m, 0.0)
        self.assertAlmostEqual(agreement.forward_error_degrees, 0.0)

    def test_accepts_transposed_hydra_serialization(self) -> None:
        camera_to_world = np.identity(4)
        camera_to_world[3, :3] = (-4.0, 5.0, 6.0)
        view = np.linalg.inv(camera_to_world).transpose()

        agreement = rendered_pose_agreement(
            _rendered(view),
            Pose(Vector3(-4.0, 5.0, 6.0), QuaternionXyzw.identity()),
        )

        self.assertAlmostEqual(agreement.position_error_m, 0.0)
        self.assertAlmostEqual(agreement.forward_error_degrees, 0.0)

    def test_reports_optical_axis_error(self) -> None:
        camera_to_world = np.identity(4)
        view = np.linalg.inv(camera_to_world)
        half_angle = math.sin(math.pi / 4.0)

        agreement = rendered_pose_agreement(
            _rendered(view),
            Pose(
                Vector3(0.0, 0.0, 0.0),
                QuaternionXyzw(0.0, half_angle, 0.0, half_angle),
            ),
        )

        self.assertAlmostEqual(agreement.forward_error_degrees, 90.0)


if __name__ == "__main__":
    unittest.main()
