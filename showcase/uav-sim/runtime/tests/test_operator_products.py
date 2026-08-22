from __future__ import annotations

import json
from collections import deque
import threading
import unittest

from veoveo_uav_sim.h264 import NativeH264AccessUnit
from veoveo_uav_sim.operator_camera import CameraRigKind
from veoveo_uav_sim.operator_camera_config import OperatorLiveViewRuntimeConfig
from veoveo_uav_sim.operator_health import OperatorProductHealth
from veoveo_uav_sim.operator_products import (
    OPERATOR_ATLAS_NAME,
    OPERATOR_ATLAS_PRODUCT_ID,
    OperatorCameraProduct,
    operator_aov_arguments,
    operator_atlas_layout,
)


def _smoothing() -> dict[str, object]:
    return {
        "translationHalfLifeMs": 120,
        "rotationHalfLifeMs": 160,
        "teleportDistanceM": 250.0,
        "resetAfterGapMs": 750,
    }


def _optics() -> dict[str, object]:
    return {
        "widthPx": 1280,
        "heightPx": 720,
        "frameRateHz": 30,
        "verticalFovDegrees": 60.0,
        "nearClipM": 0.1,
        "farClipM": 100000.0,
    }


def _camera(camera_id: str, slot: int, rig: dict[str, object]) -> dict[str, object]:
    return {
        "cameraId": camera_id,
        "revision": 1,
        "rig": rig,
        "optics": _optics(),
        "streamPolicy": "continuous",
    }


def _config(cameras: list[dict[str, object]]) -> OperatorLiveViewRuntimeConfig:
    return OperatorLiveViewRuntimeConfig.from_json(
        json.dumps(cameras),
        rtsp_port_base=8560,
    )


class OperatorCameraConfigTests(unittest.TestCase):
    def test_all_declared_rigs_parse_strictly(self) -> None:
        cameras = [
            _camera(
                "fixed",
                0,
                {
                    "kind": "fixed",
                    "pose": {
                        "positionM": {"x": 0.0, "y": 0.0, "z": 10.0},
                        "orientationXyzw": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0},
                    },
                },
            ),
            _camera(
                "look-at",
                1,
                {
                    "kind": "look_at",
                    "eyeM": {"x": 0.0, "y": 0.0, "z": 10.0},
                    "targetM": {"x": 0.0, "y": 0.0, "z": 0.0},
                    "smoothing": _smoothing(),
                },
            ),
            _camera(
                "orbit",
                2,
                {
                    "kind": "orbit",
                    "targetEntityId": "uav-1",
                    "radiusM": 50.0,
                    "azimuthDegrees": 30.0,
                    "elevationDegrees": 20.0,
                    "smoothing": _smoothing(),
                },
            ),
            _camera(
                "follow",
                3,
                {
                    "kind": "follow_entity",
                    "targetEntityId": "uav-1",
                    "eyeOffsetFluM": {"x": -30.0, "y": 0.0, "z": 8.0},
                    "targetOffsetFluM": {"x": 0.0, "y": 0.0, "z": 0.0},
                    "smoothing": _smoothing(),
                },
            ),
            _camera(
                "chase",
                4,
                {
                    "kind": "chase_entity",
                    "targetEntityId": "uav-1",
                    "distanceM": 25.0,
                    "heightM": 6.0,
                    "smoothing": _smoothing(),
                },
            ),
            _camera(
                "mount",
                5,
                {
                    "kind": "stabilized_mounted_entity",
                    "targetEntityId": "uav-1",
                    "mount": {
                        "positionM": {"x": 1.0, "y": 0.0, "z": -0.2},
                        "orientationXyzw": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0},
                    },
                    "smoothing": _smoothing(),
                },
            ),
            _camera(
                "formation",
                6,
                {
                    "kind": "formation_overview",
                    "targetEntityIds": ["uav-1", "uav-2"],
                    "paddingM": 20.0,
                    "smoothing": _smoothing(),
                },
            ),
        ]
        config = _config(cameras)
        self.assertEqual(
            [camera.rig_kind for camera in config.cameras],
            list(CameraRigKind),
        )

    def test_unknown_fields_fail_closed(self) -> None:
        camera = _camera(
            "follow",
            1,
            {
                "kind": "follow_entity",
                "targetEntityId": "uav-1",
                "eyeOffsetFluM": {"x": -30.0, "y": 0.0, "z": 8.0},
                "targetOffsetFluM": {"x": 0.0, "y": 0.0, "z": 0.0},
                "smoothing": _smoothing(),
            },
        )
        _config([camera])
        camera["unknown"] = True
        with self.assertRaisesRegex(ValueError, "unknown"):
            _config([camera])


