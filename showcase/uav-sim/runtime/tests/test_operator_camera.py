from __future__ import annotations

import math
import unittest

from veoveo_uav_sim.operator_camera import (
    CameraSmoothingProfile,
    EntityTransform,
    Pose,
    QuaternionXyzw,
    Vector3,
)
from veoveo_uav_sim.operator_camera_rigs import (
    ChaseEntityRig,
    FollowEntityRig,
    FormationOverviewRig,
    StabilizedMountedEntityRig,
    desired_camera_pose,
)
from veoveo_uav_sim.operator_camera_smoothing import (
    CameraFilterResetReason,
    CameraPoseFilter,
    half_life_alpha,
    shortest_arc_slerp,
)


def _profile() -> CameraSmoothingProfile:
    return CameraSmoothingProfile(
        translation_half_life_ms=100,
        rotation_half_life_ms=100,
        teleport_distance_m=100.0,
        reset_after_gap_ms=500,
    )


def _yaw(degrees: float) -> QuaternionXyzw:
    radians = math.radians(degrees) * 0.5
    return QuaternionXyzw(0.0, 0.0, math.sin(radians), math.cos(radians))


class OperatorCameraSmoothingTests(unittest.TestCase):
    def test_half_life_halves_remaining_translation_error(self) -> None:
        camera = CameraPoseFilter(_profile())
        start = Pose(Vector3(0.0, 0.0, 0.0), QuaternionXyzw.identity())
        desired = Pose(Vector3(10.0, 0.0, 0.0), QuaternionXyzw.identity())
        camera.update(
            start,
            monotonic_seconds=0.0,
            target_identity="uav-1",
            camera_revision=1,
            simulation_generation=1,
            physics_step=0,
        )
        result = camera.update(
            desired,
            monotonic_seconds=0.1,
            target_identity="uav-1",
            camera_revision=1,
            simulation_generation=1,
            physics_step=6,
        )
        self.assertAlmostEqual(result.position_m.x, 5.0, places=9)
        self.assertAlmostEqual(half_life_alpha(0.1, 100), 0.5, places=9)

    def test_filter_is_frame_rate_independent(self) -> None:
        def run(rate_hz: int) -> Pose:
            camera = CameraPoseFilter(_profile())
            pose = Pose(Vector3(0.0, 0.0, 0.0), QuaternionXyzw.identity())
            camera.update(
                pose,
                monotonic_seconds=0.0,
                target_identity="uav-1",
                camera_revision=1,
                simulation_generation=1,
                physics_step=0,
            )
            desired = Pose(Vector3(12.0, -3.0, 8.0), _yaw(135.0))
            for step in range(1, rate_hz + 1):
                pose = camera.update(
                    desired,
                    monotonic_seconds=step / rate_hz,
                    target_identity="uav-1",
                    camera_revision=1,
                    simulation_generation=1,
                    physics_step=step,
                )
            return pose

        at_30 = run(30)
        for rate in (60, 120):
            result = run(rate)
            self.assertLess(result.position_m.distance(at_30.position_m), 1.0e-9)
            self.assertAlmostEqual(
                abs(result.orientation_xyzw.dot(at_30.orientation_xyzw)),
                1.0,
                places=9,
            )

    def test_shortest_arc_slerp_normalizes_opposite_signs(self) -> None:
        target = _yaw(170.0)
        result = shortest_arc_slerp(
            QuaternionXyzw.identity(), target.negated(), 0.5
        )
        self.assertAlmostEqual(result.norm(), 1.0, places=12)
        expected = _yaw(85.0)
        self.assertAlmostEqual(abs(result.dot(expected)), 1.0, places=9)

    def test_zero_half_life_snaps_without_overshoot(self) -> None:
        profile = CameraSmoothingProfile(0, 0, 100.0, 500)
        camera = CameraPoseFilter(profile)
        camera.update(
            Pose(Vector3(0.0, 0.0, 0.0), QuaternionXyzw.identity()),
            monotonic_seconds=0.0,
            target_identity="uav-1",
            camera_revision=1,
            simulation_generation=1,
            physics_step=0,
        )
        desired = Pose(Vector3(4.0, 5.0, 6.0), _yaw(45.0))
        result = camera.update(
            desired,
            monotonic_seconds=0.01,
            target_identity="uav-1",
            camera_revision=1,
            simulation_generation=1,
            physics_step=1,
        )
        self.assertEqual(result.position_m, desired.position_m)
        self.assertAlmostEqual(abs(result.orientation_xyzw.dot(_yaw(45.0))), 1.0)

    def test_target_revision_reset_gap_and_teleport_snap(self) -> None:
        camera = CameraPoseFilter(_profile())
        initial = Pose(Vector3(0.0, 0.0, 0.0), QuaternionXyzw.identity())
        camera.update(
            initial,
            monotonic_seconds=0.0,
            target_identity="uav-1",
            camera_revision=1,
            simulation_generation=1,
            physics_step=0,
        )
        target_changed = Pose(Vector3(1.0, 0.0, 0.0), _yaw(10.0))
        self.assertEqual(
            camera.update(
                target_changed,
                monotonic_seconds=0.01,
                target_identity="uav-2",
                camera_revision=1,
                simulation_generation=1,
                physics_step=1,
            ),
            target_changed,
        )
        self.assertEqual(
            camera.diagnostics.last_reset_reason,
            CameraFilterResetReason.TARGET_CHANGED,
        )
        revised = Pose(Vector3(2.0, 0.0, 0.0), _yaw(20.0))
        camera.update(
            revised,
            monotonic_seconds=0.02,
            target_identity="uav-2",
            camera_revision=2,
            simulation_generation=1,
            physics_step=2,
        )
        self.assertEqual(
            camera.diagnostics.last_reset_reason,
            CameraFilterResetReason.CAMERA_REVISION_CHANGED,
        )
        after_gap = Pose(Vector3(3.0, 0.0, 0.0), _yaw(30.0))
        camera.update(
            after_gap,
            monotonic_seconds=1.0,
            target_identity="uav-2",
            camera_revision=2,
            simulation_generation=1,
            physics_step=3,
        )
        self.assertEqual(
            camera.diagnostics.last_reset_reason, CameraFilterResetReason.RENDER_GAP
        )
        teleported = Pose(Vector3(200.0, 0.0, 0.0), _yaw(30.0))
        self.assertEqual(
            camera.update(
                teleported,
                monotonic_seconds=1.01,
                target_identity="uav-2",
                camera_revision=2,
                simulation_generation=1,
                physics_step=4,
            ),
            teleported,
        )
        self.assertEqual(
            camera.diagnostics.last_reset_reason, CameraFilterResetReason.TELEPORT
        )


