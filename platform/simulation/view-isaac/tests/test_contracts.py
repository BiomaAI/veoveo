from __future__ import annotations

import hashlib
import os
import struct
import tempfile
import threading
import unittest
from io import BytesIO
from pathlib import Path
from types import ModuleType
from unittest.mock import Mock, patch

from veoveo_simulation_view.camera import CameraPool, HydraRenderProductProbe
from veoveo_simulation_view.config import RendererConfig
from veoveo_simulation_view.contracts import (
    CameraBinding,
    ContractError,
    PoseSourceBinding,
    SessionBinding,
)
from veoveo_simulation_view.pose import decode_snapshot
from veoveo_simulation_view.scene import ArtifactMaterializer, ArtifactStore


class RendererContractsTest(unittest.TestCase):
    def test_render_product_is_reconfigured_without_recreation(self) -> None:
        class FakeHydraTexture:
            def __init__(self) -> None:
                self.camera_path = "/old-camera"
                self.width = 320
                self.height = 180
                self.update_history: list[bool] = []

            @property
            def updates_enabled(self) -> bool:
                return self.update_history[-1]

            @updates_enabled.setter
            def updates_enabled(self, value: bool) -> None:
                self.update_history.append(value)

            def get_settings_path(self) -> str:
                return "/hydra/slot/"

        texture = FakeHydraTexture()
        probe = HydraRenderProductProbe.__new__(HydraRenderProductProbe)
        probe._width = 320
        probe._height = 180
        probe._lock = threading.Lock()
        probe._capture_pending = True
        probe._last_capture_requested = 4.0
        probe._closed = False
        probe._generation = 3
        probe._health = None
        probe._failure = RuntimeError("stale")
        probe._subscription = object()
        probe._hydra_texture = texture

        settings = Mock()
        carb = ModuleType("carb")
        carb.__path__ = []
        carb_settings = ModuleType("carb.settings")
        carb_settings.get_settings = lambda: settings
        carb.settings = carb_settings
        with patch.dict(
            "sys.modules",
            {"carb": carb, "carb.settings": carb_settings},
        ):
            probe.reconfigure(
                camera_path="/World/SimulationView/Cameras/slot_2",
                width=640,
                height=360,
                fps=30,
            )

        self.assertIs(probe._hydra_texture, texture)
        self.assertEqual(texture.camera_path, "/World/SimulationView/Cameras/slot_2")
        self.assertEqual((texture.width, texture.height), (640, 360))
        self.assertEqual(texture.update_history, [False, True])
        self.assertEqual(probe._generation, 4)
        self.assertFalse(probe._closed)
        self.assertIsNone(probe._failure)
        settings.set.assert_called_once_with("/hydra/slot/hydraTickRate", 30)

    def test_idle_physical_slot_is_reused_by_the_next_logical_camera(
        self,
    ) -> None:
        class FakeRuntime:
            def __init__(self, binding: CameraBinding) -> None:
                self.binding = binding
                self.probe = Mock()
                self.smoothed_eye = (1.0, 2.0, 3.0)
                self.last_update = 4.0
                self.last_pose_sequence = 5
                self.pose_stale = True

            def status(self) -> dict[str, object]:
                return {"cameraId": self.binding.camera_id}

        first = CameraBinding(
            session_id="session-1",
            camera_id="camera-1",
            revision=1,
            render_slot=2,
            definition={},
        )
        second = CameraBinding(
            session_id="session-2",
            camera_id="camera-2",
            revision=1,
            render_slot=2,
            definition={},
        )
        runtime = FakeRuntime(first)
        pool = CameraPool.__new__(CameraPool)
        pool._cameras = {first.camera_id: runtime}
        pool._slots = {first.render_slot: first.camera_id}
        pool._idle = {}
        pool._probe = None

        pool.close(first.camera_id)
        runtime.probe.pause.assert_called_once_with()
        self.assertIs(pool._idle[first.render_slot], runtime)

        def configure(
            reused: FakeRuntime, binding: CameraBinding
        ) -> None:
            reused.binding = binding

        with (
            patch.object(
                pool,
                "_configure_camera",
                side_effect=configure,
            ) as configure_camera,
            patch.object(pool, "_create_camera") as create_camera,
        ):
            status = pool.upsert(second)

        configure_camera.assert_called_once_with(runtime, second)
        create_camera.assert_not_called()
        self.assertEqual(status, {"cameraId": "camera-2"})
        self.assertIs(pool._cameras[second.camera_id], runtime)

    def test_config_requires_disjoint_bounded_port_ranges(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            values = {
                "SIMULATION_VIEW_RENDERER_CONTROL_TOKEN": "a" * 32,
                "SIMULATION_VIEW_PUBLIC_MEDIA_IP": "192.0.2.42",
                "SIMULATION_VIEW_ARTIFACT_DIRECTORY": f"{directory}/artifacts",
                "SIMULATION_VIEW_POSE_DIRECTORY": f"{directory}/pose",
                "SIMULATION_VIEW_RENDERER_CACHE_DIRECTORY": f"{directory}/cache",
                "SIMULATION_VIEW_MAXIMUM_RENDER_SLOTS": "4",
                "SIMULATION_VIEW_SIGNALING_PORT_BASE": "49100",
                "SIMULATION_VIEW_MEDIA_PORT_BASE": "47998",
            }
            with patch.dict(os.environ, values, clear=True):
                config = RendererConfig.from_environment()
            self.assertEqual(config.signaling_port_base + 3, 49103)
            self.assertEqual(config.media_port_base + 3, 48001)
            self.assertEqual(
                config.maximum_artifact_bytes, 4 * 1024 * 1024 * 1024
            )

    def test_private_bindings_are_exact_and_typed(self) -> None:
        session = SessionBinding.parse(
            {"sessionId": "session-1", "epochId": "epoch-1"}
        )
        self.assertEqual(session.session_id, "session-1")
        with self.assertRaises(ContractError):
            SessionBinding.parse(
                {
                    "sessionId": "session-1",
                    "epochId": "epoch-1",
                    "owner": "must-not-cross-runtime-boundary",
                }
            )

        camera = CameraBinding.parse(
            {
                "sessionId": "session-1",
                "cameraId": "camera-1",
                "revision": 1,
                "renderSlot": 2,
                "definition": {
                    "rig": {
                        "kind": "look_at",
                        "eyeM": {"x": 4.0, "y": -4.0, "z": 3.0},
                        "targetM": {"x": 0.0, "y": 0.0, "z": 0.0},
                    },
                    "widthPx": 1280,
                    "heightPx": 720,
                    "frameRateMillihertz": 20_000,
                    "verticalFovDegrees": 60.0,
                    "nearClipM": 0.1,
                    "farClipM": 10_000.0,
                    "streamPolicy": "on_demand",
                    "recordingPolicy": "disabled",
                },
            },
            4,
        )
        self.assertEqual(camera.render_slot, 2)

    def test_artifacts_are_content_addressed_and_self_contained(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifacts = root / "artifacts"
            cache = root / "cache"
            (artifacts / "sha256").mkdir(parents=True)
            payload = b'#usda 1.0\n\ndef Xform "Root" {}\n'
            digest = hashlib.sha256(payload).hexdigest()
            path = artifacts / "sha256" / f"{digest}.usd"
            path.write_bytes(payload)
            resolved = ArtifactStore(artifacts, cache).resolve(
                {
                    "artifactUri": "artifact://fixture/world",
                    "digest": f"sha256:{digest}",
                    "format": "usd",
                    "byteLength": len(payload),
                }
            )
            self.assertEqual(resolved.path, path)
            path.write_bytes(payload + b" ")
            with self.assertRaises(ContractError):
                ArtifactStore(artifacts, cache).resolve(
                    {
                        "artifactUri": "artifact://fixture/world",
                        "digest": f"sha256:{digest}",
                        "format": "usd",
                        "byteLength": len(payload),
                    }
                )

    def test_artifact_ingest_hashes_and_materializes_atomically(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifacts = Path(directory) / "artifacts"
            (artifacts / "sha256").mkdir(parents=True)
            materializer = ArtifactMaterializer(artifacts, 1024)
            payload = b'#usda 1.0\n\ndef Xform "Root" {}\n'
            digest = hashlib.sha256(payload).hexdigest()
            path = materializer.materialize(
                digest, "usd", len(payload), BytesIO(payload)
            )
            self.assertEqual(path.read_bytes(), payload)
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)

            repeated = materializer.materialize(
                digest, "usd", len(payload), BytesIO(payload)
            )
            self.assertEqual(repeated, path)
            self.assertFalse(
                any(
                    candidate.name.endswith(".next")
                    for candidate in (artifacts / "sha256").iterdir()
                )
            )

            wrong_digest = "1" * 64
            with self.assertRaises(ContractError):
                materializer.materialize(
                    wrong_digest, "usd", len(payload), BytesIO(payload)
                )
            self.assertFalse(
                (artifacts / "sha256" / f"{wrong_digest}.usd").exists()
            )

    def test_pose_decoder_rejects_binding_mismatch(self) -> None:
        entity_id = b"entity-1"
        table_hasher = hashlib.sha256()
        table_hasher.update(struct.pack(">QH", 1, len(entity_id)))
        table_hasher.update(entity_id)
        table_digest = table_hasher.digest()
        frame_digest = bytes.fromhex("11" * 32)
        session = b"session-1"
        epoch = b"epoch-1"
        frame = b"frames://world/synthetic/revision/r1"
        entity = (
            struct.pack(">HBB", len(entity_id), 0x03, 0)
            + struct.pack(">7d", 1.0, 2.0, 3.0, 0.0, 0.0, 0.0, 1.0)
            + entity_id
        )
        header = (
            b"VVPOSE01"
            + struct.pack(">HHI", 1, 0, 0)
            + struct.pack(
                ">QqQIHHHH",
                1,
                10_000_000,
                1,
                1,
                len(session),
                len(epoch),
                len(frame),
                1,
            )
            + frame_digest
            + table_digest
            + session
            + epoch
            + frame
        )
        encoded = bytearray(header + entity)
        encoded[12:16] = struct.pack(">I", len(encoded))
        binding = PoseSourceBinding.parse(
            {
                "schemaVersion": (
                    "veoveo.io/simulation-view-pose-ingress-control/v1"
                ),
                "sessionId": "session-1",
                "epochId": "epoch-1",
                "frameRevision": {
                    "uri": frame.decode(),
                    "digest": f"sha256:{frame_digest.hex()}",
                },
                "entityTableRevision": 1,
                "entityTableDigest": f"sha256:{table_digest.hex()}",
                "limits": {
                    "maximumEntities": 8,
                    "maximumMessageBytes": 65536,
                    "maximumCadenceHz": 120,
                    "staleAfterMs": 500,
                },
                "producer": {
                    "producerId": "fixture",
                    "spiffeId": "spiffe://example.test/fixture",
                    "expiresAt": "2026-07-26T12:00:00Z",
                },
            }
        )
        snapshot = decode_snapshot(bytes(encoded), binding)
        self.assertEqual(snapshot.entities[0].position_enu_m, (1.0, 2.0, 3.0))
        wrong = PoseSourceBinding(
            session_id="session-2",
            epoch_id=binding.epoch_id,
            frame_uri=binding.frame_uri,
            frame_digest=binding.frame_digest,
            entity_table_revision=binding.entity_table_revision,
            entity_table_digest=binding.entity_table_digest,
            maximum_entities=binding.maximum_entities,
            maximum_message_bytes=binding.maximum_message_bytes,
            stale_after_ms=binding.stale_after_ms,
        )
        with self.assertRaises(ContractError):
            decode_snapshot(bytes(encoded), wrong)


if __name__ == "__main__":
    unittest.main()
