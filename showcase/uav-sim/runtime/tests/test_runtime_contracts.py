from __future__ import annotations

import os
import unittest
from unittest.mock import patch

import numpy as np

from veoveo_uav_sim.camera_quality import (
    measure_camera_frame,
    normalize_rgb_frame,
    should_record_camera_frame,
)
from veoveo_uav_sim.config import RuntimeConfig
from veoveo_uav_sim.contracts import ContractError, parse_command, parse_operation
from veoveo_uav_sim.geo import enu_to_geodetic, horizontal_distance_m
from veoveo_uav_sim.screenshot import ScreenshotGate
from veoveo_uav_sim.state import RuntimeState


VALID_ENVIRONMENT = {
    "CESIUM_ION_ACCESS_TOKEN": "test-token",
    "UAV_SIM_CESIUM_ION_ASSET_ID": "2275207",
    "UAV_SIM_FRAME_URI": "frames://frame/bioma-uav-origin",
    "UAV_SIM_RECORDING_KEY": "019f7122-3d89-7d21-8312-8940d1e0f510",
    "UAV_SIM_SESSION_ID": "bioma-uav",
    "UAV_SIM_TILE_CACHE_POLICY": "persistent",
    "UAV_SIM_WORLD_SOURCE": "google_photorealistic_3d_tiles",
}