class OperatorCameraRigTests(unittest.TestCase):
    def setUp(self) -> None:
        self.entities = {
            "uav-1": EntityTransform(
                "uav-1", Pose(Vector3(10.0, 20.0, 30.0), _yaw(90.0))
            ),
            "uav-2": EntityTransform(
                "uav-2", Pose(Vector3(14.0, 24.0, 34.0), _yaw(90.0))
            ),
        }

    def test_follow_uses_authoritative_position_and_orientation(self) -> None:
        pose = desired_camera_pose(
            FollowEntityRig(
                "uav-1",
                Vector3(-10.0, 0.0, 2.0),
                Vector3(0.0, 0.0, 0.0),
                _profile(),
            ),
            self.entities,
        )
        self.assertAlmostEqual(pose.position_m.x, 10.0, places=9)
        self.assertAlmostEqual(pose.position_m.y, 10.0, places=9)
        self.assertAlmostEqual(pose.position_m.z, 32.0, places=9)
        self.assertAlmostEqual(pose.orientation_xyzw.norm(), 1.0, places=12)

    def test_chase_and_camera_target_share_one_authoritative_transform(self) -> None:
        pose = desired_camera_pose(
            ChaseEntityRig("uav-1", distance_m=8.0, height_m=3.0, smoothing=_profile()),
            self.entities,
        )
        self.assertAlmostEqual(pose.position_m.x, 10.0, places=9)
        self.assertAlmostEqual(pose.position_m.y, 12.0, places=9)
        self.assertAlmostEqual(pose.position_m.z, 33.0, places=9)

    def test_stabilized_mount_composes_entity_and_mount(self) -> None:
        pose = desired_camera_pose(
            StabilizedMountedEntityRig(
                "uav-1",
                Pose(Vector3(1.0, 0.0, 0.0), QuaternionXyzw.identity()),
                _profile(),
            ),
            self.entities,
        )
        self.assertAlmostEqual(pose.position_m.x, 10.0, places=9)
        self.assertAlmostEqual(pose.position_m.y, 21.0, places=9)
        self.assertAlmostEqual(pose.position_m.z, 30.0, places=9)

    def test_formation_overview_uses_current_centroid(self) -> None:
        pose = desired_camera_pose(
            FormationOverviewRig(("uav-1", "uav-2"), 10.0, _profile()),
            self.entities,
        )
        self.assertLess(pose.position_m.x, 12.0)
        self.assertLess(pose.position_m.y, 22.0)
        self.assertGreater(pose.position_m.z, 32.0)
        self.assertAlmostEqual(pose.orientation_xyzw.norm(), 1.0, places=12)


if __name__ == "__main__":
    unittest.main()
