from __future__ import annotations

import json
import socket
import threading
import unittest

from veoveo_uav_sim.operator_camera import CameraRigKind
from veoveo_uav_sim.operator_camera_config import OperatorLiveViewRuntimeConfig
from veoveo_uav_sim.operator_health import OperatorProductHealth
from veoveo_uav_sim.operator_products import (
    OperatorRenderProduct,
    livestream_aov_arguments,
    livestream_aov_product_arguments,
    native_signaling_is_listening,
    operator_product_name,
    operator_stream_product_id,
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
        "streamPolicy": "on_demand",
    }


def _config(cameras: list[dict[str, object]]) -> OperatorLiveViewRuntimeConfig:
    return OperatorLiveViewRuntimeConfig.from_json(
        json.dumps(cameras),
        viewer_slot_count=2,
        activation_timeout_seconds=10.0,
        signaling_port_base=49100,
        media_port_base=47998,
        public_media_ip="203.0.113.8",
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
    def test_native_signaling_readiness_requires_a_listening_socket(self) -> None:
        listener = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
        listener.listen(1)
        try:
            self.assertTrue(native_signaling_is_listening(port))
        finally:
            listener.close()
        self.assertFalse(native_signaling_is_listening(port))

    def test_product_activation_waits_for_frame_and_signaling(self) -> None:
        product = OperatorRenderProduct.__new__(OperatorRenderProduct)
        product.product_id = "product-slot-0"
        product._activation = threading.Event()
        product._lock = threading.Lock()
        product._active = True
        product._live_view_id = "view-1"
        product._failure = None
        product._activation_frame_ready = False
        product._signaling_ready = False

        with self.assertRaisesRegex(TimeoutError, "native signaling"):
            product.wait_until_ready("view-1", 0.0)

        product._activation_frame_ready = True
        product._signaling_ready = True
        product._activation.set()
        product.wait_until_ready("view-1", 0.0)

    def test_one_aov_product_has_one_exact_gpu_stream(self) -> None:
        arguments = livestream_aov_product_arguments(
            "uav_viewer_slot_0",
            signaling_port=49100,
            media_port=47998,
            public_media_ip="127.0.0.1",
            target_fps=24,
        )
        self.assertEqual(len(arguments), 7)
        self.assertTrue(
            all("uav_viewer_slot_0.LdrColor" in item for item in arguments)
        )
        self.assertTrue(any("signalPort=49100" in item for item in arguments))
        self.assertTrue(any("streamPort=47998" in item for item in arguments))

    def test_aov_arguments_have_one_locked_port_pair_per_product(self) -> None:
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
        arguments = livestream_aov_arguments(_config(cameras))
        self.assertEqual(len(arguments), 14)
        self.assertTrue(any("uav_viewer_slot_0" in item for item in arguments))
        self.assertTrue(any("signalPort=49100" in item for item in arguments))
        self.assertTrue(any("signalPort=49101" in item for item in arguments))
        self.assertTrue(any("streamPort=47998" in item for item in arguments))
        self.assertTrue(any("streamPort=47999" in item for item in arguments))
        self.assertEqual(operator_product_name(1), "uav_viewer_slot_1")
        self.assertEqual(operator_stream_product_id(1), "product-slot-1")

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