class RuntimeConfigTests(unittest.TestCase):
    def test_google_tiles_are_mandatory_and_exact(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            config = RuntimeConfig.from_environment()
        self.assertEqual(config.cesium_ion_asset_id, 2_275_207)
        self.assertEqual(config.frame_uri, "frames://frame/bioma-uav-origin")
        self.assertEqual(config.tile_cache_policy.value, "persistent")

        invalid = {**VALID_ENVIRONMENT, "UAV_SIM_CESIUM_ION_ASSET_ID": "1"}
        with patch.dict(os.environ, invalid, clear=True):
            with self.assertRaisesRegex(ValueError, "Google Photorealistic 3D Tiles"):
                RuntimeConfig.from_environment()

    def test_direct_google_key_is_not_a_runtime_input(self) -> None:
        environment = {**VALID_ENVIRONMENT, "GOOGLE_MAPS_API_KEY": "not-used"}
        with patch.dict(os.environ, environment, clear=True):
            config = RuntimeConfig.from_environment()
        self.assertEqual(config.cesium_ion_access_token, "test-token")

    def test_default_render_cadence_matches_the_camera(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            config = RuntimeConfig.from_environment()
        self.assertEqual(config.rendering_hz, 20)
        self.assertEqual(config.rendering_hz, config.camera.fps)

    def test_nadir_camera_is_the_only_canonical_stream(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            state = RuntimeState(RuntimeConfig.from_environment()).snapshot()
        camera_path = state["cameras"][0]["entity_path"]
        recording_path = state["recordings"][0]["camera_streams"][0]
        self.assertTrue(camera_path.endswith("/camera/down"))
        self.assertEqual(recording_path, camera_path)
        self.assertNotIn("front", camera_path)

    def test_camera_optics_and_mount_are_typed_runtime_inputs(self) -> None:
        environment = {
            **VALID_ENVIRONMENT,
            "UAV_SIM_CAMERA_FOCAL_LENGTH_MM": "12.5",
            "UAV_SIM_CAMERA_CLIPPING_NEAR_M": "0.1",
            "UAV_SIM_CAMERA_CLIPPING_FAR_M": "50000",
            "UAV_SIM_CAMERA_TRANSLATION_X_M": "0.75",
        }
        with patch.dict(os.environ, environment, clear=True):
            camera = RuntimeConfig.from_environment().camera
        self.assertEqual(camera.focal_length_mm, 12.5)
        self.assertEqual(camera.clipping_near_m, 0.1)
        self.assertEqual(camera.clipping_far_m, 50_000.0)
        self.assertEqual(camera.mount.translation_xyz_m, (0.75, 0.0, 0.05))

    def test_camera_mount_rejects_a_non_unit_quaternion(self) -> None:
        environment = {
            **VALID_ENVIRONMENT,
            "UAV_SIM_CAMERA_ORIENTATION_W": "1",
            "UAV_SIM_CAMERA_ORIENTATION_X": "1",
            "UAV_SIM_CAMERA_ORIENTATION_Y": "0",
            "UAV_SIM_CAMERA_ORIENTATION_Z": "0",
        }
        with patch.dict(os.environ, environment, clear=True):
            with self.assertRaisesRegex(ValueError, "unit quaternion"):
                RuntimeConfig.from_environment()

    def test_camera_clipping_range_must_be_ordered(self) -> None:
        environment = {
            **VALID_ENVIRONMENT,
            "UAV_SIM_CAMERA_CLIPPING_NEAR_M": "10",
            "UAV_SIM_CAMERA_CLIPPING_FAR_M": "1",
        }
        with patch.dict(os.environ, environment, clear=True):
            with self.assertRaisesRegex(ValueError, "must be less than"):
                RuntimeConfig.from_environment()

    def test_showcase_screenshot_is_opt_in_and_typed(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            self.assertIsNone(RuntimeConfig.from_environment().screenshot)

        environment = {
            **VALID_ENVIRONMENT,
            "UAV_SIM_SCREENSHOT_PATH": "/tmp/isaac-uav.png",
            "UAV_SIM_SCREENSHOT_WIDTH": "1920",
            "UAV_SIM_SCREENSHOT_HEIGHT": "1080",
            "UAV_SIM_SCREENSHOT_MINIMUM_RELATIVE_ALTITUDE_M": "295",
            "UAV_SIM_SCREENSHOT_SETTLE_RENDERED_FRAMES": "45",
            "UAV_SIM_SCREENSHOT_EYE_OFFSET_X_M": "-6",
        }
        with patch.dict(os.environ, environment, clear=True):
            screenshot = RuntimeConfig.from_environment().screenshot
        self.assertIsNotNone(screenshot)
        assert screenshot is not None
        self.assertEqual(screenshot.output_path.as_posix(), "/tmp/isaac-uav.png")
        self.assertEqual((screenshot.width, screenshot.height), (1920, 1080))
        self.assertEqual(screenshot.minimum_relative_altitude_m, 295.0)
        self.assertEqual(screenshot.settle_rendered_frames, 45)
        self.assertEqual(screenshot.eye_offset_xyz_m, (-6.0, -2.2, 1.2))

    def test_showcase_screenshot_rejects_a_relative_or_non_png_path(self) -> None:
        for path in ("isaac-uav.png", "/tmp/isaac-uav.jpg"):
            with self.subTest(path=path):
                environment = {
                    **VALID_ENVIRONMENT,
                    "UAV_SIM_SCREENSHOT_PATH": path,
                }
                with patch.dict(os.environ, environment, clear=True):
                    with self.assertRaisesRegex(
                        ValueError, "absolute normalized PNG path"
                    ):
                        RuntimeConfig.from_environment()


class AdapterContractTests(unittest.TestCase):
    def test_commands_reject_unknown_fields(self) -> None:
        with self.assertRaises(ContractError):
            parse_command(
                {
                    "command": "arm",
                    "session_id": "bioma-uav",
                    "vehicle_id": "uav-1",
                    "legacy_vehicle": "one",
                }
            )

    def test_missions_require_the_typed_frame(self) -> None:
        mission = parse_operation(
            {
                "operation": "execute_mission",
                "input": {
                    "session_id": "bioma-uav",
                    "mission_id": "mission-1",
                    "frame_uri": "frames://frame/bioma-uav-origin",
                    "vehicles": [
                        {
                            "vehicle_id": "uav-1",
                            "waypoints": [
                                {
                                    "position": {
                                        "latitude_degrees": 13.6929,
                                        "longitude_degrees": -89.2182,
                                        "ellipsoid_height_m": 705.0,
                                    },
                                    "speed_mps": 3.0,
                                    "hold_seconds": 0.0,
                                }
                            ],
                        }
                    ],
                },
            }
        )
        self.assertEqual(mission.frame_uri, "frames://frame/bioma-uav-origin")

    def test_enu_origin_round_trips_to_wgs84(self) -> None:
        latitude, longitude, height = enu_to_geodetic(
            0.0, 0.0, 0.0, 13.6929, -89.2182, 700.0
        )
        self.assertAlmostEqual(latitude, 13.6929, places=8)
        self.assertAlmostEqual(longitude, -89.2182, places=8)
        self.assertAlmostEqual(height, 700.0, places=4)

    def test_horizontal_distance_resolves_short_uav_waypoints(self) -> None:
        distance = horizontal_distance_m(13.6929, -89.2182, 13.6929, -89.21818)
        self.assertGreater(distance, 2.0)
        self.assertLess(distance, 2.3)


class CameraQualityTests(unittest.TestCase):
    def test_black_camera_frame_is_not_visible(self) -> None:
        quality = measure_camera_frame(np.zeros((48, 64, 3), dtype=np.uint8))
        self.assertFalse(quality.operational)
        self.assertFalse(quality.visible)
        self.assertEqual(quality.mean_luma, 0.0)
        self.assertEqual(quality.dynamic_range, 0)
        self.assertEqual(quality.non_black_fraction, 0.0)

    def test_visible_camera_frame_is_accepted(self) -> None:
        frame = np.zeros((48, 64, 3), dtype=np.uint8)
        frame[8:40, 8:56] = (32, 128, 224)
        quality = measure_camera_frame(frame)
        self.assertTrue(quality.operational)
        self.assertTrue(quality.visible)
        self.assertGreater(quality.mean_luma, 2.0)
        self.assertGreater(quality.dynamic_range, 8)
        self.assertGreater(quality.non_black_fraction, 0.02)

    def test_uniform_bright_frame_is_not_visible_content(self) -> None:
        frame = np.full((48, 64, 3), 128, dtype=np.uint8)
        quality = measure_camera_frame(frame)
        self.assertTrue(quality.operational)
        self.assertFalse(quality.visible)
        self.assertEqual(quality.dynamic_range, 0)

    def test_ready_world_keeps_uniform_camera_frames_in_recording(self) -> None:
        quality = measure_camera_frame(np.full((48, 64, 3), 128, dtype=np.uint8))
        self.assertFalse(quality.visible)
        self.assertTrue(should_record_camera_frame(quality, tiles_ready=True))

    def test_warming_world_withholds_non_visible_camera_frames(self) -> None:
        quality = measure_camera_frame(np.zeros((48, 64, 3), dtype=np.uint8))
        self.assertFalse(should_record_camera_frame(quality, tiles_ready=False))

    def test_normalized_float_rgb_is_scaled_before_encoding(self) -> None:
        frame = np.full((4, 4, 3), 0.5, dtype=np.float32)
        normalized = normalize_rgb_frame(frame)
        self.assertEqual(normalized.dtype, np.uint8)
        self.assertEqual(int(normalized[0, 0, 0]), 128)


class ScreenshotGateTests(unittest.TestCase):
    def test_capture_requires_consecutive_ready_rendered_frames(self) -> None:
        gate = ScreenshotGate(settle_rendered_frames=3)
        self.assertFalse(gate.observe(rendered=True, ready=True))
        self.assertFalse(gate.observe(rendered=False, ready=True))
        self.assertFalse(gate.observe(rendered=True, ready=False))
        self.assertFalse(gate.observe(rendered=True, ready=True))
        self.assertFalse(gate.observe(rendered=True, ready=True))
        self.assertTrue(gate.observe(rendered=True, ready=True))
        self.assertFalse(gate.observe(rendered=True, ready=True))


if __name__ == "__main__":
    unittest.main()
