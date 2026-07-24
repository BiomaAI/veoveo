from __future__ import annotations

import asyncio
import os
import socket
import unittest
from datetime import datetime
from unittest.mock import patch

import numpy as np
from aiohttp import ClientSession, WSMsgType, web

from veoveo_uav_sim.camera_quality import (
    measure_camera_frame,
    normalize_rgb_frame,
    should_record_camera_frame,
)
from veoveo_uav_sim.config import LiveStreamConfig, RuntimeConfig
from veoveo_uav_sim.contracts import ContractError, parse_command, parse_operation
from veoveo_uav_sim.geo import enu_to_geodetic, horizontal_distance_m
from veoveo_uav_sim.live_stream import (
    LiveStreamLeaseManager,
    LiveStreamSignalingProxy,
    _authorization_token,
)
from veoveo_uav_sim.screenshot import ScreenshotGate
from veoveo_uav_sim.state import RuntimeState
from veoveo_uav_sim.world_config import (
    GeoreferenceOrigin,
    WorldConfiguration,
    WorldConfigurationError,
    WorldConfigurationSlot,
)


VALID_ENVIRONMENT = {
    "CESIUM_ION_ACCESS_TOKEN": "test-token",
    "UAV_SIM_CESIUM_ION_ASSET_ID": "2275207",
    "UAV_SIM_RECORDING_KEY": "019f7122-3d89-7d21-8312-8940d1e0f510",
    "UAV_SIM_SESSION_ID": "uav-showcase",
    "UAV_SIM_TILE_CACHE_POLICY": "persistent",
    "UAV_SIM_WORLD_SOURCE": "google_photorealistic_3d_tiles",
}

WORLD = WorldConfiguration(
    revision_uri="frames://world/uav-showcase-new-york/revision/revision-1",
    spec_sha256="1" * 64,
    simulation_frame_uri=(
        "frames://world/uav-showcase-new-york/revision/revision-1/"
        "frame/isaac-world"
    ),
    georeference_origin=GeoreferenceOrigin(
        latitude_degrees=40.758,
        longitude_degrees=-73.9855,
        ellipsoid_height_m=-17.0,
    ),
)


