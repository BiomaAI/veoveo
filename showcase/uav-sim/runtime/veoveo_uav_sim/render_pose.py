from __future__ import annotations

import math
from dataclasses import dataclass

import numpy as np

from .hydra_camera import HydraRenderedCamera
from .operator_camera import Pose, Vector3


@dataclass(frozen=True, slots=True)
class RenderPoseAgreement:
    """Agreement between camera authority and one rendered Hydra frame."""

    position_error_m: float
    forward_error_degrees: float
    rendered_position_m: Vector3
    rendered_forward: Vector3

    def as_dict(self) -> dict[str, object]:
        return {
            "position_error_m": self.position_error_m,
            "forward_error_degrees": self.forward_error_degrees,
            "rendered_position_enu_m": {
                "east_m": self.rendered_position_m.x,
                "north_m": self.rendered_position_m.y,
                "up_m": self.rendered_position_m.z,
            },
            "rendered_forward_enu": {
                "east": self.rendered_forward.x,
                "north": self.rendered_forward.y,
                "up": self.rendered_forward.z,
            },
        }


def rendered_pose_agreement(
    rendered: HydraRenderedCamera,
    expected: Pose,
) -> RenderPoseAgreement:
    """Compare Hydra's rendered view with the exact authoritative pose."""
    values = np.asarray(rendered.view, dtype=np.float64).reshape((4, 4))
    expected_position = np.asarray(
        expected.position_m.as_tuple(), dtype=np.float64
    )
    candidates: list[tuple[float, np.ndarray, np.ndarray]] = []
    for view in (values, values.transpose()):
        try:
            camera_to_world = np.linalg.inv(view)
        except np.linalg.LinAlgError:
            continue
        position = camera_to_world[3, :3]
        forward = -camera_to_world[2, :3]
        forward_norm = float(np.linalg.norm(forward))
        if (
            not np.all(np.isfinite(position))
            or not np.all(np.isfinite(forward))
            or forward_norm <= 1.0e-12
        ):
            continue
        candidates.append(
            (
                float(np.linalg.norm(position - expected_position)),
                position,
                forward / forward_norm,
            )
        )
    if not candidates:
        raise RuntimeError("RTX Hydra product returned a singular camera view")
    position_error_m, position, forward = min(
        candidates,
        key=lambda candidate: candidate[0],
    )
    expected_forward_value = expected.orientation_xyzw.rotate(
        Vector3(0.0, 0.0, -1.0)
    ).normalized()
    expected_forward = np.asarray(
        expected_forward_value.as_tuple(), dtype=np.float64
    )
    cosine = float(np.clip(np.dot(forward, expected_forward), -1.0, 1.0))
    return RenderPoseAgreement(
        position_error_m=position_error_m,
        forward_error_degrees=math.degrees(math.acos(cosine)),
        rendered_position_m=Vector3(*map(float, position)),
        rendered_forward=Vector3(*map(float, forward)),
    )