class OperatorProductTests(unittest.TestCase):
    def test_new_viewer_starts_at_latest_keyframe_and_then_advances(self) -> None:
        product = OperatorCameraProduct.__new__(OperatorCameraProduct)
        product._condition = threading.Condition()
        product._closed = False
        product._failure = None
        product._frames = deque(maxlen=256)
        product._sequence = 0
        product._camera_ids = ("follow",)
        product._health = OperatorProductHealth(maximum_frame_age_ms=1_000)
        product._health.activate()
        keyframe = NativeH264AccessUnit(b"key", (7, 8, 5))
        delta = NativeH264AccessUnit(b"delta", (1,))
        product._on_access_unit(keyframe)
        product._on_access_unit(delta)

        first = product.wait_for_frame("follow", 0, 0.01)
        assert first is not None
        self.assertEqual(first.sequence, 1)
        second = product.wait_for_frame("follow", first.sequence, 0.01)
        assert second is not None
        self.assertEqual(second.sequence, 2)

    def test_aov_arguments_have_one_tiled_rtsp_nvenc_product(self) -> None:
        cameras = [
            _camera(
                camera_id,
                slot,
                {
                    "kind": "follow_entity",
                    "targetEntityId": "uav-1",
                    "eyeOffsetFluM": {"x": -30.0, "y": 0.0, "z": 8.0},
                    "targetOffsetFluM": {"x": 0.0, "y": 0.0, "z": 0.0},
                    "smoothing": _smoothing(),
                },
            )
            for slot, camera_id in enumerate(("follow", "chase"))
        ]
        config = _config(cameras)
        arguments = operator_aov_arguments(config)
        self.assertEqual(len(arguments), 5)
        self.assertTrue(any(OPERATOR_ATLAS_NAME in item for item in arguments))
        self.assertTrue(any("signalPort=8561" in item for item in arguments))
        self.assertTrue(any("streamPort=8560" in item for item in arguments))
        self.assertEqual(OPERATOR_ATLAS_PRODUCT_ID, "camera-atlas")
        regions, width, height = operator_atlas_layout(config)
        self.assertEqual((width, height), (2560, 720))
        self.assertEqual(
            [(region.camera_id, region.x_px, region.y_px) for region in regions],
            [("follow", 0, 0), ("chase", 1280, 0)],
        )

    def test_visibility_survives_metadata_only_frame_observation(self) -> None:
        health = OperatorProductHealth(maximum_frame_age_ms=1_000)
        health.activate()
        health.observe_frame(visible=False, monotonic_seconds=1.0)
        health.observe_frame(monotonic_seconds=1.1)
        snapshot = health.snapshot(content_ready=True, monotonic_seconds=1.2)
        self.assertEqual(snapshot["lifecycle"], "failed")
        self.assertEqual(snapshot["visible"], False)
        self.assertEqual(snapshot["diagnostic"], "operator camera frame is uniform or blank")

    def test_source_to_render_latency_reports_a_bounded_nearest_rank_p95(self) -> None:
        health = OperatorProductHealth(maximum_frame_age_ms=1_000)
        health.activate()
        for sample in range(1, 101):
            health.observe_frame(
                monotonic_seconds=float(sample),
                source_to_render_microseconds=sample * 100,
            )

        snapshot = health.snapshot(content_ready=True, monotonic_seconds=100.01)

        self.assertEqual(snapshot["sourceToRenderSamples"], 100)
        self.assertEqual(snapshot["sourceToRenderP95Microseconds"], 9_500)

    def test_render_cycle_latency_does_not_fabricate_an_encoded_frame(self) -> None:
        health = OperatorProductHealth(maximum_frame_age_ms=1_000)
        health.activate()
        health.observe_frame(visible=True, monotonic_seconds=1.0)
        health.observe_source_to_render(12_500)

        snapshot = health.snapshot(content_ready=True, monotonic_seconds=1.1)

        self.assertEqual(snapshot["encodedFrames"], 1)
        self.assertEqual(snapshot["sourceToRenderSamples"], 1)
        self.assertEqual(snapshot["sourceToRenderP95Microseconds"], 12_500)

    def test_stale_and_inactive_products_are_typed(self) -> None:
        health = OperatorProductHealth(maximum_frame_age_ms=100)
        self.assertEqual(
            health.snapshot(content_ready=True, monotonic_seconds=0.0)["lifecycle"],
            "inactive",
        )
        health.activate()
        self.assertEqual(
            health.snapshot(content_ready=True, monotonic_seconds=0.0)["lifecycle"],
            "starting",
        )
        health.observe_frame(visible=True, monotonic_seconds=1.0)
        self.assertEqual(
            health.snapshot(content_ready=True, monotonic_seconds=1.05)["lifecycle"],
            "ready",
        )
        self.assertEqual(
            health.snapshot(content_ready=True, monotonic_seconds=1.2)["diagnostic"],
            "operator camera frame is stale",
        )

    def test_world_warmup_is_not_a_terminal_product_failure(self) -> None:
        health = OperatorProductHealth(maximum_frame_age_ms=100)
        health.activate()
        health.observe_frame(visible=False, monotonic_seconds=1.0)

        warming = health.snapshot(content_ready=False, monotonic_seconds=2.0)
        self.assertEqual(warming["lifecycle"], "starting")
        self.assertEqual(warming["diagnostic"], "streamed world is warming")

        failed = health.snapshot(content_ready=True, monotonic_seconds=2.0)
        self.assertEqual(failed["lifecycle"], "failed")
        self.assertEqual(
            failed["diagnostic"], "operator camera frame is uniform or blank"
        )

    def test_hydra_failure_remains_terminal_during_world_warmup(self) -> None:
        health = OperatorProductHealth(maximum_frame_age_ms=100)
        health.activate()
        health.fail("RTX product unavailable")
        snapshot = health.snapshot(content_ready=False, monotonic_seconds=1.0)
        self.assertEqual(snapshot["lifecycle"], "failed")
        self.assertEqual(snapshot["diagnostic"], "RTX product unavailable")


if __name__ == "__main__":
    unittest.main()