class RuntimeConfigTests(unittest.TestCase):
    def test_google_tiles_are_mandatory_and_exact(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            config = RuntimeConfig.from_environment()
        self.assertEqual(config.cesium_ion_asset_id, 2_275_207)
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
        self.assertEqual(config.rendering_hz, config.follow_camera.fps)

    def test_follow_camera_is_the_gpu_live_view(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            config = RuntimeConfig.from_environment()
            state = RuntimeState(config, WORLD).snapshot()
        self.assertEqual(
            (config.follow_camera.width, config.follow_camera.height),
            (1280, 720),
        )
        self.assertEqual(state["live_stream"]["source"], "follow_camera")
        self.assertEqual(state["live_stream"]["hardware_encoder"], "nvidia_nvenc")
        self.assertEqual(state["live_stream"]["codec"], "h264")

    def test_live_stream_fails_closed_on_invalid_gpu_stream_configuration(self) -> None:
        for override, message in (
            ({"UAV_SIM_LIVE_STREAM_PUBLIC_IP": ""}, "PUBLIC_IP"),
            ({"UAV_SIM_FOLLOW_CAMERA_FPS": "30"}, "must match"),
        ):
            with self.subTest(override=override):
                with patch.dict(
                    os.environ,
                    {**VALID_ENVIRONMENT, **override},
                    clear=True,
                ):
                    with self.assertRaisesRegex(ValueError, message):
                        RuntimeConfig.from_environment()

    def test_nadir_camera_is_the_only_canonical_stream(self) -> None:
        with patch.dict(os.environ, VALID_ENVIRONMENT, clear=True):
            state = RuntimeState(RuntimeConfig.from_environment(), WORLD).snapshot()
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
            "UAV_SIM_SCREENSHOT_MINIMUM_RELATIVE_ALTITUDE_M": "295",
            "UAV_SIM_SCREENSHOT_SETTLE_RENDERED_FRAMES": "45",
            "UAV_SIM_FOLLOW_CAMERA_WIDTH": "1920",
            "UAV_SIM_FOLLOW_CAMERA_HEIGHT": "1080",
            "UAV_SIM_FOLLOW_CAMERA_EYE_OFFSET_X_M": "-6",
        }
        with patch.dict(os.environ, environment, clear=True):
            config = RuntimeConfig.from_environment()
            screenshot = config.screenshot
        self.assertIsNotNone(screenshot)
        assert screenshot is not None
        self.assertEqual(screenshot.output_path.as_posix(), "/tmp/isaac-uav.png")
        self.assertEqual(screenshot.minimum_relative_altitude_m, 295.0)
        self.assertEqual(screenshot.settle_rendered_frames, 45)
        self.assertEqual(
            (config.follow_camera.width, config.follow_camera.height),
            (1920, 1080),
        )
        self.assertEqual(
            config.follow_camera.eye_offset_xyz_m,
            (-6.0, -2.2, 1.2),
        )

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
                    "session_id": "uav-showcase",
                    "vehicle_id": "uav-1",
                    "legacy_vehicle": "one",
                }
            )

    def test_missions_require_the_expected_world_revision(self) -> None:
        mission = parse_operation(
            {
                "operation": "execute_mission",
                "input": {
                    "session_id": "uav-showcase",
                    "mission_id": "mission-1",
                    "expected_world_revision_uri": WORLD.revision_uri,
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
        self.assertEqual(
            mission.expected_world_revision_uri,
            WORLD.revision_uri,
        )

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


class WorldConfigurationTests(unittest.TestCase):
    def test_world_binding_is_strict_and_typed(self) -> None:
        world = WorldConfiguration.from_request(
            {"session_id": "uav-showcase", "world": WORLD.as_dict()},
            "uav-showcase",
        )
        self.assertEqual(world, WORLD)

    def test_world_binding_rejects_a_frame_from_another_revision(self) -> None:
        payload = WORLD.as_dict()
        payload["simulation_frame_uri"] = (
            "frames://world/other/revision/revision-2/frame/isaac-world"
        )
        with self.assertRaisesRegex(
            WorldConfigurationError, "frame in revision_uri"
        ):
            WorldConfiguration.from_request(
                {"session_id": "uav-showcase", "world": payload},
                "uav-showcase",
            )

    def test_world_slot_is_idempotent_and_immutable(self) -> None:
        slot = WorldConfigurationSlot()
        self.assertEqual(slot.configure(WORLD), WORLD)
        self.assertEqual(slot.configure(WORLD), WORLD)
        other = WorldConfiguration(
            revision_uri=(
                "frames://world/uav-showcase-new-york/revision/revision-2"
            ),
            spec_sha256="2" * 64,
            simulation_frame_uri=(
                "frames://world/uav-showcase-new-york/revision/revision-2/"
                "frame/isaac-world"
            ),
            georeference_origin=WORLD.georeference_origin,
        )
        with self.assertRaisesRegex(WorldConfigurationError, "different world"):
            slot.configure(other)


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


class LiveStreamLeaseTests(unittest.TestCase):
    def test_bearer_protocol_accepts_the_nvidia_client_shape(self) -> None:
        self.assertEqual(
            _authorization_token(
                [
                    "x-nv-sessionid.stream-1",
                    "authorization.bearer.secret-token",
                ]
            ),
            "secret-token",
        )
        self.assertIsNone(_authorization_token(["x-nv-sessionid.stream-1"]))
        self.assertIsNone(
            _authorization_token(["Authorization.Bearer.secret-token"])
        )

    def test_one_short_lived_stream_lease_is_enforced(self) -> None:
        changes: list[tuple[str, int]] = []
        manager = LiveStreamLeaseManager(
            300,
            lambda lifecycle, viewers: changes.append((lifecycle, viewers)),
        )
        opened = manager.open("stream-1")
        self.assertEqual(opened["stream_id"], "stream-1")
        self.assertGreater(
            datetime.fromisoformat(opened["expires_at"].replace("Z", "+00:00")),
            datetime.now().astimezone(),
        )
        with self.assertRaisesRegex(RuntimeError, "already leased"):
            manager.open("stream-2")
        sign_in = manager.authorize(opened["access_token"])
        handoff = manager.authorize(opened["access_token"])
        self.assertEqual(sign_in.stream_id, "stream-1")
        self.assertEqual(handoff.stream_id, "stream-1")
        self.assertNotEqual(sign_in.connection_id, handoff.connection_id)
        self.assertEqual(manager.public_state(), ("live", 1))
        manager.disconnect(sign_in)
        self.assertEqual(manager.public_state(), ("live", 1))
        manager.disconnect(handoff)
        self.assertEqual(manager.public_state(), ("ready", 0))
        renewed = manager.renew("stream-1")
        self.assertEqual(renewed["access_token"], opened["access_token"])
        self.assertGreater(renewed["expires_at"], opened["expires_at"])
        manager.close("stream-1")
        self.assertFalse(manager.active("stream-1"))
        self.assertEqual(changes[-1], ("ready", 0))


class LiveStreamSignalingTests(unittest.IsolatedAsyncioTestCase):
    async def test_authenticated_proxy_bridges_only_the_leased_viewer(self) -> None:
        signal_port = _free_port()
        proxy_port = _free_port()
        upstream_requests: list[str] = []

        async def echo(request: web.Request) -> web.WebSocketResponse:
            upstream_requests.append(str(request.rel_url))
            websocket = web.WebSocketResponse(
                protocols=["x-nv-sessionid.stream-1"]
            )
            await websocket.prepare(request)
            async for message in websocket:
                if message.type == WSMsgType.TEXT:
                    await websocket.send_str(message.data)
            return websocket

        application = web.Application()
        application.add_routes([web.get("/sign_in", echo)])
        upstream = web.AppRunner(application)
        await upstream.setup()
        await web.TCPSite(upstream, "127.0.0.1", signal_port).start()

        leases = LiveStreamLeaseManager(300, lambda _state, _viewers: None)
        opened = leases.open("stream-1")
        proxy = LiveStreamSignalingProxy(
            LiveStreamConfig(
                signal_port=signal_port,
                media_port=47998,
                public_ip="127.0.0.1",
                proxy_host="127.0.0.1",
                proxy_port=proxy_port,
                signaling_path="/webrtc",
                lease_ttl_seconds=300,
            ),
            leases,
        )
        proxy.start()
        try:
            async with ClientSession() as client:
                with self.assertRaisesRegex(Exception, "401"):
                    await client.ws_connect(
                        (
                            f"ws://127.0.0.1:{proxy_port}/webrtc/sign_in"
                            "?pairing_id=stream-1"
                        ),
                        protocols=["x-nv-sessionid.stream-1"],
                    )
                async with client.ws_connect(
                    (
                        f"ws://127.0.0.1:{proxy_port}/webrtc/sign_in"
                        "?pairing_id=stream-1"
                    ),
                    protocols=[
                        "x-nv-sessionid.stream-1",
                        f"authorization.bearer.{opened['access_token']}",
                    ],
                ) as websocket:
                    await websocket.send_str("offer")
                    message = await websocket.receive(timeout=5)
                    self.assertEqual(message.type, WSMsgType.TEXT)
                    self.assertEqual(message.data, "offer")
                    self.assertEqual(leases.public_state(), ("live", 1))
                    async with client.ws_connect(
                        (
                            f"ws://127.0.0.1:{proxy_port}/webrtc/sign_in"
                            "?pairing_id=stream-1"
                        ),
                        protocols=[
                            "x-nv-sessionid.stream-1",
                            f"authorization.bearer.{opened['access_token']}",
                        ],
                    ) as handoff:
                        await handoff.send_str("handoff")
                        handoff_message = await handoff.receive(timeout=5)
                        self.assertEqual(handoff_message.type, WSMsgType.TEXT)
                        self.assertEqual(handoff_message.data, "handoff")
                        self.assertEqual(leases.public_state(), ("live", 1))
                    self.assertEqual(
                        upstream_requests,
                        [
                            "/sign_in?pairing_id=stream-1",
                            "/sign_in?pairing_id=stream-1",
                        ],
                    )
                self.assertEqual(leases.public_state(), ("ready", 0))
                with self.assertRaisesRegex(Exception, "403"):
                    await client.ws_connect(
                        (
                            f"ws://127.0.0.1:{proxy_port}/webrtc/sign_in"
                            "?pairing_id=another-stream"
                        ),
                        protocols=[
                            "x-nv-sessionid.another-stream",
                            f"authorization.bearer.{opened['access_token']}",
                        ],
                    )
        finally:
            proxy.close()
            await upstream.cleanup()


def _free_port() -> int:
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


if __name__ == "__main__":
    unittest.main()
